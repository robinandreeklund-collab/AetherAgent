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

    // Ackumulatorer
    let mut crfr_found_3 = 0u32;
    let mut crfr_found_5 = 0u32;
    let mut crfr_total_us = 0u64;
    let mut crfr_total_output = 0usize;

    let mut pipe_found_3 = 0u32;
    let mut pipe_found_5 = 0u32;
    let mut pipe_total_us = 0u64;
    let mut pipe_total_output = 0usize;

    let mut tests_run = 0u32;

    for (i, tc) in tests.iter().enumerate() {
        // Läs HTML-fil
        let html = match std::fs::read_to_string(tc.html_path) {
            Ok(h) => h,
            Err(e) => {
                println!("  [{:>2}/{}] {} — SKIP ({})", i + 1, total, tc.name, e);
                continue;
            }
        };

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
        tests_run += 1;

        // ─── CRFR ──────────────────────────────────────────────────

        let t0 = Instant::now();
        let (mut field, cache_hit) = resonance::get_or_build_field(&tree, tc.html_path);
        let crfr_results = field.propagate_top_k(tc.goal, 10);
        let crfr_us = t0.elapsed().as_micros() as u64;
        crfr_total_us += crfr_us;

        // Ge feedback och spara i cache
        let crfr_labels: Vec<(u32, String)> = crfr_results
            .iter()
            .map(|r| {
                (
                    r.node_id,
                    find_node_by_id(&tree, r.node_id).unwrap_or_default(),
                )
            })
            .collect();

        let crfr_text_3: String = crfr_labels
            .iter()
            .take(3)
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let crfr_text_5: String = crfr_labels
            .iter()
            .take(5)
            .map(|(_, l)| l.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let crfr_hit_3 = check_answer(&crfr_text_3, tc.must_contain);
        let crfr_hit_5 = check_answer(&crfr_text_5, tc.must_contain);
        if crfr_hit_3 {
            crfr_found_3 += 1;
        }
        if crfr_hit_5 {
            crfr_found_5 += 1;
        }
        crfr_total_output += crfr_results.len();

        // Feedback: noder som innehöll svaret
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
        pipe_total_us += pipe_us;

        let pipe_text_3: String = pipe_result
            .scored_nodes
            .iter()
            .take(3)
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let pipe_text_5: String = pipe_result
            .scored_nodes
            .iter()
            .take(5)
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        let pipe_hit_3 = check_answer(&pipe_text_3, tc.must_contain);
        let pipe_hit_5 = check_answer(&pipe_text_5, tc.must_contain);
        if pipe_hit_3 {
            pipe_found_3 += 1;
        }
        if pipe_hit_5 {
            pipe_found_5 += 1;
        }
        pipe_total_output += pipe_result.scored_nodes.len();

        // ─── Visa resultat ─────────────────────────────────────────

        let crfr_m3 = if crfr_hit_3 { "OK" } else { "MISS" };
        let crfr_m5 = if crfr_hit_5 { "OK" } else { "MISS" };
        let pipe_m3 = if pipe_hit_3 { "OK" } else { "MISS" };
        let pipe_m5 = if pipe_hit_5 { "OK" } else { "MISS" };
        let cache_tag = if cache_hit { "C" } else { " " };

        println!(
            "  [{:>2}/{}] {:<35} DOM:{:>5}  CRFR[{}]: @3={:<4} @5={:<4} {:>6}µs {:>2}n | Pipe: @3={:<4} @5={:<4} {:>7}µs {:>2}n",
            i + 1,
            total,
            tc.name,
            dom_nodes,
            cache_tag,
            crfr_m3,
            crfr_m5,
            crfr_us,
            crfr_results.len(),
            pipe_m3,
            pipe_m5,
            pipe_us,
            pipe_result.scored_nodes.len(),
        );

        // Visa var svaret hittades (eller top-1 om miss)
        if !crfr_hit_5 {
            if let Some((_, label)) = crfr_labels.first() {
                let trunc: String = label.chars().take(80).collect();
                println!("         CRFR top-1: {}", trunc);
            }
        }
        if !pipe_hit_5 {
            if let Some(n) = pipe_result.scored_nodes.first() {
                let trunc: String = n.label.chars().take(80).collect();
                println!("         Pipe top-1: {}", trunc);
            }
        }
    }

    // ─── Sammanfattning ─────────────────────────────────────────────────────

    let n = tests_run;
    println!("\n{}", "=".repeat(90));
    println!("  SAMMANFATTNING ({n} tester körda av {total})\n");

    println!(
        "  {:<30} {:>10} {:>10} {:>12} {:>10}",
        "Metod", "Recall@3", "Recall@5", "Avg µs", "Avg output"
    );
    println!("  {}", "-".repeat(75));
    println!(
        "  {:<30} {:>5}/{:<4} {:>5}/{:<4} {:>8} µs {:>6}",
        "CRFR (BM25+HDC+cache)",
        crfr_found_3,
        n,
        crfr_found_5,
        n,
        crfr_total_us / n as u64,
        format!("{:.1}", crfr_total_output as f64 / n as f64),
    );
    println!(
        "  {:<30} {:>5}/{:<4} {:>5}/{:<4} {:>8} µs {:>6}",
        "Pipeline (BM25+HDC+Embed)",
        pipe_found_3,
        n,
        pipe_found_5,
        n,
        pipe_total_us / n as u64,
        format!("{:.1}", pipe_total_output as f64 / n as f64),
    );

    if crfr_total_us > 0 {
        println!(
            "\n  Speedup: {:.1}x",
            pipe_total_us as f64 / crfr_total_us as f64
        );
    }
    let crfr_avg = crfr_total_output as f64 / n as f64;
    let pipe_avg = pipe_total_output as f64 / n as f64;
    if pipe_avg > 0.0 {
        println!(
            "  Token-reduktion: {:.0}% färre noder",
            (1.0 - crfr_avg / pipe_avg) * 100.0
        );
    }
    let (ce, cc) = resonance::cache_stats();
    println!("  Field cache: {ce}/{cc} entries");
    println!();
}
