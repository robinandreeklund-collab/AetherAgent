//! CRFR Convergence Test — measures convergence against REAL websites
//!
//! Protocol: "Run until 4/5 article-headings found for 3 consecutive, max 100"
//! Uses parse_crfr (auto-fetch + cache) + crfr_feedback, same pipeline as MCP tools.
//! First call fetches the page, subsequent calls hit the URL cache — no spam.
//!
//! Run with: `cargo run --bin aether-convergence-test --features fetch`

use std::time::Instant;

use aether_agent::resonance::{self, ResonanceField, ResonanceResult};
use aether_agent::types::SemanticNode;

// ─── Konfiguration ─────────────────────────────────────────────────────────

/// Konvergens: minst 4 av 5 artiklar (80%)
const CONVERGENCE_RATIO: f32 = 0.8;
/// Konsekutiva iterationer
const REQUIRED_STREAK: u32 = 3;
/// Max iterationer
const MAX_ITERATIONS: u32 = 100;

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Samla alla noder rekursivt
fn flatten_nodes(nodes: &[SemanticNode]) -> Vec<&SemanticNode> {
    let mut result = Vec::new();
    fn collect<'a>(node: &'a SemanticNode, out: &mut Vec<&'a SemanticNode>) {
        out.push(node);
        for child in &node.children {
            collect(child, out);
        }
    }
    for n in nodes {
        collect(n, &mut result);
    }
    result
}

/// Identifiera artikelrubriker i ett semantiskt träd
fn find_article_headings(nodes: &[SemanticNode]) -> Vec<u32> {
    let mut headings = Vec::new();

    fn find_in_articles(node: &SemanticNode, parent_is_article: bool, out: &mut Vec<u32>) {
        let is_article = node.role == "article"
            || (node.role == "group" && !node.label.is_empty() && node.label.len() > 20);

        if node.role == "heading" && (parent_is_article || is_article) && !node.label.is_empty() {
            out.push(node.id);
        }

        for child in &node.children {
            if child.role == "heading"
                && (is_article || parent_is_article)
                && !child.label.is_empty()
            {
                if !out.contains(&child.id) {
                    out.push(child.id);
                }
            }
            find_in_articles(child, is_article || parent_is_article, out);
        }
    }

    for node in nodes {
        find_in_articles(node, false, &mut headings);
    }

    // Fallback: ta de längsta heading-noderna
    if headings.len() < 5 {
        let all = flatten_nodes(nodes);
        let mut heading_nodes: Vec<&SemanticNode> = all
            .into_iter()
            .filter(|n| n.role == "heading" && n.label.len() > 15)
            .collect();
        heading_nodes.sort_by(|a, b| b.label.len().cmp(&a.label.len()));
        headings = heading_nodes.iter().take(10).map(|n| n.id).collect();
    }

    headings
}

/// Räkna artikelträffar i resonans-resultat
fn count_article_hits(results: &[ResonanceResult], heading_ids: &[u32]) -> usize {
    let result_ids: std::collections::HashSet<u32> = results.iter().map(|r| r.node_id).collect();
    heading_ids
        .iter()
        .filter(|id| result_ids.contains(id))
        .count()
}

fn max_causal(results: &[ResonanceResult]) -> f32 {
    results
        .iter()
        .map(|r| r.causal_boost)
        .fold(0.0f32, f32::max)
}

// ─── Resultattyper ──────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
struct IterationResult {
    i: u32,
    articles: usize,
    causal: usize,
    max_causal: f32,
    streak: u32,
}

#[derive(Debug, serde::Serialize)]
struct SiteResult {
    name: String,
    url: String,
    converged_at: Option<u32>,
    total_iterations: u32,
    total_nodes: usize,
    article_headings_found: usize,
    avg_propagation_ms: f64,
    history: Vec<IterationResult>,
}

/// Kör konvergenstest med den cachade ResonanceField
fn run_convergence(
    name: &str,
    url: &str,
    field: &mut ResonanceField,
    heading_ids: &[u32],
    total_nodes: usize,
    goal: &str,
) -> SiteResult {
    let target_count = heading_ids.len().min(5);
    let targets = &heading_ids[..target_count];

    let mut streak: u32 = 0;
    let mut converged_at: Option<u32> = None;
    let mut history = Vec::new();
    let mut total_ms: f64 = 0.0;

    for i in 1..=MAX_ITERATIONS {
        let t = Instant::now();
        let results = field.propagate(goal);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        total_ms += ms;

        let articles_found = count_article_hits(&results, targets);
        let causal_count = results.iter().filter(|r| r.causal_boost > 0.0).count();
        let mc = max_causal(&results);

        let ratio = if target_count > 0 {
            articles_found as f32 / target_count as f32
        } else {
            0.0
        };
        if ratio >= CONVERGENCE_RATIO {
            streak += 1;
        } else {
            streak = 0;
        }

        history.push(IterationResult {
            i,
            articles: articles_found,
            causal: causal_count,
            max_causal: (mc * 10000.0).round() / 10000.0,
            streak,
        });

        if streak >= REQUIRED_STREAK && converged_at.is_none() {
            converged_at = Some(i - REQUIRED_STREAK + 1);
        }

        // Feedback: markera hittade headings som framgångsrika
        let successful: Vec<u32> = targets
            .iter()
            .filter(|id| results.iter().any(|r| r.node_id == **id))
            .copied()
            .collect();
        if !successful.is_empty() {
            field.feedback(goal, &successful);
        }

        // Avbryt om konvergerat + 5 extra
        if let Some(conv) = converged_at {
            if i >= conv + REQUIRED_STREAK + 5 {
                break;
            }
        }
    }

    let total_iters = history.len() as u32;
    SiteResult {
        name: name.to_string(),
        url: url.to_string(),
        converged_at,
        total_iterations: total_iters,
        total_nodes,
        article_headings_found: target_count,
        avg_propagation_ms: total_ms / total_iters.max(1) as f64,
        history,
    }
}

// ─── Huvudprogram ───────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("=== CRFR Convergence Test (Real Sites) ===");
    println!("Protocol: 4/5 article-headings for 3 consecutive, max 100");
    println!();

    let sites: Vec<(&str, &str, &str)> = vec![
        (
            "Aftonbladet",
            "https://www.aftonbladet.se/",
            "find news article headlines",
        ),
        (
            "BBC News",
            "https://www.bbc.com/news",
            "find news article headlines",
        ),
        (
            "The Guardian",
            "https://www.theguardian.com/us",
            "find news article headlines",
        ),
        (
            "SVT Nyheter",
            "https://www.svt.se/",
            "find news article headlines",
        ),
        ("NPR", "https://www.npr.org/", "find news article headlines"),
    ];

    let config = aether_agent::types::FetchConfig::default();

    let mut all_results = Vec::new();
    let total_start = Instant::now();

    for (name, url, goal) in &sites {
        println!("--- {} ({}) ---", name, url);

        // Steg 1: Hämta sidan (bara första gången — sen är det i cache)
        print!("  Hämtar sida... ");
        let fetch_start = Instant::now();
        let fetch_result = match aether_agent::fetch::fetch_page(url, &config).await {
            Ok(r) => r,
            Err(e) => {
                println!("FEL: {}", e);
                all_results.push(SiteResult {
                    name: name.to_string(),
                    url: url.to_string(),
                    converged_at: None,
                    total_iterations: 0,
                    total_nodes: 0,
                    article_headings_found: 0,
                    avg_propagation_ms: 0.0,
                    history: Vec::new(),
                });
                continue;
            }
        };
        let fetch_ms = fetch_start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "OK ({:.0}ms, {}KB)",
            fetch_ms,
            fetch_result.body.len() / 1024
        );

        // Steg 2: Bygg semantiskt träd + ResonanceField (cachas automatiskt)
        print!("  Parsar och bygger fält... ");
        let parse_start = Instant::now();
        let tree = aether_agent::build_tree_for_crfr(
            &fetch_result.body,
            goal,
            &fetch_result.final_url,
            true, // JS-eval: handles SPA/JS-rendered pages like BBC, Guardian
        );
        let total_nodes = flatten_nodes(&tree.nodes).len();
        let parse_ms = parse_start.elapsed().as_secs_f64() * 1000.0;

        // Bygg ResonanceField (cacheas för framtida anrop)
        let (mut field, cache_hit) = resonance::get_or_build_field(&tree.nodes, url);
        println!(
            "OK ({:.0}ms, {} noder, cache={})",
            parse_ms,
            total_nodes,
            if cache_hit { "HIT" } else { "MISS" }
        );

        // Steg 3: Identifiera artikelrubriker
        let headings = find_article_headings(&tree.nodes);
        println!("  {} artikelrubriker identifierade:", headings.len());
        let all = flatten_nodes(&tree.nodes);
        for &hid in headings.iter().take(5) {
            if let Some(node) = all.iter().find(|n| n.id == hid) {
                let label_short = if node.label.chars().count() > 70 {
                    let truncated: String = node.label.chars().take(70).collect();
                    format!("{}...", truncated)
                } else {
                    node.label.clone()
                };
                println!("    [id:{}] \"{}\"", hid, label_short);
            }
        }
        if headings.len() > 5 {
            println!("    ... och {} till", headings.len() - 5);
        }

        // Steg 4: Kör konvergensloop
        if headings.is_empty() || total_nodes < 5 {
            println!("  SKIP: för få noder eller inga rubriker (sidan kräver troligen JS)");
            all_results.push(SiteResult {
                name: name.to_string(),
                url: url.to_string(),
                converged_at: None,
                total_iterations: 0,
                total_nodes,
                article_headings_found: 0,
                avg_propagation_ms: 0.0,
                history: Vec::new(),
            });
            println!();
            continue;
        }
        println!("  Kör konvergenstest...");
        let result = run_convergence(name, url, &mut field, &headings, total_nodes, goal);

        // Spara fältet efter träning
        resonance::save_field(&field);

        let status = match result.converged_at {
            Some(at) => format!("CONVERGED at iteration {}", at),
            None => format!(
                "FAILED ({} iters, max streak {})",
                result.total_iterations,
                result.history.iter().map(|h| h.streak).max().unwrap_or(0)
            ),
        };
        println!(
            "  Result: {} ({:.2}ms avg latency)",
            status, result.avg_propagation_ms
        );

        // Visa first-5 och last-5 history
        if !result.history.is_empty() {
            let first: Vec<String> = result
                .history
                .iter()
                .take(3)
                .map(|h| {
                    format!(
                        "i{}: {}/{} streak={}",
                        h.i,
                        h.articles,
                        headings.len().min(5),
                        h.streak
                    )
                })
                .collect();
            let last: Vec<String> = result
                .history
                .iter()
                .rev()
                .take(3)
                .rev()
                .map(|h| {
                    format!(
                        "i{}: {}/{} streak={}",
                        h.i,
                        h.articles,
                        headings.len().min(5),
                        h.streak
                    )
                })
                .collect();
            println!("  First: {}", first.join(" → "));
            println!("  Last:  {}", last.join(" → "));
        }
        println!();

        all_results.push(result);
    }

    let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

    // Sammanfattning
    let converged_count = all_results
        .iter()
        .filter(|r| r.converged_at.is_some())
        .count();
    let avg_convergence: f64 = {
        let converged: Vec<f64> = all_results
            .iter()
            .filter_map(|r| r.converged_at.map(|c| c as f64))
            .collect();
        if converged.is_empty() {
            f64::INFINITY
        } else {
            converged.iter().sum::<f64>() / converged.len() as f64
        }
    };
    let avg_latency: f64 = all_results
        .iter()
        .filter(|r| r.total_iterations > 0)
        .map(|r| r.avg_propagation_ms)
        .sum::<f64>()
        / all_results
            .iter()
            .filter(|r| r.total_iterations > 0)
            .count()
            .max(1) as f64;

    println!("=== Summary ===");
    println!(
        "Sites converged:     {}/{}",
        converged_count,
        all_results.len()
    );
    println!("Avg convergence:     {:.1} iterations", avg_convergence);
    println!("Avg latency:         {:.2} ms/propagation", avg_latency);
    println!("Total time:          {:.1} ms", total_ms);
    println!();

    println!(
        "{:<15} {:>10} {:>8} {:>10} {:>10}",
        "Site", "Converged", "Iters", "Latency", "Nodes"
    );
    println!("{:-<58}", "");
    for r in &all_results {
        let conv = match r.converged_at {
            Some(at) => format!("{}", at),
            None => "FAIL".to_string(),
        };
        println!(
            "{:<15} {:>10} {:>8} {:>8.2}ms {:>10}",
            r.name, conv, r.total_iterations, r.avg_propagation_ms, r.total_nodes
        );
    }
    println!();

    // JSON output
    #[derive(serde::Serialize)]
    struct Output {
        sites: Vec<SiteResult>,
        date: String,
        protocol: String,
    }
    let output = Output {
        sites: all_results,
        date: "2026-04-06".to_string(),
        protocol: "Run until 4/5 article-headings for 3 consecutive, max 100".to_string(),
    };
    let json = serde_json::to_string_pretty(&output).unwrap_or_default();
    std::fs::write("docs/convergence-baseline.json", &json).ok();
    println!("Results written to docs/convergence-baseline.json");
    println!("=== Done ===");
}
