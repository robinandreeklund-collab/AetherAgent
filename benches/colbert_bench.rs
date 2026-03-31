/// Benchmark: ColBERT vs MiniLM Stage 3 Reranker
///
/// Jämför precision och hastighet mellan:
/// - MiniLM bi-encoder (default Stage 3)
/// - ColBERT MaxSim late interaction
/// - Hybrid (ColBERT × alpha + MiniLM × (1-alpha))
///
/// Kräver `colbert` feature och en lokal ColBERTv2-modell.
///
/// Run: cargo run --release --bin aether-colbert-bench --features colbert,embeddings
///
/// Referens: Khattab & Zaharia (2020), ColBERT: Efficient and Effective
/// Passage Search via Contextualized Late Interaction over BERT. SIGIR 2020.
use std::time::Instant;

use aether_agent::scoring::colbert_reranker::adaptive_alpha;
use aether_agent::scoring::embed_score::ScoredNode;
use aether_agent::scoring::pipeline::{PipelineConfig, ScoringPipeline};
use aether_agent::types::SemanticNode;

// ─── Testfall ────────────────────────────────────────────────────────────────

struct TestCase {
    name: &'static str,
    goal: &'static str,
    /// Nyckelord som BÖR finnas i top-3 labels — keyword-baserad ground truth
    expected_keywords: Vec<&'static str>,
    html: String,
}

fn test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            name: "coinmarketcap/bitcoin",
            goal: "bitcoin price USD today",
            expected_keywords: vec!["price", "$66,825", "bitcoin"],
            html: bitcoin_page(),
        },
        TestCase {
            name: "malmo.se/befolkning",
            goal: "antal invånare Malmö 2025",
            expected_keywords: vec!["invånare", "357", "folkmängd"],
            html: malmo_page(),
        },
        TestCase {
            name: "gov.uk/minimum-wage",
            goal: "National Living Wage rate per hour",
            expected_keywords: vec!["£12.21", "living wage", "per hour"],
            html: wage_page(),
        },
        TestCase {
            name: "wikipedia/tim-cook",
            goal: "what year did Tim Cook become Apple CEO",
            expected_keywords: vec!["2011", "ceo", "august"],
            html: tim_cook_page(),
        },
        TestCase {
            name: "bankofengland",
            goal: "current Bank Rate percentage",
            expected_keywords: vec!["4.50%", "bank rate", "rate"],
            html: bank_rate_page(),
        },
        TestCase {
            name: "space.com/moon",
            goal: "distance Earth to Moon kilometres",
            expected_keywords: vec!["384,400", "kilomet", "distance"],
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

// ─── Keyword-match scoring ───────────────────────────────────────────────────

/// Räkna hur många keywords som finns i top-K labels (case-insensitive)
fn keyword_hits_at_k(top_nodes: &[ScoredNode], keywords: &[&str], k: usize) -> (usize, usize) {
    let top_labels: Vec<String> = top_nodes
        .iter()
        .take(k)
        .map(|n| n.label.to_lowercase())
        .collect();
    let concat = top_labels.join(" ");
    let hits = keywords
        .iter()
        .filter(|kw| concat.contains(&kw.to_lowercase()))
        .count();
    (hits, keywords.len())
}

fn format_top3(nodes: &[ScoredNode]) -> String {
    nodes
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, n)| {
            let trunc: String = n.label.chars().take(70).collect();
            format!("    {}. [{:.3}] {}", i + 1, n.relevance, trunc)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║  ColBERT vs MiniLM — Stage 3 Reranker Benchmark             ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");
    println!();

    // Initiera embeddings om tillgängliga
    #[cfg(feature = "embeddings")]
    {
        let model_path = std::env::var("AETHER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".to_string());
        let vocab_path = std::env::var("AETHER_EMBEDDING_VOCAB")
            .unwrap_or_else(|_| "models/vocab.txt".to_string());
        if let (Ok(model_bytes), Ok(vocab_text)) = (
            std::fs::read(&model_path),
            std::fs::read_to_string(&vocab_path),
        ) {
            if aether_agent::embedding::init_global(&model_bytes, &vocab_text).is_ok() {
                println!("  Embeddings: LOADED ({model_path})");
            }
        } else {
            println!("  Embeddings: NOT FOUND (text-only scoring)");
        }
    }

    let cases = test_cases();
    let config_minilm = PipelineConfig::default();

    // Compute goal embeddings
    #[cfg(feature = "embeddings")]
    let goal_embeddings: Vec<Option<Vec<f32>>> = cases
        .iter()
        .map(|c| aether_agent::embedding::embed(c.goal))
        .collect();
    #[cfg(not(feature = "embeddings"))]
    let goal_embeddings: Vec<Option<Vec<f32>>> = cases.iter().map(|_| None).collect();

    let mut minilm_wins = 0u32;
    #[cfg(feature = "colbert")]
    let mut colbert_wins = 0u32;
    #[cfg(feature = "colbert")]
    let mut hybrid_wins = 0u32;
    let mut total_cases = 0u32;

    for (idx, case) in cases.iter().enumerate() {
        println!("── {} ──", case.name);
        println!("  Goal: \"{}\"", case.goal);
        println!("  Keywords: {:?}", case.expected_keywords);

        // Parse HTML → SemanticTree
        let tree_json = aether_agent::parse_to_semantic_tree(case.html.as_str(), case.goal, "");
        let parsed: serde_json::Value = match serde_json::from_str(&tree_json) {
            Ok(v) => v,
            Err(e) => {
                println!("  SKIP: JSON parse error: {e}");
                continue;
            }
        };
        let nodes_value = parsed
            .get("nodes")
            .cloned()
            .unwrap_or(serde_json::Value::Array(vec![]));
        let tree_nodes: Vec<SemanticNode> = match serde_json::from_value(nodes_value) {
            Ok(nodes) => nodes,
            Err(e) => {
                println!("  SKIP: node deserialization error: {e}");
                continue;
            }
        };

        if tree_nodes.is_empty() {
            println!("  SKIP: tomt semantiskt träd");
            continue;
        }

        println!("  Noder: {} st", tree_nodes.len());

        let goal_emb = goal_embeddings[idx].as_deref();

        // ── MiniLM (baseline) ──
        let t0 = Instant::now();
        let minilm_result = ScoringPipeline::run(&tree_nodes, case.goal, goal_emb, &config_minilm);
        let minilm_ms = t0.elapsed().as_micros() as f64 / 1000.0;

        let (m_hits, m_total) =
            keyword_hits_at_k(&minilm_result.scored_nodes, &case.expected_keywords, 3);
        println!(
            "  MiniLM:  {}/{} keywords  {:.1}ms",
            m_hits, m_total, minilm_ms
        );
        println!("{}", format_top3(&minilm_result.scored_nodes));

        #[cfg(feature = "colbert")]
        let mut best_hits = m_hits;

        // ── ColBERT (ONNX, samma modell som bi-encoder) ──
        #[cfg(feature = "colbert")]
        {
            use aether_agent::scoring::colbert_reranker::Stage3Reranker;

            if aether_agent::embedding::is_loaded() {
                let config_colbert = PipelineConfig {
                    stage3_reranker: Stage3Reranker::ColBert,
                    ..PipelineConfig::default()
                };

                let t1 = Instant::now();
                let colbert_result =
                    ScoringPipeline::run(&tree_nodes, case.goal, goal_emb, &config_colbert);
                let colbert_ms = t1.elapsed().as_micros() as f64 / 1000.0;

                let (c_hits, c_total) =
                    keyword_hits_at_k(&colbert_result.scored_nodes, &case.expected_keywords, 3);
                println!(
                    "  ColBERT: {}/{} keywords  {:.1}ms",
                    c_hits, c_total, colbert_ms
                );
                println!("{}", format_top3(&colbert_result.scored_nodes));

                if c_hits > best_hits {
                    best_hits = c_hits;
                    colbert_wins += 1;
                }

                // ── Hybrid ──
                let config_hybrid = PipelineConfig {
                    stage3_reranker: Stage3Reranker::Hybrid {
                        alpha: 0.7,
                        use_adaptive_alpha: true,
                    },
                    ..PipelineConfig::default()
                };

                let t2 = Instant::now();
                let hybrid_result =
                    ScoringPipeline::run(&tree_nodes, case.goal, goal_emb, &config_hybrid);
                let hybrid_ms = t2.elapsed().as_micros() as f64 / 1000.0;

                let (h_hits, h_total) =
                    keyword_hits_at_k(&hybrid_result.scored_nodes, &case.expected_keywords, 3);
                println!(
                    "  Hybrid:  {}/{} keywords  {:.1}ms  (adaptive alpha)",
                    h_hits, h_total, hybrid_ms
                );
                println!("{}", format_top3(&hybrid_result.scored_nodes));

                if h_hits > best_hits {
                    hybrid_wins += 1;
                } else if h_hits == best_hits && h_hits > m_hits {
                    hybrid_wins += 1;
                }
            } else {
                println!("  ColBERT: SKIP — embeddings ej laddade");
            }
        }

        #[cfg(not(feature = "colbert"))]
        {
            println!("  ColBERT: SKIP — kompilera med --features colbert");
        }

        #[cfg(feature = "colbert")]
        if m_hits >= best_hits {
            minilm_wins += 1;
        }
        #[cfg(not(feature = "colbert"))]
        {
            minilm_wins += 1;
        }
        total_cases += 1;
        println!();
    }

    // ── Sammanfattning ──
    println!("═══════════════════════════════════════════════════════════════");
    println!("Resultat ({total_cases} testfall):");
    println!("  MiniLM bäst/delad:  {minilm_wins}/{total_cases}");
    #[cfg(feature = "colbert")]
    println!("  ColBERT bäst:       {colbert_wins}/{total_cases}");
    #[cfg(feature = "colbert")]
    println!("  Hybrid bäst:        {hybrid_wins}/{total_cases}");
    println!();
    println!("adaptive_alpha-tabell:");
    for len in [5, 20, 50, 80, 100, 200, 300] {
        println!("  {len:>4} tokens → alpha={:.2}", adaptive_alpha(len));
    }
}
