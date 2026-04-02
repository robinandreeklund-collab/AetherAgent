// Tool 3: stream — Adaptive DOM streaming med LLM-directives
//
// Ersätter: stream_parse, stream_parse_directive, fetch_stream_parse

use serde::Deserialize;

use super::{detect_input, now_ms, InputKind, ToolResult};

/// Request-parametrar för stream-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct StreamRequest {
    /// URL att fetcha
    #[serde(default)]
    pub url: Option<String>,
    /// Rå HTML
    #[serde(default)]
    pub html: Option<String>,
    /// Agentens mål (krävs)
    pub goal: String,
    /// Max noder i output
    #[serde(default = "default_max_nodes")]
    pub max_nodes: u32,
    /// Minsta relevans-tröskel
    #[serde(default = "default_min_relevance")]
    pub min_relevance: f32,
    /// Top-N noder
    #[serde(default)]
    pub top_n: Option<u32>,
    /// LLM-directives: ["expand(node_42)", "next_branch", "stop", "lower_threshold(0.1)"]
    #[serde(default)]
    pub directives: Vec<String>,
}

fn default_max_nodes() -> u32 {
    50
}

fn default_min_relevance() -> f32 {
    0.3
}

/// Kör stream-verktyget synkront
pub fn execute(req: &StreamRequest) -> ToolResult {
    let start = now_ms();

    let input = match detect_input(req.html.as_deref(), req.url.as_deref(), None) {
        Ok(i) => i,
        Err(e) => return ToolResult::err(e, now_ms().saturating_sub(start)),
    };

    match input {
        InputKind::Html(html) => execute_with_html(&html, req, req.url.as_deref().unwrap_or("")),
        InputKind::Url(_) => ToolResult::err(
            "URL-input kräver asynkron fetch. Använd HTTP/MCP-endpointen.",
            now_ms().saturating_sub(start),
        ),
        InputKind::Screenshot(_) => ToolResult::err(
            "stream stödjer inte screenshot-input.",
            now_ms().saturating_sub(start),
        ),
    }
}

/// Kör stream med redan hämtad HTML
pub fn execute_with_html(html: &str, req: &StreamRequest, url: &str) -> ToolResult {
    let start = now_ms();

    let config = crate::stream_engine::StreamParseConfig {
        chunk_size: req.top_n.unwrap_or(10) as usize,
        min_relevance: req.min_relevance,
        max_nodes: req.max_nodes as usize,
    };

    // Parsa directives
    let directives = parse_directives(&req.directives);

    let result = if directives.is_empty() {
        crate::stream_engine::stream_parse(html, &req.goal, url, config)
    } else {
        crate::stream_engine::stream_parse_with_directives(html, &req.goal, url, config, directives)
    };

    let warnings = result.injection_warnings.clone();
    let data = serde_json::to_value(&result)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms().saturating_sub(start)).with_warnings(warnings)
}

/// Parsa directive-strängar till Directive-enum
fn parse_directives(raw: &[String]) -> Vec<crate::stream_state::Directive> {
    raw.iter()
        .filter_map(|s| {
            let s = s.trim();
            if s == "stop" {
                Some(crate::stream_state::Directive::Stop)
            } else if s == "next_branch" {
                Some(crate::stream_state::Directive::NextBranch)
            } else if let Some(inner) = s.strip_prefix("expand(").and_then(|r| r.strip_suffix(')'))
            {
                inner
                    .trim()
                    .parse::<u32>()
                    .ok()
                    .map(|id| crate::stream_state::Directive::Expand { node_id: id })
            } else if let Some(inner) = s
                .strip_prefix("lower_threshold(")
                .and_then(|r| r.strip_suffix(')'))
            {
                inner
                    .trim()
                    .parse::<f32>()
                    .ok()
                    .map(|v| crate::stream_state::Directive::LowerThreshold { value: v })
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn large_html() -> String {
        let mut html = String::from("<html><body>");
        for i in 0..100 {
            html.push_str(&format!(
                r#"<div><h2>Sektion {i}</h2><p>Innehåll för sektion {i}</p>
                <a href="/link{i}">Länk {i}</a></div>"#
            ));
        }
        html.push_str("</body></html>");
        html
    }

    #[test]
    fn test_stream_basic() {
        let req = StreamRequest {
            html: Some(large_html()),
            url: None,
            goal: "hitta sektioner".to_string(),
            max_nodes: 20,
            min_relevance: 0.0,
            top_n: None,
            directives: vec![],
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Stream ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        let emitted = data["nodes_emitted"].as_u64().unwrap_or(0);
        assert!(
            emitted <= 20,
            "Ska begränsa till max_nodes=20, fick {emitted}"
        );
        let savings = data["token_savings_ratio"].as_f64().unwrap_or(0.0);
        assert!(savings > 0.0, "Ska ha token-besparing");
    }

    #[test]
    fn test_stream_with_directives() {
        let req = StreamRequest {
            html: Some(large_html()),
            url: None,
            goal: "hitta sektioner".to_string(),
            max_nodes: 50,
            min_relevance: 0.0,
            top_n: None,
            directives: vec![
                "lower_threshold(0.01)".to_string(),
                "next_branch".to_string(),
            ],
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Directive-stream ska lyckas: {:?}",
            result.error
        );
    }

    #[test]
    fn test_stream_stop_directive() {
        let req = StreamRequest {
            html: Some(large_html()),
            url: None,
            goal: "test".to_string(),
            max_nodes: 100,
            min_relevance: 0.0,
            top_n: None,
            directives: vec!["stop".to_string()],
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Stop-directive ska lyckas");
    }

    #[test]
    fn test_stream_no_input() {
        let req = StreamRequest {
            html: None,
            url: None,
            goal: "test".to_string(),
            max_nodes: 50,
            min_relevance: 0.3,
            top_n: None,
            directives: vec![],
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ingen input ska ge fel");
    }

    #[test]
    fn test_parse_directives_fn() {
        let raw = vec![
            "expand(42)".to_string(),
            "stop".to_string(),
            "next_branch".to_string(),
            "lower_threshold(0.15)".to_string(),
            "invalid".to_string(),
        ];
        let parsed = parse_directives(&raw);
        assert_eq!(parsed.len(), 4, "Ska parsa 4 av 5 directives");
    }

    #[test]
    fn test_stream_high_threshold() {
        let req = StreamRequest {
            html: Some(large_html()),
            url: None,
            goal: "specifik produkt XYZ-123".to_string(),
            max_nodes: 50,
            min_relevance: 0.9,
            top_n: None,
            directives: vec![],
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Hög tröskel ska lyckas");
        let data = result.data.unwrap();
        let emitted = data["nodes_emitted"].as_u64().unwrap_or(0);
        // Med hög tröskel borde mycket få noder emitteras
        assert!(emitted < 50, "Hög tröskel ska filtrera aggressivt");
    }
}
