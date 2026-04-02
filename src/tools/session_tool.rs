// Tool 10: session — Session lifecycle management
//
// Ersätter: alla 11 session-endpoints + detect_login_form

use serde::Deserialize;

use super::{build_tree, now_ms, ToolResult};

/// Request-parametrar för session-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct SessionRequest {
    /// Åtgärd (krävs): "create", "status", "cookies", "token", "oauth",
    /// "detect_login", "evict", "mark_logged_in", "refresh"
    pub action: String,
    /// Session JSON-state
    #[serde(default)]
    pub session_json: Option<String>,
    /// Cookie-domän
    #[serde(default)]
    pub domain: Option<String>,
    /// Cookie-path
    #[serde(default)]
    pub path: Option<String>,
    /// Set-Cookie headers
    #[serde(default)]
    pub cookies: Option<Vec<String>>,
    /// Access token
    #[serde(default)]
    pub access_token: Option<String>,
    /// Refresh token
    #[serde(default)]
    pub refresh_token: Option<String>,
    /// Token expiry (sekunder)
    #[serde(default)]
    pub expires_in: Option<u64>,
    /// Token scopes
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
    /// OAuth-config JSON
    #[serde(default)]
    pub oauth_config: Option<String>,
    /// Authorization code (för token exchange)
    #[serde(default)]
    pub code: Option<String>,
    /// HTML (för detect_login)
    #[serde(default)]
    pub html: Option<String>,
    /// Goal (för detect_login)
    #[serde(default)]
    pub goal: Option<String>,
    /// URL (för detect_login)
    #[serde(default)]
    pub url: Option<String>,
}

/// Kör session-verktyget
pub fn execute(req: &SessionRequest) -> ToolResult {
    let start = now_ms();

    match req.action.as_str() {
        "create" => {
            let session = crate::session::SessionManager::new();
            let data = serde_json::json!({
                "session_json": session.to_json(),
                "status": "created",
            });
            ToolResult::ok(data, now_ms().saturating_sub(start))
        }
        "status" => {
            let session = match parse_session(&req.session_json, start) {
                Ok(s) => s,
                Err(e) => return e,
            };
            let now = now_ms();
            let data = serde_json::json!({
                "authenticated": session.is_authenticated(now),
                "token_valid": session.is_token_valid(now),
                "needs_refresh": session.needs_token_refresh(now),
                "cookie_count": session.cookie_count(),
            });
            ToolResult::ok(data, now_ms().saturating_sub(start))
        }
        "cookies" => execute_cookies(req, start),
        "token" => execute_token(req, start),
        "oauth" => execute_oauth(req, start),
        "detect_login" => execute_detect_login(req, start),
        "evict" => {
            let mut session = match parse_session(&req.session_json, start) {
                Ok(s) => s,
                Err(e) => return e,
            };
            session.evict_expired(now_ms());
            let data = serde_json::json!({
                "session_json": session.to_json(),
                "cookie_count": session.cookie_count(),
            });
            ToolResult::ok(data, now_ms().saturating_sub(start))
        }
        "mark_logged_in" => {
            let mut session = match parse_session(&req.session_json, start) {
                Ok(s) => s,
                Err(e) => return e,
            };
            session.mark_logged_in();
            let data = serde_json::json!({
                "session_json": session.to_json(),
                "authenticated": true,
            });
            ToolResult::ok(data, now_ms().saturating_sub(start))
        }
        "refresh" => execute_refresh(req, start),
        other => ToolResult::err(
            format!("Okänd action: '{other}'. Använd: create, status, cookies, token, oauth, detect_login, evict, mark_logged_in, refresh."),
            now_ms().saturating_sub(start),
        ),
    }
}

fn parse_session(
    json: &Option<String>,
    start: u64,
) -> Result<crate::session::SessionManager, ToolResult> {
    match json {
        Some(j) => crate::session::SessionManager::from_json(j).map_err(|e| {
            ToolResult::err(
                format!("Ogiltig session_json: {e}"),
                now_ms().saturating_sub(start),
            )
        }),
        None => Err(ToolResult::err(
            "'session_json' krävs",
            now_ms().saturating_sub(start),
        )),
    }
}

fn execute_cookies(req: &SessionRequest, start: u64) -> ToolResult {
    let mut session = match parse_session(&req.session_json, start) {
        Ok(s) => s,
        Err(e) => return e,
    };

    let domain = match &req.domain {
        Some(d) => d.as_str(),
        None => {
            return ToolResult::err(
                "'domain' krävs för action=cookies",
                now_ms().saturating_sub(start),
            )
        }
    };

    // Om cookies skickas → lägg till, annars → hämta
    if let Some(ref cookie_headers) = req.cookies {
        session.add_cookies_from_headers(domain, cookie_headers);
        let data = serde_json::json!({
            "session_json": session.to_json(),
            "cookies_added": cookie_headers.len(),
        });
        ToolResult::ok(data, now_ms().saturating_sub(start))
    } else {
        let path = req.path.as_deref().unwrap_or("/");
        let header = session.get_cookie_header(domain, path, now_ms());
        let data = serde_json::json!({
            "cookie_header": header,
        });
        ToolResult::ok(data, now_ms().saturating_sub(start))
    }
}

fn execute_token(req: &SessionRequest, start: u64) -> ToolResult {
    let mut session = match parse_session(&req.session_json, start) {
        Ok(s) => s,
        Err(e) => return e,
    };

    let access_token = match &req.access_token {
        Some(t) => t.as_str(),
        None => {
            return ToolResult::err(
                "'access_token' krävs för action=token",
                now_ms().saturating_sub(start),
            )
        }
    };

    let now = now_ms();
    let expires_in = req.expires_in.unwrap_or(3600);
    let scopes = req.scopes.clone().unwrap_or_default();

    session.set_oauth_token(
        access_token,
        req.refresh_token.as_deref(),
        expires_in,
        now,
        scopes,
    );

    let data = serde_json::json!({
        "session_json": session.to_json(),
        "token_set": true,
        "expires_in_secs": expires_in,
    });
    ToolResult::ok(data, now_ms().saturating_sub(start))
}

fn execute_oauth(req: &SessionRequest, start: u64) -> ToolResult {
    let mut session = match parse_session(&req.session_json, start) {
        Ok(s) => s,
        Err(e) => return e,
    };

    let config_json = match &req.oauth_config {
        Some(c) => c.as_str(),
        None => {
            return ToolResult::err(
                "'oauth_config' krävs för action=oauth",
                now_ms().saturating_sub(start),
            )
        }
    };

    let config: crate::session::OAuthConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult::err(
                format!("Ogiltig oauth_config: {e}"),
                now_ms().saturating_sub(start),
            )
        }
    };

    // Om code finns → token exchange, annars → authorize URL
    if let Some(ref code) = req.code {
        let params = session.prepare_token_exchange(&config, code);
        let data = serde_json::json!({
            "session_json": session.to_json(),
            "exchange_params": params,
        });
        ToolResult::ok(data, now_ms().saturating_sub(start))
    } else {
        let auth_result = session.build_authorize_url(&config);
        let data = serde_json::json!({
            "session_json": session.to_json(),
            "authorize_url": auth_result.authorize_url,
            "state": auth_result.state,
        });
        ToolResult::ok(data, now_ms().saturating_sub(start))
    }
}

fn execute_detect_login(req: &SessionRequest, start: u64) -> ToolResult {
    let html = match &req.html {
        Some(h) => h.as_str(),
        None => {
            return ToolResult::err(
                "'html' krävs för action=detect_login",
                now_ms().saturating_sub(start),
            )
        }
    };
    let goal = req.goal.as_deref().unwrap_or("logga in");
    let url = req.url.as_deref().unwrap_or("");

    let tree = build_tree(html, goal, url);
    let login_info = crate::session::SessionManager::detect_login_form(&tree);

    let data = match login_info {
        Some(info) => serde_json::json!({
            "login_form_found": true,
            "username_node_id": info.username_node_id,
            "password_node_id": info.password_node_id,
            "submit_node_id": info.submit_node_id,
        }),
        None => serde_json::json!({
            "login_form_found": false,
        }),
    };

    ToolResult::ok(data, now_ms().saturating_sub(start))
}

fn execute_refresh(req: &SessionRequest, start: u64) -> ToolResult {
    let session = match parse_session(&req.session_json, start) {
        Ok(s) => s,
        Err(e) => return e,
    };

    let config_json = match &req.oauth_config {
        Some(c) => c.as_str(),
        None => {
            return ToolResult::err(
                "'oauth_config' krävs för action=refresh",
                now_ms().saturating_sub(start),
            )
        }
    };

    let config: crate::session::OAuthConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult::err(
                format!("Ogiltig oauth_config: {e}"),
                now_ms().saturating_sub(start),
            )
        }
    };

    let params = session.prepare_token_refresh(&config);
    let data = serde_json::json!({
        "refresh_params": params,
        "has_refresh_token": params.is_some(),
    });
    ToolResult::ok(data, now_ms().saturating_sub(start))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_create() {
        let req = SessionRequest {
            action: "create".to_string(),
            session_json: None,
            domain: None,
            path: None,
            cookies: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Create ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert!(
            data["session_json"].is_string(),
            "Ska returnera session_json"
        );
        assert_eq!(data["status"], "created");
    }

    #[test]
    fn test_session_add_and_get_cookies() {
        // Skapa session
        let session = crate::session::SessionManager::new();
        let session_json = session.to_json();

        // Lägg till cookies
        let req = SessionRequest {
            action: "cookies".to_string(),
            session_json: Some(session_json),
            domain: Some("shop.se".to_string()),
            path: None,
            cookies: Some(vec!["session_id=abc123; Path=/".to_string()]),
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Add cookies ska lyckas");
        let data = result.data.unwrap();
        let updated_session = data["session_json"].as_str().unwrap();

        // Hämta cookies
        let req2 = SessionRequest {
            action: "cookies".to_string(),
            session_json: Some(updated_session.to_string()),
            domain: Some("shop.se".to_string()),
            path: Some("/".to_string()),
            cookies: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result2 = execute(&req2);
        assert!(result2.error.is_none(), "Get cookies ska lyckas");
        let data2 = result2.data.unwrap();
        let cookie_header = data2["cookie_header"].as_str().unwrap_or("");
        assert!(
            cookie_header.contains("session_id=abc123"),
            "Ska returnera cookie: {cookie_header}"
        );
    }

    #[test]
    fn test_session_status() {
        let session = crate::session::SessionManager::new();
        let req = SessionRequest {
            action: "status".to_string(),
            session_json: Some(session.to_json()),
            domain: None,
            path: None,
            cookies: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Status ska lyckas");
        let data = result.data.unwrap();
        assert!(
            !data["authenticated"].as_bool().unwrap_or(true),
            "Ny session ska inte vara authenticated"
        );
    }

    #[test]
    fn test_session_detect_login() {
        let html = r##"<html><body>
        <form action="/login">
            <input type="text" name="username" placeholder="Användarnamn">
            <input type="password" name="password" placeholder="Lösenord">
            <button type="submit">Logga in</button>
        </form>
        </body></html>"##;

        let req = SessionRequest {
            action: "detect_login".to_string(),
            session_json: None,
            domain: None,
            path: None,
            cookies: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: Some(html.to_string()),
            goal: Some("logga in".to_string()),
            url: None,
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Detect login ska lyckas");
        let data = result.data.unwrap();
        assert!(
            data["login_form_found"].as_bool().unwrap_or(false),
            "Ska hitta login-formulär"
        );
    }

    #[test]
    fn test_session_no_session_json() {
        let req = SessionRequest {
            action: "status".to_string(),
            session_json: None,
            domain: None,
            path: None,
            cookies: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_some(),
            "Status utan session_json ska ge fel"
        );
    }

    #[test]
    fn test_session_unknown_action() {
        let req = SessionRequest {
            action: "dance".to_string(),
            session_json: None,
            domain: None,
            path: None,
            cookies: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Okänd action ska ge fel");
    }

    #[test]
    fn test_session_set_token() {
        let session = crate::session::SessionManager::new();
        let req = SessionRequest {
            action: "token".to_string(),
            session_json: Some(session.to_json()),
            domain: None,
            path: None,
            cookies: None,
            access_token: Some("bearer_abc123".to_string()),
            refresh_token: Some("refresh_xyz".to_string()),
            expires_in: Some(3600),
            scopes: Some(vec!["read".to_string(), "write".to_string()]),
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Set token ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert!(data["token_set"].as_bool().unwrap_or(false));
    }

    #[test]
    fn test_session_evict() {
        let session = crate::session::SessionManager::new();
        let req = SessionRequest {
            action: "evict".to_string(),
            session_json: Some(session.to_json()),
            domain: None,
            path: None,
            cookies: None,
            access_token: None,
            refresh_token: None,
            expires_in: None,
            scopes: None,
            oauth_config: None,
            code: None,
            html: None,
            goal: None,
            url: None,
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Evict ska lyckas");
    }
}
