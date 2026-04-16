//! Reproduce HN edge case: field_queries=8, causal_boost never activates.

fn hn_html() -> String {
    let mut html = String::from(
        r#"<html><body>
<table id="hnmain">
<tr class="athing" id="12345"><td class="title"><span class="titleline"><a href="https://example.com/article1">Show HN: A new Rust web framework for AI agents</a></span></td></tr>
<tr><td class="subtext"><span class="score" id="score_12345">142 points</span> by rustdev 3 hours ago | 47 comments</td></tr>
"#,
    );
    // Generate many HN-style posts
    for i in 0..50 {
        html.push_str(&format!(
            r#"<tr class="athing" id="{}"><td class="title"><span class="titleline"><a href="https://example.com/article{}">Post number {} about technology topic {}</a></span></td></tr>
<tr><td class="subtext"><span class="score" id="score_{}">{}  points</span> by user{} {} hours ago | {} comments</td></tr>
"#,
            10000 + i, i, i, i % 10, 10000 + i, 50 + i * 3, i, i % 24, i * 2
        ));
    }
    html.push_str("</table></body></html>");
    html
}

#[test]
fn test_hn_feedback_works() {
    let html = hn_html();
    let url = "https://news.ycombinator.com/";
    let goal = "Rust web framework AI agents programming";
    let user_id: i64 = 90001;

    // Parse 1
    let r1 = aether_agent::parse_crfr(&html, goal, url, 10, false, "json");
    let j1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let nodes1 = j1["nodes"].as_array().unwrap();
    eprintln!(
        "Parse 1: {} nodes, field_queries={}",
        nodes1.len(),
        j1["crfr"]["field_queries"]
    );

    // Find the Rust framework post
    let target = nodes1.iter().find(|n| {
        n["label"]
            .as_str()
            .unwrap_or("")
            .contains("Rust web framework")
    });

    if target.is_none() {
        eprintln!("Nodes found:");
        for n in nodes1.iter().take(10) {
            eprintln!(
                "  id={} rel={:.3} {}",
                n["id"],
                n["relevance"].as_f64().unwrap_or(0.0),
                &n["label"].as_str().unwrap_or("?")
                    [..80.min(n["label"].as_str().unwrap_or("").len())]
            );
        }
        panic!("Target 'Rust web framework' not found in results");
    }
    let target_id = target.unwrap()["id"].as_u64().unwrap() as u32;
    let boost_before = target.unwrap()["causal_boost"].as_f64().unwrap_or(0.0);
    eprintln!("Target: id={}, boost_before={:.6}", target_id, boost_before);

    // Run parse multiple times to simulate field_queries buildup
    for _ in 0..7 {
        let _ = aether_agent::parse_crfr(&html, goal, url, 10, false, "json");
    }
    let r_multi = aether_agent::parse_crfr(&html, goal, url, 10, false, "json");
    let j_multi: serde_json::Value = serde_json::from_str(&r_multi).unwrap();
    eprintln!(
        "After 8 parses: field_queries={}",
        j_multi["crfr"]["field_queries"]
    );

    // Feedback
    let fb = aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", target_id), user_id);
    eprintln!("Feedback: {}", fb);
    assert!(
        fb.contains("\"status\":\"ok\""),
        "Feedback should succeed: {}",
        fb
    );

    // Parse with user_id
    let tree = aether_agent::build_tree_for_crfr(&html, goal, url, false);
    let r2 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 10, "json", false, user_id);
    let j2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    let nodes2 = j2["nodes"].as_array().unwrap();

    let target_after = nodes2.iter().find(|n| {
        n["label"]
            .as_str()
            .unwrap_or("")
            .contains("Rust web framework")
    });

    if let Some(node) = target_after {
        let boost = node["causal_boost"].as_f64().unwrap_or(0.0);
        eprintln!(
            "AFTER feedback (field_queries=8+): boost={:.6}, type={}",
            boost,
            node["resonance_type"].as_str().unwrap_or("?")
        );
        assert!(
            boost > 0.01,
            "HN BUG: boost should be > 0.01 even with high field_queries. Got: {:.6}",
            boost
        );
    } else {
        panic!("Target node disappeared after feedback");
    }
}

#[test]
fn test_trailing_slash_url_mismatch() {
    let html = r#"<html><body>
        <p>Important content about Hacker News top stories today.</p>
        <p>Other paragraph with different info.</p>
    </body></html>"#;

    let url_no_slash = "https://news.ycombinator.com";
    let url_with_slash = "https://news.ycombinator.com/";
    let goal = "Hacker News top stories";
    let user_id: i64 = 90002;

    // Parse with no-slash URL
    let r1 = aether_agent::parse_crfr(html, goal, url_no_slash, 5, false, "json");
    let j1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let node_id = j1["nodes"].as_array().unwrap()[0]["id"].as_u64().unwrap() as u32;

    // Feedback with WITH-slash URL (simulates redirect mismatch)
    let fb =
        aether_agent::crfr_feedback_user(url_with_slash, goal, &format!("[{}]", node_id), user_id);
    eprintln!("Feedback with trailing slash: {}", fb);
    assert!(fb.contains("\"status\":\"ok\""));

    // Parse with NO-slash URL — boosts should still apply
    let tree = aether_agent::build_tree_for_crfr(html, goal, url_no_slash, false);
    let r2 = aether_agent::parse_crfr_from_tree_js_user(
        &tree,
        goal,
        url_no_slash,
        5,
        "json",
        false,
        user_id,
    );
    let j2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    let boost = j2["nodes"].as_array().unwrap()[0]["causal_boost"]
        .as_f64()
        .unwrap_or(0.0);
    eprintln!("Boost despite URL mismatch: {:.6}", boost);
    assert!(
        boost > 0.01,
        "Trailing slash difference should not break boost. Got: {:.6}",
        boost
    );
    eprintln!("PASS: URL normalization works");
}
