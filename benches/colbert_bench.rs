/// Benchmark: ColBERT vs MiniLM Stage 3 Reranker
///
/// Jämför precision och hastighet mellan:
/// - MiniLM bi-encoder (default Stage 3)
/// - ColBERT MaxSim late interaction
/// - Hybrid (ColBERT × alpha + MiniLM × (1-alpha))
///
/// Kräver `colbert` feature och en lokal ColBERTv2-modell.
///
/// Run: cargo run --bin aether-colbert-bench --features colbert,embeddings
///
/// Referens: Khattab & Zaharia (2020), ColBERT: Efficient and Effective
/// Passage Search via Contextualized Late Interaction over BERT. SIGIR 2020.
use std::time::Instant;

use aether_agent::scoring::colbert_reranker::adaptive_alpha;
use aether_agent::scoring::pipeline::{PipelineConfig, ScoringPipeline};
use aether_agent::types::SemanticNode;

// ─── Testfall ────────────────────────────────────────────────────────────────

struct TestCase {
    name: &'static str,
    goal: &'static str,
    /// Nod-IDs som anses vara ground truth (relevanta svar)
    ground_truth_ids: Vec<u32>,
    html: String,
}

fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "coinmarketcap/bitcoin",
            goal: "bitcoin price USD today",
            ground_truth_ids: vec![3, 5, 7], // Prisnoder
            html: bitcoin_page(),
        },
        TestCase {
            name: "malmo.se/befolkning",
            goal: "antal invånare Malmö 2025",
            ground_truth_ids: vec![3, 4],
            html: malmo_page(),
        },
        TestCase {
            name: "gov.uk/minimum-wage",
            goal: "National Living Wage rate per hour",
            ground_truth_ids: vec![3, 5],
            html: wage_page(),
        },
        TestCase {
            name: "wikipedia/tim-cook",
            goal: "what year did Tim Cook become Apple CEO",
            ground_truth_ids: vec![4, 5],
            html: tim_cook_page(),
        },
        TestCase {
            name: "bankofengland",
            goal: "current Bank Rate percentage",
            ground_truth_ids: vec![3, 4],
            html: bank_rate_page(),
        },
        TestCase {
            name: "space.com/moon",
            goal: "distance Earth to Moon kilometres",
            ground_truth_ids: vec![3, 5],
            html: moon_page(),
        },
    ]
}

// ─── HTML-fixtures (simulerar verkliga sidor) ────────────────────────────────

fn bitcoin_page() -> String {
    r##"<html><body>
    <nav><a href="/">Home</a><a href="/currencies">Currencies</a><a href="/exchanges">Exchanges</a></nav>
    <h1>Bitcoin Price Live Data</h1>
    <div class="price-section">
        <span class="price">$66,825.42</span>
        <span class="change">+2.34% (24h)</span>
    </div>
    <div class="stats">
        <div class="stat"><span>Market Cap</span><span>$1.34T</span></div>
        <div class="stat"><span>Volume (24h)</span><span>$38.7B</span></div>
        <div class="stat"><span>Circulating Supply</span><span>19,625,000 BTC</span></div>
        <div class="stat"><span>Max Supply</span><span>21,000,000 BTC</span></div>
    </div>
    <table><tr><th>Rank</th><th>Name</th><th>Price</th><th>Market Cap</th></tr>
    <tr><td>1</td><td>Bitcoin</td><td>$66,825</td><td>$1.34T</td></tr>
    <tr><td>2</td><td>Ethereum</td><td>$3,421</td><td>$411B</td></tr></table>
    <footer><p>CoinMarketCap © 2025</p><a href="/terms">Terms</a></footer>
    </body></html>"##
        .to_string()
}

fn malmo_page() -> String {
    r##"<html><body>
    <nav><a href="/">Start</a><a href="/kommun">Kommun & politik</a></nav>
    <h1>Befolkning i Malmö</h1>
    <div class="fact-box">
        <p>Malmö har 357 377 invånare (2025-01-01).</p>
        <p>Folkmängden ökade med 4 231 personer under 2024.</p>
    </div>
    <div class="chart"><p>Befolkningsutveckling 2000-2025</p></div>
    <aside><p>Relaterat: Stadsdelar, Statistik, Öppna data</p></aside>
    <footer><a href="/cookies">Kakor</a><a href="/tillganglighet">Tillgänglighet</a></footer>
    </body></html>"##
        .to_string()
}

fn wage_page() -> String {
    r##"<html><body>
    <nav><a href="/">Home</a><a href="/browse">Browse</a></nav>
    <h1>National Minimum Wage and National Living Wage rates</h1>
    <table>
    <tr><th>Rate</th><th>From April 2025</th><th>Previous rate</th></tr>
    <tr><td>National Living Wage (21 and over)</td><td>£12.21 per hour</td><td>£11.44</td></tr>
    <tr><td>18-20 Year Old Rate</td><td>£10.00 per hour</td><td>£8.60</td></tr>
    <tr><td>16-17 Year Old Rate</td><td>£7.55 per hour</td><td>£6.40</td></tr>
    </table>
    <p>These rates are reviewed every year by the Low Pay Commission.</p>
    <footer><p>Crown copyright</p></footer>
    </body></html>"##
        .to_string()
}

fn tim_cook_page() -> String {
    r##"<html><body>
    <nav><a href="/">Main page</a><a href="/wiki/Random">Random article</a></nav>
    <h1>Tim Cook</h1>
    <div class="infobox">
        <p>Timothy Donald Cook (born November 1, 1960)</p>
        <p>CEO of Apple Inc. since August 24, 2011</p>
        <p>Preceded by: Steve Jobs</p>
    </div>
    <p>Cook joined Apple in March 1998 as senior vice president.</p>
    <p>He was named CEO on August 24, 2011, following Steve Jobs's resignation.</p>
    <div class="references"><a href="#ref1">[1]</a><a href="#ref2">[2]</a><a href="#ref3">[3]</a></div>
    <footer><a href="/terms">Terms of Use</a></footer>
    </body></html>"##
        .to_string()
}

fn bank_rate_page() -> String {
    r##"<html><body>
    <nav><a href="/">Home</a><a href="/monetary-policy">Monetary Policy</a></nav>
    <h1>Bank Rate</h1>
    <div class="rate-display">
        <p>The current Bank Rate is 4.50%</p>
        <p>This was set on 6 February 2025</p>
    </div>
    <p>Bank Rate determines the interest rate we pay to commercial banks.</p>
    <table><tr><th>Date</th><th>Rate</th></tr>
    <tr><td>Feb 2025</td><td>4.50%</td></tr>
    <tr><td>Nov 2024</td><td>4.75%</td></tr></table>
    <footer><p>Bank of England</p></footer>
    </body></html>"##
        .to_string()
}

fn moon_page() -> String {
    r##"<html><body>
    <nav><a href="/">Home</a><a href="/space">Space</a><a href="/moon">The Moon</a></nav>
    <h1>How Far Away Is the Moon?</h1>
    <div class="fact">
        <p>The average distance from Earth to the Moon is 384,400 kilometres (238,855 miles).</p>
    </div>
    <p>The Moon's orbit is elliptical. At perigee, it is 363,300 km away. At apogee, it is 405,500 km away.</p>
    <p>Light takes about 1.28 seconds to travel from Earth to the Moon.</p>
    <aside><p>Related: Sun distance, Mars distance, ISS orbit</p></aside>
    <footer><p>Space.com © 2025</p></footer>
    </body></html>"##
        .to_string()
}

// ─── Precision@K beräkning ───────────────────────────────────────────────────

fn precision_at_k(ranked_ids: &[u32], ground_truth: &[u32], k: usize) -> f32 {
    let top_k: Vec<u32> = ranked_ids.iter().take(k).copied().collect();
    let hits = top_k.iter().filter(|id| ground_truth.contains(id)).count();
    hits as f32 / k as f32
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║  ColBERT vs MiniLM — Stage 3 Reranker Benchmark             ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    let cases = test_cases();
    let config_minilm = PipelineConfig::default();

    let mut minilm_total_p3 = 0.0f32;
    let mut total_cases = 0;

    for case in &cases {
        println!("── {} ──", case.name);
        println!("  Goal: \"{}\"", case.goal);
        println!("  Ground truth IDs: {:?}", case.ground_truth_ids);

        // Parse HTML → SemanticTree
        let tree_json = aether_agent::parse_to_semantic_tree(case.html.as_str(), case.goal, "");
        let tree_nodes: Vec<SemanticNode> = match serde_json::from_str(&tree_json) {
            Ok(nodes) => nodes,
            Err(e) => {
                println!("  SKIP: kunde inte parsa HTML: {e}");
                continue;
            }
        };

        if tree_nodes.is_empty() {
            println!("  SKIP: tomt semantiskt träd");
            continue;
        }

        // ── MiniLM (baseline) ──
        let t0 = Instant::now();
        let minilm_result = ScoringPipeline::run(&tree_nodes, case.goal, None, &config_minilm);
        let minilm_ms = t0.elapsed().as_micros() as f64 / 1000.0;

        let minilm_ids: Vec<u32> = minilm_result.scored_nodes.iter().map(|n| n.id).collect();
        let minilm_p3 = precision_at_k(&minilm_ids, &case.ground_truth_ids, 3);

        println!(
            "  MiniLM:  P@3={:.2}  top3={:?}  {:.1}ms",
            minilm_p3,
            &minilm_ids[..minilm_ids.len().min(3)],
            minilm_ms
        );

        // ── ColBERT (om feature aktiv) ──
        #[cfg(feature = "colbert")]
        {
            use aether_agent::scoring::colbert_reranker::Stage3Reranker;

            let colbert_model_dir =
                std::env::var("COLBERT_MODEL_DIR").unwrap_or_else(|_| "models/colbertv2".into());
            let model_path = std::path::PathBuf::from(&colbert_model_dir);

            if model_path.join("config.json").exists() {
                let config_colbert = PipelineConfig {
                    stage3_reranker: Stage3Reranker::ColBert {
                        model_dir: model_path.clone(),
                    },
                    ..PipelineConfig::default()
                };

                let t1 = Instant::now();
                let colbert_result =
                    ScoringPipeline::run(&tree_nodes, case.goal, None, &config_colbert);
                let colbert_ms = t1.elapsed().as_micros() as f64 / 1000.0;

                let colbert_ids: Vec<u32> =
                    colbert_result.scored_nodes.iter().map(|n| n.id).collect();
                let colbert_p3 = precision_at_k(&colbert_ids, &case.ground_truth_ids, 3);

                println!(
                    "  ColBERT: P@3={:.2}  top3={:?}  {:.1}ms",
                    colbert_p3,
                    &colbert_ids[..colbert_ids.len().min(3)],
                    colbert_ms
                );

                // ── Hybrid ──
                let config_hybrid = PipelineConfig {
                    stage3_reranker: Stage3Reranker::Hybrid {
                        model_dir: model_path,
                        alpha: 0.7,
                        use_adaptive_alpha: true,
                    },
                    ..PipelineConfig::default()
                };

                let t2 = Instant::now();
                let hybrid_result =
                    ScoringPipeline::run(&tree_nodes, case.goal, None, &config_hybrid);
                let hybrid_ms = t2.elapsed().as_micros() as f64 / 1000.0;

                let hybrid_ids: Vec<u32> =
                    hybrid_result.scored_nodes.iter().map(|n| n.id).collect();
                let hybrid_p3 = precision_at_k(&hybrid_ids, &case.ground_truth_ids, 3);

                println!(
                    "  Hybrid:  P@3={:.2}  top3={:?}  {:.1}ms  (alpha=0.7, adaptive)",
                    hybrid_p3,
                    &hybrid_ids[..hybrid_ids.len().min(3)],
                    hybrid_ms
                );
            } else {
                println!(
                    "  ColBERT: SKIP — modell saknas (set COLBERT_MODEL_DIR eller placera i models/colbertv2/)"
                );
            }
        }

        #[cfg(not(feature = "colbert"))]
        {
            println!("  ColBERT: SKIP — kompilera med --features colbert");
        }

        minilm_total_p3 += minilm_p3;
        total_cases += 1;
        println!();
    }

    // ── Sammanfattning ──
    println!("═══════════════════════════════════════════════════════════════");
    println!(
        "MiniLM genomsnitt P@3: {:.2}",
        if total_cases > 0 {
            minilm_total_p3 / total_cases as f32
        } else {
            0.0
        }
    );
    println!();
    println!("adaptive_alpha-tabell:");
    for len in [5, 20, 50, 80, 100, 200, 300] {
        println!("  {len:>4} tokens → alpha={:.2}", adaptive_alpha(len));
    }
}
