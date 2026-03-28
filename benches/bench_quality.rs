/// Quality Benchmark — 5 real sites with real questions
///
/// All engines get the exact same pre-fetched HTML.
/// No network overhead differences.
///
/// Sites & questions:
/// 1. apple.com      → "find iPhone price"
/// 2. Hacker News    → "find latest news articles"
/// 3. books.toscrape → "find book titles and prices"
/// 4. lobste.rs      → "find technology articles"
/// 5. rust-lang.org  → "download and install Rust"
use std::time::Instant;

use aether_agent::{embedding, html_to_markdown, parse_to_semantic_tree, parse_top_nodes};

struct QualityResult {
    site: &'static str,
    goal: &'static str,
    html_tokens: usize,
    json_tokens: usize,
    md_tokens: usize,
    top5_tokens: usize,
    node_count: usize,
    parse_ms: f64,
    md_savings_pct: f64,
    top5_savings_pct: f64,
    found_relevant: bool,
    top_node_label: String,
    top_node_relevance: f64,
}

fn count_json_nodes(json: &str) -> usize {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    fn count(v: &serde_json::Value) -> usize {
        let mut c = 1;
        if let Some(children) = v["children"].as_array() {
            for ch in children {
                c += count(ch);
            }
        }
        c
    }
    if let Some(nodes) = v["nodes"].as_array() {
        nodes.iter().map(|n| count(n)).sum()
    } else {
        0
    }
}

fn top_node_info(json: &str) -> (String, f64) {
    let v: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    if let Some(nodes) = v["top_nodes"].as_array() {
        if let Some(first) = nodes.first() {
            let label = first["label"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(60)
                .collect();
            let rel = first["relevance"].as_f64().unwrap_or(0.0);
            return (label, rel);
        }
    }
    ("(none)".to_string(), 0.0)
}

fn main() {
    // Init embedding
    let mp = std::env::var("AETHER_EMBEDDING_MODEL")
        .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".into());
    let vp = std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
    if let (Ok(m), Ok(v)) = (std::fs::read(&mp), std::fs::read_to_string(&vp)) {
        let _ = embedding::init_global(&m, &v);
        println!("[Embedding loaded]");
    } else {
        println!("[WARNING: Embedding not loaded — running without]");
    }

    let sites: Vec<(&str, &str, &str, &str)> = vec![
        (
            "apple.com",
            "/tmp/live_bench/apple.html",
            "find iPhone price",
            "https://www.apple.com",
        ),
        (
            "Hacker News",
            "/tmp/live_bench/hackernews.html",
            "find latest news articles",
            "https://news.ycombinator.com",
        ),
        (
            "books.toscrape",
            "/tmp/live_bench/books.html",
            "find book titles and prices",
            "https://books.toscrape.com",
        ),
        (
            "lobste.rs",
            "/tmp/live_bench/lobsters.html",
            "find technology articles",
            "https://lobste.rs",
        ),
        (
            "rust-lang.org",
            "/tmp/live_bench/rustlang.html",
            "download and install Rust",
            "https://www.rust-lang.org",
        ),
    ];

    println!("\n╔════════════════════════════════════════════════════════════════════════╗");
    println!("║       AetherAgent Quality Benchmark — 5 Real Sites, Real Questions    ║");
    println!("╚════════════════════════════════════════════════════════════════════════╝\n");

    let mut results: Vec<QualityResult> = Vec::new();

    for (site, path, goal, url) in &sites {
        let html = match std::fs::read_to_string(path) {
            Ok(h) => h,
            Err(e) => {
                println!("  SKIP {site}: {e}");
                continue;
            }
        };

        let html_tokens = html.len() / 4;

        // Parse: full tree
        let start = Instant::now();
        let json = parse_to_semantic_tree(&html, goal, url);
        let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
        let json_tokens = json.len() / 4;
        let node_count = count_json_nodes(&json);

        // Markdown
        let md = html_to_markdown(&html, goal, url);
        let md_tokens = md.len() / 4;

        // Top-5
        let top5 = parse_top_nodes(&html, goal, url, 5);
        let top5_tokens = top5.len() / 4;

        let (top_label, top_rel) = top_node_info(&top5);

        let md_savings = if html_tokens > 0 {
            (1.0 - md_tokens as f64 / html_tokens as f64) * 100.0
        } else {
            0.0
        };
        let top5_savings = if html_tokens > 0 {
            (1.0 - top5_tokens as f64 / html_tokens as f64) * 100.0
        } else {
            0.0
        };

        let found = top_rel > 0.1;

        results.push(QualityResult {
            site,
            goal,
            html_tokens,
            json_tokens,
            md_tokens,
            top5_tokens,
            node_count,
            parse_ms,
            md_savings_pct: md_savings,
            top5_savings_pct: top5_savings,
            found_relevant: found,
            top_node_label: top_label.clone(),
            top_node_relevance: top_rel,
        });

        println!("── {site} ──────────────────────────────────────────");
        println!("  Goal:          \"{goal}\"");
        println!("  Parse time:    {parse_ms:.1}ms");
        println!("  HTML tokens:   {html_tokens}");
        println!(
            "  JSON tokens:   {json_tokens} ({:.0}% of HTML)",
            json_tokens as f64 / html_tokens as f64 * 100.0
        );
        println!("  MD tokens:     {md_tokens} ({md_savings:.1}% savings)");
        println!("  Top-5 tokens:  {top5_tokens} ({top5_savings:.1}% savings)");
        println!("  Nodes:         {node_count}");
        println!("  Top node:      \"{top_label}\" (relevance: {top_rel:.3})");
        println!("  Found goal:    {}", if found { "YES ✓" } else { "NO ✗" });
        println!();

        // Show markdown preview (first 300 chars)
        let md_preview: String = md.chars().take(300).collect();
        println!("  Markdown preview:");
        for line in md_preview.lines().take(10) {
            println!("    {line}");
        }
        println!("    ...\n");
    }

    // Summary table
    println!("╔════════════════════════════════════════════════════════════════════════════════╗");
    println!("║                              SUMMARY                                          ║");
    println!("╠════════════════════════════════════════════════════════════════════════════════╣");
    println!(
        "║  {:<16} {:>6} {:>6} {:>6} {:>6} {:>8} {:>7} {:>5} ║",
        "Site", "HTML", "MD", "Top5", "Nodes", "Parse", "MD Sav", "Found"
    );
    println!(
        "║  {:<16} {:>6} {:>6} {:>6} {:>6} {:>8} {:>7} {:>5} ║",
        "", "tok", "tok", "tok", "", "ms", "%", ""
    );
    println!("╟────────────────────────────────────────────────────────────────────────────────╢");
    for r in &results {
        println!(
            "║  {:<16} {:>6} {:>6} {:>6} {:>6} {:>8.1} {:>6.1}% {:>5} ║",
            r.site,
            r.html_tokens,
            r.md_tokens,
            r.top5_tokens,
            r.node_count,
            r.parse_ms,
            r.md_savings_pct,
            if r.found_relevant { "YES" } else { "NO" }
        );
    }
    println!("╟────────────────────────────────────────────────────────────────────────────────╢");

    let total_html: usize = results.iter().map(|r| r.html_tokens).sum();
    let total_md: usize = results.iter().map(|r| r.md_tokens).sum();
    let total_top5: usize = results.iter().map(|r| r.top5_tokens).sum();
    let avg_parse: f64 = results.iter().map(|r| r.parse_ms).sum::<f64>() / results.len() as f64;
    let found_count = results.iter().filter(|r| r.found_relevant).count();
    let overall_md_save = (1.0 - total_md as f64 / total_html as f64) * 100.0;
    let overall_top5_save = (1.0 - total_top5 as f64 / total_html as f64) * 100.0;

    println!(
        "║  {:<16} {:>6} {:>6} {:>6} {:>6} {:>8.1} {:>6.1}% {:>3}/{} ║",
        "TOTAL",
        total_html,
        total_md,
        total_top5,
        "",
        avg_parse,
        overall_md_save,
        found_count,
        results.len()
    );
    println!("╚════════════════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Markdown savings:  {overall_md_save:.1}% ({total_html} → {total_md} tokens)");
    println!("  Top-5 savings:     {overall_top5_save:.1}% ({total_html} → {total_top5} tokens)");
    println!("  Goal found:        {found_count}/{} sites", results.len());
    println!("  Avg parse time:    {avg_parse:.1}ms");
}
