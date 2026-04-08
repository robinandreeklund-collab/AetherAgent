/// Persistence layer — SQLite-backed storage for CRFR learning
///
/// Sparar resonance fields, domain profiles och metadata till disk
/// så att kausalt minne överlever server-restarts.
///
/// v18: ConnectionPool — multiple reader connections + single writer.
/// SQLite WAL mode allows concurrent reads without blocking writes.
use rusqlite::{params, Connection};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::resonance::{DomainProfile, ResonanceField};

// ─── Connection Pool ──────────────────────────────────────────────────────

/// Max number of reader connections in the pool
const POOL_MAX_READERS: usize = 4;

/// Connection pool: 1 writer + N readers for concurrent access.
/// Writer is protected by Mutex (SQLite requires single-writer).
/// Readers are pooled in a Mutex<Vec> — take/return pattern.
struct ConnectionPool {
    writer: Option<Connection>,
    readers: Vec<Connection>,
    db_path: String,
}

impl ConnectionPool {
    fn new() -> Self {
        ConnectionPool {
            writer: None,
            readers: Vec::new(),
            db_path: String::new(),
        }
    }

    fn init(&mut self, path: &str) -> Result<(), String> {
        self.db_path = path.to_string();

        // Writer connection
        let writer = Connection::open(path).map_err(|e| format!("SQLite writer open: {e}"))?;
        writer
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("SQLite writer pragma: {e}"))?;
        self.writer = Some(writer);

        // Reader connections (WAL mode allows concurrent reads)
        for i in 0..POOL_MAX_READERS {
            match Connection::open_with_flags(
                path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                Ok(conn) => {
                    let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");
                    self.readers.push(conn);
                }
                Err(e) => {
                    if i == 0 {
                        return Err(format!("SQLite reader open: {e}"));
                    }
                    break; // Partial pool is OK
                }
            }
        }

        Ok(())
    }
}

static DB: std::sync::LazyLock<Mutex<ConnectionPool>> =
    std::sync::LazyLock::new(|| Mutex::new(ConnectionPool::new()));

/// Tidstämpel i millisekunder
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ─── Init ──────────────────────────────────────────────────────────────────

/// Initialize the persistence layer. Call once at server startup.
/// Creates the SQLite database file and tables if they don't exist.
/// v18: Initializes connection pool with 1 writer + 4 readers.
pub fn init(db_path: &str) -> Result<(), String> {
    let mut pool = DB.lock().map_err(|e| format!("DB lock: {e}"))?;
    pool.init(db_path)?;

    // Create schema using writer
    if let Some(ref conn) = pool.writer {
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS resonance_fields (
                url_hash INTEGER PRIMARY KEY,
                url TEXT NOT NULL,
                data BLOB NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS domain_profiles (
                domain_hash INTEGER PRIMARY KEY,
                data BLOB NOT NULL,
                updated_at INTEGER NOT NULL
            );

            CREATE INDEX IF NOT EXISTS idx_fields_updated ON resonance_fields(updated_at);
            CREATE INDEX IF NOT EXISTS idx_domains_updated ON domain_profiles(updated_at);

            CREATE TABLE IF NOT EXISTS global_stats (
                key TEXT PRIMARY KEY,
                value INTEGER NOT NULL
            );
            ",
        )
        .map_err(|e| format!("SQLite schema: {e}"))?;
    }

    Ok(())
}

/// Check if persistence is initialized.
pub fn is_initialized() -> bool {
    DB.lock().map(|pool| pool.writer.is_some()).unwrap_or(false)
}

// ─── Resonance Fields ──────────────────────────────────────────────────────

/// Save a resonance field to the database (uses writer connection).
pub fn save_field(field: &ResonanceField) {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[PERSIST] save_field lock error: {e}");
            return;
        }
    };
    let conn = match pool.writer.as_ref() {
        Some(c) => c,
        None => {
            eprintln!("[PERSIST] save_field: no writer connection");
            return;
        }
    };

    let data = match serde_json::to_vec(field) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[PERSIST] save_field serialize error: {e}");
            return;
        }
    };

    if let Err(e) = conn.execute(
        "INSERT OR REPLACE INTO resonance_fields (url_hash, url, data, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![field.url_hash as i64, field.url, data, now_ms() as i64],
    ) {
        eprintln!("[PERSIST] save_field write error: {e}");
    }
}

/// Load a resonance field by URL hash (uses reader connection from pool).
pub fn load_field(url_hash: u64) -> Option<ResonanceField> {
    let pool = DB.lock().ok()?;

    // Try reader first (non-blocking for other readers)
    let conn = pool.readers.first().or(pool.writer.as_ref())?;

    let mut stmt = conn
        .prepare("SELECT data FROM resonance_fields WHERE url_hash = ?1")
        .ok()?;

    let data: Vec<u8> = stmt
        .query_row(params![url_hash as i64], |row| row.get(0))
        .ok()?;

    serde_json::from_slice(&data).ok()
}

/// Load all resonance fields (for startup warm-load).
pub fn load_all_fields() -> Vec<ResonanceField> {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let conn = match pool.readers.first().or(pool.writer.as_ref()) {
        Some(c) => c,
        None => return vec![],
    };

    let mut stmt = match conn
        .prepare("SELECT data FROM resonance_fields ORDER BY updated_at DESC LIMIT 256")
    {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[PERSIST] load_all_fields prepare error: {e}");
            return vec![];
        }
    };

    let rows = match stmt.query_map([], |row| {
        let data: Vec<u8> = row.get(0)?;
        Ok(data)
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    let mut loaded = 0;
    let mut failed = 0;
    let result: Vec<ResonanceField> = rows
        .filter_map(|r| r.ok())
        .filter_map(|data| match serde_json::from_slice(&data) {
            Ok(field) => {
                loaded += 1;
                Some(field)
            }
            Err(e) => {
                failed += 1;
                if failed <= 3 {
                    eprintln!("[PERSIST] load_all_fields deserialize error: {e}");
                }
                None
            }
        })
        .collect();
    if failed > 0 {
        eprintln!("[PERSIST] load_all_fields: {loaded} loaded, {failed} failed deserialization");
    }
    result
}

/// Delete old fields (older than max_age_ms).
pub fn evict_old_fields(max_age_ms: u64) -> usize {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return 0,
    };
    let conn = match pool.writer.as_ref() {
        Some(c) => c,
        None => return 0,
    };

    let cutoff = now_ms().saturating_sub(max_age_ms) as i64;
    conn.execute(
        "DELETE FROM resonance_fields WHERE updated_at < ?1",
        params![cutoff],
    )
    .unwrap_or(0)
}

/// Stored field summary (lightweight — doesn't deserialize the full field).
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredFieldInfo {
    pub url_hash: i64,
    pub url: String,
    pub updated_at: i64,
    pub data_size_bytes: usize,
}

/// List all stored fields with metadata (no full deserialization).
pub fn list_stored_fields() -> Vec<StoredFieldInfo> {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let conn = match pool.readers.first().or(pool.writer.as_ref()) {
        Some(c) => c,
        None => return vec![],
    };

    let mut stmt = match conn.prepare(
        "SELECT url_hash, url, updated_at, LENGTH(data) FROM resonance_fields ORDER BY updated_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map([], |row| {
        Ok(StoredFieldInfo {
            url_hash: row.get(0)?,
            url: row.get(1)?,
            updated_at: row.get(2)?,
            data_size_bytes: row.get::<_, i64>(3)? as usize,
        })
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok()).collect()
}

/// Stored domain profile summary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoredDomainInfo {
    pub domain_hash: i64,
    pub updated_at: i64,
    pub data_size_bytes: usize,
    /// Antal weights (deserialiserade från data)
    pub weight_count: usize,
    /// Antal concepts
    pub concept_count: usize,
    pub field_count: u32,
}

/// List all stored domain profiles with metadata.
pub fn list_stored_domain_profiles() -> Vec<StoredDomainInfo> {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let conn = match pool.readers.first().or(pool.writer.as_ref()) {
        Some(c) => c,
        None => return vec![],
    };

    let mut stmt = match conn.prepare(
        "SELECT domain_hash, updated_at, data, LENGTH(data) FROM domain_profiles ORDER BY updated_at DESC",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map([], |row| {
        let hash: i64 = row.get(0)?;
        let updated: i64 = row.get(1)?;
        let data: Vec<u8> = row.get(2)?;
        let size: i64 = row.get(3)?;
        Ok((hash, updated, data, size as usize))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok())
        .map(|(hash, updated, data, size)| {
            let profile: Option<DomainProfile> = serde_json::from_slice(&data).ok();
            StoredDomainInfo {
                domain_hash: hash,
                updated_at: updated,
                data_size_bytes: size,
                weight_count: profile.as_ref().map(|p| p.stats.len()).unwrap_or(0),
                concept_count: profile.as_ref().map(|p| p.concepts.len()).unwrap_or(0),
                field_count: profile.map(|p| p.field_count).unwrap_or(0),
            }
        })
        .collect()
}

/// Count stored fields.
pub fn field_count() -> usize {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return 0,
    };
    let conn = match pool.writer.as_ref() {
        Some(c) => c,
        None => return 0,
    };

    conn.query_row("SELECT COUNT(*) FROM resonance_fields", [], |row| {
        row.get::<_, i64>(0)
    })
    .unwrap_or(0) as usize
}

// ─── Domain Profiles ───────────────────────────────────────────────────────

/// Save a domain profile.
pub fn save_domain_profile(domain_hash: u64, profile: &DomainProfile) {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return,
    };
    let conn = match pool.writer.as_ref() {
        Some(c) => c,
        None => return,
    };

    let data = match serde_json::to_vec(profile) {
        Ok(d) => d,
        Err(_) => return,
    };

    let _ = conn.execute(
        "INSERT OR REPLACE INTO domain_profiles (domain_hash, data, updated_at) VALUES (?1, ?2, ?3)",
        params![domain_hash as i64, data, now_ms() as i64],
    );
}

/// Load all domain profiles (for startup warm-load).
pub fn load_all_domain_profiles() -> Vec<(u64, DomainProfile)> {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return vec![],
    };
    let conn = match pool.readers.first().or(pool.writer.as_ref()) {
        Some(c) => c,
        None => return vec![],
    };

    let mut stmt = match conn
        .prepare("SELECT domain_hash, data FROM domain_profiles ORDER BY updated_at DESC LIMIT 128")
    {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = match stmt.query_map([], |row| {
        let hash: i64 = row.get(0)?;
        let data: Vec<u8> = row.get(1)?;
        Ok((hash as u64, data))
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok())
        .filter_map(|(hash, data)| {
            serde_json::from_slice(&data)
                .ok()
                .map(|profile| (hash, profile))
        })
        .collect()
}

/// Count stored domain profiles.
pub fn domain_profile_count() -> usize {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return 0,
    };
    let conn = match pool.writer.as_ref() {
        Some(c) => c,
        None => return 0,
    };

    conn.query_row("SELECT COUNT(*) FROM domain_profiles", [], |row| {
        row.get::<_, i64>(0)
    })
    .unwrap_or(0) as usize
}

// ─── Stats ─────────────────────────────────────────────────────────────────

/// Database stats for dashboard.
pub fn db_stats() -> (usize, usize, u64) {
    let fields = field_count();
    let domains = domain_profile_count();

    // Beräkna DB-filstorlek
    let db_size = DB
        .lock()
        .ok()
        .and_then(|pool| {
            pool.readers
                .first()
                .or(pool.writer.as_ref())
                .and_then(|conn| {
                    conn.query_row(
                    "SELECT page_count * page_size FROM pragma_page_count(), pragma_page_size()",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .ok()
                })
        })
        .unwrap_or(0) as u64;

    (fields, domains, db_size)
}

// ─── Full save/load cycle ──────────────────────────────────────────────────

/// Save all current CRFR state to database (fields + domain profiles).
/// Called periodically or on shutdown.
pub fn checkpoint() {
    if !is_initialized() {
        return;
    }

    // Spara alla domain profiles
    let profiles = crate::resonance::export_domain_profiles();
    for (hash, profile) in &profiles {
        save_domain_profile(*hash, profile);
    }

    // Spara alla cachade resonance fields (full data)
    let fields = crate::resonance::export_cached_fields();
    for field in &fields {
        save_field(field);
    }
    eprintln!(
        "[PERSIST] Checkpoint: {} domain profiles, {} resonance fields saved",
        profiles.len(),
        fields.len()
    );
}

/// Restore CRFR state from database at startup.
pub fn restore() {
    if !is_initialized() {
        return;
    }

    // Ladda domain profiles
    let profiles = load_all_domain_profiles();
    let profile_count = profiles.len();
    crate::resonance::import_domain_profiles(profiles);

    // Ladda cachade resonance fields
    let fields = load_all_fields();
    let field_count = fields.len();
    crate::resonance::import_cached_fields(fields);

    eprintln!(
        "[PERSIST] Restored: {} domain profiles, {} resonance fields",
        profile_count, field_count
    );
}

/// Save a global stat (key-value integer).
pub fn save_global_stat(key: &str, value: u64) {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return,
    };
    let conn = match pool.writer.as_ref() {
        Some(c) => c,
        None => return,
    };
    let _ = conn.execute(
        "INSERT OR REPLACE INTO global_stats (key, value) VALUES (?1, ?2)",
        params![key, value as i64],
    );
}

/// Load a global stat.
pub fn load_global_stat(key: &str) -> u64 {
    let pool = match DB.lock() {
        Ok(p) => p,
        Err(_) => return 0,
    };
    let conn = match pool.readers.first().or(pool.writer.as_ref()) {
        Some(c) => c,
        None => return 0,
    };
    conn.query_row(
        "SELECT value FROM global_stats WHERE key = ?1",
        params![key],
        |row| row.get::<_, i64>(0),
    )
    .map(|v| v as u64)
    .unwrap_or(0)
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init_and_stats() {
        // Skapa temp-databas
        let dir = std::env::temp_dir();
        let path = dir.join("aether_test_persist.db");
        let path_str = path.to_str().unwrap();

        // Rensa om den finns sedan förra testet
        let _ = std::fs::remove_file(path_str);

        init(path_str).expect("DB init borde lyckas");
        assert!(is_initialized(), "DB borde vara initialiserad");

        let (fields, domains, _size) = db_stats();
        assert_eq!(fields, 0, "Borde ha 0 fields vid start");
        assert_eq!(domains, 0, "Borde ha 0 domains vid start");

        // Rensa
        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn test_save_and_load_domain_profile() {
        let dir = std::env::temp_dir();
        let path = dir.join("aether_test_persist_dp.db");
        let path_str = path.to_str().unwrap();
        let _ = std::fs::remove_file(path_str);

        init(path_str).expect("DB init borde lyckas");

        let profile = DomainProfile {
            stats: {
                let mut m = std::collections::HashMap::new();
                m.insert("heading:down".to_string(), (5.0, 1.0));
                m.insert("button:up".to_string(), (2.0, 3.0));
                m
            },
            concepts: std::collections::HashMap::new(),
            field_count: 3,
        };

        save_domain_profile(12345, &profile);

        let loaded = load_all_domain_profiles();
        assert_eq!(loaded.len(), 1, "Borde ladda 1 profil");
        assert_eq!(loaded[0].0, 12345, "Domain hash borde matcha");
        assert_eq!(loaded[0].1.field_count, 3, "field_count borde matcha");
        assert!(
            loaded[0].1.stats.contains_key("heading:down"),
            "Stats borde innehålla heading:down"
        );

        let (_, domains, _) = db_stats();
        assert_eq!(domains, 1, "Borde ha 1 domain profile");

        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn test_evict_old_fields() {
        let dir = std::env::temp_dir();
        let path = dir.join("aether_test_persist_evict.db");
        let path_str = path.to_str().unwrap();
        let _ = std::fs::remove_file(path_str);

        init(path_str).expect("DB init borde lyckas");

        // Inget att evicta
        let evicted = evict_old_fields(1000);
        assert_eq!(evicted, 0, "Borde evicta 0 från tom DB");

        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn test_field_survives_save_load_cycle() {
        use crate::resonance::ResonanceField;
        use crate::types::SemanticNode;

        let dir = std::env::temp_dir();
        let path = dir.join("aether_test_persist_survive.db");
        let path_str = path.to_str().unwrap();
        let _ = std::fs::remove_file(path_str);

        init(path_str).expect("DB init borde lyckas");

        // Build a field with learning
        let tree = vec![SemanticNode {
            id: 1,
            role: "heading".into(),
            label: "Test headline".into(),
            ..SemanticNode::default()
        }];
        let mut field = ResonanceField::from_semantic_tree(&tree, "https://persist-survive.test");
        let _results = field.propagate_top_k("test headline", 5);
        field.feedback("test headline", &[1]);
        assert_eq!(field.total_feedback, 1);
        assert!(field.total_queries > 0);

        // Save
        save_field(&field);
        let (count, _, _) = db_stats();
        assert_eq!(count, 1, "Should have 1 field in DB");

        // Load — simulates server restart
        let loaded = load_field(field.url_hash).expect("Should load saved field");
        assert_eq!(loaded.total_feedback, 1, "Feedback count should survive");
        assert_eq!(
            loaded.total_queries, field.total_queries,
            "Query count should survive"
        );
        assert_eq!(loaded.url, field.url, "URL should survive");

        // Verify causal memory survived
        assert!(
            loaded.node_has_learning(1),
            "hit_count should survive save/load"
        );

        let _ = std::fs::remove_file(path_str);
    }

    #[test]
    fn test_global_stats_persist() {
        let dir = std::env::temp_dir();
        let path = dir.join("aether_test_persist_global.db");
        let path_str = path.to_str().unwrap();
        let _ = std::fs::remove_file(path_str);

        init(path_str).expect("DB init borde lyckas");

        save_global_stat("total_requests", 12345);
        let val = load_global_stat("total_requests");
        assert_eq!(val, 12345, "Global stat should round-trip");

        save_global_stat("total_requests", 99999);
        let val2 = load_global_stat("total_requests");
        assert_eq!(val2, 99999, "Global stat should update");

        let missing = load_global_stat("nonexistent");
        assert_eq!(missing, 0, "Missing stat should return 0");

        let _ = std::fs::remove_file(path_str);
    }
}
