/// ColBERT Quality Analysis — Faktisk nodkvalitet, inte bara scores
///
/// Testar specifika scenarier där ColBERT MaxSim borde slå bi-encoder:
/// 1. Långa noder med blandad info (fakta dolda bland brus)
/// 2. Tabeller med specifika datapunkter
/// 3. FAQ-sidor där svaret finns i en lång paragraf
/// 4. Nyhetssidor med många liknande rubriker
///
/// Visar FAKTISKA labels som varje reranker väljer — inte bara scores.
///
/// Run:
///   AETHER_EMBEDDING_MODEL=models/all-MiniLM-L6-v2.onnx \
///   AETHER_EMBEDDING_VOCAB=models/vocab.txt \
///   cargo run --release --bin aether-quality-analysis --features colbert
use aether_agent::scoring::colbert_reranker::Stage3Reranker;
use aether_agent::scoring::embed_score::ScoredNode;
use aether_agent::scoring::pipeline::{PipelineConfig, ScoringPipeline};
use aether_agent::types::SemanticNode;

struct QualityTest {
    name: &'static str,
    goal: &'static str,
    /// Det förväntade svaret (substring som borde finnas i top-1)
    expected_answer: &'static str,
    html: String,
}

fn quality_tests() -> Vec<QualityTest> {
    vec![
        // ── Test 1: Bitcoin-pris dolt bland navigation och metadata ──
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
  <p>Bitcoin is a decentralized cryptocurrency originally described in a 2008 whitepaper by a person, or group of people, using the alias Satoshi Nakamoto. It was launched soon after, in January 2009. Bitcoin is a peer-to-peer online currency, meaning that all transactions happen directly between equal, independent network participants, without the need for any intermediary to permit or facilitate them. Bitcoin was created, according to Nakamoto's own words, to allow online payments to be sent directly from one party to another without going through a financial institution.</p>
</div>
<table class="price-history">
  <tr><th>Date</th><th>Price</th><th>Volume</th><th>Market Cap</th></tr>
  <tr><td>Mar 31, 2026</td><td>$66,825</td><td>$38.7B</td><td>$1.34T</td></tr>
  <tr><td>Mar 30, 2026</td><td>$65,421</td><td>$35.2B</td><td>$1.31T</td></tr>
  <tr><td>Mar 29, 2026</td><td>$64,100</td><td>$29.8B</td><td>$1.28T</td></tr>
</table>
<div class="trending">
  <h3>Trending Coins</h3>
  <a href="/pepe">Pepe +15%</a> <a href="/floki">Floki +8%</a> <a href="/bonk">Bonk +12%</a>
</div>
<footer>
  <p>CoinMarketCap © 2026. All rights reserved. Terms Privacy Cookie Preferences</p>
  <div class="social">Twitter Telegram Instagram Facebook Reddit Discord</div>
</footer>
</body></html>"##.to_string(),
        },

        // ── Test 2: Specifik population dold i lång kommuntext ──
        QualityTest {
            name: "Invånarantal i lång kommuntext",
            goal: "antal invånare Malmö 2025",
            expected_answer: "357 377",
            html: r##"<html><body>
<nav><a href="/">Start</a><a href="/kommun">Kommun & politik</a><a href="/bo">Bo & miljö</a><a href="/utbildning">Utbildning</a></nav>
<div class="hero"><h1>Malmö – Sveriges tredje största stad</h1></div>
<div class="intro">
  <p>Malmö är en dynamisk stad i södra Sverige med en rik historia som sträcker sig tillbaka till medeltiden. Staden är känd för sin kulturella mångfald, sin moderna arkitektur och sitt strategiska läge vid Öresund. Malmö har genomgått en stor transformation från industristad till kunskapsstad under de senaste decennierna.</p>
</div>
<div class="facts">
  <h2>Fakta om Malmö</h2>
  <p>Malmö kommun har en area på 158,40 km². Staden är belägen i Skåne län och gränsar till Burlövs kommun, Vellinge kommun och Staffanstorps kommun. Malmös centralort har 357 377 invånare per den 1 januari 2025, vilket gör den till Sveriges tredje mest befolkade kommun efter Stockholm och Göteborg. Folkmängden ökade med 4 231 personer under 2024, motsvarande en tillväxt på 1,2%. Befolkningsprognosen för 2030 pekar mot 380 000 invånare.</p>
  <p>Malmö har ungefär 178 000 bostäder fördelat på hyresrätter, bostadsrätter och småhus. Den genomsnittliga månadshyran för en tvårumslägenhet är cirka 7 500 kronor.</p>
</div>
<div class="history">
  <h2>Malmös historia</h2>
  <p>Malmö omnämns första gången i skriftliga källor år 1275. Under Hansetiden var Malmö en viktig handelsstad. Sedan 1658 har Malmö tillhört Sverige efter freden i Roskilde. Staden industrialiserades kraftigt under 1800-talet med Kockums varv som största arbetsgivare.</p>
</div>
<aside>
  <h3>Relaterat</h3>
  <ul><li><a href="/stadsdelar">Stadsdelar</a></li><li><a href="/statistik">Statistik</a></li><li><a href="/oppna-data">Öppna data</a></li></ul>
</aside>
<footer><a href="/cookies">Kakor</a><a href="/tillganglighet">Tillgänglighet</a><a href="/kontakt">Kontakt</a></footer>
</body></html>"##.to_string(),
        },

        // ── Test 3: Bank Rate gömd bland policytext ──
        QualityTest {
            name: "Styrränta gömd i policytext",
            goal: "current Bank of England interest rate percentage",
            expected_answer: "4.50%",
            html: r##"<html><body>
<nav><a href="/">Home</a><a href="/monetary-policy">Monetary Policy</a><a href="/statistics">Statistics</a><a href="/news">News</a></nav>
<h1>Monetary Policy</h1>
<div class="overview">
  <p>The Monetary Policy Committee (MPC) sets monetary policy to meet the 2% inflation target, and in a way that helps to sustain growth and employment. At its meeting ending on 5 February 2025, the MPC voted by a majority of 7-2 to reduce Bank Rate by 0.25 percentage points, to 4.50%. Two members preferred to maintain Bank Rate at 4.75%.</p>
</div>
<div class="how-it-works">
  <h2>How monetary policy works</h2>
  <p>We set Bank Rate to influence other interest rates in the economy. This helps us to keep inflation low and stable, which supports economic growth. When inflation is too high, we typically raise Bank Rate to cool spending. When inflation is too low, we typically reduce Bank Rate to encourage spending. Our target for inflation, set by the Government, is 2% as measured by the Consumer Prices Index (CPI).</p>
  <p>Changes in Bank Rate affect the rates that banks and building societies charge for loans and mortgages, and the rates they pay on savings. It takes time — up to two years — for changes in Bank Rate to have their full effect on spending and inflation.</p>
</div>
<div class="minutes">
  <h2>MPC meeting minutes</h2>
  <p>The minutes of our February 2025 meeting show the Committee discussed global trade developments, labour market conditions, and the latest inflation projections before reaching its decision.</p>
</div>
<table><tr><th>Date</th><th>Bank Rate</th><th>Change</th></tr>
<tr><td>6 Feb 2025</td><td>4.50%</td><td>-0.25</td></tr>
<tr><td>7 Nov 2024</td><td>4.75%</td><td>-0.25</td></tr>
<tr><td>1 Aug 2024</td><td>5.00%</td><td>-0.25</td></tr>
</table>
<footer><p>Bank of England 2025. Threadneedle Street, London EC2R 8AH</p></footer>
</body></html>"##.to_string(),
        },

        // ── Test 4: Tim Cook CEO-år i lång Wikipedia-infobox ──
        QualityTest {
            name: "CEO-år i lång biografitext",
            goal: "what year did Tim Cook become CEO of Apple",
            expected_answer: "2011",
            html: r##"<html><body>
<nav><a href="/">Main page</a><a href="/wiki/Random">Random article</a><a href="/wiki/Portal:Current_events">Current events</a></nav>
<h1>Tim Cook</h1>
<div class="infobox">
  <table>
    <tr><th colspan="2">Tim Cook</th></tr>
    <tr><td>Born</td><td>Timothy Donald Cook, November 1, 1960 (age 65), Mobile, Alabama, U.S.</td></tr>
    <tr><td>Education</td><td>Auburn University (BS), Duke University (MBA)</td></tr>
    <tr><td>Title</td><td>CEO of Apple Inc.</td></tr>
    <tr><td>Term</td><td>August 24, 2011 – present</td></tr>
    <tr><td>Predecessor</td><td>Steve Jobs</td></tr>
    <tr><td>Board member of</td><td>Nike, Inc., National Football Foundation</td></tr>
  </table>
</div>
<div class="article">
  <p>Timothy Donald Cook (born November 1, 1960) is an American business executive who has been the chief executive officer of Apple Inc. since August 24, 2011. Cook previously served as the company's chief operating officer under its co-founder Steve Jobs.</p>
  <p>Cook joined Apple in March 1998 as senior vice president for worldwide operations, and then served as executive vice president for worldwide sales and operations. He was made chief operating officer by Jobs in 2007. On January 17, 2011, Apple's board of directors approved a third medical leave of absence requested by Jobs. During that time, Cook was responsible for most of Apple's day-to-day operations.</p>
  <p>After Jobs resigned as CEO and became chairman of the board, Cook was named as the new CEO of Apple on August 24, 2011. He has led Apple through many significant product launches including the iPhone 5, iPhone 6, Apple Watch, AirPods, Apple Silicon, and Apple Vision Pro.</p>
  <p>Under Cook's leadership, Apple became the first publicly traded U.S. company to be valued at over $1 trillion (August 2018), $2 trillion (August 2020), and $3 trillion (January 2022).</p>
</div>
<div class="references">
  <h2>References</h2>
  <ol><li><a href="#ref1">"Tim Cook Fast Facts". CNN.</a></li><li><a href="#ref2">"Apple's Tim Cook: The genius behind Steve". Fortune.</a></li></ol>
</div>
<div class="categories">Categories: 1960 births | Living people | Apple Inc. executives | Auburn University alumni | Duke University alumni | American chief executives</div>
<footer><a href="/terms">Terms of Use</a><a href="/privacy">Privacy Policy</a></footer>
</body></html>"##.to_string(),
        },

        // ── Test 5: Månens avstånd i populärvetenskaplig text ──
        QualityTest {
            name: "Specifikt avstånd i lång rymdtext",
            goal: "average distance from Earth to Moon in kilometers",
            expected_answer: "384,400",
            html: r##"<html><body>
<nav><a href="/">Home</a><a href="/space">Space</a><a href="/science">Science</a><a href="/tech">Technology</a></nav>
<h1>The Moon: Earth's Natural Satellite</h1>
<div class="intro">
  <p>The Moon is Earth's only natural satellite. It orbits our planet at a distance that varies because its orbit is not perfectly circular but elliptical. The Moon has fascinated humanity for millennia and was the target of the Apollo program, which successfully landed twelve astronauts on its surface between 1969 and 1972.</p>
</div>
<div class="distance-section">
  <h2>How Far Away Is the Moon?</h2>
  <p>The average distance from Earth to the Moon is approximately 384,400 kilometers (238,855 miles). However, this distance is not constant. At its closest approach (perigee), the Moon is about 363,300 km from Earth. At its farthest point (apogee), it reaches about 405,500 km. These variations are due to the Moon's elliptical orbit around Earth.</p>
  <p>To put this distance in perspective: light takes about 1.28 seconds to travel from Earth to the Moon. The Apollo missions took about 3 days to reach the Moon. If you could drive a car at highway speed (100 km/h) non-stop, it would take about 160 days to reach the Moon.</p>
</div>
<div class="facts">
  <h2>Moon Facts</h2>
  <table>
    <tr><td>Diameter</td><td>3,474.8 km</td></tr>
    <tr><td>Mass</td><td>7.342 × 10²² kg</td></tr>
    <tr><td>Surface gravity</td><td>1.62 m/s²</td></tr>
    <tr><td>Orbital period</td><td>27.3 days</td></tr>
    <tr><td>Average distance</td><td>384,400 km</td></tr>
  </table>
</div>
<aside><h3>Related</h3><a href="/sun">The Sun</a><a href="/mars">Mars</a><a href="/iss">ISS</a></aside>
<footer><p>Space.com © 2026</p></footer>
</body></html>"##.to_string(),
        },

        // ── Test 6: Lönedata i statlig tabell ──
        QualityTest {
            name: "Specifik lönesats i tabell bland policy",
            goal: "National Living Wage hourly rate 2025",
            expected_answer: "£12.21",
            html: r##"<html><body>
<nav><a href="/">Home</a><a href="/browse">Browse</a><a href="/government">Government</a></nav>
<div class="breadcrumb">Home &gt; Employment &gt; Pay and contracts &gt; National Minimum Wage</div>
<h1>National Minimum Wage and National Living Wage rates</h1>
<div class="intro">
  <p>The National Living Wage and National Minimum Wage rates change every April. The rates for April 2025 were announced by the Chancellor in the Autumn Statement. These rates are based on recommendations from the Low Pay Commission, an independent body that advises the Government about the minimum wage.</p>
</div>
<div class="guidance">
  <h2>Who gets the minimum wage</h2>
  <p>The National Living Wage is the minimum pay per hour for workers aged 21 and over. The National Minimum Wage is the minimum pay per hour for workers under 21 and apprentices. All employers must pay the correct minimum wage to their workers. There are penalties for not complying.</p>
</div>
<table class="rates">
  <caption>Current rates (from 1 April 2025)</caption>
  <tr><th>Category</th><th>Hourly rate</th><th>Annual increase</th></tr>
  <tr><td>National Living Wage (21 and over)</td><td>£12.21</td><td>6.7%</td></tr>
  <tr><td>18-20 Year Old Rate</td><td>£10.00</td><td>16.3%</td></tr>
  <tr><td>16-17 Year Old Rate</td><td>£7.55</td><td>18.0%</td></tr>
  <tr><td>Apprentice Rate</td><td>£7.55</td><td>18.0%</td></tr>
</table>
<div class="previous">
  <h2>Previous rates</h2>
  <table>
    <tr><th>Year</th><th>NLW (21+)</th></tr>
    <tr><td>2024-25</td><td>£11.44</td></tr>
    <tr><td>2023-24</td><td>£10.42</td></tr>
    <tr><td>2022-23</td><td>£9.50</td></tr>
  </table>
</div>
<footer><p>© Crown copyright. Open Government Licence v3.0.</p></footer>
</body></html>"##.to_string(),
        },
    ]
}

fn extract_tree(html: &str, goal: &str) -> Option<Vec<SemanticNode>> {
    let json = aether_agent::parse_to_semantic_tree(html, goal, "");
    let parsed: serde_json::Value = serde_json::from_str(&json).ok()?;
    let nodes_val = parsed.get("nodes")?.clone();
    serde_json::from_value(nodes_val).ok()
}

fn show_top_n(label: &str, nodes: &[ScoredNode], n: usize, expected: &str) {
    let found = nodes
        .iter()
        .take(n)
        .any(|node| node.label.contains(expected));
    let marker = if found { "FOUND" } else { "MISS" };
    println!("  {label} [{marker}]:");
    for (i, node) in nodes.iter().take(n).enumerate() {
        let trunc: String = node.label.chars().take(120).collect();
        let has_answer = if node.label.contains(expected) {
            " ◄◄◄"
        } else {
            ""
        };
        println!(
            "    {}. [{:.3}] [{}] {}{}",
            i + 1,
            node.relevance,
            node.role,
            trunc,
            has_answer
        );
    }
}

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║  ColBERT Quality Analysis — Actual Node Content Comparison          ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    #[cfg(feature = "embeddings")]
    {
        let mp = std::env::var("AETHER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".into());
        let vp =
            std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
        if let (Ok(mb), Ok(vt)) = (std::fs::read(&mp), std::fs::read_to_string(&vp)) {
            if aether_agent::embedding::init_global(&mb, &vt).is_ok() {
                println!("  Bi-encoder: LOADED ({})", mp);
            }
        }
        // Ladda separat ColBERT-modell om tillgänglig
        let cm = std::env::var("AETHER_COLBERT_MODEL")
            .unwrap_or_else(|_| "models/colbertv2-onnx/model.onnx".into());
        let cv = std::env::var("AETHER_COLBERT_VOCAB")
            .unwrap_or_else(|_| "models/colbertv2-onnx/vocab.txt".into());
        if let (Ok(cmb), Ok(cvt)) = (std::fs::read(&cm), std::fs::read_to_string(&cv)) {
            if aether_agent::embedding::init_colbert(&cmb, &cvt).is_ok() {
                println!("  ColBERT:    LOADED ({}, 768-dim)", cm);
            }
        } else {
            println!("  ColBERT:    using bi-encoder model (384-dim fallback)");
        }
    }

    let tests = quality_tests();
    let config_minilm = PipelineConfig::default();

    let mut minilm_found = 0;
    let mut colbert_found = 0;
    let mut hybrid_found = 0;
    let total = tests.len();

    for test in &tests {
        println!("═══════════════════════════════════════════════════════════════");
        println!("  TEST: {}", test.name);
        println!("  Goal: \"{}\"", test.goal);
        println!("  Expected answer contains: \"{}\"", test.expected_answer);
        println!();

        let tree = match extract_tree(&test.html, test.goal) {
            Some(t) => t,
            None => {
                println!("  SKIP: parse failed");
                continue;
            }
        };
        println!("  DOM: {} noder", tree.len());

        #[cfg(feature = "embeddings")]
        let goal_emb = aether_agent::embedding::embed(test.goal);
        #[cfg(not(feature = "embeddings"))]
        let goal_emb: Option<Vec<f32>> = None;

        // MiniLM
        let minilm_result =
            ScoringPipeline::run(&tree, test.goal, goal_emb.as_deref(), &config_minilm);
        let m_found = minilm_result
            .scored_nodes
            .iter()
            .take(3)
            .any(|n| n.label.contains(test.expected_answer));
        if m_found {
            minilm_found += 1;
        }
        show_top_n(
            "MiniLM (bi-encoder)",
            &minilm_result.scored_nodes,
            5,
            test.expected_answer,
        );
        println!();

        // ColBERT
        #[cfg(feature = "colbert")]
        {
            let config_colbert = PipelineConfig {
                stage3_reranker: Stage3Reranker::ColBert,
                ..PipelineConfig::default()
            };
            let colbert_result =
                ScoringPipeline::run(&tree, test.goal, goal_emb.as_deref(), &config_colbert);
            let c_found = colbert_result
                .scored_nodes
                .iter()
                .take(3)
                .any(|n| n.label.contains(test.expected_answer));
            if c_found {
                colbert_found += 1;
            }
            show_top_n(
                "ColBERT (MaxSim)",
                &colbert_result.scored_nodes,
                5,
                test.expected_answer,
            );
            println!();

            // Hybrid
            let config_hybrid = PipelineConfig {
                stage3_reranker: Stage3Reranker::Hybrid {
                    alpha: 0.7,
                    use_adaptive_alpha: true,
                },
                ..PipelineConfig::default()
            };
            let hybrid_result =
                ScoringPipeline::run(&tree, test.goal, goal_emb.as_deref(), &config_hybrid);
            let h_found = hybrid_result
                .scored_nodes
                .iter()
                .take(3)
                .any(|n| n.label.contains(test.expected_answer));
            if h_found {
                hybrid_found += 1;
            }
            show_top_n(
                "Hybrid (adaptive α)",
                &hybrid_result.scored_nodes,
                5,
                test.expected_answer,
            );
        }
        println!();
    }

    println!("═══════════════════════════════════════════════════════════════");
    println!("SAMMANFATTNING: Svar hittat i top-3 ({total} tester)");
    println!("  MiniLM:  {minilm_found}/{total}");
    println!("  ColBERT: {colbert_found}/{total}");
    println!("  Hybrid:  {hybrid_found}/{total}");
}
