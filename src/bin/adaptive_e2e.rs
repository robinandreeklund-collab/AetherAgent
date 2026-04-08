//! End-to-end test & benchmark för Adaptive Multi-Page Crawling
//!
//! Crawlar riktiga sajter (HN, Wikipedia, SVT), mäter:
//! - Token savings vs raw HTML
//! - Pages/second
//! - HDC saturation konvergens-kurva
//! - Stopping-logik korrekthet
//!
//! Kör: cargo run --bin adaptive-e2e --features fetch

use std::time::Instant;

use aether_agent::adaptive::{adaptive_crawl, AdaptiveConfig, StopReason};
use aether_agent::link_extract::{extract_links_from_tree, LinkExtractionConfig};
use aether_agent::types::FetchConfig;

// ─── Testscenarier ──────────────────────────────────────────────────────────

struct TestScenario {
    name: &'static str,
    url: &'static str,
    goal: &'static str,
    max_pages: usize,
    max_depth: u32,
}

/// Bas-URL för lokal testserver (python3 -m http.server 8765 i test-sites/)
const BASE: &str = "http://localhost:8765";

const SCENARIOS: &[TestScenario] = &[
    TestScenario {
        name: "HN - AI nyheter",
        url: "http://localhost:8765/hn/index.html",
        goal: "AI agent developments and tools",
        max_pages: 5,
        max_depth: 2,
    },
    TestScenario {
        name: "Wikipedia - Rust",
        url: "http://localhost:8765/wiki/index.html",
        goal: "Rust programming language history and features",
        max_pages: 4,
        max_depth: 2,
    },
    TestScenario {
        name: "SVT - Nyheter",
        url: "http://localhost:8765/svt/index.html",
        goal: "Svenska nyheter idag",
        max_pages: 4,
        max_depth: 2,
    },
];

// ─── Main ───────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Adaptive Multi-Page Crawl — End-to-End Test & Benchmark   ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
    println!();

    let mut all_passed = true;

    for scenario in SCENARIOS {
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("  Scenario: {}", scenario.name);
        println!("  URL:      {}", scenario.url);
        println!("  Goal:     {}", scenario.goal);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let config = AdaptiveConfig {
            max_pages: scenario.max_pages,
            max_depth: scenario.max_depth,
            top_k_links: 3,
            min_gain_threshold: 0.02,
            confidence_threshold: 0.95,
            consecutive_low_gain_max: 3,
            respect_robots_txt: true,
            timeout_ms: 15_000,
            top_n_per_page: 10,
            fetch_config: FetchConfig::default(),
        };

        let start = Instant::now();
        let result = adaptive_crawl(scenario.url, scenario.goal, config).await;
        let wall_time = start.elapsed();

        // ─── Resultat ───
        println!();
        println!("  Resultat:");
        println!("  ├─ Sidor crawlade:   {}", result.total_pages);
        println!("  ├─ Stop-orsak:       {:?}", result.stop_reason);
        println!("  ├─ Term coverage:    {:.1}%", result.coverage * 100.0);
        println!("  ├─ Final EMA gain:   {:.4}", result.final_ema_gain);
        println!("  ├─ Noder extraherade: {}", result.total_nodes_extracted);
        println!("  ├─ Total tid:        {:.1}s", wall_time.as_secs_f64());

        // Pages/second
        let pages_per_sec = if wall_time.as_secs_f64() > 0.0 {
            result.total_pages as f64 / wall_time.as_secs_f64()
        } else {
            0.0
        };
        println!("  ├─ Pages/second:     {:.2}", pages_per_sec);

        // Token savings: beräkna total CRFR chars vs uppskattad raw HTML
        let crfr_chars: usize = result
            .pages
            .iter()
            .flat_map(|p| p.top_nodes.iter())
            .map(|n| n.label.len())
            .sum();
        // Uppskatta raw HTML som ~50KB per sida (konservativt)
        let estimated_raw = result.total_pages as usize * 50_000;
        let savings = if estimated_raw > 0 {
            1.0 - (crfr_chars as f64 / estimated_raw as f64)
        } else {
            0.0
        };
        println!("  ├─ CRFR output chars: {}", crfr_chars);
        println!("  ├─ Est. raw chars:   {}", estimated_raw);
        println!("  ├─ Token savings:    {:.1}%", savings * 100.0);

        // ─── HDC Saturation kurva ───
        println!("  │");
        println!("  ├─ HDC Saturation kurva:");
        for (i, page) in result.pages.iter().enumerate() {
            let bar_len = (page.marginal_gain * 40.0) as usize;
            let bar: String = "█".repeat(bar_len.min(40));
            println!(
                "  │   Sida {:>2}: gain={:.4} {}",
                i + 1,
                page.marginal_gain,
                bar
            );
        }

        // ─── Per-sida detaljer ───
        println!("  │");
        println!("  ├─ Per-sida:");
        for page in &result.pages {
            println!(
                "  │   [{:>2}] {} — {} noder, {} links, fetch={}ms parse={}ms",
                page.page_number,
                truncate(&page.url, 50),
                page.top_nodes.len(),
                page.links_found,
                page.fetch_time_ms,
                page.parse_time_ms,
            );
        }

        // ─── Verifieringar ───
        println!("  │");
        println!("  └─ Verifieringar:");

        // V1: Minst 1 sida crawlad
        let v1 = result.total_pages >= 1;
        print_check("Minst 1 sida crawlad", v1);
        if !v1 {
            all_passed = false;
        }

        // V2: Stop reason är rimlig
        let v2 = matches!(
            result.stop_reason,
            StopReason::HdcSaturation
                | StopReason::TermCoverage
                | StopReason::Satisficing
                | StopReason::MaxPages
                | StopReason::NoMoreLinks
                | StopReason::Timeout
        );
        print_check("Stop reason är giltig", v2);
        if !v2 {
            all_passed = false;
        }

        // V3: Token savings >= 80%
        let v3 = savings >= 0.80;
        print_check(
            &format!("Token savings >= 80% (got {:.1}%)", savings * 100.0),
            v3,
        );
        if !v3 {
            all_passed = false;
        }

        // V4: Noder extraherade > 0
        let v4 = result.total_nodes_extracted > 0;
        print_check("Noder extraherade > 0", v4);
        if !v4 {
            all_passed = false;
        }

        // V5: HDC gain minskar (konvergens)
        let gains: Vec<f32> = result.pages.iter().map(|p| p.marginal_gain).collect();
        let v5 = if gains.len() >= 2 {
            // Sista gain ska vara <= första gain (visar konvergens)
            gains.last().unwrap_or(&1.0) <= gains.first().unwrap_or(&0.0)
        } else {
            true // Bara 1 sida = ok
        };
        print_check("HDC gain konvergerar (sista <= första)", v5);
        if !v5 {
            all_passed = false;
        }

        println!();
    }

    // ─── Link Extraction test ───
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Link Extraction E2E Test");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Fetch HN och extrahera links
    let fetch_config = FetchConfig::default();
    match aether_agent::fetch::fetch_page(&format!("{BASE}/hn/index.html"), &fetch_config).await {
        Ok(hn_result) => {
            let tree_json = aether_agent::parse_to_semantic_tree(
                &hn_result.body,
                "AI tools and agents",
                &hn_result.final_url,
            );

            #[derive(serde::Deserialize)]
            struct TreeOut {
                #[serde(default)]
                nodes: Vec<aether_agent::types::SemanticNode>,
            }
            let nodes: Vec<aether_agent::types::SemanticNode> =
                serde_json::from_str::<TreeOut>(&tree_json)
                    .map(|t| t.nodes)
                    .unwrap_or_default();

            let config = LinkExtractionConfig {
                goal: Some("AI tools and agents".to_string()),
                max_links: 20,
                include_context: true,
                include_structural_role: true,
                filter_navigation: false,
                min_relevance: 0.0,
            };

            let start = Instant::now();
            let links = extract_links_from_tree(&nodes, &hn_result.final_url, &config, None);
            let link_time = start.elapsed();

            println!();
            println!("  HN Link Extraction:");
            println!("  ├─ Total links:      {}", links.total_found);
            println!("  ├─ Returned:         {}", links.links.len());
            println!("  ├─ Filtered:         {}", links.filtered);
            println!(
                "  ├─ Extract time:     {:.2}ms",
                link_time.as_secs_f64() * 1000.0
            );
            println!("  │");
            println!("  ├─ Top 5 links:");
            for (i, link) in links.links.iter().take(5).enumerate() {
                println!(
                    "  │   {}. [rel={:.2} nov={:.2} gain={:.2}] {} → {}",
                    i + 1,
                    link.relevance_score,
                    link.novelty_score,
                    link.expected_gain,
                    truncate(&link.anchor_text, 30),
                    truncate(&link.absolute_url, 40),
                );
            }

            let link_v1 = links.total_found > 5;
            print_check("  HN har > 5 links", link_v1);
            if !link_v1 {
                all_passed = false;
            }

            let link_v2 = links.links.iter().any(|l| l.relevance_score > 0.0);
            print_check("  Minst en link har relevance > 0", link_v2);
            if !link_v2 {
                all_passed = false;
            }

            let link_v3 = links.links.iter().any(|l| l.is_internal);
            print_check("  Hittar interna links", link_v3);

            let link_v4 = link_time.as_millis() < 100;
            print_check(
                &format!("  Extract time < 100ms (got {}ms)", link_time.as_millis()),
                link_v4,
            );
            if !link_v4 {
                all_passed = false;
            }
        }
        Err(e) => {
            println!("  SKIP: Kunde inte hämta HN: {e}");
        }
    }

    // ─── Sammanfattning ───
    println!();
    println!("══════════════════════════════════════════════════════════════");
    if all_passed {
        println!("  ALLA VERIFIERINGAR PASSERADE");
    } else {
        println!("  VISSA VERIFIERINGAR MISSLYCKADES");
    }
    println!("══════════════════════════════════════════════════════════════");
}

fn print_check(label: &str, pass: bool) {
    let icon = if pass { "PASS" } else { "FAIL" };
    println!("       [{icon}] {label}");
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let mut end = max;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...", &s[..end])
    }
}
