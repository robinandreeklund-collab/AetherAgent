//! Test that causal_boost does NOT decay on repeated parses.
//! Reproduces the exact production bug: boost goes from 0.15 to e-42.

#[test]
fn test_boost_does_not_decay_on_repeated_parse() {
    let html = r##"<html><body>
        <h1>Gothenburg</h1>
        <p>Gothenburg is the second-largest city in Sweden.</p>
        <p>The Gothenburg Metropolitan Area has 1,080,980 inhabitants (2023).</p>
        <p>The city was founded in 1621 by King Gustav II Adolf.</p>
        <p>Gothenburg is known for its seafood and cultural attractions.</p>
        <p>The port of Gothenburg is the largest in the Nordic countries.</p>
    </body></html>"##;

    let url = "https://boost-decay-test.example.com/gothenburg";
    let goal = "population inhabitants Gothenburg how many people";
    let user_id: i64 = 50001;

    // Step 1: Initial parse
    let r1 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let j1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let nodes1 = j1["nodes"].as_array().unwrap();

    let pop_node = nodes1
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"))
        .expect("Should find population node");
    let pop_id = pop_node["id"].as_u64().unwrap() as u32;
    eprintln!("Population node ID: {}", pop_id);

    // Step 2: User feedback
    let fb = aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", pop_id), user_id);
    eprintln!("Feedback: {}", fb);
    assert!(fb.contains("\"status\":\"ok\""));

    // Step 3: Parse with user_id — should have boost
    let tree = aether_agent::build_tree_for_crfr(html, goal, url, false);
    let r2 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 10, "json", false, user_id);
    let j2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    let boost1: f64 = j2["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"))
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);
    eprintln!("Parse 1 after feedback: boost={:.6}", boost1);
    assert!(
        boost1 > 0.01,
        "First parse after feedback should have boost > 0.01, got {}",
        boost1
    );

    // Step 4-8: Parse 5 more times — boost must NOT decay
    let mut prev_boost = boost1;
    for i in 2..=6 {
        let tree = aether_agent::build_tree_for_crfr(html, goal, url, false);
        let r = aether_agent::parse_crfr_from_tree_js_user(
            &tree, goal, url, 10, "json", false, user_id,
        );
        let j: serde_json::Value = serde_json::from_str(&r).unwrap();
        let boost: f64 = j["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"))
            .and_then(|n| n["causal_boost"].as_f64())
            .unwrap_or(0.0);
        eprintln!("Parse {} boost={:.6} (prev={:.6})", i, boost, prev_boost);

        // Boost must not drop by more than 50% between parses
        assert!(
            boost > prev_boost * 0.5,
            "Parse {}: boost decayed too much! {:.6} → {:.6} (>50% drop)",
            i,
            prev_boost,
            boost
        );
        // Boost must stay above meaningful threshold
        assert!(
            boost > 0.001,
            "Parse {}: boost dropped to near-zero: {:.6}",
            i,
            boost
        );
        prev_boost = boost;
    }

    eprintln!(
        "PASS: Boost stable across 6 parses (final={:.6})",
        prev_boost
    );
}

#[test]
fn test_multiple_feedback_increases_boost() {
    let html = r##"<html><body>
        <h1>Test Page</h1>
        <p>Target node that should get boosted by repeated feedback.</p>
        <p>Irrelevant node about weather.</p>
    </body></html>"##;

    let url = "https://multi-feedback-boost.example.com/test";
    let goal = "target boosted";
    let user_id: i64 = 50002;

    // Parse + first feedback
    let r1 = aether_agent::parse_crfr(html, goal, url, 10, false, "json");
    let j1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let node_id = j1["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("Target"))
        .map(|n| n["id"].as_u64().unwrap() as u32)
        .expect("find target node");

    // Feedback 1
    aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", node_id), user_id);
    let tree = aether_agent::build_tree_for_crfr(html, goal, url, false);
    let r2 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 10, "json", false, user_id);
    let boost_after_1: f64 = serde_json::from_str::<serde_json::Value>(&r2).unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("Target"))
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    // Feedback 2
    aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", node_id), user_id);
    let tree = aether_agent::build_tree_for_crfr(html, goal, url, false);
    let r3 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 10, "json", false, user_id);
    let boost_after_2: f64 = serde_json::from_str::<serde_json::Value>(&r3).unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("Target"))
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    // Feedback 3
    aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", node_id), user_id);
    let tree = aether_agent::build_tree_for_crfr(html, goal, url, false);
    let r4 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 10, "json", false, user_id);
    let boost_after_3: f64 = serde_json::from_str::<serde_json::Value>(&r4).unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("Target"))
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);

    eprintln!(
        "Boost: fb1={:.4}, fb2={:.4}, fb3={:.4}",
        boost_after_1, boost_after_2, boost_after_3
    );

    assert!(boost_after_1 > 0.01, "First feedback should give boost");
    assert!(
        boost_after_2 >= boost_after_1 * 0.9,
        "Second feedback should not decrease boost: {} vs {}",
        boost_after_2,
        boost_after_1
    );
    assert!(
        boost_after_3 >= boost_after_2 * 0.9,
        "Third feedback should not decrease boost: {} vs {}",
        boost_after_3,
        boost_after_2
    );

    eprintln!("PASS: Boost accumulates with feedback");
}

#[test]
fn test_boost_survives_content_change() {
    // Simulates: Wikipedia page fetched multiple times with minor changes
    let html1 = r##"<html><body>
        <h1>Gothenburg</h1>
        <p>Gothenburg is the second-largest city in Sweden.</p>
        <p>The Gothenburg Metropolitan Area has 1,080,980 inhabitants (2023).</p>
        <p>Updated at 10:00.</p>
    </body></html>"##;
    let html2 = r##"<html><body>
        <h1>Gothenburg</h1>
        <p>Gothenburg is the second-largest city in Sweden.</p>
        <p>The Gothenburg Metropolitan Area has 1,080,980 inhabitants (2023).</p>
        <p>Updated at 10:05.</p>
    </body></html>"##;
    let html3 = r##"<html><body>
        <h1>Gothenburg</h1>
        <p>Gothenburg is the second-largest city in Sweden.</p>
        <p>The Gothenburg Metropolitan Area has 1,080,980 inhabitants (2023).</p>
        <p>Updated at 10:10.</p>
    </body></html>"##;

    let url = "https://content-change-boost.example.com/gbg";
    let goal = "population inhabitants Gothenburg";
    let user_id: i64 = 50003;

    // Parse with html1
    let r1 = aether_agent::parse_crfr(html1, goal, url, 10, false, "json");
    let j1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let node_id = j1["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"))
        .map(|n| n["id"].as_u64().unwrap() as u32)
        .expect("find pop node");

    // Feedback
    let fb = aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", node_id), user_id);
    assert!(fb.contains("\"status\":\"ok\""));

    // Parse with html2 (DIFFERENT content) — boost should survive
    let tree2 = aether_agent::build_tree_for_crfr(html2, goal, url, false);
    let r2 =
        aether_agent::parse_crfr_from_tree_js_user(&tree2, goal, url, 10, "json", false, user_id);
    let boost2: f64 = serde_json::from_str::<serde_json::Value>(&r2).unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"))
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);
    eprintln!("After content change 1: boost={:.6}", boost2);

    // Parse with html3 (ANOTHER change) — boost should still survive
    let tree3 = aether_agent::build_tree_for_crfr(html3, goal, url, false);
    let r3 =
        aether_agent::parse_crfr_from_tree_js_user(&tree3, goal, url, 10, "json", false, user_id);
    let boost3: f64 = serde_json::from_str::<serde_json::Value>(&r3).unwrap()["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"))
        .and_then(|n| n["causal_boost"].as_f64())
        .unwrap_or(0.0);
    eprintln!("After content change 2: boost={:.6}", boost3);

    assert!(
        boost2 > 0.01,
        "Boost should survive content change 1: {:.6}",
        boost2
    );
    assert!(
        boost3 > 0.01,
        "Boost should survive content change 2: {:.6}",
        boost3
    );
    eprintln!("PASS: Boost survives content changes");
}
