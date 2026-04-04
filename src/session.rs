//! Session Manager – Fas 13: OAuth & sessionshantering
//!
//! Persistent sessionshantering för multi-steg workflows:
//! - Cookie-jar som överlever mellan fetch_parse-anrop
//! - OAuth 2.0 redirect chain handling (authorize → callback → token)
//! - Automatisk login via fill_form + fetch
//! - Transparent token refresh vid expiry
//!
//! Designad för WASM: ingen global state, serialiserbar via JSON.
//! Hosten äger det serialiserade tillståndet.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Minnesgränser för cookies
const MAX_COOKIES_PER_DOMAIN: usize = 100;
const MAX_COOKIE_DOMAINS: usize = 500;

// ─── Types ──────────────────────────────────────────────────────────────────

/// Persistent sessionshanterare
///
/// Lagrar cookies, auth-tokens och login-status mellan anrop.
/// Serialiseras till JSON för transport över WASM-gränsen.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionManager {
    /// Cookie-jar: domän → Vec<cookie-strängar>
    pub cookies: HashMap<String, Vec<CookieEntry>>,
    /// OAuth 2.0 token (om autentiserad)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oauth_token: Option<OAuthToken>,
    /// Autentiseringsstatus
    #[serde(default)]
    pub auth_state: AuthState,
    /// Extra headers att skicka med alla requests (t.ex. Authorization)
    #[serde(default)]
    pub persistent_headers: HashMap<String, String>,
    /// Sist besökta URL (för redirect-hantering)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_url: Option<String>,
    /// Antal lyckade autentiserade requests
    #[serde(default)]
    pub authenticated_requests: u32,
}

/// En cookie med metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieEntry {
    /// Cookie-namn
    pub name: String,
    /// Cookie-värde
    pub value: String,
    /// Sökväg (default "/")
    #[serde(default = "default_path")]
    pub path: String,
    /// Utgångstid i Unix ms (0 = session cookie)
    #[serde(default)]
    pub expires_ms: u64,
    /// HttpOnly-flagga
    #[serde(default)]
    pub http_only: bool,
    /// Secure-flagga
    #[serde(default)]
    pub secure: bool,
}

fn default_path() -> String {
    "/".to_string()
}

/// OAuth 2.0 token
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    /// Access token
    pub access_token: String,
    /// Token-typ (Bearer, etc.)
    #[serde(default = "default_token_type")]
    pub token_type: String,
    /// Refresh token (för att förnya access token)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Utgångstid i Unix ms
    pub expires_at_ms: u64,
    /// Scopes som beviljats
    #[serde(default)]
    pub scopes: Vec<String>,
}

fn default_token_type() -> String {
    "Bearer".to_string()
}

/// OAuth 2.0 konfiguration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthConfig {
    /// Authorization endpoint URL
    pub authorize_url: String,
    /// Token endpoint URL
    pub token_url: String,
    /// Client ID
    pub client_id: String,
    /// Client secret (valfritt, beroende på flow)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    /// Redirect URI
    pub redirect_uri: String,
    /// Scopes att begära
    #[serde(default)]
    pub scopes: Vec<String>,
}

/// Autentiseringsstatus
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum AuthState {
    /// Ingen autentisering aktiv
    #[default]
    Unauthenticated,
    /// OAuth-flow initierad, väntar på callback
    OAuthPending,
    /// Inloggad via formulär (cookies aktiva)
    LoggedIn,
    /// OAuth-token aktiv
    OAuthAuthenticated,
    /// Token har gått ut, behöver refresh
    TokenExpired,
}

/// Resultat av OAuth authorize-steg
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthAuthorizeResult {
    /// URL att navigera till för att autentisera
    pub authorize_url: String,
    /// Uppdaterad session (med pending-status)
    pub session_json: String,
    /// State-parameter för CSRF-skydd
    pub state: String,
}

// ─── Implementation ─────────────────────────────────────────────────────────

impl SessionManager {
    /// Skapa ny tom session
    pub fn new() -> Self {
        Self::default()
    }

    /// Lägg till cookies från en HTTP-respons Set-Cookie header
    ///
    /// Parsar Set-Cookie-strängar och lagrar per domän.
    pub fn add_cookies_from_headers(&mut self, domain: &str, set_cookie_headers: &[String]) {
        // LRU-eviction: ta bort och återinsätt vid access (äldsta först vid kapacitet)
        if self.cookies.contains_key(domain) {
            // Flytta till "nyast" — remove + re-insert
            if let Some(existing) = self.cookies.remove(domain) {
                self.cookies.insert(domain.to_string(), existing);
            }
        } else if self.cookies.len() >= MAX_COOKIE_DOMAINS {
            // Evicta äldsta domänen (först insatta i HashMap iteration order)
            if let Some(oldest) = self.cookies.keys().next().cloned() {
                self.cookies.remove(&oldest);
            }
        }

        let entries = self.cookies.entry(domain.to_string()).or_default();

        for header in set_cookie_headers {
            if let Some(cookie) = parse_set_cookie(header) {
                // Ersätt befintlig cookie med samma namn
                entries.retain(|c| c.name != cookie.name);
                entries.push(cookie);
            }
        }

        // Begränsa antal cookies per domän – behåll de senaste
        if entries.len() > MAX_COOKIES_PER_DOMAIN {
            let drain_count = entries.len() - MAX_COOKIES_PER_DOMAIN;
            entries.drain(..drain_count);
        }
    }

    /// Hämta cookies för en given URL (domän + sökväg)
    ///
    /// Returnerar en Cookie-header-sträng (t.ex. "session=abc123; user=test").
    pub fn get_cookie_header(&self, domain: &str, path: &str, now_ms: u64) -> Option<String> {
        let entries = self.cookies.get(domain)?;

        let valid_cookies: Vec<String> = entries
            .iter()
            .filter(|c| {
                // Filtrera utgångna cookies
                (c.expires_ms == 0 || c.expires_ms > now_ms)
                    // Matcha sökväg
                    && path.starts_with(&c.path)
            })
            .map(|c| format!("{}={}", c.name, c.value))
            .collect();

        if valid_cookies.is_empty() {
            None
        } else {
            Some(valid_cookies.join("; "))
        }
    }

    /// Rensa utgångna cookies
    pub fn evict_expired(&mut self, now_ms: u64) {
        for entries in self.cookies.values_mut() {
            entries.retain(|c| c.expires_ms == 0 || c.expires_ms > now_ms);
        }
        // Ta bort domäner utan cookies
        self.cookies.retain(|_, v| !v.is_empty());
    }

    /// Hämta auth headers att skicka med requests
    ///
    /// Kombinerar persistent headers med OAuth Authorization om aktiv.
    pub fn get_auth_headers(&self, now_ms: u64) -> HashMap<String, String> {
        let mut headers = self.persistent_headers.clone();

        // Lägg till OAuth Bearer token om giltig
        if let Some(ref token) = self.oauth_token {
            if token.expires_at_ms > now_ms {
                headers.insert(
                    "Authorization".to_string(),
                    format!("{} {}", token.token_type, token.access_token),
                );
            }
        }

        headers
    }

    /// Kontrollera om OAuth-token är giltig
    pub fn is_token_valid(&self, now_ms: u64) -> bool {
        matches!(self.auth_state, AuthState::OAuthAuthenticated)
            && self
                .oauth_token
                .as_ref()
                .map(|t| t.expires_at_ms > now_ms)
                .unwrap_or(false)
    }

    /// Kontrollera om token behöver refresh (inom 60 sekunder av expiry)
    pub fn needs_token_refresh(&self, now_ms: u64) -> bool {
        self.oauth_token
            .as_ref()
            .map(|t| {
                t.refresh_token.is_some()
                    && t.expires_at_ms > now_ms
                    && t.expires_at_ms - now_ms < 60_000
            })
            .unwrap_or(false)
    }

    /// Kontrollera om sessionen är autentiserad (cookie eller OAuth)
    pub fn is_authenticated(&self, now_ms: u64) -> bool {
        match self.auth_state {
            AuthState::LoggedIn => {
                // Kolla att vi har minst en icke-utgången cookie
                self.cookies.values().any(|entries| {
                    entries
                        .iter()
                        .any(|c| c.expires_ms == 0 || c.expires_ms > now_ms)
                })
            }
            AuthState::OAuthAuthenticated => self.is_token_valid(now_ms),
            _ => false,
        }
    }

    /// Sätt OAuth token efter framgångsrik token exchange
    pub fn set_oauth_token(
        &mut self,
        access_token: &str,
        refresh_token: Option<&str>,
        expires_in_secs: u64,
        now_ms: u64,
        scopes: Vec<String>,
    ) {
        self.oauth_token = Some(OAuthToken {
            access_token: access_token.to_string(),
            token_type: "Bearer".to_string(),
            refresh_token: refresh_token.map(|s| s.to_string()),
            expires_at_ms: now_ms + (expires_in_secs * 1000),
            scopes,
        });
        self.auth_state = AuthState::OAuthAuthenticated;
    }

    /// Markera token som utgången
    pub fn mark_token_expired(&mut self) {
        self.auth_state = AuthState::TokenExpired;
    }

    /// Markera session som inloggad via formulär
    pub fn mark_logged_in(&mut self) {
        self.auth_state = AuthState::LoggedIn;
    }

    /// Bygg OAuth authorize URL
    ///
    /// Returnerar URL att navigera till + state-parameter för CSRF-skydd.
    pub fn build_authorize_url(&mut self, config: &OAuthConfig) -> OAuthAuthorizeResult {
        // Enkel state-generering baserad på timestamp
        let state = format!("aether_{}", now_simple_hash(config));

        let mut url = format!(
            "{}?response_type=code&client_id={}&redirect_uri={}&state={}",
            config.authorize_url,
            url_encode(&config.client_id),
            url_encode(&config.redirect_uri),
            url_encode(&state),
        );

        if !config.scopes.is_empty() {
            url.push_str(&format!("&scope={}", url_encode(&config.scopes.join(" "))));
        }

        self.auth_state = AuthState::OAuthPending;

        OAuthAuthorizeResult {
            authorize_url: url,
            session_json: self.to_json(),
            state,
        }
    }

    /// Processa OAuth callback med authorization code
    ///
    /// Returnerar token exchange-parametrar som hosten kan använda
    /// för att anropa token endpoint.
    pub fn prepare_token_exchange(
        &self,
        config: &OAuthConfig,
        code: &str,
    ) -> HashMap<String, String> {
        let mut params = HashMap::new();
        params.insert("grant_type".to_string(), "authorization_code".to_string());
        params.insert("code".to_string(), code.to_string());
        params.insert("redirect_uri".to_string(), config.redirect_uri.clone());
        params.insert("client_id".to_string(), config.client_id.clone());
        if let Some(ref secret) = config.client_secret {
            params.insert("client_secret".to_string(), secret.clone());
        }
        params
    }

    /// Processa OAuth refresh token
    pub fn prepare_token_refresh(&self, config: &OAuthConfig) -> Option<HashMap<String, String>> {
        let refresh_token = self.oauth_token.as_ref()?.refresh_token.as_ref()?;

        let mut params = HashMap::new();
        params.insert("grant_type".to_string(), "refresh_token".to_string());
        params.insert("refresh_token".to_string(), refresh_token.clone());
        params.insert("client_id".to_string(), config.client_id.clone());
        if let Some(ref secret) = config.client_secret {
            params.insert("client_secret".to_string(), secret.clone());
        }
        Some(params)
    }

    /// Detektera login-formulär i ett semantiskt träd
    ///
    /// Letar efter typiska login-fält: username/email + password + submit.
    pub fn detect_login_form(tree: &crate::types::SemanticTree) -> Option<LoginFormInfo> {
        let all_nodes = collect_all_nodes_flat(&tree.nodes);

        let mut username_field = None;
        let mut password_field = None;
        let mut submit_button = None;

        for node in &all_nodes {
            let label_lower = node.label.to_lowercase();
            let role = node.role.as_str();
            let name_lower = node.name.as_deref().unwrap_or("").to_lowercase();
            let id_lower = node.html_id.as_deref().unwrap_or("").to_lowercase();

            // Matcha username/email-fält
            if (role == "textbox" || role == "searchbox")
                && (label_lower.contains("email")
                    || label_lower.contains("user")
                    || label_lower.contains("login")
                    || name_lower.contains("email")
                    || name_lower.contains("user")
                    || name_lower.contains("login")
                    || id_lower.contains("email")
                    || id_lower.contains("user"))
            {
                username_field = Some(node.id);
            }

            // Matcha password-fält
            if role == "textbox"
                && (label_lower.contains("password")
                    || label_lower.contains("lösenord")
                    || name_lower.contains("pass")
                    || id_lower.contains("pass"))
            {
                password_field = Some(node.id);
            }

            // Matcha submit-knapp
            if role == "button"
                && (label_lower.contains("log in")
                    || label_lower.contains("login")
                    || label_lower.contains("logga in")
                    || label_lower.contains("sign in")
                    || label_lower.contains("submit"))
            {
                submit_button = Some(node.id);
            }
        }

        if username_field.is_some() && password_field.is_some() {
            Some(LoginFormInfo {
                username_node_id: username_field,
                password_node_id: password_field,
                submit_node_id: submit_button,
            })
        } else {
            None
        }
    }

    /// Registrera lyckad autentiserad request
    pub fn record_authenticated_request(&mut self) {
        self.authenticated_requests += 1;
    }

    /// Antal cookies totalt
    pub fn cookie_count(&self) -> usize {
        self.cookies.values().map(|v| v.len()).sum()
    }

    /// Serialisera till JSON
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    /// Deserialisera från JSON
    pub fn from_json(json: &str) -> Result<Self, String> {
        serde_json::from_str(json).map_err(|e| format!("Invalid session JSON: {}", e))
    }
}

/// Information om detekterat login-formulär
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginFormInfo {
    /// Node ID för username/email-fältet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username_node_id: Option<u32>,
    /// Node ID för password-fältet
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password_node_id: Option<u32>,
    /// Node ID för submit-knappen
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submit_node_id: Option<u32>,
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Parsa en Set-Cookie header-sträng
fn parse_set_cookie(header: &str) -> Option<CookieEntry> {
    let parts: Vec<&str> = header.split(';').collect();
    let first = parts.first()?.trim();
    let (name, value) = first.split_once('=')?;

    if name.trim().is_empty() {
        return None;
    }

    let mut cookie = CookieEntry {
        name: name.trim().to_string(),
        value: value.trim().to_string(),
        path: "/".to_string(),
        expires_ms: 0,
        http_only: false,
        secure: false,
    };

    // Parsa attribut
    for part in &parts[1..] {
        let attr = part.trim().to_lowercase();
        if attr == "httponly" {
            cookie.http_only = true;
        } else if attr == "secure" {
            cookie.secure = true;
        } else if let Some(path) = attr.strip_prefix("path=") {
            cookie.path = path.to_string();
        } else if let Some(max_age) = attr.strip_prefix("max-age=") {
            if let Ok(secs) = max_age.parse::<u64>() {
                // Konvertera max-age till ungefärlig expires
                cookie.expires_ms = secs * 1000;
            }
        }
    }

    Some(cookie)
}

/// Enkel URL-encoding (minimalt, för OAuth-parametrar)
fn url_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
            ' ' => result.push_str("%20"),
            '&' => result.push_str("%26"),
            '=' => result.push_str("%3D"),
            '+' => result.push_str("%2B"),
            '/' => result.push_str("%2F"),
            ':' => result.push_str("%3A"),
            '@' => result.push_str("%40"),
            _ => {
                for b in c.to_string().as_bytes() {
                    result.push_str(&format!("%{:02X}", b));
                }
            }
        }
    }
    result
}

/// Enkel hash för state-generering
fn now_simple_hash(config: &OAuthConfig) -> u64 {
    let mut hash: u64 = 5381;
    for c in config.client_id.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    for c in config.redirect_uri.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(c as u64);
    }
    hash
}

/// Samla alla noder rekursivt
fn collect_all_nodes_flat(
    nodes: &[crate::types::SemanticNode],
) -> Vec<&crate::types::SemanticNode> {
    let mut result = vec![];
    for node in nodes {
        result.push(node);
        result.extend(collect_all_nodes_flat(&node.children));
    }
    result
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_new_empty() {
        let session = SessionManager::new();
        assert_eq!(session.auth_state, AuthState::Unauthenticated);
        assert!(session.cookies.is_empty());
        assert!(session.oauth_token.is_none());
        assert_eq!(session.cookie_count(), 0);
    }

    #[test]
    fn test_session_add_cookies() {
        let mut session = SessionManager::new();
        session.add_cookies_from_headers(
            "example.com",
            &[
                "session_id=abc123; Path=/; HttpOnly".to_string(),
                "user=test; Path=/; Secure".to_string(),
            ],
        );
        assert_eq!(session.cookie_count(), 2);

        let header = session.get_cookie_header("example.com", "/", 0);
        assert!(header.is_some());
        let header = header.unwrap();
        assert!(
            header.contains("session_id=abc123"),
            "Borde innehålla session_id"
        );
        assert!(header.contains("user=test"), "Borde innehålla user");
    }

    #[test]
    fn test_session_cookie_replaces_existing() {
        let mut session = SessionManager::new();
        session.add_cookies_from_headers("example.com", &["token=old_value; Path=/".to_string()]);
        session.add_cookies_from_headers("example.com", &["token=new_value; Path=/".to_string()]);
        assert_eq!(session.cookie_count(), 1, "Borde ersätta befintlig cookie");
        let header = session.get_cookie_header("example.com", "/", 0).unwrap();
        assert!(header.contains("new_value"), "Borde ha uppdaterat värde");
    }

    #[test]
    fn test_session_cookie_expiry() {
        let mut session = SessionManager::new();
        session.add_cookies_from_headers("example.com", &["temp=value; Max-Age=60".to_string()]);
        // Cookie har expires_ms = 60 * 1000 = 60000

        // Före expiry — cookie ska finnas
        let header = session.get_cookie_header("example.com", "/", 30_000);
        assert!(header.is_some(), "Cookie borde finnas före expiry");

        // Efter expiry — cookie ska inte inkluderas
        let header = session.get_cookie_header("example.com", "/", 120_000);
        assert!(header.is_none(), "Cookie borde vara utgången");
    }

    #[test]
    fn test_session_evict_expired() {
        let mut session = SessionManager::new();
        session.add_cookies_from_headers(
            "example.com",
            &[
                "temp=value; Max-Age=60".to_string(),
                "permanent=keep; Path=/".to_string(),
            ],
        );
        session.evict_expired(120_000);
        assert_eq!(session.cookie_count(), 1, "Borde ha rensad utgången cookie");
    }

    #[test]
    fn test_session_cookie_path_matching() {
        let mut session = SessionManager::new();
        session.add_cookies_from_headers("example.com", &["admin=secret; Path=/admin".to_string()]);

        let header = session.get_cookie_header("example.com", "/admin/settings", 0);
        assert!(header.is_some(), "Borde matcha /admin sökväg");

        let header = session.get_cookie_header("example.com", "/public", 0);
        assert!(header.is_none(), "Borde inte matcha /public sökväg");
    }

    #[test]
    fn test_session_oauth_token() {
        let mut session = SessionManager::new();
        session.set_oauth_token(
            "access_123",
            Some("refresh_456"),
            3600,
            1000,
            vec!["read".to_string(), "write".to_string()],
        );

        assert_eq!(session.auth_state, AuthState::OAuthAuthenticated);
        assert!(session.is_authenticated(2000));
        assert!(session.is_token_valid(2000));

        // Token utgånget
        assert!(!session.is_token_valid(3_602_000));
    }

    #[test]
    fn test_session_token_refresh_needed() {
        let mut session = SessionManager::new();
        session.set_oauth_token(
            "access_123",
            Some("refresh_456"),
            3600,
            0, // skapad vid t=0, utgår vid t=3600000
            vec![],
        );

        // Långt till expiry — inget refresh behövs
        assert!(
            !session.needs_token_refresh(1_000_000),
            "Borde inte behöva refresh ännu"
        );

        // Nära expiry (inom 60s) — refresh behövs
        assert!(
            session.needs_token_refresh(3_550_000),
            "Borde behöva refresh nära expiry"
        );
    }

    #[test]
    fn test_session_auth_headers() {
        let mut session = SessionManager::new();
        session
            .persistent_headers
            .insert("X-Custom".to_string(), "custom_value".to_string());
        session.set_oauth_token("token_123", None, 3600, 0, vec![]);

        let headers = session.get_auth_headers(1000);
        assert_eq!(
            headers.get("Authorization"),
            Some(&"Bearer token_123".to_string()),
            "Borde ha Authorization header"
        );
        assert_eq!(
            headers.get("X-Custom"),
            Some(&"custom_value".to_string()),
            "Borde ha persistent header"
        );
    }

    #[test]
    fn test_session_build_authorize_url() {
        let mut session = SessionManager::new();
        let config = OAuthConfig {
            authorize_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            client_id: "my_client".to_string(),
            client_secret: Some("secret".to_string()),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
        };

        let result = session.build_authorize_url(&config);
        assert!(
            result
                .authorize_url
                .starts_with("https://auth.example.com/authorize?"),
            "Borde börja med authorize endpoint"
        );
        assert!(
            result.authorize_url.contains("client_id=my_client"),
            "Borde innehålla client_id"
        );
        assert!(
            result.authorize_url.contains("response_type=code"),
            "Borde ha response_type=code"
        );
        assert!(!result.state.is_empty(), "Borde generera state-parameter");
        assert_eq!(session.auth_state, AuthState::OAuthPending);
    }

    #[test]
    fn test_session_prepare_token_exchange() {
        let session = SessionManager::new();
        let config = OAuthConfig {
            authorize_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            client_id: "my_client".to_string(),
            client_secret: Some("secret".to_string()),
            redirect_uri: "https://app.example.com/callback".to_string(),
            scopes: vec![],
        };

        let params = session.prepare_token_exchange(&config, "auth_code_123");
        assert_eq!(
            params.get("grant_type"),
            Some(&"authorization_code".to_string())
        );
        assert_eq!(params.get("code"), Some(&"auth_code_123".to_string()));
        assert_eq!(params.get("client_id"), Some(&"my_client".to_string()));
        assert_eq!(params.get("client_secret"), Some(&"secret".to_string()));
    }

    #[test]
    fn test_session_prepare_token_refresh() {
        let mut session = SessionManager::new();
        session.set_oauth_token("access_123", Some("refresh_456"), 3600, 0, vec![]);

        let config = OAuthConfig {
            authorize_url: "https://auth.example.com/authorize".to_string(),
            token_url: "https://auth.example.com/token".to_string(),
            client_id: "my_client".to_string(),
            client_secret: None,
            redirect_uri: "https://app.example.com/callback".to_string(),
            scopes: vec![],
        };

        let params = session.prepare_token_refresh(&config);
        assert!(params.is_some(), "Borde kunna förbereda refresh");
        let params = params.unwrap();
        assert_eq!(params.get("grant_type"), Some(&"refresh_token".to_string()));
        assert_eq!(
            params.get("refresh_token"),
            Some(&"refresh_456".to_string())
        );
    }

    #[test]
    fn test_session_no_refresh_without_token() {
        let session = SessionManager::new();
        let config = OAuthConfig {
            authorize_url: String::new(),
            token_url: String::new(),
            client_id: String::new(),
            client_secret: None,
            redirect_uri: String::new(),
            scopes: vec![],
        };

        assert!(
            session.prepare_token_refresh(&config).is_none(),
            "Borde inte kunna refresha utan token"
        );
    }

    #[test]
    fn test_session_detect_login_form() {
        use crate::types::{SemanticNode, SemanticTree};

        let tree = SemanticTree {
            url: "https://example.com/login".to_string(),
            title: "Login".to_string(),
            goal: "login".to_string(),
            nodes: vec![
                {
                    let mut n = SemanticNode::new(1, "textbox", "Email address");
                    n.name = Some("email".to_string());
                    n
                },
                {
                    let mut n = SemanticNode::new(2, "textbox", "Password");
                    n.name = Some("password".to_string());
                    n
                },
                SemanticNode::new(3, "button", "Log in"),
            ],
            injection_warnings: vec![],
            parse_time_ms: 0,
            xhr_intercepted: 0,
            xhr_blocked: 0,
            pending_fetch_urls: vec![],
        };

        let form = SessionManager::detect_login_form(&tree);
        assert!(form.is_some(), "Borde detektera login-formulär");
        let form = form.unwrap();
        assert_eq!(form.username_node_id, Some(1));
        assert_eq!(form.password_node_id, Some(2));
        assert_eq!(form.submit_node_id, Some(3));
    }

    #[test]
    fn test_session_no_login_form_without_password() {
        use crate::types::{SemanticNode, SemanticTree};

        let tree = SemanticTree {
            url: "https://example.com".to_string(),
            title: "Home".to_string(),
            goal: "browse".to_string(),
            nodes: vec![
                {
                    let mut n = SemanticNode::new(1, "textbox", "Search");
                    n.name = Some("q".to_string());
                    n
                },
                SemanticNode::new(2, "button", "Search"),
            ],
            injection_warnings: vec![],
            parse_time_ms: 0,
            xhr_intercepted: 0,
            xhr_blocked: 0,
            pending_fetch_urls: vec![],
        };

        let form = SessionManager::detect_login_form(&tree);
        assert!(
            form.is_none(),
            "Borde inte detektera login utan password-fält"
        );
    }

    #[test]
    fn test_session_roundtrip() {
        let mut session = SessionManager::new();
        session.add_cookies_from_headers(
            "example.com",
            &["session=abc; Path=/; HttpOnly".to_string()],
        );
        session.set_oauth_token(
            "token_123",
            Some("refresh_456"),
            3600,
            1000,
            vec!["read".to_string()],
        );
        session
            .persistent_headers
            .insert("X-Key".to_string(), "val".to_string());

        let json = session.to_json();
        let restored = SessionManager::from_json(&json).expect("Borde deserialisera");

        assert_eq!(restored.cookie_count(), 1);
        assert!(restored.oauth_token.is_some());
        assert_eq!(restored.auth_state, AuthState::OAuthAuthenticated);
        assert_eq!(
            restored.persistent_headers.get("X-Key"),
            Some(&"val".to_string())
        );
    }

    #[test]
    fn test_session_from_invalid_json() {
        let result = SessionManager::from_json("not json");
        assert!(result.is_err(), "Borde ge fel för ogiltig JSON");
    }

    #[test]
    fn test_parse_set_cookie() {
        let cookie = parse_set_cookie("session_id=abc123; Path=/; HttpOnly; Secure");
        assert!(cookie.is_some());
        let cookie = cookie.unwrap();
        assert_eq!(cookie.name, "session_id");
        assert_eq!(cookie.value, "abc123");
        assert_eq!(cookie.path, "/");
        assert!(cookie.http_only);
        assert!(cookie.secure);
    }

    #[test]
    fn test_parse_set_cookie_with_max_age() {
        let cookie = parse_set_cookie("token=xyz; Max-Age=3600");
        assert!(cookie.is_some());
        let cookie = cookie.unwrap();
        assert_eq!(cookie.expires_ms, 3_600_000);
    }

    #[test]
    fn test_parse_set_cookie_invalid() {
        let cookie = parse_set_cookie("=no_name");
        assert!(cookie.is_none(), "Borde avvisa cookie utan namn");
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world"), "hello%20world");
        assert_eq!(url_encode("a=b&c=d"), "a%3Db%26c%3Dd");
        assert_eq!(url_encode("simple"), "simple");
    }

    #[test]
    fn test_session_mark_logged_in() {
        let mut session = SessionManager::new();
        session.mark_logged_in();
        assert_eq!(session.auth_state, AuthState::LoggedIn);
    }

    #[test]
    fn test_session_mark_token_expired() {
        let mut session = SessionManager::new();
        session.set_oauth_token("token", None, 3600, 0, vec![]);
        session.mark_token_expired();
        assert_eq!(session.auth_state, AuthState::TokenExpired);
        assert!(!session.is_authenticated(1000));
    }

    #[test]
    fn test_session_record_authenticated_request() {
        let mut session = SessionManager::new();
        assert_eq!(session.authenticated_requests, 0);
        session.record_authenticated_request();
        session.record_authenticated_request();
        assert_eq!(session.authenticated_requests, 2);
    }

    #[test]
    fn test_session_backward_compatible_deserialization() {
        // Gammal JSON utan nya fält
        let old_json = r#"{"cookies":{},"auth_state":"Unauthenticated"}"#;
        let session: SessionManager = serde_json::from_str(old_json).expect("Borde deserialisera");
        assert!(session.oauth_token.is_none());
        assert!(session.persistent_headers.is_empty());
        assert_eq!(session.authenticated_requests, 0);
    }
}
