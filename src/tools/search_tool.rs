// Tool 6: search — Webb-sökning via DuckDuckGo
//
// Ersätter: search, fetch_search, search_from_html, build_search_url

use serde::Deserialize;

use super::{now_ms, ToolResult};

/// Request-parametrar för search-verktyget
#[derive(Debug, Clone, Deserialize)]
pub struct SearchRequest {
    /// Sökfråga (krävs)
    pub query: String,
    /// Mål för relevansfiltrering
    #[serde(default)]
    pub goal: Option<String>,
    /// Antal resultat (default: 5)
    #[serde(default = "default_top_n")]
    pub top_n: u32,
    /// Deep-fetch: hämta och parsa varje resultat (default: true)
    #[serde(default = "default_true")]
    pub deep: bool,
    /// Max noder per resultat (vid deep)
    #[serde(default = "default_max_nodes")]
    pub max_nodes_per_result: u32,
    /// Streaming-läge
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_top_n() -> u32 {
    5
}

fn default_true() -> bool {
    true
}

fn default_max_nodes() -> u32 {
    10
}

/// Kör search synkront (bara URL-byggning + DDG HTML-parse)
/// Faktisk fetch sker asynkront i MCP/HTTP-servern
pub fn execute(req: &SearchRequest) -> ToolResult {
    let start = now_ms();

    // Bygg DDG-URL
    let ddg_url = crate::search::build_ddg_url(&req.query);

    let data = serde_json::json!({
        "action": "fetch_required",
        "search_url": ddg_url,
        "query": req.query,
        "goal": req.goal,
        "top_n": req.top_n,
        "deep": req.deep,
        "max_nodes_per_result": req.max_nodes_per_result,
    });

    ToolResult::ok(data, now_ms() - start)
}

/// Kör search med redan hämtad DDG HTML
pub fn execute_with_html(ddg_html: &str, req: &SearchRequest) -> ToolResult {
    let start = now_ms();

    let goal = req.goal.as_deref().unwrap_or(&req.query);
    let effective_goal = if goal.is_empty() {
        format!("hitta svar på: {}", req.query)
    } else {
        goal.to_string()
    };
    let effective_top_n = if req.top_n == 0 {
        3
    } else {
        (req.top_n as usize).min(10)
    };

    // Detektera DDG CAPTCHA
    if crate::search::is_ddg_captcha(ddg_html) {
        return ToolResult::err(
            "DuckDuckGo returnerade en CAPTCHA-sida istället för sökresultat. DDG blockerar bot-liknande requests. Prova igen senare eller använd en annan sökmotor.",
            now_ms() - start,
        );
    }

    let ddg_url = crate::search::build_ddg_url(&req.query);
    let tree = super::build_tree(ddg_html, &effective_goal, &ddg_url);
    let results = crate::search::extract_results(&tree.nodes, effective_top_n);

    let (direct_answer, direct_answer_confidence) = crate::search::detect_direct_answer(&results)
        .map(|(a, c)| (Some(a), c))
        .unwrap_or((None, 0.0));

    let search_result = crate::search::SearchResult {
        query: req.query.clone(),
        results,
        direct_answer,
        direct_answer_confidence,
        source_url: ddg_url,
        parse_ms: now_ms() - start,
        nodes_seen: tree.nodes.len(),
        nodes_emitted: tree.nodes.len(),
        deep: None,
        deep_fetch_ms: None,
    };

    let data = serde_json::to_value(&search_result)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms() - start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_builds_url() {
        let req = SearchRequest {
            query: "rust wasm tutorial".to_string(),
            goal: None,
            top_n: 5,
            deep: false,
            max_nodes_per_result: 10,
            stream: false,
        };
        let result = execute(&req);
        assert!(
            result.error.is_none(),
            "Search ska lyckas: {:?}",
            result.error
        );
        let data = result.data.unwrap();
        let url = data["search_url"].as_str().unwrap_or("");
        assert!(
            url.contains("duckduckgo.com"),
            "Ska generera DDG-URL, fick: {url}"
        );
        assert!(
            url.contains("rust+wasm+tutorial") || url.contains("rust%20wasm"),
            "Ska inkludera söktermen i URL"
        );
    }

    #[test]
    fn test_search_with_ddg_html() {
        // Minimal DDG-liknande HTML
        let ddg_html = r##"<html><body>
        <div class="results">
            <div class="result">
                <a class="result__a" href="https://example.com/rust">Rust WASM Guide</a>
                <a class="result__snippet">Learn how to build WASM apps with Rust</a>
            </div>
        </div>
        </body></html>"##;

        let req = SearchRequest {
            query: "rust wasm".to_string(),
            goal: Some("lär mig rust wasm".to_string()),
            top_n: 5,
            deep: false,
            max_nodes_per_result: 10,
            stream: false,
        };
        let result = execute_with_html(ddg_html, &req);
        assert!(
            result.error.is_none(),
            "DDG-parse ska lyckas: {:?}",
            result.error
        );
    }

    #[test]
    fn test_search_swedish_query() {
        let req = SearchRequest {
            query: "bästa hotell stockholm".to_string(),
            goal: Some("hitta hotell".to_string()),
            top_n: 3,
            deep: true,
            max_nodes_per_result: 5,
            stream: false,
        };
        let result = execute(&req);
        assert!(result.error.is_none(), "Svensk sökning ska lyckas");
        let data = result.data.unwrap();
        assert!(
            data["search_url"]
                .as_str()
                .unwrap_or("")
                .contains("duckduckgo"),
            "Ska bygga DDG-URL"
        );
    }
}
