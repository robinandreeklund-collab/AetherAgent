// Fas 10: XHR Network Interception
// Fångar fetch()/XHR-anrop från JS-sandbox och hämtar dem genom Semantic Firewall

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::types::{InjectionWarning, SemanticNode, SemanticTree, TrustLevel};

// ─── Typer ──────────────────────────────────────────────────────────────────

/// A captured XHR/fetch request from JS analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XhrCapture {
    /// Target URL for the XHR/fetch call
    pub url: String,
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// Request headers
    pub headers: HashMap<String, String>,
}

/// Result of XHR interception and fetching
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct XhrInterceptResult {
    /// Semantic nodes created from XHR responses
    pub captures: Vec<XhrCapture>,
    /// Noder skapade från XHR-svar
    pub nodes: Vec<SemanticNode>,
    /// Antal XHR-anrop som hämtades
    pub intercepted_count: u32,
    /// Antal XHR-anrop som blockerades av Semantic Firewall
    pub blocked_count: u32,
    /// Injection-varningar från XHR-svar
    pub xhr_injection_warnings: Vec<InjectionWarning>,
}

/// Configuration for XHR interception (opt-in, disabled by default)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterceptConfig {
    /// Aktivera XHR-interception
    #[serde(default)]
    pub enabled: bool,
    /// Max antal XHR-anrop att följa
    #[serde(default = "InterceptConfig::default_max_requests")]
    pub max_requests: usize,
    /// Timeout per XHR-anrop i millisekunder
    #[serde(default = "InterceptConfig::default_timeout_ms")]
    pub timeout_ms: u64,
}

impl InterceptConfig {
    fn default_max_requests() -> usize {
        10
    }

    fn default_timeout_ms() -> u64 {
        2000
    }
}

impl Default for InterceptConfig {
    fn default() -> Self {
        InterceptConfig {
            enabled: false,
            max_requests: Self::default_max_requests(),
            timeout_ms: Self::default_timeout_ms(),
        }
    }
}

// ─── Prisfält att söka i JSON ───────────────────────────────────────────────

/// Fältnamn som kan innehålla prisinformation
const PRICE_FIELDS: &[&str] = &["price", "amount", "cost", "total", "pris", "belopp"];

// ─── Async interception (kräver fetch-feature) ──────────────────────────────

/// Intercept XHR captures: filter through firewall, fetch allowed URLs, parse responses
#[cfg(feature = "fetch")]
pub async fn intercept_xhr(
    captures: &[XhrCapture],
    goal: &str,
    config: &InterceptConfig,
    firewall_config: &crate::firewall::FirewallConfig,
) -> XhrInterceptResult {
    use crate::firewall;
    use crate::trust;

    let mut nodes = Vec::new();
    let mut intercepted_count = 0u32;
    let mut blocked_count = 0u32;
    let mut warnings = Vec::new();
    let mut node_id_counter = 10_000u32; // Högt startID för att undvika kollision

    // Begränsa antal requests
    let max = config.max_requests.min(captures.len());

    for capture in captures.iter().take(max) {
        // Kör igenom firewall
        let verdict = firewall::classify_request(&capture.url, goal, firewall_config);
        if !verdict.allowed {
            blocked_count += 1;
            continue;
        }

        // Hämta URL:en via reqwest
        let client = match reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(config.timeout_ms))
            .build()
        {
            Ok(c) => c,
            Err(_) => continue,
        };

        let response = match client.get(&capture.url).send().await {
            Ok(r) => r,
            Err(_) => continue,
        };

        let body = match response.text().await {
            Ok(b) => b,
            Err(_) => continue,
        };

        intercepted_count += 1;

        // Analysera svarskroppen för injection
        let (_, warning) = trust::analyze_text(node_id_counter, &body);
        if let Some(w) = warning {
            warnings.push(w);
        }

        // Beräkna relevans från firewall-verdict
        let relevance = verdict.relevance_score.unwrap_or(0.5);

        // Skapa semantisk nod
        let node = normalize_xhr_to_node(&body, &capture.url, relevance, node_id_counter);
        nodes.push(node);
        node_id_counter += 1;
    }

    XhrInterceptResult {
        captures: captures.to_vec(),
        nodes,
        intercepted_count,
        blocked_count,
        xhr_injection_warnings: warnings,
    }
}

/// Extract a price value from a JSON object by searching recursively for price-like fields
pub fn extract_price_from_json(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            // Sök i detta objekt efter prisfält
            for (key, val) in map {
                let lower_key = key.to_lowercase();
                if PRICE_FIELDS.iter().any(|f| lower_key == *f) {
                    return match val {
                        serde_json::Value::String(s) => Some(s.clone()),
                        serde_json::Value::Number(n) => Some(n.to_string()),
                        _ => Some(val.to_string()),
                    };
                }
            }
            // Rekursiv sökning i nästlade objekt
            for (_key, val) in map {
                if let Some(price) = extract_price_from_json(val) {
                    return Some(price);
                }
            }
            None
        }
        serde_json::Value::Array(arr) => {
            // Sök i första elementet
            for item in arr {
                if let Some(price) = extract_price_from_json(item) {
                    return Some(price);
                }
            }
            None
        }
        _ => None,
    }
}

/// Create a SemanticNode from an XHR response body
///
/// If the body is JSON containing a price field, the node gets role "price".
/// Otherwise it gets role "data" with truncated body content as label.
pub fn normalize_xhr_to_node(body: &str, url: &str, relevance: f32, node_id: u32) -> SemanticNode {
    // Försök tolka som JSON och hitta pris
    if let Ok(json) = serde_json::from_str::<serde_json::Value>(body) {
        if let Some(price) = extract_price_from_json(&json) {
            let mut node = SemanticNode::new(node_id, "price", &price);
            node.relevance = relevance;
            node.trust = TrustLevel::Untrusted;
            node.action = None;
            // Spara URL som value för spårbarhet
            node.value = Some(url.to_string());
            return node;
        }
    }

    // Fallback: vanlig data-nod med trunkerat innehåll
    let label = truncate_body(body, 200);
    let mut node = SemanticNode::new(node_id, "data", &label);
    node.relevance = relevance;
    node.trust = TrustLevel::Untrusted;
    node.action = None;
    node.value = Some(url.to_string());
    node
}

/// Merge XHR-derived nodes into an existing semantic tree
pub fn merge_xhr_nodes(tree: &mut SemanticTree, xhr_nodes: Vec<SemanticNode>) {
    // Lägg till noder som toppnivå-noder
    tree.nodes.extend(xhr_nodes);
}

/// Stub: merge utan fetch-feature (ingen XHR-interception)
#[cfg(not(feature = "fetch"))]
pub fn _merge_xhr_nodes_stub(
    tree: &mut crate::types::SemanticTree,
    _xhr_nodes: Vec<crate::types::SemanticNode>,
) {
    // Ingen XHR-interception utan fetch-feature
    let _ = tree;
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Trunkera body-text säkert (UTF-8-aware)
fn truncate_body(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_price_from_json_simple() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"price": "$1,599"}"#).expect("Giltig JSON");
        let result = extract_price_from_json(&json);
        assert_eq!(
            result,
            Some("$1,599".to_string()),
            "Borde hitta pris i enkelt objekt"
        );
    }

    #[test]
    fn test_extract_price_nested() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"data": {"product": {"price": "299 kr"}}}"#)
                .expect("Giltig JSON");
        let result = extract_price_from_json(&json);
        assert_eq!(
            result,
            Some("299 kr".to_string()),
            "Borde hitta pris i nästlat objekt"
        );
    }

    #[test]
    fn test_extract_price_numeric() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"amount": 1599.00}"#).expect("Giltig JSON");
        let result = extract_price_from_json(&json);
        assert!(result.is_some(), "Borde hitta numeriskt pris");
        assert!(
            result.as_ref().map_or(false, |v| v.contains("1599")),
            "Borde innehålla 1599, fick: {:?}",
            result
        );
    }

    #[test]
    fn test_extract_price_missing() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"name": "Product"}"#).expect("Giltig JSON");
        let result = extract_price_from_json(&json);
        assert!(result.is_none(), "Borde inte hitta pris utan prisfält");
    }

    #[test]
    fn test_extract_price_swedish_field() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"pris": "499 kr"}"#).expect("Giltig JSON");
        let result = extract_price_from_json(&json);
        assert_eq!(
            result,
            Some("499 kr".to_string()),
            "Borde hitta svenskt prisfält"
        );
    }

    #[test]
    fn test_extract_price_belopp_field() {
        let json: serde_json::Value =
            serde_json::from_str(r#"{"belopp": 250}"#).expect("Giltig JSON");
        let result = extract_price_from_json(&json);
        assert!(result.is_some(), "Borde hitta 'belopp'-fält");
    }

    #[test]
    fn test_normalize_xhr_to_node_json_price() {
        let body = r#"{"price": "$1,299.00", "name": "Laptop"}"#;
        let node = normalize_xhr_to_node(body, "https://api.shop.se/product/1", 0.8, 100);
        assert_eq!(node.role, "price", "JSON med pris borde ge roll 'price'");
        assert_eq!(node.label, "$1,299.00", "Label borde vara prisvärdet");
        assert_eq!(
            node.trust,
            TrustLevel::Untrusted,
            "XHR-data är alltid Untrusted"
        );
    }

    #[test]
    fn test_normalize_xhr_to_node_plain() {
        let body = "Some plain text response without JSON";
        let node = normalize_xhr_to_node(body, "https://example.com/data", 0.5, 200);
        assert_eq!(node.role, "data", "Ej-JSON borde ge roll 'data'");
        assert_eq!(
            node.trust,
            TrustLevel::Untrusted,
            "XHR-data är alltid Untrusted"
        );
    }

    #[test]
    fn test_normalize_xhr_to_node_json_no_price() {
        let body = r#"{"name": "Widget", "sku": "ABC123"}"#;
        let node = normalize_xhr_to_node(body, "https://api.shop.se/info", 0.6, 300);
        assert_eq!(node.role, "data", "JSON utan pris borde ge roll 'data'");
    }

    #[test]
    fn test_merge_xhr_nodes() {
        let mut tree = SemanticTree {
            url: "https://shop.se".to_string(),
            title: "Shop".to_string(),
            goal: "köp".to_string(),
            nodes: vec![SemanticNode::new(1, "button", "Köp")],
            injection_warnings: vec![],
            parse_time_ms: 0,
            xhr_intercepted: 0,
            xhr_blocked: 0,
        };

        let xhr_nodes = vec![
            SemanticNode::new(10_000, "price", "$99.00"),
            SemanticNode::new(10_001, "data", "Product info"),
        ];

        let count = xhr_nodes.len();
        merge_xhr_nodes(&mut tree, xhr_nodes);

        assert_eq!(tree.nodes.len(), 1 + count, "Borde ha original + XHR-noder");
        assert_eq!(
            tree.nodes.last().map(|n| n.role.as_str()),
            Some("data"),
            "Sista noden borde vara XHR-data"
        );
    }

    #[test]
    fn test_intercept_config_defaults() {
        let config = InterceptConfig::default();
        assert!(!config.enabled, "Borde vara avaktiverad per default");
        assert_eq!(
            config.max_requests, 10,
            "Default max_requests borde vara 10"
        );
        assert_eq!(config.timeout_ms, 2000, "Default timeout borde vara 2000ms");
    }

    #[test]
    fn test_xhr_capture_creation() {
        let capture = XhrCapture {
            url: "https://api.shop.se/price".to_string(),
            method: "GET".to_string(),
            headers: HashMap::new(),
        };
        assert_eq!(capture.url, "https://api.shop.se/price");
        assert_eq!(capture.method, "GET");
    }

    #[test]
    fn test_truncate_body() {
        let short = "kort text";
        assert_eq!(truncate_body(short, 200), "kort text");

        let long = "a".repeat(300);
        let truncated = truncate_body(&long, 200);
        assert!(truncated.len() <= 203, "Borde trunkeras till ~200 tecken");
        assert!(truncated.ends_with("..."), "Borde sluta med ...");
    }

    #[test]
    fn test_truncate_body_multibyte() {
        // Svenska tecken (2 bytes per char)
        let text = "å".repeat(150);
        let truncated = truncate_body(&text, 100);
        assert!(truncated.ends_with("..."), "Borde sluta med ...");
        // Borde inte panika på char boundary
    }

    #[test]
    fn test_extract_price_from_array() {
        let json: serde_json::Value =
            serde_json::from_str(r#"[{"price": "199 kr"}, {"price": "299 kr"}]"#)
                .expect("Giltig JSON");
        let result = extract_price_from_json(&json);
        assert_eq!(
            result,
            Some("199 kr".to_string()),
            "Borde hitta pris i första elementet"
        );
    }
}
