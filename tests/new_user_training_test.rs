//! Comprehensive end-to-end tests for new user training.
//! Simulates: signup → extract → feedback → extract again → verify ranking.
//! Tests every edge case that could break in production.

/// Helper: extract node by label substring from parse result
fn find_node(result: &str, label_contains: &str) -> Option<(u32, f64, f64)> {
    let json: serde_json::Value = serde_json::from_str(result).ok()?;
    let nodes = json["nodes"].as_array()?;
    nodes.iter().find_map(|n| {
        let label = n["label"].as_str().unwrap_or("");
        if label.contains(label_contains) {
            Some((
                n["id"].as_u64().unwrap_or(0) as u32,
                n["relevance"].as_f64().unwrap_or(0.0),
                n["causal_boost"].as_f64().unwrap_or(0.0),
            ))
        } else {
            None
        }
    })
}

const TEST_HTML: &str = r##"<html><body>
    <h1>Stockholm Travel Guide</h1>
    <p>Stockholm is the capital of Sweden, built on 14 islands.</p>
    <p>The population of Stockholm municipality is about 1 million people.</p>
    <p>Weather in Stockholm: cold winters, warm summers, average 7°C.</p>
    <p>Top restaurants: Oaxen Krog (2 Michelin stars), Frantzén (3 stars).</p>
    <p>Public transport: SL runs metro, buses, and commuter trains.</p>
    <p>The Greater Stockholm region has approximately 2.4 million inhabitants.</p>
    <p>Famous landmarks include Gamla Stan, Vasa Museum, and ABBA Museum.</p>
    <p>Stockholm was founded in 1252 by Birger Jarl.</p>
</body></html>"##;

const TEST_URL: &str = "https://new-user-test-complete.example.com/stockholm";
const TEST_GOAL: &str = "population inhabitants Stockholm how many people";

// ─── Test 1: Brand new user, first-ever parse + feedback + verify ───

#[test]
fn test_new_user_first_parse_feedback_changes_ranking() {
    let user_id: i64 = 10001; // fresh user, never seen before

    // Step 1: First parse (global — no user history exists)
    let result1 = aether_agent::parse_crfr(TEST_HTML, TEST_GOAL, TEST_URL, 10, false, "json");
    let (pop_id, rel_before, boost_before) =
        find_node(&result1, "2.4 million").expect("Should find 2.4 million node");
    eprintln!(
        "Step 1 - BEFORE feedback: node={}, relevance={:.4}, boost={:.4}",
        pop_id, rel_before, boost_before
    );
    assert!(rel_before > 0.0, "Node should have some relevance");
    assert!(
        boost_before.abs() < 0.001,
        "No boost before feedback: {:.4}",
        boost_before
    );

    // Step 2: User sends feedback — "2.4 million node was correct"
    let fb =
        aether_agent::crfr_feedback_user(TEST_URL, TEST_GOAL, &format!("[{}]", pop_id), user_id);
    eprintln!("Step 2 - Feedback response: {}", fb);
    assert!(
        fb.contains("\"status\":\"ok\""),
        "Feedback should succeed: {}",
        fb
    );
    assert!(
        fb.contains("\"isolated\":true"),
        "Should be isolated: {}",
        fb
    );

    // Step 3: Parse again WITH user_id — should show boost
    let tree = aether_agent::build_tree_for_crfr(TEST_HTML, TEST_GOAL, TEST_URL, false);
    let result2 = aether_agent::parse_crfr_from_tree_js_user(
        &tree, TEST_GOAL, TEST_URL, 10, "json", false, user_id,
    );
    let (_, rel_after, boost_after) =
        find_node(&result2, "2.4 million").expect("Should still find node after feedback");
    eprintln!(
        "Step 3 - AFTER feedback (user query): relevance={:.4}, boost={:.4}",
        rel_after, boost_after
    );
    assert!(
        boost_after > 0.0,
        "Causal boost must be > 0 after feedback. Got: {:.4}",
        boost_after
    );
    assert!(
        rel_after > rel_before,
        "Relevance must increase. Before: {:.4}, After: {:.4}",
        rel_before,
        rel_after
    );

    eprintln!("PASS: New user feedback changes ranking");
}

// ─── Test 2: User feedback does NOT affect global weights ───

#[test]
fn test_user_feedback_isolation_from_global() {
    let user_id: i64 = 10002;
    let url = "https://isolation-test-global.example.com/page";

    // Global baseline
    let r1 = aether_agent::parse_crfr(TEST_HTML, TEST_GOAL, url, 10, false, "json");
    let (node_id, global_rel_before, global_boost_before) =
        find_node(&r1, "2.4 million").expect("find node");
    eprintln!(
        "Global BEFORE: relevance={:.4}, boost={:.4}",
        global_rel_before, global_boost_before
    );

    // User feedback
    let fb = aether_agent::crfr_feedback_user(url, TEST_GOAL, &format!("[{}]", node_id), user_id);
    assert!(fb.contains("\"status\":\"ok\""));

    // Global query AFTER user feedback — must be unchanged
    let r2 = aether_agent::parse_crfr(TEST_HTML, TEST_GOAL, url, 10, false, "json");
    let (_, global_rel_after, global_boost_after) =
        find_node(&r2, "2.4 million").expect("find node");
    eprintln!(
        "Global AFTER user feedback: relevance={:.4}, boost={:.4}",
        global_rel_after, global_boost_after
    );

    assert!(
        (global_boost_after - global_boost_before).abs() < 0.001,
        "Global boost must NOT change from user feedback. Before: {:.4}, After: {:.4}",
        global_boost_before,
        global_boost_after
    );

    eprintln!("PASS: User feedback does not affect global");
}

// ─── Test 3: Different users don't affect each other ───

#[test]
fn test_different_users_isolated() {
    let user_a: i64 = 10003;
    let user_b: i64 = 10004;
    let url = "https://multi-user-isolation.example.com/page";

    // Parse baseline
    let r1 = aether_agent::parse_crfr(TEST_HTML, TEST_GOAL, url, 10, false, "json");
    let (pop_id, _, _) = find_node(&r1, "2.4 million").expect("find pop node");
    let (weather_id, _, _) = find_node(&r1, "Weather").unwrap_or((0, 0.0, 0.0));

    // User A: feedback on population node
    let fb_a = aether_agent::crfr_feedback_user(url, TEST_GOAL, &format!("[{}]", pop_id), user_a);
    assert!(fb_a.contains("\"status\":\"ok\""));

    // User B: feedback on weather node (different preference)
    if weather_id > 0 {
        let fb_b = aether_agent::crfr_feedback_user(
            url,
            "weather temperature Stockholm",
            &format!("[{}]", weather_id),
            user_b,
        );
        assert!(fb_b.contains("\"status\":\"ok\""));
    }

    // User A query: population should be boosted
    let tree = aether_agent::build_tree_for_crfr(TEST_HTML, TEST_GOAL, url, false);
    let ra = aether_agent::parse_crfr_from_tree_js_user(
        &tree, TEST_GOAL, url, 10, "json", false, user_a,
    );
    let (_, _, boost_a_pop) = find_node(&ra, "2.4 million").expect("find pop");
    eprintln!("User A pop boost: {:.4}", boost_a_pop);
    assert!(boost_a_pop > 0.0, "User A should have pop boost");

    // User B query: population should NOT be boosted (B didn't train on it)
    let rb = aether_agent::parse_crfr_from_tree_js_user(
        &tree, TEST_GOAL, url, 10, "json", false, user_b,
    );
    let (_, _, boost_b_pop) = find_node(&rb, "2.4 million").unwrap_or((0, 0.0, 0.0));
    eprintln!("User B pop boost: {:.4}", boost_b_pop);
    // User B only trained on weather, not population — should have 0 pop boost
    assert!(
        boost_b_pop < 0.01,
        "User B should NOT have population boost. Got: {:.4}",
        boost_b_pop
    );

    eprintln!("PASS: Different users are isolated from each other");
}

// ─── Test 4: Multiple feedbacks accumulate ───

#[test]
fn test_multiple_feedbacks_accumulate() {
    let user_id: i64 = 10005;
    let url = "https://accumulate-feedback.example.com/page";

    // Parse baseline
    let r1 = aether_agent::parse_crfr(TEST_HTML, TEST_GOAL, url, 10, false, "json");
    let (node_id, _, _) = find_node(&r1, "2.4 million").expect("find node");

    // Send feedback 3 times
    for i in 0..3 {
        let fb =
            aether_agent::crfr_feedback_user(url, TEST_GOAL, &format!("[{}]", node_id), user_id);
        assert!(
            fb.contains("\"status\":\"ok\""),
            "Feedback {} failed: {}",
            i,
            fb
        );
    }

    // Query with user — should have accumulated boost
    let tree = aether_agent::build_tree_for_crfr(TEST_HTML, TEST_GOAL, url, false);
    let r2 = aether_agent::parse_crfr_from_tree_js_user(
        &tree, TEST_GOAL, url, 10, "json", false, user_id,
    );
    let (_, rel, boost) = find_node(&r2, "2.4 million").expect("find node");
    eprintln!(
        "After 3 feedbacks: relevance={:.4}, boost={:.4}",
        rel, boost
    );
    assert!(
        boost > 0.1,
        "3 feedbacks should give substantial boost. Got: {:.4}",
        boost
    );

    eprintln!("PASS: Multiple feedbacks accumulate");
}

// ─── Test 5: user_id=0 rejected, negative user_id rejected ───

#[test]
fn test_invalid_user_ids_rejected() {
    let url = "https://reject-test.example.com/page";
    let _ = aether_agent::parse_crfr(TEST_HTML, TEST_GOAL, url, 10, false, "json");

    let fb0 = aether_agent::crfr_feedback_user(url, TEST_GOAL, "[1]", 0);
    assert!(
        fb0.contains("error") || fb0.contains("required"),
        "user_id=0 must be rejected: {}",
        fb0
    );

    let fb_neg = aether_agent::crfr_feedback_user(url, TEST_GOAL, "[1]", -1);
    assert!(
        fb_neg.contains("error") || fb_neg.contains("required"),
        "user_id=-1 must be rejected: {}",
        fb_neg
    );

    // Direct global feedback function should still work (internal use only)
    let fb_global = aether_agent::crfr_feedback(url, TEST_GOAL, "[1]");
    assert!(
        fb_global.contains("\"status\":\"ok\""),
        "Global feedback should work: {}",
        fb_global
    );

    eprintln!("PASS: Invalid user IDs rejected");
}

// ─── Test 6: Feedback on URL not yet parsed returns error ───

#[test]
fn test_feedback_on_unknown_url_returns_error() {
    let fb = aether_agent::crfr_feedback_user(
        "https://never-parsed-before.example.com/unknown",
        "test",
        "[1, 2, 3]",
        10006,
    );
    eprintln!("Feedback on unknown URL: {}", fb);
    assert!(
        fb.contains("no_field") || fb.contains("No cached"),
        "Feedback on unknown URL should return no_field: {}",
        fb
    );

    eprintln!("PASS: Unknown URL returns proper error");
}

// ─── Test 7: User query without prior feedback matches global ───

#[test]
fn test_user_query_without_feedback_matches_global() {
    let user_id: i64 = 10007;
    let url = "https://no-feedback-yet.example.com/page";

    // Global parse
    let r_global = aether_agent::parse_crfr(TEST_HTML, TEST_GOAL, url, 10, false, "json");
    let (_, global_rel, _) = find_node(&r_global, "2.4 million").expect("find node");

    // User parse (no feedback given yet)
    let tree = aether_agent::build_tree_for_crfr(TEST_HTML, TEST_GOAL, url, false);
    let r_user = aether_agent::parse_crfr_from_tree_js_user(
        &tree, TEST_GOAL, url, 10, "json", false, user_id,
    );
    let (_, user_rel, user_boost) = find_node(&r_user, "2.4 million").expect("find node");

    eprintln!(
        "No-feedback: global_rel={:.4}, user_rel={:.4}, user_boost={:.4}",
        global_rel, user_rel, user_boost
    );

    // Without feedback, user should get same results as global
    assert!(
        (user_rel - global_rel).abs() < 0.1,
        "Without feedback, user results should match global. Global: {:.4}, User: {:.4}",
        global_rel,
        user_rel
    );
    assert!(
        user_boost.abs() < 0.001,
        "Without feedback, user boost should be 0. Got: {:.4}",
        user_boost
    );

    eprintln!("PASS: User without feedback matches global");
}
