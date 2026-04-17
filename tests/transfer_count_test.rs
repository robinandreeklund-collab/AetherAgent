//! Verify that transfer_from only transfers nodes with actual feedback,
//! not ALL nodes. Reproduces the "2257 nodes transferred" bug.

#[test]
fn test_user_field_only_has_feedback_nodes_learned() {
    let html = r##"<html><body>
        <h1>City Guide</h1>
        <p>Stockholm is the capital of Sweden with about 1 million inhabitants.</p>
        <p>The weather today is sunny and 22 degrees.</p>
        <p>Local restaurants include Oaxen Krog and Frantzén.</p>
        <p>Public transport runs 24/7 with SL cards.</p>
        <p>The population of Greater Stockholm is 2.4 million.</p>
        <p>Famous landmarks include Gamla Stan and Vasa Museum.</p>
        <p>The city was founded in 1252 by Birger Jarl.</p>
        <p>Stockholm hosted the 1912 Summer Olympics.</p>
        <p>The metro system has over 100 stations.</p>
        <p>Average temperature in January is -3°C.</p>
    </body></html>"##;

    let url = "https://transfer-count-test.example.com/stockholm";
    let goal = "population inhabitants Stockholm";
    let user_id: i64 = 70001;

    // Parse to create global field
    let r1 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let j1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let nodes = j1["nodes"].as_array().unwrap();
    assert!(nodes.len() >= 5, "Should have multiple nodes");

    // Get just ONE node ID for feedback
    let pop_node = nodes
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("2.4 million"))
        .expect("find 2.4 million node");
    let pop_id = pop_node["id"].as_u64().unwrap() as u32;
    eprintln!("Feedback on node {} only", pop_id);

    // Send feedback on just that ONE node
    let fb = aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", pop_id), user_id);
    eprintln!("Feedback: {}", fb);
    assert!(fb.contains("\"status\":\"ok\""));
    assert!(fb.contains("\"nodes_updated\":1"));

    // Now parse with user_id and check the merge log
    let tree = aether_agent::build_tree_for_crfr(html, goal, url, false);
    let r2 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 10, "json", false, user_id);
    let j2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    let nodes2 = j2["nodes"].as_array().unwrap();

    // Check that the boosted node has actual boost
    let pop_boost: f64 = nodes2
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("2.4 million"))
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);
    eprintln!("Population node boost after feedback: {:.6}", pop_boost);

    // The weather node should NOT have boost (no feedback given on it)
    let weather_boost: f64 = nodes2
        .iter()
        .find(|n| {
            n["label"].as_str().unwrap_or("").contains("weather")
                || n["label"].as_str().unwrap_or("").contains("sunny")
        })
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);
    eprintln!("Weather node boost (no feedback): {:.6}", weather_boost);

    assert!(
        pop_boost > 0.01,
        "Feedback node must have boost > 0.01, got {:.6}",
        pop_boost
    );
    assert!(
        weather_boost < 0.01,
        "Non-feedback node should not have boost, got {:.6}",
        weather_boost
    );

    eprintln!("PASS: Only feedback nodes get boosted");
}
