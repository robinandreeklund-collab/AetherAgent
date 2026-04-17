//! Simulate Wikipedia-scale page (6000+ nodes) with per-user feedback.
//! This reproduces the exact production failure: 3 feedback nodes,
//! transfer finds 1, causal_boost = 0 on the target node.

fn generate_wikipedia_html(target_paragraph: &str, num_paragraphs: usize) -> String {
    let mut html = String::from("<html><body>\n<h1>Gothenburg</h1>\n");
    // Generate many paragraphs to simulate Wikipedia scale
    for i in 0..num_paragraphs {
        if i == num_paragraphs / 2 {
            // Insert target paragraph in the middle
            html.push_str(&format!("<p>{}</p>\n", target_paragraph));
        } else {
            html.push_str(&format!(
                "<p>Paragraph {} contains information about topic {} with details about subject {} and references to item {}.</p>\n",
                i, i % 50, i % 30, i % 20
            ));
        }
    }
    html.push_str("</body></html>");
    html
}

#[test]
fn test_feedback_on_large_page() {
    let target = "The Gothenburg Metropolitan Area has 1,080,980 inhabitants (2023) and extends to the municipalities of Ale, Alingsas, Goteborg, Harryda, Kungalv, Lerum, Lilla Edet, Molndal, Partille, Stenungsund.";
    let html = generate_wikipedia_html(target, 200); // 200 paragraphs
    let url = "https://wiki-sim-large.example.com/gbg";
    let goal = "population inhabitants Gothenburg";
    let user_id: i64 = 80001;

    // Step 1: Parse
    let r1 = aether_agent::parse_crfr(&html, goal, url, 15, false, "json");
    let j1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let nodes = j1["nodes"].as_array().unwrap();

    // Find the target node
    let target_node = nodes
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"));

    if target_node.is_none() {
        eprintln!("Target node not in top-15 results. Nodes:");
        for n in nodes {
            eprintln!(
                "  id={} rel={:.3} {}",
                n["id"],
                n["relevance"].as_f64().unwrap_or(0.0),
                &n["label"].as_str().unwrap_or("?")
                    [..60.min(n["label"].as_str().unwrap_or("").len())]
            );
        }
        panic!("Target node '1,080,980' not found in results");
    }

    let target_id = target_node.unwrap()["id"].as_u64().unwrap() as u32;
    let target_rel_before = target_node.unwrap()["relevance"].as_f64().unwrap_or(0.0);
    eprintln!(
        "Target node ID: {}, relevance before: {:.4}",
        target_id, target_rel_before
    );

    // Step 2: Feedback on target node
    let fb = aether_agent::crfr_feedback_user(url, goal, &format!("[{}]", target_id), user_id);
    eprintln!("Feedback: {}", fb);
    assert!(fb.contains("\"status\":\"ok\""), "Feedback should succeed");

    // Step 3: Parse again with user_id — SAME HTML (simulates cache hit)
    let tree = aether_agent::build_tree_for_crfr(&html, goal, url, false);
    let r2 =
        aether_agent::parse_crfr_from_tree_js_user(&tree, goal, url, 15, "json", false, user_id);
    let j2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    let nodes2 = j2["nodes"].as_array().unwrap();

    let target_after = nodes2
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("1,080,980"));

    if let Some(node) = target_after {
        let boost = node["causal_boost"].as_f64().unwrap_or(0.0);
        let rel = node["relevance"].as_f64().unwrap_or(0.0);
        let rtype = node["resonance_type"].as_str().unwrap_or("?");
        eprintln!(
            "AFTER feedback: boost={:.6}, relevance={:.4}, type={}",
            boost, rel, rtype
        );

        assert!(
            boost > 0.01,
            "Target node MUST have causal_boost > 0.01 after feedback. Got: {:.6}. \
             This is the core bug: feedback succeeds but boost is 0.",
            boost
        );
    } else {
        eprintln!("Target node not in results after feedback");
        for n in nodes2 {
            eprintln!(
                "  id={} boost={} rel={:.3} {}",
                n["id"],
                n["causal_boost"],
                n["relevance"].as_f64().unwrap_or(0.0),
                &n["label"].as_str().unwrap_or("?")
                    [..60.min(n["label"].as_str().unwrap_or("").len())]
            );
        }
        panic!("Target node disappeared from results");
    }
}
