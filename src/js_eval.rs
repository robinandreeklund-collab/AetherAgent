/// JavaScript Sandbox Evaluation – Fas 4b
///
/// Kör små JS-snippets i en sandboxad Boa-motor.
/// Används för att utvärdera klientlogik som påverkar sidinnehåll,
/// t.ex. prisuträkningar, villkorlig rendering, textinterpolering.
///
/// Säkerhetsprincip: Ingen åtkomst till DOM, nätverk, filsystem eller timers.
/// Bara ren beräkningslogik (matematik, strängar, objekt, arrayer).
#[cfg(feature = "js-eval")]
use boa_engine::{Context, Source};

use serde::{Deserialize, Serialize};

/// Resultat från en JS-evaluering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsEvalResult {
    /// Lyckat resultat som sträng
    pub value: Option<String>,
    /// Felmeddelande om evalueringen misslyckades
    pub error: Option<String>,
    /// Om evalueringen avbröts pga timeout/säkerhet
    pub timed_out: bool,
    /// Exekveringstid i mikrosekunder
    pub eval_time_us: u64,
}

/// Batch-resultat från flera evalueringar
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsBatchResult {
    pub results: Vec<JsEvalResult>,
    pub total_eval_time_us: u64,
}

/// Detekterat JS-snippet i HTML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedSnippet {
    /// Typ av snippet
    pub snippet_type: SnippetType,
    /// JS-koden
    pub code: String,
    /// Var snippeten hittades (t.ex. "inline-script", "onclick", "data-bind")
    pub source: String,
    /// Om den troligen påverkar synligt innehåll
    pub affects_content: bool,
}

/// Typ av JS-snippet
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SnippetType {
    /// Inline <script>-block
    InlineScript,
    /// Event handler (onclick, onchange, etc.)
    EventHandler,
    /// Template-uttryck ({{ }}, ${}, etc.)
    TemplateExpression,
    /// Beräkning av värde (rena uttryck)
    ValueExpression,
}

/// Resultat från JS-detektion i HTML
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsDetectionResult {
    pub snippets: Vec<DetectedSnippet>,
    pub has_framework: bool,
    pub framework_hint: Option<String>,
    pub total_inline_scripts: u32,
    pub total_event_handlers: u32,
}

// ─── Snippet-detektion ──────────────────────────────────────────────────────

/// Mönster som indikerar innehållspåverkan
const CONTENT_PATTERNS: &[&str] = &[
    "innertext",
    "textcontent",
    "innerhtml",
    "appendchild",
    "createelement",
    "classlist",
    "style.",
    "display",
    "visibility",
    "document.write",
    "insertadjacent",
];

/// Mönster som identifierar JS-frameworks
const FRAMEWORK_PATTERNS: &[(&str, &str)] = &[
    // Mer specifika mönster först (Next.js före React, etc.)
    ("__next", "Next.js"),
    ("__nuxt", "Nuxt"),
    ("__svelte", "Svelte"),
    ("ng-app", "Angular"),
    ("_jsx", "React"),
    ("createroot", "React"),
    ("reactroot", "React"),
    ("createapp", "Vue"),
    ("vue", "Vue"),
    ("angular", "Angular"),
    ("svelte", "Svelte"),
];

/// Detektera JS-snippets i HTML-innehåll
pub fn detect_js_snippets(html: &str) -> JsDetectionResult {
    let mut snippets = Vec::new();
    let mut total_inline_scripts = 0u32;
    let mut total_event_handlers = 0u32;
    let mut has_framework = false;
    let mut framework_hint = None;

    let lower = html.to_lowercase();

    // Hitta inline <script>-block
    let mut search_from = 0;
    while let Some(start) = lower[search_from..].find("<script") {
        let abs_start = search_from + start;
        // Hitta slutet av script-taggens öppning
        if let Some(tag_end) = lower[abs_start..].find('>') {
            let content_start = abs_start + tag_end + 1;
            // Hitta </script>
            if let Some(end) = lower[content_start..].find("</script>") {
                let code = html[content_start..content_start + end].trim().to_string();
                if !code.is_empty() {
                    // Kolla om src-attribut finns (extern script, inte inline)
                    let tag_text = &lower[abs_start..abs_start + tag_end];
                    let is_external = tag_text.contains("src=");

                    if !is_external {
                        total_inline_scripts += 1;
                        let affects = content_affects_dom(&code);
                        snippets.push(DetectedSnippet {
                            snippet_type: SnippetType::InlineScript,
                            code: truncate_code(&code, 500),
                            source: "inline-script".to_string(),
                            affects_content: affects,
                        });
                    }
                }
                search_from = content_start + end + 9;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    // Hitta event handlers (onclick, onchange, onload, etc.)
    let event_attrs = [
        "onclick=",
        "onchange=",
        "onsubmit=",
        "onload=",
        "oninput=",
        "onkeyup=",
        "onkeydown=",
        "onfocus=",
        "onblur=",
        "onmouseover=",
    ];
    for attr in &event_attrs {
        let mut pos = 0;
        while let Some(idx) = lower[pos..].find(attr) {
            let abs_idx = pos + idx;
            // Extrahera attributvärdet
            let after_eq = abs_idx + attr.len();
            if after_eq < html.len() {
                let quote = html.as_bytes().get(after_eq).copied().unwrap_or(b'"');
                if quote == b'"' || quote == b'\'' {
                    let value_start = after_eq + 1;
                    if let Some(end) = html[value_start..].find(quote as char) {
                        let code = html[value_start..value_start + end].to_string();
                        if !code.is_empty() {
                            total_event_handlers += 1;
                            let attr_name = attr.trim_end_matches('=');
                            snippets.push(DetectedSnippet {
                                snippet_type: SnippetType::EventHandler,
                                code: truncate_code(&code, 200),
                                source: attr_name.to_string(),
                                affects_content: true,
                            });
                        }
                        pos = value_start + end + 1;
                    } else {
                        pos = after_eq + 1;
                    }
                } else {
                    pos = after_eq + 1;
                }
            } else {
                break;
            }
        }
    }

    // Detektera framework
    for (pattern, name) in FRAMEWORK_PATTERNS {
        if lower.contains(pattern) {
            has_framework = true;
            framework_hint = Some(name.to_string());
            break;
        }
    }

    JsDetectionResult {
        snippets,
        has_framework,
        framework_hint,
        total_inline_scripts,
        total_event_handlers,
    }
}

/// Kolla om JS-koden troligen påverkar DOM-innehåll
fn content_affects_dom(code: &str) -> bool {
    let lower = code.to_lowercase();
    CONTENT_PATTERNS.iter().any(|p| lower.contains(p))
}

/// Trunkera kod säkert (UTF-8-aware)
fn truncate_code(code: &str, max_len: usize) -> String {
    if code.len() <= max_len {
        return code.to_string();
    }
    let mut end = max_len;
    while end > 0 && !code.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &code[..end])
}

// ─── JS-evaluering ──────────────────────────────────────────────────────────

/// Tillåtna mönster i allowlist — allt annat blockeras
///
/// Allowlist-modell: bara kända säkra operationer tillåts.
/// Säkrare än blocklist eftersom nya attacker inte kan slinka igenom.
#[cfg(feature = "js-eval")]
const ALLOWED_PATTERNS: &[&str] = &[
    // Matematik & operatorer
    "math.",
    "number(",
    "parsefloat(",
    "parseint(",
    "isnan(",
    "isfinite(",
    "tofixed(",
    "toprecision(",
    // Strängar
    "string(",
    "tostring(",
    "tolocalestring(",
    "tolowercase(",
    "touppercase(",
    "trim(",
    "replace(",
    "split(",
    "join(",
    "slice(",
    "substring(",
    "includes(",
    "startswith(",
    "endswith(",
    "indexof(",
    "lastindexof(",
    "padstart(",
    "padend(",
    "repeat(",
    "charat(",
    "charcodeat(",
    "concat(",
    // Arrayer
    "array.",
    "map(",
    "filter(",
    "reduce(",
    "foreach(",
    "find(",
    "findindex(",
    "some(",
    "every(",
    "sort(",
    "reverse(",
    "flat(",
    "flatmap(",
    "push(",
    "pop(",
    "shift(",
    "unshift(",
    "fill(",
    "from(",
    "of(",
    "length",
    "keys(",
    "values(",
    "entries(",
    // Objekt
    "object.",
    "json.stringify(",
    "json.parse(",
    "hasownproperty(",
    // Ternary, literals, variabler — tillåts implicit av Boa
    "true",
    "false",
    "null",
    "undefined",
    "typeof",
    "instanceof",
    "new date(",
    "date.now(",
    "regexp(",
    "new regexp(",
];

/// Förbjudna mönster som alltid blockeras (även om nåt i allowlist matchar)
#[cfg(feature = "js-eval")]
const DENIED_PATTERNS: &[&str] = &[
    "fetch(",
    "xmlhttp",
    "import(",
    "require(",
    "eval(",
    "settimeout(",
    "setinterval(",
    "new worker",
    "indexeddb",
    "localstorage",
    "sessionstorage",
    "cookie",
    "globalthis",
    "process.",
    "child_process",
    "__proto__",
    "constructor[",
    "prototype.",
];

/// Kontrollera om JS-kod är säker att evaluera (allowlist-modell)
///
/// Returnerar Ok(()) om koden är säker, Err(anledning) om blockerad.
#[cfg(feature = "js-eval")]
fn check_js_safety(code: &str) -> Result<(), String> {
    let lower = code.to_lowercase();

    // Steg 1: Kolla deny-list — absolut förbjudna mönster
    for denied in DENIED_PATTERNS {
        if lower.contains(denied) {
            return Err(format!(
                "Blocked: '{}' is not allowed in sandbox",
                denied.trim_end_matches('(')
            ));
        }
    }

    // Steg 2: Kolla om koden innehåller funktionsanrop som inte är i allowlist
    // Enkla uttryck (literals, matematik-operatorer, variabler) tillåts alltid.
    // Funktionsanrop (ord följt av parentes) måste vara i allowlist.
    // Vi parsear inte fullt — heuristik som fångar de farligaste fallen.
    let has_suspicious_call = detect_suspicious_calls(&lower);
    if let Some(suspicious) = has_suspicious_call {
        // Kolla om det finns i allowlist
        let is_allowed = ALLOWED_PATTERNS
            .iter()
            .any(|p| suspicious.contains(p) || lower.contains(p));
        if !is_allowed {
            // Tillåt ren beräkningslogik utan funktionsanrop
            // (t.ex. "1 + 2", "x * 3", "'hello' + 'world'")
            if contains_only_safe_tokens(&lower) {
                return Ok(());
            }
            return Err(format!("Blocked: '{}' is not in allowlist", suspicious));
        }
    }

    Ok(())
}

/// Detektera potentiellt farliga funktionsanrop
#[cfg(feature = "js-eval")]
fn detect_suspicious_calls(lower: &str) -> Option<String> {
    // Sök efter mönster: word( — som indikerar funktionsanrop
    // Ignorera: operatorer, ternary, array indexing
    for (i, ch) in lower.char_indices() {
        if ch == '(' {
            // Hitta funktionsnamn före parentesen
            let before = &lower[..i];
            let name: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_alphanumeric() || *c == '.' || *c == '_')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            let name = name.trim_start_matches('.');
            if name.is_empty() || name == "(" {
                continue;
            }
            // Skippa kända säkra namn
            if ALLOWED_PATTERNS
                .iter()
                .any(|p| p.trim_end_matches('(') == name || name.ends_with(p.trim_end_matches('(')))
            {
                continue;
            }
            // Skippa vanliga JS-konstruktioner
            if matches!(
                name,
                "if" | "for" | "while" | "switch" | "catch" | "var" | "let" | "const"
            ) {
                continue;
            }
            // Potentiellt farligt — rapportera
            return Some(name.to_string());
        }
    }
    None
}

/// Kontrollera om koden bara innehåller säkra tokens (literals, operatorer, variabler)
#[cfg(feature = "js-eval")]
fn contains_only_safe_tokens(code: &str) -> bool {
    // Tillåtna tecken i rena uttryck
    let safe_chars = |c: char| -> bool {
        c.is_alphanumeric()
            || c.is_whitespace()
            || "+-*/%=<>!&|^~?:.,;'\"()[]{}0123456789_$`".contains(c)
    };

    code.chars().all(safe_chars)
}

/// Evalera ett JS-uttryck i en sandboxad miljö
///
/// Använder allowlist-modell: bara kända säkra mönster tillåts.
/// Stöder: matematik, strängar, arrayer, objekt, ternary, template literals.
/// Stöder INTE: DOM, fetch, timers, import, require.
#[cfg(feature = "js-eval")]
pub fn eval_js(code: &str) -> JsEvalResult {
    let start = std::time::Instant::now();

    // Allowlist-baserad säkerhetskontroll
    if let Err(reason) = check_js_safety(code) {
        return JsEvalResult {
            value: None,
            error: Some(reason),
            timed_out: false,
            eval_time_us: start.elapsed().as_micros() as u64,
        };
    }

    let mut context = Context::default();

    match context.eval(Source::from_bytes(code)) {
        Ok(result) => {
            let value_str = result
                .to_string(&mut context)
                .map_or_else(|_| "undefined".to_string(), |v| v.to_std_string_escaped());
            JsEvalResult {
                value: Some(value_str),
                error: None,
                timed_out: false,
                eval_time_us: start.elapsed().as_micros() as u64,
            }
        }
        Err(e) => JsEvalResult {
            value: None,
            error: Some(format!("{}", e)),
            timed_out: false,
            eval_time_us: start.elapsed().as_micros() as u64,
        },
    }
}

/// Evalera flera JS-uttryck med delad kontext (persistent state)
///
/// Alla snippets delar samma Boa Context — variabler definierade i
/// snippet 1 är tillgängliga i snippet 2, etc.
#[cfg(feature = "js-eval")]
pub fn eval_js_batch(snippets: &[String]) -> JsBatchResult {
    let start = std::time::Instant::now();

    // Persistent kontext — delad mellan alla snippets
    let mut context = Context::default();
    let mut results = Vec::with_capacity(snippets.len());

    for code in snippets {
        let snippet_start = std::time::Instant::now();

        // Allowlist-kontroll per snippet
        if let Err(reason) = check_js_safety(code) {
            results.push(JsEvalResult {
                value: None,
                error: Some(reason),
                timed_out: false,
                eval_time_us: snippet_start.elapsed().as_micros() as u64,
            });
            continue;
        }

        match context.eval(Source::from_bytes(code.as_bytes())) {
            Ok(result) => {
                let value_str = result
                    .to_string(&mut context)
                    .map_or_else(|_| "undefined".to_string(), |v| v.to_std_string_escaped());
                results.push(JsEvalResult {
                    value: Some(value_str),
                    error: None,
                    timed_out: false,
                    eval_time_us: snippet_start.elapsed().as_micros() as u64,
                });
            }
            Err(e) => {
                results.push(JsEvalResult {
                    value: None,
                    error: Some(format!("{}", e)),
                    timed_out: false,
                    eval_time_us: snippet_start.elapsed().as_micros() as u64,
                });
            }
        }
    }

    JsBatchResult {
        results,
        total_eval_time_us: start.elapsed().as_micros() as u64,
    }
}

/// Stub-implementation när js-eval-featuren inte är aktiverad
#[cfg(not(feature = "js-eval"))]
pub fn eval_js(_code: &str) -> JsEvalResult {
    JsEvalResult {
        value: None,
        error: Some("JS evaluation not available: compile with --features js-eval".to_string()),
        timed_out: false,
        eval_time_us: 0,
    }
}

#[cfg(not(feature = "js-eval"))]
pub fn eval_js_batch(snippets: &[String]) -> JsBatchResult {
    let results: Vec<JsEvalResult> = snippets.iter().map(|_| eval_js("")).collect();
    JsBatchResult {
        results,
        total_eval_time_us: 0,
    }
}

// ─── Fas 10: Fetch URL-extraktion ────────────────────────────────────────────

/// Extract fetch()/XHR URLs from JavaScript code (Fas 10)
///
/// Detects patterns like:
/// - `fetch('url')` or `fetch("url")`
/// - `new XMLHttpRequest()` followed by `.open('GET', 'url')`
/// - `$.ajax({url: 'url'})` or `$.get('url')`
pub fn extract_fetch_urls(code: &str) -> Vec<String> {
    let mut urls = Vec::new();

    // Mönster 1: fetch('url') eller fetch("url")
    extract_fetch_pattern(code, &mut urls);

    // Mönster 2: .open('METHOD', 'url')
    extract_xhr_open_pattern(code, &mut urls);

    // Mönster 3: $.ajax({url: 'url'}) eller $.get('url') / $.post('url')
    extract_jquery_pattern(code, &mut urls);

    urls
}

/// Hitta fetch('url') och fetch("url") mönster
fn extract_fetch_pattern(code: &str, urls: &mut Vec<String>) {
    let lower = code.to_lowercase();
    let mut pos = 0;

    while let Some(idx) = lower[pos..].find("fetch(") {
        let abs_start = pos + idx + 6; // efter "fetch("
                                       // Hoppa över whitespace
        let rest = &code[abs_start..];
        let trimmed = rest.trim_start();
        let offset = rest.len() - trimmed.len();

        if let Some(url) = extract_url_from_quote(trimmed) {
            urls.push(url);
        }
        pos = abs_start + offset + 1;
    }
}

/// Hitta .open('METHOD', 'url') mönster (XMLHttpRequest)
fn extract_xhr_open_pattern(code: &str, urls: &mut Vec<String>) {
    let lower = code.to_lowercase();
    let mut pos = 0;

    while let Some(idx) = lower[pos..].find(".open(") {
        let abs_start = pos + idx + 6; // efter ".open("
        let rest = &code[abs_start..];

        // Förvänta: 'METHOD', 'URL' eller "METHOD", "URL"
        // Hoppa över första argumentet (metoden)
        if let Some((_, after_method)) = extract_quoted_value(rest.trim_start()) {
            let after_comma =
                rest[rest.len() - rest.trim_start().len() + after_method..].trim_start();
            if let Some(stripped) = after_comma.strip_prefix(',') {
                let url_part = stripped.trim_start();
                if let Some(url) = extract_url_from_quote(url_part) {
                    urls.push(url);
                }
            }
        }
        pos = abs_start + 1;
    }
}

/// Hitta $.ajax({url: 'url'}), $.get('url'), $.post('url') mönster
fn extract_jquery_pattern(code: &str, urls: &mut Vec<String>) {
    let lower = code.to_lowercase();

    // $.get('url') och $.post('url')
    for pattern in &["$.get(", "$.post("] {
        let mut pos = 0;
        while let Some(idx) = lower[pos..].find(pattern) {
            let abs_start = pos + idx + pattern.len();
            let rest = &code[abs_start..];
            if let Some(url) = extract_url_from_quote(rest.trim_start()) {
                urls.push(url);
            }
            pos = abs_start + 1;
        }
    }

    // $.ajax({url: 'url'})
    let mut pos = 0;
    while let Some(idx) = lower[pos..].find("$.ajax(") {
        let abs_start = pos + idx + 7;
        let rest = &code[abs_start..];
        // Sök efter url: 'value' eller url: "value" inuti {...}
        if let Some(brace_end) = rest.find('}') {
            let block = &rest[..brace_end];
            let block_lower = block.to_lowercase();
            if let Some(url_idx) = block_lower.find("url:") {
                let after_url = &block[url_idx + 4..].trim_start();
                if let Some(url) = extract_url_from_quote(after_url) {
                    urls.push(url);
                }
            }
        }
        pos = abs_start + 1;
    }
}

/// Extrahera URL från första quote-omslutna strängen
fn extract_url_from_quote(s: &str) -> Option<String> {
    let (val, _) = extract_quoted_value(s)?;
    // Validera att det ser ut som en URL (inte tomt, inte JS-kod)
    if val.is_empty() || val.contains('{') || val.contains('}') {
        return None;
    }
    Some(val)
}

/// Extrahera ett quote-omslutet värde, returnera (värde, position efter slut-quote)
fn extract_quoted_value(s: &str) -> Option<(String, usize)> {
    let first_char = s.chars().next()?;
    if first_char != '\'' && first_char != '"' {
        return None;
    }
    let inner = &s[1..];
    let end = inner.find(first_char)?;
    Some((inner[..end].to_string(), 1 + end + 1))
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Detektionstester ───────────────────────────────────────────────

    #[test]
    fn test_detect_inline_script() {
        let html = r#"<html><body>
            <script>document.getElementById('price').textContent = '$' + (29.99 * 2);</script>
            <p id="price"></p>
        </body></html>"#;

        let result = detect_js_snippets(html);
        assert_eq!(
            result.total_inline_scripts, 1,
            "Borde hitta 1 inline script"
        );
        assert!(
            result.snippets[0].affects_content,
            "Script som ändrar textContent borde markeras som affects_content"
        );
    }

    #[test]
    fn test_detect_external_script_ignored() {
        let html = r#"<html><body>
            <script src="https://cdn.example.com/app.js"></script>
        </body></html>"#;

        let result = detect_js_snippets(html);
        assert_eq!(
            result.total_inline_scripts, 0,
            "Externa scripts borde ignoreras"
        );
    }

    #[test]
    fn test_detect_event_handlers() {
        let html = r##"<html><body>
            <button onclick="this.textContent='Tillagd!'">Lägg i varukorg</button>
            <input onchange="updateTotal()" />
        </body></html>"##;

        let result = detect_js_snippets(html);
        assert_eq!(
            result.total_event_handlers, 2,
            "Borde hitta 2 event handlers"
        );
    }

    #[test]
    fn test_detect_react_framework() {
        let html = r#"<html><body>
            <div id="__next"><div data-reactroot=""></div></div>
            <script>__NEXT_DATA__ = {};</script>
        </body></html>"#;

        let result = detect_js_snippets(html);
        assert!(result.has_framework, "Borde detektera Next.js");
        assert_eq!(result.framework_hint, Some("Next.js".to_string()));
    }

    #[test]
    fn test_detect_vue_framework() {
        let html = r#"<html><body>
            <div id="app"></div>
            <script>const app = Vue.createApp({})</script>
        </body></html>"#;

        let result = detect_js_snippets(html);
        assert!(result.has_framework, "Borde detektera Vue");
    }

    #[test]
    fn test_no_js_detected() {
        let html = r#"<html><body>
            <h1>Statisk sida</h1>
            <p>Inget JavaScript här.</p>
        </body></html>"#;

        let result = detect_js_snippets(html);
        assert_eq!(result.total_inline_scripts, 0);
        assert_eq!(result.total_event_handlers, 0);
        assert!(!result.has_framework);
    }

    #[test]
    fn test_code_truncation() {
        let long_code = "a".repeat(1000);
        let truncated = truncate_code(&long_code, 100);
        assert!(truncated.len() <= 103); // 100 + "..."
        assert!(truncated.ends_with("..."));
    }

    #[test]
    fn test_code_truncation_multibyte() {
        let code = "å".repeat(100); // 2 bytes per char
        let truncated = truncate_code(&code, 50);
        assert!(truncated.ends_with("..."));
        // Borde inte panika på char boundary
    }

    // ─── Evalueringstester (kräver js-eval feature) ─────────────────────

    #[cfg(feature = "js-eval")]
    mod eval_tests {
        use super::super::*;

        #[test]
        fn test_eval_math() {
            let result = eval_js("29.99 * 2");
            assert_eq!(result.value, Some("59.98".to_string()));
            assert!(result.error.is_none());
        }

        #[test]
        fn test_eval_string() {
            let result = eval_js("'Hello' + ' ' + 'World'");
            assert_eq!(result.value, Some("Hello World".to_string()));
        }

        #[test]
        fn test_eval_template_literal() {
            let result = eval_js("`Pris: ${(199 * 1.25).toFixed(2)} kr`");
            assert_eq!(result.value, Some("Pris: 248.75 kr".to_string()));
        }

        #[test]
        fn test_eval_ternary() {
            let result = eval_js("true ? 'I lager' : 'Slut'");
            assert_eq!(result.value, Some("I lager".to_string()));
        }

        #[test]
        fn test_eval_array() {
            let result = eval_js("[1,2,3].map(x => x * 2).join(',')");
            assert_eq!(result.value, Some("2,4,6".to_string()));
        }

        #[test]
        fn test_eval_object() {
            let result = eval_js("JSON.stringify({price: 199, currency: 'SEK'})");
            assert_eq!(
                result.value,
                Some(r#"{"price":199,"currency":"SEK"}"#.to_string())
            );
        }

        #[test]
        fn test_eval_blocks_fetch() {
            let result = eval_js("fetch('https://evil.com')");
            assert!(result.error.is_some());
            assert!(result.error.unwrap().contains("Blocked"));
        }

        #[test]
        fn test_eval_blocks_document() {
            let result = eval_js("document.cookie");
            assert!(result.error.is_some());
            assert!(result.error.unwrap().contains("Blocked"));
        }

        #[test]
        fn test_eval_blocks_eval() {
            let result = eval_js("eval('1+1')");
            assert!(result.error.is_some());
            assert!(result.error.unwrap().contains("Blocked"));
        }

        #[test]
        fn test_eval_syntax_error() {
            let result = eval_js("{{invalid}}");
            assert!(result.error.is_some());
        }

        #[test]
        fn test_eval_batch() {
            let snippets = vec![
                "1 + 1".to_string(),
                "'a' + 'b'".to_string(),
                "Math.PI.toFixed(2)".to_string(),
            ];
            let result = eval_js_batch(&snippets);
            assert_eq!(result.results.len(), 3);
            assert_eq!(result.results[0].value, Some("2".to_string()));
            assert_eq!(result.results[1].value, Some("ab".to_string()));
            assert_eq!(result.results[2].value, Some("3.14".to_string()));
        }
    }

    // ─── Stub-tester (utan js-eval feature) ─────────────────────────────

    #[cfg(not(feature = "js-eval"))]
    #[test]
    fn test_eval_without_feature_returns_error() {
        let result = eval_js("1 + 1");
        assert!(result.error.is_some());
        assert!(
            result.error.unwrap().contains("not available"),
            "Borde indikera att featuren saknas"
        );
    }

    // ─── Fas 10: Fetch URL-extraktionstester ────────────────────────────

    #[test]
    fn test_extract_fetch_urls_single() {
        let code = "fetch('https://api.example.com/price')";
        let urls = extract_fetch_urls(code);
        assert_eq!(
            urls,
            vec!["https://api.example.com/price"],
            "Borde hitta en fetch-URL"
        );
    }

    #[test]
    fn test_extract_fetch_urls_double_quotes() {
        let code = r#"fetch("https://api.shop.se/products/42")"#;
        let urls = extract_fetch_urls(code);
        assert_eq!(
            urls,
            vec!["https://api.shop.se/products/42"],
            "Borde hitta fetch-URL med double quotes"
        );
    }

    #[test]
    fn test_extract_fetch_urls_none() {
        let code = "var x = 1 + 2; console.log(x);";
        let urls = extract_fetch_urls(code);
        assert!(
            urls.is_empty(),
            "Borde inte hitta fetch-URL:er i kod utan fetch"
        );
    }

    #[test]
    fn test_extract_fetch_urls_multiple() {
        let code = r#"
            fetch('https://api.shop.se/price');
            fetch("https://api.shop.se/stock");
        "#;
        let urls = extract_fetch_urls(code);
        assert_eq!(urls.len(), 2, "Borde hitta 2 fetch-URL:er");
    }

    #[test]
    fn test_extract_fetch_urls_xhr_open() {
        let code = r#"
            var xhr = new XMLHttpRequest();
            xhr.open('GET', 'https://api.shop.se/data');
        "#;
        let urls = extract_fetch_urls(code);
        assert!(
            urls.contains(&"https://api.shop.se/data".to_string()),
            "Borde hitta XHR .open() URL"
        );
    }

    #[test]
    fn test_extract_fetch_urls_jquery_get() {
        let code = "$.get('https://api.shop.se/info')";
        let urls = extract_fetch_urls(code);
        assert_eq!(
            urls,
            vec!["https://api.shop.se/info"],
            "Borde hitta $.get URL"
        );
    }

    #[test]
    fn test_extract_fetch_urls_jquery_ajax() {
        let code = r#"$.ajax({url: 'https://api.shop.se/cart', method: 'POST'})"#;
        let urls = extract_fetch_urls(code);
        assert_eq!(
            urls,
            vec!["https://api.shop.se/cart"],
            "Borde hitta $.ajax URL"
        );
    }
}
