/// Tier 2 CDP: TieredBackend — intelligent screenshot-eskalering
///
/// Arkitektur:
///   Tier 1 (Blitz): Ren Rust HTML/CSS-rendering, ~10-50ms, noll Chrome
///   Tier 2 (CDP):   Chrome DevTools Protocol, ~60-80ms warm, JS-kapabel
///
/// TieredBackend provar Blitz först. Om Blitz misslyckas (blank bild,
/// JS-renderat innehåll) eskaleras till CDP. XHR-interceptorn kan
/// skippa Blitz direkt via TierHint::RequiresJs.
///
/// Chrome startar bara om CDP-requests faktiskt inträffar.
/// De flesta agent-sessioner slutar utan att Chrome startats.
use serde::{Deserialize, Serialize};
use std::time::Instant;

// Global Chrome browser — initieras i bakgrunden vid serverstart
#[cfg(feature = "cdp")]
static CDP_BROWSER: std::sync::OnceLock<std::sync::Mutex<headless_chrome::Browser>> =
    std::sync::OnceLock::new();

// Signalerar att bakgrunds-warmup har startats (undvik dubbla starter)
#[cfg(feature = "cdp")]
static CDP_WARMUP_STARTED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Starta Chrome i en bakgrundstråd — anropa vid serverstart.
///
/// Servern startar direkt och börjar lyssna medan Chrome
/// initieras parallellt. Första CDP-request väntar bara om
/// Chrome inte hunnit klart (sällan, ~1-2s).
#[cfg(feature = "cdp")]
pub fn warmup_cdp_background() {
    // Undvik att starta flera gånger
    if CDP_WARMUP_STARTED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(|| {
        eprintln!("CDP warmup: starting Chrome in background...");
        match init_chrome_browser() {
            Ok(_) => eprintln!("CDP warmup: Chrome ready"),
            Err(e) => eprintln!("CDP warmup: Chrome failed: {e}"),
        }
    });
}

#[cfg(not(feature = "cdp"))]
pub fn warmup_cdp_background() {
    // CDP inte kompilerad — noop
}

/// Intern: starta Chrome och sätt globalt
#[cfg(feature = "cdp")]
fn init_chrome_browser() -> Result<(), String> {
    if CDP_BROWSER.get().is_some() {
        return Ok(());
    }
    use headless_chrome::{Browser, LaunchOptions};
    let options = LaunchOptions {
        headless: true,
        sandbox: false,
        window_size: Some((1280, 900)),
        args: vec![
            std::ffi::OsStr::new("--no-sandbox"),
            std::ffi::OsStr::new("--disable-gpu"),
            std::ffi::OsStr::new("--disable-dev-shm-usage"),
            std::ffi::OsStr::new("--disable-software-rasterizer"),
            std::ffi::OsStr::new("--disable-extensions"),
        ],
        ..LaunchOptions::default()
    };
    let browser = Browser::new(options).map_err(|e| format!("Chrome start failed: {e}"))?;
    let _ = CDP_BROWSER.set(std::sync::Mutex::new(browser));
    Ok(())
}

/// Hämta Chrome-browser (väntar om warmup pågår, startar om ej startad)
#[cfg(feature = "cdp")]
fn get_or_init_browser() -> Result<&'static std::sync::Mutex<headless_chrome::Browser>, String> {
    // Snabbväg: redan klar
    if let Some(browser) = CDP_BROWSER.get() {
        return Ok(browser);
    }

    // Warmup pågår — vänta max 15s
    let deadline = Instant::now() + std::time::Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Some(browser) = CDP_BROWSER.get() {
            return Ok(browser);
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    // Timeout — försök starta själv som fallback
    init_chrome_browser()?;
    CDP_BROWSER
        .get()
        .ok_or_else(|| "CDP browser init failed after timeout".to_string())
}

// ─── Typer ──────────────────────────────────────────────────────────────────

/// Vilken rendering-nivå som ska användas
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum TierHint {
    /// Prova Blitz först (default)
    #[default]
    TryBlitzFirst,
    /// XHR-data indikerar JavaScript-rendering → skippa Blitz direkt
    RequiresJs {
        /// Anledning till att JS krävs (loggning/debugging)
        reason: String,
    },
}

/// Vilken tier som faktiskt levererade screenshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScreenshotTier {
    /// Tier 1 — snabb, in-process (Blitz)
    Blitz,
    /// Tier 2 — fullständig, JS-kapabel (CDP/Chrome)
    Cdp,
    /// Tier 2 ej tillgänglig — Blitz-only mode
    BlitzOnly,
}

/// Screenshot-begäran med tier-hint
#[derive(Debug, Clone)]
pub struct ScreenshotRequest {
    /// URL att rendera (krävs för Blitz, kan vara None för raw HTML)
    pub url: String,
    /// Raw HTML (om redan hämtad)
    pub html: Option<String>,
    /// Viewport-bredd
    pub width: u32,
    /// Viewport-höjd
    pub height: u32,
    /// Snabb rendering (skippa externa resurser)
    pub fast_render: bool,
    /// Hint om vilken tier som troligen behövs
    pub tier_hint: TierHint,
    /// Agentens mål (för loggning)
    pub goal: String,
}

/// Screenshot-resultat med metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResult {
    /// PNG-bytes
    #[serde(skip)]
    pub png_bytes: Vec<u8>,
    /// Bredd i pixlar
    pub width: u32,
    /// Höjd i pixlar
    pub height: u32,
    /// Latens i millisekunder
    pub latency_ms: u64,
    /// Storlek i bytes
    pub size_bytes: usize,
    /// Vilken tier som faktiskt levererade
    pub tier_used: ScreenshotTier,
    /// Om eskalering skedde, varför
    #[serde(skip_serializing_if = "Option::is_none")]
    pub escalation_reason: Option<String>,
}

/// Statistik för tier-fördelning i produktion
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TierStats {
    /// Antal requests hanterade av Blitz
    pub blitz_count: u64,
    /// Antal requests hanterade av CDP
    pub cdp_count: u64,
    /// Antal Blitz-misslyckanden som eskalerades till CDP
    pub escalation_count: u64,
    /// Antal requests som skippade Blitz (TierHint::RequiresJs)
    pub skip_blitz_count: u64,
    /// Genomsnittlig Blitz-latens (ms)
    pub avg_blitz_latency_ms: f64,
    /// Genomsnittlig CDP-latens (ms)
    pub avg_cdp_latency_ms: f64,
}

// ─── JS-indikatorer ──────────────────────────────────────────────────────────

/// Kända indikatorer som tyder på att sidan kräver JS-rendering
const JS_CHART_INDICATORS: &[&str] = &[
    "chartType",
    "canvasId",
    "plotly",
    "vega",
    "datasets",
    "echarts",
    "highcharts",
    "d3.select",
    "new Chart(",
    "Plotly.newPlot",
    "google.charts",
];

/// Kända SPA-ramverksmarkörer som tyder på JS-renderat innehåll
const SPA_INDICATORS: &[&str] = &[
    "react-root",
    "__next",
    "__nuxt",
    "ng-app",
    "data-reactroot",
    "id=\"app\"",
    "id=\"root\"",
    "<noscript>",
];

// ─── TierHint-bestämning ──────────────────────────────────────────────────

/// Analysera XHR-captures och HTML för att bestämma TierHint
///
/// Om XHR-innehåll eller HTML-markörer indikerar JS-rendering → RequiresJs
/// Annars → TryBlitzFirst
pub fn determine_tier_hint(html: &str, xhr_bodies: &[&str]) -> TierHint {
    // Kolla XHR-kroppar efter chart/canvas-indikatorer
    for body in xhr_bodies {
        for indicator in JS_CHART_INDICATORS {
            if body.contains(indicator) {
                return TierHint::RequiresJs {
                    reason: format!("XHR body contains JS indicator: {}", indicator),
                };
            }
        }
    }

    // Kolla HTML efter JS chart/canvas-indikatorer (t.ex. inline <script>)
    for indicator in JS_CHART_INDICATORS {
        if html.contains(indicator) {
            return TierHint::RequiresJs {
                reason: format!("HTML contains JS indicator: {}", indicator),
            };
        }
    }

    // Kolla HTML efter SPA-ramverksmarkörer med tom body
    let html_lower = html.to_lowercase();
    let has_spa_marker = SPA_INDICATORS
        .iter()
        .any(|marker| html_lower.contains(marker));

    if has_spa_marker {
        // Kolla om body-elementet är (nästan) tomt → allt renderas av JS
        let body_content = extract_body_content(html);
        if body_content.len() < 200 {
            return TierHint::RequiresJs {
                reason: "SPA framework detected with minimal body content".to_string(),
            };
        }
    }

    TierHint::TryBlitzFirst
}

/// Extrahera textinnehållet mellan <body> och </body>
fn extract_body_content(html: &str) -> String {
    let lower = html.to_lowercase();
    let start = lower
        .find("<body")
        .and_then(|i| lower[i..].find('>').map(|j| i + j + 1));
    let end = lower.find("</body>");
    match (start, end) {
        (Some(s), Some(e)) if e > s => {
            // Ta bort script/style-taggar och räkna text
            let body = &html[s..e];
            let stripped = strip_tags(body);
            stripped.trim().to_string()
        }
        _ => String::new(),
    }
}

/// Enkel tag-strippare (tar bort allt mellan < och >)
fn strip_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            result.push(ch);
        }
    }
    result
}

// ─── TieredBackend ────────────────────────────────────────────────────────

/// TieredBackend — konfiguration och tillstånd
pub struct TieredBackend {
    /// Om CDP är tillgänglig
    cdp_available: bool,
    /// Statistik
    stats: std::sync::Mutex<TierStats>,
}

impl TieredBackend {
    /// Skapa ny TieredBackend
    ///
    /// `cdp_available`: true om Chrome/CDP finns i miljön
    pub fn new(cdp_available: bool) -> Self {
        TieredBackend {
            cdp_available,
            stats: std::sync::Mutex::new(TierStats::default()),
        }
    }

    /// Kör screenshot med intelligent tier-val
    ///
    /// 1. Om TierHint::RequiresJs → CDP direkt (om tillgänglig)
    /// 2. Annars: Blitz först → validera → eskalera vid behov
    pub fn screenshot(&self, req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        // Om XHR/HTML-hints indikerar JS → skippa Blitz
        if matches!(req.tier_hint, TierHint::RequiresJs { .. }) && self.cdp_available {
            self.update_stats_skip_blitz();
            return self.screenshot_cdp(req);
        }

        // Tier 1: Blitz
        let blitz_start = Instant::now();
        match self.screenshot_blitz(req) {
            Ok(result) => {
                // Kvalitetskontroll
                if self.blitz_result_is_valid(&result) {
                    self.update_stats_blitz(blitz_start.elapsed().as_millis() as f64);
                    return Ok(result);
                }
                // Blitz-resultat ogiltigt → eskalera
                if self.cdp_available {
                    self.update_stats_escalation();
                    let mut cdp_result = self.screenshot_cdp(req)?;
                    cdp_result.escalation_reason =
                        Some("Blitz render invalid (blank/too small)".to_string());
                    return Ok(cdp_result);
                }
                // Ingen CDP → returnera Blitz-resultat ändå
                Ok(result)
            }
            Err(e) => {
                if self.cdp_available {
                    self.update_stats_escalation();
                    let mut cdp_result = self.screenshot_cdp(req)?;
                    cdp_result.escalation_reason = Some(format!("Blitz failed: {}", e));
                    return Ok(cdp_result);
                }
                Err(format!("Blitz failed and CDP unavailable: {}", e))
            }
        }
    }

    /// Blitz-rendering (Tier 1)
    #[cfg(feature = "blitz")]
    fn screenshot_blitz(&self, req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        let start = Instant::now();

        let html = match &req.html {
            Some(h) => h.clone(),
            None => return Err("Blitz kräver HTML (använd fetch först)".to_string()),
        };

        let png_bytes =
            crate::render_html_to_png(&html, &req.url, req.width, req.height, req.fast_render)?;

        let size_bytes = png_bytes.len();
        Ok(ScreenshotResult {
            png_bytes,
            width: req.width,
            height: req.height,
            latency_ms: start.elapsed().as_millis() as u64,
            size_bytes,
            tier_used: ScreenshotTier::Blitz,
            escalation_reason: None,
        })
    }

    #[cfg(not(feature = "blitz"))]
    fn screenshot_blitz(&self, _req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        Err("Blitz feature inte aktiverad".to_string())
    }

    /// CDP-rendering (Tier 2) — headless Chrome via headless_chrome crate
    ///
    /// Lazy Chrome-init: browser startas vid första anropet och återanvänds.
    /// Om HTML finns i request: sätter Page.setDocumentContent direkt (undviker nätverksnavigering).
    /// Annars: navigerar till URL och väntar på page load.
    #[cfg(feature = "cdp")]
    fn screenshot_cdp(&self, req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        let start = Instant::now();

        let browser_mutex = get_or_init_browser().map_err(|e| format!("CDP browser init: {e}"))?;
        let browser = browser_mutex
            .lock()
            .map_err(|e| format!("CDP browser lock: {e}"))?;

        // Skapa ny tab och navigera
        let tab = browser.new_tab().map_err(|e| format!("CDP new tab: {e}"))?;

        // Sätt viewport-storlek
        tab.set_bounds(headless_chrome::types::Bounds::Normal {
            left: None,
            top: None,
            width: Some(req.width as f64),
            height: Some(req.height as f64),
        })
        .map_err(|e| format!("CDP set bounds: {e}"))?;

        if let Some(html) = &req.html {
            // HTML redan tillgänglig — injicera direkt via Page.setDocumentContent
            // Navigera först till about:blank för att få ett giltigt frame
            tab.navigate_to("about:blank")
                .map_err(|e| format!("CDP navigate blank: {e}"))?;
            tab.wait_until_navigated()
                .map_err(|e| format!("CDP wait blank: {e}"))?;

            // Hämta main frame ID
            let frame_tree = tab
                .call_method(headless_chrome::protocol::cdp::Page::GetFrameTree(None))
                .map_err(|e| format!("CDP get frame tree: {e}"))?;
            let frame_id = frame_tree.frame_tree.frame.id.clone();

            // Sätt HTML-innehåll direkt
            tab.call_method(headless_chrome::protocol::cdp::Page::SetDocumentContent {
                frame_id,
                html: html.clone(),
            })
            .map_err(|e| format!("CDP set document content: {e}"))?;

            // Kort delay så Chrome hinner layouta
            std::thread::sleep(std::time::Duration::from_millis(100));
        } else {
            // Ingen HTML — navigera till URL
            tab.navigate_to(&req.url)
                .map_err(|e| format!("CDP navigate: {e}"))?;

            // Vänta på page load (max 10s)
            tab.wait_until_navigated()
                .map_err(|e| format!("CDP wait: {e}"))?;
        }

        // Ta viewport screenshot som PNG
        let png_bytes = tab
            .capture_screenshot(
                headless_chrome::protocol::cdp::Page::CaptureScreenshotFormatOption::Png,
                None, // quality
                None, // clip (hela viewport)
                true, // from_surface
            )
            .map_err(|e| format!("CDP screenshot: {e}"))?;

        // Stäng tab för att frigöra resurser
        let _ = tab.close(true);

        let latency_ms = start.elapsed().as_millis() as u64;
        let size_bytes = png_bytes.len();

        // Uppdatera CDP-statistik
        if let Ok(mut s) = self.stats.lock() {
            let total = s.cdp_count as f64 * s.avg_cdp_latency_ms + latency_ms as f64;
            s.cdp_count += 1;
            s.avg_cdp_latency_ms = total / s.cdp_count as f64;
        }

        Ok(ScreenshotResult {
            png_bytes,
            width: req.width,
            height: req.height,
            latency_ms,
            size_bytes,
            tier_used: ScreenshotTier::Cdp,
            escalation_reason: None,
        })
    }

    #[cfg(not(feature = "cdp"))]
    fn screenshot_cdp(&self, _req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        Err("CDP feature inte aktiverad. Kompilera med --features cdp".to_string())
    }

    /// Avgör om Blitz-resultatet är giltigt
    fn blitz_result_is_valid(&self, result: &ScreenshotResult) -> bool {
        // Under 500 bytes = blank rendering (typisk blank PNG header ~67 bytes)
        if result.size_bytes < 500 {
            return false;
        }

        // Extremt liten = troligen rendering-fel
        if result.width == 0 || result.height == 0 {
            return false;
        }

        true
    }

    /// Hämta tier-statistik
    pub fn stats(&self) -> TierStats {
        self.stats.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update_stats_blitz(&self, latency_ms: f64) {
        if let Ok(mut s) = self.stats.lock() {
            let total = s.blitz_count as f64 * s.avg_blitz_latency_ms + latency_ms;
            s.blitz_count += 1;
            s.avg_blitz_latency_ms = total / s.blitz_count as f64;
        }
    }

    fn update_stats_escalation(&self) {
        if let Ok(mut s) = self.stats.lock() {
            s.escalation_count += 1;
        }
    }

    fn update_stats_skip_blitz(&self) {
        if let Ok(mut s) = self.stats.lock() {
            s.skip_blitz_count += 1;
        }
    }
}

impl Default for TieredBackend {
    fn default() -> Self {
        // Auto-detect CDP: feature-flagga + kolla att Chrome-binär finns
        let cdp_available = if cfg!(feature = "cdp") {
            // Verifiera att Chrome faktiskt finns i PATH
            std::process::Command::new("chromium")
                .arg("--version")
                .output()
                .is_ok()
                || std::process::Command::new("chromium-browser")
                    .arg("--version")
                    .output()
                    .is_ok()
                || std::process::Command::new("google-chrome")
                    .arg("--version")
                    .output()
                    .is_ok()
        } else {
            false
        };
        TieredBackend::new(cdp_available)
    }
}

// ─── Tester ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_hint_default() {
        let hint = TierHint::default();
        assert_eq!(
            hint,
            TierHint::TryBlitzFirst,
            "Default tier hint borde vara TryBlitzFirst"
        );
    }

    #[test]
    fn test_tier_hint_requires_js() {
        let hint = TierHint::RequiresJs {
            reason: "chart detected".to_string(),
        };
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "Borde vara RequiresJs"
        );
    }

    #[test]
    fn test_determine_tier_hint_static_html() {
        let html = r##"<html><body><h1>Hello</h1><p>Content here</p></body></html>"##;
        let hint = determine_tier_hint(html, &[]);
        assert_eq!(
            hint,
            TierHint::TryBlitzFirst,
            "Statisk HTML borde ge TryBlitzFirst"
        );
    }

    #[test]
    fn test_determine_tier_hint_chart_xhr() {
        let xhr_body = r#"{"chartType": "line", "datasets": [{"data": [1,2,3]}]}"#;
        let hint = determine_tier_hint("<html><body></body></html>", &[xhr_body]);
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "XHR med chartType borde ge RequiresJs"
        );
    }

    #[test]
    fn test_determine_tier_hint_spa_empty_body() {
        let html = r#"<html><head></head><body><div id="root"></div><script src="bundle.js"></script></body></html>"#;
        let hint = determine_tier_hint(html, &[]);
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "SPA med tom body borde ge RequiresJs"
        );
    }

    #[test]
    fn test_determine_tier_hint_spa_with_content() {
        // React-app med pre-renderat SSR-innehåll → Blitz klarar
        let long_content = "a".repeat(500);
        let html = format!(
            r#"<html><body><div data-reactroot>{}</div></body></html>"#,
            long_content
        );
        let hint = determine_tier_hint(&html, &[]);
        assert_eq!(
            hint,
            TierHint::TryBlitzFirst,
            "SSR-renderad SPA med innehåll borde ge TryBlitzFirst"
        );
    }

    #[test]
    fn test_determine_tier_hint_plotly() {
        let xhr = r#"Plotly.newPlot('myDiv', data, layout)"#;
        let hint = determine_tier_hint("<html><body></body></html>", &[xhr]);
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "Plotly borde ge RequiresJs"
        );
    }

    #[test]
    fn test_determine_tier_hint_d3() {
        let xhr = r#"d3.select('#chart').append('svg')"#;
        let hint = determine_tier_hint("<html><body></body></html>", &[xhr]);
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "D3 borde ge RequiresJs"
        );
    }

    #[test]
    fn test_strip_tags() {
        assert_eq!(strip_tags("<b>bold</b>"), "bold");
        assert_eq!(strip_tags("<div class='x'>hello</div>"), "hello");
        assert_eq!(strip_tags("no tags"), "no tags");
        assert_eq!(strip_tags(""), "");
    }

    #[test]
    fn test_extract_body_content() {
        let html = r#"<html><head><title>T</title></head><body><h1>Hello</h1></body></html>"#;
        let body = extract_body_content(html);
        assert!(
            body.contains("Hello"),
            "Body borde innehålla 'Hello', fick: {}",
            body
        );
    }

    #[test]
    fn test_screenshot_tier_serde() {
        let tier = ScreenshotTier::Blitz;
        let json = serde_json::to_string(&tier).expect("Borde serialisera");
        assert!(json.contains("Blitz"), "JSON borde innehålla 'Blitz'");
    }

    #[test]
    fn test_tiered_backend_no_cdp() {
        let backend = TieredBackend::new(false);
        assert!(!backend.cdp_available, "CDP borde inte vara tillgänglig");
    }

    #[test]
    fn test_tiered_backend_stats_default() {
        let backend = TieredBackend::new(false);
        let stats = backend.stats();
        assert_eq!(stats.blitz_count, 0, "Borde starta med 0 blitz");
        assert_eq!(stats.cdp_count, 0, "Borde starta med 0 cdp");
    }

    #[test]
    fn test_blitz_result_invalid_too_small() {
        let backend = TieredBackend::new(false);
        let result = ScreenshotResult {
            png_bytes: vec![0; 100],
            width: 1280,
            height: 800,
            latency_ms: 50,
            size_bytes: 100,
            tier_used: ScreenshotTier::Blitz,
            escalation_reason: None,
        };
        assert!(
            !backend.blitz_result_is_valid(&result),
            "100 bytes borde vara ogiltigt (blank rendering)"
        );
    }

    #[test]
    fn test_blitz_result_valid() {
        let backend = TieredBackend::new(false);
        let result = ScreenshotResult {
            png_bytes: vec![0; 50000],
            width: 1280,
            height: 800,
            latency_ms: 50,
            size_bytes: 50000,
            tier_used: ScreenshotTier::Blitz,
            escalation_reason: None,
        };
        assert!(
            backend.blitz_result_is_valid(&result),
            "50KB borde vara giltigt"
        );
    }

    #[test]
    fn test_blitz_result_zero_dimensions() {
        let backend = TieredBackend::new(false);
        let result = ScreenshotResult {
            png_bytes: vec![0; 5000],
            width: 0,
            height: 0,
            latency_ms: 50,
            size_bytes: 5000,
            tier_used: ScreenshotTier::Blitz,
            escalation_reason: None,
        };
        assert!(
            !backend.blitz_result_is_valid(&result),
            "0x0 dimensioner borde vara ogiltigt"
        );
    }

    #[test]
    fn test_screenshot_result_serde_roundtrip() {
        let result = ScreenshotResult {
            png_bytes: vec![], // Skippas i serde
            width: 1280,
            height: 800,
            latency_ms: 42,
            size_bytes: 50000,
            tier_used: ScreenshotTier::Blitz,
            escalation_reason: None,
        };
        let json = serde_json::to_string(&result).expect("Borde serialisera");
        let restored: ScreenshotResult = serde_json::from_str(&json).expect("Borde deserialisera");
        assert_eq!(restored.width, 1280);
        assert_eq!(restored.tier_used, ScreenshotTier::Blitz);
        assert_eq!(restored.latency_ms, 42);
    }

    #[test]
    fn test_tier_stats_serde() {
        let stats = TierStats {
            blitz_count: 100,
            cdp_count: 35,
            escalation_count: 5,
            skip_blitz_count: 10,
            avg_blitz_latency_ms: 12.5,
            avg_cdp_latency_ms: 72.3,
        };
        let json = serde_json::to_string(&stats).expect("Borde serialisera");
        let restored: TierStats = serde_json::from_str(&json).expect("Borde deserialisera");
        assert_eq!(restored.blitz_count, 100);
        assert_eq!(restored.cdp_count, 35);
    }

    #[test]
    fn test_js_indicators_complete() {
        // Verifiera att alla kända chart-libraries finns
        assert!(
            JS_CHART_INDICATORS.contains(&"plotly"),
            "Borde innehålla plotly"
        );
        assert!(
            JS_CHART_INDICATORS.contains(&"echarts"),
            "Borde innehålla echarts"
        );
        assert!(
            JS_CHART_INDICATORS.contains(&"d3.select"),
            "Borde innehålla d3"
        );
        assert!(
            JS_CHART_INDICATORS.len() >= 8,
            "Borde ha minst 8 JS-indikatorer"
        );
    }

    #[test]
    fn test_spa_indicators_complete() {
        assert!(
            SPA_INDICATORS.contains(&"react-root"),
            "Borde innehålla react-root"
        );
        assert!(
            SPA_INDICATORS.contains(&"__next"),
            "Borde innehålla __next (Next.js)"
        );
        assert!(
            SPA_INDICATORS.contains(&"__nuxt"),
            "Borde innehålla __nuxt (Nuxt)"
        );
    }

    #[test]
    fn test_screenshot_request_defaults() {
        let req = ScreenshotRequest {
            url: "https://example.com".to_string(),
            html: None,
            width: 1280,
            height: 800,
            fast_render: true,
            tier_hint: TierHint::default(),
            goal: "test".to_string(),
        };
        assert_eq!(req.width, 1280);
        assert!(req.fast_render);
        assert_eq!(req.tier_hint, TierHint::TryBlitzFirst);
    }
}
