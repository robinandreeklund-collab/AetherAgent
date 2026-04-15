/// TOON (Token-Oriented Object Notation) format tests
use aether_agent::*;

#[test]
fn test_toon_output_format() {
    let html = r##"<html><body>
        <h1>Breaking News</h1>
        <p class="price">$49.99</p>
        <a href="/article/123">Read full article about technology</a>
        <p>Scientists discovered a new method for quantum computing.</p>
    </body></html>"##;

    let result = parse_crfr(
        html,
        "news article technology quantum",
        "https://toon-test.example.com",
        10,
        false,
        "toon",
    );

    println!("TOON output:\n{}", result);

    // Must start with header line ~count|total|time
    assert!(result.starts_with('~'), "TOON must start with ~ header");
    let first_line = result.lines().next().unwrap();
    assert!(
        first_line.contains('|') && first_line.contains("ms"),
        "Header must have ~count|total|timems format: {}",
        first_line
    );

    // Must have at least one node line with role code
    let node_lines: Vec<&str> = result
        .lines()
        .filter(|l| !l.starts_with('~') && !l.starts_with('#') && !l.is_empty())
        .collect();
    assert!(
        !node_lines.is_empty(),
        "TOON must have at least one node line"
    );

    // Role codes must be single uppercase letter
    for line in &node_lines {
        let first_char = line.chars().next().unwrap();
        assert!(
            first_char.is_ascii_uppercase(),
            "Node line must start with role code (A-Z): {}",
            line
        );
    }

    // Must contain pipe separators
    for line in &node_lines {
        assert!(
            line.contains('|'),
            "Node line must have | separators: {}",
            line
        );
    }

    // No JSON braces or quotes
    assert!(!result.contains('{'), "TOON must not contain JSON braces");
    assert!(!result.contains('}'), "TOON must not contain JSON braces");
    assert!(
        !result.contains("\"role\""),
        "TOON must not contain JSON field names"
    );
}

#[test]
fn test_toon_is_smaller_than_json_and_markdown() {
    let html = r##"<html><body>
        <h1>Product Page</h1>
        <p class="price">$199.00</p>
        <p>The best laptop for professionals with M4 chip.</p>
        <a href="/buy">Buy Now</a>
        <a href="/reviews">Read Reviews</a>
        <p>Free shipping on orders over $50.</p>
        <h2>Specifications</h2>
        <p>16GB RAM, 512GB SSD, 14-inch display.</p>
    </body></html>"##;

    let url = "https://toon-size-test.example.com";
    let goal = "product price buy laptop specifications";

    let json = parse_crfr(html, goal, url, 10, false, "json");
    let md = parse_crfr(html, goal, url, 10, false, "markdown");
    let toon = parse_crfr(html, goal, url, 10, false, "toon");

    println!("JSON:     {} chars", json.len());
    println!("Markdown: {} chars", md.len());
    println!("TOON:     {} chars", toon.len());
    println!(
        "TOON savings vs JSON: {:.0}%",
        (1.0 - toon.len() as f64 / json.len() as f64) * 100.0
    );
    println!(
        "TOON savings vs MD:   {:.0}%",
        (1.0 - toon.len() as f64 / md.len() as f64) * 100.0
    );
    println!("\nTOON output:\n{}", toon);

    assert!(
        toon.len() < md.len(),
        "TOON ({}) must be smaller than Markdown ({})",
        toon.len(),
        md.len()
    );
    assert!(
        toon.len() < json.len(),
        "TOON ({}) must be smaller than JSON ({})",
        toon.len(),
        json.len()
    );
}
