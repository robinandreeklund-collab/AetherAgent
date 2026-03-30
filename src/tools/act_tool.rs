// Tool 2: act — Unified interaction tool
//
// Ersätter: find_and_click, fill_form, extract_data, fetch_click, fetch_extract

use serde::Deserialize;
use std::collections::HashMap;

use super::{build_tree, detect_input, now_ms, InputKind, ToolResult};

/// Request-parametrar för act-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct ActRequest {
    /// URL att fetcha
    #[serde(default)]
    pub url: Option<String>,
    /// Rå HTML
    #[serde(default)]
    pub html: Option<String>,
    /// Agentens mål (krävs)
    pub goal: String,
    /// Åtgärd: "click", "fill", "extract" (krävs)
    pub action: String,
    /// Label/text att klicka (för click)
    #[serde(default)]
    pub target: Option<String>,
    /// Fält att fylla i (för fill)
    #[serde(default)]
    pub fields: Option<HashMap<String, String>>,
    /// Nycklar att extrahera (för extract)
    #[serde(default)]
    pub keys: Option<Vec<String>>,
    /// Streaming-läge
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_true() -> bool {
    true
}

/// Kör act-verktyget synkront
pub fn execute(req: &ActRequest) -> ToolResult {
    let start = now_ms();

    let input = match detect_input(req.html.as_deref(), req.url.as_deref(), None) {
        Ok(i) => i,
        Err(e) => return ToolResult::err(e, now_ms() - start),
    };

    match input {
        InputKind::Html(html) => execute_with_html(&html, req, req.url.as_deref().unwrap_or("")),
        InputKind::Url(_) => ToolResult::err(
            "URL-input kräver asynkron fetch. Använd HTTP/MCP-endpointen.",
            now_ms() - start,
        ),
        InputKind::Screenshot(_) => ToolResult::err(
            "act stödjer inte screenshot-input. Använd parse eller vision istället.",
            now_ms() - start,
        ),
    }
}

/// Kör act med redan hämtad HTML
pub fn execute_with_html(html: &str, req: &ActRequest, url: &str) -> ToolResult {
    let start = now_ms();

    let tree = build_tree(html, &req.goal, url);
    let warnings = tree.injection_warnings.clone();

    match req.action.as_str() {
        "click" => {
            let target = match &req.target {
                Some(t) => t.as_str(),
                None => {
                    return ToolResult::err("'target' krävs för action=click", now_ms() - start)
                }
            };
            let result = crate::intent::find_best_clickable(&tree, target);
            let data = serde_json::to_value(&result)
                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            ToolResult::ok(data, now_ms() - start).with_warnings(warnings)
        }
        "fill" => {
            let fields = match &req.fields {
                Some(f) => f.clone(),
                None => return ToolResult::err("'fields' krävs för action=fill", now_ms() - start),
            };
            let result = crate::intent::map_form_fields(&tree, &fields);
            let data = serde_json::to_value(&result)
                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            ToolResult::ok(data, now_ms() - start).with_warnings(warnings)
        }
        "extract" => {
            let keys = match &req.keys {
                Some(k) => k.clone(),
                None => {
                    return ToolResult::err("'keys' krävs för action=extract", now_ms() - start)
                }
            };
            let result = crate::intent::extract_by_keys(&tree, &keys);
            let data = serde_json::to_value(&result)
                .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
            ToolResult::ok(data, now_ms() - start).with_warnings(warnings)
        }
        other => ToolResult::err(
            format!("Okänd action: '{other}'. Använd 'click', 'fill', eller 'extract'."),
            now_ms() - start,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form_html() -> &'static str {
        r##"<html><body>
        <h1>Logga in</h1>
        <form>
            <label for="email">E-post</label>
            <input id="email" type="email" name="email">
            <label for="pass">Lösenord</label>
            <input id="pass" type="password" name="password">
            <button type="submit">Logga in</button>
        </form>
        <a href="/register">Registrera</a>
        <p>Pris: 299 kr</p>
        </body></html>"##
    }

    #[test]
    fn test_act_click() {
        let req = ActRequest {
            html: Some(form_html().to_string()),
            url: None,
            goal: "logga in".to_string(),
            action: "click".to_string(),
            target: Some("Logga in".to_string()),
            fields: None,
            keys: None,
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Click ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        assert!(
            data["found"].as_bool().unwrap_or(false),
            "Ska hitta knappen"
        );
    }

    #[test]
    fn test_act_fill() {
        let mut fields = HashMap::new();
        fields.insert("email".to_string(), "test@test.se".to_string());
        fields.insert("password".to_string(), "secret".to_string());

        let req = ActRequest {
            html: Some(form_html().to_string()),
            url: None,
            goal: "logga in".to_string(),
            action: "fill".to_string(),
            target: None,
            fields: Some(fields),
            keys: None,
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Fill ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        let mappings = data["mappings"].as_array();
        assert!(
            mappings.map(|m| !m.is_empty()).unwrap_or(false),
            "Ska mappa formulärfält"
        );
    }

    #[test]
    fn test_act_extract() {
        let req = ActRequest {
            html: Some(form_html().to_string()),
            url: None,
            goal: "hitta pris".to_string(),
            action: "extract".to_string(),
            target: None,
            fields: None,
            keys: Some(vec!["pris".to_string()]),
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Extract ska lyckas: {:?}",
            result.error
        );
    }

    #[test]
    fn test_act_missing_target() {
        let req = ActRequest {
            html: Some(form_html().to_string()),
            url: None,
            goal: "test".to_string(),
            action: "click".to_string(),
            target: None,
            fields: None,
            keys: None,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Click utan target ska ge fel");
    }

    #[test]
    fn test_act_missing_fields() {
        let req = ActRequest {
            html: Some(form_html().to_string()),
            url: None,
            goal: "test".to_string(),
            action: "fill".to_string(),
            target: None,
            fields: None,
            keys: None,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Fill utan fields ska ge fel");
    }

    #[test]
    fn test_act_unknown_action() {
        let req = ActRequest {
            html: Some(form_html().to_string()),
            url: None,
            goal: "test".to_string(),
            action: "dance".to_string(),
            target: None,
            fields: None,
            keys: None,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Okänd action ska ge fel");
    }

    #[test]
    fn test_act_no_input() {
        let req = ActRequest {
            html: None,
            url: None,
            goal: "test".to_string(),
            action: "click".to_string(),
            target: Some("btn".to_string()),
            fields: None,
            keys: None,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ingen input ska ge fel");
    }
}
