// Fas 19: Adaptive Multi-Page Crawling
//
// Intelligent crawling med:
// - Contextual Thompson Sampling för link selection
// - HDC Bundle Saturation för stopping
// - CRFR-baserad per-page distillation
//
// Använder befintlig fetch.rs, link_extract.rs, resonance.rs, scoring/hdc.rs.

use std::collections::{BinaryHeap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::link_extract::{self, EnrichedLink, LinkExtractionConfig};
use crate::scoring::hdc::Hypervector;
use crate::types::{FetchConfig, SemanticNode};

// ─── Konstanter ─────────────────────────────────────────────────────────────

/// EMA smoothing factor för marginal gain
const EMA_ALPHA: f32 = 0.3;
/// Default min gain threshold (HDC saturation)
const DEFAULT_MIN_GAIN: f32 = 0.02;
/// Default confidence threshold (BM25 term coverage)
const DEFAULT_CONFIDENCE: f32 = 0.95;
/// Max antal satisficing-missar
const DEFAULT_CONSECUTIVE_LOW: u32 = 3;

// ─── Typer ──────────────────────────────────────────────────────────────────

/// Konfiguration för adaptive crawling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveConfig {
    /// Max antal sidor att crawla
    #[serde(default = "default_max_pages")]
    pub max_pages: usize,
    /// Max klickdjup
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Antal bästa links att följa per sida
    #[serde(default = "default_top_k")]
    pub top_k_links: usize,
    /// HDC saturation tröskel (lägre = crawla mer)
    #[serde(default = "default_min_gain")]
    pub min_gain_threshold: f32,
    /// BM25 term coverage tröskel
    #[serde(default = "default_confidence")]
    pub confidence_threshold: f32,
    /// Satisficing: stopp efter N sidor utan nytt
    #[serde(default = "default_consecutive_low")]
    pub consecutive_low_gain_max: u32,
    /// Respektera robots.txt
    #[serde(default = "default_true")]
    pub respect_robots_txt: bool,
    /// Timeout per sida (ms)
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// Max noder att extrahera per sida (CRFR top-N)
    #[serde(default = "default_top_n")]
    pub top_n_per_page: u32,
    /// Fetch-konfiguration
    #[serde(default)]
    pub fetch_config: FetchConfig,
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
    DEFAULT_MIN_GAIN
}
fn default_confidence() -> f32 {
    DEFAULT_CONFIDENCE
}
fn default_consecutive_low() -> u32 {
    DEFAULT_CONSECUTIVE_LOW
}
fn default_true() -> bool {
    true
}
fn default_timeout() -> u64 {
    30_000
}
fn default_top_n() -> u32 {
    15
}

impl Default for AdaptiveConfig {
    fn default() -> Self {
        AdaptiveConfig {
            max_pages: default_max_pages(),
            max_depth: default_max_depth(),
            top_k_links: default_top_k(),
            min_gain_threshold: default_min_gain(),
            confidence_threshold: default_confidence(),
            consecutive_low_gain_max: default_consecutive_low(),
            respect_robots_txt: true,
            timeout_ms: default_timeout(),
            top_n_per_page: default_top_n(),
            fetch_config: FetchConfig::default(),
        }
    }
}

/// Varför crawlen stoppade
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StopReason {
    /// HDC marginal gain under tröskel
    HdcSaturation,
    /// Alla goal-termer hittade
    TermCoverage,
    /// N sidor utan signifikant nytt (satisficing)
    Satisficing,
    /// Max antal sidor nått
    MaxPages,
    /// Timeout
    Timeout,
    /// Inga fler links i frontier
    NoMoreLinks,
}

/// Resultat per crawlad sida
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlPageResult {
    /// Slutgiltig URL
    pub url: String,
    /// Sidtitel
    pub title: String,
    /// CRFR-filtrerade top-noder
    pub top_nodes: Vec<SemanticNode>,
    /// Antal links hittade
    pub links_found: u32,
    /// Antal links pushade till frontier
    pub links_followed: u32,
    /// HDC marginal gain denna sida bidrog
    pub marginal_gain: f32,
    /// Sida-nummer (1-indexerat)
    pub page_number: u32,
    /// Djup från startsida
    pub depth: u32,
    /// Fetch-tid i ms
    pub fetch_time_ms: u64,
    /// Parse-tid i ms
    pub parse_time_ms: u64,
    /// Rå HTML-storlek i tecken
    pub raw_html_chars: usize,
    /// Extraherad text-storlek i tecken
    pub out_chars: usize,
}

/// Slutresultat från en adaptive crawl
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdaptiveCrawlResult {
    /// Mål
    pub goal: String,
    /// Alla crawlade sidor
    pub pages: Vec<CrawlPageResult>,
    /// Totalt antal sidor
    pub total_pages: u32,
    /// Varför vi stoppade
    pub stop_reason: StopReason,
    /// BM25 term coverage (0.0–1.0)
    pub coverage: f32,
    /// HDC saturation senaste EMA (0.0–1.0)
    pub final_ema_gain: f32,
    /// Total tid i ms
    pub total_time_ms: u64,
    /// Totalt antal extraherade noder
    pub total_nodes_extracted: u32,
    /// Totalt raw HTML chars (alla sidor)
    pub total_raw_chars: usize,
    /// Totalt extraherade chars (alla sidor)
    pub total_out_chars: usize,
}

// ─── Frontier (prioritetskö) ────────────────────────────────────────────────

/// En länk i frontier-kön med Thompson-samplad expected gain
#[derive(Debug, Clone)]
pub struct ScoredLink {
    pub url: String,
    pub expected_gain: f32,
    pub depth: u32,
    pub source_url: String,
    pub anchor_text: String,
    pub structural_role: String,
}

impl PartialEq for ScoredLink {
    fn eq(&self, other: &Self) -> bool {
        self.expected_gain.total_cmp(&other.expected_gain) == std::cmp::Ordering::Equal
    }
}

impl Eq for ScoredLink {}

impl PartialOrd for ScoredLink {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScoredLink {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.expected_gain.total_cmp(&other.expected_gain)
    }
}

// ─── CrawlSession ───────────────────────────────────────────────────────────

/// Tillstånd för en pågående adaptive crawl
pub struct CrawlSession {
    /// Mål
    pub goal: String,
    /// Goal-termer (tokeniserade)
    goal_words: Vec<String>,
    /// Konfiguration
    pub config: AdaptiveConfig,
    /// Besökta URLs
    visited: HashSet<String>,
    /// Prioritetskö med links att följa
    frontier: BinaryHeap<ScoredLink>,
    /// HDC bundle — ackumulerat semantiskt innehåll
    accumulated_hv: Hypervector,
    /// EMA av marginal gain
    ema_gain: f32,
    /// Goal-termer som hittats
    term_hits: HashSet<String>,
    /// Antal sidor crawlade
    pages_crawled: u32,
    /// Antal sidor i rad med låg gain
    consecutive_low_gain: u32,
    /// Ackumulerade resultat
    pub results: Vec<CrawlPageResult>,
    /// Beta-parametrar per link-kluster: kluster → (successes, failures)
    link_stats: HashMap<String, (f32, f32)>,
    /// Starttid
    start_time_ms: u64,
}

impl CrawlSession {
    /// Skapa ny crawl-session
    pub fn new(goal: &str, start_url: &str, config: AdaptiveConfig) -> Self {
        let goal_words: Vec<String> = goal
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .map(String::from)
            .collect();

        let mut frontier = BinaryHeap::new();
        frontier.push(ScoredLink {
            url: start_url.to_string(),
            expected_gain: 1.0, // Startsida = max priority
            depth: 0,
            source_url: String::new(),
            anchor_text: goal.to_string(),
            structural_role: "seed".to_string(),
        });

        CrawlSession {
            goal: goal.to_string(),
            goal_words,
            config,
            visited: HashSet::new(),
            frontier,
            accumulated_hv: Hypervector::zero(),
            ema_gain: 1.0, // Hög initial gain
            term_hits: HashSet::new(),
            pages_crawled: 0,
            consecutive_low_gain: 0,
            results: Vec::new(),
            link_stats: HashMap::new(),
            start_time_ms: now_ms(),
        }
    }

    /// Nästa URL att crawla (None = klart)
    pub fn next_url(&mut self) -> Option<ScoredLink> {
        while let Some(link) = self.frontier.pop() {
            if !self.visited.contains(&link.url) {
                return Some(link);
            }
        }
        None
    }

    /// Kolla om vi ska stoppa
    pub fn should_stop(&self) -> Option<StopReason> {
        if self.pages_crawled >= self.config.max_pages as u32 {
            return Some(StopReason::MaxPages);
        }
        // Kolla timeout
        let elapsed = now_ms().saturating_sub(self.start_time_ms);
        let total_timeout = self.config.timeout_ms * self.config.max_pages as u64;
        if total_timeout > 0 && elapsed > total_timeout {
            return Some(StopReason::Timeout);
        }
        // HDC saturation (efter minst 2 sidor)
        if self.pages_crawled >= 2 && self.ema_gain < self.config.min_gain_threshold {
            return Some(StopReason::HdcSaturation);
        }
        // Term coverage
        if !self.goal_words.is_empty() {
            let coverage = self.coverage();
            if coverage >= self.config.confidence_threshold {
                return Some(StopReason::TermCoverage);
            }
        }
        // Satisficing
        if self.consecutive_low_gain >= self.config.consecutive_low_gain_max {
            return Some(StopReason::Satisficing);
        }
        // Tom frontier
        if self.frontier.is_empty() && self.pages_crawled > 0 {
            return Some(StopReason::NoMoreLinks);
        }
        None
    }

    /// BM25 term coverage: andel goal-termer som hittats
    pub fn coverage(&self) -> f32 {
        if self.goal_words.is_empty() {
            return 0.0;
        }
        self.term_hits.len() as f32 / self.goal_words.len() as f32
    }

    /// Processa med separata nod-set. Returnerar extraherade links för async HEAD-enrichment.
    #[allow(clippy::too_many_arguments)]
    pub fn process_page_with_links(
        &mut self,
        top_nodes: &[SemanticNode],
        full_nodes: &[SemanticNode],
        url: &str,
        title: &str,
        depth: u32,
        fetch_time_ms: u64,
        parse_time_ms: u64,
    ) -> link_extract::LinkExtractionResult {
        self.process_page_inner(
            top_nodes,
            Some(full_nodes),
            url,
            title,
            depth,
            fetch_time_ms,
            parse_time_ms,
        )
    }

    /// Processa en hämtad sida. Returnerar links. Anropa push_enriched_links() efteråt.
    pub fn process_page(
        &mut self,
        tree_nodes: &[SemanticNode],
        url: &str,
        title: &str,
        depth: u32,
        fetch_time_ms: u64,
        parse_time_ms: u64,
    ) -> link_extract::LinkExtractionResult {
        let result = self.process_page_inner(
            tree_nodes,
            None,
            url,
            title,
            depth,
            fetch_time_ms,
            parse_time_ms,
        );
        // Sync path: pusha links direkt (utan HEAD-enrichment)
        self.push_enriched_links(&result.links, depth);
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn process_page_inner(
        &mut self,
        tree_nodes: &[SemanticNode],
        full_nodes_for_links: Option<&[SemanticNode]>,
        url: &str,
        title: &str,
        depth: u32,
        fetch_time_ms: u64,
        parse_time_ms: u64,
    ) -> link_extract::LinkExtractionResult {
        self.visited.insert(url.to_string());
        self.pages_crawled += 1;

        // Extrahera top-N noder
        let top_nodes: Vec<SemanticNode> = tree_nodes
            .iter()
            .take(self.config.top_n_per_page as usize)
            .cloned()
            .collect();

        // Uppdatera term coverage
        let combined_text: String = top_nodes
            .iter()
            .map(|n| n.label.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        for word in &self.goal_words {
            if combined_text.contains(word.as_str()) {
                self.term_hits.insert(word.clone());
            }
        }

        // HDC saturation: beräkna marginal gain
        let page_hv = Hypervector::from_text_ngrams(&combined_text);
        let marginal_gain = if self.pages_crawled == 1 {
            // Första sidan: full nyhet
            self.accumulated_hv = page_hv;
            1.0
        } else {
            let new_accumulated = Hypervector::bundle(&[&self.accumulated_hv, &page_hv]);
            let sim = self.accumulated_hv.similarity(&new_accumulated);
            let gain = ((1.0 - sim) / 2.0).clamp(0.0, 1.0);
            self.accumulated_hv = new_accumulated;
            gain
        };

        // EMA uppdatering
        self.ema_gain = EMA_ALPHA * marginal_gain + (1.0 - EMA_ALPHA) * self.ema_gain;

        // Satisficing: var denna sida low-gain?
        if marginal_gain < self.config.min_gain_threshold * 2.0 {
            self.consecutive_low_gain += 1;
        } else {
            self.consecutive_low_gain = 0;
        }

        // Extrahera links (returneras för async HEAD-enrichment)
        let link_config = LinkExtractionConfig {
            goal: Some(self.goal.clone()),
            max_links: self.config.top_k_links * 3,
            include_context: true,
            include_structural_role: true,
            filter_navigation: false,
            min_relevance: 0.0,
            ..Default::default()
        };
        let link_source = full_nodes_for_links.unwrap_or(tree_nodes);
        let link_result = link_extract::extract_links_from_tree(
            link_source,
            url,
            &link_config,
            Some(&self.accumulated_hv),
        );

        // Beräkna chars out
        let out_chars: usize = top_nodes.iter().map(|n| n.label.len()).sum();

        // Spara resultat (links_followed uppdateras efter HEAD-enrichment)
        self.results.push(CrawlPageResult {
            url: url.to_string(),
            title: title.to_string(),
            top_nodes,
            links_found: link_result.total_found,
            links_followed: 0, // Uppdateras av push_enriched_links
            marginal_gain,
            page_number: self.pages_crawled,
            depth,
            fetch_time_ms,
            parse_time_ms,
            raw_html_chars: 0,
            out_chars,
        });

        link_result
    }

    /// Pusha berikade links till frontier (anropas efter async HEAD-enrichment)
    pub fn push_enriched_links(&mut self, links: &[EnrichedLink], depth: u32) {
        let mut scored = self.score_links(links, depth);
        scored.truncate(self.config.top_k_links);
        let followed = scored.len() as u32;
        for sl in scored {
            if !self.visited.contains(&sl.url) && sl.depth <= self.config.max_depth {
                self.frontier.push(sl);
            }
        }
        // Uppdatera senaste sidans links_followed
        if let Some(last) = self.results.last_mut() {
            last.links_followed = followed;
        }
    }

    /// Thompson Sampling: scora links med Beta-distribution
    fn score_links(&self, links: &[EnrichedLink], parent_depth: u32) -> Vec<ScoredLink> {
        let mut scored: Vec<ScoredLink> = links
            .iter()
            .filter(|l| !self.visited.contains(&l.absolute_url))
            .map(|link| {
                // Link-kluster: domän + structural role
                let cluster = format!(
                    "{}:{}",
                    link_extract::extract_domain_pub(&link.absolute_url),
                    link.structural_role
                );

                // Thompson Sampling: sample från Beta(α, β)
                let (alpha, beta) = self.link_stats.get(&cluster).copied().unwrap_or((1.0, 1.0)); // Uniform prior
                let thompson_sample = sample_beta(alpha, beta);

                // Kombinerad score
                let gain = thompson_sample * link.expected_gain;

                ScoredLink {
                    url: link.absolute_url.clone(),
                    expected_gain: gain,
                    depth: parent_depth + 1,
                    source_url: String::new(),
                    anchor_text: link.anchor_text.clone(),
                    structural_role: link.structural_role.clone(),
                }
            })
            .collect();

        scored.sort_by(|a, b| b.expected_gain.total_cmp(&a.expected_gain));
        scored
    }

    /// Uppdatera link-statistik efter att en sida crawlats
    pub fn update_link_stats(&mut self, url: &str, structural_role: &str, was_useful: bool) {
        let cluster = format!(
            "{}:{}",
            link_extract::extract_domain_pub(url),
            structural_role
        );
        let entry = self.link_stats.entry(cluster).or_insert((1.0, 1.0));
        if was_useful {
            entry.0 += 1.0; // α++
        } else {
            entry.1 += 1.0; // β++
        }
    }

    /// Bygg slutresultat
    pub fn finish(self, stop_reason: StopReason) -> AdaptiveCrawlResult {
        let total_nodes: u32 = self.results.iter().map(|p| p.top_nodes.len() as u32).sum();
        let total_raw: usize = self.results.iter().map(|p| p.raw_html_chars).sum();
        let total_out: usize = self.results.iter().map(|p| p.out_chars).sum();
        let total_time = now_ms().saturating_sub(self.start_time_ms);
        let coverage = self.coverage();
        AdaptiveCrawlResult {
            goal: self.goal,
            pages: self.results,
            total_pages: self.pages_crawled,
            stop_reason,
            coverage,
            final_ema_gain: self.ema_gain,
            total_time_ms: total_time,
            total_nodes_extracted: total_nodes,
            total_raw_chars: total_raw,
            total_out_chars: total_out,
        }
    }
}

// ─── Thompson Sampling ──────────────────────────────────────────────────────

/// Sample från Beta(α, β) distribution via Jöhnk's rejection algorithm.
///
/// Uses a thread-local xorshift64 PRNG (seeded from system time) so no
/// external dependencies are needed while still providing genuine stochastic
/// exploration for Thompson Sampling's explore/exploit balance.
///
/// Jöhnk's method: for U1, U2 ~ Uniform(0,1), if X = U1^{1/α} and
/// Y = U2^{1/β} satisfy X + Y ≤ 1, then X/(X+Y) ~ Beta(α, β).
/// Acceptance rate is adequate for the small α, β values typical in CRFR
/// cold-start (Beta(1,1) to ~Beta(15,15)).
fn sample_beta(alpha: f32, beta: f32) -> f32 {
    use std::cell::Cell;

    // Xorshift64 PRNG — Marsaglia 2003. Thread-local avoids locks.
    thread_local! {
        static RNG: Cell<u64> = Cell::new({
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64;
            // Ensure non-zero seed; mix in a constant for extra entropy
            (t ^ (t >> 33) ^ 0x6c62_272e_07bb_0142).max(1)
        });
    }

    let rand_f64 = || -> f64 {
        RNG.with(|rng| {
            let mut x = rng.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            rng.set(x);
            // Map upper 53 bits to (0, 1)
            (x >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
        })
    };

    let a = alpha as f64;
    let b = beta as f64;

    // Degenerate: one parameter is zero or negative
    if a <= 0.0 || b <= 0.0 {
        return (a / (a + b).max(f64::EPSILON)) as f32;
    }

    // For very large a+b the Beta is tightly concentrated around its mean.
    // Use proper Box-Muller normal approximation (much faster than Jöhnk's rejection
    // which has near-zero acceptance rate when both a and b are large).
    if a + b > 80.0 {
        let mean = a / (a + b);
        let var = (a * b) / ((a + b) * (a + b) * (a + b + 1.0));
        let std_dev = var.sqrt();
        // Box-Muller transform: (U1, U2) → Normal(0, 1)
        let u1 = rand_f64().max(f64::EPSILON);
        let u2 = rand_f64();
        let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        return (mean + std_dev * z).clamp(0.01, 0.99) as f32;
    }

    // Jöhnk's rejection sampling — typically accepts within 3-5 iterations
    // for the parameter ranges used in CRFR (α, β ∈ [0.5, 20]).
    for _ in 0..200 {
        let u1 = rand_f64().max(f64::EPSILON);
        let u2 = rand_f64().max(f64::EPSILON);
        let x = u1.powf(1.0 / a);
        let y = u2.powf(1.0 / b);
        let s = x + y;
        if s <= 1.0 + f64::EPSILON {
            return (x / s.max(f64::EPSILON)) as f32;
        }
    }
    // Fallback (unreachable in practice for valid parameters)
    (a / (a + b)) as f32
}

/// Tidstämpel i millisekunder
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ─── Async Crawl Loop ───────────────────────────────────────────────────────

/// Kör en komplett adaptive crawl.
///
/// Fetch-loopen anropar fetch_page() per URL, parsar med CRFR,
/// och uppdaterar CrawlSession tills should_stop() triggas.
#[cfg(feature = "fetch")]
pub async fn adaptive_crawl(
    start_url: &str,
    goal: &str,
    config: AdaptiveConfig,
) -> AdaptiveCrawlResult {
    let mut session = CrawlSession::new(goal, start_url, config.clone());

    loop {
        // Kolla stopp-villkor
        if let Some(reason) = session.should_stop() {
            return session.finish(reason);
        }

        // Hämta nästa URL
        let next = match session.next_url() {
            Some(link) => link,
            None => return session.finish(StopReason::NoMoreLinks),
        };

        let depth = next.depth;
        let structural_role = next.structural_role.clone();

        // Fetch
        let fetch_start = std::time::Instant::now();
        let fetch_result = crate::fetch::fetch_page(&next.url, &config.fetch_config).await;
        let fetch_time_ms = fetch_start.elapsed().as_millis() as u64;

        let (html, final_url, title) = match fetch_result {
            Ok(result) => {
                if result.status_code >= 400 {
                    session.update_link_stats(&next.url, &structural_role, false);
                    continue;
                }
                let title = extract_title_from_html(&result.body);
                (result.body, result.final_url, title)
            }
            Err(_) => {
                session.update_link_stats(&next.url, &structural_role, false);
                continue;
            }
        };

        // Parse: full tree för link extraction + CRFR top-N för content
        let parse_start = std::time::Instant::now();

        // Full tree (alla noder, inklusive links)
        let full_tree_json = crate::parse_to_semantic_tree(&html, goal, &final_url);
        let full_nodes = parse_crfr_nodes(&full_tree_json);

        // CRFR top-N (content-filtrerade noder)
        let crfr_json = crate::parse_crfr(
            &html,
            goal,
            &final_url,
            config.top_n_per_page,
            false,
            "json",
        );
        let top_nodes = parse_crfr_nodes(&crfr_json);

        let parse_time_ms = parse_start.elapsed().as_millis() as u64;

        // Avgör om sidan var användbar (har relevanta noder)
        let was_useful = top_nodes.iter().any(|n| n.relevance > 0.3);

        session.update_link_stats(&final_url, &structural_role, was_useful);

        // OBS: CRFR feedback lämnas INTE automatiskt här.
        // Den externa LLM-agenten analyserar resultaten och anropar
        // crfr_feedback() med de nod-ID:n som faktiskt besvarade frågan.
        // Auto-feedback vore cirkulärt (vi matar tillbaka vår egen scoring).

        // Processa: top_nodes för content, full_nodes för link extraction
        let html_len = html.len();
        let mut link_result = session.process_page_with_links(
            &top_nodes,
            &full_nodes,
            &final_url,
            &title,
            depth,
            fetch_time_ms,
            parse_time_ms,
        );
        // Sätt raw_html_chars
        if let Some(last) = session.results.last_mut() {
            last.raw_html_chars = html_len;
        }

        // HEAD-enrich links innan vi väljer vilka att följa
        // Ger bättre scoring: title + meta description matchar mot goal
        if !link_result.links.is_empty() {
            crate::link_extract::enrich_links_with_head(
                &mut link_result.links,
                &session.goal_words,
                6, // Begränsad concurrency per sida (inte 8 — vi crawlar redan)
            )
            .await;
        }

        // Pusha berikade links till frontier
        session.push_enriched_links(&link_result.links, depth);
    }
}

/// Extrahera <title> från HTML (snabb regex-fri)
fn extract_title_from_html(html: &str) -> String {
    let lower = html.to_lowercase();
    if let Some(start) = lower.find("<title") {
        if let Some(tag_end) = lower[start..].find('>') {
            let content_start = start + tag_end + 1;
            if let Some(end) = lower[content_start..].find("</title>") {
                let title = &html[content_start..content_start + end];
                return title.trim().to_string();
            }
        }
    }
    String::new()
}

/// Parse CRFR JSON-output till SemanticNode-vektor
fn parse_crfr_nodes(json: &str) -> Vec<SemanticNode> {
    // CRFR returnerar {"nodes": [...], ...}
    #[derive(Deserialize)]
    struct CrfrOutput {
        #[serde(default)]
        nodes: Vec<SemanticNode>,
    }
    serde_json::from_str::<CrfrOutput>(json)
        .map(|o| o.nodes)
        .unwrap_or_default()
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SemanticNode;

    fn make_node(id: u32, role: &str, label: &str, relevance: f32) -> SemanticNode {
        let mut n = SemanticNode::new(id, role, label);
        n.relevance = relevance;
        n
    }

    fn make_link_node(id: u32, label: &str, href: &str) -> SemanticNode {
        let mut n = SemanticNode::new(id, "link", label);
        n.value = Some(href.to_string());
        n
    }

    #[test]
    fn test_session_creation() {
        let session = CrawlSession::new(
            "hitta AI nyheter",
            "https://example.com",
            AdaptiveConfig::default(),
        );
        assert_eq!(session.pages_crawled, 0);
        assert!(session.should_stop().is_none(), "Ska inte stoppa direkt");
        assert!(session.ema_gain > 0.5, "Initial gain ska vara hög");
    }

    #[test]
    fn test_next_url_dedup() {
        let mut session =
            CrawlSession::new("test", "https://example.com", AdaptiveConfig::default());
        // Pop startsida
        let first = session.next_url();
        assert!(first.is_some(), "Ska ha startsida");
        assert_eq!(first.unwrap().url, "https://example.com");

        // Markera som besökt och pusha samma igen
        session.visited.insert("https://example.com".to_string());
        session.frontier.push(ScoredLink {
            url: "https://example.com".to_string(),
            expected_gain: 0.5,
            depth: 1,
            source_url: String::new(),
            anchor_text: "Hem".to_string(),
            structural_role: "navigation".to_string(),
        });

        // Ska returnera None (redan besökt)
        assert!(session.next_url().is_none(), "Ska skippa redan besökt URL");
    }

    #[test]
    fn test_stop_max_pages() {
        let config = AdaptiveConfig {
            max_pages: 2,
            ..Default::default()
        };
        let mut session = CrawlSession::new("test", "https://example.com", config);
        session.pages_crawled = 2;
        assert_eq!(
            session.should_stop(),
            Some(StopReason::MaxPages),
            "Ska stoppa vid max_pages"
        );
    }

    #[test]
    fn test_stop_hdc_saturation() {
        let config = AdaptiveConfig {
            min_gain_threshold: 0.05,
            ..Default::default()
        };
        let mut session = CrawlSession::new("test", "https://example.com", config);
        session.pages_crawled = 3;
        session.ema_gain = 0.01; // Låg gain
        assert_eq!(
            session.should_stop(),
            Some(StopReason::HdcSaturation),
            "Ska stoppa vid HDC saturation"
        );
    }

    #[test]
    fn test_stop_satisficing() {
        let config = AdaptiveConfig {
            consecutive_low_gain_max: 3,
            ..Default::default()
        };
        let mut session = CrawlSession::new("test", "https://example.com", config);
        session.pages_crawled = 5;
        session.consecutive_low_gain = 3;
        assert_eq!(
            session.should_stop(),
            Some(StopReason::Satisficing),
            "Ska stoppa vid satisficing"
        );
    }

    #[test]
    fn test_stop_term_coverage() {
        let config = AdaptiveConfig {
            confidence_threshold: 0.95,
            ..Default::default()
        };
        let mut session = CrawlSession::new("AI nyheter", "https://example.com", config);
        // Markera alla termer som hittade
        session.term_hits.insert("nyheter".to_string());
        assert_eq!(
            session.should_stop(),
            Some(StopReason::TermCoverage),
            "Ska stoppa vid full term coverage"
        );
    }

    #[test]
    fn test_process_page_updates_saturation() {
        let mut session = CrawlSession::new(
            "rust programmering",
            "https://example.com",
            AdaptiveConfig::default(),
        );
        session.visited.remove("https://example.com");

        let nodes = vec![
            make_node(1, "heading", "Rust Programming Guide", 0.9),
            make_node(2, "text", "Learn Rust programming language basics", 0.7),
            make_link_node(3, "Tutorial", "/tutorial"),
        ];

        session.process_page(&nodes, "https://example.com", "Rust Guide", 0, 100, 50);

        assert_eq!(session.pages_crawled, 1);
        assert!(!session.results.is_empty(), "Ska ha resultat");
        assert!(
            session.results[0].marginal_gain > 0.0,
            "Första sidan ska ha positiv marginal gain"
        );
    }

    #[test]
    fn test_process_page_convergence() {
        let mut session =
            CrawlSession::new("rust", "https://example.com", AdaptiveConfig::default());

        // Simulera samma content upprepade gånger → gain ska minska
        let nodes = vec![make_node(1, "text", "Rust programming language", 0.8)];

        for i in 0..5 {
            session.process_page(
                &nodes,
                &format!("https://example.com/page{i}"),
                "Rust",
                0,
                50,
                20,
            );
        }

        // EMA gain ska ha minskat mot 0
        assert!(
            session.ema_gain < 0.3,
            "EMA gain ska konvergera nedåt med repetitivt content: {}",
            session.ema_gain
        );
    }

    #[test]
    fn test_thompson_sampling_exploits_good_clusters() {
        // Beta(10, 1) = bra kluster → högt sample
        let good = sample_beta(10.0, 1.0);
        // Beta(1, 10) = dåligt kluster → lågt sample
        let bad = sample_beta(1.0, 10.0);
        assert!(
            good > bad,
            "Bra kluster ska ge högre sample: good={good}, bad={bad}"
        );
    }

    #[test]
    fn test_thompson_cold_start() {
        // Beta(1, 1) = uniform prior → bör ge ~0.5
        let sample = sample_beta(1.0, 1.0);
        assert!(
            (0.3..=0.7).contains(&sample),
            "Cold-start sample bör vara nära 0.5: {sample}"
        );
    }

    #[test]
    fn test_link_stats_update() {
        let mut session =
            CrawlSession::new("test", "https://example.com", AdaptiveConfig::default());

        session.update_link_stats("https://example.com/good", "content", true);
        session.update_link_stats("https://example.com/good", "content", true);
        session.update_link_stats("https://example.com/bad", "content", false);

        let stats = session.link_stats.get("example.com:content").unwrap();
        assert_eq!(stats.0, 3.0, "Alpha ska vara 3 (1 prior + 2 successes)");
        assert_eq!(stats.1, 2.0, "Beta ska vara 2 (1 prior + 1 failure)");
    }

    #[test]
    fn test_finish_result() {
        let mut session = CrawlSession::new(
            "AI research",
            "https://example.com",
            AdaptiveConfig::default(),
        );
        session.pages_crawled = 5;
        session.term_hits.insert("research".to_string());

        let result = session.finish(StopReason::HdcSaturation);
        assert_eq!(result.stop_reason, StopReason::HdcSaturation);
        assert_eq!(result.total_pages, 5);
        assert_eq!(result.goal, "AI research");
    }

    #[test]
    fn test_extract_title() {
        assert_eq!(
            extract_title_from_html("<html><head><title>Hello World</title></head></html>"),
            "Hello World"
        );
        assert_eq!(
            extract_title_from_html("<html><body>No title</body></html>"),
            ""
        );
        assert_eq!(
            extract_title_from_html("<TITLE>Mixed Case</TITLE>"),
            "Mixed Case"
        );
    }

    #[test]
    fn test_coverage_calculation() {
        let mut session = CrawlSession::new(
            "rust programming guide",
            "https://example.com",
            AdaptiveConfig::default(),
        );
        // "rust" (3 chars) → included, "programming" → included, "guide" → included
        assert_eq!(session.coverage(), 0.0, "Initial coverage ska vara 0");

        session.term_hits.insert("rust".to_string());
        let cov = session.coverage();
        assert!(cov > 0.0 && cov < 1.0, "Partiell coverage: {cov}");

        // Lägg till alla
        session.term_hits.insert("programming".to_string());
        session.term_hits.insert("guide".to_string());
        assert!(
            (session.coverage() - 1.0).abs() < 0.01,
            "Full coverage ska vara 1.0"
        );
    }
}
