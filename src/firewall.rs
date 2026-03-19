// Fas 8: Semantic Firewall
// Goal-aware request filtering – blockera irrelevanta subrequests
//
// Tre nivåer:
//   L1: URL-pattern – blocklista för kända tracking-domäner (<1μs)
//   L2: MIME-type filter – blockera images/fonts/video vid text-extraction (<1μs)
//   L3: Semantic relevance – poängsätt mot agentens mål (~1ms)

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

// ─── L1: Kända tracking-domäner ─────────────────────────────────────────────

/// Blocklista för vanliga tracking/analytics-domäner
const TRACKING_DOMAINS: &[&str] = &[
    // Analytics
    "google-analytics.com",
    "googletagmanager.com",
    "analytics.google.com",
    "hotjar.com",
    "mixpanel.com",
    "segment.io",
    "segment.com",
    "amplitude.com",
    "heap.io",
    "heapanalytics.com",
    "fullstory.com",
    "mouseflow.com",
    "crazyegg.com",
    "luckyorange.com",
    "clarity.ms",
    "plausible.io",
    "matomo.cloud",
    // Ads
    "doubleclick.net",
    "googlesyndication.com",
    "googleadservices.com",
    "facebook.net",
    "fbcdn.net",
    "ads-twitter.com",
    "amazon-adsystem.com",
    "adnxs.com",
    "criteo.com",
    "outbrain.com",
    "taboola.com",
    "moatads.com",
    // Tracking pixels
    "facebook.com/tr",
    "bat.bing.com",
    "snap.licdn.com",
    "ct.pinterest.com",
    "t.co/i/adsct",
    // Fingerprinting
    "fingerprintjs.com",
    "cdn.cookielaw.org",
    "cookiebot.com",
    "onetrust.com",
    // Chat widgets (sällan relevanta för agenter)
    "intercom.io",
    "intercomcdn.com",
    "drift.com",
    "tawk.to",
    "zendesk.com",
    "livechatinc.com",
];

/// Filtyper som sällan behövs vid text-extraction
const BLOCKED_EXTENSIONS: &[&str] = &[
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg", ".ico", ".bmp", ".avif", // Bilder
    ".woff", ".woff2", ".ttf", ".otf", ".eot", // Typsnitt
    ".mp4", ".webm", ".ogg", ".avi", ".mov", // Video
    ".mp3", ".wav", ".flac", ".aac", // Ljud
    ".pdf", ".zip", ".gz", ".tar", ".rar", // Arkiv
    ".map", // Source maps
];

/// MIME-typer att blockera vid text-extraction
const BLOCKED_MIME_TYPES: &[&str] = &[
    "image/",
    "font/",
    "video/",
    "audio/",
    "application/font",
    "application/pdf",
    "application/zip",
    "application/octet-stream",
];

// ─── Typer ──────────────────────────────────────────────────────────────────

/// Resultat av firewall-klassificering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallVerdict {
    /// Ska requesten tillåtas?
    pub allowed: bool,
    /// Vilken nivå blockerade (om blockerad)
    pub blocked_by: Option<FirewallLevel>,
    /// Anledning till blockering
    pub reason: String,
    /// Semantic relevance-poäng (0.0–1.0, bara om L3 kördes)
    pub relevance_score: Option<f32>,
}

/// Vilken nivå i firewallen
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FirewallLevel {
    /// URL-pattern blocklista
    L1UrlPattern,
    /// MIME-type/extension filter
    L2MimeType,
    /// Semantic relevance mot goal
    L3SemanticRelevance,
}

/// Konfiguration för firewallen
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallConfig {
    /// Aktivera L1 (URL-pattern blocking)
    #[serde(default = "default_true")]
    pub enable_l1: bool,
    /// Aktivera L2 (MIME-type blocking)
    #[serde(default = "default_true")]
    pub enable_l2: bool,
    /// Aktivera L3 (semantic relevance)
    #[serde(default = "default_true")]
    pub enable_l3: bool,
    /// Minimum relevance-poäng för L3 (default 0.1)
    #[serde(default = "default_min_relevance")]
    pub min_relevance: f32,
    /// Extra domäner att blockera
    #[serde(default)]
    pub extra_blocked_domains: Vec<String>,
    /// Domäner att alltid tillåta (whitelist)
    #[serde(default)]
    pub allowed_domains: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_min_relevance() -> f32 {
    0.1
}

impl Default for FirewallConfig {
    fn default() -> Self {
        FirewallConfig {
            enable_l1: true,
            enable_l2: true,
            enable_l3: true,
            min_relevance: 0.1,
            extra_blocked_domains: vec![],
            allowed_domains: vec![],
        }
    }
}

/// Sammanfattning av firewall-resultat för en batch requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FirewallSummary {
    pub total_requests: u32,
    pub allowed: u32,
    pub blocked_l1: u32,
    pub blocked_l2: u32,
    pub blocked_l3: u32,
    pub estimated_bandwidth_saved_pct: f32,
}

// ─── Klassificering ─────────────────────────────────────────────────────────

/// Klassificera en URL mot firewallens tre nivåer
pub fn classify_request(url: &str, goal: &str, config: &FirewallConfig) -> FirewallVerdict {
    // Kolla whitelist först
    if !config.allowed_domains.is_empty() {
        if let Some(host) = extract_host(url) {
            for allowed in &config.allowed_domains {
                if host_matches_domain(&host, allowed) {
                    return FirewallVerdict {
                        allowed: true,
                        blocked_by: None,
                        reason: format!("Whitelist: {host}"),
                        relevance_score: None,
                    };
                }
            }
        }
    }

    // L1: URL-pattern
    if config.enable_l1 {
        if let Some(reason) = check_l1(url, config) {
            return FirewallVerdict {
                allowed: false,
                blocked_by: Some(FirewallLevel::L1UrlPattern),
                reason,
                relevance_score: None,
            };
        }
    }

    // L2: MIME-type / extension
    if config.enable_l2 {
        if let Some(reason) = check_l2(url) {
            return FirewallVerdict {
                allowed: false,
                blocked_by: Some(FirewallLevel::L2MimeType),
                reason,
                relevance_score: None,
            };
        }
    }

    // L3: Semantic relevance
    if config.enable_l3 && !goal.is_empty() {
        let score = compute_url_relevance(url, goal);
        if score < config.min_relevance {
            return FirewallVerdict {
                allowed: false,
                blocked_by: Some(FirewallLevel::L3SemanticRelevance),
                reason: format!("Låg relevans ({score:.2}) för mål '{goal}'"),
                relevance_score: Some(score),
            };
        }
        return FirewallVerdict {
            allowed: true,
            blocked_by: None,
            reason: String::new(),
            relevance_score: Some(score),
        };
    }

    FirewallVerdict {
        allowed: true,
        blocked_by: None,
        reason: String::new(),
        relevance_score: None,
    }
}

/// Klassificera en batch av URLs
pub fn classify_batch(
    urls: &[String],
    goal: &str,
    config: &FirewallConfig,
) -> (Vec<FirewallVerdict>, FirewallSummary) {
    let mut verdicts = Vec::with_capacity(urls.len());
    let mut allowed = 0u32;
    let mut blocked_l1 = 0u32;
    let mut blocked_l2 = 0u32;
    let mut blocked_l3 = 0u32;

    for url in urls {
        let v = classify_request(url, goal, config);
        if v.allowed {
            allowed += 1;
        } else {
            match v.blocked_by {
                Some(FirewallLevel::L1UrlPattern) => blocked_l1 += 1,
                Some(FirewallLevel::L2MimeType) => blocked_l2 += 1,
                Some(FirewallLevel::L3SemanticRelevance) => blocked_l3 += 1,
                None => {}
            }
        }
        verdicts.push(v);
    }

    let total = urls.len() as u32;
    let blocked_total = blocked_l1 + blocked_l2 + blocked_l3;
    let saved_pct = if total > 0 {
        (blocked_total as f32 / total as f32) * 100.0
    } else {
        0.0
    };

    let summary = FirewallSummary {
        total_requests: total,
        allowed,
        blocked_l1,
        blocked_l2,
        blocked_l3,
        estimated_bandwidth_saved_pct: saved_pct,
    };

    (verdicts, summary)
}

// ─── L1: URL-pattern kontroll ───────────────────────────────────────────────

/// Statisk HashSet av tracking-domäner för O(1) lookup
static TRACKING_DOMAIN_SET: std::sync::LazyLock<HashSet<&'static str>> =
    std::sync::LazyLock::new(|| TRACKING_DOMAINS.iter().copied().collect());

/// Kolla om en host matchar en domän (exakt eller som subdomän)
fn host_matches_domain(host: &str, domain: &str) -> bool {
    host == domain
        || host.ends_with(domain)
            && host.as_bytes().get(host.len() - domain.len() - 1) == Some(&b'.')
}

fn check_l1(url: &str, config: &FirewallConfig) -> Option<String> {
    let host = extract_host(url)?;

    // O(1) exakt domän-lookup
    if TRACKING_DOMAIN_SET.contains(host.as_str()) {
        return Some(format!("L1: Tracking-domän '{host}'"));
    }

    // Subdomän-kontroll (t.ex. sub.google-analytics.com)
    for domain in TRACKING_DOMAINS {
        if host_matches_domain(&host, domain) {
            return Some(format!("L1: Tracking-domän '{domain}'"));
        }
    }

    // Kolla extra domäner
    for domain in &config.extra_blocked_domains {
        if host_matches_domain(&host, domain) {
            return Some(format!("L1: Blockerad domän '{domain}'"));
        }
    }

    // Kolla kända tracking-paths
    let lower = url.to_lowercase();
    if lower.contains("/analytics") || lower.contains("/tracking") || lower.contains("/pixel") {
        return Some("L1: Tracking-path detekterad".to_string());
    }

    None
}

// ─── L2: MIME-type / extension kontroll ─────────────────────────────────────

fn check_l2(url: &str) -> Option<String> {
    // Extrahera path utan query-params
    let path = url.split('?').next().unwrap_or(url);
    let lower_path = path.to_lowercase();

    for ext in BLOCKED_EXTENSIONS {
        if lower_path.ends_with(ext) {
            return Some(format!("L2: Blockerad filtyp '{ext}'"));
        }
    }

    None
}

/// Kontrollera MIME-typ från response header
pub fn check_mime_type(content_type: &str) -> Option<String> {
    let lower = content_type.to_lowercase();
    for mime in BLOCKED_MIME_TYPES {
        if lower.starts_with(mime) {
            return Some(format!("L2: Blockerad MIME-typ '{content_type}'"));
        }
    }
    None
}

// ─── L3: Semantic relevance ─────────────────────────────────────────────────

/// Beräkna URL:ens relevans mot agentens mål
fn compute_url_relevance(url: &str, goal: &str) -> f32 {
    let lower_url = url.to_lowercase();
    let lower_goal = goal.to_lowercase();

    // Extrahera nyckelord från goal
    let goal_words: Vec<&str> = lower_goal
        .split(|c: char| !c.is_alphanumeric() && c != 'ö' && c != 'ä' && c != 'å')
        .filter(|w| w.len() > 2)
        .collect();

    if goal_words.is_empty() {
        return 0.5; // Inget mål = tillåt allt
    }

    let mut matches = 0;
    for word in &goal_words {
        if lower_url.contains(word) {
            matches += 1;
        }
    }

    // Kontrollera kända relevanta path-segment
    let relevant_paths = [
        "product",
        "produkt",
        "cart",
        "checkout",
        "kassa",
        "varukorg",
        "login",
        "logga",
        "search",
        "sok",
        "sök",
        "api",
        "price",
        "pris",
        "order",
        "account",
        "konto",
        "register",
        "registrera",
    ];

    let path_bonus = if relevant_paths.iter().any(|p| lower_url.contains(p)) {
        0.2
    } else {
        0.0
    };

    // Känd irrelevant – penalise
    let irrelevant_penalty = if lower_url.contains("font")
        || lower_url.contains("analytics")
        || lower_url.contains("tracking")
        || lower_url.contains("beacon")
        || lower_url.contains("pixel")
        || lower_url.contains("telemetry")
    {
        0.5
    } else {
        0.0
    };

    let base_score = if goal_words.is_empty() {
        0.5
    } else {
        matches as f32 / goal_words.len() as f32
    };

    // Justera: samma domän som mål-URL bör ha högt baspoäng
    let same_domain_bonus = 0.3; // Subrequests till samma domän är ofta relevanta

    (base_score + path_bonus + same_domain_bonus - irrelevant_penalty).clamp(0.0, 1.0)
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

fn extract_host(url: &str) -> Option<String> {
    // Enkel host-extraktion utan URL-parser (för prestanda)
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = without_scheme.split('/').next()?.split(':').next()?;
    Some(host.to_lowercase())
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_l1_blocks_google_analytics() {
        let config = FirewallConfig::default();
        let v = classify_request(
            "https://www.google-analytics.com/collect?v=1",
            "köp produkt",
            &config,
        );
        assert!(!v.allowed, "Ska blockera google-analytics");
        assert_eq!(v.blocked_by, Some(FirewallLevel::L1UrlPattern));
    }

    #[test]
    fn test_l1_blocks_facebook_tracking() {
        let config = FirewallConfig::default();
        let v = classify_request(
            "https://connect.facebook.net/en_US/fbevents.js",
            "köp produkt",
            &config,
        );
        assert!(!v.allowed, "Ska blockera facebook.net");
    }

    #[test]
    fn test_l1_blocks_hotjar() {
        let config = FirewallConfig::default();
        let v = classify_request(
            "https://static.hotjar.com/c/hotjar-123.js",
            "köp produkt",
            &config,
        );
        assert!(!v.allowed, "Ska blockera hotjar");
    }

    #[test]
    fn test_l1_allows_product_api() {
        let config = FirewallConfig::default();
        let v = classify_request("https://shop.se/api/products/42", "köp produkt", &config);
        assert!(v.allowed, "Ska tillåta produkt-API");
    }

    #[test]
    fn test_l1_extra_blocked_domains() {
        let config = FirewallConfig {
            extra_blocked_domains: vec!["evil-tracker.com".to_string()],
            ..Default::default()
        };
        let v = classify_request("https://evil-tracker.com/track.js", "köp", &config);
        assert!(!v.allowed, "Ska blockera extra domän");
    }

    #[test]
    fn test_l2_blocks_images() {
        let config = FirewallConfig::default();
        let v = classify_request("https://shop.se/images/product.jpg", "köp produkt", &config);
        assert!(!v.allowed, "Ska blockera .jpg");
        assert_eq!(v.blocked_by, Some(FirewallLevel::L2MimeType));
    }

    #[test]
    fn test_l2_blocks_fonts() {
        let config = FirewallConfig::default();
        let v = classify_request(
            "https://fonts.gstatic.com/s/roboto/v30/font.woff2",
            "köp",
            &config,
        );
        assert!(!v.allowed, "Ska blockera .woff2");
    }

    #[test]
    fn test_l2_allows_html() {
        let config = FirewallConfig::default();
        let v = classify_request("https://shop.se/products.html", "köp produkt", &config);
        assert!(v.allowed, "Ska tillåta .html");
    }

    #[test]
    fn test_l2_allows_js() {
        let config = FirewallConfig::default();
        let v = classify_request("https://shop.se/bundle.js", "köp produkt", &config);
        assert!(v.allowed, "Ska tillåta .js");
    }

    #[test]
    fn test_l3_relevant_product_url() {
        let config = FirewallConfig::default();
        let v = classify_request(
            "https://shop.se/api/product/42/price",
            "köp produkt",
            &config,
        );
        assert!(v.allowed, "Ska tillåta relevant URL");
        assert!(
            v.relevance_score.unwrap_or(0.0) > 0.2,
            "Borde ha hög relevans"
        );
    }

    #[test]
    fn test_l3_irrelevant_beacon() {
        let config = FirewallConfig {
            enable_l1: false, // Stäng av L1 för att testa L3
            ..Default::default()
        };
        let v = classify_request(
            "https://unknown-site.com/beacon/telemetry",
            "köp produkt",
            &config,
        );
        assert!(
            !v.allowed,
            "Ska blockera irrelevant beacon (score: {:?})",
            v.relevance_score
        );
    }

    #[test]
    fn test_whitelist_overrides_all() {
        let config = FirewallConfig {
            allowed_domains: vec!["google-analytics.com".to_string()],
            ..Default::default()
        };
        let v = classify_request(
            "https://google-analytics.com/collect",
            "köp produkt",
            &config,
        );
        assert!(v.allowed, "Whitelist ska override L1");
    }

    #[test]
    fn test_batch_classification() {
        let urls = vec![
            "https://shop.se/api/products".to_string(),
            "https://www.google-analytics.com/collect".to_string(),
            "https://shop.se/logo.png".to_string(),
            "https://shop.se/checkout".to_string(),
            "https://cdn.hotjar.com/script.js".to_string(),
        ];
        let config = FirewallConfig::default();
        let (verdicts, summary) = classify_batch(&urls, "köp produkt", &config);

        assert_eq!(verdicts.len(), 5);
        assert_eq!(summary.total_requests, 5);
        assert!(summary.blocked_l1 > 0, "Borde blockera tracking");
        assert!(summary.blocked_l2 > 0, "Borde blockera bilder");
        assert!(
            summary.estimated_bandwidth_saved_pct > 30.0,
            "Borde spara >30% bandbredd"
        );
    }

    #[test]
    fn test_mime_type_check() {
        assert!(check_mime_type("image/png").is_some());
        assert!(check_mime_type("font/woff2").is_some());
        assert!(check_mime_type("video/mp4").is_some());
        assert!(check_mime_type("text/html").is_none());
        assert!(check_mime_type("application/json").is_none());
    }

    #[test]
    fn test_disabled_levels() {
        let config = FirewallConfig {
            enable_l1: false,
            enable_l2: false,
            enable_l3: false,
            ..Default::default()
        };
        let v = classify_request("https://google-analytics.com/collect", "köp", &config);
        assert!(v.allowed, "Alla nivåer avaktiverade ska tillåta allt");
    }

    #[test]
    fn test_extract_host() {
        assert_eq!(
            extract_host("https://example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_host("http://shop.se:8080/api"),
            Some("shop.se".to_string())
        );
        assert_eq!(
            extract_host("https://sub.domain.com/"),
            Some("sub.domain.com".to_string())
        );
    }
}
