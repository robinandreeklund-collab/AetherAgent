/// AetherAgent – LLM-native browser engine
///
/// Publik WASM-API som exponeras till Python, Node.js och edge-runtimes.
mod parser;
mod semantic;
mod trust;
mod types;

use std::time::{SystemTime, UNIX_EPOCH};
use wasm_bindgen::prelude::*;

use parser::parse_html;
use semantic::{extract_title, SemanticBuilder};

// --- Publik API ---

/// Parsa HTML till ett semantiskt träd med goal-relevance scoring
///
/// # Arguments
/// * `html` - Rå HTML-sträng från webbsidan
/// * `goal` - Agentens nuvarande mål (t.ex. "köp billigaste flyg")
/// * `url` - Sidans URL (för kontext)
///
/// # Returns
/// JSON-sträng med SemanticTree, redo att skickas till LLM:en
#[wasm_bindgen]
pub fn parse_to_semantic_tree(html: &str, goal: &str, url: &str) -> String {
    let start = now_ms();

    let dom = parse_html(html);
    let title = extract_title(&dom);

    let mut builder = SemanticBuilder::new(goal);
    let mut tree = builder.build(&dom, url, &title);

    tree.parse_time_ms = now_ms() - start;

    // Sortera noder efter relevance (högst först) för LLM-effektivitet
    tree.nodes
        .sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());

    match serde_json::to_string_pretty(&tree) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Snabbversion – returnerar bara de mest relevanta noderna
/// Perfekt för snabba agent-beslut utan att fylla hela kontextfönstret
///
/// # Arguments
/// * `html` - Rå HTML-sträng
/// * `goal` - Agentens mål
/// * `url` - Sidans URL
/// * `top_n` - Max antal noder att returnera (rekommenderat: 10-20)
#[wasm_bindgen]
pub fn parse_top_nodes(html: &str, goal: &str, url: &str, top_n: u32) -> String {
    let start = now_ms();
    let dom = parse_html(html);
    let title = extract_title(&dom);

    let mut builder = SemanticBuilder::new(goal);
    let mut tree = builder.build(&dom, url, &title);

    tree.parse_time_ms = now_ms() - start;

    // Samla alla noder platt och sortera
    let mut all_nodes = collect_all_nodes(&tree.nodes);
    all_nodes.sort_by(|a, b| b.relevance.partial_cmp(&a.relevance).unwrap());

    // Ta topp-N
    let top: Vec<_> = all_nodes
        .into_iter()
        .take(top_n as usize)
        .cloned()
        .collect();

    // Bygg ett förenklat svar
    let result = serde_json::json!({
        "url": tree.url,
        "title": tree.title,
        "goal": tree.goal,
        "top_nodes": top,
        "injection_warnings": tree.injection_warnings.len(),
        "parse_time_ms": tree.parse_time_ms,
    });

    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Analysera ett textstycke för prompt injection
/// Kan användas separat för extra skydd i agent-loopar
#[wasm_bindgen]
pub fn check_injection(text: &str) -> String {
    let (_, warning) = trust::analyze_text(0, text);
    if let Some(w) = warning {
        serde_json::to_string_pretty(&w).unwrap_or_else(|_| "{}".to_string())
    } else {
        r#"{"safe": true}"#.to_string()
    }
}

/// Wrappa text i content-boundary markers för säker LLM-konsumption
#[wasm_bindgen]
pub fn wrap_untrusted(content: &str) -> String {
    trust::wrap_untrusted(content)
}

/// Sanitetskontroll – verifiera att WASM-modulen laddats korrekt
#[wasm_bindgen]
pub fn health_check() -> String {
    r#"{"status": "ok", "version": "0.1.0", "engine": "AetherAgent"}"#.to_string()
}

// --- Internals ---

fn collect_all_nodes(nodes: &[types::SemanticNode]) -> Vec<&types::SemanticNode> {
    let mut result = vec![];
    for node in nodes {
        result.push(node);
        result.extend(collect_all_nodes(&node.children));
    }
    result
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// --- WASM-tester ---

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check() {
        let result = health_check();
        assert!(result.contains("ok"));
    }

    #[test]
    fn test_parse_returns_valid_json() {
        let html = r#"<html>
            <head><title>Test Shop</title></head>
            <body>
                <button>Lägg i varukorg</button>
                <a href="/checkout">Till kassan</a>
                <input type="text" placeholder="Sök produkter..." />
            </body>
        </html>"#;

        let result = parse_to_semantic_tree(html, "lägg i varukorg", "https://test.com");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Ska vara valid JSON");

        assert!(parsed["nodes"].is_array());
        assert_eq!(parsed["goal"], "lägg i varukorg");
    }

    #[test]
    fn test_top_nodes_respects_limit() {
        let html = r#"<html><body>
            <button>Knapp 1</button>
            <button>Knapp 2</button>
            <button>Knapp 3</button>
            <button>Knapp 4</button>
            <button>Knapp 5</button>
        </body></html>"#;

        let result = parse_top_nodes(html, "klicka knapp", "https://test.com", 3);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Ska vara valid JSON");

        assert!(parsed["top_nodes"].as_array().unwrap().len() <= 3);
    }
}
