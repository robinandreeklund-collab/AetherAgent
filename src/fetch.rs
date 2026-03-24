// Fas 7+8: HTTP Fetch Integration + Ethical Engine
// Hämtning av webbsidor med cookie-stöd, redirects, robots.txt, rate limiting
//
// Fas 7: Grundläggande fetch med reqwest
// Fas 8: Google robots.txt-parser (robotstxt crate), governor rate limiter,
//         Semantic Firewall-integration

use crate::types::{FetchConfig, FetchResult};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

// ─── Rate Limiter (per domän) ────────────────────────────────────────────────

use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::RateLimiter;
use std::num::NonZeroU32;
use std::sync::Arc;

type DomainLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Global rate limiter per domän (lazy initialized)
static RATE_LIMITERS: std::sync::LazyLock<Mutex<HashMap<String, Arc<DomainLimiter>>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Återanvändbar HTTP-klient — undvik att bygga ny TLS-session per request.
/// Konfigureras med rimliga defaults (10s timeout, gzip/brotli).
/// OBS: cookie_store(true) togs bort — den ackumulerade cookies från alla domäner
/// utan eviction och orsakade OOM (49 MB → 23 GB). Cookies hanteras istället
/// per-session via SessionManager.
static SHARED_CLIENT: std::sync::LazyLock<reqwest::Client> = std::sync::LazyLock::new(|| {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .gzip(true)
        .brotli(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

// Minnesgräns: max antal domäner i rate limiter cache
const MAX_RATE_LIMITER_DOMAINS: usize = 1_000;

/// Hämta eller skapa en rate limiter för en domän (default 2 req/s)
fn get_rate_limiter(domain: &str, requests_per_second: u32) -> Arc<DomainLimiter> {
    let mut limiters = RATE_LIMITERS.lock().unwrap_or_else(|e| e.into_inner());

    // Evicta slumpmässig domän om vi når gränsen (billig operation)
    if !limiters.contains_key(domain) && limiters.len() >= MAX_RATE_LIMITER_DOMAINS {
        // Ta bort en godtycklig entry (HashMap iteration order)
        if let Some(old_key) = limiters.keys().next().cloned() {
            limiters.remove(&old_key);
        }
    }

    limiters
        .entry(domain.to_string())
        .or_insert_with(|| {
            let rps = NonZeroU32::new(requests_per_second.max(1)).unwrap_or(NonZeroU32::MIN);
            Arc::new(RateLimiter::direct(governor::Quota::per_second(rps)))
        })
        .clone()
}

/// Vänta tills rate limiter tillåter request
async fn wait_for_rate_limit(domain: &str, rps: u32) {
    let limiter = get_rate_limiter(domain, rps);
    limiter.until_ready().await;
}

// ─── Huvudfunktion: fetch_page ──────────────────────────────────────────────

/// Hämta en webbsida och returnera HTML + metadata
pub async fn fetch_page(url: &str, config: &FetchConfig) -> Result<FetchResult, String> {
    let start = Instant::now();

    // Extrahera domän för rate limiting
    let domain = extract_domain(url).unwrap_or_default();

    // Respektera rate limit (default 2 req/s per domän)
    wait_for_rate_limit(&domain, config.rate_limit_rps).await;

    // Kontrollera robots.txt med Googles parser om konfigurerat
    if config.respect_robots_txt {
        check_robots_txt_google(url, &config.user_agent).await?;
    }

    // Återanvänd global klient (undviker TLS-handshake + connection pool setup per request)
    let client = &*SHARED_CLIENT;

    // Bygg request med realistiska headers
    let mut request = client.get(url);
    request = request.header("User-Agent", &config.user_agent);
    request = request.header(
        "Accept",
        "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    );
    request = request.header("Accept-Language", "en-US,en;q=0.9,sv;q=0.8");
    request = request.header("Accept-Encoding", "gzip, deflate, br");
    request = request.header("DNT", "1");
    request = request.header("Sec-Fetch-Dest", "document");
    request = request.header("Sec-Fetch-Mode", "navigate");
    request = request.header("Sec-Fetch-Site", "none");
    request = request.header("Sec-Fetch-User", "?1");
    request = request.header("Upgrade-Insecure-Requests", "1");
    for (key, value) in &config.extra_headers {
        request = request.header(key.as_str(), value.as_str());
    }

    // Utför request
    let response = request
        .send()
        .await
        .map_err(|e| format!("Fetch misslyckades: {e}"))?;

    let status_code = response.status().as_u16();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // Hantera Retry-After header (429/503)
    if status_code == 429 || status_code == 503 {
        if let Some(retry_after) = response
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
        {
            return Err(format!(
                "Rate limited ({status_code}): Retry-After {retry_after}s"
            ));
        }
        return Err(format!("Rate limited ({status_code}): försök igen senare"));
    }

    // Bygg redirect-kedja
    let mut redirect_chain = Vec::new();
    if final_url != url {
        redirect_chain.push(url.to_string());
        redirect_chain.push(final_url.clone());
    }

    // Läs body med storleksgräns (max 20 MB) för att förhindra OOM
    const MAX_BODY_SIZE: usize = 20 * 1024 * 1024;

    // Kolla Content-Length INNAN vi allokerar — avvisa tidigt om servern annonserar
    // en body större än gränsen (förhindrar OOM vid stora filer)
    if let Some(cl) = response.content_length() {
        if cl as usize > MAX_BODY_SIZE {
            return Err(format!(
                "Svar för stort enligt Content-Length: {cl} bytes (max {MAX_BODY_SIZE})"
            ));
        }
    }

    let body_bytes = response
        .bytes()
        .await
        .map_err(|e| format!("Kunde inte läsa body: {e}"))?;
    if body_bytes.len() > MAX_BODY_SIZE {
        return Err(format!(
            "Svar för stort: {} bytes (max {MAX_BODY_SIZE})",
            body_bytes.len()
        ));
    }
    let body = String::from_utf8_lossy(&body_bytes).into_owned();
    let body_size_bytes = body.len();
    let fetch_time_ms = start.elapsed().as_millis() as u64;

    Ok(FetchResult {
        final_url,
        status_code,
        content_type,
        body,
        redirect_chain,
        fetch_time_ms,
        body_size_bytes,
    })
}

// ─── robots.txt med Googles parser ──────────────────────────────────────────

/// Kontrollera robots.txt med Googles officiella parser (robotstxt crate)
pub async fn check_robots_txt_google(url: &str, user_agent: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Ogiltig URL: {e}"))?;
    let robots_url = format!(
        "{}://{}/robots.txt",
        parsed.scheme(),
        parsed.host_str().unwrap_or("")
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| format!("robots.txt klient-fel: {e}"))?;

    let resp = client.get(&robots_url).send().await;

    // Om robots.txt inte finns eller inte nås, tillåt (RFC 9309)
    let Ok(resp) = resp else {
        return Ok(());
    };

    if !resp.status().is_success() {
        return Ok(());
    }

    let body = resp.text().await.unwrap_or_default();
    let path = parsed.path();

    // Använd Googles officiella parser
    let mut matcher = robotstxt::DefaultMatcher::default();
    if !matcher.one_agent_allowed_by_robots(&body, user_agent, path) {
        return Err(format!(
            "Blockerad av robots.txt för '{user_agent}' på '{path}'"
        ));
    }

    Ok(())
}

// ─── URL-validering (SSRF-skydd) ────────────────────────────────────────────

/// Validera att URL:en är rimlig (HTTP/HTTPS, inte lokal)
pub fn validate_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Ogiltig URL: {e}"))?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Ogiltigt schema: {scheme} (bara http/https stöds)")),
    }

    if let Some(host) = parsed.host_str() {
        if host == "localhost"
            || host == "127.0.0.1"
            || host == "::1"
            || host == "0.0.0.0"
            || host.starts_with("10.")
            || host.starts_with("192.168.")
            || is_rfc1918_172(host)
        {
            return Err(format!("Blockerad: interna adresser tillåts inte ({host})"));
        }
    } else {
        return Err("URL saknar host".to_string());
    }

    Ok(())
}

/// Kontrollera om en host är i RFC 1918 172.16.0.0/12-rangen (172.16.0.0–172.31.255.255)
fn is_rfc1918_172(host: &str) -> bool {
    if let Some(rest) = host.strip_prefix("172.") {
        if let Some(second_octet_str) = rest.split('.').next() {
            if let Ok(second_octet) = second_octet_str.parse::<u8>() {
                return (16..=31).contains(&second_octet);
            }
        }
    }
    false
}

// ─── Hjälpfunktioner ────────────────────────────────────────────────────────

/// Extrahera domän från URL
fn extract_domain(url: &str) -> Option<String> {
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))?;
    let host = without_scheme.split('/').next()?.split(':').next()?;
    Some(host.to_lowercase())
}

// ─── CSS Inlining för Blitz-rendering ──────────────────────────────────────

/// Resultat av CSS-inlining med detaljerad felrapportering
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CssInlineResult {
    /// Modifierad HTML med inlinad CSS
    pub html: String,
    /// Antal CSS-filer som hittades
    pub css_found: usize,
    /// Antal CSS-filer som laddades OK
    pub css_loaded: usize,
    /// Antal CSS-filer som misslyckades
    pub css_failed: usize,
    /// Detaljer per CSS-fil
    pub css_details: Vec<CssFileStatus>,
    /// Totalt antal CSS-bytes som inlinades
    pub css_bytes_added: usize,
}

/// Status för en enskild CSS-fil
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CssFileStatus {
    /// CSS-filens URL
    pub url: String,
    /// Om filen laddades OK
    pub loaded: bool,
    /// Storlek i bytes (0 om misslyckad)
    pub size_bytes: usize,
    /// Felmeddelande (None om OK)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Hämta externa CSS-filer (<link rel="stylesheet">) och inlina dem som <style>-taggar.
///
/// Blitz (ren Rust-renderer) kan inte ladda externa resurser tillförlitligt.
/// Denna funktion prefetchar alla CSS-länkar parallellt och ersätter
/// <link>-taggarna med <style>-block i HTML:en.
///
/// Returnerar CssInlineResult med modifierad HTML och detaljerad felrapportering.
pub async fn inline_external_css_detailed(html: &str, base_url: &str) -> CssInlineResult {
    let mut css_links = extract_css_links(html, base_url);
    let css_found = css_links.len();

    if css_links.is_empty() {
        return CssInlineResult {
            html: html.to_string(),
            css_found: 0,
            css_loaded: 0,
            css_failed: 0,
            css_details: vec![],
            css_bytes_added: 0,
        };
    }

    const MAX_CSS_LINKS: usize = 50;
    css_links.truncate(MAX_CSS_LINKS);

    const MAX_CSS_BYTES: usize = 2 * 1024 * 1024;

    // Parallell CSS-hämtning med tokio tasks — nu med felrapportering
    let mut handles = Vec::with_capacity(css_links.len());
    for link in &css_links {
        let client = SHARED_CLIENT.clone();
        let url = link.url.clone();
        handles.push(tokio::spawn(async move {
            match client.get(&url).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if !status.is_success() {
                        return Err(format!("HTTP {status}"));
                    }
                    match resp.bytes().await {
                        Ok(bytes) => {
                            if bytes.len() > MAX_CSS_BYTES {
                                return Err(format!(
                                    "För stor: {} bytes (max {MAX_CSS_BYTES})",
                                    bytes.len()
                                ));
                            }
                            match String::from_utf8(bytes.to_vec()) {
                                Ok(s) => Ok(s),
                                Err(_) => Err("Ej giltig UTF-8".to_string()),
                            }
                        }
                        Err(e) => Err(format!("Body-läsning misslyckades: {e}")),
                    }
                }
                Err(e) => Err(format!("Nätverksfel: {e}")),
            }
        }));
    }

    let mut results: Vec<Result<String, String>> = Vec::with_capacity(handles.len());
    for handle in handles {
        match handle.await {
            Ok(r) => results.push(r),
            Err(e) => results.push(Err(format!("Task-fel: {e}"))),
        }
    }

    let mut css_details = Vec::with_capacity(results.len());
    let mut css_loaded = 0usize;
    let mut css_failed = 0usize;
    let mut css_bytes_added = 0usize;

    let estimated_size: usize = results
        .iter()
        .filter_map(|r| r.as_ref().ok())
        .map(|s| s.len() + 80)
        .sum();
    let mut inlined_css = String::with_capacity(estimated_size);

    // Begränsa total CSS-budget: HTML + inlinad CSS får ej överstiga 2MB
    // (github: 569KB HTML + 842KB CSS = 1.4MB OK, men 569KB + 2MB CSS = OOM)
    const MAX_TOTAL_CSS_BUDGET: usize = 1500 * 1024; // 1.5MB total CSS

    for (i, css_result) in results.iter().enumerate() {
        match css_result {
            Ok(css) => {
                if css.len() <= 512_000 && css_bytes_added + css.len() <= MAX_TOTAL_CSS_BUDGET {
                    inlined_css.push_str(&format!(
                        "\n<style data-inlined-from=\"{}\">\n{}\n</style>\n",
                        css_links[i].url, css
                    ));
                    css_bytes_added += css.len();
                    css_details.push(CssFileStatus {
                        url: css_links[i].url.clone(),
                        loaded: true,
                        size_bytes: css.len(),
                        error: None,
                    });
                    css_loaded += 1;
                } else if css_bytes_added + css.len() > MAX_TOTAL_CSS_BUDGET {
                    css_details.push(CssFileStatus {
                        url: css_links[i].url.clone(),
                        loaded: false,
                        size_bytes: css.len(),
                        error: Some(format!(
                            "CSS-budget överstigen: +{} bytes > {MAX_TOTAL_CSS_BUDGET}",
                            css.len()
                        )),
                    });
                    css_failed += 1;
                } else {
                    css_details.push(CssFileStatus {
                        url: css_links[i].url.clone(),
                        loaded: false,
                        size_bytes: css.len(),
                        error: Some(format!("Trunkerad: {} bytes > 512KB", css.len())),
                    });
                    css_failed += 1;
                }
            }
            Err(e) => {
                eprintln!("[CSS] Misslyckad: {} — {}", css_links[i].url, e);
                css_details.push(CssFileStatus {
                    url: css_links[i].url.clone(),
                    loaded: false,
                    size_bytes: 0,
                    error: Some(e.clone()),
                });
                css_failed += 1;
            }
        }
    }

    let html_out = if inlined_css.is_empty() {
        html.to_string()
    } else if let Some(pos) = html.find("</head>") {
        format!("{}{}{}", &html[..pos], inlined_css, &html[pos..])
    } else if let Some(pos) = html.find("<body") {
        format!("{}{}{}", &html[..pos], inlined_css, &html[pos..])
    } else {
        format!("{}{}", inlined_css, html)
    };

    CssInlineResult {
        html: html_out,
        css_found,
        css_loaded,
        css_failed,
        css_details,
        css_bytes_added,
    }
}

/// Bakåtkompatibel wrapper — returnerar bara HTML-strängen
pub async fn inline_external_css(html: &str, base_url: &str) -> String {
    inline_external_css_detailed(html, base_url).await.html
}

// ─── Extern JS-hämtning (SPA-stöd) ──────────────────────────────────────────

/// Resultat från extern script-inlining
#[derive(Debug, Clone)]
pub struct JsInlineResult {
    /// HTML med externa scripts ersatta av inlinade scripts
    pub html: String,
    /// Antal externa scripts hittade
    pub scripts_found: usize,
    /// Antal scripts som hämtades
    pub scripts_loaded: usize,
    /// Antal scripts som misslyckades
    pub scripts_failed: usize,
    /// Total JS-storlek inlinad (bytes)
    pub js_bytes_added: usize,
}

/// Hämta externa `<script src="...">` filer och inlina dem i HTML:en.
///
/// Ersätter `<script src="bundle.js"></script>` med `<script>/* hämtad kod */</script>`.
/// Detta gör att den befintliga JS-lifecycle-pipelinen (QuickJS + DOM bridge)
/// kan köra SPA-bundles som bygger upp sidan.
///
/// Begränsningar:
/// - Max 20 externa scripts
/// - Max 3 MB per script-fil
/// - Max 8 MB totalt
/// - 10 sekunders timeout per hämtning
pub async fn fetch_and_inline_external_scripts(html: &str, base_url: &str) -> JsInlineResult {
    #[cfg(not(feature = "js-eval"))]
    {
        let _ = base_url;
        return JsInlineResult {
            html: html.to_string(),
            scripts_found: 0,
            scripts_loaded: 0,
            scripts_failed: 0,
            js_bytes_added: 0,
        };
    }

    #[cfg(feature = "js-eval")]
    {
        use crate::js_eval::{extract_all_scripts, ScriptEntry};

        const MAX_SCRIPTS: usize = 10;
        const MAX_SCRIPT_SIZE: usize = 512 * 1024; // 512KB per fil
        const MAX_TOTAL_SIZE: usize = 1024 * 1024; // 1MB totalt
                                                   // Skippa JS-inlining för redan stora sidor — Blitz klarar inte >2MB HTML
        const MAX_HTML_FOR_JS_INLINE: usize = 500 * 1024;

        if html.len() > MAX_HTML_FOR_JS_INLINE {
            return JsInlineResult {
                html: html.to_string(),
                scripts_found: 0,
                scripts_loaded: 0,
                scripts_failed: 0,
                js_bytes_added: 0,
            };
        }

        let entries = extract_all_scripts(html, base_url);
        let external_urls: Vec<(usize, String)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| match e {
                ScriptEntry::External(url) => Some((i, url.clone())),
                ScriptEntry::Inline => None,
            })
            .take(MAX_SCRIPTS)
            .collect();

        let scripts_found = external_urls.len();
        if scripts_found == 0 {
            return JsInlineResult {
                html: html.to_string(),
                scripts_found: 0,
                scripts_loaded: 0,
                scripts_failed: 0,
                js_bytes_added: 0,
            };
        }

        // Parallell hämtning av externa scripts
        let mut handles = Vec::with_capacity(external_urls.len());
        for (_idx, url) in &external_urls {
            let client = SHARED_CLIENT.clone();
            let url = url.clone();
            handles.push(tokio::spawn(async move {
                let resp = client
                    .get(&url)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await;
                match resp {
                    Ok(r) if r.status().is_success() => match r.bytes().await {
                        Ok(bytes) if bytes.len() <= MAX_SCRIPT_SIZE => {
                            String::from_utf8(bytes.to_vec()).ok()
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }));
        }

        // Samla resultat
        let mut fetched: Vec<Option<String>> = Vec::with_capacity(handles.len());
        for handle in handles {
            fetched.push(handle.await.ok().flatten());
        }

        // Bygg URL → kod-mappning
        let mut url_to_code: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
        let mut total_bytes = 0usize;
        let mut scripts_loaded = 0usize;
        let mut scripts_failed = 0usize;

        for (i, (_idx, url)) in external_urls.iter().enumerate() {
            if let Some(Some(code)) = fetched.get(i) {
                if total_bytes + code.len() <= MAX_TOTAL_SIZE {
                    total_bytes += code.len();
                    url_to_code.insert(url.as_str(), code.as_str());
                    scripts_loaded += 1;
                } else {
                    scripts_failed += 1;
                }
            } else {
                scripts_failed += 1;
            }
        }

        if scripts_loaded == 0 {
            return JsInlineResult {
                html: html.to_string(),
                scripts_found,
                scripts_loaded: 0,
                scripts_failed,
                js_bytes_added: 0,
            };
        }

        // Ersätt <script src="URL"></script> med <script>KOD</script> i HTML:en
        let html_out = replace_external_scripts_with_inline(html, base_url, &url_to_code);

        JsInlineResult {
            html: html_out,
            scripts_found,
            scripts_loaded,
            scripts_failed,
            js_bytes_added: total_bytes,
        }
    }
}

/// Ersätt externa script-taggar med inlinade versioner
#[cfg(feature = "js-eval")]
fn replace_external_scripts_with_inline(
    html: &str,
    base_url: &str,
    url_to_code: &std::collections::HashMap<&str, &str>,
) -> String {
    use crate::js_eval::extract_all_scripts;
    use crate::js_eval::ScriptEntry;

    let entries = extract_all_scripts(html, base_url);
    let mut result =
        String::with_capacity(html.len() + url_to_code.values().map(|v| v.len()).sum::<usize>());
    let lower = html.to_lowercase();
    let mut last_pos = 0;
    let mut search_from = 0;
    let mut entry_idx = 0;

    while let Some(start) = lower[search_from..].find("<script") {
        let abs_start = search_from + start;
        if let Some(tag_end_offset) = lower[abs_start..].find('>') {
            let tag_end = abs_start + tag_end_offset + 1;
            if let Some(close_offset) = lower[tag_end..].find("</script>") {
                let close_end = tag_end + close_offset + 9; // 9 = "</script>".len()
                let tag_text = &lower[abs_start..abs_start + tag_end_offset];

                // Matcha mot entries — kontrollera att det är en extern script
                let is_json = tag_text.contains("application/json")
                    || tag_text.contains("application/ld+json")
                    || tag_text.contains("importmap");
                let is_external = tag_text.contains("src=");

                if !is_json && is_external {
                    if let Some(entry) = entries.get(entry_idx) {
                        if let ScriptEntry::External(url) = entry {
                            if let Some(code) = url_to_code.get(url.as_str()) {
                                // Ersätt hela <script src="...">...</script> med <script>KOD</script>
                                result.push_str(&html[last_pos..abs_start]);
                                result.push_str("<script>");
                                // Escape </script> i JS-koden
                                result.push_str(
                                    &code
                                        .replace("</script>", "<\\/script>")
                                        .replace("</Script>", "<\\/Script>")
                                        .replace("</SCRIPT>", "<\\/SCRIPT>"),
                                );
                                result.push_str("</script>");
                                last_pos = close_end;
                            }
                        }
                        entry_idx += 1;
                    }
                } else if !is_json && !is_external {
                    // Inline script — räkna entry-index
                    let code = html[tag_end..tag_end + close_offset].trim();
                    if !code.is_empty() {
                        entry_idx += 1;
                    }
                }

                search_from = close_end;
            } else {
                break;
            }
        } else {
            break;
        }
    }

    result.push_str(&html[last_pos..]);
    result
}

/// Intern: extrahera CSS-länk-URLer från HTML
struct CssLink {
    url: String,
}

fn extract_css_links(html: &str, base_url: &str) -> Vec<CssLink> {
    let mut links = Vec::new();
    let lower = html.to_lowercase();

    // Enkel regex-fri parser: hitta <link ... rel="stylesheet" ... href="...">
    let mut search_from = 0;
    while let Some(link_start) = lower[search_from..].find("<link") {
        let abs_start = search_from + link_start;
        let tag_end = match lower[abs_start..].find('>') {
            Some(e) => abs_start + e,
            None => break,
        };
        let tag = &html[abs_start..=tag_end];
        let tag_lower = &lower[abs_start..=tag_end];

        // Kolla att det är stylesheet
        if tag_lower.contains("stylesheet") || tag_lower.contains("text/css") {
            if let Some(href) = extract_href(tag) {
                let url = resolve_url(base_url, &href);
                if url.starts_with("http://") || url.starts_with("https://") {
                    links.push(CssLink { url });
                }
            }
        }
        search_from = tag_end + 1;
    }
    links
}

/// Intern: plocka ut href-attribut ur en tag-sträng
fn extract_href(tag: &str) -> Option<String> {
    // Hitta href="..." eller href='...'
    let lower = tag.to_lowercase();
    let href_pos = lower.find("href=")?;
    let after_href = &tag[href_pos + 5..];

    if let Some(stripped) = after_href.strip_prefix('"') {
        let end = stripped.find('"')?;
        Some(stripped[..end].to_string())
    } else if let Some(stripped) = after_href.strip_prefix('\'') {
        let end = stripped.find('\'')?;
        Some(stripped[..end].to_string())
    } else {
        // href=value (utan citattecken, ovanligt men giltigt)
        let end = after_href.find([' ', '>', '/'])?;
        Some(after_href[..end].to_string())
    }
}

/// Intern: resolva relativa URLer
fn resolve_url(base_url: &str, href: &str) -> String {
    // Avkoda HTML-entiteter i href (&amp; → &)
    let href = href.replace("&amp;", "&");
    let href = href.as_str();

    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    if href.starts_with("//") {
        // Protokoll-relativ
        let scheme = if base_url.starts_with("https://") {
            "https:"
        } else {
            "http:"
        };
        return format!("{}{}", scheme, href);
    }

    // Extrahera origin (scheme + host) — behövs för både absolut och relativ
    let origin_end = base_url
        .find("://")
        .and_then(|i| base_url[i + 3..].find('/').map(|j| i + 3 + j));

    if href.starts_with('/') {
        // Absolut sökväg
        if let Some(oe) = origin_end {
            return format!("{}{}", &base_url[..oe], href);
        }
        return format!("{}{}", base_url.trim_end_matches('/'), href);
    }

    // Relativ sökväg — hitta sista '/' EFTER origin
    // BUG-FIX: utan detta kunde "https://news.ycombinator.com" (ingen trailing /)
    // ge base_dir = "https://" (hittade / i ://) → felaktiga URLer
    let base_dir = if let Some(oe) = origin_end {
        // base_url har sökväg — hitta sista / i sökvägen
        if let Some(last_slash) = base_url[oe..].rfind('/') {
            &base_url[..oe + last_slash + 1]
        } else {
            // Sökväg utan / → använd origin + /
            &base_url[..oe + 1]
        }
    } else {
        // Bara origin utan sökväg — lägg till /
        // "https://example.com" → "https://example.com/"
        return format!("{}/{}", base_url.trim_end_matches('/'), href);
    };
    format!("{}{}", base_dir, href)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FetchConfig;

    #[test]
    fn test_validate_url_valid() {
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("https://shop.se/products?q=test").is_ok());
        assert!(validate_url("http://example.com/path").is_ok());
    }

    #[test]
    fn test_validate_url_blocked_internal() {
        assert!(
            validate_url("http://localhost:3000").is_err(),
            "Ska blockera localhost"
        );
        assert!(
            validate_url("http://127.0.0.1/admin").is_err(),
            "Ska blockera 127.0.0.1"
        );
        assert!(
            validate_url("http://192.168.1.1").is_err(),
            "Ska blockera privata IP"
        );
        assert!(
            validate_url("http://10.0.0.1/secret").is_err(),
            "Ska blockera 10.x.x.x"
        );
    }

    #[test]
    fn test_validate_url_blocked_172_full_range() {
        // Fas A.3: Kontrollera hela 172.16.0.0/12 (172.16–172.31)
        assert!(
            validate_url("http://172.16.0.1").is_err(),
            "Ska blockera 172.16.x.x"
        );
        assert!(
            validate_url("http://172.20.0.1").is_err(),
            "Ska blockera 172.20.x.x"
        );
        assert!(
            validate_url("http://172.31.255.255").is_err(),
            "Ska blockera 172.31.x.x"
        );
        // 172.15 och 172.32 är INTE privata
        assert!(
            validate_url("http://172.15.0.1").is_ok(),
            "Ska INTE blockera 172.15.x.x (ej RFC 1918)"
        );
        assert!(
            validate_url("http://172.32.0.1").is_ok(),
            "Ska INTE blockera 172.32.x.x (ej RFC 1918)"
        );
    }

    #[test]
    fn test_validate_url_bad_scheme() {
        assert!(
            validate_url("ftp://example.com").is_err(),
            "Ska blockera ftp"
        );
        assert!(
            validate_url("file:///etc/passwd").is_err(),
            "Ska blockera file://"
        );
    }

    #[test]
    fn test_validate_url_invalid() {
        assert!(
            validate_url("not-a-url").is_err(),
            "Ska avvisa ogiltiga URL:er"
        );
    }

    #[test]
    fn test_fetch_config_defaults() {
        let config = FetchConfig::default();
        assert_eq!(config.timeout_ms, 10_000);
        assert_eq!(config.max_redirects, 10);
        assert!(!config.respect_robots_txt);
        assert!(config.user_agent.contains("AetherAgent"));
        assert_eq!(config.rate_limit_rps, 2);
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://example.com/path"),
            Some("example.com".to_string())
        );
        assert_eq!(
            extract_domain("http://shop.se:8080/api"),
            Some("shop.se".to_string())
        );
        assert_eq!(extract_domain("invalid"), None);
    }

    #[test]
    fn test_rate_limiter_creation() {
        let limiter1 = get_rate_limiter("test-domain.com", 5);
        let limiter2 = get_rate_limiter("test-domain.com", 5);
        // Samma instans
        assert!(Arc::ptr_eq(&limiter1, &limiter2));

        let limiter3 = get_rate_limiter("other-domain.com", 10);
        // Olika instans
        assert!(!Arc::ptr_eq(&limiter1, &limiter3));
    }

    #[test]
    fn test_robots_txt_google_parser() {
        // Testar Googles parser direkt (utan nätverk)
        let robots_body = "User-agent: *\nDisallow: /admin\nDisallow: /private\n";
        let mut matcher = robotstxt::DefaultMatcher::default();

        assert!(
            matcher.one_agent_allowed_by_robots(robots_body, "AetherAgent", "/products"),
            "Borde tillåta /products"
        );
        assert!(
            !matcher.one_agent_allowed_by_robots(robots_body, "AetherAgent", "/admin"),
            "Borde blockera /admin"
        );
        assert!(
            !matcher.one_agent_allowed_by_robots(robots_body, "AetherAgent", "/private/data"),
            "Borde blockera /private/data"
        );
    }

    #[test]
    fn test_robots_txt_specific_agent() {
        let robots_body = "User-agent: AetherAgent\nDisallow: /secret\n\nUser-agent: *\nAllow: /\n";
        let mut matcher = robotstxt::DefaultMatcher::default();

        assert!(
            !matcher.one_agent_allowed_by_robots(robots_body, "AetherAgent", "/secret"),
            "Borde blockera /secret för AetherAgent"
        );
        assert!(
            matcher.one_agent_allowed_by_robots(robots_body, "OtherBot", "/secret"),
            "Borde tillåta /secret för andra botar"
        );
    }

    #[test]
    fn test_extract_css_links_finds_stylesheets() {
        let html = r#"<html><head>
            <link rel="stylesheet" href="/css/main.css">
            <link rel="stylesheet" href="https://cdn.example.com/style.css">
            <link rel="icon" href="/favicon.ico">
        </head><body></body></html>"#;
        let links = super::extract_css_links(html, "https://www.hjo.se");
        assert_eq!(links.len(), 2, "Borde hitta 2 CSS-länkar");
        assert_eq!(links[0].url, "https://www.hjo.se/css/main.css");
        assert_eq!(links[1].url, "https://cdn.example.com/style.css");
    }

    #[test]
    fn test_extract_css_links_no_stylesheets() {
        let html = "<html><head><title>Hej</title></head><body></body></html>";
        let links = super::extract_css_links(html, "https://example.com");
        assert!(links.is_empty(), "Borde inte hitta CSS-länkar i enkel HTML");
    }

    #[test]
    fn test_resolve_url_absolute() {
        assert_eq!(
            super::resolve_url("https://www.hjo.se/sida", "https://cdn.example.com/s.css"),
            "https://cdn.example.com/s.css"
        );
    }

    #[test]
    fn test_resolve_url_relative_root() {
        assert_eq!(
            super::resolve_url("https://www.hjo.se/sida/info", "/css/main.css"),
            "https://www.hjo.se/css/main.css"
        );
    }

    #[test]
    fn test_resolve_url_protocol_relative() {
        assert_eq!(
            super::resolve_url("https://www.hjo.se", "//cdn.example.com/s.css"),
            "https://cdn.example.com/s.css"
        );
    }

    #[test]
    fn test_extract_href_double_quotes() {
        assert_eq!(
            super::extract_href(r#"<link rel="stylesheet" href="/style.css">"#),
            Some("/style.css".to_string())
        );
    }

    #[test]
    fn test_extract_href_single_quotes() {
        assert_eq!(
            super::extract_href("<link rel='stylesheet' href='/style.css'>"),
            Some("/style.css".to_string())
        );
    }
}
