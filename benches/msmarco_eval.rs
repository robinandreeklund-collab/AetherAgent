/// MS MARCO Evaluation Harness for CRFR Pipeline
///
/// Kör CRFR:s BM25 + HDC + Causal Memory pipeline på MS MARCO passage ranking.
/// Skapar en flat fake-DOM per query (varje passage = en SemanticNode) och mäter:
///   - nDCG@10, MRR@10, Recall@100, Recall@1000
///   - Latency: p50, p95, p99, mean
///   - Throughput: queries per second (QPS)
///
/// Ablation-varianter:
///   A) BM25 only
///   B) BM25 + HDC
///   C) CRFR cold (full pipeline, no feedback)
///   D) CRFR + simulated feedback loop (topic-grouped queries)
///
/// Dataformat (standard MS MARCO):
///   queries.tsv:    qid\tquery_text
///   collection.tsv: pid\tpassage_text
///   qrels.tsv:      qid\t0\tpid\t1       (TREC format)
///   top1000.tsv:    qid\tpid\tquery\tpassage  (pre-retrieved candidates, optional)
///
/// Användning:
///   cargo run --bin msmarco-eval --release -- --data-dir ./msmarco-data [--top1000] [--max-queries 100] [--feedback]
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::time::Instant;

use aether_agent::resonance::ResonanceField;
use aether_agent::scoring::hdc::Hypervector;
use aether_agent::scoring::tfidf::TfIdfIndex;
use aether_agent::types::{NodeState, SemanticNode, TrustLevel};

// ─── CLI ────────────────────────────────────────────────────────────────────

struct Args {
    data_dir: PathBuf,
    max_queries: usize,
    use_top1000: bool,
    run_feedback: bool,
    json_output: bool,
    verbose: bool,
}

fn parse_args() -> Args {
    let args: Vec<String> = std::env::args().collect();
    let mut data_dir = PathBuf::from("./msmarco-data");
    let mut max_queries = 0usize; // 0 = alla
    let mut use_top1000 = false;
    let mut run_feedback = false;
    let mut json_output = false;
    let mut verbose = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--data-dir" => {
                i += 1;
                if i < args.len() {
                    data_dir = PathBuf::from(&args[i]);
                }
            }
            "--max-queries" => {
                i += 1;
                if i < args.len() {
                    max_queries = args[i].parse().unwrap_or(0);
                }
            }
            "--top1000" => use_top1000 = true,
            "--feedback" => run_feedback = true,
            "--json" => json_output = true,
            "--verbose" | "-v" => verbose = true,
            "--help" | "-h" => {
                eprintln!(
                    "Usage: msmarco-eval [OPTIONS]\n\n\
                     Options:\n  \
                       --data-dir <PATH>    MS MARCO data directory (default: ./msmarco-data)\n  \
                       --max-queries <N>    Limit number of queries (0 = all)\n  \
                       --top1000            Use pre-retrieved top1000.tsv candidates\n  \
                       --feedback           Run simulated feedback loop variant\n  \
                       --json               Output results as JSON\n  \
                       -v, --verbose        Verbose per-query output\n  \
                       -h, --help           Show this help"
                );
                std::process::exit(0);
            }
            _ => {
                eprintln!("Okänt argument: {}", args[i]);
            }
        }
        i += 1;
    }

    Args {
        data_dir,
        max_queries,
        use_top1000,
        run_feedback,
        json_output,
        verbose,
    }
}

// ─── Data Loading ───────────────────────────────────────────────────────────

/// Ladda queries.tsv → HashMap<qid, query_text>
fn load_queries(path: &std::path::Path) -> HashMap<u32, String> {
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Kan inte öppna {}: {}", path.display(), e);
        std::process::exit(1);
    });
    let reader = BufReader::new(file);
    let mut queries = HashMap::new();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            if let Ok(qid) = parts[0].parse::<u32>() {
                queries.insert(qid, parts[1].to_string());
            }
        }
    }
    queries
}

/// Ladda collection.tsv → HashMap<pid, passage_text>
fn load_collection(path: &std::path::Path) -> HashMap<u32, String> {
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Kan inte öppna {}: {}", path.display(), e);
        std::process::exit(1);
    });
    let reader = BufReader::with_capacity(1024 * 1024, file);
    let mut collection = HashMap::new();
    let mut count = 0u64;
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let parts: Vec<&str> = line.splitn(2, '\t').collect();
        if parts.len() == 2 {
            if let Ok(pid) = parts[0].parse::<u32>() {
                collection.insert(pid, parts[1].to_string());
            }
        }
        count += 1;
        if count.is_multiple_of(1_000_000) {
            eprintln!("  Laddat {} passages...", count);
        }
    }
    collection
}

/// Ladda qrels.tsv → HashMap<qid, Vec<pid>>
/// Format: qid \t 0 \t pid \t relevance (TREC)
/// Eller: qid \t pid \t query \t passage (MS MARCO qrels.dev.tsv)
fn load_qrels(path: &std::path::Path) -> HashMap<u32, Vec<u32>> {
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Kan inte öppna {}: {}", path.display(), e);
        std::process::exit(1);
    });
    let reader = BufReader::new(file);
    let mut qrels: HashMap<u32, Vec<u32>> = HashMap::new();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            if let (Ok(qid), Ok(pid)) = (parts[0].parse::<u32>(), parts[2].parse::<u32>()) {
                qrels.entry(qid).or_default().push(pid);
            }
        }
    }
    qrels
}

/// Ladda top1000.tsv → HashMap<qid, Vec<(pid, passage_text)>>
/// Format: qid \t pid \t query \t passage
fn load_top1000(path: &std::path::Path) -> HashMap<u32, Vec<(u32, String)>> {
    let file = File::open(path).unwrap_or_else(|e| {
        eprintln!("Kan inte öppna {}: {}", path.display(), e);
        std::process::exit(1);
    });
    let reader = BufReader::with_capacity(512 * 1024, file);
    let mut top1000: HashMap<u32, Vec<(u32, String)>> = HashMap::new();
    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => continue,
        };
        let parts: Vec<&str> = line.splitn(4, '\t').collect();
        if parts.len() == 4 {
            if let (Ok(qid), Ok(pid)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
                top1000
                    .entry(qid)
                    .or_default()
                    .push((pid, parts[3].to_string()));
            }
        }
    }
    top1000
}

// ─── Passage → SemanticNode adapter ─────────────────────────────────────────

/// Konvertera en lista av (pid, passage_text) till SemanticNodes (flat DOM).
/// Varje passage blir en SemanticNode med role="paragraph" under en implicit root.
fn passages_to_nodes(passages: &[(u32, &str)]) -> Vec<SemanticNode> {
    passages
        .iter()
        .map(|&(pid, text)| SemanticNode {
            id: pid,
            role: "paragraph".to_string(),
            label: text.to_string(),
            value: None,
            state: NodeState::default_state(),
            action: None,
            relevance: 0.0,
            trust: TrustLevel::Untrusted,
            children: vec![],
            html_id: None,
            name: None,
            bbox: None,
        })
        .collect()
}

// ─── IR Metrics ─────────────────────────────────────────────────────────────

/// Reciprocal Rank: 1/rank av första relevanta dokumentet
fn reciprocal_rank(ranked: &[u32], relevant: &[u32]) -> f64 {
    for (i, pid) in ranked.iter().enumerate() {
        if relevant.contains(pid) {
            return 1.0 / (i as f64 + 1.0);
        }
    }
    0.0
}

/// nDCG@k: Normalized Discounted Cumulative Gain
fn ndcg_at_k(ranked: &[u32], relevant: &[u32], k: usize) -> f64 {
    let k = k.min(ranked.len());
    if k == 0 || relevant.is_empty() {
        return 0.0;
    }

    // DCG
    let mut dcg = 0.0f64;
    for (i, pid) in ranked.iter().enumerate().take(k) {
        let rel = if relevant.contains(pid) { 1.0 } else { 0.0 };
        dcg += rel / (i as f64 + 2.0).log2();
    }

    // Ideal DCG (alla relevanta i toppen)
    let ideal_k = relevant.len().min(k);
    let mut idcg = 0.0f64;
    for i in 0..ideal_k {
        idcg += 1.0 / (i as f64 + 2.0).log2();
    }

    if idcg == 0.0 {
        0.0
    } else {
        dcg / idcg
    }
}

/// Recall@k: andel relevanta dokument som finns i top-k
fn recall_at_k(ranked: &[u32], relevant: &[u32], k: usize) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    let k = k.min(ranked.len());
    let found = ranked[..k]
        .iter()
        .filter(|pid| relevant.contains(pid))
        .count();
    found as f64 / relevant.len() as f64
}

// ─── Ablation Variants ──────────────────────────────────────────────────────

/// Variant A: BM25 only — ren keyword retrieval
fn run_bm25_only(passages: &[(u32, &str)], query: &str, top_k: usize) -> Vec<u32> {
    let nodes: Vec<(u32, &str)> = passages.to_vec();
    let index = TfIdfIndex::build(&nodes);
    let results = index.query(query, top_k);
    results.into_iter().map(|(pid, _score)| pid).collect()
}

/// Variant B: BM25 + HDC reranking
fn run_bm25_hdc(passages: &[(u32, &str)], query: &str, top_k: usize) -> Vec<u32> {
    let nodes: Vec<(u32, &str)> = passages.to_vec();
    let index = TfIdfIndex::build(&nodes);
    // BM25 top-200 candidates
    let bm25_results = index.query(query, 200);

    if bm25_results.is_empty() {
        return vec![];
    }

    // HDC reranking
    let goal_hv = Hypervector::from_text_ngrams(query);
    let mut scored: Vec<(u32, f32)> = bm25_results
        .iter()
        .map(|&(pid, bm25_score)| {
            // Hitta passage-text för HDC
            let text = passages
                .iter()
                .find(|&&(id, _)| id == pid)
                .map(|&(_, t)| t)
                .unwrap_or("");
            let text_hv = Hypervector::from_text_ngrams(text);
            let hdc_sim = goal_hv.similarity(&text_hv);
            // CombMNZ-liknande fusion: 75% BM25 + 20% HDC + 5% baseline
            let combined = 0.75 * bm25_score + 0.20 * hdc_sim.max(0.0) + 0.05;
            (pid, combined)
        })
        .collect();

    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.into_iter().take(top_k).map(|(pid, _)| pid).collect()
}

/// Variant C: CRFR cold with BM25 pre-filter (top-200 → ResonanceField)
/// Matchar produktionspipelinen: BM25 filtrerar ner till hanterbar storlek
fn run_crfr_cold(passages: &[(u32, &str)], query: &str, top_k: usize) -> Vec<u32> {
    // BM25 pre-filter: ta top-200 candidates (som i ScoringPipeline steg 1)
    let index = TfIdfIndex::build(passages);
    let bm25_top = index.query(query, 200);
    let filtered: Vec<(u32, &str)> = bm25_top
        .iter()
        .filter_map(|&(pid, _)| {
            passages
                .iter()
                .find(|&&(id, _)| id == pid)
                .map(|&(id, text)| (id, text))
        })
        .collect();
    if filtered.is_empty() {
        return vec![];
    }
    let nodes = passages_to_nodes(&filtered);
    let mut field = ResonanceField::from_semantic_tree(&nodes, "msmarco://eval");
    let results = field.propagate_top_k(query, top_k);
    results.into_iter().map(|r| r.node_id).collect()
}

/// Variant D: CRFR with feedback — återanvänd field med feedback mellan queries
fn run_crfr_with_feedback(
    field: &mut ResonanceField,
    query: &str,
    top_k: usize,
    relevant_pids: &[u32],
) -> Vec<u32> {
    let results = field.propagate_top_k(query, top_k);
    let ranked: Vec<u32> = results.iter().map(|r| r.node_id).collect();

    // Auto-feedback: ge feedback med de relevanta dokument som faktiskt rankades högt
    let successful: Vec<u32> = ranked
        .iter()
        .filter(|pid| relevant_pids.contains(pid))
        .copied()
        .collect();
    if !successful.is_empty() {
        field.feedback(query, &successful);
    }

    ranked
}

// ─── Latency Tracking ───────────────────────────────────────────────────────

struct LatencyStats {
    times_us: Vec<u64>,
}

impl LatencyStats {
    fn new() -> Self {
        Self { times_us: vec![] }
    }

    fn record(&mut self, us: u64) {
        self.times_us.push(us);
    }

    fn percentile(&self, p: f64) -> u64 {
        if self.times_us.is_empty() {
            return 0;
        }
        let mut sorted = self.times_us.clone();
        sorted.sort_unstable();
        let idx = ((p / 100.0) * (sorted.len() as f64 - 1.0)).round() as usize;
        sorted[idx.min(sorted.len() - 1)]
    }

    fn mean(&self) -> f64 {
        if self.times_us.is_empty() {
            return 0.0;
        }
        let sum: u64 = self.times_us.iter().sum();
        sum as f64 / self.times_us.len() as f64
    }

    fn count(&self) -> usize {
        self.times_us.len()
    }

    fn total_ms(&self) -> f64 {
        let sum: u64 = self.times_us.iter().sum();
        sum as f64 / 1000.0
    }
}

// ─── Result Aggregation ─────────────────────────────────────────────────────

#[derive(Default)]
struct MetricAccumulator {
    mrr_sum: f64,
    ndcg10_sum: f64,
    recall100_sum: f64,
    recall1000_sum: f64,
    count: usize,
}

impl MetricAccumulator {
    fn add(&mut self, ranked: &[u32], relevant: &[u32]) {
        self.mrr_sum += reciprocal_rank(ranked, relevant);
        self.ndcg10_sum += ndcg_at_k(ranked, relevant, 10);
        self.recall100_sum += recall_at_k(ranked, relevant, 100);
        self.recall1000_sum += recall_at_k(ranked, relevant, 1000);
        self.count += 1;
    }

    fn mrr(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.mrr_sum / self.count as f64
        }
    }
    fn ndcg10(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.ndcg10_sum / self.count as f64
        }
    }
    fn recall100(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.recall100_sum / self.count as f64
        }
    }
    fn recall1000(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.recall1000_sum / self.count as f64
        }
    }
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    let args = parse_args();

    eprintln!("=== CRFR MS MARCO Evaluation ===");
    eprintln!("Data dir: {}", args.data_dir.display());

    // Ladda data
    eprintln!("\n[1/4] Laddar queries...");
    let queries = load_queries(&args.data_dir.join("queries.dev.small.tsv"));
    eprintln!("  {} queries", queries.len());

    eprintln!("[2/4] Laddar qrels...");
    let qrels = load_qrels(&args.data_dir.join("qrels.dev.small.tsv"));
    eprintln!("  {} queries med relevance judgments", qrels.len());

    // Bestäm query-lista (begränsad av --max-queries och filtrerad till de med qrels)
    let mut query_ids: Vec<u32> = qrels.keys().copied().collect();
    query_ids.sort_unstable();
    if args.max_queries > 0 && args.max_queries < query_ids.len() {
        query_ids.truncate(args.max_queries);
    }
    eprintln!("  Kör {} queries", query_ids.len());

    // Ladda passager — antingen top1000 (snabbare) eller full collection
    let use_top1000 = args.use_top1000 && args.data_dir.join("top1000.dev.tsv").exists();

    // top1000 mode: per-query passage-set
    let top1000: Option<HashMap<u32, Vec<(u32, String)>>> = if use_top1000 {
        eprintln!("[3/4] Laddar top1000.dev.tsv (pre-retrieved candidates)...");
        let t = load_top1000(&args.data_dir.join("top1000.dev.tsv"));
        eprintln!("  {} queries med candidates", t.len());
        Some(t)
    } else {
        None
    };

    // Om vi inte har top1000, ladda hela collection
    let collection: Option<HashMap<u32, String>> = if top1000.is_none() {
        eprintln!("[3/4] Laddar collection.tsv (full, kan ta tid)...");
        let c = load_collection(&args.data_dir.join("collection.tsv"));
        eprintln!("  {} passages", c.len());
        Some(c)
    } else {
        None
    };

    eprintln!("[4/4] Kör evaluation...\n");

    // ── Kör alla ablation-varianter ──

    let variants: Vec<(&str, bool)> = if args.run_feedback {
        vec![
            ("A: BM25 only", false),
            ("B: BM25 + HDC", false),
            ("C: CRFR cold", false),
            ("D: CRFR + feedback", true),
        ]
    } else {
        vec![
            ("A: BM25 only", false),
            ("B: BM25 + HDC", false),
            ("C: CRFR cold", false),
        ]
    };

    let mut all_results: Vec<(String, MetricAccumulator, LatencyStats)> = Vec::new();

    for &(variant_name, _needs_feedback) in &variants {
        eprintln!("── {} ──", variant_name);
        let mut metrics = MetricAccumulator::default();
        let mut latency = LatencyStats::new();

        // För feedback-varianten: bygg ett persistent ResonanceField
        // med alla passager (eller använd per-query field)
        let mut feedback_field: Option<ResonanceField> = None;

        for (qi, &qid) in query_ids.iter().enumerate() {
            let query_text = match queries.get(&qid) {
                Some(q) => q.as_str(),
                None => continue,
            };
            let relevant = match qrels.get(&qid) {
                Some(r) => r.as_slice(),
                None => continue,
            };

            // Hämta passage-kandidater för denna query
            let passages: Vec<(u32, &str)> = if let Some(ref t1k) = top1000 {
                match t1k.get(&qid) {
                    Some(ps) => ps.iter().map(|(pid, text)| (*pid, text.as_str())).collect(),
                    None => continue,
                }
            } else if let Some(ref coll) = collection {
                // Full collection mode — sample top passages via BM25 pre-filter
                // (full 8.8M passage eval per query tar för lång tid utan indexering)
                let all_passages: Vec<(u32, &str)> = coll
                    .iter()
                    .map(|(pid, text)| (*pid, text.as_str()))
                    .collect();

                // Pre-filter: bygg BM25-index och ta top-1000
                let index = TfIdfIndex::build(&all_passages);
                let bm25_top = index.query(query_text, 1000);
                bm25_top
                    .iter()
                    .filter_map(|&(pid, _)| coll.get(&pid).map(|text| (pid, text.as_str())))
                    .collect()
            } else {
                continue;
            };

            if passages.is_empty() {
                continue;
            }

            // Kör variant
            let start = Instant::now();
            let ranked = match variant_name {
                "A: BM25 only" => run_bm25_only(&passages, query_text, 1000),
                "B: BM25 + HDC" => run_bm25_hdc(&passages, query_text, 1000),
                "C: CRFR cold" => run_crfr_cold(&passages, query_text, 1000),
                "D: CRFR + feedback" => {
                    // BM25 pre-filter → CRFR field med feedback
                    // Nytt field per query, men feedback ackumuleras via domain registry
                    let index = TfIdfIndex::build(&passages);
                    let bm25_top = index.query(query_text, 200);
                    let filtered: Vec<(u32, &str)> = bm25_top
                        .iter()
                        .filter_map(|&(pid, _)| {
                            passages
                                .iter()
                                .find(|&&(id, _)| id == pid)
                                .map(|&(id, text)| (id, text))
                        })
                        .collect();
                    if filtered.is_empty() {
                        continue;
                    }
                    let nodes = passages_to_nodes(&filtered);
                    let field = feedback_field
                        .insert(ResonanceField::from_semantic_tree(&nodes, "msmarco://eval"));
                    run_crfr_with_feedback(field, query_text, 1000, relevant)
                }
                _ => vec![],
            };
            let elapsed_us = start.elapsed().as_micros() as u64;

            latency.record(elapsed_us);
            metrics.add(&ranked, relevant);

            if args.verbose && qi < 10 {
                let rr = reciprocal_rank(&ranked, relevant);
                let ndcg = ndcg_at_k(&ranked, relevant, 10);
                eprintln!(
                    "  q{}: qid={} RR={:.3} nDCG@10={:.3} ({} candidates, {}µs)",
                    qi,
                    qid,
                    rr,
                    ndcg,
                    passages.len(),
                    elapsed_us
                );
            }

            if (qi + 1) % 500 == 0 {
                eprintln!(
                    "  ...{}/{} queries (MRR={:.4})",
                    qi + 1,
                    query_ids.len(),
                    metrics.mrr()
                );
            }
        }

        eprintln!(
            "  MRR@10={:.4}  nDCG@10={:.4}  R@100={:.4}  R@1000={:.4}",
            metrics.mrr(),
            metrics.ndcg10(),
            metrics.recall100(),
            metrics.recall1000()
        );
        let qps = if latency.total_ms() > 0.0 {
            latency.count() as f64 / (latency.total_ms() / 1000.0)
        } else {
            0.0
        };
        eprintln!(
            "  Latency: mean={:.0}µs  p50={}µs  p95={}µs  p99={}µs  QPS={:.0}",
            latency.mean(),
            latency.percentile(50.0),
            latency.percentile(95.0),
            latency.percentile(99.0),
            qps
        );
        eprintln!();

        all_results.push((variant_name.to_string(), metrics, latency));
    }

    // ── Output ──

    if args.json_output {
        print_json_results(&all_results);
    } else {
        print_table_results(&all_results);
    }
}

fn print_table_results(results: &[(String, MetricAccumulator, LatencyStats)]) {
    println!("\n╔══════════════════════════╦══════════╦══════════╦══════════╦══════════╦══════════╦══════════╦══════════╗");
    println!("║ Variant                  ║ MRR@10   ║ nDCG@10  ║ R@100    ║ R@1000   ║ p50(µs)  ║ p95(µs)  ║ QPS      ║");
    println!("╠══════════════════════════╬══════════╬══════════╬══════════╬══════════╬══════════╬══════════╬══════════╣");
    for (name, metrics, latency) in results {
        let qps = if latency.total_ms() > 0.0 {
            latency.count() as f64 / (latency.total_ms() / 1000.0)
        } else {
            0.0
        };
        println!(
            "║ {:<24} ║ {:<8.4} ║ {:<8.4} ║ {:<8.4} ║ {:<8.4} ║ {:<8} ║ {:<8} ║ {:<8.0} ║",
            name,
            metrics.mrr(),
            metrics.ndcg10(),
            metrics.recall100(),
            metrics.recall1000(),
            latency.percentile(50.0),
            latency.percentile(95.0),
            qps
        );
    }
    println!("╚══════════════════════════╩══════════╩══════════╩══════════╩══════════╩══════════╩══════════╩══════════╝");
    println!();
    println!("Referens baselines (MS MARCO dev):");
    println!("  BM25 (Anserini):    MRR@10 ≈ 0.187");
    println!("  docT5query + BM25:  MRR@10 ≈ 0.277");
    println!("  ANCE (dense):       MRR@10 ≈ 0.330");
    println!("  ColBERT v2:         MRR@10 ≈ 0.397");
}

fn print_json_results(results: &[(String, MetricAccumulator, LatencyStats)]) {
    println!("{{");
    println!("  \"benchmark\": \"CRFR MS MARCO Evaluation\",");
    println!("  \"variants\": [");
    for (i, (name, metrics, latency)) in results.iter().enumerate() {
        let qps = if latency.total_ms() > 0.0 {
            latency.count() as f64 / (latency.total_ms() / 1000.0)
        } else {
            0.0
        };
        println!("    {{");
        println!("      \"name\": \"{}\",", name);
        println!("      \"metrics\": {{");
        println!("        \"mrr_at_10\": {:.6},", metrics.mrr());
        println!("        \"ndcg_at_10\": {:.6},", metrics.ndcg10());
        println!("        \"recall_at_100\": {:.6},", metrics.recall100());
        println!("        \"recall_at_1000\": {:.6}", metrics.recall1000());
        println!("      }},");
        println!("      \"latency\": {{");
        println!("        \"mean_us\": {:.1},", latency.mean());
        println!("        \"p50_us\": {},", latency.percentile(50.0));
        println!("        \"p95_us\": {},", latency.percentile(95.0));
        println!("        \"p99_us\": {},", latency.percentile(99.0));
        println!("        \"qps\": {:.1}", qps);
        println!("      }},");
        println!("      \"queries_evaluated\": {}", metrics.count);
        if i < results.len() - 1 {
            println!("    }},");
        } else {
            println!("    }}");
        }
    }
    println!("  ],");
    println!("  \"reference_baselines\": {{");
    println!("    \"bm25_anserini_mrr10\": 0.187,");
    println!("    \"doct5query_bm25_mrr10\": 0.277,");
    println!("    \"ance_dense_mrr10\": 0.330,");
    println!("    \"colbert_v2_mrr10\": 0.397");
    println!("  }}");
    println!("}}");
}
