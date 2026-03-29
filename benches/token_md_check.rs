/// Compare: raw HTML vs JSON tree vs Markdown output token counts
use aether_agent::{embedding, html_to_markdown, parse_to_semantic_tree, parse_top_nodes};

fn main() {
    let mp = std::env::var("AETHER_EMBEDDING_MODEL")
        .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".into());
    let vp = std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
    if let (Ok(m), Ok(v)) = (std::fs::read(&mp), std::fs::read_to_string(&vp)) {
        let _ = embedding::init_global(&m, &v);
        println!("[Embedding loaded]\n");
    }

    let fixtures: Vec<(&str, &str)> = vec![
        ("benches/campfire_fixture.html", "buy the backpack"),
        ("tests/fixtures/01_ecommerce_product.html", "buy iPhone"),
        ("tests/fixtures/03_search_results.html", "find results"),
        ("tests/fixtures/06_news_article.html", "read article"),
        ("tests/fixtures/15_grocery_store.html", "add to cart"),
        (
            "tests/fixtures/17_wiki_article.html",
            "read about the topic",
        ),
        ("tests/fixtures/20_large_catalog.html", "find product"),
        (
            "tests/fixtures/34_ecommerce_cart_review.html",
            "review cart",
        ),
        ("tests/fixtures/42_edge_huge_page.html", "find product"),
        ("tests/fixtures/45_complex_dashboard.html", "view reports"),
    ];

    println!(
        "{:<42} {:>7} {:>7} {:>7} {:>7} {:>8} {:>8}",
        "Fixture", "HTML", "JSON", "Top-5", "MD", "JSON%", "MD Save%"
    );
    println!("{}", "-".repeat(95));

    let mut total_html = 0usize;
    let mut total_json = 0usize;
    let mut total_top5 = 0usize;
    let mut total_md = 0usize;

    for (path, goal) in &fixtures {
        let html = match std::fs::read_to_string(path) {
            Ok(h) => h,
            Err(_) => continue,
        };
        let url = "https://test.se";
        let html_tok = html.len() / 4;

        let json_tree = parse_to_semantic_tree(&html, goal, url);
        let json_tok = json_tree.len() / 4;

        let top5 = parse_top_nodes(&html, goal, url, 5);
        let top5_tok = top5.len() / 4;

        let md = html_to_markdown(&html, goal, url);
        let md_tok = md.len() / 4;

        let json_pct = if html_tok > 0 {
            json_tok as f64 / html_tok as f64 * 100.0
        } else {
            0.0
        };
        let md_save = if html_tok > 0 {
            (1.0 - md_tok as f64 / html_tok as f64) * 100.0
        } else {
            0.0
        };

        total_html += html_tok;
        total_json += json_tok;
        total_top5 += top5_tok;
        total_md += md_tok;

        let short = path
            .rsplit('/')
            .next()
            .unwrap_or(path)
            .chars()
            .take(40)
            .collect::<String>();
        println!(
            "{:<42} {:>7} {:>7} {:>7} {:>7} {:>7.0}% {:>7.1}%",
            short, html_tok, json_tok, top5_tok, md_tok, json_pct, md_save
        );
    }

    println!("{}", "-".repeat(95));
    let total_json_pct = total_json as f64 / total_html as f64 * 100.0;
    let total_md_save = (1.0 - total_md as f64 / total_html as f64) * 100.0;
    println!(
        "{:<42} {:>7} {:>7} {:>7} {:>7} {:>7.0}% {:>7.1}%",
        "TOTAL", total_html, total_json, total_top5, total_md, total_json_pct, total_md_save
    );

    println!("\n=== SUMMARY ===");
    println!("  Raw HTML:      {} tokens (baseline)", total_html);
    println!(
        "  JSON tree:     {} tokens ({:.0}% of HTML — LARGER due to metadata)",
        total_json, total_json_pct
    );
    println!(
        "  Top-5 JSON:    {} tokens ({:.0}% of HTML)",
        total_top5,
        total_top5 as f64 / total_html as f64 * 100.0
    );
    println!(
        "  Markdown:      {} tokens ({:.1}% savings vs HTML)",
        total_md, total_md_save
    );
    println!("\n  → Markdown is the correct output format for LLM token savings");
    println!("  → JSON tree is for structured agent operations (click, fill, extract)");
}
