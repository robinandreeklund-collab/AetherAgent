/// CRFR vs ColBERT Quality Comparison
///
/// Kör samma 6 kvalitetstester som colbert_quality_analysis.rs
/// men lägger till CRFR (Causal Resonance Field Retrieval) som
/// fjärde pipeline. Mäter: recall@3, latens, token-output.
///
/// Run:
///   AETHER_EMBEDDING_MODEL=models/all-MiniLM-L6-v2.onnx \
///   AETHER_EMBEDDING_VOCAB=models/vocab.txt \
///   cargo run --release --bin aether-crfr-vs-colbert --features colbert
///
/// Utan embeddings (bara CRFR + baseline):
///   cargo run --release --bin aether-crfr-vs-colbert
use std::time::Instant;

use aether_agent::resonance::ResonanceField;
use aether_agent::scoring::pipeline::{PipelineConfig, ScoringPipeline};
use aether_agent::types::SemanticNode;

// ─── Testdata ──────────────────────────────────────────────────────────────

struct QualityTest {
    name: &'static str,
    goal: &'static str,
    expected_answer: &'static str,
    html: String,
}

fn quality_tests() -> Vec<QualityTest> {
    vec![
        QualityTest {
            name: "Bitcoin pris i brusig sida",
            goal: "bitcoin price USD today",
            expected_answer: "$66,825",
            html: r##"<html><body>
<nav>
  <a href="/">Home</a><a href="/currencies">Currencies</a><a href="/exchanges">Exchanges</a>
  <a href="/nft">NFT</a><a href="/portfolio">Portfolio</a><a href="/watchlist">Watchlist</a>
  <div class="dropdown">Bitcoin Ethereum Solana Cardano Polkadot Dogecoin Shiba Avalanche</div>
</nav>
<div class="breadcrumb">Cryptocurrencies &gt; Bitcoin &gt; Price</div>
<h1>Bitcoin Price Live Data</h1>
<div class="hero-stats">
  <div class="stat-row">Bitcoin (BTC) price today is $66,825.42 with a 24-hour trading volume of $38.7B. BTC is +2.34% in the last 24 hours with a live market cap of $1.34T. It has a circulating supply of 19,625,000 BTC and a max supply of 21,000,000 BTC.</div>
</div>
<div class="converter">
  <p>Bitcoin to USD Converter: Enter amount in BTC to convert to US Dollars</p>
  <input type="number" placeholder="1 BTC">
  <span>= $66,825.42 USD</span>
</div>
<div class="about">
  <h2>About Bitcoin</h2>
  <p>Bitcoin is a decentralized cryptocurrency originally described in a 2008 whitepaper by a person, or group of people, using the alias Satoshi Nakamoto. It was launched soon after, in January 2009. Bitcoin is a peer-to-peer online currency, meaning that all transactions happen directly between equal, independent network participants, without the need for any intermediary to permit or facilitate them.</p>
</div>
<table class="price-history">
  <tr><th>Date</th><th>Price</th><th>Volume</th><th>Market Cap</th></tr>
  <tr><td>Mar 31, 2026</td><td>$66,825</td><td>$38.7B</td><td>$1.34T</td></tr>
  <tr><td>Mar 30, 2026</td><td>$65,421</td><td>$35.2B</td><td>$1.31T</td></tr>
</table>
<div class="trending">
  <h3>Trending Coins</h3>
  <a href="/pepe">Pepe +15%</a> <a href="/floki">Floki +8%</a> <a href="/bonk">Bonk +12%</a>
</div>
<footer>
  <p>CoinMarketCap 2026. All rights reserved. Terms Privacy Cookie Preferences</p>
  <div class="social">Twitter Telegram Instagram Facebook Reddit Discord</div>
</footer>
</body></html>"##
                .to_string(),
        },
        QualityTest {
            name: "Invånarantal i lång kommuntext",
            goal: "antal invånare Malmö 2025",
            expected_answer: "357 377",
            html: r##"<html><body>
<nav><a href="/">Start</a><a href="/kommun">Kommun & politik</a><a href="/bo">Bo & miljö</a></nav>
<div class="hero"><h1>Malmö -- Sveriges tredje största stad</h1></div>
<div class="intro">
  <p>Malmö är en dynamisk stad i södra Sverige med en rik historia som sträcker sig tillbaka till medeltiden. Staden är känd för sin kulturella mångfald, sin moderna arkitektur och sitt strategiska läge vid Öresund.</p>
</div>
<div class="facts">
  <h2>Fakta om Malmö</h2>
  <p>Malmö kommun har en area på 158,40 km². Staden är belägen i Skåne län. Malmös centralort har 357 377 invånare per den 1 januari 2025, vilket gör den till Sveriges tredje mest befolkade kommun efter Stockholm och Göteborg. Folkmängden ökade med 4 231 personer under 2024.</p>
  <p>Malmö har ungefär 178 000 bostäder fördelat på hyresrätter, bostadsrätter och småhus.</p>
</div>
<div class="history">
  <h2>Malmös historia</h2>
  <p>Malmö omnämns första gången i skriftliga källor år 1275. Under Hansetiden var Malmö en viktig handelsstad.</p>
</div>
<aside><h3>Relaterat</h3><ul><li><a href="/stadsdelar">Stadsdelar</a></li><li><a href="/statistik">Statistik</a></li></ul></aside>
<footer><a href="/cookies">Kakor</a><a href="/kontakt">Kontakt</a></footer>
</body></html>"##
                .to_string(),
        },
        QualityTest {
            name: "Styrränta gömd i policytext",
            goal: "current Bank of England interest rate percentage",
            expected_answer: "4.50%",
            html: r##"<html><body>
<nav><a href="/">Home</a><a href="/monetary-policy">Monetary Policy</a><a href="/statistics">Statistics</a></nav>
<h1>Monetary Policy</h1>
<div class="overview">
  <p>The Monetary Policy Committee (MPC) sets monetary policy to meet the 2% inflation target, and in a way that helps to sustain growth and employment. At its meeting ending on 5 February 2025, the MPC voted by a majority of 7-2 to reduce Bank Rate by 0.25 percentage points, to 4.50%. Two members preferred to maintain Bank Rate at 4.75%.</p>
</div>
<div class="how-it-works">
  <h2>How monetary policy works</h2>
  <p>We set Bank Rate to influence other interest rates in the economy. This helps us to keep inflation low and stable, which supports economic growth.</p>
  <p>Changes in Bank Rate affect the rates that banks and building societies charge for loans and mortgages.</p>
</div>
<table><tr><th>Date</th><th>Bank Rate</th><th>Change</th></tr>
<tr><td>6 Feb 2025</td><td>4.50%</td><td>-0.25</td></tr>
<tr><td>7 Nov 2024</td><td>4.75%</td><td>-0.25</td></tr>
</table>
<footer><p>Bank of England 2025. Threadneedle Street, London EC2R 8AH</p></footer>
</body></html>"##
                .to_string(),
        },
        QualityTest {
            name: "CEO-år i lång biografitext",
            goal: "what year did Tim Cook become CEO of Apple",
            expected_answer: "2011",
            html: r##"<html><body>
<nav><a href="/">Main page</a><a href="/wiki/Random">Random article</a></nav>
<h1>Tim Cook</h1>
<div class="infobox">
  <table>
    <tr><th colspan="2">Tim Cook</th></tr>
    <tr><td>Born</td><td>Timothy Donald Cook, November 1, 1960</td></tr>
    <tr><td>Title</td><td>CEO of Apple Inc.</td></tr>
    <tr><td>Term</td><td>August 24, 2011 -- present</td></tr>
    <tr><td>Predecessor</td><td>Steve Jobs</td></tr>
  </table>
</div>
<div class="article">
  <p>Timothy Donald Cook (born November 1, 1960) is an American business executive who has been the chief executive officer of Apple Inc. since August 24, 2011.</p>
  <p>Cook joined Apple in March 1998 as senior vice president. After Jobs resigned as CEO, Cook was named as the new CEO on August 24, 2011.</p>
  <p>Under Cook's leadership, Apple became the first publicly traded U.S. company valued at over $1 trillion.</p>
</div>
<div class="references"><h2>References</h2><ol><li><a href="#">Tim Cook Fast Facts. CNN.</a></li></ol></div>
<footer><a href="/terms">Terms of Use</a></footer>
</body></html>"##
                .to_string(),
        },
        QualityTest {
            name: "Specifikt avstånd i lång rymdtext",
            goal: "average distance from Earth to Moon in kilometers",
            expected_answer: "384,400",
            html: r##"<html><body>
<nav><a href="/">Home</a><a href="/space">Space</a><a href="/science">Science</a></nav>
<h1>The Moon: Earth's Natural Satellite</h1>
<div class="intro">
  <p>The Moon is Earth's only natural satellite. It orbits our planet at a distance that varies because its orbit is not perfectly circular but elliptical.</p>
</div>
<div class="distance-section">
  <h2>How Far Away Is the Moon?</h2>
  <p>The average distance from Earth to the Moon is approximately 384,400 kilometers (238,855 miles). At perigee: 363,300 km. At apogee: 405,500 km.</p>
  <p>Light takes about 1.28 seconds to travel from Earth to the Moon. The Apollo missions took about 3 days.</p>
</div>
<div class="facts">
  <h2>Moon Facts</h2>
  <table>
    <tr><td>Diameter</td><td>3,474.8 km</td></tr>
    <tr><td>Mass</td><td>7.342 x 10^22 kg</td></tr>
    <tr><td>Average distance</td><td>384,400 km</td></tr>
  </table>
</div>
<aside><h3>Related</h3><a href="/sun">The Sun</a><a href="/mars">Mars</a></aside>
<footer><p>Space.com 2026</p></footer>
</body></html>"##
                .to_string(),
        },
        QualityTest {
            name: "Specifik lönesats i tabell bland policy",
            goal: "National Living Wage hourly rate 2025",
            expected_answer: "£12.21",
            html: r##"<html><body>
<nav><a href="/">Home</a><a href="/browse">Browse</a><a href="/government">Government</a></nav>
<h1>National Minimum Wage and National Living Wage rates</h1>
<div class="intro">
  <p>The National Living Wage and National Minimum Wage rates change every April. These rates are based on recommendations from the Low Pay Commission.</p>
</div>
<div class="guidance">
  <h2>Who gets the minimum wage</h2>
  <p>The National Living Wage is the minimum pay per hour for workers aged 21 and over.</p>
</div>
<table class="rates">
  <caption>Current rates (from 1 April 2025)</caption>
  <tr><th>Category</th><th>Hourly rate</th><th>Annual increase</th></tr>
  <tr><td>National Living Wage (21 and over)</td><td>£12.21</td><td>6.7%</td></tr>
  <tr><td>18-20 Year Old Rate</td><td>£10.00</td><td>16.3%</td></tr>
  <tr><td>Apprentice Rate</td><td>£7.55</td><td>18.0%</td></tr>
</table>
<div class="previous">
  <h2>Previous rates</h2>
  <table>
    <tr><th>Year</th><th>NLW (21+)</th></tr>
    <tr><td>2024-25</td><td>£11.44</td></tr>
    <tr><td>2023-24</td><td>£10.42</td></tr>
  </table>
</div>
<footer><p>Crown copyright. Open Government Licence v3.0.</p></footer>
</body></html>"##
                .to_string(),
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

/// Rekursiv sökning: hitta en nod via ID i det nästlade trädet
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

/// Räkna totalt antal noder (inkl. barn)
fn count_nodes(nodes: &[SemanticNode]) -> usize {
    let mut count = nodes.len();
    for n in nodes {
        count += count_nodes(&n.children);
    }
    count
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║  CRFR vs Pipeline — Quality & Latency Comparison                    ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    // Ladda embedding-modeller om tillgängliga
    #[cfg(feature = "embeddings")]
    {
        let mp = std::env::var("AETHER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".into());
        let vp =
            std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
        if let (Ok(mb), Ok(vt)) = (std::fs::read(&mp), std::fs::read_to_string(&vp)) {
            if aether_agent::embedding::init_global(&mb, &vt).is_ok() {
                println!("  Embedding: LOADED ({})", mp);
            }
        }
        #[cfg(feature = "colbert")]
        {
            let cm = std::env::var("AETHER_COLBERT_MODEL")
                .unwrap_or_else(|_| "models/colbertv2-onnx/model.onnx".into());
            let cv = std::env::var("AETHER_COLBERT_VOCAB")
                .unwrap_or_else(|_| "models/colbertv2-onnx/vocab.txt".into());
            if let (Ok(cmb), Ok(cvt)) = (std::fs::read(&cm), std::fs::read_to_string(&cv)) {
                if aether_agent::embedding::init_colbert(&cmb, &cvt).is_ok() {
                    println!("  ColBERT:   LOADED ({})", cm);
                }
            } else {
                println!("  ColBERT:   using bi-encoder fallback (384-dim)");
            }
        }
    }
    println!();

    let tests = quality_tests();
    let total = tests.len();

    // Räknare per metod
    let mut crfr_found = 0u32;
    let mut crfr_total_us = 0u64;
    let mut crfr_total_nodes = 0usize;

    let mut pipeline_found = 0u32;
    let mut pipeline_total_us = 0u64;
    let mut pipeline_total_nodes = 0usize;

    // Kausal lärande: CRFR med feedback efter varje test
    let mut crfr_causal_found = 0u32;

    for (test_idx, test) in tests.iter().enumerate() {
        println!("═══ Test {}/{}: {} ═══", test_idx + 1, total, test.name);
        println!("  Goal: \"{}\"", test.goal);
        println!("  Expected: \"{}\"", test.expected_answer);

        let tree = match extract_tree(&test.html, test.goal) {
            Some(t) => t,
            None => {
                println!("  SKIP: parse failed\n");
                continue;
            }
        };
        let dom_nodes = count_nodes(&tree);
        println!("  DOM: {} noder\n", dom_nodes);

        // ─── CRFR (cold — inga kausal-minnen) ─────────────────────────

        let t0 = Instant::now();
        let mut field = ResonanceField::from_semantic_tree(&tree, "test");
        let crfr_results = field.propagate(test.goal);
        let crfr_us = t0.elapsed().as_micros() as u64;
        crfr_total_us += crfr_us;

        // Hitta labels via node_id-lookup i originalträdet
        let crfr_top: Vec<(u32, f32, String)> = crfr_results
            .iter()
            .take(5)
            .map(|r| {
                let label = find_node_by_id(&tree, r.node_id).unwrap_or_default();
                (r.node_id, r.amplitude, label)
            })
            .collect();

        let crfr_hit = crfr_top
            .iter()
            .take(3)
            .any(|(_, _, label)| label.contains(test.expected_answer));
        if crfr_hit {
            crfr_found += 1;
        }
        crfr_total_nodes += crfr_results.len();

        let crfr_marker = if crfr_hit { "FOUND" } else { "MISS" };
        println!(
            "  CRFR (cold) [{crfr_marker}] — {crfr_us} µs, {} noder i output:",
            crfr_results.len()
        );
        for (i, (_, amp, label)) in crfr_top.iter().enumerate() {
            let trunc: String = label.chars().take(100).collect();
            let hit = if label.contains(test.expected_answer) {
                " <<<"
            } else {
                ""
            };
            println!("    {}. [{:.3}] {}{}", i + 1, amp, trunc, hit);
        }
        println!();

        // ─── CRFR med kausal feedback (simulera lärande) ──────────────

        // Ge feedback på noder som innehöll svaret
        let success_ids: Vec<u32> = crfr_results
            .iter()
            .filter(|r| {
                find_node_by_id(&tree, r.node_id)
                    .unwrap_or_default()
                    .contains(test.expected_answer)
            })
            .map(|r| r.node_id)
            .collect();
        if !success_ids.is_empty() {
            field.feedback(test.goal, &success_ids);
        }

        // Kör igen med kausalt minne
        let crfr_causal = field.propagate(test.goal);
        let crfr_causal_top: Vec<(u32, f32, String)> = crfr_causal
            .iter()
            .take(5)
            .map(|r| {
                let label = find_node_by_id(&tree, r.node_id).unwrap_or_default();
                (r.node_id, r.amplitude, label)
            })
            .collect();

        let causal_hit = crfr_causal_top
            .iter()
            .take(3)
            .any(|(_, _, label)| label.contains(test.expected_answer));
        if causal_hit {
            crfr_causal_found += 1;
        }

        let causal_marker = if causal_hit { "FOUND" } else { "MISS" };
        println!("  CRFR (causal) [{causal_marker}]:");
        for (i, (_, amp, label)) in crfr_causal_top.iter().enumerate() {
            let trunc: String = label.chars().take(100).collect();
            let hit = if label.contains(test.expected_answer) {
                " <<<"
            } else {
                ""
            };
            println!("    {}. [{:.3}] {}{}", i + 1, amp, trunc, hit);
        }
        println!();

        // ─── Standard Pipeline (BM25 + HDC + Embedding) ──────────────

        #[cfg(feature = "embeddings")]
        let goal_emb = aether_agent::embedding::embed(test.goal);
        #[cfg(not(feature = "embeddings"))]
        let goal_emb: Option<Vec<f32>> = None;

        let config = PipelineConfig::default();
        let t1 = Instant::now();
        let pipeline_result = ScoringPipeline::run(&tree, test.goal, goal_emb.as_deref(), &config);
        let pipeline_us = t1.elapsed().as_micros() as u64;
        pipeline_total_us += pipeline_us;

        let pipeline_hit = pipeline_result
            .scored_nodes
            .iter()
            .take(3)
            .any(|n| n.label.contains(test.expected_answer));
        if pipeline_hit {
            pipeline_found += 1;
        }
        pipeline_total_nodes += pipeline_result.scored_nodes.len();

        let pipe_marker = if pipeline_hit { "FOUND" } else { "MISS" };
        println!(
            "  Pipeline (BM25+HDC+Embed) [{pipe_marker}] — {pipeline_us} µs, {} noder:",
            pipeline_result.scored_nodes.len()
        );
        for (i, node) in pipeline_result.scored_nodes.iter().take(5).enumerate() {
            let trunc: String = node.label.chars().take(100).collect();
            let hit = if node.label.contains(test.expected_answer) {
                " <<<"
            } else {
                ""
            };
            println!(
                "    {}. [{:.3}] [{}] {}{}",
                i + 1,
                node.relevance,
                node.role,
                trunc,
                hit
            );
        }
        println!();

        // ─── ColBERT (om tillgänglig) ─────────────────────────────────

        #[cfg(feature = "colbert")]
        {
            use aether_agent::scoring::colbert_reranker::Stage3Reranker;
            let config_colbert = PipelineConfig {
                stage3_reranker: Stage3Reranker::ColBert,
                ..PipelineConfig::default()
            };
            let t2 = Instant::now();
            let colbert_result =
                ScoringPipeline::run(&tree, test.goal, goal_emb.as_deref(), &config_colbert);
            let colbert_us = t2.elapsed().as_micros() as u64;

            let colbert_hit = colbert_result
                .scored_nodes
                .iter()
                .take(3)
                .any(|n| n.label.contains(test.expected_answer));
            let cb_marker = if colbert_hit { "FOUND" } else { "MISS" };
            println!(
                "  ColBERT (MaxSim) [{cb_marker}] — {colbert_us} µs, {} noder:",
                colbert_result.scored_nodes.len()
            );
            for (i, node) in colbert_result.scored_nodes.iter().take(5).enumerate() {
                let trunc: String = node.label.chars().take(100).collect();
                let hit = if node.label.contains(test.expected_answer) {
                    " <<<"
                } else {
                    ""
                };
                println!(
                    "    {}. [{:.3}] [{}] {}{}",
                    i + 1,
                    node.relevance,
                    node.role,
                    trunc,
                    hit
                );
            }
            println!();
        }
    }

    // ─── Sammanfattning ─────────────────────────────────────────────────────

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("SAMMANFATTNING ({total} tester)\n");

    println!(
        "  {:<25} {:>10} {:>10} {:>12} {:>10}",
        "Metod", "Recall@3", "Avg µs", "Avg output", "Noder/q"
    );
    println!("  {}", "-".repeat(70));
    println!(
        "  {:<25} {:>7}/{:<2} {:>8} µs {:>10} {:>10}",
        "CRFR (cold)",
        crfr_found,
        total,
        crfr_total_us / total as u64,
        format!("{:.1}", crfr_total_nodes as f64 / total as f64),
        ""
    );
    println!(
        "  {:<25} {:>7}/{:<2} {:>8}    {:>10} {:>10}",
        "CRFR (causal feedback)", crfr_causal_found, total, "—", "—", ""
    );
    println!(
        "  {:<25} {:>7}/{:<2} {:>8} µs {:>10} {:>10}",
        "Pipeline (BM25+HDC+Embed)",
        pipeline_found,
        total,
        pipeline_total_us / total as u64,
        format!("{:.1}", pipeline_total_nodes as f64 / total as f64),
        ""
    );

    println!("\n  Speedup CRFR vs Pipeline: {:.1}x", {
        if crfr_total_us > 0 {
            pipeline_total_us as f64 / crfr_total_us as f64
        } else {
            f64::INFINITY
        }
    });

    let crfr_avg_output = crfr_total_nodes as f64 / total as f64;
    let pipe_avg_output = pipeline_total_nodes as f64 / total as f64;
    if pipe_avg_output > 0.0 {
        println!(
            "  Token-reduktion CRFR vs Pipeline: {:.0}% färre noder",
            (1.0 - crfr_avg_output / pipe_avg_output) * 100.0
        );
    }

    println!();
}
