// Fas 7: HTTP Fetch Integration
// Hämtning av webbsidor med cookie-stöd, redirects, robots.txt

use crate::types::{FetchConfig, FetchResult};
use std::time::Instant;

/// Hämta en webbsida och returnera HTML + metadata
pub async fn fetch_page(url: &str, config: &FetchConfig) -> Result<FetchResult, String> {
    let start = Instant::now();

    // Kontrollera robots.txt om konfigurerat
    if config.respect_robots_txt {
        check_robots_txt(url, &config.user_agent).await?;
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

    // Bygg request med extra headers
    let mut request = client.get(url);
    request = request.header("Accept", "text/html,application/xhtml+xml,*/*;q=0.8");
    request = request.header("Accept-Language", "en-US,en;q=0.9,sv;q=0.8");
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

    // Bygg redirect-kedja (reqwest hanterar redirects automatiskt,
    // vi loggar start- och slut-URL)
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

/// Kontrollera robots.txt för given URL
async fn check_robots_txt(url: &str, user_agent: &str) -> Result<(), String> {
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

    // Om robots.txt inte finns eller inte nås, tillåt
    let Ok(resp) = resp else {
        return Ok(());
    };

    if !resp.status().is_success() {
        return Ok(());
    }

    let body = resp.text().await.unwrap_or_default();
    let path = parsed.path();

    // Enkel robots.txt-parser: hitta User-agent: * eller vår agent
    let mut in_matching_section = false;
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }

        if let Some(agent) = line
            .strip_prefix("User-agent:")
            .or_else(|| line.strip_prefix("user-agent:"))
        {
            let agent = agent.trim();
            in_matching_section = agent == "*" || user_agent.contains(agent);
        } else if in_matching_section {
            if let Some(disallowed) = line
                .strip_prefix("Disallow:")
                .or_else(|| line.strip_prefix("disallow:"))
            {
                let disallowed = disallowed.trim();
                if !disallowed.is_empty() && path.starts_with(disallowed) {
                    return Err(format!("Blockerad av robots.txt: {disallowed}"));
                }
            }
        }
    }

    Ok(())
}

/// Validera att URL:en är rimlig (HTTP/HTTPS, inte lokal)
pub fn validate_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| format!("Ogiltig URL: {e}"))?;

    match parsed.scheme() {
        "http" | "https" => {}
        scheme => return Err(format!("Ogiltigt schema: {scheme} (bara http/https stöds)")),
    }

    if let Some(host) = parsed.host_str() {
        // Blockera interna adresser för säkerhet (SSRF-skydd)
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

#[cfg(test)]
mod tests {
    use super::*;

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
    }
}
