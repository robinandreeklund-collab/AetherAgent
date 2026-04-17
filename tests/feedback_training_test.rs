//! Verify that feedback ACTUALLY changes ranking on next query.
//! This is the real end-to-end test — not just "returns ok".

#[test]
fn test_global_feedback_changes_ranking() {
    let html = r##"<html><body>
        <h1>City Guide</h1>
        <p>Stockholm is the capital of Sweden with about 1 million inhabitants.</p>
        <p>The weather today is sunny and 22 degrees.</p>
        <p>Local restaurants include Oaxen Krog and Frantzén.</p>
        <p>Public transport runs 24/7 with SL cards.</p>
        <p>The population of Greater Stockholm is 2.4 million.</p>
    </body></html>"##;

    let url = "https://test-global-training-verify.example.com/city";
    let goal = "population inhabitants Stockholm";

    // Parse 1: get baseline ranking
    let result1 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let json1: serde_json::Value = serde_json::from_str(&result1).unwrap();
    let nodes1 = json1["nodes"].as_array().unwrap();
    assert!(!nodes1.is_empty(), "Should have nodes");

    // Find the node about "2.4 million" and a less relevant node
    let mut population_node_id = None;
    let mut weather_node_id = None;
    for n in nodes1 {
        let label = n["label"].as_str().unwrap_or("");
        if label.contains("2.4 million") || label.contains("1 million") {
            population_node_id = Some(n["id"].as_u64().unwrap() as u32);
        }
        if label.contains("weather") || label.contains("sunny") {
            weather_node_id = Some(n["id"].as_u64().unwrap() as u32);
        }
    }

    let pop_id = population_node_id.expect("Should find population node");
    eprintln!("Population node ID: {}", pop_id);
    eprintln!("Weather node ID: {:?}", weather_node_id);

    // Get baseline relevance for population node
    let pop_relevance_before: f64 = nodes1
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == pop_id)
        .and_then(|n| n["relevance"].as_f64())
        .unwrap_or(0.0);
    eprintln!(
        "Population relevance BEFORE feedback: {:.4}",
        pop_relevance_before
    );

    // Send feedback: "population node was correct"
    let feedback = aether_agent::crfr_feedback(url, goal, &format!("[{}]", pop_id));
    eprintln!("Feedback response: {}", feedback);
    assert!(
        feedback.contains("\"status\":\"ok\""),
        "Feedback should succeed"
    );

    // Parse 2: same query — population node should now have higher relevance
    let result2 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let json2: serde_json::Value = serde_json::from_str(&result2).unwrap();
    let nodes2 = json2["nodes"].as_array().unwrap();

    let pop_relevance_after: f64 = nodes2
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == pop_id)
        .and_then(|n| n["relevance"].as_f64())
        .unwrap_or(0.0);

    let causal_boost: f64 = nodes2
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == pop_id)
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    eprintln!(
        "Population relevance AFTER feedback: {:.4}",
        pop_relevance_after
    );
    eprintln!("Causal boost: {:.4}", causal_boost);

    assert!(
        causal_boost > 0.0 || pop_relevance_after > pop_relevance_before,
        "After feedback, population node should have causal_boost > 0 or higher relevance. Before: {:.4}, After: {:.4}, Boost: {:.4}",
        pop_relevance_before, pop_relevance_after, causal_boost
    );
}

#[test]
fn test_user_feedback_changes_ranking_isolated() {
    let html = r##"<html><body>
        <h1>Programming Languages</h1>
        <p>Python is popular for data science and machine learning.</p>
        <p>Rust provides memory safety without garbage collection.</p>
        <p>JavaScript runs in every browser worldwide.</p>
        <p>Go is designed for simplicity and concurrency.</p>
        <p>TypeScript adds type safety to JavaScript.</p>
    </body></html>"##;

    let url = "https://test-user-training-verify.example.com/langs";
    let goal = "memory safety systems programming";
    let user_id: i64 = 777;

    // Parse 1: baseline (global)
    let result1 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let json1: serde_json::Value = serde_json::from_str(&result1).unwrap();
    let nodes1 = json1["nodes"].as_array().unwrap();

    // Find Rust node
    let rust_id = nodes1
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("Rust"))
        .map(|n| n["id"].as_u64().unwrap() as u32)
        .expect("Should find Rust node");

    let rust_boost_before: f64 = nodes1
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == rust_id)
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    eprintln!(
        "Rust node ID: {}, boost before: {:.4}",
        rust_id, rust_boost_before
    );

    // Send USER feedback (not global)
    let feedback = aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", rust_id), user_id);
    eprintln!("User feedback: {}", feedback);
    assert!(
        feedback.contains("\"status\":\"ok\""),
        "User feedback should succeed"
    );
    assert!(feedback.contains("\"isolated\":true"), "Should be isolated");

    // Parse 2: GLOBAL query should NOT have boost (different user)
    let result2_global = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let json2g: serde_json::Value = serde_json::from_str(&result2_global).unwrap();
    let nodes2g = json2g["nodes"].as_array().unwrap();

    let global_boost_after: f64 = nodes2g
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == rust_id)
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    eprintln!(
        "Global Rust boost AFTER user feedback: {:.4}",
        global_boost_after
    );
    assert!(
        (global_boost_after - rust_boost_before).abs() < 0.001,
        "Global weights should NOT change from user feedback. Before: {:.4}, After: {:.4}",
        rust_boost_before,
        global_boost_after
    );

    eprintln!("VERIFIED: User feedback did NOT affect global weights");
}

#[test]
fn test_user_feedback_actually_changes_user_ranking() {
    let html = r##"<html><body>
        <h1>Food Guide</h1>
        <p>Pizza is an Italian dish with tomato sauce and cheese.</p>
        <p>Sushi is a Japanese dish made with vinegared rice.</p>
        <p>Tacos are a Mexican dish with tortillas and fillings.</p>
        <p>Pad Thai is a Thai stir-fried noodle dish.</p>
        <p>Ramen is a Japanese noodle soup with rich broth.</p>
    </body></html>"##;

    let url = "https://test-user-ranking-change.example.com/food";
    let goal = "Japanese food noodle soup";
    let user_id: i64 = 999;

    // Parse 1: global baseline
    let result1 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let json1: serde_json::Value = serde_json::from_str(&result1).unwrap();
    let nodes1 = json1["nodes"].as_array().unwrap();

    // Find ramen node
    let ramen_id = nodes1
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("Ramen"))
        .map(|n| n["id"].as_u64().unwrap() as u32)
        .expect("Should find Ramen node");

    let ramen_boost_before: f64 = nodes1
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == ramen_id)
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);
    eprintln!(
        "Ramen node ID: {}, causal_boost before: {:.4}",
        ramen_id, ramen_boost_before
    );

    // Send user feedback: "ramen was the right answer"
    let fb = aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", ramen_id), user_id);
    eprintln!("User feedback: {}", fb);
    assert!(fb.contains("\"status\":\"ok\""));

    // Parse 2: WITH user_id — should show boost
    let tree = aether_agent::build_tree_for_crfr(html, goal, url, false);
    let result2 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 10, "json", false, user_id);
    let json2: serde_json::Value = serde_json::from_str(&result2).unwrap();
    let nodes2 = json2["nodes"].as_array().unwrap();

    let ramen_boost_after: f64 = nodes2
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == ramen_id)
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    let ramen_relevance_after: f64 = nodes2
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == ramen_id)
        .and_then(|n| n["relevance"].as_f64())
        .unwrap_or(0.0);

    eprintln!(
        "Ramen causal_boost AFTER user feedback (user query): {:.4}",
        ramen_boost_after
    );
    eprintln!(
        "Ramen relevance AFTER user feedback (user query): {:.4}",
        ramen_relevance_after
    );

    assert!(
        ramen_boost_after > ramen_boost_before,
        "User feedback MUST increase causal_boost for user queries. Before: {:.4}, After: {:.4}",
        ramen_boost_before,
        ramen_boost_after
    );

    // Parse 3: WITHOUT user_id (global) — should NOT show boost
    let result3 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let json3: serde_json::Value = serde_json::from_str(&result3).unwrap();
    let nodes3 = json3["nodes"].as_array().unwrap();

    let ramen_boost_global: f64 = nodes3
        .iter()
        .find(|n| n["id"].as_u64().unwrap() as u32 == ramen_id)
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    eprintln!(
        "Ramen causal_boost GLOBAL (no user): {:.4}",
        ramen_boost_global
    );

    assert!(
        (ramen_boost_global - ramen_boost_before).abs() < 0.01,
        "Global query should NOT be affected by user feedback. Before: {:.4}, Global after: {:.4}",
        ramen_boost_before,
        ramen_boost_global
    );

    eprintln!("=== VERIFIED: User feedback changes ranking for user, NOT for global ===");
}
