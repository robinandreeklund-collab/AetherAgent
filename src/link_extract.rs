// Fas 19: Rich Link Extraction with Metadata
//
// Extraherar länkar från ett semantiskt träd med rik metadata:
// - Resolved absolut URL
// - Relevance score (BM25 mot goal)
// - Novelty score (HDC vs ackumulerad HV)
// - Structural role (navigation/content/footer)
// - Context snippet (omgivande text)
//
// Inga nya dependencies — använder befintlig scoring/hdc + scoring/tfidf.

use serde::{Deserialize, Serialize};

use crate::scoring::hdc::Hypervector;
use crate::types::SemanticNode;

// ─── Typer ──────────────────────────────────────────────────────────────────

/// En länk med rik metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedLink {
    /// Rå href-attribut
    pub href: String,
    /// Fullt resolved URL
    pub absolute_url: String,
    /// Synlig ankartext
    pub anchor_text: String,
    /// title-attribut (om det finns)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// CRFR-baserad relevans mot goal (0.0–1.0)
    pub relevance_score: f32,
    /// HDC novelty vs ackumulerad HV (0.0–1.0)
    pub novelty_score: f32,
    /// Kombinerad expected gain
    pub expected_gain: f32,
    /// Omgivande text (±50 tecken)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_snippet: Option<String>,
    /// Strukturell roll: "navigation", "content", "footer", "sidebar", "card"
    pub structural_role: String,
    /// Samma domän som sidan?
    pub is_internal: bool,
    /// Klickdjup från startsida
    pub depth: u32,
    /// Ordning i DOM (position bland alla links)
    pub dom_position: u32,
    /// Title från HEAD-fetch (opt-in)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_title: Option<String>,
    /// Meta description från HEAD-fetch (opt-in)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta_description: Option<String>,
}

/// Konfiguration för link extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkExtractionConfig {
    /// Om satt → relevance scoring aktiveras
    #[serde(default)]
    pub goal: Option<String>,
    /// Max antal links att returnera
    #[serde(default = "default_max_links")]
    pub max_links: usize,
    /// Inkludera context snippet
    #[serde(default = "default_true")]
    pub include_context: bool,
    /// Inkludera structural role
    #[serde(default = "default_true")]
    pub include_structural_role: bool,
    /// Filtrera bort navigation-links
    #[serde(default)]
    pub filter_navigation: bool,
    /// Minimum relevance (0.0 = alla)
    #[serde(default)]
    pub min_relevance: f32,
}

fn default_max_links() -> usize {
    50
}
fn default_true() -> bool {
    true
}

impl Default for LinkExtractionConfig {
    fn default() -> Self {
        LinkExtractionConfig {
            goal: None,
            max_links: default_max_links(),
            include_context: true,
            include_structural_role: true,
            filter_navigation: false,
            min_relevance: 0.0,
        }
    }
}

/// Resultat från link extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkExtractionResult {
    /// Extraherade links
    pub links: Vec<EnrichedLink>,
    /// Totalt antal links hittade i trädet
    pub total_found: u32,
    /// Antal filtrerade (under min_relevance eller navigation)
    pub filtered: u32,
    /// Exekveringstid i ms
    pub extract_time_ms: u64,
}

// ─── Implementation ─────────────────────────────────────────────────────────

/// Extrahera enriched links från ett semantiskt träd.
///
/// Traverserar hela trädet rekursivt, samlar alla role="link" noder,
/// och berikar dem med relevance, novelty, structural role och context.
pub fn extract_links_from_tree(
    tree: &[SemanticNode],
    page_url: &str,
    config: &LinkExtractionConfig,
    accumulated_hv: Option<&Hypervector>,
) -> LinkExtractionResult {
    let start = std::time::Instant::now();
    let base_domain = extract_domain(page_url);

    // Samla alla link-noder med kontext
    let mut raw_links: Vec<RawLink> = Vec::new();
    for node in tree {
        collect_links(node, &mut raw_links, &[], 0);
    }
    let total_found = raw_links.len() as u32;

    // Dedup på absolut URL
    let mut seen_urls = std::collections::HashSet::new();
    raw_links.retain(|l| {
        let abs = resolve_url(&l.href, page_url);
        seen_urls.insert(abs)
    });

    // Beräkna goal-relaterade scores
    let goal_words: Vec<String> = config
        .goal
        .as_ref()
        .map(|g| tokenize_goal(g))
        .unwrap_or_default();

    // Bygg enriched links
    let mut enriched: Vec<EnrichedLink> = raw_links
        .iter()
        .enumerate()
        .map(|(i, raw)| {
            let absolute_url = resolve_url(&raw.href, page_url);
            let link_domain = extract_domain(&absolute_url);
            let is_internal = link_domain == base_domain;

            // Relevance: BM25-liknande term overlap
            let relevance_score = if goal_words.is_empty() {
                0.0
            } else {
                compute_relevance(&raw.anchor_text, &raw.context, &goal_words)
            };

            // Novelty: HDC avstånd från ackumulerat
            let novelty_score = if let Some(acc) = accumulated_hv {
                let link_text = format!("{} {}", raw.anchor_text, raw.context);
                let link_hv = Hypervector::from_text_ngrams(&link_text);
                // similarity returnerar -1.0..1.0, novelty = 1.0 - sim
                let sim = link_hv.similarity(acc);
                ((1.0 - sim) / 2.0).clamp(0.0, 1.0)
            } else {
                0.5 // Neutral om ingen ackumulering
            };

            // Structural role
            let structural_role = if config.include_structural_role {
                infer_structural_role(&raw.parent_roles)
            } else {
                "unknown".to_string()
            };

            // Structural bonus
            let structural_bonus = match structural_role.as_str() {
                "content" | "card" => 1.0,
                "sidebar" => 0.6,
                "navigation" => 0.3,
                "footer" => 0.2,
                _ => 0.5,
            };

            // Expected gain: weighted combination
            let expected_gain = if goal_words.is_empty() {
                structural_bonus
            } else {
                0.4 * novelty_score + 0.35 * relevance_score + 0.25 * structural_bonus
            };

            // Context snippet
            let context_snippet = if config.include_context && !raw.context.is_empty() {
                Some(truncate_context(&raw.context, 100))
            } else {
                None
            };

            EnrichedLink {
                href: raw.href.clone(),
                absolute_url,
                anchor_text: raw.anchor_text.clone(),
                title: None,
                relevance_score,
                novelty_score,
                expected_gain,
                context_snippet,
                structural_role,
                is_internal,
                depth: raw.depth,
                dom_position: i as u32,
                head_title: None,
                meta_description: None,
            }
        })
        .collect();

    // Filtrera
    let pre_filter_count = enriched.len();
    if config.filter_navigation {
        enriched.retain(|l| l.structural_role != "navigation");
    }
    if config.min_relevance > 0.0 {
        enriched.retain(|l| l.relevance_score >= config.min_relevance);
    }
    // Filtrera bort fragment-only och javascript: links
    enriched.retain(|l| {
        !l.absolute_url.starts_with("javascript:")
            && !l.absolute_url.is_empty()
            && l.absolute_url != page_url
    });
    let filtered = (pre_filter_count - enriched.len()) as u32;

    // Sortera efter expected_gain (högst först)
    enriched.sort_by(|a, b| b.expected_gain.total_cmp(&a.expected_gain));

    // Begränsa
    enriched.truncate(config.max_links);

    let extract_time_ms = start.elapsed().as_millis() as u64;

    LinkExtractionResult {
        links: enriched,
        total_found,
        filtered,
        extract_time_ms,
    }
}

// ─── Interna hjälpfunktioner ────────────────────────────────────────────────

/// Rå link-data innan enrichment
struct RawLink {
    href: String,
    anchor_text: String,
    context: String,
    parent_roles: Vec<String>,
    depth: u32,
}

/// Samla alla link-noder rekursivt
fn collect_links(
    node: &SemanticNode,
    links: &mut Vec<RawLink>,
    parent_roles: &[String],
    depth: u32,
) {
    if node.role == "link" {
        if let Some(ref href) = node.value {
            if !href.is_empty() {
                // Kontext: förälder-nodens label eller tomt
                let context = if parent_roles.is_empty() {
                    String::new()
                } else {
                    // Sök uppåt i trädets roller
                    String::new()
                };

                links.push(RawLink {
                    href: href.clone(),
                    anchor_text: node.label.clone(),
                    context,
                    parent_roles: parent_roles.to_vec(),
                    depth,
                });
            }
        }
    }

    // Traversera barn med uppdaterade parent_roles
    let mut child_parents = parent_roles.to_vec();
    child_parents.push(node.role.clone());
    for child in &node.children {
        collect_links(child, links, &child_parents, depth);
    }
}

/// Samla kontext för en link: syskon-noders labels
pub fn collect_sibling_context(parent: &SemanticNode, link_id: u32) -> String {
    let mut parts = Vec::new();
    for child in &parent.children {
        if child.id != link_id && child.role != "link" && !child.label.is_empty() {
            parts.push(child.label.as_str());
        }
    }
    parts.join(" ")
}

/// Resolve en relativ URL mot base
fn resolve_url(href: &str, base_url: &str) -> String {
    let trimmed = href.trim();
    if trimmed.is_empty() || trimmed.starts_with("javascript:") || trimmed.starts_with("mailto:") {
        return trimmed.to_string();
    }

    // Redan absolut
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return trimmed.to_string();
    }

    // Protocol-relative
    if let Some(rest) = trimmed.strip_prefix("//") {
        let scheme = if base_url.starts_with("https://") {
            "https"
        } else {
            "http"
        };
        return format!("{scheme}://{rest}");
    }

    // Extrahera bas-delar
    let base_parts = parse_base_url(base_url);

    if let Some(rest) = trimmed.strip_prefix('/') {
        // Absolut path
        format!("{}://{}/{}", base_parts.scheme, base_parts.host, rest)
    } else if trimmed.starts_with('#') {
        // Fragment — returnera base + fragment
        format!(
            "{}{}",
            base_url.split('#').next().unwrap_or(base_url),
            trimmed
        )
    } else if trimmed.starts_with('?') {
        // Query — returnera base path + query
        let path = base_url.split('?').next().unwrap_or(base_url);
        format!("{path}{trimmed}")
    } else {
        // Relativ path
        let base_path = base_parts
            .path
            .rsplit_once('/')
            .map(|(p, _)| p)
            .unwrap_or("");
        format!(
            "{}://{}{}/{}",
            base_parts.scheme, base_parts.host, base_path, trimmed
        )
    }
}

struct UrlParts {
    scheme: String,
    host: String,
    path: String,
}

fn parse_base_url(url: &str) -> UrlParts {
    let (scheme, rest) = url.split_once("://").unwrap_or(("https", url));
    let (host_path, _query) = rest.split_once('?').unwrap_or((rest, ""));
    let (host_path, _frag) = host_path.split_once('#').unwrap_or((host_path, ""));
    let (host, path) = host_path
        .find('/')
        .map(|i| (&host_path[..i], &host_path[i..]))
        .unwrap_or((host_path, "/"));
    UrlParts {
        scheme: scheme.to_string(),
        host: host.to_string(),
        path: path.to_string(),
    }
}

/// Extrahera domän från URL (utan www.) — publik wrapper
pub fn extract_domain_pub(url: &str) -> String {
    extract_domain(url)
}

/// Extrahera domän från URL (utan www.)
fn extract_domain(url: &str) -> String {
    let without_scheme = url.split("://").last().unwrap_or(url);
    let domain = without_scheme.split('/').next().unwrap_or("");
    domain.strip_prefix("www.").unwrap_or(domain).to_lowercase()
}

/// Beräkna term-overlap relevance (BM25-liknande men lättare)
fn compute_relevance(anchor_text: &str, context: &str, goal_words: &[String]) -> f32 {
    if goal_words.is_empty() {
        return 0.0;
    }
    let combined = format!("{} {}", anchor_text, context).to_lowercase();
    let matched = goal_words
        .iter()
        .filter(|w| combined.contains(w.as_str()))
        .count();
    (matched as f32 / goal_words.len() as f32).clamp(0.0, 1.0)
}

/// Tokenisera goal till ord (>2 tecken)
fn tokenize_goal(goal: &str) -> Vec<String> {
    goal.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| s.len() > 2)
        .map(String::from)
        .collect()
}

/// Klassificera structural role baserat på parent-roller
fn infer_structural_role(parent_roles: &[String]) -> String {
    for role in parent_roles.iter().rev() {
        match role.as_str() {
            "navigation" | "banner" => return "navigation".to_string(),
            "contentinfo" | "footer" => return "footer".to_string(),
            "complementary" => return "sidebar".to_string(),
            "article" | "main" => return "content".to_string(),
            "product_card" | "card" => return "card".to_string(),
            _ => {}
        }
    }
    "content".to_string() // Default: content
}

/// Trunkera kontext till max_len tecken, respektera char boundaries
fn truncate_context(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let mut end = max_len;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &text[..end])
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SemanticNode;

    fn make_link(id: u32, label: &str, href: &str) -> SemanticNode {
        let mut node = SemanticNode::new(id, "link", label);
        node.value = Some(href.to_string());
        node
    }

    fn make_container(id: u32, role: &str, children: Vec<SemanticNode>) -> SemanticNode {
        let mut node = SemanticNode::new(id, role, "");
        node.children = children;
        node
    }

    #[test]
    fn test_extract_links_basic() {
        let tree = vec![make_container(
            1,
            "navigation",
            vec![
                make_link(2, "Hem", "/"),
                make_link(3, "Produkter", "/produkter"),
                make_link(4, "Om oss", "/om"),
            ],
        )];

        let config = LinkExtractionConfig::default();
        let result = extract_links_from_tree(&tree, "https://example.com/page", &config, None);

        assert_eq!(result.total_found, 3, "Ska hitta 3 links");
        assert_eq!(result.links.len(), 3, "Ska returnera 3 links");
        assert!(
            result.links[0].absolute_url.starts_with("https://"),
            "URLs ska vara resolved"
        );
    }

    #[test]
    fn test_filter_navigation() {
        let tree = vec![
            make_container(1, "navigation", vec![make_link(2, "Nav link", "/nav")]),
            make_container(3, "article", vec![make_link(4, "Content link", "/content")]),
        ];

        let config = LinkExtractionConfig {
            filter_navigation: true,
            ..Default::default()
        };
        let result = extract_links_from_tree(&tree, "https://example.com", &config, None);

        assert_eq!(result.links.len(), 1, "Ska bara ha content-link");
        assert_eq!(
            result.links[0].structural_role, "content",
            "Ska vara content-roll"
        );
    }

    #[test]
    fn test_relevance_scoring() {
        let tree = vec![
            make_link(1, "Köp skor online", "/skor"),
            make_link(2, "Om företaget", "/om"),
            make_link(3, "Sneakers REA", "/sneakers"),
        ];

        let config = LinkExtractionConfig {
            goal: Some("köp skor".to_string()),
            ..Default::default()
        };
        let result = extract_links_from_tree(&tree, "https://example.com", &config, None);

        assert!(
            result.links[0].relevance_score > result.links.last().unwrap().relevance_score,
            "Sko-links ska rankas högre än om-link"
        );
    }

    #[test]
    fn test_resolve_relative_urls() {
        assert_eq!(
            resolve_url("/path", "https://example.com/page"),
            "https://example.com/path"
        );
        assert_eq!(
            resolve_url("https://other.com/x", "https://example.com"),
            "https://other.com/x"
        );
        assert_eq!(
            resolve_url("//cdn.example.com/f", "https://example.com"),
            "https://cdn.example.com/f"
        );
        assert_eq!(
            resolve_url("sub/page", "https://example.com/dir/index.html"),
            "https://example.com/dir/sub/page"
        );
    }

    #[test]
    fn test_is_internal() {
        let tree = vec![
            make_link(1, "Intern", "/page2"),
            make_link(2, "Extern", "https://other.com/page"),
        ];

        let config = LinkExtractionConfig::default();
        let result = extract_links_from_tree(&tree, "https://example.com", &config, None);

        let intern = result
            .links
            .iter()
            .find(|l| l.anchor_text == "Intern")
            .unwrap();
        let extern_l = result
            .links
            .iter()
            .find(|l| l.anchor_text == "Extern")
            .unwrap();
        assert!(intern.is_internal, "Intern link ska vara internal");
        assert!(!extern_l.is_internal, "Extern link ska vara external");
    }

    #[test]
    fn test_dedup_same_href() {
        let tree = vec![
            make_link(1, "Link A", "/same"),
            make_link(2, "Link B", "/same"),
        ];

        let config = LinkExtractionConfig::default();
        let result = extract_links_from_tree(&tree, "https://example.com", &config, None);

        assert_eq!(result.links.len(), 1, "Samma URL ska dedup:as");
    }

    #[test]
    fn test_novelty_scoring() {
        let tree = vec![
            make_link(1, "Helt ny topic om AI", "/ai"),
            make_link(2, "Skor och kläder", "/skor"),
        ];

        // Ackumulerad HV som handlar om skor
        let acc = Hypervector::from_text_ngrams("skor kläder mode sneakers");

        let config = LinkExtractionConfig {
            goal: Some("AI nyheter".to_string()),
            ..Default::default()
        };
        let result = extract_links_from_tree(&tree, "https://example.com", &config, Some(&acc));

        let ai_link = result
            .links
            .iter()
            .find(|l| l.anchor_text.contains("AI"))
            .unwrap();
        let sko_link = result
            .links
            .iter()
            .find(|l| l.anchor_text.contains("Skor"))
            .unwrap();
        assert!(
            ai_link.novelty_score > sko_link.novelty_score,
            "AI-link ska ha högre novelty (mer olikt ackumulerat)"
        );
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(
            extract_domain("https://www.example.com/path"),
            "example.com"
        );
        assert_eq!(extract_domain("https://sub.example.com"), "sub.example.com");
        assert_eq!(extract_domain("http://example.com"), "example.com");
    }

    #[test]
    fn test_structural_role_inference() {
        assert_eq!(
            infer_structural_role(&["navigation".to_string()]),
            "navigation"
        );
        assert_eq!(
            infer_structural_role(&["generic".to_string(), "article".to_string()]),
            "content"
        );
        assert_eq!(
            infer_structural_role(&["contentinfo".to_string()]),
            "footer"
        );
        assert_eq!(
            infer_structural_role(&["generic".to_string()]),
            "content", // Default
        );
    }

    #[test]
    fn test_max_links() {
        let links: Vec<SemanticNode> = (0..100)
            .map(|i| make_link(i, &format!("Link {i}"), &format!("/page/{i}")))
            .collect();

        let config = LinkExtractionConfig {
            max_links: 10,
            ..Default::default()
        };
        let result = extract_links_from_tree(&links, "https://example.com", &config, None);

        assert_eq!(result.links.len(), 10, "Ska begränsas till max_links");
    }
}
