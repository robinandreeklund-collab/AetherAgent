/// Progressive Escalation — Fas 17.4
///
/// Intelligent tier-val som väljer minsta möjliga exekveringsnivå per sida.
/// Kör aldrig mer arbete än nödvändigt: hydration-data → 0 ms JS,
/// statisk HTML → ingen JS-motor, enkel JS → sandboxad QuickJS+DOM, etc.
use serde::{Deserialize, Serialize};

use crate::hydration;
use crate::js_eval;

// ─── Typer ──────────────────────────────────────────────────────────────────

/// Parse-pipeline tier (Tier 0 = snabbast, Tier 4 = långsammast)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ParseTier {
    /// Tier 0: Hydration extraction — 0 ms JS, extraherar SSR-data direkt
    Hydration { framework: String },
    /// Tier 1: Statisk HTML parse — ingen JS, ~1 ms
    StaticParse,
    /// Tier 2: Sandboxad QuickJS + DOM — kör inline scripts mot ArenaDom, ~10-50 ms
    QuickJsDom { script_count: u32 },
    /// Tier 2.5: QuickJS + DOM + lifecycle — DOMContentLoaded/load events, ~50-200 ms
    QuickJsLifecycle { script_count: u32 },
    /// Tier 3: Blitz render — ren Rust CSS-layout, ~10-50 ms
    BlitzRender,
    /// Tier 4: Chrome CDP — fullständig browser, ~500-2000 ms
    ChromeCdp { reason: String },
}

/// Beslut från tier-analys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierDecision {
    /// Vald tier
    pub tier: ParseTier,
    /// Anledning till valet (kort beskrivning)
    pub reason: String,
    /// Confidence (0.0–1.0) att vald tier räcker
    pub confidence: f32,
    /// Analystid i mikrosekunder
    pub analysis_time_us: u64,
}

// ─── Tier-detektorer ────────────────────────────────────────────────────────

/// Mönster som indikerar att sidan behöver full browser (Tier 4)
const HEAVY_JS_PATTERNS: &[&str] = &[
    "webgl",
    "canvas.getcontext",
    "web worker",
    "new worker",
    "serviceworker",
    "webassembly",
    "wasm",
    "indexeddb",
    "createobjecturl",
    "websocket",
    "webrtc",
    "rtcpeerconnection",
];

/// SPA-ramverk som typiskt kräver JS-exekvering
const SPA_MARKERS: &[&str] = &[
    "react-root",
    "react-app",
    "__next",
    "ng-version",
    "ng-app",
    "data-reactroot",
    "data-reactid",
    "svelte-",
    "nuxt",
    "gatsby",
    "remix",
    // Extra: React script bundles (Anthropic, Vercel, etc.)
    "react-dom",
    "react.production",
    "react.development",
    "_buildmanifest.js",
    "_ssgmanifest.js",
    // Vue/Nuxt
    "__vue_ssr_context",
    "__nuxt",
    // Generic SPA: empty body + heavy JS
    "\"use client\"",
    "createroot",
    "hydrateroot",
];

/// CSS-patterns som kräver layout-beräkning (Tier 3+)
const LAYOUT_PATTERNS: &[&str] = &[
    "position:absolute",
    "position:fixed",
    "position:sticky",
    "display:grid",
    "display:flex",
    "transform:",
    "animation:",
    "transition:",
    "@media",
    "calc(",
];

/// Välj optimal parse-tier för en given HTML-sida
pub fn select_tier(html: &str, url: &str) -> TierDecision {
    let start = std::time::Instant::now();
    let html_lower = html.to_lowercase();

    // ─── Tier 0: Hydration check ────────────────────────────────────────

    if let Some(data) = hydration::extract_hydration_state(html) {
        let framework = format!("{:?}", data.framework);
        return TierDecision {
            tier: ParseTier::Hydration {
                framework: framework.clone(),
            },
            reason: format!("{} hydration-data hittad, JS onödig", framework),
            confidence: 0.9,
            analysis_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // ─── JS-detektion ───────────────────────────────────────────────────

    let js_info = js_eval::detect_js_snippets(html);

    // ─── Tier 4: Tung JS check ──────────────────────────────────────────

    for pattern in HEAVY_JS_PATTERNS {
        if html_lower.contains(pattern) {
            return TierDecision {
                tier: ParseTier::ChromeCdp {
                    reason: format!("heavy JS: {}", pattern),
                },
                reason: format!("Sidan använder {} — kräver full browser", pattern),
                confidence: 0.95,
                analysis_time_us: start.elapsed().as_micros() as u64,
            };
        }
    }

    // ─── Tier 1: Ingen JS alls ──────────────────────────────────────────

    if js_info.total_inline_scripts == 0
        && js_info.total_event_handlers == 0
        && !js_info.has_framework
    {
        return TierDecision {
            tier: ParseTier::StaticParse,
            reason: "Ingen JS detekterad — statisk parse räcker".to_string(),
            confidence: 0.95,
            analysis_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // ─── Content sufficiency check ────────────────────────────────────────
    // Om body har ≥5 substantiella text-element OCH inga SPA-markörer,
    // klassificera som StaticParse. JS-eval körs fortfarande om anroparen
    // explicit sätter run_js=true, men kvalitetschecken i lifecycle-parse
    // säkerställer att JS-resultatet inte är sämre.
    let static_content_sufficient = has_sufficient_static_content(&html_lower);
    if static_content_sufficient && !is_spa_shell(&html_lower) && !js_info.has_framework {
        return TierDecision {
            tier: ParseTier::StaticParse,
            reason: "Statisk HTML har tillräckligt content — JS-eval rekommenderas ej".to_string(),
            confidence: 0.85,
            analysis_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // ─── Tier 3: Layout-beroende ────────────────────────────────────────

    let needs_layout = check_layout_dependency(&html_lower);

    // ─── Tier 2: JS med DOM ─────────────────────────────────────────────

    // Kolla om JS:en påverkar innehåll (DOM-mutation)
    let has_content_affecting_js = js_info.snippets.iter().any(|s| s.affects_content);

    if has_content_affecting_js && !needs_layout {
        // SPA-check: om det är ett SPA-ramverk med tom body, behövs kanske CDP
        if is_spa_shell(&html_lower) {
            return TierDecision {
                tier: ParseTier::ChromeCdp {
                    reason: "SPA shell with empty body".to_string(),
                },
                reason: "SPA-skelett utan server-renderat innehåll".to_string(),
                confidence: 0.8,
                analysis_time_us: start.elapsed().as_micros() as u64,
            };
        }

        // Framework-sidor med DOMContentLoaded/load-beroende → Tier 2.5
        let needs_lifecycle = js_info.has_framework
            || html_lower.contains("domcontentloaded")
            || html_lower.contains("addeventlistener")
            || html_lower.contains("onload");

        if needs_lifecycle {
            return TierDecision {
                tier: ParseTier::QuickJsLifecycle {
                    script_count: js_info.total_inline_scripts,
                },
                reason: format!(
                    "{} inline scripts + framework/lifecycle — QuickJS+lifecycle",
                    js_info.total_inline_scripts
                ),
                confidence: 0.80,
                analysis_time_us: start.elapsed().as_micros() as u64,
            };
        }

        return TierDecision {
            tier: ParseTier::QuickJsDom {
                script_count: js_info.total_inline_scripts,
            },
            reason: format!(
                "{} inline scripts med DOM-åtkomst — QuickJS+DOM räcker",
                js_info.total_inline_scripts
            ),
            confidence: 0.85,
            analysis_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // Om det finns JS men den inte påverkar innehåll, statisk parse räcker
    if !has_content_affecting_js && !needs_layout {
        return TierDecision {
            tier: ParseTier::StaticParse,
            reason: "JS finns men påverkar inte sidinnehåll".to_string(),
            confidence: 0.8,
            analysis_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // Layout-beroende utan tung JS → Blitz
    if needs_layout && !has_content_affecting_js {
        return TierDecision {
            tier: ParseTier::BlitzRender,
            reason: "CSS-layout kräver rendering, men ingen JS-påverkan".to_string(),
            confidence: 0.75,
            analysis_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // Layout + JS → CDP
    let _url = url; // Undvik unused-varning
    TierDecision {
        tier: ParseTier::ChromeCdp {
            reason: "layout + content JS".to_string(),
        },
        reason: "Kräver både CSS-layout och JS-exekvering".to_string(),
        confidence: 0.7,
        analysis_time_us: start.elapsed().as_micros() as u64,
    }
}

/// Kontrollera om sidan har CSS-mönster som kräver layout-beräkning
fn check_layout_dependency(html_lower: &str) -> bool {
    // Kolla inline styles och style-block
    let mut layout_count = 0;
    for pattern in LAYOUT_PATTERNS {
        if html_lower.contains(pattern) {
            layout_count += 1;
        }
    }
    // Minst 3 layout-patterns tyder på layout-beroende sida
    layout_count >= 3
}

/// Kolla om statisk HTML redan har tillräckligt med content.
/// Räknar substantiella text-element i body (inte scripts/styles/nav).
/// Returnerar true om ≥5 content-element hittas — JS-eval är då onödig.
fn has_sufficient_static_content(html_lower: &str) -> bool {
    let body_start = match html_lower.find("<body") {
        Some(s) => s,
        None => return false,
    };
    let body_end = html_lower.find("</body>").unwrap_or(html_lower.len());
    let body = match html_lower.get(body_start..body_end) {
        Some(b) => b,
        None => return false,
    };

    // Räkna content-tags som typiskt innehåller text
    let content_tags = [
        "<p>",
        "<p ",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<li>",
        "<li ",
        "<td>",
        "<td ",
        "<th>",
        "<th ",
        "<article",
        "<figcaption",
        "<blockquote",
    ];
    let mut content_count: usize = 0;
    for tag in content_tags {
        content_count += body.matches(tag).count();
    }

    // Om det finns ≥5 content-element har sidan tillräcklig statisk content
    // Undantag: om body har en SPA-mount och väldigt lite content runt den
    content_count >= 5
}

/// Detektera SPA-skelett (tom body med bara en mount-point)
fn is_spa_shell(html_lower: &str) -> bool {
    // Kolla om body är nästan tom (bara ett div med SPA-markör)
    let has_spa_marker = SPA_MARKERS.iter().any(|m| html_lower.contains(m));
    if !has_spa_marker {
        return false;
    }

    // Räkna synliga text-noder utanför script/style
    // Grov heuristik: om <body> innehåller <5 text-element, är det ett skelett
    let body_start = html_lower.find("<body");
    let body_end = html_lower.find("</body>");
    if let (Some(start), Some(end)) = (body_start, body_end) {
        if let Some(body_content) = html_lower.get(start..end) {
            // Räkna <p>, <h1>-<h6>, <span>, <a> med text
            let text_tags = ["<p>", "<p ", "<h1", "<h2", "<h3", "<span>", "<span ", "<a "];
            let text_count: usize = text_tags
                .iter()
                .map(|t| body_content.matches(t).count())
                .sum();
            return text_count < 3;
        }
    }

    false
}

// ─── Tester ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier0_nextjs_hydration() {
        let html = r##"
        <html><head></head><body>
        <div id="__next"><h1>Hem</h1></div>
        <script id="__NEXT_DATA__" type="application/json">
        {"props":{"pageProps":{"title":"Hem"}},"page":"/"}
        </script>
        </body></html>
        "##;

        let decision = select_tier(html, "https://example.com");
        assert!(
            matches!(decision.tier, ParseTier::Hydration { .. }),
            "Next.js-sida borde ge Tier 0 (Hydration), fick {:?}",
            decision.tier
        );
        assert!(
            decision.confidence > 0.8,
            "Confidence borde vara hög för hydration"
        );
    }

    #[test]
    fn test_tier1_static_html() {
        let html = r#"
        <html><head><title>Statisk sida</title></head>
        <body>
            <h1>Välkommen</h1>
            <p>Det här är en enkel sida utan JavaScript.</p>
            <a href="/kontakt">Kontakt</a>
        </body></html>
        "#;

        let decision = select_tier(html, "https://example.com");
        assert_eq!(
            decision.tier,
            ParseTier::StaticParse,
            "Statisk HTML utan JS borde ge Tier 1, fick {:?}",
            decision.tier
        );
    }

    #[test]
    fn test_tier2_inline_js_with_dom() {
        let html = r##"
        <html><body>
            <span id="price">0</span>
            <script>
                document.getElementById('price').textContent = (29.99 * 2).toFixed(2);
            </script>
        </body></html>
        "##;

        let decision = select_tier(html, "https://shop.example.com");
        assert!(
            matches!(decision.tier, ParseTier::QuickJsDom { .. }),
            "Inline JS med DOM-access borde ge Tier 2 (QuickJsDom), fick {:?}",
            decision.tier
        );
    }

    #[test]
    fn test_tier4_webgl() {
        let html = r#"
        <html><body>
            <canvas id="gl"></canvas>
            <script>
                var ctx = document.getElementById('gl').getContext('webgl');
            </script>
        </body></html>
        "#;

        let decision = select_tier(html, "https://game.example.com");
        assert!(
            matches!(decision.tier, ParseTier::ChromeCdp { .. }),
            "WebGL-sida borde ge Tier 4 (ChromeCDP), fick {:?}",
            decision.tier
        );
    }

    #[test]
    fn test_tier4_spa_shell() {
        let html = r#"
        <html><head></head>
        <body>
            <div id="react-root"></div>
            <script src="/static/js/main.chunk.js"></script>
            <script>
                document.getElementById('react-root').innerHTML = '';
            </script>
        </body></html>
        "#;

        let decision = select_tier(html, "https://app.example.com");
        assert!(
            matches!(decision.tier, ParseTier::ChromeCdp { .. }),
            "SPA-skelett borde ge Tier 4, fick {:?}",
            decision.tier
        );
    }

    #[test]
    fn test_tier1_js_without_dom() {
        let html = r#"
        <html><body>
            <p>Synligt innehåll</p>
            <script>
                var analytics = { page: 'home', ts: Date.now() };
            </script>
        </body></html>
        "#;

        let decision = select_tier(html, "https://example.com");
        assert_eq!(
            decision.tier,
            ParseTier::StaticParse,
            "JS som inte påverkar DOM borde ge Tier 1, fick {:?}",
            decision.tier
        );
    }

    #[test]
    fn test_tier0_nuxt() {
        let html = r#"
        <html><body>
            <div id="__nuxt">Content</div>
            <script>window.__NUXT__={"data":[{"items":["a"]}]}</script>
        </body></html>
        "#;

        let decision = select_tier(html, "https://nuxt.example.com");
        assert!(
            matches!(decision.tier, ParseTier::Hydration { .. }),
            "Nuxt-sida borde ge Tier 0, fick {:?}",
            decision.tier
        );
    }

    #[test]
    fn test_analysis_time_is_fast() {
        let html = r#"<html><body><p>Simple</p></body></html>"#;
        let decision = select_tier(html, "https://example.com");
        assert!(
            decision.analysis_time_us < 10_000,
            "Tier-analys borde ta <10ms, tog {} µs",
            decision.analysis_time_us
        );
    }

    #[test]
    fn test_decision_serialization() {
        let decision = TierDecision {
            tier: ParseTier::StaticParse,
            reason: "Test".to_string(),
            confidence: 0.9,
            analysis_time_us: 42,
        };
        let json = serde_json::to_string(&decision);
        assert!(json.is_ok(), "TierDecision borde vara serialiserbar");
        let json_str = json.unwrap();
        assert!(
            json_str.contains("StaticParse"),
            "JSON borde innehålla tier-namn"
        );
    }

    #[test]
    fn test_is_spa_shell_detection() {
        let spa = r#"<html><body><div id="react-root"></div><script src="app.js"></script></body></html>"#;
        assert!(
            is_spa_shell(&spa.to_lowercase()),
            "Borde detektera SPA-skelett"
        );

        let not_spa = r#"<html><body><h1>Title</h1><p>Content</p><p>More</p><a href="/">Link</a></body></html>"#;
        assert!(
            !is_spa_shell(&not_spa.to_lowercase()),
            "Borde INTE flagga sida med mycket innehåll som SPA"
        );
    }

    #[test]
    fn test_tier_lifecycle_for_framework_page() {
        let html = r##"<html><body>
            <div id="app">Content</div>
            <script>
                document.addEventListener('DOMContentLoaded', function() {
                    document.getElementById('app').textContent = 'Loaded';
                });
            </script>
        </body></html>"##;
        let decision = select_tier(html, "https://example.com");
        match &decision.tier {
            ParseTier::QuickJsLifecycle { .. } => {}
            other => panic!(
                "Borde välja QuickJsLifecycle för DOMContentLoaded-sida, fick {:?}",
                other
            ),
        }
    }
}
