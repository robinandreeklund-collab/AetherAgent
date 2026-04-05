/// AetherAgent Live Dashboard — Full Observability Layer
///
/// Real-time insight into CRFR pipeline, DOM engine, SPA runtime,
/// causal memory, fetch/cache status, and WPT quality.
use serde::{Deserialize, Serialize};

use crate::resonance;

/// Serde-modul: serialiserar u64 som JSON-sträng (JS Number förlorar precision >2^53)
mod u64_as_string {
    use serde::{self, Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(val: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&val.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ─── 1. CRFR Query Explorer ────────────────────────────────────────────────

/// Full CRFR query trace: entire pipeline from goal to ranked results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrfrQueryExplorer {
    pub goal: String,
    pub url: String,
    /// BM25 top-200 candidate count
    pub bm25_candidates: usize,
    /// Nodes passing cascade filter (BM25 + causal memory)
    pub cascade_candidates: usize,
    /// Wave propagation iterations (1..6, typically converges at 2-3)
    pub propagation_iterations: u32,
    /// Per-iteration convergence delta
    pub iteration_deltas: Vec<f32>,
    /// Amplitude gap cut position (None = hard top_k limit)
    pub gap_cut_position: Option<usize>,
    /// Template structure match detected
    pub template_match: bool,
    /// Per-node score breakdown (top nodes)
    pub node_scores: Vec<NodeScoreView>,
    /// Top result nodes with ranking
    pub top_nodes: Vec<CrfrNodeRank>,
    /// Timing
    pub field_build_ms: u64,
    pub propagation_ms: u64,
    pub cache_hit: bool,
    pub total_field_nodes: usize,
    pub total_queries: u32,
}

/// Per-node scoring breakdown for CRFR explorer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeScoreView {
    pub node_id: u32,
    pub role: String,
    pub label: String,
    pub bm25_score: f32,
    pub hdc_score: f32,
    pub role_priority: f32,
    pub concept_boost: f32,
    pub causal_boost: f32,
    pub answer_shape: f32,
    pub answer_type_boost: f32,
    pub zone_penalty: f32,
    pub meta_penalty: f32,
    pub combmnz: f32,
    pub template_boost: bool,
    pub final_amplitude: f32,
}

/// Ranked node in CRFR results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrfrNodeRank {
    pub node_id: u32,
    pub role: String,
    pub label: String,
    pub amplitude: f32,
    pub confidence: f32,
    pub resonance_type: String,
    pub causal_boost: f32,
}

// ─── 2. DOM Visualizer ─────────────────────────────────────────────────────

/// DOM node view with full scoring data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomNodeView {
    pub id: u32,
    pub role: String,
    pub label: String,
    pub depth: u32,
    pub amplitude: f32,
    pub bm25_score: f32,
    pub hdc_score: f32,
    pub causal_memory_strength: f32,
    pub children_count: usize,
    pub children: Vec<DomNodeView>,
}

/// Full DOM tree view
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomTreeView {
    pub url: String,
    pub title: String,
    pub total_nodes: usize,
    pub max_depth: u32,
    pub roots: Vec<DomNodeView>,
}

// ─── 3. SPA Runtime Monitor ────────────────────────────────────────────────

/// SPA runtime snapshot (QuickJS sandbox state)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaRuntimeSnapshot {
    /// Engine info
    pub engine: String,
    /// Limits
    pub max_timers: usize,
    pub max_delay_ms: u64,
    pub max_ticks: usize,
    pub max_runtime_us: u64,
    pub raf_interval_ms: u64,
    /// DOM bridge capabilities
    pub dom_element_wrappers: usize,
    pub dom_methods: usize,
    /// Feature flags
    pub mutation_observer: bool,
    pub event_dispatch: bool,
    pub computed_style: bool,
    pub form_state: bool,
    /// Cumulative runtime stats (real data from event loop)
    pub total_eval_runs: u64,
    pub total_ticks: u64,
    pub total_timers_fired: u64,
    pub total_rafs_fired: u64,
    pub total_mutations_delivered: u64,
    pub peak_ticks_single_run: u64,
}

/// Build SPA runtime info with live cumulative stats
pub fn build_spa_runtime() -> SpaRuntimeSnapshot {
    #[cfg(feature = "js-eval")]
    let cum = crate::event_loop::cumulative_stats();

    SpaRuntimeSnapshot {
        engine: "QuickJS (rquickjs 0.11)".into(),
        max_timers: 500,
        max_delay_ms: 5000,
        max_ticks: 5000,
        max_runtime_us: 500_000,
        raf_interval_ms: 16,
        dom_element_wrappers: 90,
        dom_methods: 55,
        mutation_observer: true,
        event_dispatch: true,
        computed_style: true,
        form_state: true,
        #[cfg(feature = "js-eval")]
        total_eval_runs: cum.total_runs,
        #[cfg(feature = "js-eval")]
        total_ticks: cum.total_ticks,
        #[cfg(feature = "js-eval")]
        total_timers_fired: cum.total_timers_fired,
        #[cfg(feature = "js-eval")]
        total_rafs_fired: cum.total_rafs_fired,
        #[cfg(feature = "js-eval")]
        total_mutations_delivered: cum.total_mutations_delivered,
        #[cfg(feature = "js-eval")]
        peak_ticks_single_run: cum.peak_ticks,
        #[cfg(not(feature = "js-eval"))]
        total_eval_runs: 0,
        #[cfg(not(feature = "js-eval"))]
        total_ticks: 0,
        #[cfg(not(feature = "js-eval"))]
        total_timers_fired: 0,
        #[cfg(not(feature = "js-eval"))]
        total_rafs_fired: 0,
        #[cfg(not(feature = "js-eval"))]
        total_mutations_delivered: 0,
        #[cfg(not(feature = "js-eval"))]
        peak_ticks_single_run: 0,
    }
}

// ─── 4. Propagation Weights & Causal Memory ────────────────────────────────

/// Beta distribution weight per role+direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationWeight {
    pub key: String,
    pub alpha: f32,
    pub beta: f32,
    pub mean: f32,
    pub variance: f32,
    pub confidence: f32,
}

/// Causal memory overview for a field
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalMemoryView {
    pub url: String,
    #[serde(with = "u64_as_string")]
    pub url_hash: u64,
    pub propagation_weights: Vec<PropagationWeight>,
    pub concept_memory_count: usize,
    pub total_feedback_events: u32,
    #[serde(with = "u64_as_string")]
    pub domain_hash: u64,
    pub field_age_ms: u64,
    #[serde(with = "u64_as_string")]
    pub structure_hash: u64,
    pub max_depth: u32,
    pub edge_count: usize,
}

/// Build PropagationWeight from Beta parameters
pub fn build_propagation_weight(key: &str, alpha: f32, beta: f32) -> PropagationWeight {
    let sum = alpha + beta;
    let mean = if sum > 0.0 { alpha / sum } else { 0.5 };
    let variance = if sum > 0.0 {
        (alpha * beta) / (sum * sum * (sum + 1.0))
    } else {
        0.0
    };
    let obs = (sum - 2.0).max(0.0);
    let confidence = (1.0 + obs).ln() / (1.0 + 100.0_f32).ln();
    PropagationWeight {
        key: key.to_string(),
        alpha,
        beta,
        mean,
        variance,
        confidence: confidence.clamp(0.0, 1.0),
    }
}

/// Build CausalMemoryView from a FieldSummary
pub fn build_causal_memory_view(field: &resonance::FieldSummary, now_ms: u64) -> CausalMemoryView {
    let mut weights: Vec<PropagationWeight> = field
        .propagation_stats
        .iter()
        .map(|(k, &(a, b))| build_propagation_weight(k, a, b))
        .collect();
    weights.sort_by(|a, b| b.mean.total_cmp(&a.mean));

    CausalMemoryView {
        url: field.url.clone(),
        url_hash: field.url_hash,
        propagation_weights: weights,
        concept_memory_count: field.concept_memory_count,
        total_feedback_events: field.total_queries,
        domain_hash: field.domain_hash,
        field_age_ms: now_ms.saturating_sub(field.created_at_ms),
        structure_hash: field.structure_hash,
        max_depth: field.max_depth,
        edge_count: field.edge_count,
    }
}

// ─── 5. Site & Cache Overview ──────────────────────────────────────────────

/// Default cache TTL (matches resonance.rs DEFAULT_CACHE_TTL_MS)
const CACHE_TTL_MS: u64 = 180_000;

/// A cached site entry with full metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteCacheEntry {
    #[serde(with = "u64_as_string")]
    pub url_hash: u64,
    pub url: String,
    pub node_count: usize,
    pub total_queries: u32,
    #[serde(with = "u64_as_string")]
    pub domain_hash: u64,
    pub created_at_ms: u64,
    pub age_ms: u64,
    pub ttl_remaining_ms: u64,
    pub propagation_weight_count: usize,
    pub concept_memory_count: usize,
    #[serde(with = "u64_as_string")]
    pub structure_hash: u64,
    pub max_depth: u32,
    pub edge_count: usize,
}

/// Cache overview
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheOverview {
    pub entries: Vec<SiteCacheEntry>,
    pub total_entries: usize,
    pub capacity: usize,
    pub ttl_ms: u64,
}

/// Build cache overview from field summaries
pub fn build_cache_overview(
    fields: &[resonance::FieldSummary],
    cache_len: usize,
    cache_cap: usize,
    now_ms: u64,
) -> CacheOverview {
    let entries: Vec<SiteCacheEntry> = fields
        .iter()
        .map(|f| {
            let age = now_ms.saturating_sub(f.insert_ms);
            SiteCacheEntry {
                url_hash: f.url_hash,
                url: f.url.clone(),
                node_count: f.node_count,
                total_queries: f.total_queries,
                domain_hash: f.domain_hash,
                created_at_ms: f.created_at_ms,
                age_ms: age,
                ttl_remaining_ms: CACHE_TTL_MS.saturating_sub(age),
                propagation_weight_count: f.propagation_weight_count,
                concept_memory_count: f.concept_memory_count,
                structure_hash: f.structure_hash,
                max_depth: f.max_depth,
                edge_count: f.edge_count,
            }
        })
        .collect();

    CacheOverview {
        entries,
        total_entries: cache_len,
        capacity: cache_cap,
        ttl_ms: CACHE_TTL_MS,
    }
}

// ─── 6. WPT Status Panel ──���───────────────────────────────────────────────

/// WPT suite status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WptSuiteStatus {
    pub suite: String,
    pub total_cases: u32,
    pub passed: u32,
    pub rate_percent: f32,
    /// Missing API surfaces for full compatibility
    pub missing_apis: Vec<String>,
}

/// WPT panel with regression detection
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WptPanel {
    pub suites: Vec<WptSuiteStatus>,
    pub total_cases: u32,
    pub total_passed: u32,
    pub overall_rate: f32,
    pub last_updated: String,
}

/// Build WPT baseline panel
pub fn build_wpt_baseline() -> WptPanel {
    let suites = vec![
        WptSuiteStatus {
            suite: "dom/nodes".into(),
            total_cases: 6676,
            passed: 5666,
            rate_percent: 84.9,
            missing_apis: vec!["Node.isEqualNode".into(), "Document.createEvent".into()],
        },
        WptSuiteStatus {
            suite: "dom/events".into(),
            total_cases: 318,
            passed: 213,
            rate_percent: 67.0,
            missing_apis: vec![
                "CustomEvent".into(),
                "Event.composedPath".into(),
                "AbortSignal".into(),
            ],
        },
        WptSuiteStatus {
            suite: "dom/ranges".into(),
            total_cases: 10943,
            passed: 7404,
            rate_percent: 67.7,
            missing_apis: vec![
                "Range.extractContents".into(),
                "Range.surroundContents".into(),
            ],
        },
        WptSuiteStatus {
            suite: "dom/traversal".into(),
            total_cases: 1584,
            passed: 1449,
            rate_percent: 91.5,
            missing_apis: vec!["NodeIterator edge cases".into()],
        },
        WptSuiteStatus {
            suite: "dom/collections".into(),
            total_cases: 48,
            passed: 27,
            rate_percent: 56.2,
            missing_apis: vec![
                "HTMLCollection.namedItem".into(),
                "HTMLOptionsCollection".into(),
            ],
        },
        WptSuiteStatus {
            suite: "dom/lists".into(),
            total_cases: 189,
            passed: 181,
            rate_percent: 95.8,
            missing_apis: vec![],
        },
        WptSuiteStatus {
            suite: "domparsing".into(),
            total_cases: 453,
            passed: 85,
            rate_percent: 18.8,
            missing_apis: vec![
                "DOMParser".into(),
                "XMLSerializer".into(),
                "innerHTML setter".into(),
                "outerHTML setter".into(),
            ],
        },
        WptSuiteStatus {
            suite: "css/selectors".into(),
            total_cases: 3457,
            passed: 1840,
            rate_percent: 53.2,
            missing_apis: vec![
                ":has()".into(),
                ":is()/:where()".into(),
                "::part()".into(),
                ":host-context()".into(),
            ],
        },
        WptSuiteStatus {
            suite: "css/cssom".into(),
            total_cases: 531,
            passed: 76,
            rate_percent: 14.3,
            missing_apis: vec![
                "CSSStyleSheet".into(),
                "getComputedStyle completeness".into(),
                "CSSRule".into(),
            ],
        },
        WptSuiteStatus {
            suite: "html/syntax".into(),
            total_cases: 340,
            passed: 68,
            rate_percent: 20.0,
            missing_apis: vec![
                "template element".into(),
                "foreign content parsing".into(),
                "adoption agency".into(),
            ],
        },
    ];
    let total_cases: u32 = suites.iter().map(|s| s.total_cases).sum();
    let total_passed: u32 = suites.iter().map(|s| s.passed).sum();
    let overall_rate = if total_cases > 0 {
        total_passed as f32 / total_cases as f32 * 100.0
    } else {
        0.0
    };
    WptPanel {
        suites,
        total_cases,
        total_passed,
        overall_rate,
        last_updated: "2026-03-26".into(),
    }
}

// ─── Engine Capabilities ───────────────────────────────────────────────────

/// Engine capability info
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineCapabilities {
    pub phases_completed: u32,
    pub scoring_pipeline: Vec<String>,
    pub trust_patterns: u32,
    pub firewall_levels: u32,
    pub hydration_frameworks: u32,
    pub render_tiers: Vec<String>,
    pub features: Vec<FeatureStatus>,
}

/// Feature status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureStatus {
    pub name: String,
    pub status: String,
    pub detail: String,
}

/// Build engine capabilities
pub fn build_engine_capabilities() -> EngineCapabilities {
    EngineCapabilities {
        phases_completed: 18,
        scoring_pipeline: vec![
            "BM25 (Okapi, k1=1.2)".into(),
            "HDC 2048-bit pruning".into(),
            "MiniLM bi-encoder".into(),
            "ColBERT MaxSim (opt-in)".into(),
            "CRFR wave propagation".into(),
        ],
        trust_patterns: 20,
        firewall_levels: 3,
        hydration_frameworks: 10,
        render_tiers: vec![
            "Tier 1: Blitz (pure Rust, ~10-50ms)".into(),
            "Tier 2: CDP/Chrome (JS-heavy, feature-gated)".into(),
        ],
        features: vec![
            FeatureStatus {
                name: "Arena DOM".into(),
                status: "active".into(),
                detail: "SlotMap-based, 5-10x faster DFS".into(),
            },
            FeatureStatus {
                name: "Streaming Parse".into(),
                status: "active".into(),
                detail: "Adaptive + directive-driven, 95-99% token savings".into(),
            },
            FeatureStatus {
                name: "Session Management".into(),
                status: "active".into(),
                detail: "Cookie jar, OAuth 2.0, token refresh".into(),
            },
            FeatureStatus {
                name: "Workflow Orchestration".into(),
                status: "active".into(),
                detail: "Multi-page with rollback/retry".into(),
            },
            FeatureStatus {
                name: "XHR Interception".into(),
                status: "active".into(),
                detail: "Firewall-filtered, response caching".into(),
            },
            FeatureStatus {
                name: "Vision (YOLOv8)".into(),
                status: "active".into(),
                detail: "ONNX inference, 9 UI element types".into(),
            },
            FeatureStatus {
                name: "Causal Action Graph".into(),
                status: "active".into(),
                detail: "State transitions, safest-path finding".into(),
            },
            FeatureStatus {
                name: "WebMCP Discovery".into(),
                status: "active".into(),
                detail: "navigator.modelContext, script[mcp+json]".into(),
            },
        ],
    }
}

// ─── Full Dashboard Snapshot ────────��───────────────────────────────────────

/// Complete dashboard snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    pub crfr_cache: CacheOverview,
    pub memory_stats: MemoryStatsView,
    pub wpt_baseline: WptPanel,
    pub spa_runtime: SpaRuntimeSnapshot,
    pub engine: EngineCapabilities,
    pub persist: PersistStats,
    pub vision_available: bool,
    pub endpoint_count: usize,
    pub timestamp_ms: u64,
}

/// Persistence stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistStats {
    pub enabled: bool,
    pub stored_fields: usize,
    pub stored_domains: usize,
    pub db_size_bytes: u64,
}

/// Memory stats
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatsView {
    pub rss_mb: u64,
    pub peak_rss_mb: u64,
    pub threads: u64,
}

/// Read process memory (Linux /proc/self/status)
pub fn read_memory_stats() -> MemoryStatsView {
    let mut rss_mb = 0u64;
    let mut peak_rss_mb = 0u64;
    let mut threads = 0u64;

    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(val) = line.strip_prefix("VmRSS:") {
                let kb: u64 = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                rss_mb = kb / 1024;
            } else if let Some(val) = line.strip_prefix("VmHWM:") {
                let kb: u64 = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                peak_rss_mb = kb / 1024;
            } else if let Some(val) = line.strip_prefix("Threads:") {
                threads = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
        }
    }

    MemoryStatsView {
        rss_mb,
        peak_rss_mb,
        threads,
    }
}

/// Build CrfrNodeRank from result data (truncate long labels)
pub fn build_crfr_node_rank(
    node_id: u32,
    role: &str,
    label: &str,
    amplitude: f32,
    confidence: f32,
    resonance_type: &str,
    causal_boost: f32,
) -> CrfrNodeRank {
    CrfrNodeRank {
        node_id,
        role: role.to_string(),
        label: if label.len() > 120 {
            format!("{}...", &label[..label.floor_char_boundary(117)])
        } else {
            label.to_string()
        },
        amplitude,
        confidence,
        resonance_type: resonance_type.to_string(),
        causal_boost,
    }
}

// ─── Tests ───────��──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_propagation_weight_uniform() {
        let w = build_propagation_weight("heading:down", 1.0, 1.0);
        assert!(
            (w.mean - 0.5).abs() < 0.01,
            "Uniform prior borde ge mean ~0.5, fick {}",
            w.mean
        );
        assert!(
            w.confidence >= 0.0 && w.confidence <= 1.0,
            "Confidence borde vara [0,1], fick {}",
            w.confidence
        );
    }

    #[test]
    fn test_build_propagation_weight_strong_alpha() {
        let w = build_propagation_weight("button:up", 10.0, 2.0);
        assert!(
            w.mean > 0.7,
            "Stark alpha borde ge hog mean, fick {}",
            w.mean
        );
        assert!(
            w.variance > 0.0,
            "Variansen borde vara positiv, fick {}",
            w.variance
        );
    }

    #[test]
    fn test_build_propagation_weight_zero() {
        let w = build_propagation_weight("x:down", 0.0, 0.0);
        assert!((w.mean - 0.5).abs() < 0.01, "Noll-summa borde ge 0.5 mean");
    }

    #[test]
    fn test_wpt_baseline_has_all_suites() {
        let panel = build_wpt_baseline();
        assert_eq!(panel.suites.len(), 10, "Borde ha 10 WPT-suites");
        assert!(
            panel.suites.iter().all(|s| s.rate_percent > 0.0),
            "Alla suites borde ha positiv rate"
        );
        assert!(panel.total_cases > 0, "Total cases borde vara > 0");
        assert!(
            (panel.overall_rate - 69.3).abs() < 1.0,
            "Overall rate borde vara ~69.3%, fick {}",
            panel.overall_rate
        );
    }

    #[test]
    fn test_wpt_missing_apis() {
        let panel = build_wpt_baseline();
        let domparsing = panel
            .suites
            .iter()
            .find(|s| s.suite == "domparsing")
            .unwrap();
        assert!(
            !domparsing.missing_apis.is_empty(),
            "domparsing borde ha missing APIs"
        );
        assert!(
            domparsing
                .missing_apis
                .iter()
                .any(|a| a.contains("DOMParser")),
            "domparsing borde sakna DOMParser"
        );
    }

    #[test]
    fn test_read_memory_stats_no_panic() {
        let stats = read_memory_stats();
        assert!(
            stats.rss_mb < 1_000_000,
            "RSS borde vara rimlig, fick {}",
            stats.rss_mb
        );
    }

    #[test]
    fn test_crfr_node_rank_truncation() {
        let long_label = "a".repeat(200);
        let rank = build_crfr_node_rank(1, "text", &long_label, 0.5, 0.8, "Direct", 0.0);
        assert!(
            rank.label.len() <= 123,
            "Label borde trunkeras, fick {} tecken",
            rank.label.len()
        );
        assert!(
            rank.label.ends_with("..."),
            "Trunkerad label borde sluta med ..."
        );
    }

    #[test]
    fn test_spa_runtime_snapshot() {
        let spa = build_spa_runtime();
        assert_eq!(spa.max_timers, 500);
        assert!(spa.mutation_observer);
        assert_eq!(spa.dom_element_wrappers, 90);
    }

    #[test]
    fn test_engine_capabilities() {
        let caps = build_engine_capabilities();
        assert_eq!(caps.phases_completed, 18);
        assert!(
            caps.scoring_pipeline.len() >= 4,
            "Borde ha minst 4 scoring stages"
        );
        assert!(!caps.features.is_empty(), "Borde ha features");
    }

    #[test]
    fn test_cache_overview_ttl() {
        let fields = vec![];
        let overview = build_cache_overview(&fields, 0, 64, 1000);
        assert_eq!(overview.capacity, 64);
        assert_eq!(overview.ttl_ms, CACHE_TTL_MS);
    }

    #[test]
    fn test_dashboard_snapshot_serialization() {
        let snap = DashboardSnapshot {
            crfr_cache: CacheOverview {
                entries: vec![],
                total_entries: 0,
                capacity: 64,
                ttl_ms: CACHE_TTL_MS,
            },
            memory_stats: MemoryStatsView {
                rss_mb: 128,
                peak_rss_mb: 200,
                threads: 4,
            },
            wpt_baseline: build_wpt_baseline(),
            spa_runtime: build_spa_runtime(),
            engine: build_engine_capabilities(),
            persist: PersistStats {
                enabled: true,
                stored_fields: 5,
                stored_domains: 2,
                db_size_bytes: 4096,
            },
            vision_available: false,
            endpoint_count: 72,
            timestamp_ms: 1234567890,
        };
        let json = serde_json::to_string(&snap).expect("Borde kunna serialisera DashboardSnapshot");
        assert!(json.contains("crfr_cache"), "JSON borde ha crfr_cache");
        assert!(json.contains("wpt_baseline"), "JSON borde ha wpt_baseline");
        assert!(json.contains("spa_runtime"), "JSON borde ha spa_runtime");
        assert!(json.contains("engine"), "JSON borde ha engine");
        assert!(json.contains("overall_rate"), "JSON borde ha overall_rate");
    }
}
