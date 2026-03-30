// Tool 9: discover — Unified resource discovery
//
// Ersätter: discover_webmcp, detect_xhr_urls

use serde::Deserialize;

use super::{detect_input, now_ms, InputKind, ToolResult};

/// Request-parametrar för discover-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct DiscoverRequest {
    /// URL att fetcha
    #[serde(default)]
    pub url: Option<String>,
    /// Rå HTML
    #[serde(default)]
    pub html: Option<String>,
    /// Mode: "all" (default), "webmcp", "xhr"
    #[serde(default = "default_mode")]
    pub mode: String,
}

fn default_mode() -> String {
    "all".to_string()
}

/// Kör discover synkront
pub fn execute(req: &DiscoverRequest) -> ToolResult {
    let start = now_ms();

    let input = match detect_input(req.html.as_deref(), req.url.as_deref(), None) {
        Ok(i) => i,
        Err(e) => return ToolResult::err(e, now_ms() - start),
    };

    match input {
        InputKind::Html(html) => execute_with_html(&html, req),
        InputKind::Url(_) => ToolResult::err(
            "URL-input kräver asynkron fetch. Använd HTTP/MCP-endpointen.",
            now_ms() - start,
        ),
        InputKind::Screenshot(_) => {
            ToolResult::err("discover stödjer inte screenshot-input.", now_ms() - start)
        }
    }
}

/// Kör discover med redan hämtad HTML
pub fn execute_with_html(html: &str, req: &DiscoverRequest) -> ToolResult {
    let start = now_ms();
    let url = req.url.as_deref().unwrap_or("");

    match req.mode.as_str() {
        "all" => {
            let webmcp = crate::webmcp::discover_webmcp_tools(html, url);
            let xhr_captures = crate::js_bridge::extract_xhr_from_snippets(html);

            let data = serde_json::json!({
                "mode": "all",
                "webmcp": {
                    "has_webmcp": webmcp.has_webmcp,
                    "tools": webmcp.tools,
                    "scripts_scanned": webmcp.scripts_scanned,
                },
                "xhr": {
                    "captures": xhr_captures,
                    "count": xhr_captures.len(),
                },
            });
            ToolResult::ok(data, now_ms() - start)
        }
        "webmcp" => {
            let result = crate::webmcp::discover_webmcp_tools(html, url);
            let data = serde_json::to_value(&result)
                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            ToolResult::ok(data, now_ms() - start)
        }
        "xhr" => {
            let captures = crate::js_bridge::extract_xhr_from_snippets(html);
            let data = serde_json::json!({
                "mode": "xhr",
                "captures": captures,
                "count": captures.len(),
            });
            ToolResult::ok(data, now_ms() - start)
        }
        other => ToolResult::err(
            format!("Okänt mode: '{other}'. Använd 'all', 'webmcp', eller 'xhr'."),
            now_ms() - start,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spa_html() -> &'static str {
        r##"<html><head>
        <script>
            fetch('/api/products').then(r => r.json());
            const xhr = new XMLHttpRequest();
            xhr.open('GET', '/api/user');
        </script>
        <script type="application/mcp+json">
        {
            "tools": [
                {"name": "get_products", "description": "Hämta alla produkter"}
            ]
        }
        </script>
        </head><body><div id="app"></div></body></html>"##
    }

    #[test]
    fn test_discover_all() {
        let req = DiscoverRequest {
            html: Some(spa_html().to_string()),
            url: None,
            mode: "all".to_string(),
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Discover ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert_eq!(data["mode"], "all");
        // WebMCP
        assert!(
            data["webmcp"]["has_webmcp"].as_bool().unwrap_or(false),
            "Ska hitta WebMCP-tools"
        );
        // XHR
        let xhr_count = data["xhr"]["count"].as_u64().unwrap_or(0);
        assert!(xhr_count > 0, "Ska hitta XHR-URLs, fick {xhr_count}");
    }

    #[test]
    fn test_discover_webmcp_only() {
        let req = DiscoverRequest {
            html: Some(spa_html().to_string()),
            url: None,
            mode: "webmcp".to_string(),
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "WebMCP-discover ska lyckas");
    }

    #[test]
    fn test_discover_xhr_only() {
        let req = DiscoverRequest {
            html: Some(spa_html().to_string()),
            url: None,
            mode: "xhr".to_string(),
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "XHR-discover ska lyckas");
        let data = result.data.unwrap();
        let count = data["count"].as_u64().unwrap_or(0);
        assert!(count > 0, "Ska hitta XHR-URLs");
    }

    #[test]
    fn test_discover_no_input() {
        let req = DiscoverRequest {
            html: None,
            url: None,
            mode: "all".to_string(),
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ingen input ska ge fel");
    }

    #[test]
    fn test_discover_unknown_mode() {
        let req = DiscoverRequest {
            html: Some("<html></html>".to_string()),
            url: None,
            mode: "quantum".to_string(),
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Okänt mode ska ge fel");
    }

    #[test]
    fn test_discover_empty_page() {
        let req = DiscoverRequest {
            html: Some("<html><body></body></html>".to_string()),
            url: None,
            mode: "all".to_string(),
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Tom sida ska lyckas");
        let data = result.data.unwrap();
        assert!(
            !data["webmcp"]["has_webmcp"].as_bool().unwrap_or(true),
            "Tom sida ska inte ha WebMCP"
        );
    }
}
