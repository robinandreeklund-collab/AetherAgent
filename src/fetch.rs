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

/// Hämta eller skapa en rate limiter för en domän (default 2 req/s)
fn get_rate_limiter(domain: &str, requests_per_second: u32) -> Arc<DomainLimiter> {
    let mut limiters = RATE_LIMITERS.lock().unwrap_or_else(|e| e.into_inner());
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

    // Bygg klient med konfiguration
    let client_builder = reqwest::Client::builder()
        .user_agent(&config.user_agent)
        .timeout(std::time::Duration::from_millis(config.timeout_ms))
        .redirect(reqwest::redirect::Policy::limited(
            config.max_redirects as usize,
        ))
        .cookie_store(true)
        .gzip(true)
        .brotli(true);

    let client = client_builder
        .build()
        .map_err(|e| format!("Kunde inte skapa HTTP-klient: {e}"))?;

    // Bygg request med realistiska headers
    let mut request = client.get(url);
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

    // Läs body
    let body = response
        .text()
        .await
        .map_err(|e| format!("Kunde inte läsa body: {e}"))?;

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
async fn check_robots_txt_google(url: &str, user_agent: &str) -> Result<(), String> {
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
            || host.starts_with("172.16.")
        {
            return Err(format!("Blockerad: interna adresser tillåts inte ({host})"));
        }
    } else {
        return Err("URL saknar host".to_string());
    }

    Ok(())
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

/// Hämta externa CSS-filer (<link rel="stylesheet">) och inlina dem som <style>-taggar.
///
/// Blitz (ren Rust-renderer) kan inte ladda externa resurser tillförlitligt.
/// Denna funktion prefetchar alla CSS-länkar parallellt och ersätter
/// <link>-taggarna med <style>-block i HTML:en.
///
/// Returnerar modifierad HTML med inlinad CSS.
pub async fn inline_external_css(html: &str, base_url: &str) -> String {
    // Hitta alla <link rel="stylesheet" href="...">
    let css_links = extract_css_links(html, base_url);
    if css_links.is_empty() {
        return html.to_string();
    }

    // Hämta alla CSS-filer parallellt (max 3s per fil)
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new());

    // Hämta CSS sekventiellt (undviker futures-dependency, sällan >3-4 filer)
    let mut results: Vec<Option<String>> = Vec::with_capacity(css_links.len());
    for link in &css_links {
        let css = match client.get(&link.url).send().await {
            Ok(resp) => resp.text().await.ok(),
            Err(_) => None,
        };
        results.push(css);
    }

    // Bygg inlinad CSS
    let mut inlined_css = String::new();
    for (i, css_text) in results.iter().enumerate() {
        if let Some(css) = css_text {
            // Begränsa storlek (max 500KB per fil)
            if css.len() <= 512_000 {
                inlined_css.push_str(&format!(
                    "\n<style data-inlined-from=\"{}\">\n{}\n</style>\n",
                    css_links[i].url, css
                ));
            }
        }
    }

    if inlined_css.is_empty() {
        return html.to_string();
    }

    // Injicera inlinad CSS före </head> (eller i början av <body>)
    let result = if let Some(pos) = html.find("</head>") {
        format!("{}{}{}", &html[..pos], inlined_css, &html[pos..])
    } else if let Some(pos) = html.find("<body") {
        format!("{}{}{}", &html[..pos], inlined_css, &html[pos..])
    } else {
        format!("{}{}", inlined_css, html)
    };

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
    if href.starts_with('/') {
        // Absolut sökväg — extrahera origin
        if let Some(origin_end) = base_url
            .find("://")
            .and_then(|i| base_url[i + 3..].find('/').map(|j| i + 3 + j))
        {
            return format!("{}{}", &base_url[..origin_end], href);
        }
        return format!("{}{}", base_url.trim_end_matches('/'), href);
    }
    // Relativ sökväg
    let base_dir = if let Some(last_slash) = base_url.rfind('/') {
        &base_url[..last_slash + 1]
    } else {
        base_url
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
