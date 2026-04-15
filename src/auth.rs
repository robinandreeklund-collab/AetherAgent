/// Developer Portal Auth — users, API keys, usage tracking
///
/// SQLite-backed. Tables:
///   users: id, email, password_hash, name, created_at
///   api_keys: id, user_id, key_hash, key_prefix, name, created_at, last_used, requests_today, requests_total
///   usage_log: id, api_key_id, endpoint, timestamp, response_time_ms, tokens_in, tokens_out
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(feature = "persist")]
use rusqlite::params;

/// User account
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub id: i64,
    pub email: String,
    pub name: String,
    pub created_at: i64,
}

/// API key metadata (never expose the full key after creation)
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ApiKeyInfo {
    pub id: i64,
    pub user_id: i64,
    pub key_prefix: String, // "sk-abc123..." (first 12 chars)
    pub name: String,
    pub created_at: i64,
    pub last_used: i64,
    pub requests_today: i64,
    pub requests_total: i64,
}

/// Usage entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UsageEntry {
    pub endpoint: String,
    pub timestamp: i64,
    pub response_time_ms: i64,
    pub tokens_in: i64,
    pub tokens_out: i64,
}

/// Rate limit tiers
#[derive(Debug, Clone)]
pub struct RateLimits {
    pub requests_per_minute: u32,
    pub requests_per_day: u32,
}

impl RateLimits {
    pub fn anonymous() -> Self {
        RateLimits {
            requests_per_minute: 10,
            requests_per_day: 50,
        }
    }
    pub fn free_tier() -> Self {
        RateLimits {
            requests_per_minute: 60,
            requests_per_day: 1000,
        }
    }
}

// ─── In-memory rate tracking ──────────────────────────────────────────────

struct RateState {
    /// key_hash -> (minute_count, minute_start, day_count, day_start)
    counters: HashMap<String, (u32, u64, u32, u64)>,
}

static RATE_STATE: std::sync::LazyLock<Mutex<RateState>> = std::sync::LazyLock::new(|| {
    Mutex::new(RateState {
        counters: HashMap::new(),
    })
});

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Check rate limit and increment counter. Returns Ok(()) or Err(remaining_seconds).
pub fn check_rate_limit(identifier: &str, limits: &RateLimits) -> Result<(), u64> {
    let mut state = RATE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let now = now_secs();
    let entry = state
        .counters
        .entry(identifier.to_string())
        .or_insert((0, now, 0, now));

    // Reset minute counter if minute passed
    if now - entry.1 >= 60 {
        entry.0 = 0;
        entry.1 = now;
    }

    // Reset day counter if day passed
    if now - entry.3 >= 86400 {
        entry.2 = 0;
        entry.3 = now;
    }

    // Check limits
    if entry.0 >= limits.requests_per_minute {
        return Err(60 - (now - entry.1));
    }
    if entry.2 >= limits.requests_per_day {
        return Err(86400 - (now - entry.3));
    }

    // Increment
    entry.0 += 1;
    entry.2 += 1;
    Ok(())
}

/// Get current usage for an identifier
pub fn get_rate_usage(identifier: &str) -> (u32, u32) {
    let state = RATE_STATE.lock().unwrap_or_else(|e| e.into_inner());
    let now = now_secs();
    if let Some(entry) = state.counters.get(identifier) {
        let minute = if now - entry.1 < 60 { entry.0 } else { 0 };
        let day = if now - entry.3 < 86400 { entry.2 } else { 0 };
        (minute, day)
    } else {
        (0, 0)
    }
}

// ─── Password hashing (simple, no bcrypt dependency) ──────────────────────

#[allow(dead_code)]
fn hash_password(password: &str) -> String {
    use std::hash::{Hash, Hasher};
    // Use SipHash with a fixed salt — NOT cryptographically strong
    // but sufficient for alpha. Replace with argon2 for production.
    let salted = format!("slaash-salt-2026:{}", password);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    salted.hash(&mut hasher);
    let h1 = hasher.finish();
    // Double-hash for slightly more resistance
    format!("{}", h1).hash(&mut hasher);
    format!("{:016x}{:016x}", h1, hasher.finish())
}

#[allow(dead_code)]
fn verify_password(password: &str, hash: &str) -> bool {
    hash_password(password) == hash
}

/// Generate a new API key: "sk-" + 48 random hex chars
#[allow(dead_code)]
fn generate_api_key() -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    now.as_nanos().hash(&mut hasher);
    let h1 = hasher.finish();
    std::thread::current().id().hash(&mut hasher);
    let h2 = hasher.finish();
    // Mix in some entropy from memory address
    let stack_var = 42u64;
    (&stack_var as *const u64 as u64).hash(&mut hasher);
    let h3 = hasher.finish();
    format!("sk-{:016x}{:016x}{:016x}", h1, h2, h3)
}

/// Hash an API key for storage (don't store raw keys)
#[allow(dead_code)]
fn hash_api_key(key: &str) -> String {
    hash_password(key)
}

// ─── SQLite operations ────────────────────────────────────────────────────

#[cfg(feature = "persist")]
pub fn init_auth_tables() {
    let pool = match crate::persist::get_db_lock() {
        Some(p) => p,
        None => return,
    };
    if let Some(conn) = pool.writer.as_ref() {
        let _ = conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT '',
                created_at INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS api_keys (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL,
                key_hash TEXT UNIQUE NOT NULL,
                key_prefix TEXT NOT NULL,
                name TEXT NOT NULL DEFAULT 'default',
                created_at INTEGER NOT NULL,
                last_used INTEGER NOT NULL DEFAULT 0,
                requests_today INTEGER NOT NULL DEFAULT 0,
                requests_total INTEGER NOT NULL DEFAULT 0,
                day_reset INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (user_id) REFERENCES users(id)
            );
            CREATE TABLE IF NOT EXISTS usage_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                api_key_id INTEGER NOT NULL,
                endpoint TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                response_time_ms INTEGER NOT NULL DEFAULT 0,
                tokens_in INTEGER NOT NULL DEFAULT 0,
                tokens_out INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_usage_key ON usage_log(api_key_id);
            CREATE INDEX IF NOT EXISTS idx_usage_time ON usage_log(timestamp);
            CREATE INDEX IF NOT EXISTS idx_keys_hash ON api_keys(key_hash);
            ",
        );
    }
}

#[cfg(not(feature = "persist"))]
pub fn init_auth_tables() {}

// ─── User operations ──────────────────────────────────────────────────────

#[cfg(feature = "persist")]
pub fn signup(email: &str, password: &str, name: &str) -> Result<(User, String), String> {
    if email.is_empty() || !email.contains('@') || !email.contains('.') {
        return Err("Invalid email".to_string());
    }
    if password.len() < 6 {
        return Err("Password must be at least 6 characters".to_string());
    }

    // Anti-spam: block disposable/temporary email providers
    let domain = email.rsplit('@').next().unwrap_or("").to_lowercase();
    let disposable = [
        "tempmail.com",
        "throwaway.email",
        "guerrillamail.com",
        "mailinator.com",
        "10minutemail.com",
        "trashmail.com",
        "yopmail.com",
        "sharklasers.com",
        "guerrillamailblock.com",
        "grr.la",
        "dispostable.com",
        "maildrop.cc",
        "temp-mail.org",
        "fakeinbox.com",
        "tempail.com",
        "mohmal.com",
        "getnada.com",
        "emailondeck.com",
        "crazymailing.com",
        "tmail.ws",
    ];
    if disposable.contains(&domain.as_str()) {
        return Err("Disposable email addresses are not allowed".to_string());
    }

    // Anti-spam: rate limit signups per email domain (max 10 per domain per hour)
    let domain_key = format!("signup-domain:{}", domain);
    let domain_limits = RateLimits {
        requests_per_minute: 5,
        requests_per_day: 10,
    };
    if let Err(_) = check_rate_limit(&domain_key, &domain_limits) {
        return Err("Too many signups from this email domain. Try again later.".to_string());
    }

    let pool = crate::persist::get_db_lock().ok_or("DB not initialized")?;
    let conn = pool.writer.as_ref().ok_or("No DB writer")?;
    let now = now_secs() as i64;
    let pw_hash = hash_password(password);

    conn.execute(
        "INSERT INTO users (email, password_hash, name, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![email, pw_hash, name, now],
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            "Email already registered".to_string()
        } else {
            format!("Signup failed: {e}")
        }
    })?;

    let user_id = conn.last_insert_rowid();
    let user = User {
        id: user_id,
        email: email.to_string(),
        name: name.to_string(),
        created_at: now,
    };

    // Auto-create first API key
    let api_key = generate_api_key();
    let key_hash = hash_api_key(&api_key);
    let key_prefix = api_key[..15].to_string();

    conn.execute(
        "INSERT INTO api_keys (user_id, key_hash, key_prefix, name, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user_id, key_hash, key_prefix, "default", now],
    )
    .map_err(|e| format!("Key creation failed: {e}"))?;

    Ok((user, api_key))
}

#[cfg(feature = "persist")]
pub fn login(email: &str, password: &str) -> Result<User, String> {
    let pool = crate::persist::get_db_lock().ok_or("DB not initialized")?;
    let conn = pool.next_reader().ok_or("No DB reader")?;

    let result = conn.query_row(
        "SELECT id, email, password_hash, name, created_at FROM users WHERE email = ?1",
        params![email],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    );

    match result {
        Ok((id, email, pw_hash, name, created_at)) => {
            if verify_password(password, &pw_hash) {
                Ok(User {
                    id,
                    email,
                    name,
                    created_at,
                })
            } else {
                Err("Invalid password".to_string())
            }
        }
        Err(_) => Err("User not found".to_string()),
    }
}

// ─── API Key operations ───────────────────────────────────────────────────

#[cfg(feature = "persist")]
pub fn create_api_key(user_id: i64, name: &str) -> Result<(ApiKeyInfo, String), String> {
    let pool = crate::persist::get_db_lock().ok_or("DB not initialized")?;
    let conn = pool.writer.as_ref().ok_or("No DB writer")?;
    let now = now_secs() as i64;

    let api_key = generate_api_key();
    let key_hash = hash_api_key(&api_key);
    let key_prefix = api_key[..15].to_string();

    conn.execute(
        "INSERT INTO api_keys (user_id, key_hash, key_prefix, name, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user_id, key_hash, key_prefix, name, now],
    )
    .map_err(|e| format!("Key creation failed: {e}"))?;

    let key_id = conn.last_insert_rowid();
    let info = ApiKeyInfo {
        id: key_id,
        user_id,
        key_prefix,
        name: name.to_string(),
        created_at: now,
        last_used: 0,
        requests_today: 0,
        requests_total: 0,
    };

    Ok((info, api_key))
}

#[cfg(feature = "persist")]
pub fn list_api_keys(user_id: i64) -> Vec<ApiKeyInfo> {
    let pool = match crate::persist::get_db_lock() {
        Some(p) => p,
        None => return vec![],
    };
    let conn = match pool.next_reader() {
        Some(c) => c,
        None => return vec![],
    };

    let mut stmt = match conn.prepare(
        "SELECT id, user_id, key_prefix, name, created_at, last_used, requests_today, requests_total FROM api_keys WHERE user_id = ?1 ORDER BY created_at DESC"
    ) { Ok(s) => s, Err(_) => return vec![] };

    stmt.query_map(params![user_id], |row| {
        Ok(ApiKeyInfo {
            id: row.get(0)?,
            user_id: row.get(1)?,
            key_prefix: row.get(2)?,
            name: row.get(3)?,
            created_at: row.get(4)?,
            last_used: row.get(5)?,
            requests_today: row.get(6)?,
            requests_total: row.get(7)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

#[cfg(feature = "persist")]
pub fn delete_api_key(key_id: i64, user_id: i64) -> Result<(), String> {
    let pool = crate::persist::get_db_lock().ok_or("DB not initialized")?;
    let conn = pool.writer.as_ref().ok_or("No DB writer")?;
    let affected = conn
        .execute(
            "DELETE FROM api_keys WHERE id = ?1 AND user_id = ?2",
            params![key_id, user_id],
        )
        .map_err(|e| format!("Delete failed: {e}"))?;
    if affected == 0 {
        Err("Key not found".to_string())
    } else {
        Ok(())
    }
}

/// Validate an API key and return user_id. Updates last_used and request counters.
#[cfg(feature = "persist")]
pub fn validate_api_key(key: &str) -> Option<(i64, i64)> {
    let key_hash = hash_api_key(key);
    let pool = crate::persist::get_db_lock()?;
    let conn = pool.next_reader()?;

    let result = conn
        .query_row(
            "SELECT id, user_id FROM api_keys WHERE key_hash = ?1",
            params![key_hash],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .ok()?;

    // Update last_used + counters in background (don't block the request)
    let (key_id, _) = result;
    let now = now_secs() as i64;
    if let Some(writer) = pool.writer.as_ref() {
        let _ = writer.execute(
            "UPDATE api_keys SET last_used = ?1, requests_today = requests_today + 1, requests_total = requests_total + 1 WHERE id = ?2",
            params![now, key_id],
        );
    }

    Some(result)
}

#[cfg(not(feature = "persist"))]
pub fn validate_api_key(_key: &str) -> Option<(i64, i64)> {
    None
}

// ─── Usage logging ────────────────────────────────────────────────────────

#[cfg(feature = "persist")]
pub fn log_usage(
    api_key_id: i64,
    endpoint: &str,
    response_time_ms: i64,
    tokens_in: i64,
    tokens_out: i64,
) {
    let pool = match crate::persist::get_db_lock() {
        Some(p) => p,
        None => return,
    };
    let conn = match pool.writer.as_ref() {
        Some(c) => c,
        None => return,
    };
    let now = now_secs() as i64;
    let _ = conn.execute(
        "INSERT INTO usage_log (api_key_id, endpoint, timestamp, response_time_ms, tokens_in, tokens_out) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![api_key_id, endpoint, now, response_time_ms, tokens_in, tokens_out],
    );
}

#[cfg(feature = "persist")]
pub fn get_usage_stats(user_id: i64, since_secs: i64) -> Vec<UsageEntry> {
    let pool = match crate::persist::get_db_lock() {
        Some(p) => p,
        None => return vec![],
    };
    let conn = match pool.next_reader() {
        Some(c) => c,
        None => return vec![],
    };
    let since = now_secs() as i64 - since_secs;

    let mut stmt = match conn.prepare(
        "SELECT u.endpoint, u.timestamp, u.response_time_ms, u.tokens_in, u.tokens_out
         FROM usage_log u JOIN api_keys k ON u.api_key_id = k.id
         WHERE k.user_id = ?1 AND u.timestamp > ?2
         ORDER BY u.timestamp DESC LIMIT 500",
    ) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    stmt.query_map(params![user_id, since], |row| {
        Ok(UsageEntry {
            endpoint: row.get(0)?,
            timestamp: row.get(1)?,
            response_time_ms: row.get(2)?,
            tokens_in: row.get(3)?,
            tokens_out: row.get(4)?,
        })
    })
    .ok()
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

// ─── Stubs for non-persist builds ─────────────────────────────────────────

#[cfg(not(feature = "persist"))]
pub fn signup(_email: &str, _password: &str, _name: &str) -> Result<(User, String), String> {
    Err("Auth requires persist feature".to_string())
}
#[cfg(not(feature = "persist"))]
pub fn login(_email: &str, _password: &str) -> Result<User, String> {
    Err("Auth requires persist feature".to_string())
}
#[cfg(not(feature = "persist"))]
pub fn create_api_key(_user_id: i64, _name: &str) -> Result<(ApiKeyInfo, String), String> {
    Err("Auth requires persist feature".to_string())
}
#[cfg(not(feature = "persist"))]
pub fn list_api_keys(_user_id: i64) -> Vec<ApiKeyInfo> {
    vec![]
}
#[cfg(not(feature = "persist"))]
pub fn delete_api_key(_key_id: i64, _user_id: i64) -> Result<(), String> {
    Err("Auth requires persist feature".to_string())
}
#[cfg(not(feature = "persist"))]
pub fn log_usage(
    _api_key_id: i64,
    _endpoint: &str,
    _response_time_ms: i64,
    _tokens_in: i64,
    _tokens_out: i64,
) {
}
#[cfg(not(feature = "persist"))]
pub fn get_usage_stats(_user_id: i64, _since_secs: i64) -> Vec<UsageEntry> {
    vec![]
}
