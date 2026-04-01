/// Pipeline breakdown — visar exakt BM25/HDC/Stage3 timing per sajt
///
/// Run:
///   AETHER_EMBEDDING_MODEL=models/all-MiniLM-L6-v2.onnx \
///   AETHER_EMBEDDING_VOCAB=models/vocab.txt \
///   AETHER_COLBERT_MODEL=models/all-MiniLM-L6-v2-int8.onnx \
///   AETHER_COLBERT_VOCAB=models/vocab.txt \
///   cargo run --release --bin aether-pipeline-breakdown --features colbert
use aether_agent::scoring::colbert_reranker::Stage3Reranker;
use aether_agent::scoring::pipeline::PipelineConfig;

fn main() {
    #[cfg(feature = "embeddings")]
    {
        let mp = std::env::var("AETHER_EMBEDDING_MODEL")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2.onnx".into());
        let vp =
            std::env::var("AETHER_EMBEDDING_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
        if let (Ok(mb), Ok(vt)) = (std::fs::read(&mp), std::fs::read_to_string(&vp)) {
            let _ = aether_agent::embedding::init_global(&mb, &vt);
        }
        let cm = std::env::var("AETHER_COLBERT_MODEL")
            .unwrap_or_else(|_| "models/all-MiniLM-L6-v2-int8.onnx".into());
        let cv =
            std::env::var("AETHER_COLBERT_VOCAB").unwrap_or_else(|_| "models/vocab.txt".into());
        if let (Ok(cmb), Ok(cvt)) = (std::fs::read(&cm), std::fs::read_to_string(&cv)) {
            let _ = aether_agent::embedding::init_colbert(&cmb, &cvt);
        }
    }

    let sites: Vec<(&str, &str, &str)> = vec![
        (
            "Hacker News",
            "https://news.ycombinator.com",
            "top stories today",
        ),
        (
            "MDN HTML",
            "https://developer.mozilla.org/en-US/docs/Web/HTML",
            "HTML elements reference",
        ),
        (
            "Tailwind CSS",
            "https://tailwindcss.com",
            "utility-first CSS framework",
        ),
        (
            "pkg.go.dev",
            "https://pkg.go.dev",
            "Go packages and modules",
        ),
        (
            "CNN Lite",
            "https://lite.cnn.com",
            "top news headlines today",
        ),
        (
            "Lobsters",
            "https://lobste.rs",
            "programming stories and discussions",
        ),
        (
            "GitHub Explore",
            "https://github.com/explore",
            "trending repositories",
        ),
        (
            "Docker Hub",
            "https://hub.docker.com",
            "search container images",
        ),
    ];

    println!(
        "{:<17} {:<8} {:>8} {:>8} {:>8} {:>8} {:>10} {:>10} {:>5} {:>5}",
        "Site", "Method", "BM25bld", "HDCbld", "BM25q", "HDCprn", "Stage3", "Total", "Cand", "Surv"
    );
    println!("{}", "─".repeat(105));

    for (name, url, goal) in &sites {
        let html = std::process::Command::new("curl")
            .args(["-sL", "--max-time", "10", "--compressed", url])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .unwrap_or_default();
        if html.len() < 100 {
            println!("{:<17} FETCH FAIL", name);
            continue;
        }

        let configs: Vec<(&str, PipelineConfig)> = vec![
            ("MiniLM", PipelineConfig::default()),
            #[cfg(feature = "colbert")]
            (
                "ColBERT",
                PipelineConfig {
                    stage3_reranker: Stage3Reranker::ColBert,
                    ..Default::default()
                },
            ),
        ];

        for (label, config) in &configs {
            let json = aether_agent::parse_top_nodes_with_config(&html, goal, url, 10, config);
            let pv: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
            let p = &pv["pipeline"];
            let bm25_bld = p["build_bm25_us"].as_u64().unwrap_or(0);
            let hdc_bld = p["build_hdc_us"].as_u64().unwrap_or(0);
            let bm25_q = p["query_bm25_us"].as_u64().unwrap_or(0);
            let hdc_prn = p["prune_hdc_us"].as_u64().unwrap_or(0);
            let stage3 = p["score_embed_us"].as_u64().unwrap_or(0);
            let total = p["total_pipeline_us"].as_u64().unwrap_or(0);
            let cand = p["bm25_candidates"].as_u64().unwrap_or(0);
            let surv = p["hdc_survivors"].as_u64().unwrap_or(0);
            let cache = if p["cache_hit"].as_bool().unwrap_or(false) {
                "©"
            } else {
                ""
            };
            println!(
                "{:<17} {:<8} {:>7}µ {:>7}µ {:>7}µ {:>7}µ {:>9}µ {:>9}µ {:>5} {:>5} {}",
                name, label, bm25_bld, hdc_bld, bm25_q, hdc_prn, stage3, total, cand, surv, cache
            );
        }
        println!();
    }
}
