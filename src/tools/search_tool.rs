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
    /// Scoring-metod: "hybrid" (BM25+HDC+Embedding) eller "legacy" (enkel relevans)
    #[serde(default = "default_scoring")]
    pub scoring: String,
    /// Streaming-läge
    #[serde(default = "default_true")]
    pub stream: bool,
}

fn default_scoring() -> String {
    "hybrid".to_string()
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

    ToolResult::ok(data, now_ms().saturating_sub(start))
}

/// Kör search med redan hämtad DDG HTML
pub fn execute_with_html(ddg_html: &str, req: &SearchRequest) -> ToolResult {
    let start = now_ms();
    let (search_result, _results_for_deep) = parse_ddg_results(ddg_html, req, start);

    let data = serde_json::to_value(&search_result)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms().saturating_sub(start))
}

/// Async variant: extrahera DDG-resultat + hybrid_parse top-3 resultat-sidor
#[cfg(feature = "fetch")]
pub async fn execute_with_html_async(ddg_html: &str, req: &SearchRequest) -> ToolResult {
    let start = now_ms();
    let (mut search_result, results_for_deep) = parse_ddg_results(ddg_html, req, start);

    // Deep fetch: hybrid_parse top-3 resultat-sidor
    let goal = req.goal.as_deref().unwrap_or(&req.query);
    let max_nodes = req.max_nodes_per_result as usize;
    let deep_start = now_ms();

    // BUGG R: Per-URL timeout cap (3s) + Wikipedia top_n begränsning
    const DEEP_FETCH_TIMEOUT_MS: u64 = 3000;

    let top_n_deep = results_for_deep.len().min(3);
    for (i, url) in results_for_deep.iter().enumerate().take(top_n_deep) {
        let fetch_start = now_ms();

        // Firewall-check
        if super::firewall_check(url, goal).is_some() {
            continue;
        }

        // BUGG R: Timeout-skyddad fetch (3s per URL)
        let config = crate::types::FetchConfig::default();
        let fetch_fut = crate::fetch::fetch_page(url, &config);
        let fetched = match tokio::time::timeout(
            std::time::Duration::from_millis(DEEP_FETCH_TIMEOUT_MS),
            fetch_fut,
        )
        .await
        {
            Ok(Ok(r)) => r,
            _ => continue, // Timeout eller fetch-fel → skippa
        };

        // BUGG R: Kolla elapsed — skippa om vi redan överskridit timeout
        if now_ms().saturating_sub(fetch_start) > DEEP_FETCH_TIMEOUT_MS + 500 {
            continue;
        }

        // BUGG R: Wikipedia/stora sidor → begränsa top_n för att hålla latens
        let effective_top_n = if url.contains("wikipedia.org") || fetched.body.len() > 200_000 {
            max_nodes.min(10) // Max 10 noder från stora sidor
        } else {
            max_nodes
        };

        // Kör parse med vald metod
        let html = fetched.body;
        let url_clone = url.clone();
        let goal_clone = goal.to_string();
        let scoring = req.scoring.clone();
        let nodes = tokio::task::spawn_blocking(move || {
            if scoring == "full_markdown" {
                // Komplett DOM som markdown — inga rankade noder
                let md = crate::html_to_markdown(&html, &goal_clone, &url_clone);
                // Wrappa markdown som en enda PageNode
                vec![crate::search::PageNode {
                    role: "markdown".to_string(),
                    label: md.chars().take(2000).collect(),
                    relevance: 1.0,
                }]
            } else {
                // CRFR scoring — resonansfält med BM25+HDC+wave (snabbare, inget ONNX)
                let json = crate::parse_crfr(
                    &html,
                    &goal_clone,
                    &url_clone,
                    effective_top_n as u32,
                    false,
                    "json",
                );
                let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
                parsed["nodes"]
                    .as_array()
                    .map(|arr| {
                        arr.iter()
                            .map(|n| crate::search::PageNode {
                                role: n["role"].as_str().unwrap_or("").to_string(),
                                label: n["label"].as_str().unwrap_or("").to_string(),
                                relevance: n["relevance"].as_f64().unwrap_or(0.0) as f32,
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
            }
        })
        .await
        .unwrap_or_default();

        let fetch_ms = now_ms().saturating_sub(fetch_start);

        // Berika sökresultatet
        if i < search_result.results.len() && !nodes.is_empty() {
            search_result.results[i].page_content = Some(nodes);
            search_result.results[i].fetch_ms = Some(fetch_ms);
        }
    }

    search_result.deep = Some(true);
    search_result.deep_fetch_ms = Some(now_ms().saturating_sub(deep_start));

    let data = serde_json::to_value(&search_result)
        .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));

    ToolResult::ok(data, now_ms().saturating_sub(start))
}

/// Gemensam DDG-parsningslogik
fn parse_ddg_results(
    ddg_html: &str,
    req: &SearchRequest,
    start: u64,
) -> (crate::search::SearchResult, Vec<String>) {
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

    if crate::search::is_ddg_captcha(ddg_html) {
        return (
            crate::search::SearchResult {
                query: req.query.clone(),
                results: vec![],
                direct_answer: None,
                direct_answer_confidence: 0.0,
                source_url: crate::search::build_ddg_url(&req.query),
                parse_ms: now_ms().saturating_sub(start),
                nodes_seen: 0,
                nodes_emitted: 0,
                deep: None,
                deep_fetch_ms: None,
            },
            vec![],
        );
    }

    let ddg_url = crate::search::build_ddg_url(&req.query);
    let tree = super::build_tree(ddg_html, &effective_goal, &ddg_url);
    let results = crate::search::extract_results(&tree.nodes, effective_top_n);

    let urls_for_deep: Vec<String> = results.iter().map(|r| r.url.clone()).collect();

    let (direct_answer, direct_answer_confidence) = crate::search::detect_direct_answer(&results)
        .map(|(a, c)| (Some(a), c))
        .unwrap_or((None, 0.0));

    let search_result = crate::search::SearchResult {
        query: req.query.clone(),
        results,
        direct_answer,
        direct_answer_confidence,
        source_url: ddg_url,
        parse_ms: now_ms().saturating_sub(start),
        nodes_seen: tree.nodes.len(),
        nodes_emitted: tree.nodes.len(),
        deep: None,
        deep_fetch_ms: None,
    };

    (search_result, urls_for_deep)
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
            scoring: "hybrid".to_string(),
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
            scoring: "hybrid".to_string(),
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
            scoring: "hybrid".to_string(),
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
