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
/// Chrome startas i bakgrunden vid serverstart (warmup_cdp_background).
/// Första CDP-request väntar bara om Chrome inte hunnit klart (~1-2s).
use serde::{Deserialize, Serialize};
use std::time::Instant;

// Global Chrome browser — kan omstartas vid WebSocket-disconnect
#[cfg(feature = "cdp")]
static CDP_BROWSER: std::sync::OnceLock<std::sync::Mutex<Option<headless_chrome::Browser>>> =
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
            Ok(_) => {
                eprintln!("CDP warmup: Chrome ready");
                notify_cdp_ready();
            }
            Err(e) => eprintln!("CDP warmup: Chrome failed: {e}"),
        }
    });
}

/// Callback efter CDP-warmup: sätt cdp_available=true på global backend
#[cfg(feature = "cdp")]
fn notify_cdp_ready() {
    // Om GLOBAL_TIERED_BACKEND i lib.rs redan initierats, uppdatera den.
    // Annars kommer default() se CDP_BROWSER.get().is_some() == true.
    // Vi exponerar en publik funktion som lib.rs kan koppla in.
    if let Some(cb) = CDP_READY_CALLBACK.get() {
        cb();
    }
}

/// Callback-register för CDP ready notification
#[cfg(feature = "cdp")]
static CDP_READY_CALLBACK: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Registrera callback som anropas när CDP är redo
#[cfg(feature = "cdp")]
pub fn on_cdp_ready(f: impl Fn() + Send + Sync + 'static) {
    let _ = CDP_READY_CALLBACK.set(Box::new(f));
}

#[cfg(feature = "cdp")]
use std::sync::OnceLock;

#[cfg(not(feature = "cdp"))]
pub fn warmup_cdp_background() {
    // CDP inte kompilerad — noop
}

#[cfg(not(feature = "cdp"))]
pub fn on_cdp_ready(_f: impl Fn() + Send + Sync + 'static) {
    // CDP inte kompilerad — noop
}

/// Skapa Chrome LaunchOptions (återanvänds av init + restart)
#[cfg(feature = "cdp")]
fn chrome_launch_options() -> headless_chrome::LaunchOptions<'static> {
    headless_chrome::LaunchOptions {
        headless: true,
        sandbox: false,
        window_size: Some((1280, 900)),
        args: vec![
            std::ffi::OsStr::new("--no-sandbox"),
            std::ffi::OsStr::new("--disable-gpu"),
            std::ffi::OsStr::new("--disable-dev-shm-usage"),
            std::ffi::OsStr::new("--disable-software-rasterizer"),
            std::ffi::OsStr::new("--disable-extensions"),
            // BUG-5 fix: Dölj automation-flaggor för bot-detection (Cloudflare m.fl.)
            std::ffi::OsStr::new("--disable-blink-features=AutomationControlled"),
        ],
        ..headless_chrome::LaunchOptions::default()
    }
}

/// Intern: starta Chrome och sätt globalt
#[cfg(feature = "cdp")]
fn init_chrome_browser() -> Result<(), String> {
    let browser = headless_chrome::Browser::new(chrome_launch_options())
        .map_err(|e| format!("Chrome start failed: {e}"))?;
    let _ = CDP_BROWSER.get_or_init(|| std::sync::Mutex::new(Some(browser)));
    Ok(())
}

/// Starta om Chrome efter WebSocket-disconnect
#[cfg(feature = "cdp")]
fn restart_chrome_browser() -> Result<(), String> {
    eprintln!("CDP: restarting Chrome (WebSocket disconnected)...");
    let browser = headless_chrome::Browser::new(chrome_launch_options())
        .map_err(|e| format!("Chrome restart failed: {e}"))?;
    if let Some(mutex) = CDP_BROWSER.get() {
        if let Ok(mut guard) = mutex.lock() {
            *guard = Some(browser);
            eprintln!("CDP: Chrome restarted successfully");
            return Ok(());
        }
    }
    Err("CDP_BROWSER mutex unavailable".to_string())
}

/// Hämta Chrome-browser (startar Chrome lazy vid första anrop)
#[cfg(feature = "cdp")]
fn get_or_init_browser(
) -> Result<&'static std::sync::Mutex<Option<headless_chrome::Browser>>, String> {
    // Snabbväg: redan klar
    if let Some(browser) = CDP_BROWSER.get() {
        return Ok(browser);
    }

    // Lazy start: Chrome startas här vid första CDP-request
    eprintln!("CDP: starting Chrome on first use...");
    init_chrome_browser()?;
    notify_cdp_ready();
    CDP_BROWSER
        .get()
        .ok_or_else(|| "CDP browser init failed".to_string())
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

/// Kända JS-ramverk/chart-bibliotek i <script src="..."> URLs
const SCRIPT_SRC_JS_INDICATORS: &[&str] = &[
    "react",
    "vue",
    "angular",
    "next",
    "nuxt",
    "svelte",
    "plotly",
    "chart.js",
    "chartjs",
    "d3.js",
    "d3.min.js",
    "highcharts",
    "echarts",
    "tradingview",
    "lightweight-charts",
    "apex",
    "amcharts",
    "canvasjs",
];

/// Kända SPA-domäner som alltid kräver JS-rendering
const KNOWN_SPA_DOMAINS: &[&str] = &[
    "tradingview.com",
    "plotly.com",
    "app.powerbi.com",
    "datastudio.google.com",
    "grafana",
    "kibana",
    "vercel.app",
    "netlify.app",
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

    let html_lower = html.to_lowercase();

    // BUG-2 fix: Scanna <script src="..."> efter kända JS-ramverk/chart-bibliotek.
    // Löser hönan-och-ägg-problemet: Blitz kör inte JS → ingen XHR → ingen tier-hint.
    // Statisk analys av script-källor ger oss hinten utan att köra JS.
    if let Some(reason) = detect_js_framework_in_script_src(&html_lower) {
        return TierHint::RequiresJs { reason };
    }

    // Kolla HTML efter SPA-ramverksmarkörer med tom body
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

/// Analysera URL för kända SPA-domäner som alltid kräver JS
pub fn determine_tier_hint_with_url(html: &str, xhr_bodies: &[&str], url: &str) -> TierHint {
    // Kolla URL mot kända SPA-domäner
    let url_lower = url.to_lowercase();
    for domain in KNOWN_SPA_DOMAINS {
        if url_lower.contains(domain) {
            return TierHint::RequiresJs {
                reason: format!("Known SPA domain: {}", domain),
            };
        }
    }

    // Fallback till vanlig HTML-analys
    determine_tier_hint(html, xhr_bodies)
}

/// Scanna <script src="..."> efter kända JS-ramverk
fn detect_js_framework_in_script_src(html_lower: &str) -> Option<String> {
    let mut pos = 0;
    while let Some(script_start) = html_lower[pos..].find("<script") {
        let abs_start = pos + script_start;
        let tag_end = match html_lower[abs_start..].find('>') {
            Some(p) => abs_start + p,
            None => break,
        };
        let tag = &html_lower[abs_start..tag_end];

        // Extrahera src-attribut
        if let Some(src_start) = tag.find("src=") {
            let after_src = &tag[src_start + 4..];
            let quote = if after_src.starts_with('"') {
                '"'
            } else if after_src.starts_with('\'') {
                '\''
            } else {
                pos = tag_end + 1;
                continue;
            };
            let src_value_start = 1; // Skip quote
            if let Some(src_end) = after_src[src_value_start..].find(quote) {
                let src_value = &after_src[src_value_start..src_value_start + src_end];
                for indicator in SCRIPT_SRC_JS_INDICATORS {
                    if src_value.contains(indicator) {
                        return Some(format!(
                            "Script src contains JS framework: {} (src={})",
                            indicator, src_value
                        ));
                    }
                }
            }
        }

        pos = tag_end + 1;
    }
    None
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
    /// Om CDP är tillgänglig (AtomicBool för att kunna uppdateras efter warmup)
    cdp_available: std::sync::atomic::AtomicBool,
    /// Statistik
    stats: std::sync::Mutex<TierStats>,
}

// Kompileringsgaranti: TieredBackend måste vara Send + Sync för serverless scaling
const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<TieredBackend>();
};

impl TieredBackend {
    /// Skapa ny TieredBackend
    ///
    /// `cdp_available`: true om Chrome/CDP finns i miljön
    pub fn new(cdp_available: bool) -> Self {
        TieredBackend {
            cdp_available: std::sync::atomic::AtomicBool::new(cdp_available),
            stats: std::sync::Mutex::new(TierStats::default()),
        }
    }

    /// Sätt CDP-tillgänglighet (anropas efter warmup lyckats)
    pub fn set_cdp_available(&self, available: bool) {
        self.cdp_available
            .store(available, std::sync::atomic::Ordering::SeqCst);
    }

    /// Kontrollera om CDP är tillgänglig
    fn is_cdp_available(&self) -> bool {
        self.cdp_available.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Kör screenshot med intelligent tier-val
    ///
    /// 1. Om TierHint::RequiresJs → CDP direkt (om tillgänglig)
    /// 2. Annars: Blitz först → validera → eskalera vid behov
    pub fn screenshot(&self, req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        // Om XHR/HTML-hints indikerar JS → skippa Blitz
        if matches!(req.tier_hint, TierHint::RequiresJs { .. }) && self.is_cdp_available() {
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
                if self.is_cdp_available() {
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
                if self.is_cdp_available() {
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
    /// Om HTML finns i request: sätter Page.setDocumentContent direkt.
    /// Auto-restart: vid WebSocket-disconnect startas Chrome om automatiskt.
    #[cfg(feature = "cdp")]
    fn screenshot_cdp(&self, req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        // Försök en gång, om WebSocket-disconnect → starta om och försök igen
        match self.screenshot_cdp_inner(req) {
            Ok(result) => Ok(result),
            Err(e) if e.contains("connection is closed") || e.contains("disconnected") => {
                // Chrome-processen tappade WebSocket → starta om
                restart_chrome_browser()?;
                self.screenshot_cdp_inner(req)
            }
            Err(e) => Err(e),
        }
    }

    /// Intern CDP-rendering (anropas av screenshot_cdp med retry-logik)
    #[cfg(feature = "cdp")]
    fn screenshot_cdp_inner(&self, req: &ScreenshotRequest) -> Result<ScreenshotResult, String> {
        let start = Instant::now();

        let browser_mutex = get_or_init_browser().map_err(|e| format!("CDP browser init: {e}"))?;
        let guard = browser_mutex
            .lock()
            .map_err(|e| format!("CDP browser lock: {e}"))?;
        let browser = guard
            .as_ref()
            .ok_or_else(|| "CDP browser not initialized".to_string())?;

        // Skapa ny tab och navigera
        let tab = browser.new_tab().map_err(|e| format!("CDP new tab: {e}"))?;

        // BUG-5 fix: Dölj navigator.webdriver = true för att undvika
        // Cloudflare/bot-detection-blockering (inet.se, komplett.se m.fl.)
        let _ = tab.evaluate(
            "Object.defineProperty(navigator, 'webdriver', { get: () => undefined })",
            false,
        );

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

            // networkidle0-logik: vänta tills inga aktiva nätverksanrop pågår.
            // Injicerar JS-interceptor som räknar fetch()/XHR-anrop och pollar.
            // Fallback: max 5s total väntetid (undviker eviga lopar på long-poll/WebSocket-sidor).
            wait_for_network_idle(&tab, 500, 5000);

            // BUG-002 fix: Vänta på DOM-stabilitet efter network idle.
            // SPAs renderar ofta asynkront efter att XHR/fetch slutfört — DOM
            // ändras fortfarande (spinner → faktiskt innehåll). Pollar
            // document.body.innerHTML.length tills det stabiliserats.
            wait_for_dom_stable(&tab, 300, 3000);
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
    ///
    /// BUG-3 fix: Utökad validering — förutom size/dimension kollar vi även
    /// om PNG:en är nästan helt enhetlig (vit/svart bakgrund utan innehåll),
    /// vilket indikerar att Blitz inte kunde rendera sidan korrekt
    /// (t.ex. background-image saknas, CSS-renderat innehåll ej laddat).
    fn blitz_result_is_valid(&self, result: &ScreenshotResult) -> bool {
        // Under 500 bytes = blank rendering (typisk blank PNG header ~67 bytes)
        if result.size_bytes < 500 {
            return false;
        }

        // Extremt liten = troligen rendering-fel
        if result.width == 0 || result.height == 0 {
            return false;
        }

        // Heuristik: Väldigt liten PNG för den givna viewporten antyder
        // nästan tom rendering. En 1280x800 sida med rimligt innehåll
        // bör vara > 5KB. Under det indikerar blank/spinner-rendering.
        let pixels = result.width as usize * result.height as usize;
        let bytes_per_pixel = if pixels > 0 {
            result.size_bytes as f64 / pixels as f64
        } else {
            0.0
        };
        // Extremt lågt bytes/pixel = enhetlig bild (blank/single-color)
        // Typisk kompression: vit PNG ~0.01 bytes/px, innehållsrik ~0.1-1.0
        if bytes_per_pixel < 0.005 && result.size_bytes < 5000 {
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
        // Starta med cdp_available=false. warmup_cdp_background() sätter true
        // efter Chrome initierats. OnceLock + binärnamn-check funkar inte
        // i alla miljöer (Playwright, snap, flatpak etc.) — headless_chrome
        // har egen Chrome-detection som är mer robust.
        let cdp_available = if cfg!(feature = "cdp") {
            // Kolla om CDP_BROWSER redan initierats av warmup
            #[cfg(feature = "cdp")]
            {
                CDP_BROWSER.get().is_some()
            }
            #[cfg(not(feature = "cdp"))]
            {
                false
            }
        } else {
            false
        };
        TieredBackend::new(cdp_available)
    }
}

/// networkidle0: Vänta tills inga nätverksanrop pågår under `quiet_ms` millisekunder.
///
/// Injicerar JS som interceptar fetch() och XMLHttpRequest för att räkna aktiva requests.
/// Pollar var 100ms och returnerar när räknaren hållit sig på 0 under `quiet_ms`.
/// Avbryter efter `timeout_ms` totalt (undviker eviga WebSocket/long-poll-lopar).
#[cfg(feature = "cdp")]
fn wait_for_network_idle(
    tab: &std::sync::Arc<headless_chrome::Tab>,
    quiet_ms: u64,
    timeout_ms: u64,
) {
    // Injicera nätverksräknare — interceptar fetch() och XHR
    let inject_result = tab.evaluate(
        r#"
        (function() {
            if (window.__aether_net_count !== undefined) return;
            window.__aether_net_count = 0;

            // Intercepta fetch()
            const origFetch = window.fetch;
            window.fetch = function(...args) {
                window.__aether_net_count++;
                return origFetch.apply(this, args).finally(() => {
                    window.__aether_net_count = Math.max(0, window.__aether_net_count - 1);
                });
            };

            // Intercepta XMLHttpRequest
            const origOpen = XMLHttpRequest.prototype.open;
            const origSend = XMLHttpRequest.prototype.send;
            XMLHttpRequest.prototype.open = function(...args) {
                this.__aether_tracked = true;
                return origOpen.apply(this, args);
            };
            XMLHttpRequest.prototype.send = function(...args) {
                if (this.__aether_tracked) {
                    window.__aether_net_count++;
                    this.addEventListener('loadend', function() {
                        window.__aether_net_count = Math.max(0, window.__aether_net_count - 1);
                    }, { once: true });
                }
                return origSend.apply(this, args);
            };
        })()
        "#,
        false,
    );

    if inject_result.is_err() {
        // Om JS-injection misslyckades, fall tillbaka på fast delay
        std::thread::sleep(std::time::Duration::from_millis(quiet_ms));
        return;
    }

    let poll_interval = std::time::Duration::from_millis(100);
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let quiet_duration = std::time::Duration::from_millis(quiet_ms);
    let start = std::time::Instant::now();
    let mut idle_since: Option<std::time::Instant> = None;

    loop {
        if start.elapsed() >= timeout {
            break;
        }

        let count = tab
            .evaluate("window.__aether_net_count || 0", false)
            .ok()
            .and_then(|v| v.value)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if count == 0 {
            let now = std::time::Instant::now();
            if let Some(since) = idle_since {
                if now.duration_since(since) >= quiet_duration {
                    break; // Nätverket har varit idle tillräckligt länge
                }
            } else {
                idle_since = Some(now);
            }
        } else {
            idle_since = None; // Aktiva requests → nollställ
        }

        std::thread::sleep(poll_interval);
    }
}

/// BUG-002 fix: Vänta tills DOM stabiliserats (innerHTML.length slutar ändras).
///
/// SPAs renderar ofta asynkront efter nätverks-idle:
///   1. HTML laddas med tom <div id="root">
///   2. JS hämtar data via fetch/XHR (network idle väntar på detta)
///   3. React/Vue/Angular renderar DOM med data (~50-500ms efter network idle)
///
/// Denna funktion pollar `document.body.innerHTML.length` var 100ms.
/// Om längden inte ändrats under `stable_ms` → DOM är stabil.
/// Max `timeout_ms` total väntetid.
#[cfg(feature = "cdp")]
fn wait_for_dom_stable(
    tab: &std::sync::Arc<headless_chrome::Tab>,
    stable_ms: u64,
    timeout_ms: u64,
) {
    let poll_interval = std::time::Duration::from_millis(100);
    let timeout = std::time::Duration::from_millis(timeout_ms);
    let stable_duration = std::time::Duration::from_millis(stable_ms);
    let start = std::time::Instant::now();

    let mut last_length: i64 = -1;
    let mut stable_since: Option<std::time::Instant> = None;

    loop {
        if start.elapsed() >= timeout {
            break;
        }

        let current_length = tab
            .evaluate("document.body ? document.body.innerHTML.length : 0", false)
            .ok()
            .and_then(|v| v.value)
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        if current_length == last_length && current_length > 0 {
            let now = std::time::Instant::now();
            if let Some(since) = stable_since {
                if now.duration_since(since) >= stable_duration {
                    break; // DOM har varit stabil tillräckligt länge
                }
            } else {
                stable_since = Some(now);
            }
        } else {
            last_length = current_length;
            stable_since = None; // DOM ändrades → nollställ
        }

        std::thread::sleep(poll_interval);
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
        assert!(
            !backend.is_cdp_available(),
            "CDP borde inte vara tillgänglig"
        );
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

    // ─── BUG-2 tester: script src detection ────────────────────────────────

    #[test]
    fn test_determine_tier_hint_react_script_src() {
        let html = r##"<html><head>
            <script src="https://cdn.example.com/react.production.min.js"></script>
        </head><body><div id="root"></div></body></html>"##;
        let hint = determine_tier_hint(html, &[]);
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "React script src borde ge RequiresJs, fick: {:?}",
            hint
        );
    }

    #[test]
    fn test_determine_tier_hint_chartjs_script_src() {
        let html = r##"<html><head>
            <script src="/js/chart.js"></script>
        </head><body><canvas id="myChart"></canvas></body></html>"##;
        let hint = determine_tier_hint(html, &[]);
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "Chart.js script src borde ge RequiresJs"
        );
    }

    #[test]
    fn test_determine_tier_hint_tradingview_url() {
        let html = r##"<html><body><div id="app"></div></body></html>"##;
        let hint =
            determine_tier_hint_with_url(html, &[], "https://www.tradingview.com/chart/ABC123");
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "TradingView URL borde ge RequiresJs"
        );
    }

    #[test]
    fn test_determine_tier_hint_plotly_url() {
        let html = r##"<html><body><p>Charts</p></body></html>"##;
        let hint =
            determine_tier_hint_with_url(html, &[], "https://plotly.com/javascript/line-charts/");
        assert!(
            matches!(hint, TierHint::RequiresJs { .. }),
            "Plotly URL borde ge RequiresJs"
        );
    }

    #[test]
    fn test_determine_tier_hint_normal_url() {
        let html = r##"<html><body><h1>Normal sida</h1><p>Med mycket text</p></body></html>"##;
        let hint = determine_tier_hint_with_url(html, &[], "https://example.com/about");
        assert_eq!(
            hint,
            TierHint::TryBlitzFirst,
            "Normal URL borde ge TryBlitzFirst"
        );
    }

    #[test]
    fn test_detect_js_framework_d3() {
        let html = r##"<script src="https://d3js.org/d3.min.js"></script>"##;
        let result = detect_js_framework_in_script_src(&html.to_lowercase());
        assert!(result.is_some(), "Borde detektera d3.min.js i script src");
    }

    // ─── BUG-3 tester: förbättrad blank-detection ─────────────────────────

    #[test]
    fn test_blitz_result_blank_large_viewport() {
        let backend = TieredBackend::new(false);
        // En 1280x800 viewport som genererar en väldigt liten PNG = blank
        let result = ScreenshotResult {
            png_bytes: vec![0; 2000],
            width: 1280,
            height: 800,
            latency_ms: 50,
            size_bytes: 2000,
            tier_used: ScreenshotTier::Blitz,
            escalation_reason: None,
        };
        assert!(
            !backend.blitz_result_is_valid(&result),
            "Liten PNG för stor viewport borde vara ogiltigt (blank rendering)"
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
