// DDG Search Layer – Fas 17
//
// Sökmodul som använder DuckDuckGo HTML-sök för att hitta relevanta sidor.
// Återanvänder befintlig fetch + stream_parse pipeline.
// Ingen JavaScript krävs – DDG HTML är ren HTML, Blitz Tier 1 räcker.

use crate::types::SemanticNode;
use serde::{Deserialize, Serialize};

// ─── Typer ───────────────────────────────────────────────────────────────────

/// En semantisk nod extraherad från en resultat-sida (deep fetch)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageNode {
    pub role: String,
    pub label: String,
    pub relevance: f32,
}

/// Ett sökresultat från DDG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResultEntry {
    pub rank: usize,
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub domain: String,
    pub confidence: f32,
    /// Semantiska noder från själva sidan (fylls av deep fetch)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_content: Option<Vec<PageNode>>,
    /// Fetch-tid i ms för deep fetch av denna sida
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fetch_ms: Option<u64>,
}

/// Komplett svar från search()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub query: String,
    pub results: Vec<SearchResultEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direct_answer: Option<String>,
    pub direct_answer_confidence: f32,
    pub source_url: String,
    pub parse_ms: u64,
    pub nodes_seen: usize,
    pub nodes_emitted: usize,
    /// Indikerar om deep fetch utfördes
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep: Option<bool>,
    /// Total tid för alla deep fetches i ms
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deep_fetch_ms: Option<u64>,
}

// ─── DDG URL-byggare ─────────────────────────────────────────────────────────

/// Bygg DuckDuckGo Lite-sök-URL (lägre risk för CAPTCHA)
pub fn build_ddg_url(query: &str) -> String {
    let encoded = percent_encode(query);
    format!("https://lite.duckduckgo.com/lite/?q={}", encoded)
}

/// Detektera om DDG returnerade en CAPTCHA/anomaly-sida istället för resultat
pub fn is_ddg_captcha(html: &str) -> bool {
    html.contains("anomaly-modal")
        || html.contains("bots use DuckDuckGo")
        || html.contains("challenge to confirm")
}

/// Enkel percent-encoding för URL-query-parametrar.
/// Undviker att lägga till urlencoding-crate som beroende.
fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(hex_digit(byte >> 4));
                out.push(hex_digit(byte & 0x0F));
            }
        }
    }
    out
}

fn hex_digit(nibble: u8) -> char {
    match nibble {
        0..=9 => (b'0' + nibble) as char,
        _ => (b'A' + nibble - 10) as char,
    }
}

/// BUGG P: Detektera annons-/tracker-URLs som inte ska inkluderas i resultat.
fn is_ad_url(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.contains("duckduckgo.com/y.js")
        || lower.contains("/ad/click")
        || lower.contains("/ads/")
        || lower.contains("doubleclick.net")
        || lower.contains("googleadservices.com")
        || lower.contains("ad.atdmt.com")
        || lower.contains("bing.com/aclick")
}

// ─── Resultat-extraktion ─────────────────────────────────────────────────────

/// Extrahera sökresultat från semantiska noder.
///
/// DDG HTML har tre typer av noder per resultat:
/// 1. heading/link med DDG redirect-URL → titel + riktig URL
/// 2. link med samma redirect men label = display-URL → skip (duplikat)
/// 3. text/paragraph nod → snippet
///
/// Depth-first traversering bevarar DDG:s ordning: title → display-url → snippet.
pub fn extract_results(nodes: &[SemanticNode], top_n: usize) -> Vec<SearchResultEntry> {
    let flat = flatten_nodes_dfs(nodes);
    let mut results = Vec::new();
    let mut current: Option<SearchResultEntry> = None;
    let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();

    for node in &flat {
        let role = node.role.as_str();
        let value = node.value.as_deref().unwrap_or("");
        let has_ddg_redirect = value.contains("duckduckgo.com/l/?uddg=");

        // BUGG P: Skippa DDG-annonser (duckduckgo.com/y.js? URLs)
        if value.contains("duckduckgo.com/y.js") {
            continue;
        }

        // Steg 1: Ny titel-länk med DDG redirect
        if has_ddg_redirect && (role == "heading" || role == "link" || role == "cta") {
            let real_url = decode_ddg_redirect(value);

            // BUGG P: Skippa ad-tracker URLs
            if is_ad_url(&real_url) {
                continue;
            }

            // Skip display-URL-dubblett (samma URL, label ser ut som en URL)
            if seen_urls.contains(&real_url) {
                continue;
            }

            // Spara föregående halvfärdigt resultat
            if let Some(prev) = current.take() {
                if !prev.title.is_empty() {
                    results.push(prev);
                    if results.len() >= top_n {
                        return results;
                    }
                }
            }

            seen_urls.insert(real_url.clone());
            let domain = extract_domain(&real_url);
            current = Some(SearchResultEntry {
                rank: results.len() + 1,
                title: node.label.clone(),
                url: real_url,
                domain,
                snippet: String::new(),
                confidence: node.relevance,
                page_content: None,
                fetch_ms: None,
            });
            continue;
        }

        // Steg 2: Snippet-text (text/paragraph/link utan DDG-redirect)
        if let Some(ref mut r) = current {
            if r.snippet.is_empty() && !node.label.is_empty() {
                let label = node.label.as_str();
                // Ignorera display-URL-liknande labels (oavsett längd)
                let looks_like_url = label.contains("www.")
                    || label.contains("http")
                    || label.contains(".com/")
                    || label.contains(".se/")
                    || label.contains(".org/")
                    || label.contains(".net/")
                    || label.contains(".io/")
                    || label.contains(".de/")
                    || label.contains(".uk/")
                    || label.contains(".no/")
                    || label.contains(".dk/")
                    || label.contains(".fi/");
                if !looks_like_url && label.len() > 10 {
                    r.snippet = label.to_string();
                    results.push(r.clone());
                    current = None;
                    if results.len() >= top_n {
                        return results;
                    }
                }
            }
        }
    }

    // Pusha sista resultat
    if let Some(r) = current {
        if !r.title.is_empty() {
            results.push(r);
        }
    }

    results
}

/// Platta ut nod-träd till en flat lista (depth-first, pre-order).
/// Bevarar DOM-ordning: titel → display-url → snippet.
fn flatten_nodes_dfs(nodes: &[SemanticNode]) -> Vec<&SemanticNode> {
    let mut flat = Vec::new();
    for node in nodes {
        flat.push(node);
        flat.extend(flatten_nodes_dfs(&node.children));
    }
    flat
}

/// Avkoda DDG redirect-URL: //duckduckgo.com/l/?uddg=ENCODED_URL&rut=...
pub fn decode_ddg_redirect(url: &str) -> String {
    url.split("uddg=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .map(percent_decode)
        .unwrap_or_default()
}

/// Extrahera domän från URL
pub fn extract_domain(url: &str) -> String {
    // Enkel domänextraktion utan url-crate
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);
    without_scheme.split('/').next().unwrap_or("").to_string()
}

/// Enkel percent-decode
fn percent_decode(input: &str) -> String {
    let mut out = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push(h << 4 | l);
                i += 3;
                continue;
            }
        } else if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

// ─── Direct Answer Detection ─────────────────────────────────────────────────

/// Confidence-gate: generera inte direct_answer vid låg relevance
const DIRECT_ANSWER_MIN_CONFIDENCE: f32 = 0.3;

/// Välj bästa noden för direct_answer.
///
/// Prioritet:
/// 1. Bästa page_content-nod (deep-fetch resultat) — har faktisk siddata
/// 2. Bästa snippet — DDG:s sammanfattning
///
/// Returnerar hela nodens label orörd + källdomän.
/// AetherAgent tolkar INTE innehållet — det är LLM:ens ansvar.
pub fn detect_direct_answer(results: &[SearchResultEntry]) -> Option<(String, f32)> {
    // Steg 1: Sök bland alla page_content-noder (deep-fetch)
    let mut best_deep: Option<(&str, f32, &str)> = None; // (label, relevance, domain)
    for r in results {
        if let Some(ref pages) = r.page_content {
            for node in pages {
                if node.relevance >= DIRECT_ANSWER_MIN_CONFIDENCE
                    && node.label.len() > 20
                    && best_deep.is_none_or(|(_, rel, _)| node.relevance > rel)
                {
                    best_deep = Some((&node.label, node.relevance, &r.domain));
                }
            }
        }
    }

    if let Some((label, relevance, domain)) = best_deep {
        return Some((format!("{} ({})", label.trim(), domain), relevance));
    }

    // Steg 2: Fallback till bästa snippet
    for r in results.iter().take(3) {
        if r.confidence >= DIRECT_ANSWER_MIN_CONFIDENCE && r.snippet.len() > 20 {
            return Some((format!("{} ({})", r.snippet.trim(), r.domain), r.confidence));
        }
    }

    None
}

// ─── Tester ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_ddg_url_basic() {
        let url = build_ddg_url("hello world");
        assert_eq!(
            url, "https://lite.duckduckgo.com/lite/?q=hello+world",
            "Enkel query med mellanslag ska plus-kodas"
        );
    }

    #[test]
    fn test_build_ddg_url_special_chars() {
        let url = build_ddg_url("hur många bor i Sverige?");
        assert!(
            url.contains("hur+m"),
            "Svenska tecken ska percent-kodas korrekt"
        );
        assert!(url.contains("lite.duckduckgo.com"), "Ska använda DDG Lite");
    }

    #[test]
    fn test_percent_encode_roundtrip() {
        let input = "test med åäö & special=chars";
        let encoded = percent_encode(input);
        let decoded = percent_decode(&encoded);
        assert_eq!(decoded, input, "Encode-decode roundtrip ska bevara input");
    }

    #[test]
    fn test_decode_ddg_redirect() {
        let ddg_url = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.scb.se%2Fstatistik&rut=abc123";
        let result = decode_ddg_redirect(ddg_url);
        assert_eq!(
            result, "https://www.scb.se/statistik",
            "DDG redirect ska avkodas till riktig URL"
        );
    }

    #[test]
    fn test_decode_ddg_redirect_empty() {
        assert_eq!(
            decode_ddg_redirect("https://example.com"),
            "",
            "URL utan uddg-parameter ska returnera tom sträng"
        );
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://www.scb.se/hitta"), "www.scb.se");
        assert_eq!(extract_domain("http://example.com"), "example.com");
        assert_eq!(extract_domain("bare-string"), "bare-string");
    }

    #[test]
    fn test_detect_direct_answer_from_snippet() {
        let results = vec![SearchResultEntry {
            rank: 1,
            title: "SCB".to_string(),
            url: "https://scb.se".to_string(),
            snippet: "Sverige har 10 521 556 invånare enligt SCB 2024".to_string(),
            domain: "scb.se".to_string(),
            confidence: 0.85,
            page_content: None,
            fetch_ms: None,
        }];
        let answer = detect_direct_answer(&results);
        assert!(answer.is_some(), "Ska hitta direktsvar i snippet");
        let (text, conf) = answer.unwrap();
        assert!(
            text.contains("10 521 556 invånare"),
            "Ska innehålla hela snippeten: {text}"
        );
        assert!(text.contains("scb.se"), "Ska ha källdomän: {text}");
        assert!((conf - 0.85).abs() < 0.01);
    }

    #[test]
    fn test_detect_direct_answer_prefers_page_content() {
        let results = vec![SearchResultEntry {
            rank: 1,
            title: "SCB".to_string(),
            url: "https://scb.se".to_string(),
            snippet: "SCB statistik kort snippet".to_string(),
            domain: "scb.se".to_string(),
            confidence: 0.5,
            page_content: Some(vec![crate::search::PageNode {
                role: "text".to_string(),
                label: "Sveriges befolkning uppgår till 10 521 556 invånare per 2024".to_string(),
                relevance: 0.9,
            }]),
            fetch_ms: Some(100),
        }];
        let answer = detect_direct_answer(&results);
        assert!(answer.is_some(), "Ska hitta svar i page_content");
        let (text, _) = answer.unwrap();
        assert!(
            text.contains("10 521 556 invånare"),
            "Ska föredra page_content: {text}"
        );
    }

    #[test]
    fn test_detect_direct_answer_low_confidence_rejected() {
        let results = vec![SearchResultEntry {
            rank: 1,
            title: "X".to_string(),
            url: "https://example.com".to_string(),
            snippet: "Short".to_string(),
            domain: "example.com".to_string(),
            confidence: 0.1,
            page_content: None,
            fetch_ms: None,
        }];
        assert!(
            detect_direct_answer(&results).is_none(),
            "Låg confidence ska inte ge direct_answer"
        );
    }

    #[test]
    fn test_extract_results_from_nodes() {
        use crate::types::{NodeState, TrustLevel};

        // Simulera DDG-liknande noder
        let nodes = vec![
            SemanticNode {
                id: 1,
                role: "link".to_string(),
                label: "SCB Statistik".to_string(),
                value: Some("//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.scb.se&rut=x".to_string()),
                relevance: 0.8,
                trust: TrustLevel::Untrusted,
                state: NodeState::default_state(),
                action: None,
                children: vec![],
                html_id: None,
                name: None,
                bbox: None,
            },
            SemanticNode {
                id: 2,
                role: "text".to_string(),
                label: "Befolkningsstatistik för Sverige".to_string(),
                value: None,
                relevance: 0.5,
                trust: TrustLevel::Untrusted,
                state: NodeState::default_state(),
                action: None,
                children: vec![],
                html_id: None,
                name: None,
                bbox: None,
            },
        ];

        let results = extract_results(&nodes, 3);
        assert_eq!(results.len(), 1, "Ska hitta ett resultat");
        assert_eq!(results[0].title, "SCB Statistik");
        assert_eq!(results[0].url, "https://www.scb.se");
        assert_eq!(results[0].domain, "www.scb.se");
        assert_eq!(results[0].snippet, "Befolkningsstatistik för Sverige");
    }

    #[test]
    fn test_extract_results_empty() {
        let nodes: Vec<SemanticNode> = vec![];
        let results = extract_results(&nodes, 5);
        assert!(results.is_empty(), "Tom nod-lista ska ge tomt resultat");
    }

    #[test]
    fn test_flatten_nodes_dfs_depth() {
        use crate::types::{NodeState, TrustLevel};

        let child = SemanticNode {
            id: 2,
            role: "text".to_string(),
            label: "barn".to_string(),
            value: None,
            relevance: 0.3,
            trust: TrustLevel::Untrusted,
            state: NodeState::default_state(),
            action: None,
            children: vec![],
            html_id: None,
            name: None,
            bbox: None,
        };
        let parent = SemanticNode {
            id: 1,
            role: "heading".to_string(),
            label: "förälder".to_string(),
            value: None,
            relevance: 0.5,
            trust: TrustLevel::Untrusted,
            state: NodeState::default_state(),
            action: None,
            children: vec![child],
            html_id: None,
            name: None,
            bbox: None,
        };

        let nodes = [parent];
        let flat = flatten_nodes_dfs(&nodes);
        assert_eq!(flat.len(), 2, "Flatten ska ge alla noder inkl barn");
        assert_eq!(flat[0].label, "förälder");
        assert_eq!(flat[1].label, "barn");
    }

    /// Integrations-test: simulerar DDG HTML med fullständig struktur
    #[test]
    fn test_search_from_html_ddg_structure() {
        // Simulera DDG:s HTML-struktur
        let ddg_html = r##"<html><body>
        <div class="results">
          <div class="result">
            <h2 class="result__title">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.scb.se%2Fstatistik&rut=abc">
                Sveriges befolkning – SCB
              </a>
            </h2>
            <a class="result__url" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.scb.se%2Fstatistik&rut=abc">
              www.scb.se/statistik
            </a>
            <a class="result__snippet">
              Sveriges befolkning uppgår till 10 521 556 invånare enligt SCB:s senaste statistik.
            </a>
          </div>
          <div class="result">
            <h2 class="result__title">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fsv.wikipedia.org%2Fwiki%2FSverige&rut=def">
                Sverige – Wikipedia
              </a>
            </h2>
            <a class="result__url" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fsv.wikipedia.org%2Fwiki%2FSverige&rut=def">
              sv.wikipedia.org/wiki/Sverige
            </a>
            <a class="result__snippet">
              Sverige har en befolkning på drygt 10,5 miljoner invånare och är till ytan det tredje största landet i EU.
            </a>
          </div>
        </div>
        </body></html>"##;

        let result = crate::search_from_html(
            "hur många bor i Sverige",
            ddg_html,
            3,
            "hitta befolkningstal",
        );

        let parsed: serde_json::Value =
            serde_json::from_str(&result).expect("search_from_html ska returnera giltig JSON");
        let results = parsed["results"].as_array().expect("ska ha results-array");

        assert!(
            !results.is_empty(),
            "Ska hitta minst ett resultat i DDG HTML"
        );

        let first = &results[0];
        assert!(
            first["title"]
                .as_str()
                .unwrap_or("")
                .contains("Sveriges befolkning")
                || first["title"].as_str().unwrap_or("").contains("SCB"),
            "Första resultatets titel ska innehålla 'Sveriges befolkning' eller 'SCB'"
        );
        assert!(
            first["url"].as_str().unwrap_or("").contains("scb.se"),
            "URL ska vara avkodad SCB-URL"
        );

        // Kontrollera att snippet extraheras (inte tom)
        let snippet = first["snippet"].as_str().unwrap_or("");
        assert!(
            snippet.contains("10 521 556") || snippet.contains("invånare") || !snippet.is_empty(),
            "Snippet ska innehålla befolkningsdata: got '{}'",
            snippet
        );

        // Kontrollera direct_answer (ska hitta siffran)
        if let Some(answer) = parsed["direct_answer"].as_str() {
            assert!(
                answer.contains("10 521 556"),
                "Direktsvar ska vara '10 521 556': got '{}'",
                answer
            );
        }
    }
}
