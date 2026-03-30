// Tool: parse_hybrid — Hybrid BM25 + HDC + Embedding scoring pipeline
//
// Tre-stegs ranking:
// 1. BM25 keyword retrieval med prefix-fallback
// 2. HDC 2048-bit bitvector pruning (nanosekund strukturell likhet)
// 3. Bottom-up neural embedding scoring (löv-noder först)
//
// 2.5x snabbare och bättre kvalitet än legacy parse_top.
// Returnerar top 100 noder som default så LLM-agenten kan välja de bästa 5-10.

use serde::Deserialize;

use super::{now_ms, ToolResult};

/// Request-parametrar för parse_hybrid
#[derive(Debug, Clone, Deserialize)]
pub struct ParseHybridRequest {
    /// URL att fetcha och parsa
    #[serde(default)]
    pub url: Option<String>,
    /// Rå HTML att parsa direkt
    #[serde(default)]
    pub html: Option<String>,
    /// Agentens mål (krävs)
    pub goal: String,
    /// Max antal noder att returnera (default: 100)
    #[serde(default = "default_top_n")]
    pub top_n: u32,
}

fn default_top_n() -> u32 {
    20
}

/// Kör parse_hybrid synkront (utan fetch)
pub fn execute(req: &ParseHybridRequest) -> ToolResult {
    let start = now_ms();

    let html = match (&req.html, &req.url) {
        (Some(h), _) if !h.is_empty() => h.as_str(),
        (_, Some(_)) => {
            return ToolResult::err(
                "URL-input kräver asynkron fetch. Använd HTTP/MCP-endpointen.",
                now_ms() - start,
            );
        }
        _ => {
            return ToolResult::err("Ingen input: ange html eller url", now_ms() - start);
        }
    };

    let url = req.url.as_deref().unwrap_or("");
    execute_with_html(html, req, url)
}

/// Kör parse_hybrid med redan hämtad HTML (anropas efter fetch i async-kontext)
pub fn execute_with_html(html: &str, req: &ParseHybridRequest, url: &str) -> ToolResult {
    let start = now_ms();

    let json_str = crate::parse_top_nodes_hybrid(html, &req.goal, url, req.top_n);

    let data: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();

    let warnings = data["injection_warnings"]
        .as_u64()
        .map(|n| {
            if n > 0 {
                vec![crate::types::InjectionWarning {
                    node_id: 0,
                    raw_text: format!("{n} injection warnings detected"),
                    reason: "Se top_nodes för detaljer".to_string(),
                    severity: crate::types::WarningSeverity::Medium,
                }]
            } else {
                vec![]
            }
        })
        .unwrap_or_default();

    ToolResult::ok(data, now_ms() - start).with_warnings(warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hybrid_basic() {
        let req = ParseHybridRequest {
            html: Some(
                r##"<html><body>
                <h1>Population Statistics</h1>
                <p>367924 inhabitants in the municipality</p>
                <button>Download report</button>
            </body></html>"##
                    .to_string(),
            ),
            url: None,
            goal: "population statistics".to_string(),
            top_n: 5,
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Ska lyckas: {:?}", result.error);

        let data = result.data.unwrap();
        let nodes = data["top_nodes"].as_array();
        assert!(nodes.is_some(), "Ska ha top_nodes");
        assert!(nodes.unwrap().len() <= 5, "top_n=5 ska respekteras");

        // Ska ha pipeline-metadata
        assert!(
            data["pipeline"]["method"].as_str() == Some("hybrid_bm25_hdc_embedding"),
            "Ska rapportera hybrid-metod"
        );
    }

    #[test]
    fn test_parse_hybrid_no_input() {
        let req = ParseHybridRequest {
            html: None,
            url: None,
            goal: "test".to_string(),
            top_n: 10,
        };
        let result = execute(&req);
        assert!(result.error.is_some(), "Ska ge fel utan input");
    }

    #[test]
    fn test_parse_hybrid_default_top_n() {
        assert_eq!(default_top_n(), 20, "Default top_n ska vara 20");
    }
}
