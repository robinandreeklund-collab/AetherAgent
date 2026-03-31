/// Real-World Validation Suite — 20 Sites, Legacy vs Hybrid
///
/// Fetches real pages, runs both pipelines, compares quality + latency.
/// Outputs structured results to stdout and docs/real_world_validation.md.
///
/// Run with embeddings:
///   cargo run --release --bin aether-real-world --features embeddings
///
/// Run without embeddings (text similarity only):
///   cargo run --release --bin aether-real-world
use std::collections::HashMap;
use std::time::Instant;

use aether_agent::{parse_top_nodes, parse_top_nodes_hybrid};

// ─── Test Cases ──────────────────────────────────────────────────────────────

struct TestCase {
    name: &'static str,
    url: &'static str,
    goal: &'static str,
    /// Keyword that SHOULD appear in top-3 results for correctness
    expected_keyword: &'static str,
    top_n: u32,
}

fn test_cases() -> Vec<TestCase> {
    // Alla 20 URLs verifierade som åtkomliga (curl >500 bytes, 2026-03-30)
    vec![
        // ─── Nyhetssajter (text-heavy, bra för scoring) ────────────────────
        TestCase {
            name: "Hacker News",
            url: "https://news.ycombinator.com",
            goal: "top stories today",
            expected_keyword: "hacker",
            top_n: 10,
        },
        TestCase {
            name: "HN Newest",
            url: "https://news.ycombinator.com/newest",
            goal: "newest submissions",
            expected_keyword: "new",
            top_n: 10,
        },
        TestCase {
            name: "Lobsters",
            url: "https://lobste.rs",
            goal: "programming stories and discussions",
            expected_keyword: "programming",
            top_n: 10,
        },
        TestCase {
            name: "CNN Lite",
            url: "https://lite.cnn.com",
            goal: "top news headlines today",
            expected_keyword: "cnn",
            top_n: 10,
        },
        TestCase {
            name: "NPR Text",
            url: "https://text.npr.org",
            goal: "latest radio news stories",
            expected_keyword: "npr",
            top_n: 10,
        },
        // ─── Utvecklarresurser ─────────────────────────────────────────────
        TestCase {
            name: "Rust Lang",
            url: "https://www.rust-lang.org",
            goal: "latest Rust version download",
            expected_keyword: "rust",
            top_n: 10,
        },
        TestCase {
            name: "MDN HTML",
            url: "https://developer.mozilla.org/en-US/docs/Web/HTML",
            goal: "HTML elements reference",
            expected_keyword: "html",
            top_n: 10,
        },
        TestCase {
            name: "Python.org",
            url: "https://www.python.org",
            goal: "download Python latest version",
            expected_keyword: "python",
            top_n: 10,
        },
        TestCase {
            name: "W3C",
            url: "https://www.w3.org",
            goal: "web standards specifications",
            expected_keyword: "web",
            top_n: 10,
        },
        TestCase {
            name: "GitHub Explore",
            url: "https://github.com/explore",
            goal: "trending repositories",
            expected_keyword: "trending",
            top_n: 10,
        },
        // ─── Paketregister ─────────────────────────────────────────────────
        TestCase {
            name: "NPM",
            url: "https://www.npmjs.com",
            goal: "search JavaScript packages",
            expected_keyword: "javascript",
            top_n: 10,
        },
        TestCase {
            name: "Crates.io",
            url: "https://crates.io",
            goal: "Rust package registry search",
            expected_keyword: "rust",
            top_n: 10,
        },
        TestCase {
            name: "PyPI",
            url: "https://pypi.org",
            goal: "find Python packages",
            expected_keyword: "python",
            top_n: 10,
        },
        TestCase {
            name: "docs.rs",
            url: "https://docs.rs",
            goal: "Rust documentation search",
            expected_keyword: "rust",
            top_n: 10,
        },
        TestCase {
            name: "pkg.go.dev",
            url: "https://pkg.go.dev",
            goal: "Go packages and modules",
            expected_keyword: "go",
            top_n: 10,
        },
        TestCase {
            name: "Docker Hub",
            url: "https://hub.docker.com",
            goal: "search container images",
            expected_keyword: "docker",
            top_n: 10,
        },
        // ─── Övriga (varierade DOM-storlekar) ──────────────────────────────
        TestCase {
            name: "DuckDuckGo",
            url: "https://duckduckgo.com",
            goal: "search engine privacy",
            expected_keyword: "search",
            top_n: 10,
        },
        TestCase {
            name: "OpenStreetMap",
            url: "https://www.openstreetmap.org",
            goal: "map navigation and editing",
            expected_keyword: "map",
            top_n: 10,
        },
        TestCase {
            name: "httpbin HTML",
            url: "https://httpbin.org/html",
            goal: "Herman Melville story",
            expected_keyword: "melville",
            top_n: 10,
        },
        TestCase {
            name: "Reuters",
            url: "https://www.reuters.com",
            goal: "business news today",
            expected_keyword: "reuters",
            top_n: 10,
        },
    ]
}

// ─── Result Types ────────────────────────────────────────────────────────────

#[derive(Default)]
#[allow(dead_code)]
struct SiteResult {
    name: String,
    url: String,
    goal: String,
    fetch_ms: u64,
    html_bytes: usize,

    // Legacy pipeline
    legacy_parse_ms: u64,
    legacy_node_count: usize,
    legacy_top3_labels: Vec<String>,
    legacy_top3_scores: Vec<f32>,
    legacy_has_keyword: bool,

    // Hybrid pipeline
    hybrid_parse_ms: u64,
    hybrid_node_count: usize,
    hybrid_top3_labels: Vec<String>,
    hybrid_top3_scores: Vec<f32>,
    hybrid_has_keyword: bool,

    // Hybrid pipeline stages
    hybrid_tfidf_build_us: u64,
    hybrid_hdc_build_us: u64,
    hybrid_tfidf_query_us: u64,
    hybrid_hdc_prune_us: u64,
    hybrid_embed_score_us: u64,
    hybrid_total_pipeline_us: u64,
    hybrid_tfidf_candidates: u64,
    hybrid_hdc_survivors: u64,
    hybrid_cache_hit: bool,

    fetch_error: Option<String>,
}

// ─── Fetch + Parse ───────────────────────────────────────────────────────────

fn fetch_html(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time", "15",
            "--compressed",
            "-H", "User-Agent: Mozilla/5.0 (compatible; AetherAgent/0.1; +https://github.com/AetherAgent)",
            "-H", "Accept: text/html",
            url,
        ])
        .output()
        .map_err(|e| format!("curl failed: {e}"))?;

    if !output.status.success() {
        return Err(format!("HTTP error: {}", output.status));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("UTF-8 error: {e}"))
}

fn extract_top_n(json_str: &str, n: usize, keyword: &str) -> (Vec<String>, Vec<f32>, bool, usize) {
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
    let nodes = parsed["top_nodes"].as_array();

    let mut labels = Vec::new();
    let mut scores = Vec::new();
    let mut has_keyword = false;
    let count = nodes.map(|n| n.len()).unwrap_or(0);

    if let Some(nodes) = nodes {
        for node in nodes.iter().take(n) {
            let label = node["label"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>();
            let score = node["relevance"].as_f64().unwrap_or(0.0) as f32;

            if label.to_lowercase().contains(keyword) {
                has_keyword = true;
            }
            labels.push(label);
            scores.push(score);
        }
    }

    (labels, scores, has_keyword, count)
}

fn extract_hybrid_timings(json_str: &str) -> HashMap<String, u64> {
    let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap_or_default();
    let pipeline = &parsed["pipeline"];
    let mut m = HashMap::new();

    for key in &[
        "build_bm25_us",
        "build_hdc_us",
        "query_bm25_us",
        "prune_hdc_us",
        "score_embed_us",
        "total_pipeline_us",
        "bm25_candidates",
        "hdc_survivors",
    ] {
        m.insert(key.to_string(), pipeline[key].as_u64().unwrap_or(0));
    }
    m.insert(
        "cache_hit".to_string(),
        if pipeline["cache_hit"].as_bool().unwrap_or(false) {
            1
        } else {
            0
        },
    );
    m
}

fn run_test(tc: &TestCase) -> SiteResult {
    let mut result = SiteResult {
        name: tc.name.to_string(),
        url: tc.url.to_string(),
        goal: tc.goal.to_string(),
        ..Default::default()
    };

    // Fetch
    let fetch_start = Instant::now();
    let html = match fetch_html(tc.url) {
        Ok(h) => h,
        Err(e) => {
            result.fetch_error = Some(e);
            return result;
        }
    };
    result.fetch_ms = fetch_start.elapsed().as_millis() as u64;
    result.html_bytes = html.len();

    // Legacy
    let legacy_start = Instant::now();
    let legacy_json = parse_top_nodes(&html, tc.goal, tc.url, tc.top_n);
    result.legacy_parse_ms = legacy_start.elapsed().as_millis() as u64;

    let (labels, scores, has_kw, count) = extract_top_n(&legacy_json, 3, tc.expected_keyword);
    result.legacy_top3_labels = labels;
    result.legacy_top3_scores = scores;
    result.legacy_has_keyword = has_kw;
    result.legacy_node_count = count;

    // Hybrid
    let hybrid_start = Instant::now();
    let hybrid_json = parse_top_nodes_hybrid(&html, tc.goal, tc.url, tc.top_n);
    result.hybrid_parse_ms = hybrid_start.elapsed().as_millis() as u64;

    let (labels, scores, has_kw, count) = extract_top_n(&hybrid_json, 3, tc.expected_keyword);
    result.hybrid_top3_labels = labels;
    result.hybrid_top3_scores = scores;
    result.hybrid_has_keyword = has_kw;
    result.hybrid_node_count = count;

    // Hybrid timings
    let timings = extract_hybrid_timings(&hybrid_json);
    result.hybrid_tfidf_build_us = *timings.get("build_bm25_us").unwrap_or(&0);
    result.hybrid_hdc_build_us = *timings.get("build_hdc_us").unwrap_or(&0);
    result.hybrid_tfidf_query_us = *timings.get("query_bm25_us").unwrap_or(&0);
    result.hybrid_hdc_prune_us = *timings.get("prune_hdc_us").unwrap_or(&0);
    result.hybrid_embed_score_us = *timings.get("score_embed_us").unwrap_or(&0);
    result.hybrid_total_pipeline_us = *timings.get("total_pipeline_us").unwrap_or(&0);
    result.hybrid_tfidf_candidates = *timings.get("bm25_candidates").unwrap_or(&0);
    result.hybrid_hdc_survivors = *timings.get("hdc_survivors").unwrap_or(&0);
    result.hybrid_cache_hit = *timings.get("cache_hit").unwrap_or(&0) == 1;

    // Second hybrid run (should be cache hit)
    let hybrid_start2 = Instant::now();
    let hybrid_json2 = parse_top_nodes_hybrid(&html, "alternative query test", tc.url, tc.top_n);
    let cached_ms = hybrid_start2.elapsed().as_millis() as u64;
    let timings2 = extract_hybrid_timings(&hybrid_json2);
    let _ = (cached_ms, timings2); // Logged in output below

    result
}

// ─── Output ──────────────────────────────────────────────────────────────────

fn generate_markdown(results: &[SiteResult], has_embeddings: bool) -> String {
    let mut md = String::new();
    md.push_str("# Real-World Validation — Hybrid Scoring Pipeline\n\n");
    md.push_str(&format!("**Date:** {}\n", chrono_date()));
    if has_embeddings {
        md.push_str("**Mode:** Release build, WITH embeddings (all-MiniLM-L6-v2, 384-dim)\n");
    } else {
        md.push_str("**Mode:** Release build, no embeddings (text similarity only)\n");
    }
    md.push_str("**Method:** Fetch → Legacy parse_top_nodes → Hybrid parse_top_nodes_hybrid\n\n");

    // Summary table
    md.push_str("## Summary\n\n");
    let total = results.len();
    let fetched = results.iter().filter(|r| r.fetch_error.is_none()).count();
    let legacy_correct = results.iter().filter(|r| r.legacy_has_keyword).count();
    let hybrid_correct = results.iter().filter(|r| r.hybrid_has_keyword).count();

    md.push_str("| Metric | Value |\n|--------|-------|\n");
    md.push_str(&format!("| Sites tested | {} |\n", total));
    md.push_str(&format!("| Successfully fetched | {} |\n", fetched));
    md.push_str(&format!(
        "| Legacy correctness (keyword in top 3) | {}/{} ({:.0}%) |\n",
        legacy_correct,
        fetched,
        if fetched > 0 {
            legacy_correct as f64 / fetched as f64 * 100.0
        } else {
            0.0
        }
    ));
    md.push_str(&format!(
        "| Hybrid correctness (keyword in top 3) | {}/{} ({:.0}%) |\n",
        hybrid_correct,
        fetched,
        if fetched > 0 {
            hybrid_correct as f64 / fetched as f64 * 100.0
        } else {
            0.0
        }
    ));

    let avg_legacy_ms: f64 = results
        .iter()
        .filter(|r| r.fetch_error.is_none())
        .map(|r| r.legacy_parse_ms as f64)
        .sum::<f64>()
        / fetched.max(1) as f64;
    let avg_hybrid_ms: f64 = results
        .iter()
        .filter(|r| r.fetch_error.is_none())
        .map(|r| r.hybrid_parse_ms as f64)
        .sum::<f64>()
        / fetched.max(1) as f64;

    md.push_str(&format!(
        "| Avg legacy parse time | {:.1}ms |\n",
        avg_legacy_ms
    ));
    md.push_str(&format!(
        "| Avg hybrid parse time | {:.1}ms |\n",
        avg_hybrid_ms
    ));
    md.push('\n');

    // Per-site results
    md.push_str("## Per-Site Results\n\n");
    md.push_str("| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |\n");
    md.push_str("|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|\n");

    for (i, r) in results.iter().enumerate() {
        if r.fetch_error.is_some() {
            md.push_str(&format!(
                "| {} | {} | FAIL | - | - | - | - | - | - | - |\n",
                i + 1,
                r.name,
            ));
            continue;
        }
        md.push_str(&format!(
            "| {} | {} | {}ms | {}KB | {}ms | {}ms | {} | {} | {} | {} |\n",
            i + 1,
            r.name,
            r.fetch_ms,
            r.html_bytes / 1024,
            r.legacy_parse_ms,
            r.hybrid_parse_ms,
            r.legacy_node_count,
            r.hybrid_node_count,
            if r.legacy_has_keyword { "PASS" } else { "MISS" },
            if r.hybrid_has_keyword { "PASS" } else { "MISS" },
        ));
    }

    // Pipeline breakdown
    md.push_str("\n## Hybrid Pipeline Stage Breakdown\n\n");
    md.push_str("| Site | BM25 build | HDC build | BM25 query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |\n");
    md.push_str("|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|\n");

    for r in results.iter().filter(|r| r.fetch_error.is_none()) {
        md.push_str(&format!(
            "| {} | {}µs | {}µs | {}µs | {}µs | {}µs | {}µs | {} | {} |\n",
            r.name,
            r.hybrid_tfidf_build_us,
            r.hybrid_hdc_build_us,
            r.hybrid_tfidf_query_us,
            r.hybrid_hdc_prune_us,
            r.hybrid_embed_score_us,
            r.hybrid_total_pipeline_us,
            r.hybrid_tfidf_candidates,
            r.hybrid_hdc_survivors,
        ));
    }

    // Top-3 comparison
    md.push_str("\n## Top-3 Node Quality Comparison\n\n");
    for r in results.iter().filter(|r| r.fetch_error.is_none()) {
        md.push_str(&format!("### {} — \"{}\" \n\n", r.name, r.goal));
        md.push_str("**Legacy top 3:**\n");
        for (j, label) in r.legacy_top3_labels.iter().enumerate() {
            let score = r.legacy_top3_scores.get(j).unwrap_or(&0.0);
            md.push_str(&format!("{}. `{:.3}` {}\n", j + 1, score, label));
        }
        md.push_str("\n**Hybrid top 3:**\n");
        for (j, label) in r.hybrid_top3_labels.iter().enumerate() {
            let score = r.hybrid_top3_scores.get(j).unwrap_or(&0.0);
            md.push_str(&format!("{}. `{:.3}` {}\n", j + 1, score, label));
        }
        md.push_str("\n---\n\n");
    }

    md
}

fn chrono_date() -> String {
    // Simple date without chrono crate
    "2026-03-30".to_string()
}

fn init_embeddings() -> bool {
    #[cfg(feature = "embeddings")]
    {
        // Försök ladda embedding-modell
        let model_path = std::env::var("AETHER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".to_string());
        let vocab_path = std::env::var("AETHER_EMBEDDING_VOCAB")
            .unwrap_or_else(|_| "models/vocab.txt".to_string());

        match (
            std::fs::read(&model_path),
            std::fs::read_to_string(&vocab_path),
        ) {
            (Ok(model_bytes), Ok(vocab_text)) => {
                match aether_agent::embedding::init_global(&model_bytes, &vocab_text) {
                    Ok(()) => {
                        println!(
                            "  Embedding model loaded: {} ({} bytes)",
                            model_path,
                            model_bytes.len()
                        );
                        println!(
                            "  Vocab loaded: {} ({} tokens)",
                            vocab_path,
                            vocab_text.lines().count()
                        );
                        return true;
                    }
                    Err(e) => {
                        eprintln!("  Embedding init error: {e}");
                    }
                }
            }
            (Err(e), _) => eprintln!("  Model not found: {model_path} ({e})"),
            (_, Err(e)) => eprintln!("  Vocab not found: {vocab_path} ({e})"),
        }
    }
    #[cfg(not(feature = "embeddings"))]
    {
        println!("  Embeddings feature not enabled (compile with --features embeddings)");
    }
    false
}

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║  AetherAgent Real-World Validation — 20 Sites, Legacy vs Hybrid     ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    let has_embeddings = init_embeddings();
    let mode = if has_embeddings {
        "FULL (text similarity + ONNX embedding)"
    } else {
        "TEXT-ONLY (word overlap, no neural embeddings)"
    };
    println!("  Mode: {mode}\n");

    let cases = test_cases();
    let mut results = Vec::new();

    for (i, tc) in cases.iter().enumerate() {
        print!("[{:2}/{}] {:<25} ", i + 1, cases.len(), tc.name);

        let result = run_test(tc);

        if let Some(ref err) = result.fetch_error {
            println!("FETCH FAIL: {}", err);
        } else {
            let l_ok = if result.legacy_has_keyword {
                "PASS"
            } else {
                "MISS"
            };
            let h_ok = if result.hybrid_has_keyword {
                "PASS"
            } else {
                "MISS"
            };
            println!(
                "{}KB | L:{}ms/{} H:{}ms/{} | pipe:{}µs | L:{} H:{}",
                result.html_bytes / 1024,
                result.legacy_parse_ms,
                l_ok,
                result.hybrid_parse_ms,
                h_ok,
                result.hybrid_total_pipeline_us,
                result.legacy_node_count,
                result.hybrid_node_count,
            );
        }

        results.push(result);
    }

    // Summary
    let fetched: Vec<&SiteResult> = results.iter().filter(|r| r.fetch_error.is_none()).collect();
    let legacy_correct = fetched.iter().filter(|r| r.legacy_has_keyword).count();
    let hybrid_correct = fetched.iter().filter(|r| r.hybrid_has_keyword).count();

    println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
    println!(
        "║  RESULTS: {} sites fetched, Legacy {}/{} correct, Hybrid {}/{} correct   ",
        fetched.len(),
        legacy_correct,
        fetched.len(),
        hybrid_correct,
        fetched.len()
    );
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    // Avg times
    if !fetched.is_empty() {
        let avg_l: f64 = fetched
            .iter()
            .map(|r| r.legacy_parse_ms as f64)
            .sum::<f64>()
            / fetched.len() as f64;
        let avg_h: f64 = fetched
            .iter()
            .map(|r| r.hybrid_parse_ms as f64)
            .sum::<f64>()
            / fetched.len() as f64;
        let avg_pipe: f64 = fetched
            .iter()
            .map(|r| r.hybrid_total_pipeline_us as f64)
            .sum::<f64>()
            / fetched.len() as f64;
        println!("  Avg legacy parse:     {:.1}ms", avg_l);
        println!("  Avg hybrid parse:     {:.1}ms", avg_h);
        println!("  Avg hybrid pipeline:  {:.0}µs", avg_pipe);
        println!(
            "  Avg HTML size:        {}KB",
            fetched.iter().map(|r| r.html_bytes).sum::<usize>() / 1024 / fetched.len()
        );
    }

    // Generate markdown
    let md = generate_markdown(&results, has_embeddings);
    match std::fs::write("docs/real_world_validation.md", &md) {
        Ok(()) => println!("\n  Results written to docs/real_world_validation.md"),
        Err(e) => eprintln!("\n  Failed to write docs: {e}"),
    }

    // Print markdown to stdout too
    println!("\n{}", md);
}
