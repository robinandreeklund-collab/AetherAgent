// Anti-bot Detection — 3-tier system med page-size gates
//
// Detekterar om en fetch returnerade en bot-challenge (Cloudflare, Akamai, etc.)
// istället för faktiskt innehåll. Förhindrar att CRFR bygger fält från garbage-HTML.
//
// Tier 1 (Hög confidence, alla sidstorlekar): Specifika fingerprints
// Tier 2 (Medel confidence, bara <10KB): Generiska block-termer
// Tier 3 (Strukturell, bara <50KB): Tomma skal utan content

/// Resultat från bot-detection
#[derive(Debug, Clone)]
pub struct BotDetectResult {
    /// Detekterades bot-skydd?
    pub is_blocked: bool,
    /// Vilken tier som triggade (None om ej blockerad)
    pub tier: Option<u8>,
    /// Specifikt system som detekterades (t.ex. "cloudflare", "akamai")
    pub system: Option<&'static str>,
    /// Confidence (0.0-1.0)
    pub confidence: f32,
}

impl BotDetectResult {
    fn pass() -> Self {
        BotDetectResult {
            is_blocked: false,
            tier: None,
            system: None,
            confidence: 0.0,
        }
    }

    fn blocked(tier: u8, system: &'static str, confidence: f32) -> Self {
        BotDetectResult {
            is_blocked: true,
            tier: Some(tier),
            system: Some(system),
            confidence,
        }
    }
}

// ─── Tier 1: Specifika fingerprints (alla sidstorlekar) ────────────────────

/// Tier 1 patterns: unika tekniska signaturer per WAF/anti-bot
static TIER1_PATTERNS: &[(&str, &str)] = &[
    // Cloudflare
    ("cf-browser-verification", "cloudflare"),
    ("cf_chl_opt", "cloudflare"),
    ("_cf_chl_tk", "cloudflare"),
    ("cdn-cgi/challenge-platform", "cloudflare"),
    ("cf-turnstile", "cloudflare"),
    // Akamai Bot Manager
    ("akamai-bm-telemetry", "akamai"),
    ("_abck", "akamai"),
    ("ak_bmsc", "akamai"),
    ("akamaighost", "akamai"),
    // PerimeterX / HUMAN
    ("window._pxappid", "perimeterx"),
    ("_pxhd", "perimeterx"),
    ("px-captcha", "perimeterx"),
    ("human-challenge", "perimeterx"),
    // DataDome
    ("datadome", "datadome"),
    ("dd.initializer", "datadome"),
    // Imperva / Incapsula
    ("incap_ses_", "imperva"),
    ("visid_incap_", "imperva"),
    ("_incapsula_resource", "imperva"),
    // Kasada
    ("cd_client_key", "kasada"),
    // Sucuri
    ("sucuri-firewall", "sucuri"),
];

fn check_tier1(html_lower: &str) -> Option<BotDetectResult> {
    for &(pattern, system) in TIER1_PATTERNS {
        if html_lower.contains(pattern) {
            return Some(BotDetectResult::blocked(1, system, 0.95));
        }
    }
    None
}

// ─── Tier 2: Generiska block-termer (bara <10KB) ──────────────────────────

static TIER2_PATTERNS: &[(&str, &str)] = &[
    ("access denied", "generic_block"),
    ("403 forbidden", "generic_block"),
    ("just a moment", "cloudflare_challenge"),
    ("checking your browser", "cloudflare_challenge"),
    ("please wait while we verify", "generic_challenge"),
    ("enable javascript and cookies", "generic_challenge"),
    ("g-recaptcha", "recaptcha"),
    ("h-captcha", "hcaptcha"),
    ("hcaptcha.com/1/api.js", "hcaptcha"),
    ("recaptcha/api.js", "recaptcha"),
    ("ray id:", "cloudflare_block"),
    ("please turn javascript on", "js_required"),
    ("you have been blocked", "generic_block"),
    ("sorry, you have been blocked", "generic_block"),
    ("pardon our interruption", "generic_challenge"),
];

fn check_tier2(html_lower: &str, size: usize) -> Option<BotDetectResult> {
    if size > 10_000 {
        return None; // Page-size gate: stora sidor har nästan alltid content
    }
    let mut matches = 0;
    let mut matched_system = "generic_block";
    for &(pattern, system) in TIER2_PATTERNS {
        if html_lower.contains(pattern) {
            matches += 1;
            matched_system = system;
        }
    }
    // Kräv minst 1 match
    if matches >= 1 {
        let confidence = (0.5 + matches as f32 * 0.15).min(0.9);
        Some(BotDetectResult::blocked(2, matched_system, confidence))
    } else {
        None
    }
}

// ─── Tier 3: Strukturell integritet (bara <50KB) ──────────────────────────

fn check_tier3(html: &str, html_lower: &str, size: usize) -> Option<BotDetectResult> {
    if size > 50_000 {
        return None; // Page-size gate
    }

    let mut signals = 0u32;

    // Signal 1: Ingen <body> tag
    if !html_lower.contains("<body") {
        signals += 1;
    }

    // Signal 2: Minimal synlig text (efter att strippa tags)
    let text_content = estimate_text_content(html);
    if size > 200 && text_content < 20 {
        signals += 1;
    }

    // Signal 3: Nästan bara script/style (>80% av innehållet)
    let script_bytes: usize = html_lower
        .match_indices("<script")
        .filter_map(|(start, _)| html_lower[start..].find("</script>").map(|end| end + 9))
        .sum();
    if size > 200 && script_bytes > size * 4 / 5 {
        signals += 1;
    }

    // Signal 4: Ingen text-content utanför tags (SPA shell)
    // Kräver > 1KB sida och < 30 synliga tecken — extremt tomt
    if size > 1000 && text_content < 30 {
        signals += 1;
    }

    // Kräv 2+ signaler (eller 1 på mycket små sidor <1KB)
    let threshold = if size < 1000 { 1 } else { 2 };
    if signals >= threshold {
        Some(BotDetectResult::blocked(
            3,
            "empty_shell",
            0.3 + signals as f32 * 0.2,
        ))
    } else {
        None
    }
}

/// Estimera antal tecken synlig text (utanför HTML-tags)
fn estimate_text_content(html: &str) -> usize {
    let mut in_tag = false;
    let mut in_script = false;
    let mut text_chars = 0usize;
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        let b = bytes[i];
        if b == b'<' {
            in_tag = true;
            // Kolla om det är <script
            if i + 7 < len && bytes[i + 1..].starts_with(b"script") {
                in_script = true;
            }
            if i + 8 < len && bytes[i + 1..].starts_with(b"/script") {
                in_script = false;
            }
            // Hoppa över <style> också
            if i + 6 < len && bytes[i + 1..].starts_with(b"style") {
                in_script = true;
            }
            if i + 7 < len && bytes[i + 1..].starts_with(b"/style") {
                in_script = false;
            }
        } else if b == b'>' {
            in_tag = false;
        } else if !in_tag && !in_script && !b.is_ascii_whitespace() {
            text_chars += 1;
        }
        i += 1;
    }

    text_chars
}

// ─── Publik API ────────────────────────────────────────────────────────────

/// Detektera om HTML är en bot-challenge/block istället för riktig content.
/// Kör alla 3 tiers i ordning. Returnerar vid första match.
pub fn detect(html: &str) -> BotDetectResult {
    let size = html.len();
    let html_lower = html.to_lowercase();

    // Tier 1: Specifika fingerprints (alla storlekar)
    if let Some(result) = check_tier1(&html_lower) {
        return result;
    }

    // Tier 2: Generiska termer (bara <10KB)
    if let Some(result) = check_tier2(&html_lower, size) {
        return result;
    }

    // Tier 3: Strukturell integritet (bara <50KB)
    if let Some(result) = check_tier3(html, &html_lower, size) {
        return result;
    }

    BotDetectResult::pass()
}

// ─── Tester ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier1_cloudflare() {
        let html = r#"<html><head><script src="/cdn-cgi/challenge-platform/h/b"></script></head><body>Checking...</body></html>"#;
        let result = detect(html);
        assert!(result.is_blocked, "Cloudflare challenge ska detekteras");
        assert_eq!(result.tier, Some(1));
        assert_eq!(result.system, Some("cloudflare"));
    }

    #[test]
    fn test_tier1_akamai() {
        let html = r#"<html><body><script>window._abck="abc"</script></body></html>"#;
        let result = detect(html);
        assert!(result.is_blocked, "Akamai bot manager ska detekteras");
        assert_eq!(result.system, Some("akamai"));
    }

    #[test]
    fn test_tier1_perimeterx() {
        let html = r#"<script>window._pxAppId="abc123"</script>"#;
        let result = detect(html);
        assert!(result.is_blocked, "PerimeterX ska detekteras");
        assert_eq!(result.system, Some("perimeterx"));
    }

    #[test]
    fn test_tier2_access_denied_small() {
        let html = "<html><body><h1>Access Denied</h1><p>You are blocked.</p></body></html>";
        let result = detect(html);
        assert!(result.is_blocked, "Liten access-denied-sida ska detekteras");
        assert_eq!(result.tier, Some(2));
    }

    #[test]
    fn test_tier2_skips_large_pages() {
        // Stor sida med "access denied" i content — ska INTE blockeras
        let mut html = "<html><body><h1>Access Denied Policy Updates</h1>".to_string();
        html.push_str(&"a".repeat(15_000)); // Gör sidan >10KB
        html.push_str("</body></html>");
        let result = detect(&html);
        assert!(
            !result.is_blocked,
            "Stor sida med 'access denied' ska INTE blockeras (page-size gate)"
        );
    }

    #[test]
    fn test_tier3_empty_shell() {
        let html = "<html><script>var app={};</script><script>render()</script>";
        let result = detect(html);
        assert!(result.is_blocked, "Tom SPA-shell ska detekteras");
        assert_eq!(result.tier, Some(3));
    }

    #[test]
    fn test_normal_page_passes() {
        let html = r#"<html><head><title>Nyheter</title></head><body>
            <h1>Senaste nyheterna</h1>
            <p>Idag hände det mycket. Regeringen presenterade ny budget med
            fokus på infrastruktur och klimatinvesteringar. Oppositionen
            kritiserade förslaget som otillräckligt.</p>
            <p>Mer nyheter kommer snart...</p>
        </body></html>"#;
        let result = detect(html);
        assert!(
            !result.is_blocked,
            "Normal nyhetssida ska INTE detekteras som bot-block"
        );
    }

    #[test]
    fn test_large_page_with_scripts_passes() {
        let mut html = "<html><body><h1>App</h1><p>Content here.</p>".to_string();
        html.push_str(&"<script>var x=1;</script>".repeat(500));
        html.push_str(&"<p>More real content about important topics.</p>".repeat(50));
        html.push_str("</body></html>");
        let result = detect(&html);
        assert!(
            !result.is_blocked,
            "Stor sida med scripts + content ska passera"
        );
    }
}
