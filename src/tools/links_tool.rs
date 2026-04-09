// Tool: extract_links — Rich Link Extraction with Metadata
//
// Extraherar alla links från en sida med relevance scoring,
// HDC novelty, structural role och context snippets.

use serde::Deserialize;

use super::{detect_input, now_ms, InputKind, ToolResult};

/// Request-parametrar för extract_links
#[derive(Debug, Clone, Deserialize)]
pub struct LinksRequest {
    /// URL att fetcha
    #[serde(default)]
    pub url: Option<String>,
    /// Rå HTML
    #[serde(default)]
    pub html: Option<String>,
    /// Mål för relevance scoring
    #[serde(default)]
    pub goal: Option<String>,
    /// Max antal links
    #[serde(default = "default_max_links")]
    pub max_links: usize,
    /// Inkludera context snippet
    #[serde(default = "default_true")]
    pub include_context: bool,
    /// Filtrera bort navigation-links
    #[serde(default)]
    pub filter_navigation: bool,
    /// Minimum relevance
    #[serde(default)]
    pub min_relevance: f32,
}

fn default_max_links() -> usize {
    50
}
fn default_true() -> bool {
    true
}

/// Kör extract_links synkront (med HTML)
pub fn execute(req: &LinksRequest) -> ToolResult {
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
            "extract_links stödjer inte screenshot-input.",
            now_ms().saturating_sub(start),
        ),
    }
}

/// Kör extract_links med redan hämtad HTML
pub fn execute_with_html(html: &str, req: &LinksRequest, url: &str) -> ToolResult {
    let start = now_ms();

    // Parsa HTML → semantiskt träd
    let tree = crate::parse_to_semantic_tree(html, req.goal.as_deref().unwrap_or(""), url);
    let tree_nodes: Vec<crate::types::SemanticNode> = {
        #[derive(serde::Deserialize)]
        struct TreeOutput {
            #[serde(default)]
            nodes: Vec<crate::types::SemanticNode>,
        }
        serde_json::from_str::<TreeOutput>(&tree)
            .map(|t| t.nodes)
            .unwrap_or_default()
    };

    // Konfigurera link extraction
    let config = crate::link_extract::LinkExtractionConfig {
        goal: req.goal.clone(),
        max_links: req.max_links,
        include_context: req.include_context,
        include_structural_role: true,
        filter_navigation: req.filter_navigation,
        min_relevance: req.min_relevance,
        ..Default::default()
    };

    let result = crate::link_extract::extract_links_from_tree(&tree_nodes, url, &config, None);

    let data = serde_json::to_value(&result)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms().saturating_sub(start))
}
