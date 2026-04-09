// Tool: adaptive_crawl — Adaptive Multi-Page Crawling
//
// Intelligent crawling med:
// - Contextual Thompson Sampling för link selection
// - HDC Bundle Saturation för stopping
// - CRFR per-page distillation

use serde::Deserialize;

use super::{now_ms, ToolResult};

/// Request-parametrar för adaptive_crawl
#[derive(Debug, Clone, Deserialize)]
pub struct CrawlRequest {
    /// Start-URL (krävs)
    pub url: String,
    /// Mål (krävs)
    pub goal: String,
    /// Max antal sidor
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,
    /// Max djup
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Länkar att följa per sida
    #[serde(default = "default_top_k")]
    pub top_k_links: usize,
    /// HDC saturation tröskel
    #[serde(default = "default_min_gain")]
    pub min_gain: f32,
    /// Top-N noder per sida
    #[serde(default = "default_top_n")]
    pub top_n_per_page: u32,
    /// Respektera robots.txt
    #[serde(default = "default_true")]
    pub respect_robots_txt: bool,
}

fn default_max_pages() -> usize {
    20
}
fn default_max_depth() -> u32 {
    3
}
fn default_top_k() -> usize {
    5
}
fn default_min_gain() -> f32 {
    0.02
}
fn default_top_n() -> u32 {
    15
}
fn default_true() -> bool {
    true
}

/// Bygg AdaptiveConfig från request
pub fn build_config(req: &CrawlRequest) -> crate::adaptive::AdaptiveConfig {
    crate::adaptive::AdaptiveConfig {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        top_k_links: req.top_k_links,
        min_gain_threshold: req.min_gain,
        top_n_per_page: req.top_n_per_page,
        respect_robots_txt: req.respect_robots_txt,
        ..Default::default()
    }
}

/// Synkron del: returnera info om att async krävs
pub fn execute(req: &CrawlRequest) -> ToolResult {
    let start = now_ms();
    let data = serde_json::json!({
        "action": "async_crawl_required",
        "url": req.url,
        "goal": req.goal,
        "max_pages": req.max_pages,
        "max_depth": req.max_depth,
        "top_k_links": req.top_k_links,
    });
    ToolResult::ok(data, now_ms().saturating_sub(start))
}
