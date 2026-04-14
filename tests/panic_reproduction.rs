/// Panic reproduction test — simulates CNN/Spiegel/Aftonbladet large HTML pages
/// that crash the production server with "task panicked".
///
/// Run: cargo test --test panic_reproduction -- --nocapture
use aether_agent::*;

/// Generate HTML similar to CNN (~3MB with inline JS, nested divs, SSR data)
fn generate_large_html(size_mb: usize) -> String {
    let target_bytes = size_mb * 1024 * 1024;
    let mut html = String::with_capacity(target_bytes + 1024);

    html.push_str("<!DOCTYPE html><html><head><title>Large News Site</title>");
    html.push_str("<meta name=\"description\" content=\"Breaking news, latest news and videos\">");
    html.push_str("</head><body>");

    html.push_str("<nav><ul>");
    for i in 0..20 {
        html.push_str(&format!(
            "<li><a href=\"/section-{i}\">Section {i}</a></li>"
        ));
    }
    html.push_str("</ul></nav>");

    // SSR JSON data block
    html.push_str("<script type=\"application/json\" id=\"__data\">{\"sections\":[");
    let mut section_count = 0;
    while html.len() < target_bytes / 3 {
        if section_count > 0 {
            html.push(',');
        }
        html.push_str(&format!(
            "{{\"id\":{s},\"title\":\"Article {s}: Breaking news headline\",\"url\":\"/article/{s}\"}}",
            s = section_count
        ));
        section_count += 1;
    }
    html.push_str("]}</script>");

    // Inline JavaScript
    html.push_str("<script>");
    while html.len() < target_bytes * 2 / 3 {
        html.push_str("var _d={};_d.m=function(e){return e};document.getElementById('root');");
    }
    html.push_str("</script>");

    // Content articles
    html.push_str("<div id=\"root\">");
    let mut a = 0;
    while html.len() < target_bytes {
        html.push_str(&format!(
            "<article><h2>Headline {a}</h2><p>Content {a}.</p></article>",
            a = a
        ));
        a += 1;
    }
    html.push_str("</div></body></html>");
    html
}

/// Generate deeply nested HTML (can cause stack overflow in recursive parsers)
fn generate_deeply_nested_html(depth: usize) -> String {
    let mut html = String::with_capacity(depth * 20 + 200);
    html.push_str("<!DOCTYPE html><html><head><title>Deep</title></head><body>");
    for i in 0..depth {
        html.push_str(&format!("<div class=\"l-{i}\">"));
    }
    html.push_str("<p>Deep content</p>");
    for _ in 0..depth {
        html.push_str("</div>");
    }
    html.push_str("</body></html>");
    html
}

#[test]
fn test_1mb_html_does_not_panic() {
    let html = generate_large_html(1);
    println!("1MB HTML: {} bytes", html.len());
    let result = parse_crfr(
        &html,
        "breaking news headlines",
        "https://large-1mb.test",
        10,
        false,
        "json",
    );
    let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
    println!("1MB: nodes={}, total={}", p["node_count"], p["total_nodes"]);
    assert!(p["node_count"].as_u64().unwrap_or(0) > 0);
}

#[test]
fn test_3mb_html_does_not_panic() {
    let html = generate_large_html(3);
    println!("3MB HTML: {} bytes", html.len());
    let result = parse_crfr(
        &html,
        "breaking news headlines",
        "https://large-3mb.test",
        10,
        false,
        "json",
    );
    let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
    println!("3MB: nodes={}, total={}", p["node_count"], p["total_nodes"]);
}

#[test]
fn test_5mb_html_does_not_panic() {
    let html = generate_large_html(5);
    println!("5MB HTML: {} bytes", html.len());
    let result = parse_crfr(
        &html,
        "breaking news headlines",
        "https://large-5mb.test",
        10,
        false,
        "json",
    );
    let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
    println!("5MB: nodes={}, total={}", p["node_count"], p["total_nodes"]);
}

#[test]
fn test_deep_nesting_500() {
    let html = generate_deeply_nested_html(500);
    println!("500-deep: {} bytes", html.len());
    let result = parse_crfr(
        &html,
        "deep content",
        "https://deep-500.test",
        5,
        false,
        "json",
    );
    let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
    println!("500-deep: nodes={}", p["node_count"]);
}

#[test]
fn test_deep_nesting_2000() {
    let html = generate_deeply_nested_html(2000);
    println!("2000-deep: {} bytes", html.len());
    let result = parse_crfr(
        &html,
        "deep content",
        "https://deep-2000.test",
        5,
        false,
        "json",
    );
    let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
    println!("2000-deep: nodes={}", p["node_count"]);
}

/// Simulate Docker 2MB stack — the exact production environment
#[test]
fn test_3mb_html_on_2mb_stack() {
    let html = generate_large_html(3);
    println!("3MB HTML on 2MB stack: {} bytes", html.len());

    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            parse_crfr(
                &html,
                "breaking news headlines",
                "https://stack-test.test",
                10,
                false,
                "json",
            )
        })
        .expect("spawn");

    match handle.join() {
        Ok(result) => {
            let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
            println!("2MB stack OK: nodes={}", p["node_count"]);
        }
        Err(e) => {
            panic!(
                "PANICKED on 2MB stack! This is the production bug. Error: {:?}",
                e
            );
        }
    }
}

/// Simulate Docker 2MB stack with deep nesting
#[test]
fn test_deep_nesting_on_2mb_stack() {
    let html = generate_deeply_nested_html(2000);
    println!("2000-deep on 2MB stack: {} bytes", html.len());

    let handle = std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(move || {
            parse_crfr(
                &html,
                "deep content",
                "https://deep-stack-test.test",
                5,
                false,
                "json",
            )
        })
        .expect("spawn");

    match handle.join() {
        Ok(result) => {
            let p: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
            println!("2MB stack deep OK: nodes={}", p["node_count"]);
        }
        Err(e) => {
            panic!("PANICKED on 2MB stack deep nesting! Error: {:?}", e);
        }
    }
}
