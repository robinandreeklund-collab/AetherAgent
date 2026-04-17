/// Find exact depth that causes stack overflow
use aether_agent::*;

fn gen_deep(depth: usize) -> String {
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
fn test_depth_50() {
    let r = parse_crfr(
        &gen_deep(50),
        "content",
        "https://d50.test",
        3,
        false,
        "json",
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&r).is_ok(),
        "depth 50 failed"
    );
    println!("depth 50: OK");
}

#[test]
fn test_depth_100() {
    let r = parse_crfr(
        &gen_deep(100),
        "content",
        "https://d100.test",
        3,
        false,
        "json",
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&r).is_ok(),
        "depth 100 failed"
    );
    println!("depth 100: OK");
}

#[test]
fn test_depth_200() {
    let r = parse_crfr(
        &gen_deep(200),
        "content",
        "https://d200.test",
        3,
        false,
        "json",
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&r).is_ok(),
        "depth 200 failed"
    );
    println!("depth 200: OK");
}

#[test]
fn test_depth_300() {
    let r = parse_crfr(
        &gen_deep(300),
        "content",
        "https://d300.test",
        3,
        false,
        "json",
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&r).is_ok(),
        "depth 300 failed"
    );
    println!("depth 300: OK");
}
