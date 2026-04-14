/// Phase 3-4 verification tests
/// Tests link-following intelligence, XHR pipeline, stealth headers, and bot detection.
///
/// Run: cargo test --test phase34_verification -- --nocapture
use aether_agent::*;

fn parse_crfr_json(html: &str, goal: &str, url: &str, top_n: u32) -> serde_json::Value {
    let result = parse_crfr(html, goal, url, top_n, false, "json");
    serde_json::from_str(&result).expect("parse_crfr ska returnera giltig JSON")
}

// ─── Phase 3.1: Auto-feedback in CRFR ──────────────────────────────────────

#[test]
fn test_phase31_feedback_persists_and_boosts() {
    // Parse a page, give feedback, re-parse — verify causal_boost > 0
    let html = r##"<html><body>
        <h1>Stock Market Report</h1>
        <p id="p1">AAPL rose 3.2% to $198.50 on strong earnings.</p>
        <p id="p2">MSFT fell 1.1% amid cloud revenue concerns.</p>
        <p id="p3">TSLA gained 5.7% after delivery numbers beat estimates.</p>
        <p id="p4">The S&P 500 closed at 5,421, up 0.8% for the day.</p>
        <nav><a href="/markets">Markets</a><a href="/login">Login</a></nav>
    </body></html>"##;

    let url = "https://finance-phase31-test.example.com";
    let goal = "stock price AAPL Apple shares earnings";

    // First parse
    let result1 = parse_crfr_json(html, goal, url, 10);
    let nodes1 = result1["nodes"].as_array().unwrap();
    assert!(!nodes1.is_empty(), "Ska ha resultat vid första parse");

    // Find a content node to give feedback on
    let content_id = nodes1
        .iter()
        .find(|n| n["label"].as_str().unwrap_or("").contains("AAPL"))
        .and_then(|n| n["id"].as_u64())
        .unwrap_or(nodes1[0]["id"].as_u64().unwrap_or(0));

    // Give feedback
    let ids_json = format!("[{}]", content_id);
    let fb_result = crfr_feedback(url, goal, &ids_json);
    let fb: serde_json::Value = serde_json::from_str(&fb_result).unwrap();
    assert_eq!(
        fb["status"].as_str().unwrap_or(""),
        "ok",
        "Feedback ska accepteras"
    );

    // Re-parse — same goal
    let result2 = parse_crfr_json(html, goal, url, 10);
    let nodes2 = result2["nodes"].as_array().unwrap();

    // Check if any node has causal_boost > 0
    let has_boost = nodes2
        .iter()
        .any(|n| n["causal_boost"].as_f64().unwrap_or(0.0) > 0.0);

    println!("Phase 3.1: Feedback test");
    println!("  Feedback node ID: {}", content_id);
    println!("  Feedback status: {}", fb["status"]);
    for n in nodes2.iter().take(3) {
        println!(
            "  [{}] {} boost={:.3}",
            n["role"].as_str().unwrap_or("?"),
            &n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(50)],
            n["causal_boost"].as_f64().unwrap_or(0.0)
        );
    }
    assert!(has_boost, "Re-query efter feedback ska ge causal_boost > 0");
}

// ─── Phase 3.2: Cross-page relevance ────────────────────────────────────

#[test]
fn test_phase32_results_sorted_by_relevance() {
    // Verify that CRFR results are always sorted by relevance (highest first)
    let html = r##"<html><body>
        <article><h2>Big Story</h2><p>This is the most important story of the day.</p></article>
        <article><h2>Small Update</h2><p>Minor changes to existing policy.</p></article>
        <article><h2>Breaking: Major Discovery</h2><p>Scientists find breakthrough in quantum computing research.</p></article>
    </body></html>"##;

    let result = parse_crfr_json(
        html,
        "breaking news important major discovery",
        "https://news.example.com",
        10,
    );
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 3.2: Relevance ordering");
    let mut prev_rel = f64::MAX;
    for n in nodes {
        let rel = n["relevance"].as_f64().unwrap_or(0.0);
        println!(
            "  [{:.3}] {}",
            rel,
            &n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(60)]
        );
        assert!(
            rel <= prev_rel + 0.001,
            "Noder ska vara sorterade fallande efter relevans: {:.3} > {:.3}",
            prev_rel,
            rel
        );
        prev_rel = rel;
    }
}

// ─── Phase 3.3: XHR detection ───────────────────────────────────────────

#[test]
fn test_phase33_xhr_urls_detected() {
    // Verify that fetch()/XHR patterns are detected in JavaScript
    let html = r##"<html><body>
        <div id="data"></div>
        <script>
            fetch('/api/products?page=1')
                .then(r => r.json())
                .then(data => {
                    document.getElementById('data').innerHTML = data.html;
                });
            var xhr = new XMLHttpRequest();
            xhr.open('GET', '/api/categories');
            xhr.send();
        </script>
    </body></html>"##;

    let xhr_json = detect_xhr_urls(html);
    let xhr: serde_json::Value = serde_json::from_str(&xhr_json).unwrap();
    let urls = xhr.as_array().unwrap();

    println!("Phase 3.3: XHR detection");
    for u in urls {
        println!(
            "  {} {}",
            u["method"].as_str().unwrap_or("?"),
            u["url"].as_str().unwrap_or("?")
        );
    }

    assert!(
        urls.len() >= 2,
        "Ska detektera minst 2 XHR-URLs (fetch + XMLHttpRequest), fick {}",
        urls.len()
    );

    let has_products = urls
        .iter()
        .any(|u| u["url"].as_str().unwrap_or("").contains("/api/products"));
    assert!(has_products, "Ska hitta /api/products URL");
}

// ─── Phase 4.1: Stealth headers ─────────────────────────────────────────

#[test]
fn test_phase41_user_agent_no_aetheragent_identifier() {
    // Verify that default FetchConfig user agent doesn't contain "AetherAgent"
    let config = types::FetchConfig::default();
    println!("Phase 4.1: User-Agent = {}", config.user_agent);

    assert!(
        !config.user_agent.contains("AetherAgent"),
        "User-Agent ska INTE innehålla 'AetherAgent' — triggar bot-detection"
    );
    assert!(
        config.user_agent.contains("Chrome/"),
        "User-Agent ska se ut som riktig Chrome"
    );
    assert!(
        config.user_agent.contains("Mozilla/5.0"),
        "User-Agent ska ha Mozilla prefix"
    );
}

// ─── Phase 4.2: Bot-blocking detection ──────────────────────────────────

#[test]
fn test_phase42_bot_blocked_flag_in_parse() {
    // Verify that CRFR detects bot-blocked pages
    let blocked_html = r##"<html>
    <head><title>Access Denied</title></head>
    <body>
        <h1>Access Denied</h1>
        <p>You don't have permission to access this resource.</p>
        <p>Reference#18.d3cf5868.1776109080.5287f06</p>
    </body></html>"##;

    let result = parse_crfr_json(
        blocked_html,
        "products buy price",
        "https://blocked.example.com",
        10,
    );

    println!(
        "Phase 4.2: bot_blocked={}, error_detected={}",
        result["bot_blocked"], result["error_detected"]
    );

    // Should detect this as an error/blocked page
    let is_error = result["error_detected"].as_bool().unwrap_or(false);
    let is_blocked = result["bot_blocked"].as_bool().unwrap_or(false);
    assert!(
        is_error || is_blocked,
        "'Access Denied' sida ska flaggas som error eller bot_blocked"
    );
}

#[test]
fn test_phase42_cloudflare_challenge_detected() {
    let cf_html = r##"<html>
    <head><title>Just a moment...</title></head>
    <body>
        <div class="cf-browser-verification">
            <h2>Checking your browser before accessing the website.</h2>
            <noscript>Please enable JavaScript and cookies to continue.</noscript>
        </div>
        <script src="/cdn-cgi/challenge-platform/scripts/jsd/main.js"></script>
    </body></html>"##;

    let result = parse_crfr_json(
        cf_html,
        "find products",
        "https://cf-blocked.example.com",
        10,
    );

    println!(
        "Phase 4.2 CF: bot_blocked={}, suggested_action={}",
        result["bot_blocked"], result["suggested_action"]
    );

    let is_blocked = result["bot_blocked"].as_bool().unwrap_or(false);
    assert!(
        is_blocked,
        "Cloudflare challenge ska flaggas som bot_blocked"
    );
}

// ─── Phase 4.3: Cookie consent filtering ────────────────────────────────

#[test]
fn test_phase43_cookie_consent_filtered() {
    let html = r##"<html><body>
        <div class="cookie-banner">
            <p>We use cookies to improve your experience. By using our site you accept our cookie policy.</p>
            <button>Accept all cookies</button>
            <button>Reject all cookies</button>
        </div>
        <main>
            <h1>Latest Tech News</h1>
            <p>AI breakthrough: New model achieves 95% accuracy on complex reasoning tasks.</p>
            <p>Quantum computing milestone: 1000-qubit processor demonstrated.</p>
        </main>
    </body></html>"##;

    let result = parse_crfr_json(
        html,
        "AI breakthrough technology news quantum computing",
        "https://news.example.com",
        10,
    );
    let nodes = result["nodes"].as_array().unwrap();

    println!("Phase 4.3: {} nodes", nodes.len());
    for n in nodes {
        println!(
            "  [{}] {}",
            n["role"].as_str().unwrap_or("?"),
            &n["label"].as_str().unwrap_or("")[..n["label"].as_str().unwrap_or("").len().min(60)]
        );
    }

    // Cookie consent text should be filtered out
    let has_cookie = nodes.iter().any(|n| {
        let l = n["label"].as_str().unwrap_or("").to_lowercase();
        l.contains("we use cookies") && l.contains("accept")
    });
    assert!(
        !has_cookie,
        "Cookie consent-text ska filtreras bort från resultat"
    );

    // Actual content should remain
    let has_content = nodes.iter().any(|n| {
        let l = n["label"].as_str().unwrap_or("");
        l.contains("AI breakthrough") || l.contains("quantum")
    });
    assert!(has_content, "Riktigt content ska finnas kvar");
}
