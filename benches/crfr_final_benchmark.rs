/// CRFR Final Benchmark — Riktiga sajter, svarskvalitet
///
/// Kör CRFR + Pipeline på riktiga HTML-filer och mäter:
/// 1. Hittar vi svaret i top-k?
/// 2. Latens
/// 3. Output-storlek (token-effektivitet)
///
/// Run:
///   AETHER_EMBEDDING_MODEL=models/colbert-small-int8.onnx \
///   AETHER_EMBEDDING_VOCAB=models/vocab.txt \
///   cargo run --release --bin aether-final-bench --features embeddings
///
/// Utan embeddings:
///   cargo run --release --bin aether-final-bench
use std::time::Instant;

use aether_agent::resonance;
use aether_agent::scoring::pipeline::{PipelineConfig, ScoringPipeline};
use aether_agent::types::SemanticNode;

// ─── Testdefinitioner ───────────────────────────────────────────────────────

struct TestCase {
    name: &'static str,
    /// Sökväg till HTML-fil (relativ till projektrot)
    html_path: &'static str,
    goal: &'static str,
    /// Strängar som MÅSTE finnas i svaret (case-insensitive)
    must_contain: &'static [&'static str],
    /// Beskrivning av vad vi söker (visas vid miss)
    _description: &'static str,
}

fn test_cases() -> Vec<TestCase> {
    vec![
        // ─── Riktiga sajter (testsuite/benchmark/lightpanda/) ───
        TestCase {
            name: "Books.toscrape — bokpris",
            html_path: "testsuite/benchmark/lightpanda/books_toscrape_com.html",
            goal: "find book price cost catalogue",
            must_contain: &["£"],
            _description: "Bokpris i pund",
        },
        TestCase {
            name: "Books.toscrape — boktitel",
            html_path: "testsuite/benchmark/lightpanda/books_toscrape_com.html",
            goal: "book titles catalog list available books",
            must_contain: &["book"],
            _description: "Bokkatalog med titlar",
        },
        TestCase {
            name: "Hacker News — artiklar",
            html_path: "testsuite/benchmark/lightpanda/news_ycombinator_com.html",
            goal: "find top stories articles submissions on Hacker News",
            must_contain: &["points"],
            _description: "Artikelrader med poäng",
        },
        TestCase {
            name: "HN — iPhone LLM artikel",
            html_path: "testsuite/benchmark/lightpanda/news_ycombinator_com.html",
            goal: "iPhone 17 Pro LLM 400B article",
            must_contain: &["iphone", "llm"],
            _description: "Specifik artikel om iPhone 17 Pro + LLM",
        },
        TestCase {
            name: "Apple.com — iPhone 17 Pro",
            html_path: "testsuite/benchmark/lightpanda/www_apple_com.html",
            goal: "iPhone 17 Pro new features",
            must_contain: &["iphone 17"],
            _description: "iPhone 17 Pro produktinfo",
        },
        TestCase {
            name: "GitHub — repositories",
            html_path: "testsuite/benchmark/lightpanda/github_com.html",
            goal: "trending repositories popular open source projects",
            must_contain: &["github"],
            _description: "GitHub repository-information",
        },
        TestCase {
            name: "Expressen — nyheter",
            html_path: "testsuite/benchmark/lightpanda/www_expressen_se.html",
            goal: "senaste nyheter rubriker idag Sverige",
            must_contain: &["expressen"],
            _description: "Nyhetsrubriker från Expressen",
        },
        TestCase {
            name: "DI — ekonomi",
            html_path: "testsuite/benchmark/lightpanda/www_di_se.html",
            goal: "börsen aktier ekonomi nyheter Dagens Industri",
            must_contain: &["industri"],
            _description: "Ekonominyheter från DI",
        },
        // ─── Fixtures (tests/fixtures/) — svenska HTML ─────────
        TestCase {
            name: "E-commerce — produktpris",
            html_path: "tests/fixtures/01_ecommerce_product.html",
            goal: "pris produkt köp iPhone kostnad kr",
            must_contain: &["kr"],
            _description: "Produktpris i kronor",
        },
        TestCase {
            name: "Sökresultat — hotell",
            html_path: "tests/fixtures/03_search_results.html",
            goal: "sökresultat hotell boende Stockholm",
            must_contain: &["kr"],
            _description: "Sökresultat med priser i kr",
        },
        TestCase {
            name: "Checkout — ordersumma",
            html_path: "tests/fixtures/05_checkout.html",
            goal: "beställning total summa betala kr pris",
            must_contain: &["kr"],
            _description: "Ordersumma vid checkout",
        },
        TestCase {
            name: "Nyhetsartikel — publicerad",
            html_path: "tests/fixtures/06_news_article.html",
            goal: "artikel publicerad nyheter e-handel AI",
            must_contain: &["publicer"],
            _description: "Publiceringsinfo i nyhetsartikel",
        },
        TestCase {
            name: "Restaurang — meny priser",
            html_path: "tests/fixtures/08_restaurant_menu.html",
            goal: "meny mat rätter priser restaurang lunch",
            must_contain: &["kr"],
            _description: "Menyrätter med priser i kr",
        },
        TestCase {
            name: "Bank — kontosaldo",
            html_path: "tests/fixtures/12_banking.html",
            goal: "konto saldo belopp sparande lönekonto kr",
            must_contain: &["kr"],
            _description: "Kontosaldo i kronor",
        },
        TestCase {
            name: "Fastighet — pris",
            html_path: "tests/fixtures/13_real_estate.html",
            goal: "bostad pris lägenhet hus kostnad kr",
            must_contain: &["kr"],
            _description: "Fastighetspris i kronor",
        },
        TestCase {
            name: "Jobbannons — lön",
            html_path: "tests/fixtures/14_job_listing.html",
            goal: "jobb lön tjänst position ansök",
            must_contain: &["lön"],
            _description: "Löneuppgift i jobbannons",
        },
        TestCase {
            name: "Wiki — Stockholm",
            html_path: "tests/fixtures/17_wiki_article.html",
            goal: "Stockholm huvudstad Sverige historia",
            must_contain: &["stockholm"],
            _description: "Wikipedia-artikel om Stockholm",
        },
        TestCase {
            name: "Nutrition — kalorier",
            html_path: "tests/fixtures/27_semantic_nutrition.html",
            goal: "calories nutritional information food energy",
            must_contain: &["calori"],
            _description: "Kalorinnehåll",
        },
        TestCase {
            name: "Öppettider",
            html_path: "tests/fixtures/29_semantic_hours.html",
            goal: "opening hours schedule when open close times",
            must_contain: &["09:00"],
            _description: "Öppettider med klockslag",
        },
        TestCase {
            name: "E-commerce jämförelse",
            html_path: "tests/fixtures/35_ecommerce_product_compare.html",
            goal: "jämför produkter specifikationer pris kr",
            must_contain: &["kr"],
            _description: "Produktjämförelse med priser i kr",
        },
    ]
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn extract_tree(html: &str, goal: &str) -> Option<Vec<SemanticNode>> {
    let json = aether_agent::parse_to_semantic_tree(html, goal, "");
    let parsed: serde_json::Value = serde_json::from_str(&json).ok()?;
    let nodes_val = parsed.get("nodes")?.clone();
    serde_json::from_value(nodes_val).ok()
}

fn find_node_by_id(nodes: &[SemanticNode], target_id: u32) -> Option<String> {
    for node in nodes {
        if node.id == target_id {
            return Some(node.label.clone());
        }
        if let Some(found) = find_node_by_id(&node.children, target_id) {
            return Some(found);
        }
    }
    None
}

fn count_nodes(nodes: &[SemanticNode]) -> usize {
    let mut c = nodes.len();
    for n in nodes {
        c += count_nodes(&n.children);
    }
    c
}

/// Kolla om alla must_contain finns i texten (case-insensitive)
fn check_answer(text: &str, must_contain: &[&str]) -> bool {
    let lower = text.to_lowercase();
    must_contain
        .iter()
        .all(|kw| lower.contains(&kw.to_lowercase()))
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════════════════╗");
    println!("║  FINAL BENCHMARK — CRFR vs Pipeline — Riktiga sajter + Svarskvalitet   ║");
    println!("╚═══════════════════════════════════════════════════════════════════════════╝\n");

    #[cfg(feature = "embeddings")]
    {
        let mp = std::env::var("AETHER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "models/colbert-small-int8.onnx".into());
        let vp =
            std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
        if let (Ok(mb), Ok(vt)) = (std::fs::read(&mp), std::fs::read_to_string(&vp)) {
            if aether_agent::embedding::init_global(&mb, &vt).is_ok() {
                println!("  Embedding: LOADED ({})\n", mp);
            }
        }
    }

    let tests = test_cases();
    let total = tests.len();

    // Per-test resultat
    struct TestResult {
        _name: String,
        dom_nodes: usize,
        html_tokens: usize,
        // CRFR
        crfr_hits: [bool; 4], // @1, @3, @10, @20
        crfr_us: u64,
        crfr_output: usize,
        crfr_output_tokens: usize,
        crfr_cache: bool,
        // Pipeline
        pipe_hits: [bool; 4],
        pipe_us: u64,
        pipe_output: usize,
        pipe_output_tokens: usize,
    }

    let mut results: Vec<TestResult> = Vec::new();

    for (i, tc) in tests.iter().enumerate() {
        let html = match std::fs::read_to_string(tc.html_path) {
            Ok(h) => h,
            Err(e) => {
                println!("  [{:>2}/{}] {} — SKIP ({})", i + 1, total, tc.name, e);
                continue;
            }
        };

        let html_tokens = html.len() / 4; // ~4 chars per token

        let tree = match extract_tree(&html, tc.goal) {
            Some(t) => t,
            None => {
                println!(
                    "  [{:>2}/{}] {} — SKIP (parse failed)",
                    i + 1,
                    total,
                    tc.name
                );
                continue;
            }
        };

        let dom_nodes = count_nodes(&tree);

        // ─── CRFR (top_k=20 för att mäta @1/@3/@10/@20) ──────────

        let t0 = Instant::now();
        let (mut field, cache_hit) = resonance::get_or_build_field(&tree, tc.html_path);
        let crfr_results = field.propagate_top_k(tc.goal, 20);
        let crfr_us = t0.elapsed().as_micros() as u64;

        let crfr_labels: Vec<(u32, String)> = crfr_results
            .iter()
            .map(|r| {
                (
                    r.node_id,
                    find_node_by_id(&tree, r.node_id).unwrap_or_default(),
                )
            })
            .collect();

        // Mät recall vid varje cutoff
        let crfr_check = |k: usize| -> bool {
            let text: String = crfr_labels
                .iter()
                .take(k)
                .map(|(_, l)| l.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            check_answer(&text, tc.must_contain)
        };
        let crfr_hits = [crfr_check(1), crfr_check(3), crfr_check(10), crfr_check(20)];

        let crfr_output_tokens: usize = crfr_labels.iter().take(20).map(|(_, l)| l.len() / 4).sum();

        // Feedback + cache
        let success_ids: Vec<u32> = crfr_labels
            .iter()
            .filter(|(_, label)| check_answer(label, tc.must_contain))
            .map(|(id, _)| *id)
            .collect();
        if !success_ids.is_empty() {
            field.feedback(tc.goal, &success_ids);
        }
        resonance::save_field(&field);

        // ─── Pipeline ──────────────────────────────────────────────

        #[cfg(feature = "embeddings")]
        let goal_emb = aether_agent::embedding::embed(tc.goal);
        #[cfg(not(feature = "embeddings"))]
        let goal_emb: Option<Vec<f32>> = None;

        let config = PipelineConfig::default();
        let t1 = Instant::now();
        let pipe_result = ScoringPipeline::run(&tree, tc.goal, goal_emb.as_deref(), &config);
        let pipe_us = t1.elapsed().as_micros() as u64;

        let pipe_check = |k: usize| -> bool {
            let text: String = pipe_result
                .scored_nodes
                .iter()
                .take(k)
                .map(|n| n.label.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            check_answer(&text, tc.must_contain)
        };
        let pipe_hits = [pipe_check(1), pipe_check(3), pipe_check(10), pipe_check(20)];

        let pipe_output_tokens: usize = pipe_result
            .scored_nodes
            .iter()
            .take(20)
            .map(|n| n.label.len() / 4)
            .sum();

        // ─── Visa rad ──────────────────────────────────────────────

        let c = if cache_hit { "C" } else { " " };
        let mk = |b: bool| if b { "OK" } else { "--" };

        println!(
            "  [{:>2}/{}] {:<30} {:>5} nod  CRFR[{}] @1={} @3={} @10={} @20={} {:>7}µs {:>2}n | Pipe @1={} @3={} @10={} @20={} {:>8}µs {:>2}n",
            i + 1, total, tc.name, dom_nodes, c,
            mk(crfr_hits[0]), mk(crfr_hits[1]), mk(crfr_hits[2]), mk(crfr_hits[3]),
            crfr_us, crfr_results.len(),
            mk(pipe_hits[0]), mk(pipe_hits[1]), mk(pipe_hits[2]), mk(pipe_hits[3]),
            pipe_us, pipe_result.scored_nodes.len(),
        );

        // Visa top-1 vid miss@20
        if !crfr_hits[3] {
            if let Some((_, label)) = crfr_labels.first() {
                let trunc: String = label.chars().take(80).collect();
                println!("         CRFR top-1: {}", trunc);
            }
        }
        if !pipe_hits[3] {
            if let Some(n) = pipe_result.scored_nodes.first() {
                let trunc: String = n.label.chars().take(80).collect();
                println!("         Pipe top-1: {}", trunc);
            }
        }

        results.push(TestResult {
            _name: tc.name.to_string(),
            dom_nodes,
            html_tokens,
            crfr_hits,
            crfr_us,
            crfr_output: crfr_results.len(),
            crfr_output_tokens,
            crfr_cache: cache_hit,
            pipe_hits,
            pipe_us,
            pipe_output: pipe_result.scored_nodes.len(),
            pipe_output_tokens,
        });
    }

    // ─── Sammanfattning ─────────────────────────────────────────────────────

    let n = results.len();
    if n == 0 {
        println!("\n  Inga tester körda.");
        return;
    }

    println!("\n{}", "=".repeat(100));
    println!("  SAMMANFATTNING ({n} tester av {total})\n");

    // Recall-tabell
    let cutoffs = [("@1", 0), ("@3", 1), ("@10", 2), ("@20", 3)];
    println!(
        "  {:<30} {:>8} {:>8} {:>8} {:>8} {:>10} {:>8}",
        "Metod", "@1", "@3", "@10", "@20", "Avg µs", "Avg nod"
    );
    println!("  {}", "-".repeat(85));

    // CRFR rad
    let crfr_counts: Vec<usize> = cutoffs
        .iter()
        .map(|(_, idx)| results.iter().filter(|r| r.crfr_hits[*idx]).count())
        .collect();
    let crfr_avg_us = results.iter().map(|r| r.crfr_us).sum::<u64>() / n as u64;
    let crfr_avg_n = results.iter().map(|r| r.crfr_output).sum::<usize>() as f64 / n as f64;
    println!(
        "  {:<30} {:>4}/{:<3} {:>4}/{:<3} {:>4}/{:<3} {:>4}/{:<3} {:>8} {:>8.1}",
        "CRFR (BM25+HDC+cache)",
        crfr_counts[0],
        n,
        crfr_counts[1],
        n,
        crfr_counts[2],
        n,
        crfr_counts[3],
        n,
        crfr_avg_us,
        crfr_avg_n,
    );

    // Pipeline rad
    let pipe_counts: Vec<usize> = cutoffs
        .iter()
        .map(|(_, idx)| results.iter().filter(|r| r.pipe_hits[*idx]).count())
        .collect();
    let pipe_avg_us = results.iter().map(|r| r.pipe_us).sum::<u64>() / n as u64;
    let pipe_avg_n = results.iter().map(|r| r.pipe_output).sum::<usize>() as f64 / n as f64;
    println!(
        "  {:<30} {:>4}/{:<3} {:>4}/{:<3} {:>4}/{:<3} {:>4}/{:<3} {:>8} {:>8.1}",
        "Pipeline (BM25+HDC+Embed)",
        pipe_counts[0],
        n,
        pipe_counts[1],
        n,
        pipe_counts[2],
        n,
        pipe_counts[3],
        n,
        pipe_avg_us,
        pipe_avg_n,
    );

    // Token-besparingar
    println!("\n  TOKEN-EFFEKTIVITET:");
    let total_html_tokens: usize = results.iter().map(|r| r.html_tokens).sum();
    let total_crfr_tokens: usize = results.iter().map(|r| r.crfr_output_tokens).sum();
    let total_pipe_tokens: usize = results.iter().map(|r| r.pipe_output_tokens).sum();
    let avg_dom: usize = results.iter().map(|r| r.dom_nodes).sum::<usize>() / n;

    println!(
        "  Avg HTML:       {:>8} tokens ({} noder avg)",
        total_html_tokens / n,
        avg_dom
    );
    println!(
        "  CRFR output:    {:>8} tokens ({:.1} noder avg) — {:.1}% av HTML",
        total_crfr_tokens / n,
        crfr_avg_n,
        total_crfr_tokens as f64 / total_html_tokens as f64 * 100.0,
    );
    println!(
        "  Pipeline output: {:>7} tokens ({:.1} noder avg) — {:.1}% av HTML",
        total_pipe_tokens / n,
        pipe_avg_n,
        total_pipe_tokens as f64 / total_html_tokens as f64 * 100.0,
    );

    // Speedup
    println!("\n  PRESTANDA:");
    println!(
        "  Speedup CRFR vs Pipeline: {:.1}x",
        pipe_avg_us as f64 / crfr_avg_us.max(1) as f64
    );
    let cache_hits = results.iter().filter(|r| r.crfr_cache).count();
    println!("  Cache hits: {cache_hits}/{n}");
    let (ce, cc) = resonance::cache_stats();
    println!("  Field cache: {ce}/{cc} entries");
    println!();
}
