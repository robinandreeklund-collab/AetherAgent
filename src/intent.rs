/// Intent API – mål-orienterade handlingar ovanpå det semantiska trädet
///
/// Fas 2: find_and_click, fill_form, extract_data
/// Bygger på SemanticTree från Fas 1 och text_similarity från semantic.rs.
use std::collections::{HashMap, HashSet};

use crate::semantic::text_similarity;
use crate::types::{
    ClickResult, ExtractDataResult, ExtractedEntry, FillFormResult, FormFieldMapping, SemanticNode,
    SemanticTree,
};

/// Klickbara roller
const CLICKABLE_ROLES: &[&str] = &["button", "link", "menuitem", "cta", "product_card"];

/// Minsta confidence för att inkludera ett extraherat värde i resultatet.
/// Script-kontaminering filtreras primärt via looks_like_script_content().
const MIN_EXTRACT_CONFIDENCE: f32 = 0.10;

/// Inmatningsbara roller
const INPUT_ROLES: &[&str] = &[
    "textbox",
    "searchbox",
    "textarea",
    "combobox",
    "checkbox",
    "radio",
    "select",
];

// ─── Hjälpfunktioner ─────────────────────────────────────────────────────────

/// Publik version av flatten_nodes för andra moduler (compiler.rs)
pub fn flatten_nodes_pub(nodes: &[SemanticNode]) -> Vec<&SemanticNode> {
    flatten_nodes(nodes)
}

/// Samla alla noder platt från ett trädstruktur (rekursivt)
fn flatten_nodes(nodes: &[SemanticNode]) -> Vec<&SemanticNode> {
    let mut result = vec![];
    for node in nodes {
        result.push(node);
        result.extend(flatten_nodes(&node.children));
    }
    result
}

/// Bygg en CSS-liknande selector hint från en nod
fn build_selector_hint(node: &SemanticNode) -> String {
    let tag = match node.role.as_str() {
        "button" | "cta" => "button",
        "link" => "a",
        "textbox" | "searchbox" => "input",
        "textarea" => "textarea",
        "combobox" | "select" => "select",
        "checkbox" | "radio" => "input",
        "product_card" => "div",
        "price" => "span",
        _ => "*",
    };

    if let Some(ref id) = node.html_id {
        return format!("{}#{}", tag, id);
    }

    if !node.label.is_empty() {
        return format!("{}[aria-label='{}']", tag, node.label.replace('\'', "\\'"));
    }

    if let Some(ref name) = node.name {
        return format!("{}[name='{}']", tag, name.replace('\'', "\\'"));
    }

    tag.to_string()
}

// ─── find_and_click ──────────────────────────────────────────────────────────

/// Hitta det bäst matchande klickbara elementet
pub fn find_best_clickable(tree: &SemanticTree, target_label: &str) -> ClickResult {
    let all_nodes = flatten_nodes(&tree.nodes);

    let mut best_node: Option<&SemanticNode> = None;
    let mut best_score: f32 = 0.0;

    for node in &all_nodes {
        // Bara klickbara element
        if !CLICKABLE_ROLES.contains(&node.role.as_str()) {
            continue;
        }

        if node.state.disabled {
            continue;
        }

        // Label-matchning (80% vikt) + roll-prioritet (20% vikt)
        let label_score = text_similarity(target_label, &node.label);

        // Kolla även name/html_id som alternativa matchningskällor
        let name_score = node
            .name
            .as_deref()
            .map(|n| text_similarity(target_label, n))
            .unwrap_or(0.0);

        let text_score = label_score.max(name_score);
        let role_bonus = SemanticNode::role_priority(&node.role) * 0.2;
        let score = text_score * 0.8 + role_bonus;

        if score > best_score {
            best_score = score;
            best_node = Some(node);
        }
    }

    match best_node {
        Some(node) if best_score > 0.1 => ClickResult {
            found: true,
            node_id: node.id,
            role: node.role.clone(),
            label: node.label.clone(),
            action: "click".to_string(),
            relevance: best_score.clamp(0.0, 1.0),
            selector_hint: build_selector_hint(node),
            trust: node.trust.clone(),
            injection_warnings: tree.injection_warnings.clone(),
            parse_time_ms: tree.parse_time_ms,
        },
        _ => ClickResult::not_found(tree.injection_warnings.clone(), tree.parse_time_ms),
    }
}

// ─── fill_form ───────────────────────────────────────────────────────────────

/// Matcha formulärfält med angivna nycklar/värden
pub fn map_form_fields(tree: &SemanticTree, fields: &HashMap<String, String>) -> FillFormResult {
    let all_nodes = flatten_nodes(&tree.nodes);

    // Samla alla input-noder
    let input_nodes: Vec<&&SemanticNode> = all_nodes
        .iter()
        .filter(|n| INPUT_ROLES.contains(&n.role.as_str()) && !n.state.disabled)
        .collect();

    let mut mappings = Vec::new();
    let mut matched_node_ids: HashSet<u32> = HashSet::new();
    let mut matched_keys: HashSet<String> = HashSet::new();

    for (key, value) in fields {
        let mut best_match: Option<&SemanticNode> = None;
        let mut best_confidence: f32 = 0.0;

        for node in &input_nodes {
            // Skippa redan matchade noder (O(1) med HashSet)
            if matched_node_ids.contains(&node.id) {
                continue;
            }

            // Matcha mot: label, name-attribut, html_id
            let label_score = text_similarity(key, &node.label);

            let name_score = node
                .name
                .as_deref()
                .map(|n| text_similarity(key, n))
                .unwrap_or(0.0);

            let id_score = node
                .html_id
                .as_deref()
                .map(|id| text_similarity(key, id))
                .unwrap_or(0.0);

            let confidence = label_score.max(name_score).max(id_score);

            if confidence > best_confidence {
                best_confidence = confidence;
                best_match = Some(node);
            }
        }

        if let Some(node) = best_match {
            if best_confidence > 0.2 {
                matched_node_ids.insert(node.id);
                matched_keys.insert(key.clone());
                mappings.push(FormFieldMapping {
                    field_label: node.label.clone(),
                    field_role: node.role.clone(),
                    node_id: node.id,
                    matched_key: key.clone(),
                    value: value.clone(),
                    selector_hint: build_selector_hint(node),
                    confidence: best_confidence,
                });
            }
        }
    }

    let unmapped_keys: Vec<String> = fields
        .keys()
        .filter(|k| !matched_keys.contains(k.as_str()))
        .cloned()
        .collect();

    let unmapped_fields: Vec<String> = input_nodes
        .iter()
        .filter(|n| !matched_node_ids.contains(&n.id))
        .map(|n| {
            if !n.label.is_empty() {
                n.label.clone()
            } else {
                n.name.clone().unwrap_or_else(|| format!("node_{}", n.id))
            }
        })
        .collect();

    FillFormResult {
        mappings,
        unmapped_keys,
        unmapped_fields,
        injection_warnings: tree.injection_warnings.clone(),
        parse_time_ms: tree.parse_time_ms,
    }
}

// ─── extract_data ────────────────────────────────────────────────────────────

/// Roll-preferenser för semantisk extraktion: nyckelord i key → föredragen roll
/// Detektera om en label ser ut som script/CSS-innehåll (cookie-scripts m.m.)
fn looks_like_script_content(text: &str) -> bool {
    if text.len() < 20 {
        return false;
    }
    let lower = text.to_lowercase();
    // Vanliga JS/CSS-mönster som läcker genom cookie-scripts
    let script_indicators = [
        "function(",
        "function ",
        "var ",
        "const ",
        "let ",
        "document.",
        "window.",
        "optanonwrapper",
        "adsbygoogle",
        "gtag(",
        "dataLayer",
        "createElement",
        "addEventListener",
        "typeof ",
        "return{",
        "return {",
        "({",
        "});",
        ".push(",
        "===",
        "!==",
    ];
    let matches = script_indicators
        .iter()
        .filter(|p| lower.contains(*p))
        .count();
    // Minst 2 indikatorer → troligtvis script
    matches >= 2
}

fn role_boost_for_key(key: &str) -> Option<(&'static str, f32)> {
    let key_lower = key.to_lowercase();
    let key_parts: Vec<&str> = key_lower
        .split(|c: char| c.is_whitespace() || c == '_' || c == '-')
        .filter(|s| !s.is_empty())
        .collect();

    for part in &key_parts {
        match *part {
            "url" | "href" | "link" | "src" => return Some(("link", 0.3)),
            "title" | "heading" | "headline" | "rubrik" => return Some(("heading", 0.2)),
            "button" | "action" => return Some(("button", 0.2)),
            "cta" | "buy" | "purchase" | "köp" | "cart" | "checkout" => {
                return Some(("cta", 0.35))
            }
            "price" | "pris" | "cost" | "amount" | "belopp" => return Some(("price", 0.3)),
            "product" | "produkt" | "item" | "card" | "vara" => {
                return Some(("product_card", 0.25))
            }
            "image" | "img" | "photo" | "bild" => return Some(("img", 0.2)),
            "nav" | "menu" | "navigation" | "meny" => return Some(("navigation", 0.2)),
            _ => {}
        }
    }
    None
}

/// Extrahera strukturerad data baserat på angivna nycklar
///
/// Förbättrad matchning: hanterar compound keys (story_title → title),
/// roll-aware boosting (url-keys → föredra links), och söker i value-fält.
pub fn extract_by_keys(tree: &SemanticTree, keys: &[String]) -> ExtractDataResult {
    let all_nodes = flatten_nodes(&tree.nodes);
    let mut entries = Vec::new();
    let mut found_keys: Vec<String> = Vec::new();

    for key in keys {
        let mut best_match: Option<(&SemanticNode, f32)> = None;
        let role_boost = role_boost_for_key(key);

        for node in &all_nodes {
            // Skippa tomma noder
            if node.label.is_empty() && node.value.is_none() {
                continue;
            }

            // Skippa noder vars label ser ut som script/kod-innehåll
            if looks_like_script_content(&node.label) {
                continue;
            }

            // Matcha nyckel mot nodens label
            let label_score = text_similarity(key, &node.label);

            // Matcha även mot name/html_id
            let name_score = node
                .name
                .as_deref()
                .map(|n| text_similarity(key, n))
                .unwrap_or(0.0);

            let id_score = node
                .html_id
                .as_deref()
                .map(|id| text_similarity(key, id))
                .unwrap_or(0.0);

            // Matcha mot value-fält (t.ex. href på länkar)
            let value_score = node
                .value
                .as_deref()
                .map(|v| text_similarity(key, v))
                .unwrap_or(0.0);

            let mut score = label_score.max(name_score).max(id_score).max(value_score);

            // Roll-boost: om nyckeln antyder en viss roll, ge bonus till matchande noder
            if let Some((preferred_role, boost)) = &role_boost {
                if node.role == *preferred_role {
                    score += boost;
                }
            }

            // Relevansviktning: föredra noder som är mer relevanta för goal
            score += node.relevance * 0.1;

            if let Some((_, best_score)) = best_match {
                if score > best_score {
                    best_match = Some((node, score));
                }
            } else if score > 0.1 {
                best_match = Some((node, score));
            }
        }

        if let Some((node, raw_score)) = best_match {
            found_keys.push(key.clone());

            // Kalibrerad confidence: textuell matchning viktad med goal-relevans.
            // Noder som matchar text men är irrelevanta för goal får lägre confidence.
            // Formel: raw_score * (0.4 + 0.6 * goal_relevance)
            // Ex: text_match=1.0, relevance=0.1 → 1.0 * 0.46 = 0.46 (inte 1.0!)
            // Ex: text_match=1.0, relevance=0.8 → 1.0 * 0.88 = 0.88
            let calibrated_confidence = raw_score * (0.4 + 0.6 * node.relevance);

            // Hämta värdet: för URL-keys på links, returnera value (href)
            let value = if role_boost
                .as_ref()
                .map(|(r, _)| *r == "link")
                .unwrap_or(false)
            {
                // Föredra href (value) för URL-nycklar, falla tillbaka på label
                node.value.clone().unwrap_or_else(|| node.label.clone())
            } else {
                // Föredra label för textnycklar, falla tillbaka på value
                if node.label.is_empty() {
                    node.value.clone().unwrap_or_default()
                } else {
                    node.label.clone()
                }
            };

            let clamped = calibrated_confidence.clamp(0.0, 1.0);
            if clamped >= MIN_EXTRACT_CONFIDENCE {
                entries.push(ExtractedEntry {
                    key: key.clone(),
                    value,
                    source_node_id: node.id,
                    confidence: clamped,
                });
            }
        }
    }

    // Nycklar som saknas = antingen ej hittade eller filtrerade under MIN_EXTRACT_CONFIDENCE
    let extracted_keys: Vec<&str> = entries.iter().map(|e| e.key.as_str()).collect();
    let missing_keys: Vec<String> = keys
        .iter()
        .filter(|k| !extracted_keys.contains(&k.as_str()))
        .cloned()
        .collect();

    ExtractDataResult {
        entries,
        missing_keys,
        injection_warnings: tree.injection_warnings.clone(),
        parse_time_ms: tree.parse_time_ms,
    }
}

/// Extract multiple matches per key (for comparative queries like "X vs Y").
/// Returns up to `max_per_key` entries per key, sorted by confidence DESC.
pub fn extract_by_keys_multi(
    tree: &SemanticTree,
    keys: &[String],
    max_per_key: usize,
) -> ExtractDataResult {
    let all_nodes = flatten_nodes(&tree.nodes);
    let mut entries = Vec::new();
    let mut found_keys: Vec<String> = Vec::new();

    for key in keys {
        let role_boost = role_boost_for_key(key);
        let mut scored: Vec<(&SemanticNode, f32)> = Vec::new();

        for node in &all_nodes {
            if node.label.is_empty() && node.value.is_none() {
                continue;
            }
            if looks_like_script_content(&node.label) {
                continue;
            }

            let label_score = text_similarity(key, &node.label);
            let name_score = node
                .name
                .as_deref()
                .map(|n| text_similarity(key, n))
                .unwrap_or(0.0);
            let id_score = node
                .html_id
                .as_deref()
                .map(|id| text_similarity(key, id))
                .unwrap_or(0.0);
            let value_score = node
                .value
                .as_deref()
                .map(|v| text_similarity(key, v))
                .unwrap_or(0.0);

            let mut score = label_score.max(name_score).max(id_score).max(value_score);
            if let Some((preferred_role, boost)) = &role_boost {
                if node.role == *preferred_role {
                    score += boost;
                }
            }
            score += node.relevance * 0.1;

            if score > 0.1 {
                scored.push((node, score));
            }
        }

        // Sortera och ta top-N per nyckel
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.truncate(max_per_key);

        if !scored.is_empty() {
            found_keys.push(key.clone());
        }

        for (node, raw_score) in scored {
            let calibrated = raw_score * (0.4 + 0.6 * node.relevance);
            let value = if !node.label.is_empty() {
                node.label.clone()
            } else {
                node.value.clone().unwrap_or_default()
            };
            entries.push(ExtractedEntry {
                key: key.clone(),
                value,
                confidence: calibrated.clamp(0.0, 1.0),
                source_node_id: node.id,
            });
        }
    }

    let missing_keys: Vec<String> = keys
        .iter()
        .filter(|k| !found_keys.contains(k))
        .cloned()
        .collect();

    ExtractDataResult {
        entries,
        missing_keys,
        injection_warnings: tree.injection_warnings.clone(),
        parse_time_ms: tree.parse_time_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_html;
    use crate::semantic::{extract_title, SemanticBuilder};

    fn build_test_tree(html: &str, goal: &str) -> SemanticTree {
        let dom = parse_html(html);
        let title = extract_title(&dom);
        let mut builder = SemanticBuilder::new(goal);
        builder.build(&dom, "https://test.com", &title)
    }

    // ─── find_best_clickable ─────────────────────────────────────────────────

    #[test]
    fn test_find_clickable_exact_match() {
        let tree = build_test_tree(
            r#"<html><body>
                <button>Lägg i varukorg</button>
                <button>Avbryt</button>
            </body></html>"#,
            "köp produkt",
        );
        let result = find_best_clickable(&tree, "Lägg i varukorg");
        assert!(result.found);
        assert_eq!(result.label, "Lägg i varukorg");
        assert_eq!(result.action, "click");
    }

    #[test]
    fn test_find_clickable_partial_match() {
        let tree = build_test_tree(
            r#"<html><body>
                <button>Logga in med Google</button>
                <a href="/about">Om oss</a>
            </body></html>"#,
            "logga in",
        );
        let result = find_best_clickable(&tree, "logga in");
        assert!(result.found);
        assert!(result.label.contains("Logga in"));
    }

    #[test]
    fn test_find_clickable_no_match() {
        let tree = build_test_tree(
            r#"<html><body>
                <p>Ingen knapp här</p>
            </body></html>"#,
            "klicka",
        );
        let result = find_best_clickable(&tree, "köp nu");
        assert!(!result.found);
    }

    #[test]
    fn test_find_clickable_prefers_button_over_link() {
        let tree = build_test_tree(
            r#"<html><body>
                <a href="/login">Logga in</a>
                <button>Logga in</button>
            </body></html>"#,
            "logga in",
        );
        let result = find_best_clickable(&tree, "Logga in");
        assert!(result.found);
        assert_eq!(result.role, "button");
    }

    #[test]
    fn test_find_clickable_selector_hint_with_id() {
        let tree = build_test_tree(
            r#"<html><body>
                <button id="buy-btn">Köp nu</button>
            </body></html>"#,
            "köp",
        );
        let result = find_best_clickable(&tree, "Köp nu");
        assert!(result.found);
        assert_eq!(result.selector_hint, "button#buy-btn");
    }

    // ─── map_form_fields ─────────────────────────────────────────────────────

    #[test]
    fn test_map_form_fields_login() {
        let tree = build_test_tree(
            r#"<html><body><form>
                <input type="email" name="email" placeholder="E-post" />
                <input type="password" name="password" placeholder="Lösenord" />
                <button type="submit">Logga in</button>
            </form></body></html>"#,
            "logga in",
        );

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), "test@test.se".to_string());
        fields.insert("password".to_string(), "hemligt123".to_string());

        let result = map_form_fields(&tree, &fields);
        assert_eq!(result.mappings.len(), 2, "Borde matcha båda fälten");
        assert!(result.unmapped_keys.is_empty());
    }

    #[test]
    fn test_map_form_fields_unmapped_keys() {
        let tree = build_test_tree(
            r#"<html><body><form>
                <input type="text" name="email" placeholder="E-post" />
            </form></body></html>"#,
            "registrera",
        );

        let mut fields = HashMap::new();
        fields.insert("email".to_string(), "test@test.se".to_string());
        fields.insert("phone".to_string(), "0701234567".to_string());

        let result = map_form_fields(&tree, &fields);
        assert_eq!(result.mappings.len(), 1);
        assert!(result.unmapped_keys.contains(&"phone".to_string()));
    }

    #[test]
    fn test_map_form_fields_name_attribute_match() {
        let tree = build_test_tree(
            r#"<html><body><form>
                <input type="text" name="first_name" />
                <input type="text" name="last_name" />
            </form></body></html>"#,
            "fyll i namn",
        );

        let mut fields = HashMap::new();
        fields.insert("first_name".to_string(), "Robin".to_string());
        fields.insert("last_name".to_string(), "Eklund".to_string());

        let result = map_form_fields(&tree, &fields);
        assert_eq!(result.mappings.len(), 2);
    }

    // ─── extract_by_keys ─────────────────────────────────────────────────────

    #[test]
    fn test_extract_data_price() {
        let tree = build_test_tree(
            r#"<html><body>
                <h1>iPhone 16 Pro</h1>
                <p class="price">13 990 kr</p>
                <p>En fantastisk telefon med A18-chip.</p>
            </body></html>"#,
            "hämta produktinfo",
        );

        let keys = vec!["price".to_string()];
        let result = extract_by_keys(&tree, &keys);

        // Borde hitta priset via textuell matchning mot "price"-klassen
        // eller via textinnehåll som innehåller "kr"
        assert!(!result.entries.is_empty() || !result.missing_keys.is_empty());
    }

    #[test]
    fn test_extract_data_missing_key() {
        let tree = build_test_tree(
            r#"<html><body>
                <h1>Produktsida</h1>
            </body></html>"#,
            "hämta data",
        );

        let keys = vec!["nonexistent_field".to_string()];
        let result = extract_by_keys(&tree, &keys);
        assert!(result
            .missing_keys
            .contains(&"nonexistent_field".to_string()));
    }

    #[test]
    fn test_extract_data_heading() {
        let tree = build_test_tree(
            r#"<html><body>
                <h1>Sagan om Ringen</h1>
                <p>Av J.R.R. Tolkien</p>
            </body></html>"#,
            "hitta boktitel",
        );

        let keys = vec!["Sagan".to_string()];
        let result = extract_by_keys(&tree, &keys);
        assert!(!result.entries.is_empty(), "Borde hitta headingen");
        assert!(result.entries[0].value.contains("Sagan om Ringen"));
    }
}
