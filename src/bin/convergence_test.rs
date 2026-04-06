//! CRFR Convergence Test v2 — Honest evaluation with standard IR metrics
//!
//! Protocol (matches crfr-20site-evaluation.json):
//!   Phase BASELINE: Q1 cold start, no feedback
//!   Phase TRAIN: Q2-Q7 with feedback after each
//!   Phase TEST: Q8-Q10 no feedback, measure generalization
//!
//! Metrics: nDCG@5, MRR, P@5 — only top-20 results considered
//! Relevance: binary keyword match (same as original eval)
//!
//! Run with: `cargo run --bin aether-convergence-test --features "fetch,js-eval"`

use std::time::Instant;

use aether_agent::resonance::{self, ResonanceField, ResonanceResult};
use aether_agent::types::SemanticNode;

// ─── Konfiguration ─────────────────────────────────────────────────────────

const TOP_N: usize = 20;

// ─── IR Metrics ─────────────────────────────────────────────────────────────

fn ndcg_at_k(rels: &[f32], k: usize) -> f32 {
    let rels: Vec<f32> = rels.iter().take(k).copied().collect();
    let dcg: f32 = rels
        .iter()
        .enumerate()
        .map(|(i, &r)| r / (i as f32 + 2.0).log2())
        .sum();
    let mut ideal = rels.clone();
    ideal.sort_by(|a, b| b.total_cmp(a));
    let idcg: f32 = ideal
        .iter()
        .enumerate()
        .map(|(i, &r)| r / (i as f32 + 2.0).log2())
        .sum();
    if idcg > 0.0 {
        dcg / idcg
    } else {
        0.0
    }
}

fn mrr(rels: &[f32]) -> f32 {
    for (i, &r) in rels.iter().enumerate() {
        if r > 0.0 {
            return 1.0 / (i as f32 + 1.0);
        }
    }
    0.0
}

fn precision_at_k(rels: &[f32], k: usize) -> f32 {
    let relevant = rels.iter().take(k).filter(|&&r| r > 0.0).count();
    relevant as f32 / k as f32
}

// ─── Relevance Judgment ─────────────────────────────────────────────────────

/// Binary relevance: does the node label contain content keywords?
/// Filters out obvious nav/boilerplate.
fn is_relevant(label: &str, keywords: &[&str]) -> bool {
    let lower = label.to_lowercase();

    // Filter nav/boilerplate
    let nav_signals = [
        "cookie",
        "privacy",
        "sign in",
        "log in",
        "subscribe",
        "newsletter",
        "skip to",
        "menu",
        "footer",
        "copyright",
        "terms of use",
    ];
    for nav in &nav_signals {
        if lower.contains(nav) && lower.len() < 100 {
            return false;
        }
    }

    keywords.iter().any(|kw| lower.contains(&kw.to_lowercase()))
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

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

// ─── Site definitions ───────────────────────────────────────────────────────

struct SiteConfig {
    name: &'static str,
    url: &'static str,
    goals: [&'static str; 10],
    keywords: &'static [&'static str],
}

fn sites() -> Vec<SiteConfig> {
    vec![
        SiteConfig {
            name: "BBC News",
            url: "https://www.bbc.com/news",
            goals: [
                "latest news headlines today",
                "breaking news stories right now",
                "top news articles today",
                "current world news updates",
                "major news events happening now",
                "important world headlines today",
                "todays top news stories",
                "what is happening in the world right now",
                "global news and current events",
                "recent major world developments",
            ],
            keywords: &["news", "article", "headline", "story", "report", "breaking"],
        },
        SiteConfig {
            name: "NPR",
            url: "https://www.npr.org/",
            goals: [
                "latest news stories today",
                "breaking news headlines now",
                "top articles published today",
                "important current events",
                "major stories happening right now",
                "key news developments today",
                "most notable news stories",
                "what are todays biggest news stories",
                "current affairs and global events",
                "recent notable world happenings",
            ],
            keywords: &["news", "article", "story", "report", "headline"],
        },
        SiteConfig {
            name: "Wikipedia Einstein",
            url: "https://en.wikipedia.org/wiki/Albert_Einstein",
            goals: [
                "when was Einstein born",
                "Einstein birth date and place",
                "where was Albert Einstein born",
                "year Einstein was born",
                "Einstein early life birthplace",
                "born Albert Einstein date",
                "Einstein birth year and location",
                "what year was Einstein born and where",
                "Einstein origins and birthplace",
                "birth facts about Albert Einstein",
            ],
            keywords: &["1879", "march", "ulm", "born", "germany", "birth"],
        },
        SiteConfig {
            name: "Wikipedia Rust",
            url: "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            goals: [
                "who created Rust programming language",
                "Rust language creator",
                "who invented Rust",
                "Rust programming origin story",
                "creator of the Rust language",
                "who designed Rust originally",
                "Rust language author and history",
                "who started the Rust project",
                "developer who made Rust",
                "Rust programming creation history",
            ],
            keywords: &["graydon", "hoare", "2006", "2010", "mozilla", "created"],
        },
        SiteConfig {
            name: "ESPN",
            url: "https://www.espn.com/",
            goals: [
                "latest sports scores today",
                "todays game results",
                "live sports scores and updates",
                "major sports results today",
                "current game scores",
                "todays match results",
                "sports scores and highlights",
                "what are todays sports results",
                "live game updates and scores",
                "current sports standings and scores",
            ],
            keywords: &[
                "score", "game", "win", "loss", "team", "match", "nba", "nfl", "mlb",
            ],
        },
        SiteConfig {
            name: "USA.gov",
            url: "https://www.usa.gov/",
            goals: [
                "government benefits and services",
                "how to apply for government benefits",
                "federal services for citizens",
                "government assistance programs",
                "public benefits information",
                "citizen services overview",
                "federal government help",
                "what government services are available",
                "how to get government assistance",
                "public services and benefits guide",
            ],
            keywords: &[
                "benefit",
                "service",
                "government",
                "federal",
                "apply",
                "assistance",
            ],
        },
        SiteConfig {
            name: "Nature",
            url: "https://www.nature.com/",
            goals: [
                "latest scientific research",
                "new science publications today",
                "recent scientific discoveries",
                "important research papers",
                "breakthrough science news",
                "major scientific findings",
                "new research in science journals",
                "what are the latest scientific discoveries",
                "cutting edge research papers",
                "notable science publications this week",
            ],
            keywords: &[
                "research", "study", "science", "paper", "publish", "discover", "journal",
            ],
        },
        SiteConfig {
            name: "WebMD",
            url: "https://www.webmd.com/",
            goals: [
                "common cold symptoms and treatment",
                "cold flu symptoms guide",
                "how to treat a cold",
                "symptoms of common cold",
                "cold remedies and treatment",
                "what helps with a cold",
                "cold virus symptoms list",
                "home remedies for cold symptoms",
                "when to see doctor for cold",
                "cold versus flu symptoms difference",
            ],
            keywords: &[
                "cold",
                "symptom",
                "treatment",
                "fever",
                "cough",
                "flu",
                "remedy",
            ],
        },
    ]
}

// ─── Resultattyper ──────────────────────────────────────────────────────────

#[derive(Debug, serde::Serialize)]
struct IterationResult {
    i: u32,
    goal: String,
    phase: String,
    ndcg5: f32,
    mrr: f32,
    p5: f32,
    causal: usize,
    relevant: usize,
    total_returned: usize,
}

#[derive(Debug, serde::Serialize)]
struct SiteResult {
    name: String,
    url: String,
    total_nodes: usize,
    baseline_ndcg5: f32,
    baseline_mrr: f32,
    train_avg_ndcg5: f32,
    test_avg_ndcg5: f32,
    test_avg_mrr: f32,
    test_avg_p5: f32,
    test_causal_avg: f32,
    feedback_total: usize,
    feedback_relevant: usize,
    iterations: Vec<IterationResult>,
}

// ─── Huvudprogram ───────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("=== CRFR 20-Site Evaluation v2 (Honest Metrics) ===");
    println!("Protocol: Q1=baseline, Q2-Q7=train+feedback, Q8-Q10=test");
    println!("Metrics: nDCG@5, MRR, P@5 on TOP-{} results only", TOP_N);
    println!();

    let config = aether_agent::types::FetchConfig::default();
    let all_sites = sites();
    let mut all_results: Vec<SiteResult> = Vec::new();

    for site in &all_sites {
        println!("--- {} ({}) ---", site.name, site.url);

        // Fetch page
        let fetch_result = match aether_agent::fetch::fetch_page(site.url, &config).await {
            Ok(r) => r,
            Err(e) => {
                println!("  FETCH ERROR: {}", e);
                continue;
            }
        };

        if fetch_result.body.len() < 100 {
            println!("  SKIP: body too small ({}B)", fetch_result.body.len());
            continue;
        }

        // Parse tree
        let tree = aether_agent::build_tree_for_crfr(
            &fetch_result.body,
            site.goals[0],
            &fetch_result.final_url,
            true,
        );
        let total_nodes = flatten_nodes(&tree.nodes).len();
        println!("  {} nodes parsed", total_nodes);

        if total_nodes < 5 {
            println!("  SKIP: too few nodes");
            continue;
        }

        // Build field (fresh — no cache)
        let (mut field, _) = resonance::get_or_build_field(&tree.nodes, site.url);

        let mut iterations = Vec::new();
        let mut feedback_total: usize = 0;
        let mut feedback_relevant: usize = 0;

        for (qi, goal) in site.goals.iter().enumerate() {
            let phase = match qi {
                0 => "BASELINE",
                1..=6 => "TRAIN",
                _ => "TEST",
            };

            // Propagate
            let results = field.propagate(goal);

            // Take only top-N (like parse_crfr does)
            let top_results: Vec<&ResonanceResult> = results.iter().take(TOP_N).collect();

            // Compute relevance for each result
            let node_labels: std::collections::HashMap<u32, String> = {
                let all = flatten_nodes(&tree.nodes);
                all.iter().map(|n| (n.id, n.label.clone())).collect()
            };

            let rels: Vec<f32> = top_results
                .iter()
                .map(|r| {
                    let label = node_labels
                        .get(&r.node_id)
                        .map(|s| s.as_str())
                        .unwrap_or("");
                    if is_relevant(label, site.keywords) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();

            let n5 = ndcg_at_k(&rels, 5);
            let m = mrr(&rels);
            let p = precision_at_k(&rels, 5);
            let causal = top_results.iter().filter(|r| r.causal_boost > 0.0).count();
            let relevant = rels.iter().filter(|&&r| r > 0.0).count();

            iterations.push(IterationResult {
                i: qi as u32 + 1,
                goal: goal.to_string(),
                phase: phase.to_string(),
                ndcg5: n5,
                mrr: m,
                p5: p,
                causal,
                relevant,
                total_returned: top_results.len(),
            });

            // Feedback only in TRAIN phase
            if phase == "TRAIN" {
                let successful: Vec<u32> = top_results
                    .iter()
                    .filter(|r| {
                        let label = node_labels
                            .get(&r.node_id)
                            .map(|s| s.as_str())
                            .unwrap_or("");
                        is_relevant(label, site.keywords)
                    })
                    .map(|r| r.node_id)
                    .collect();

                feedback_total += successful.len();
                feedback_relevant += successful.len(); // All feedback is keyword-verified
                if !successful.is_empty() {
                    field.feedback(goal, &successful);
                }
            }

            // Dump top-5 node content for Q1 (baseline) and Q8 (first test)
            if qi == 0 || qi == 7 {
                println!("    TOP-5 CONTENT:");
                for (rank, r) in top_results.iter().take(5).enumerate() {
                    let label = node_labels
                        .get(&r.node_id)
                        .map(|s| s.as_str())
                        .unwrap_or("?");
                    let short: String = label.chars().take(80).collect();
                    let rel = if rels.get(rank).copied().unwrap_or(0.0) > 0.0 {
                        "REL"
                    } else {
                        "---"
                    };
                    println!(
                        "    #{} [{}] id:{} amp={:.3} role={} \"{}\"",
                        rank + 1,
                        rel,
                        r.node_id,
                        r.amplitude,
                        node_labels.get(&r.node_id).map(|_| {
                            // Get role from tree
                            flatten_nodes(&tree.nodes)
                                .iter()
                                .find(|n| n.id == r.node_id)
                                .map(|n| n.role.as_str())
                                .unwrap_or("?")
                        }).unwrap_or("?"),
                        short
                    );
                }
            }

            let causal_str = if causal > 0 {
                format!(" caus={}", causal)
            } else {
                String::new()
            };
            println!(
                "  {} Q{}: nDCG@5={:.3} MRR={:.3} rel={}/{}{} \"{}\"",
                phase,
                qi + 1,
                n5,
                m,
                relevant,
                top_results.len(),
                causal_str,
                &goal[..goal.len().min(40)]
            );
        }

        // Compute aggregates
        let baseline = &iterations[0];
        let train_avg: f32 = iterations[1..7].iter().map(|it| it.ndcg5).sum::<f32>() / 6.0;
        let test_iters: Vec<&IterationResult> = iterations[7..].iter().collect();
        let test_ndcg: f32 =
            test_iters.iter().map(|it| it.ndcg5).sum::<f32>() / test_iters.len() as f32;
        let test_mrr: f32 =
            test_iters.iter().map(|it| it.mrr).sum::<f32>() / test_iters.len() as f32;
        let test_p5: f32 = test_iters.iter().map(|it| it.p5).sum::<f32>() / test_iters.len() as f32;
        let test_causal: f32 =
            test_iters.iter().map(|it| it.causal as f32).sum::<f32>() / test_iters.len() as f32;

        println!(
            "  => BL={:.3} TRAIN={:.3} TEST nDCG={:.3} MRR={:.3} P@5={:.3} causal={:.1}",
            baseline.ndcg5, train_avg, test_ndcg, test_mrr, test_p5, test_causal
        );
        println!();

        all_results.push(SiteResult {
            name: site.name.to_string(),
            url: site.url.to_string(),
            total_nodes,
            baseline_ndcg5: baseline.ndcg5,
            baseline_mrr: baseline.mrr,
            train_avg_ndcg5: train_avg,
            test_avg_ndcg5: test_ndcg,
            test_avg_mrr: test_mrr,
            test_avg_p5: test_p5,
            test_causal_avg: test_causal,
            feedback_total,
            feedback_relevant,
            iterations,
        });
    }

    // Summary
    println!("=== Summary ===");
    println!(
        "{:<22} {:>8} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "Site", "BL", "Train", "Test", "T-MRR", "T-P@5", "Causal"
    );
    println!("{:-<76}", "");
    for r in &all_results {
        println!(
            "{:<22} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.3} {:>8.1}",
            r.name,
            r.baseline_ndcg5,
            r.train_avg_ndcg5,
            r.test_avg_ndcg5,
            r.test_avg_mrr,
            r.test_avg_p5,
            r.test_causal_avg
        );
    }

    let n = all_results.len() as f32;
    let avg_bl: f32 = all_results.iter().map(|r| r.baseline_ndcg5).sum::<f32>() / n;
    let avg_train: f32 = all_results.iter().map(|r| r.train_avg_ndcg5).sum::<f32>() / n;
    let avg_test: f32 = all_results.iter().map(|r| r.test_avg_ndcg5).sum::<f32>() / n;
    let avg_mrr: f32 = all_results.iter().map(|r| r.test_avg_mrr).sum::<f32>() / n;
    println!("{:-<76}", "");
    println!(
        "{:<22} {:>8.3} {:>8.3} {:>8.3} {:>8.3}",
        "AVERAGE", avg_bl, avg_train, avg_test, avg_mrr
    );

    // JSON output
    let json = serde_json::to_string_pretty(&all_results).unwrap_or_default();
    std::fs::write("docs/convergence-v18-honest.json", &json).ok();
    println!("\nResults written to docs/convergence-v18-honest.json");
}
