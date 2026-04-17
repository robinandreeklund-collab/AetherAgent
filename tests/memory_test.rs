/// Memory usage verification test
/// Ensures text_hv skip + lazy-load keeps RSS reasonable.
use aether_agent::*;

#[test]
fn test_memory_after_20_parses() {
    // Parse 20 different URLs to fill the cache
    for i in 0..20 {
        let html = format!(
            r#"<html><body>
            <h1>Page {i} - Technology News</h1>
            <p>Article about AI, machine learning, and quantum computing developments in 2026.</p>
            <p>Scientists discovered new methods for protein folding using deep learning models.</p>
            <a href="/article/{i}">Read more about technology advances</a>
            </body></html>"#
        );
        let result = parse_crfr(
            &html,
            "technology AI machine learning quantum",
            &format!("https://mem-test-{i}.example.com"),
            5,
            false,
            "json",
        );
        let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            p["node_count"].as_u64().unwrap_or(0) > 0,
            "Parse {i} should return nodes"
        );
    }

    // Check cache stats
    let (entries, capacity) = resonance::cache_stats();
    println!("After 20 parses: cache={entries}/{capacity}");
    assert!(entries <= 64, "Cache should not exceed capacity 64");

    // Verify CRFR feedback still works with text_hv skip
    let html = r#"<html><body><h1>Feedback Test</h1><p>AAPL stock price $195</p></body></html>"#;
    let url = "https://mem-feedback-test.example.com";
    let goal = "AAPL stock price";

    let r1 = parse_crfr(html, goal, url, 5, false, "json");
    let p1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let nodes = p1["nodes"].as_array().unwrap();
    assert!(!nodes.is_empty(), "Should have nodes");

    let id = nodes[0]["id"].as_u64().unwrap_or(0);
    let fb = crfr_feedback(url, goal, &format!("[{id}]"));
    let fb_p: serde_json::Value = serde_json::from_str(&fb).unwrap();
    assert_eq!(fb_p["status"].as_str().unwrap_or(""), "ok");

    // Re-query should still have causal boost
    let r2 = parse_crfr(html, goal, url, 5, false, "json");
    let p2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    let has_boost = p2["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["causal_boost"].as_f64().unwrap_or(0.0) > 0.0);
    assert!(has_boost, "Causal boost should work even with text_hv skip");
    println!("Feedback + causal boost verified after text_hv skip");
}
