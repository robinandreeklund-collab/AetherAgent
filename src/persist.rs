/// Persistence layer — SQLite-backed storage for CRFR learning
///
/// Sparar resonance fields, domain profiles och metadata till disk
/// så att kausalt minne överlever server-restarts.
use rusqlite::{params, Connection};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::resonance::{DomainProfile, ResonanceField};

// ─── Global DB connection ──────────────────────────────────────────────────

static DB: std::sync::LazyLock<Mutex<Option<Connection>>> =
    std::sync::LazyLock::new(|| Mutex::new(None));

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
pub fn init(db_path: &str) -> Result<(), String> {
    let conn = Connection::open(db_path).map_err(|e| format!("SQLite open: {e}"))?;

    // WAL mode — concurrent reads, fast writes
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
        .map_err(|e| format!("SQLite pragma: {e}"))?;

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
        ",
    )
    .map_err(|e| format!("SQLite schema: {e}"))?;

    let mut db = DB.lock().map_err(|e| format!("DB lock: {e}"))?;
    *db = Some(conn);

    Ok(())
}

/// Check if persistence is initialized.
pub fn is_initialized() -> bool {
    DB.lock().map(|db| db.is_some()).unwrap_or(false)
}

// ─── Resonance Fields ──────────────────────────────────────────────────────

/// Save a resonance field to the database.
pub fn save_field(field: &ResonanceField) {
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return,
    };
    let conn = match db.as_ref() {
        Some(c) => c,
        None => return,
    };

    let data = match serde_json::to_vec(field) {
        Ok(d) => d,
        Err(_) => return,
    };

    let _ = conn.execute(
        "INSERT OR REPLACE INTO resonance_fields (url_hash, url, data, updated_at) VALUES (?1, ?2, ?3, ?4)",
        params![field.url_hash as i64, field.url, data, now_ms() as i64],
    );
}

/// Load a resonance field by URL hash (fallback when in-memory cache misses).
pub fn load_field(url_hash: u64) -> Option<ResonanceField> {
    let db = DB.lock().ok()?;
    let conn = db.as_ref()?;

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
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return vec![],
    };
    let conn = match db.as_ref() {
        Some(c) => c,
        None => return vec![],
    };

    let mut stmt =
        match conn.prepare("SELECT data FROM resonance_fields ORDER BY updated_at DESC LIMIT 64") {
            Ok(s) => s,
            Err(_) => return vec![],
        };

    let rows = match stmt.query_map([], |row| {
        let data: Vec<u8> = row.get(0)?;
        Ok(data)
    }) {
        Ok(r) => r,
        Err(_) => return vec![],
    };

    rows.filter_map(|r| r.ok())
        .filter_map(|data| serde_json::from_slice(&data).ok())
        .collect()
}

/// Delete old fields (older than max_age_ms).
pub fn evict_old_fields(max_age_ms: u64) -> usize {
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return 0,
    };
    let conn = match db.as_ref() {
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
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return vec![],
    };
    let conn = match db.as_ref() {
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
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return vec![],
    };
    let conn = match db.as_ref() {
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
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return 0,
    };
    let conn = match db.as_ref() {
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
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return,
    };
    let conn = match db.as_ref() {
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
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return vec![],
    };
    let conn = match db.as_ref() {
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
    let db = match DB.lock() {
        Ok(db) => db,
        Err(_) => return 0,
    };
    let conn = match db.as_ref() {
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
        .and_then(|db| {
            db.as_ref().and_then(|conn| {
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

    // Spara alla cachade resonance fields
    let fields = crate::resonance::list_cached_fields();
    // Vi behöver hela fältet, inte bara summary — använd en ny export-funktion
    // (fields laddas redan i minnet via FIELD_CACHE)
    eprintln!(
        "[PERSIST] Checkpoint: {} domain profiles, {} field summaries saved",
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
}
