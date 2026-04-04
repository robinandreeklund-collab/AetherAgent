/// AetherAgent Live Dashboard — Real-time Insight
///
/// Observability-lager som ger insyn i CRFR-pipeline, DOM-motor,
/// SPA-runtime, kausalt minne, fetch/cache-status och WPT-kvalitet.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ─── CRFR Query Explorer ────────────────────────────────────────────────────

/// Snapshot av en CRFR-query: hela pipelinen från goal till resultat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrfrQuerySnapshot {
    pub goal: String,
    pub url: String,
    pub bm25_top_count: usize,
    pub hdc_survivor_count: usize,
    pub propagation_iterations: u32,
    pub amplitude_gap_k: usize,
    pub causal_boosts: Vec<CausalBoostEntry>,
    pub top_nodes: Vec<CrfrNodeRank>,
    pub field_build_ms: u64,
    pub propagation_ms: u64,
    pub cache_hit: bool,
    pub total_queries: u32,
}

/// En nod i CRFR-rankingen
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

/// Kausal boost-entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalBoostEntry {
    pub node_id: u32,
    pub boost: f32,
    pub hit_count: u32,
}

// ─── DOM Visualizer ─────────────────────────────────────────────────────────

/// DOM-nodsvy för dashboard-visualisering
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

/// Hel DOM-trädvy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomTreeView {
    pub url: String,
    pub title: String,
    pub total_nodes: usize,
    pub max_depth: u32,
    pub roots: Vec<DomNodeView>,
}

// ─── SPA Runtime Monitor ────────────────────────────────────────────────────

/// SPA runtime-status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaRuntimeSnapshot {
    pub history_length: usize,
    pub current_url: String,
    pub history_entries: Vec<SpaHistoryEntry>,
    pub fetch_requests: Vec<SpaFetchEntry>,
    pub pending_requests: usize,
    pub event_loop_ticks: u64,
    pub timers_active: usize,
    pub observers_active: usize,
}

/// History-entry i SPA
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaHistoryEntry {
    pub url: String,
    pub method: String,
    pub timestamp_ms: u64,
}

/// Fetch/XHR-entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaFetchEntry {
    pub url: String,
    pub method: String,
    pub status: u16,
    pub blocked: bool,
    pub cached: bool,
    pub timestamp_ms: u64,
}

// ─── Propagation Weights & Causal Memory ────────────────────────────────────

/// Beta-distribution vikt per roll+direction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropagationWeight {
    pub key: String,
    pub alpha: f32,
    pub beta: f32,
    pub mean: f32,
    pub variance: f32,
    pub confidence: f32,
}

/// Kausalt minne-sammandrag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalMemoryView {
    pub propagation_weights: Vec<PropagationWeight>,
    pub concept_memory_count: usize,
    pub total_feedback_events: u32,
    pub domain_hash: u64,
    pub field_age_ms: u64,
}

// ─── Site & Cache Overview ──────────────────────────────────────────────────

/// En cachad site-entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteCacheEntry {
    pub url_hash: u64,
    pub node_count: usize,
    pub total_queries: u32,
    pub domain_hash: u64,
    pub created_at_ms: u64,
    pub age_ms: u64,
    pub propagation_weight_count: usize,
    pub concept_memory_count: usize,
}

/// Cache-översikt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheOverview {
    pub entries: Vec<SiteCacheEntry>,
    pub total_entries: usize,
    pub capacity: usize,
}

// ─── WPT Status Panel ──────────────────────────────────────────────────────

/// WPT suite-status (statisk baseline + eventuell live-data)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WptSuiteStatus {
    pub suite: String,
    pub total_cases: u32,
    pub passed: u32,
    pub rate_percent: f32,
}

/// WPT-panel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WptPanel {
    pub suites: Vec<WptSuiteStatus>,
    pub last_updated: String,
}

// ─── Full Dashboard Snapshot ────────────────────────────────────────────────

/// Komplett dashboard-snapshot: allt i ett JSON-anrop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    pub crfr_cache: CacheOverview,
    pub memory_stats: MemoryStatsView,
    pub wpt_baseline: WptPanel,
    pub vision_available: bool,
    pub endpoint_count: usize,
    pub timestamp_ms: u64,
}

/// Minnesstatistik
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStatsView {
    pub rss_mb: u64,
    pub threads: u64,
}

// ─── Builders ───────────────────────────────────────────────────────────────

/// Bygg PropagationWeight från Beta-parametrar
pub fn build_propagation_weight(key: &str, alpha: f32, beta: f32) -> PropagationWeight {
    let sum = alpha + beta;
    let mean = if sum > 0.0 { alpha / sum } else { 0.5 };
    let variance = if sum > 0.0 {
        (alpha * beta) / (sum * sum * (sum + 1.0))
    } else {
        0.0
    };
    // Confidence: log-skala av totala observationer (α+β-2 = netto obs)
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

/// Bygg CausalMemoryView från ett resonance-fälts stats
pub fn build_causal_memory_view(
    propagation_stats: &HashMap<String, (f32, f32)>,
    concept_memory_count: usize,
    total_queries: u32,
    domain_hash: u64,
    created_at_ms: u64,
    now_ms: u64,
) -> CausalMemoryView {
    let mut weights: Vec<PropagationWeight> = propagation_stats
        .iter()
        .map(|(k, &(a, b))| build_propagation_weight(k, a, b))
        .collect();
    weights.sort_by(|a, b| b.mean.total_cmp(&a.mean));

    CausalMemoryView {
        propagation_weights: weights,
        concept_memory_count,
        total_feedback_events: total_queries,
        domain_hash,
        field_age_ms: now_ms.saturating_sub(created_at_ms),
    }
}

/// Bygg WPT baseline-panel (statisk data från CLAUDE.md)
pub fn build_wpt_baseline() -> WptPanel {
    WptPanel {
        suites: vec![
            WptSuiteStatus {
                suite: "dom/nodes".into(),
                total_cases: 6676,
                passed: 5666,
                rate_percent: 84.9,
            },
            WptSuiteStatus {
                suite: "dom/events".into(),
                total_cases: 318,
                passed: 213,
                rate_percent: 67.0,
            },
            WptSuiteStatus {
                suite: "dom/ranges".into(),
                total_cases: 10943,
                passed: 7404,
                rate_percent: 67.7,
            },
            WptSuiteStatus {
                suite: "dom/traversal".into(),
                total_cases: 1584,
                passed: 1449,
                rate_percent: 91.5,
            },
            WptSuiteStatus {
                suite: "dom/collections".into(),
                total_cases: 48,
                passed: 27,
                rate_percent: 56.2,
            },
            WptSuiteStatus {
                suite: "dom/lists".into(),
                total_cases: 189,
                passed: 181,
                rate_percent: 95.8,
            },
            WptSuiteStatus {
                suite: "domparsing".into(),
                total_cases: 453,
                passed: 85,
                rate_percent: 18.8,
            },
            WptSuiteStatus {
                suite: "css/selectors".into(),
                total_cases: 3457,
                passed: 1840,
                rate_percent: 53.2,
            },
            WptSuiteStatus {
                suite: "css/cssom".into(),
                total_cases: 531,
                passed: 76,
                rate_percent: 14.3,
            },
            WptSuiteStatus {
                suite: "html/syntax".into(),
                total_cases: 340,
                passed: 68,
                rate_percent: 20.0,
            },
        ],
        last_updated: "2026-03-26".into(),
    }
}

/// Läs processminne (Linux /proc/self/status)
pub fn read_memory_stats() -> MemoryStatsView {
    let mut rss_mb = 0u64;
    let mut threads = 0u64;

    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(val) = line.strip_prefix("VmRSS:") {
                // Format: "VmRSS:    12345 kB"
                let kb: u64 = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
                rss_mb = kb / 1024;
            } else if let Some(val) = line.strip_prefix("Threads:") {
                threads = val
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            }
        }
    }

    MemoryStatsView { rss_mb, threads }
}

/// Bygg CrfrNodeRank från resonance-resultat + semantisk nod
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

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_propagation_weight_uniform() {
        let w = build_propagation_weight("heading:down", 1.0, 1.0);
        assert!(
            (w.mean - 0.5).abs() < 0.01,
            "Uniform prior borde ge mean ≈ 0.5, fick {}",
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
            "Stark alpha borde ge hög mean, fick {}",
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
    fn test_build_causal_memory_view() {
        let mut stats = HashMap::new();
        stats.insert("heading:down".to_string(), (5.0, 1.0));
        stats.insert("button:up".to_string(), (2.0, 3.0));

        let view = build_causal_memory_view(&stats, 4, 10, 12345, 1000, 5000);

        assert_eq!(view.propagation_weights.len(), 2, "Borde ha 2 vikter");
        assert_eq!(view.concept_memory_count, 4, "Borde ha 4 concepts");
        assert_eq!(view.field_age_ms, 4000, "Ålder borde vara 4000ms");
        // Sorterad: heading:down (mean=0.833) > button:up (mean=0.4)
        assert_eq!(
            view.propagation_weights[0].key, "heading:down",
            "Högst mean borde komma först"
        );
    }

    #[test]
    fn test_wpt_baseline_has_all_suites() {
        let panel = build_wpt_baseline();
        assert_eq!(panel.suites.len(), 10, "Borde ha 10 WPT-suites");
        assert!(
            panel.suites.iter().all(|s| s.rate_percent > 0.0),
            "Alla suites borde ha positiv rate"
        );
    }

    #[test]
    fn test_read_memory_stats_no_panic() {
        // Borde inte panika oavsett OS
        let stats = read_memory_stats();
        // På icke-Linux returneras 0:or
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
    fn test_crfr_node_rank_short_label() {
        let rank = build_crfr_node_rank(2, "button", "Köp nu", 0.9, 0.95, "CausalMemory", 0.3);
        assert_eq!(rank.label, "Köp nu", "Kort label borde behållas");
        assert_eq!(rank.role, "button");
        assert_eq!(rank.node_id, 2);
    }

    #[test]
    fn test_site_cache_entry_serialization() {
        let entry = SiteCacheEntry {
            url_hash: 123456,
            node_count: 42,
            total_queries: 5,
            domain_hash: 789,
            created_at_ms: 1000,
            age_ms: 3000,
            propagation_weight_count: 8,
            concept_memory_count: 2,
        };
        let json = serde_json::to_string(&entry).expect("Borde kunna serialisera SiteCacheEntry");
        assert!(json.contains("123456"), "JSON borde innehålla url_hash");
        assert!(json.contains("42"), "JSON borde innehålla node_count");
    }

    #[test]
    fn test_dashboard_snapshot_serialization() {
        let snap = DashboardSnapshot {
            crfr_cache: CacheOverview {
                entries: vec![],
                total_entries: 0,
                capacity: 64,
            },
            memory_stats: MemoryStatsView {
                rss_mb: 128,
                threads: 4,
            },
            wpt_baseline: build_wpt_baseline(),
            vision_available: false,
            endpoint_count: 65,
            timestamp_ms: 1234567890,
        };
        let json = serde_json::to_string(&snap).expect("Borde kunna serialisera DashboardSnapshot");
        assert!(json.contains("crfr_cache"), "JSON borde ha crfr_cache");
        assert!(json.contains("wpt_baseline"), "JSON borde ha wpt_baseline");
    }
}
