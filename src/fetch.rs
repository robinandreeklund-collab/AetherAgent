// Fas 7+8: HTTP Fetch Integration + Ethical Engine
// Hämtning av webbsidor med cookie-stöd, redirects, robots.txt, rate limiting
//
// Fas 7: Grundläggande fetch med reqwest
// Fas 8: Google robots.txt-parser (robotstxt crate), governor rate limiter,
//         Semantic Firewall-integration

use crate::types::{FetchConfig, FetchResult};
use std::collections::HashMap;
use std::error::Error as StdError;
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
        .map_err(|e| {
            let mut msg = format!("Fetch misslyckades: {e}");
            let mut source = e.source();
            while let Some(cause) = source {
                msg.push_str(&format!(" -> {cause}"));
                source = cause.source();
            }
            msg
        })?;

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
        let matcher = robotstxt::DefaultMatcher::default();

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
        let matcher = robotstxt::DefaultMatcher::default();

        assert!(
            !matcher.one_agent_allowed_by_robots(robots_body, "AetherAgent", "/secret"),
            "Borde blockera /secret för AetherAgent"
        );
        assert!(
            matcher.one_agent_allowed_by_robots(robots_body, "OtherBot", "/secret"),
            "Borde tillåta /secret för andra botar"
        );
    }
}
