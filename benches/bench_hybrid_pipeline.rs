/// Benchmark: Hybrid Scoring Pipeline vs Legacy Scoring
///
/// Jämför hastighet och korrekthet mellan:
/// - Legacy: single-pass embedding (parse_top_nodes)
/// - Hybrid: TF-IDF → HDC → Embedding (parse_top_nodes_hybrid)
///
/// Run: cargo run --bin aether-hybrid-bench
use std::time::Instant;

use aether_agent::{parse_top_nodes, parse_top_nodes_hybrid};

// ─── HTML Fixtures ───────────────────────────────────────────────────────────

fn simple_page() -> &'static str {
    r##"<html><head><title>Simple</title></head><body>
        <h1>Hello World</h1>
        <p>A simple paragraph about population.</p>
        <a href="/about">About</a>
    </body></html>"##
}

fn medium_page() -> String {
    let mut html = String::from("<html><body>\n<h1>Population Statistics</h1>\n");
    html.push_str("<main>\n");
    for i in 0..50 {
        html.push_str(&format!(
            "<div class='item'><h3>Region {i}</h3><p>{} inhabitants in region {i}</p><a href='/region/{i}'>Details</a></div>\n",
            10000 + i * 137
        ));
    }
    html.push_str("</main>\n");
    html.push_str("<nav>");
    for i in 0..20 {
        html.push_str(&format!("<a href='/page/{i}'>Page {i}</a>\n"));
    }
    html.push_str("</nav>\n");
    html.push_str("<footer><p>Cookie settings</p><button>Accept cookies</button></footer>\n");
    html.push_str("</body></html>");
    html
}

fn large_page() -> String {
    let mut html = String::from("<html><body>\n");
    html.push_str("<header><h1>Large Data Portal</h1><nav>");
    for i in 0..30 {
        html.push_str(&format!("<a href='/nav/{i}'>Nav {i}</a>\n"));
    }
    html.push_str("</nav></header>\n<main>\n");

    for section in 0..20 {
        html.push_str(&format!("<section><h2>Section {section}</h2>\n"));
        for item in 0..25 {
            let id = section * 25 + item;
            html.push_str(&format!(
                "<div class='card'><h3>Item {id}</h3><p>Data point {} for analysis category {section}</p><span class='stat'>{} units</span><button>View details</button></div>\n",
                id * 42 + 100,
                id * 7 + 50
            ));
        }
        html.push_str("</section>\n");
    }

    html.push_str("</main>\n<footer>");
    for i in 0..15 {
        html.push_str(&format!("<a href='/footer/{i}'>Footer link {i}</a>\n"));
    }
    html.push_str("<button>Cookie consent</button></footer>\n</body></html>");
    html
}

// ─── Benchmark Runner ────────────────────────────────────────────────────────

struct BenchResult {
    name: String,
    legacy_us: u64,
    hybrid_us: u64,
    legacy_nodes: usize,
    hybrid_nodes: usize,
    speedup: f64,
}

fn bench_parse(name: &str, html: &str, goal: &str, top_n: u32, iterations: u32) -> BenchResult {
    // Warmup
    let _ = parse_top_nodes(html, goal, "https://bench.test", top_n);
    let _ = parse_top_nodes_hybrid(html, goal, "https://bench.test", top_n);

    // Legacy
    let start = Instant::now();
    let mut legacy_json = String::new();
    for _ in 0..iterations {
        legacy_json = parse_top_nodes(html, goal, "https://bench.test", top_n);
    }
    let legacy_total = start.elapsed().as_micros() as u64;

    // Hybrid
    let start = Instant::now();
    let mut hybrid_json = String::new();
    for _ in 0..iterations {
        hybrid_json = parse_top_nodes_hybrid(html, goal, "https://bench.test", top_n);
    }
    let hybrid_total = start.elapsed().as_micros() as u64;

    let legacy_us = legacy_total / iterations as u64;
    let hybrid_us = hybrid_total / iterations as u64;

    // Parse node counts
    let legacy_parsed: serde_json::Value = serde_json::from_str(&legacy_json).unwrap_or_default();
    let hybrid_parsed: serde_json::Value = serde_json::from_str(&hybrid_json).unwrap_or_default();

    let legacy_nodes = legacy_parsed["top_nodes"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    let hybrid_nodes = hybrid_parsed["top_nodes"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    let speedup = if hybrid_us > 0 {
        legacy_us as f64 / hybrid_us as f64
    } else {
        0.0
    };

    BenchResult {
        name: name.to_string(),
        legacy_us,
        hybrid_us,
        legacy_nodes,
        hybrid_nodes,
        speedup,
    }
}

fn bench_correctness(html: &str, goal: &str, expected_keyword: &str) -> (bool, bool) {
    let legacy_json = parse_top_nodes(html, goal, "https://bench.test", 5);
    let hybrid_json = parse_top_nodes_hybrid(html, goal, "https://bench.test", 5);

    let legacy: serde_json::Value = serde_json::from_str(&legacy_json).unwrap_or_default();
    let hybrid: serde_json::Value = serde_json::from_str(&hybrid_json).unwrap_or_default();

    let legacy_has = legacy["top_nodes"]
        .as_array()
        .map(|nodes| {
            nodes.iter().take(3).any(|n| {
                n["label"]
                    .as_str()
                    .map(|l| l.to_lowercase().contains(expected_keyword))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    let hybrid_has = hybrid["top_nodes"]
        .as_array()
        .map(|nodes| {
            nodes.iter().take(3).any(|n| {
                n["label"]
                    .as_str()
                    .map(|l| l.to_lowercase().contains(expected_keyword))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    (legacy_has, hybrid_has)
}

fn main() {
    println!("═══════════════════════════════════════════════════════════════════");
    println!("  AetherAgent — Hybrid Scoring Pipeline Benchmark");
    println!("  Legacy (single-pass) vs Hybrid (TF-IDF → HDC → Embedding)");
    println!("═══════════════════════════════════════════════════════════════════\n");

    let simple = simple_page();
    let medium = medium_page();
    let large = large_page();

    // ─── Speed Benchmarks ────────────────────────────────────────────────────

    println!("── Speed Comparison (avg over 50 iterations) ──\n");
    println!(
        "{:<25} {:>12} {:>12} {:>8} {:>8} {:>8}",
        "Scenario", "Legacy (µs)", "Hybrid (µs)", "Speedup", "L-nodes", "H-nodes"
    );
    println!("{}", "-".repeat(78));

    let results = vec![
        bench_parse("Simple (3 nodes)", &simple, "population", 5, 50),
        bench_parse("Medium (50 items)", &medium, "inhabitants region", 10, 50),
        bench_parse("Medium top_3", &medium, "inhabitants region", 3, 50),
        bench_parse("Large (500 items)", &large, "data analysis units", 10, 20),
        bench_parse("Large top_5", &large, "data analysis units", 5, 20),
    ];

    for r in &results {
        println!(
            "{:<25} {:>12} {:>12} {:>7.2}x {:>8} {:>8}",
            r.name, r.legacy_us, r.hybrid_us, r.speedup, r.legacy_nodes, r.hybrid_nodes
        );
    }

    // ─── Correctness Comparison ──────────────────────────────────────────────

    println!("\n── Correctness Comparison ──\n");
    println!("{:<40} {:>10} {:>10}", "Test case", "Legacy", "Hybrid");
    println!("{}", "-".repeat(62));

    let correctness_tests = vec![
        (
            "Population in medium page",
            medium.as_str(),
            "inhabitants",
            "inhabitants",
        ),
        (
            "Data in large page",
            large.as_str(),
            "data analysis",
            "data",
        ),
        ("Simple population", &simple, "population", "population"),
    ];

    let mut legacy_correct = 0;
    let mut hybrid_correct = 0;

    for (name, html, goal, keyword) in &correctness_tests {
        let (leg, hyb) = bench_correctness(html, goal, keyword);
        let leg_str = if leg { "PASS" } else { "MISS" };
        let hyb_str = if hyb { "PASS" } else { "MISS" };
        println!("{:<40} {:>10} {:>10}", name, leg_str, hyb_str);
        if leg {
            legacy_correct += 1;
        }
        if hyb {
            hybrid_correct += 1;
        }
    }

    println!(
        "\nCorrectness score: Legacy {}/{}, Hybrid {}/{}",
        legacy_correct,
        correctness_tests.len(),
        hybrid_correct,
        correctness_tests.len()
    );

    // ─── Pipeline Breakdown ──────────────────────────────────────────────────

    println!("\n── Hybrid Pipeline Breakdown (Large Page) ──\n");

    let hybrid_json =
        parse_top_nodes_hybrid(&large, "data analysis units", "https://bench.test", 10);
    let hybrid: serde_json::Value = serde_json::from_str(&hybrid_json).unwrap_or_default();

    if let Some(pipeline) = hybrid.get("pipeline") {
        println!("  TF-IDF build:      {:>6} µs", pipeline["build_tfidf_us"]);
        println!("  HDC build:         {:>6} µs", pipeline["build_hdc_us"]);
        println!("  TF-IDF query:      {:>6} µs", pipeline["query_tfidf_us"]);
        println!("  HDC prune:         {:>6} µs", pipeline["prune_hdc_us"]);
        println!("  Embedding score:   {:>6} µs", pipeline["score_embed_us"]);
        println!(
            "  Total pipeline:    {:>6} µs",
            pipeline["total_pipeline_us"]
        );
        println!("\n  TF-IDF candidates: {}", pipeline["tfidf_candidates"]);
        println!("  HDC survivors:     {}", pipeline["hdc_survivors"]);
        println!("  Final scored:      {}", pipeline["final_scored"]);
    }

    // ─── Amortized Query Time ─────────────────────────────────────────────

    println!("\n── Amortized Query Time (build excluded) ──\n");
    if let Some(pipeline) = hybrid.get("pipeline") {
        let build_us = pipeline["build_tfidf_us"].as_u64().unwrap_or(0)
            + pipeline["build_hdc_us"].as_u64().unwrap_or(0);
        let query_us = pipeline["query_tfidf_us"].as_u64().unwrap_or(0)
            + pipeline["prune_hdc_us"].as_u64().unwrap_or(0)
            + pipeline["score_embed_us"].as_u64().unwrap_or(0);
        let total_us = pipeline["total_pipeline_us"].as_u64().unwrap_or(0);

        println!(
            "  Build phase:       {:>6} µs (cached per URL, one-time cost)",
            build_us
        );
        println!("  Query phase:       {:>6} µs (per goal-query)", query_us);
        println!(
            "  Overhead:          {:>6} µs (index lookup, scoring)",
            total_us - build_us - query_us
        );
        println!();
        println!(
            "  → Med cache: hybrid query = {} µs vs legacy full = {} µs",
            query_us,
            results.last().map(|r| r.legacy_us).unwrap_or(0)
        );

        let legacy_last = results.last().map(|r| r.legacy_us).unwrap_or(1) as f64;
        if query_us > 0 {
            println!(
                "  → Amortized speedup: {:.1}x snabbare",
                legacy_last / query_us as f64
            );
        }
    }

    println!("\n═══════════════════════════════════════════════════════════════════");
}
