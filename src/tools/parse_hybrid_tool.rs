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

/// Kör parse_hybrid med redan hämtad HTML (synkron — utan pending fetch resolution)
pub fn execute_with_html(html: &str, req: &ParseHybridRequest, url: &str) -> ToolResult {
    let start = now_ms();
    let json_str = crate::parse_top_nodes_hybrid(html, &req.goal, url, req.top_n);
    let data: serde_json::Value = serde_json::from_str(&json_str).unwrap_or_default();
    result_from_json(data, start)
}

/// Async variant — resolvar pending fetch-URLs innan hybrid scoring (BUGG J).
///
/// Flöde: build_tree_with_js → resolve_pending_fetches → hybrid score → top_n
#[cfg(feature = "fetch")]
pub async fn execute_with_html_async(
    html: &str,
    req: &ParseHybridRequest,
    url: &str,
) -> ToolResult {
    let start = now_ms();

    // Steg 1: Bygg träd med JS-eval (samlar pending fetch-URLs)
    let mut tree = super::build_tree_with_js(html, &req.goal, url);
    tree.parse_time_ms = now_ms() - start;

    // Steg 2: Resolve pending fetch-URLs (async — hämtar via reqwest)
    super::resolve_pending_fetches(&mut tree, &req.goal).await;

    // Steg 3: Kör hybrid scoring på det kompletta trädet
    let goal_embedding = crate::embedding::embed(&req.goal);
    let config = crate::scoring::PipelineConfig::default();
    let pipeline_result = crate::scoring::ScoringPipeline::run_cached(
        html,
        &tree.nodes,
        &req.goal,
        goal_embedding.as_deref(),
        &config,
    );

    // Applicera scores
    let score_map = crate::scoring::pipeline::scores_to_map(&pipeline_result.scored_nodes);
    crate::scoring::pipeline::apply_scores_to_tree(&mut tree.nodes, &score_map);

    // Top-N
    let top_scored = crate::scoring::ScoringPipeline::apply_top_n(
        pipeline_result.scored_nodes,
        Some(req.top_n as usize),
    );

    let timings = &pipeline_result.timings;
    let data = serde_json::json!({
        "url": tree.url,
        "title": tree.title,
        "goal": tree.goal,
        "top_nodes": top_scored.iter().map(|s| {
            serde_json::json!({
                "id": s.id,
                "role": s.role,
                "label": s.label,
                "relevance": s.relevance,
            })
        }).collect::<Vec<_>>(),
        "node_count": top_scored.len(),
        "total_nodes": super::count_all_nodes(&tree.nodes),
        "injection_warnings": tree.injection_warnings.len(),
        "xhr_intercepted": tree.xhr_intercepted,
        "xhr_blocked": tree.xhr_blocked,
        "parse_time_ms": tree.parse_time_ms,
        "pipeline": {
            "method": "hybrid_bm25_hdc_embedding",
            "bm25_candidates": timings.tfidf_candidates,
            "hdc_survivors": timings.hdc_survivors,
            "total_pipeline_us": timings.total_us,
            "cache_hit": timings.cache_hit,
            "pending_urls_resolved": tree.pending_fetch_urls.is_empty(),
        }
    });

    result_from_json(data, start)
}

fn result_from_json(data: serde_json::Value, start: u64) -> ToolResult {
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
