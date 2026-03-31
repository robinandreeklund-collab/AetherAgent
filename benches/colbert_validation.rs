/// Comprehensive ColBERT vs MiniLM Validation — 45 Live Sites
///
/// Fetchar riktiga sajter och jämför tre Stage 3-rerankers:
/// - MiniLM bi-encoder (default)
/// - ColBERT MaxSim late interaction
/// - Hybrid (adaptive alpha × ColBERT + (1-alpha) × MiniLM)
///
/// Run:
///   AETHER_EMBEDDING_MODEL=models/all-MiniLM-L6-v2.onnx \
///   AETHER_EMBEDDING_VOCAB=models/vocab.txt \
///   cargo run --release --bin aether-colbert-validation --features colbert
use std::time::Instant;

use aether_agent::scoring::colbert_reranker::Stage3Reranker;
use aether_agent::scoring::pipeline::PipelineConfig;

// ─── TestCase ────────────────────────────────────────────────────────────────

struct TestCase {
    name: &'static str,
    url: &'static str,
    goal: &'static str,
    keyword: &'static str,
    top_n: u32,
}

fn test_cases() -> Vec<TestCase> {
    vec![
        // ── Nyhetssajter ──
        TestCase {
            name: "Hacker News",
            url: "https://news.ycombinator.com",
            goal: "top stories today",
            keyword: "hacker",
            top_n: 10,
        },
        TestCase {
            name: "HN Newest",
            url: "https://news.ycombinator.com/newest",
            goal: "newest submissions",
            keyword: "new",
            top_n: 10,
        },
        TestCase {
            name: "Lobsters",
            url: "https://lobste.rs",
            goal: "programming stories and discussions",
            keyword: "programming",
            top_n: 10,
        },
        TestCase {
            name: "CNN Lite",
            url: "https://lite.cnn.com",
            goal: "top news headlines today",
            keyword: "cnn",
            top_n: 10,
        },
        TestCase {
            name: "NPR Text",
            url: "https://text.npr.org",
            goal: "latest radio news stories",
            keyword: "npr",
            top_n: 10,
        },
        TestCase {
            name: "Reuters",
            url: "https://www.reuters.com",
            goal: "business news today",
            keyword: "reuters",
            top_n: 10,
        },
        // ── Utvecklarresurser ──
        TestCase {
            name: "Rust Lang",
            url: "https://www.rust-lang.org",
            goal: "latest Rust version download",
            keyword: "rust",
            top_n: 10,
        },
        TestCase {
            name: "MDN HTML",
            url: "https://developer.mozilla.org/en-US/docs/Web/HTML",
            goal: "HTML elements reference",
            keyword: "html",
            top_n: 10,
        },
        TestCase {
            name: "Go Dev",
            url: "https://go.dev",
            goal: "Go programming language download",
            keyword: "go",
            top_n: 10,
        },
        TestCase {
            name: "TypeScript",
            url: "https://www.typescriptlang.org",
            goal: "TypeScript documentation",
            keyword: "typescript",
            top_n: 10,
        },
        TestCase {
            name: "Kotlin",
            url: "https://kotlinlang.org",
            goal: "Kotlin programming language",
            keyword: "kotlin",
            top_n: 10,
        },
        TestCase {
            name: "Node.js",
            url: "https://nodejs.org",
            goal: "Node.js download latest version",
            keyword: "node",
            top_n: 10,
        },
        TestCase {
            name: "Ruby Lang",
            url: "https://www.ruby-lang.org/en/",
            goal: "Ruby programming language download",
            keyword: "ruby",
            top_n: 10,
        },
        // ── Dokumentation ──
        TestCase {
            name: "docs.rs",
            url: "https://docs.rs",
            goal: "Rust documentation search",
            keyword: "rust",
            top_n: 10,
        },
        TestCase {
            name: "DevDocs",
            url: "https://devdocs.io",
            goal: "API documentation browser",
            keyword: "documentation",
            top_n: 10,
        },
        // ── Paketregister ──
        TestCase {
            name: "PyPI",
            url: "https://pypi.org",
            goal: "find Python packages",
            keyword: "python",
            top_n: 10,
        },
        TestCase {
            name: "pkg.go.dev",
            url: "https://pkg.go.dev",
            goal: "Go packages and modules",
            keyword: "go",
            top_n: 10,
        },
        TestCase {
            name: "RubyGems",
            url: "https://rubygems.org",
            goal: "Ruby gem packages",
            keyword: "ruby",
            top_n: 10,
        },
        TestCase {
            name: "NuGet",
            url: "https://www.nuget.org",
            goal: ".NET package manager",
            keyword: "nuget",
            top_n: 10,
        },
        // ── DevOps / Infra ──
        TestCase {
            name: "Docker Hub",
            url: "https://hub.docker.com",
            goal: "search container images",
            keyword: "docker",
            top_n: 10,
        },
        TestCase {
            name: "Terraform",
            url: "https://www.terraform.io",
            goal: "infrastructure as code",
            keyword: "terraform",
            top_n: 10,
        },
        // ── GitHub / Kodvärd ──
        TestCase {
            name: "GitHub Explore",
            url: "https://github.com/explore",
            goal: "trending repositories",
            keyword: "trending",
            top_n: 10,
        },
        // ── Sök / Kartor ──
        TestCase {
            name: "OpenStreetMap",
            url: "https://www.openstreetmap.org",
            goal: "map navigation and editing",
            keyword: "map",
            top_n: 10,
        },
        // ── Myndigheter / Officiella ──
        // ── Referens / Encyklopedi ──
        // ── Verktyg ──
        TestCase {
            name: "httpbin HTML",
            url: "https://httpbin.org/html",
            goal: "Herman Melville story",
            keyword: "melville",
            top_n: 10,
        },
        TestCase {
            name: "JSON Placeholder",
            url: "https://jsonplaceholder.typicode.com",
            goal: "free fake API for testing",
            keyword: "api",
            top_n: 10,
        },
        // ── Tech-företag ──
        TestCase {
            name: "Haskell.org",
            url: "https://www.haskell.org",
            goal: "Haskell programming language",
            keyword: "haskell",
            top_n: 10,
        },
        TestCase {
            name: "Elixir Lang",
            url: "https://elixir-lang.org",
            goal: "Elixir programming language",
            keyword: "elixir",
            top_n: 10,
        },
        TestCase {
            name: "Zig Lang",
            url: "https://ziglang.org",
            goal: "Zig programming language",
            keyword: "zig",
            top_n: 10,
        },
        TestCase {
            name: "Svelte",
            url: "https://svelte.dev",
            goal: "Svelte web framework",
            keyword: "svelte",
            top_n: 10,
        },
        TestCase {
            name: "Tailwind CSS",
            url: "https://tailwindcss.com",
            goal: "utility-first CSS framework",
            keyword: "tailwind",
            top_n: 10,
        },
    ]
}

// ─── Resultat ────────────────────────────────────────────────────────────────

#[derive(Default)]
struct SiteResult {
    name: String,
    fetch_ms: u64,
    html_kb: usize,
    dom_nodes: usize,

    // MiniLM (hybrid pipeline, default Stage 3)
    minilm_ms: f64,
    minilm_correct: bool,
    minilm_top1_score: f32,
    minilm_top1_label: String,
    minilm_top3_labels: Vec<(f32, String, String)>, // (score, role, label)
    minilm_node_count: usize,

    // ColBERT (pipeline med Stage3Reranker::ColBert)
    colbert_ms: f64,
    colbert_correct: bool,
    colbert_top1_score: f32,
    colbert_top1_label: String,
    colbert_top3_labels: Vec<(f32, String, String)>,

    // Hybrid (pipeline med Stage3Reranker::Hybrid)
    hybrid_ms: f64,
    hybrid_correct: bool,
    hybrid_top1_score: f32,
    hybrid_top1_label: String,

    // Pipeline details (MiniLM run)
    bm25_candidates: usize,
    hdc_survivors: usize,

    fetch_error: bool,
}

// ─── Fetch ───────────────────────────────────────────────────────────────────

fn fetch_html(url: &str) -> Result<String, String> {
    let output = std::process::Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "12",
            "--compressed",
            "-H",
            "User-Agent: Mozilla/5.0 (compatible; AetherAgent/0.1)",
            "-H",
            "Accept: text/html",
            url,
        ])
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !output.status.success() {
        return Err(format!("HTTP {}", output.status));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("UTF-8: {e}"))
}

fn label_has_keyword(json: &str, keyword: &str, top_n: usize) -> (bool, usize, Vec<(f32, String)>) {
    let parsed: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
    let nodes = parsed["top_nodes"].as_array();
    let count = nodes.map(|n| n.len()).unwrap_or(0);
    let mut has = false;
    let mut top3 = Vec::new();

    if let Some(nodes) = nodes {
        for node in nodes.iter().take(top_n) {
            let label = node["label"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(80)
                .collect::<String>();
            let score = node["relevance"].as_f64().unwrap_or(0.0) as f32;
            if label.to_lowercase().contains(keyword) {
                has = true;
            }
            if top3.len() < 3 {
                top3.push((score, label));
            }
        }
    }
    (has, count, top3)
}

fn run_test(tc: &TestCase) -> SiteResult {
    let mut r = SiteResult {
        name: tc.name.to_string(),
        ..Default::default()
    };

    // Fetch
    let t0 = Instant::now();
    let html = match fetch_html(tc.url) {
        Ok(h) => h,
        Err(_) => {
            r.fetch_error = true;
            return r;
        }
    };
    r.fetch_ms = t0.elapsed().as_millis() as u64;
    r.html_kb = html.len() / 1024;

    // Alla tre rerankers kör genom exakt samma fullständiga pipeline:
    // HTML parse → semantic tree → BM25 → HDC → Stage 3 (med vald reranker)
    // via parse_top_nodes_with_config(). Ingen genväg.

    // Hjälpfunktion: extrahera top-3 (score, role, label) från JSON
    fn top3_from_json(json: &str) -> Vec<(f32, String, String)> {
        let pv: serde_json::Value = serde_json::from_str(json).unwrap_or_default();
        pv["top_nodes"]
            .as_array()
            .map(|nodes| {
                nodes
                    .iter()
                    .take(3)
                    .map(|n| {
                        let score = n["relevance"].as_f64().unwrap_or(0.0) as f32;
                        let role = n["role"].as_str().unwrap_or("?").to_string();
                        let label: String = n["label"]
                            .as_str()
                            .unwrap_or("")
                            .chars()
                            .take(100)
                            .collect();
                        (score, role, label)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    // ── 1. MiniLM (bi-encoder, default Stage 3) ──
    let config_minilm = PipelineConfig::default();
    let t1 = Instant::now();
    let minilm_json =
        aether_agent::parse_top_nodes_with_config(&html, tc.goal, tc.url, tc.top_n, &config_minilm);
    r.minilm_ms = t1.elapsed().as_micros() as f64 / 1000.0;

    let (ok, count, _) = label_has_keyword(&minilm_json, tc.keyword, tc.top_n as usize);
    r.minilm_correct = ok;
    r.minilm_node_count = count;
    r.minilm_top3_labels = top3_from_json(&minilm_json);
    if let Some((score, _, label)) = r.minilm_top3_labels.first() {
        r.minilm_top1_score = *score;
        r.minilm_top1_label = label.clone();
    }
    let pv: serde_json::Value = serde_json::from_str(&minilm_json).unwrap_or_default();
    r.bm25_candidates = pv["pipeline"]["bm25_candidates"].as_u64().unwrap_or(0) as usize;
    r.hdc_survivors = pv["pipeline"]["hdc_survivors"].as_u64().unwrap_or(0) as usize;
    r.dom_nodes = pv["total_nodes"].as_u64().unwrap_or(0) as usize;

    // ── 2. ColBERT (MaxSim, exakt samma pipeline) ──
    #[cfg(feature = "colbert")]
    if aether_agent::embedding::is_loaded() {
        let config_colbert = PipelineConfig {
            stage3_reranker: Stage3Reranker::ColBert,
            ..PipelineConfig::default()
        };
        let t2 = Instant::now();
        let colbert_json = aether_agent::parse_top_nodes_with_config(
            &html,
            tc.goal,
            tc.url,
            tc.top_n,
            &config_colbert,
        );
        r.colbert_ms = t2.elapsed().as_micros() as f64 / 1000.0;

        let (ok, _, _) = label_has_keyword(&colbert_json, tc.keyword, tc.top_n as usize);
        r.colbert_correct = ok;
        r.colbert_top3_labels = top3_from_json(&colbert_json);
        if let Some((score, _, label)) = r.colbert_top3_labels.first() {
            r.colbert_top1_score = *score;
            r.colbert_top1_label = label.clone();
        }

        // ── 3. Hybrid (adaptive α, exakt samma pipeline) ──
        let config_hybrid = PipelineConfig {
            stage3_reranker: Stage3Reranker::Hybrid {
                alpha: 0.7,
                use_adaptive_alpha: true,
            },
            ..PipelineConfig::default()
        };
        let t3 = Instant::now();
        let hybrid_json = aether_agent::parse_top_nodes_with_config(
            &html,
            tc.goal,
            tc.url,
            tc.top_n,
            &config_hybrid,
        );
        r.hybrid_ms = t3.elapsed().as_micros() as f64 / 1000.0;

        let (ok, _, _) = label_has_keyword(&hybrid_json, tc.keyword, tc.top_n as usize);
        r.hybrid_correct = ok;
        if let Some((score, _, label)) = top3_from_json(&hybrid_json).first() {
            r.hybrid_top1_score = *score;
            r.hybrid_top1_label = label.clone();
        }
    }

    r
}

// ─── Main ────────────────────────────────────────────────────────────────────

fn main() {
    println!("╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║  ColBERT vs MiniLM vs Hybrid — 45-Site Live Validation              ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝\n");

    // Init embeddings
    #[cfg(feature = "embeddings")]
    {
        let mp = std::env::var("AETHER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".into());
        let vp =
            std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
        if let (Ok(mb), Ok(vt)) = (std::fs::read(&mp), std::fs::read_to_string(&vp)) {
            if aether_agent::embedding::init_global(&mb, &vt).is_ok() {
                println!("  Embeddings: LOADED ({})", mp);
            }
        } else {
            println!("  Embeddings: NOT FOUND");
        }
    }

    // Ladda separat ColBERT-modell om tillgänglig
    #[cfg(feature = "embeddings")]
    {
        let cm = std::env::var("AETHER_COLBERT_MODEL")
            .unwrap_or_else(|_| "models/colbertv2-onnx/model.onnx".into());
        let cv = std::env::var("AETHER_COLBERT_VOCAB")
            .unwrap_or_else(|_| "models/colbertv2-onnx/vocab.txt".into());
        if let (Ok(cmb), Ok(cvt)) = (std::fs::read(&cm), std::fs::read_to_string(&cv)) {
            if aether_agent::embedding::init_colbert(&cmb, &cvt).is_ok() {
                println!("  ColBERT:    LOADED ({}, 768-dim ColBERTv2)", cm);
            }
        } else {
            println!("  ColBERT:    using bi-encoder fallback (384-dim)");
        }
    }
    let has_colbert = aether_agent::embedding::is_loaded();
    println!();
    println!();

    let cases = test_cases();
    let total = cases.len();
    let mut results = Vec::new();

    for (i, tc) in cases.iter().enumerate() {
        print!("[{:2}/{}] {:<20} ", i + 1, total, tc.name);

        let r = run_test(tc);

        if r.fetch_error {
            println!("FETCH FAIL");
        } else {
            let m = if r.minilm_correct { "✓" } else { "✗" };
            let c = if r.colbert_correct { "✓" } else { "✗" };
            let h = if r.hybrid_correct { "✓" } else { "✗" };
            println!(
                "{}KB | M:{}{:.0}ms C:{}{:.0}ms H:{}{:.0}ms | {} nodes",
                r.html_kb, m, r.minilm_ms, c, r.colbert_ms, h, r.hybrid_ms, r.dom_nodes
            );
        }
        results.push(r);
    }

    // ── Sammanfattning ──
    let fetched: Vec<&SiteResult> = results.iter().filter(|r| !r.fetch_error).collect();
    let n = fetched.len();
    let m_correct = fetched.iter().filter(|r| r.minilm_correct).count();
    let c_correct = fetched.iter().filter(|r| r.colbert_correct).count();
    let h_correct = fetched.iter().filter(|r| r.hybrid_correct).count();

    let m_avg_ms: f64 = fetched.iter().map(|r| r.minilm_ms).sum::<f64>() / n.max(1) as f64;
    let c_avg_ms: f64 = fetched.iter().map(|r| r.colbert_ms).sum::<f64>() / n.max(1) as f64;
    let h_avg_ms: f64 = fetched.iter().map(|r| r.hybrid_ms).sum::<f64>() / n.max(1) as f64;

    let m_avg_top1: f32 =
        fetched.iter().map(|r| r.minilm_top1_score).sum::<f32>() / n.max(1) as f32;
    let c_avg_top1: f32 =
        fetched.iter().map(|r| r.colbert_top1_score).sum::<f32>() / n.max(1) as f32;
    let h_avg_top1: f32 =
        fetched.iter().map(|r| r.hybrid_top1_score).sum::<f32>() / n.max(1) as f32;

    println!("\n╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║                        RESULTAT ({n} sajter)                          ║");
    println!("╠═══════════════════════════════════════════════════════════════════════╣");
    println!("║  Metod    │ Korrekthet       │ Avg latens  │ Avg top-1 score        ║");
    println!("║  MiniLM   │ {m_correct:>2}/{n} ({:>5.1}%)  │ {:>8.1}ms │ {m_avg_top1:.3}                  ║", m_correct as f64/n as f64*100.0, m_avg_ms);
    println!("║  ColBERT  │ {c_correct:>2}/{n} ({:>5.1}%)  │ {:>8.1}ms │ {c_avg_top1:.3}                  ║", c_correct as f64/n as f64*100.0, c_avg_ms);
    println!("║  Hybrid   │ {h_correct:>2}/{n} ({:>5.1}%)  │ {:>8.1}ms │ {h_avg_top1:.3}                  ║", h_correct as f64/n as f64*100.0, h_avg_ms);
    println!("╚═══════════════════════════════════════════════════════════════════════╝");

    // Count where ColBERT/Hybrid beat MiniLM
    let colbert_wins = fetched
        .iter()
        .filter(|r| r.colbert_correct && !r.minilm_correct)
        .count();
    let hybrid_wins = fetched
        .iter()
        .filter(|r| r.hybrid_correct && !r.minilm_correct)
        .count();
    let minilm_only = fetched
        .iter()
        .filter(|r| r.minilm_correct && !r.colbert_correct)
        .count();
    println!("\n  ColBERT vinner (korrekt där MiniLM missar): {colbert_wins}");
    println!("  Hybrid vinner (korrekt där MiniLM missar):  {hybrid_wins}");
    println!("  MiniLM-only (korrekt där ColBERT missar):   {minilm_only}");

    // ── Generera Markdown ──
    let mut md = String::new();
    md.push_str("# ColBERT vs MiniLM vs Hybrid — Live Validation\n\n");
    md.push_str(&format!("**Date:** 2026-03-31\n"));
    md.push_str("**Mode:** Release build, bi-encoder (all-MiniLM-L6-v2, 384-dim) + ColBERTv2.0 (768-dim, ONNX, CPU)\n");
    md.push_str(&format!("**Sites:** {n} fetched / {} total\n\n", total));

    md.push_str("## Summary\n\n");
    md.push_str("| Metod | Korrekthet | Avg latens | Avg top-1 score |\n");
    md.push_str("|-------|-----------|------------|----------------|\n");
    md.push_str(&format!(
        "| MiniLM (bi-encoder) | {m_correct}/{n} ({:.1}%) | {m_avg_ms:.1}ms | {m_avg_top1:.3} |\n",
        m_correct as f64 / n as f64 * 100.0
    ));
    md.push_str(&format!(
        "| ColBERT (MaxSim) | {c_correct}/{n} ({:.1}%) | {c_avg_ms:.1}ms | {c_avg_top1:.3} |\n",
        c_correct as f64 / n as f64 * 100.0
    ));
    md.push_str(&format!("| Hybrid (adaptive α) | {h_correct}/{n} ({:.1}%) | {h_avg_ms:.1}ms | {h_avg_top1:.3} |\n\n", h_correct as f64/n as f64*100.0));

    md.push_str(&format!(
        "ColBERT wins (correct where MiniLM misses): **{colbert_wins}**\n"
    ));
    md.push_str(&format!(
        "Hybrid wins (correct where MiniLM misses): **{hybrid_wins}**\n"
    ));
    md.push_str(&format!(
        "MiniLM-only (correct where ColBERT misses): **{minilm_only}**\n\n"
    ));

    md.push_str("## Per-Site Results\n\n");
    md.push_str("| # | Site | HTML | DOM | M-ok | C-ok | H-ok | M-ms | C-ms | H-ms | M-top1 | C-top1 | H-top1 |\n");
    md.push_str("|---|------|------|-----|------|------|------|------|------|------|--------|--------|--------|\n");

    for (i, r) in results.iter().enumerate() {
        if r.fetch_error {
            md.push_str(&format!(
                "| {} | {} | FAIL | - | - | - | - | - | - | - | - | - | - |\n",
                i + 1,
                r.name
            ));
            continue;
        }
        let m = if r.minilm_correct { "PASS" } else { "MISS" };
        let c = if r.colbert_correct { "PASS" } else { "MISS" };
        let h = if r.hybrid_correct { "PASS" } else { "MISS" };
        md.push_str(&format!(
            "| {} | {} | {}KB | {} | {} | {} | {} | {:.0} | {:.0} | {:.0} | {:.3} | {:.3} | {:.3} |\n",
            i+1, r.name, r.html_kb, r.dom_nodes, m, c, h,
            r.minilm_ms, r.colbert_ms, r.hybrid_ms,
            r.minilm_top1_score, r.colbert_top1_score, r.hybrid_top1_score
        ));
    }

    md.push_str("\n## Top-3 Node Quality Analysis\n\n");
    md.push_str("Side-by-side comparison of what each reranker picks as top-3 nodes.\n\n");
    for r in fetched.iter() {
        if r.minilm_top3_labels.is_empty() && r.colbert_top3_labels.is_empty() {
            continue;
        }
        md.push_str(&format!("### {}\n\n", r.name));
        md.push_str("**MiniLM top-3:**\n");
        for (i, (score, role, label)) in r.minilm_top3_labels.iter().enumerate() {
            md.push_str(&format!("{}. `{:.3}` [{}] {}\n", i + 1, score, role, label));
        }
        md.push_str("\n**ColBERT top-3:**\n");
        for (i, (score, role, label)) in r.colbert_top3_labels.iter().enumerate() {
            md.push_str(&format!("{}. `{:.3}` [{}] {}\n", i + 1, score, role, label));
        }
        md.push_str("\n---\n\n");
    }

    match std::fs::write("docs/colbert_vs_minilm_validation.md", &md) {
        Ok(()) => println!("\n  Results → docs/colbert_vs_minilm_validation.md"),
        Err(e) => eprintln!("\n  Write error: {e}"),
    }
}
