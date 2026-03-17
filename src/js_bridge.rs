/// Selective JS Execution – Fas 4c
///
/// Automatisk pipeline som:
/// 1. Detekterar JS-beroende innehåll i HTML
/// 2. Extraherar evaluerbara uttryck från inline-scripts
/// 3. Matchar script-targets (getElementById, querySelector) till semantiska noder
/// 4. Kör uttryck i sandbox (Fas 4b)
/// 5. Applicerar resultaten tillbaka till det semantiska trädet
///
/// Resultatet: en "enhanced" SemanticTree med JS-beräknade värden ifyllda,
/// plus metadata om vilka noder som påverkades och vilka scripts som kördes.
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::js_eval;
use crate::types::SemanticTree;

/// En matchning mellan ett JS-uttryck och en semantisk nod
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsNodeBinding {
    /// ID på noden som påverkas
    pub node_id: u32,
    /// CSS-selector eller ID som scriptet refererar
    pub target_selector: String,
    /// Vilken egenskap som sätts (textContent, innerHTML, value, etc.)
    pub target_property: String,
    /// JS-uttrycket som beräknar värdet
    pub expression: String,
    /// Beräknat värde (om evalueringen lyckades)
    pub computed_value: Option<String>,
    /// Felmeddelande (om evalueringen misslyckades)
    pub error: Option<String>,
}

/// Resultat från selective JS execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectiveExecResult {
    /// Det ursprungliga semantiska trädet (oförändrat)
    pub tree: SemanticTree,
    /// JS-bindningar som hittades och evaluerades
    pub js_bindings: Vec<JsNodeBinding>,
    /// Noder vars label uppdaterades med JS-beräknade värden
    pub enhanced_node_ids: Vec<u32>,
    /// JS-analys: antal inline-scripts, event handlers, framework
    pub js_analysis: JsAnalysisSummary,
    /// Totalt antal evalueringar som kördes
    pub total_evals: u32,
    /// Totalt antal lyckade evalueringar
    pub successful_evals: u32,
    /// Exekveringstid i millisekunder
    pub exec_time_ms: u64,
}

/// Sammanfattning av JS-analys
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsAnalysisSummary {
    pub total_inline_scripts: u32,
    pub total_event_handlers: u32,
    pub has_framework: bool,
    pub framework_hint: Option<String>,
    pub evaluable_expressions: u32,
    pub content_affecting_scripts: u32,
}

// ─── DOM-target patterns ────────────────────────────────────────────────────

/// Mönster: document.getElementById('id').property = expression
struct DomTarget {
    selector: String,
    property: String,
    expression: String,
}

/// Extrahera DOM-targets från JS-kod
fn extract_dom_targets(code: &str) -> Vec<DomTarget> {
    let mut targets = Vec::new();

    // Mönster 1: document.getElementById('id').property = expr
    extract_get_element_by_id(code, &mut targets);

    // Mönster 2: document.querySelector('selector').property = expr
    extract_query_selector(code, &mut targets);

    // Mönster 3: variabeltilldelning + DOM-sättning (enklare mönster)
    extract_variable_assignments(code, &mut targets);

    targets
}

/// Hitta getElementById-mönster
fn extract_get_element_by_id(code: &str, targets: &mut Vec<DomTarget>) {
    // Matcha: document.getElementById('X').Y = Z
    // och:   document.getElementById("X").Y = Z
    let patterns = ["document.getelementbyid(", "document.getelementbyid ("];

    let lower = code.to_lowercase();

    for pattern in &patterns {
        let mut pos = 0;
        while let Some(idx) = lower[pos..].find(pattern) {
            let abs_start = pos + idx + pattern.len();
            if let Some(target) = parse_get_element_pattern(code, abs_start) {
                targets.push(target);
            }
            pos = abs_start;
        }
    }
}

/// Parsa getElementById('id').property = expression
fn parse_get_element_pattern(code: &str, start: usize) -> Option<DomTarget> {
    let rest = &code[start..];

    // Hitta id:t (inom quotes)
    let (id, after_id) = extract_quoted_string(rest)?;

    // Hoppa över ) och .
    let after_close = rest[after_id..].find(')')?;
    let after_dot_start = after_id + after_close + 1;
    let after_dot = rest.get(after_dot_start..)?;

    // Skippa whitespace och punkt
    let trimmed = after_dot.trim_start();
    let trimmed = trimmed.strip_prefix('.')?;

    // Hitta property-namn
    let prop_end = trimmed
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(trimmed.len());
    let property = trimmed[..prop_end].to_string();

    if property.is_empty() {
        return None;
    }

    // Hitta = och uttrycket
    let after_prop = &trimmed[prop_end..].trim_start();
    let after_eq = after_prop.strip_prefix('=')?;
    // Se till att det inte är == (jämförelse)
    if after_eq.starts_with('=') {
        return None;
    }

    // Extrahera uttrycket fram till första ; eller nyrad
    let expr_text = after_eq.trim();
    let expr_end = expr_text
        .find(';')
        .or_else(|| expr_text.find('\n'))
        .unwrap_or(expr_text.len());
    let expression = expr_text[..expr_end].trim().to_string();

    if expression.is_empty() {
        return None;
    }

    Some(DomTarget {
        selector: id,
        property,
        expression,
    })
}

/// Hitta querySelector-mönster
fn extract_query_selector(code: &str, targets: &mut Vec<DomTarget>) {
    let patterns = ["document.queryselector(", "document.queryselector ("];

    let lower = code.to_lowercase();

    for pattern in &patterns {
        let mut pos = 0;
        while let Some(idx) = lower[pos..].find(pattern) {
            let abs_start = pos + idx + pattern.len();
            // Samma parser som getElementById men selector kan vara mer komplex
            if let Some(target) = parse_get_element_pattern(code, abs_start) {
                targets.push(target);
            }
            pos = abs_start;
        }
    }
}

/// Extrahera enkla variabeltilldelningar som sedan används i DOM
fn extract_variable_assignments(code: &str, targets: &mut Vec<DomTarget>) {
    // Mönster: var/let/const name = expression; ... getElementById(id).prop = name
    // Förenkling: hitta alla getElementById-tilldelningar och försök extrahera uttrycket
    // direkt. Mer avancerad analys kan läggas till senare.

    // Hitta mönster som: el.textContent = uttryck  (efter getElementById)
    // Redan hanterat ovan, så vi fokuserar på inline beräkningar
    // t.ex. <script>price = 29.99 * qty;</script>

    // Identifiera rena beräkningsuttryck utan DOM-beroende
    let lines: Vec<&str> = code.split(';').collect();
    for line in lines {
        let trimmed = line.trim();
        // Skippa DOM-operationer (redan hanterade)
        let lower = trimmed.to_lowercase();
        if lower.contains("document.") || lower.contains("window.") {
            continue;
        }
        // Hitta variabeltilldelningar med beräkningar
        if let Some(eq_pos) = trimmed.find('=') {
            // Se till att det inte är ==, !=, <=, >=, =>
            if eq_pos > 0
                && !trimmed[..eq_pos].ends_with('!')
                && !trimmed[..eq_pos].ends_with('<')
                && !trimmed[..eq_pos].ends_with('>')
                && !trimmed[..eq_pos].ends_with('=')
                && trimmed.get(eq_pos + 1..eq_pos + 2) != Some("=")
                && trimmed.get(eq_pos + 1..eq_pos + 2) != Some(">")
            {
                let var_part = trimmed[..eq_pos].trim();
                let expr_part = trimmed[eq_pos + 1..].trim();

                // Skippa var/let/const prefix
                let var_name = var_part
                    .strip_prefix("var ")
                    .or_else(|| var_part.strip_prefix("let "))
                    .or_else(|| var_part.strip_prefix("const "))
                    .unwrap_or(var_part)
                    .trim();

                // Bara enkla variabelnamn (inga property access)
                if !var_name.is_empty()
                    && var_name
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '$')
                    && !expr_part.is_empty()
                    && expr_part.len() < 200
                {
                    targets.push(DomTarget {
                        selector: format!("var:{}", var_name),
                        property: "value".to_string(),
                        expression: expr_part.to_string(),
                    });
                }
            }
        }
    }
}

/// Extrahera en sträng inom quotes ('...' eller "...")
fn extract_quoted_string(s: &str) -> Option<(String, usize)> {
    let trimmed = s.trim_start();
    let offset = s.len() - trimmed.len();

    let quote = trimmed.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let inner = &trimmed[1..];
    let end = inner.find(quote)?;
    Some((inner[..end].to_string(), offset + 1 + end + 1))
}

// ─── Selective execution pipeline ───────────────────────────────────────────

/// Kör selective JS execution på en HTML-sida
///
/// 1. Parsear HTML till semantiskt träd
/// 2. Detekterar JS-snippets
/// 3. Extraherar evaluerbara uttryck
/// 4. Matchar mot semantiska noder (via html_id)
/// 5. Evaluerar i sandbox
/// 6. Applicerar resultaten
pub fn selective_exec(tree: &SemanticTree, html: &str) -> SelectiveExecResult {
    // Steg 1: Detektera JS-snippets
    let detection = js_eval::detect_js_snippets(html);

    let content_affecting = detection
        .snippets
        .iter()
        .filter(|s| s.affects_content)
        .count() as u32;

    let js_analysis = JsAnalysisSummary {
        total_inline_scripts: detection.total_inline_scripts,
        total_event_handlers: detection.total_event_handlers,
        has_framework: detection.has_framework,
        framework_hint: detection.framework_hint,
        evaluable_expressions: 0, // uppdateras nedan
        content_affecting_scripts: content_affecting,
    };

    // Steg 2: Extrahera DOM-targets från alla inline scripts
    let mut all_targets = Vec::new();
    for snippet in &detection.snippets {
        if snippet.snippet_type == js_eval::SnippetType::InlineScript {
            let targets = extract_dom_targets(&snippet.code);
            all_targets.extend(targets);
        }
    }

    // Steg 3: Bygg nod-index (html_id → node_id)
    let node_index = build_node_index(&tree.nodes);

    // Steg 4: Matcha targets till noder och evaluera
    let mut bindings = Vec::new();
    let mut enhanced_ids = Vec::new();
    let mut total_evals = 0u32;
    let mut successful_evals = 0u32;
    let mut evaluable_count = 0u32;

    for target in &all_targets {
        // Skippa variabeltilldelningar som inte har nod-koppling
        if target.selector.starts_with("var:") {
            evaluable_count += 1;
            let eval_result = js_eval::eval_js(&target.expression);
            total_evals += 1;

            let binding = JsNodeBinding {
                node_id: 0,
                target_selector: target.selector.clone(),
                target_property: target.property.clone(),
                expression: target.expression.clone(),
                computed_value: eval_result.value.clone(),
                error: eval_result.error,
            };

            if eval_result.value.is_some() {
                successful_evals += 1;
            }
            bindings.push(binding);
            continue;
        }

        // Matcha selector till nod
        if let Some(&node_id) = node_index.get(&target.selector) {
            evaluable_count += 1;
            let eval_result = js_eval::eval_js(&target.expression);
            total_evals += 1;

            let binding = JsNodeBinding {
                node_id,
                target_selector: target.selector.clone(),
                target_property: target.property.clone(),
                expression: target.expression.clone(),
                computed_value: eval_result.value.clone(),
                error: eval_result.error,
            };

            if eval_result.value.is_some() {
                successful_evals += 1;
                enhanced_ids.push(node_id);
            }
            bindings.push(binding);
        }
    }

    // Steg 5: Applicera resultat till en kopia av trädet
    let mut enhanced_tree = tree.clone();
    for binding in &bindings {
        if binding.node_id > 0 {
            if let Some(computed) = &binding.computed_value {
                apply_to_node(
                    &mut enhanced_tree.nodes,
                    binding.node_id,
                    &binding.target_property,
                    computed,
                );
            }
        }
    }

    let mut result_analysis = js_analysis;
    result_analysis.evaluable_expressions = evaluable_count;

    SelectiveExecResult {
        tree: enhanced_tree,
        js_bindings: bindings,
        enhanced_node_ids: enhanced_ids,
        js_analysis: result_analysis,
        total_evals,
        successful_evals,
        exec_time_ms: 0, // sätts av anroparen
    }
}

/// Bygg index: html_id → node_id (rekursivt)
fn build_node_index(nodes: &[crate::types::SemanticNode]) -> HashMap<String, u32> {
    let mut index = HashMap::new();
    build_node_index_recursive(nodes, &mut index);
    index
}

fn build_node_index_recursive(
    nodes: &[crate::types::SemanticNode],
    index: &mut HashMap<String, u32>,
) {
    for node in nodes {
        if let Some(ref id) = node.html_id {
            index.insert(id.clone(), node.id);
        }
        build_node_index_recursive(&node.children, index);
    }
}

/// Applicera ett beräknat värde till en nod i trädet
fn apply_to_node(
    nodes: &mut [crate::types::SemanticNode],
    node_id: u32,
    property: &str,
    value: &str,
) {
    for node in nodes.iter_mut() {
        if node.id == node_id {
            match property.to_lowercase().as_str() {
                "textcontent" | "innertext" | "innerhtml" => {
                    // Uppdatera label med beräknat värde
                    if node.label.is_empty() || node.label.trim().is_empty() {
                        node.label = value.to_string();
                    } else {
                        // Lägg till som JS-beräknat värde
                        node.value = Some(value.to_string());
                    }
                }
                "value" => {
                    node.value = Some(value.to_string());
                }
                _ => {
                    // Övriga properties: spara som value
                    node.value = Some(value.to_string());
                }
            }
            return;
        }
        apply_to_node(&mut node.children, node_id, property, value);
    }
}

// ─── Tester ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_quoted_string_single() {
        let (s, end) = extract_quoted_string("'hello'").unwrap();
        assert_eq!(s, "hello");
        assert_eq!(end, 7);
    }

    #[test]
    fn test_extract_quoted_string_double() {
        let (s, _) = extract_quoted_string("\"world\"").unwrap();
        assert_eq!(s, "world");
    }

    #[test]
    fn test_extract_dom_targets_getelementbyid() {
        let code = "document.getElementById('price').textContent = (29.99 * 2).toFixed(2);";
        let targets = extract_dom_targets(code);
        assert!(!targets.is_empty(), "Borde hitta getElementById-target");
        assert_eq!(targets[0].selector, "price");
        assert_eq!(targets[0].property, "textContent");
        assert!(
            targets[0].expression.contains("29.99"),
            "Borde extrahera uttrycket"
        );
    }

    #[test]
    fn test_extract_dom_targets_double_quotes() {
        let code = r#"document.getElementById("total").textContent = '$' + total;"#;
        let targets = extract_dom_targets(code);
        assert!(!targets.is_empty(), "Borde hitta target med double quotes");
        assert_eq!(targets[0].selector, "total");
    }

    #[test]
    fn test_extract_dom_targets_queryselector() {
        let code = "document.querySelector('#status').textContent = 'Active';";
        let targets = extract_dom_targets(code);
        assert!(!targets.is_empty(), "Borde hitta querySelector-target");
        assert_eq!(targets[0].selector, "#status");
    }

    #[test]
    fn test_extract_variable_assignments() {
        let code = "var total = 29.99 * 2; let tax = total * 0.25;";
        let targets = extract_dom_targets(code);
        let vars: Vec<_> = targets
            .iter()
            .filter(|t| t.selector.starts_with("var:"))
            .collect();
        assert!(vars.len() >= 2, "Borde hitta variabeltilldelningar");
    }

    #[test]
    fn test_extract_skips_comparisons() {
        let code = "if (price == 0) { return; }";
        let targets = extract_dom_targets(code);
        let vars: Vec<_> = targets
            .iter()
            .filter(|t| t.selector.starts_with("var:"))
            .collect();
        assert!(vars.is_empty(), "Borde inte matcha == som tilldelning");
    }

    #[test]
    fn test_build_node_index() {
        use crate::types::{NodeState, SemanticNode, TrustLevel};
        let nodes = vec![
            SemanticNode {
                id: 1,
                role: "text".to_string(),
                label: "Price".to_string(),
                value: None,
                state: NodeState {
                    disabled: false,
                    checked: None,
                    expanded: None,
                    focused: false,
                    visible: true,
                },
                action: None,
                relevance: 0.5,
                trust: TrustLevel::Untrusted,
                children: vec![],
                html_id: Some("price".to_string()),
                name: None,
                bbox: None,
            },
            SemanticNode {
                id: 2,
                role: "button".to_string(),
                label: "Buy".to_string(),
                value: None,
                state: NodeState {
                    disabled: false,
                    checked: None,
                    expanded: None,
                    focused: false,
                    visible: true,
                },
                action: Some("click".to_string()),
                relevance: 0.9,
                trust: TrustLevel::Untrusted,
                children: vec![],
                html_id: Some("buy-btn".to_string()),
                name: None,
                bbox: None,
            },
        ];

        let index = build_node_index(&nodes);
        assert_eq!(index.get("price"), Some(&1));
        assert_eq!(index.get("buy-btn"), Some(&2));
        assert_eq!(index.get("nonexistent"), None);
    }

    #[cfg(feature = "js-eval")]
    #[test]
    fn test_selective_exec_basic() {
        use crate::parser::parse_html;
        use crate::semantic::{extract_title, SemanticBuilder};

        let html = r##"<html><head><title>Shop</title></head><body>
            <h1>Produktsida</h1>
            <script>document.getElementById('buy').textContent = 'Köp: ' + (29.99 * 2).toFixed(2) + ' kr';</script>
            <a id="buy" href="#">Köp nu</a>
        </body></html>"##;

        let dom = parse_html(html);
        let title = extract_title(&dom);
        let mut builder = SemanticBuilder::new("köp");
        let tree = builder.build(&dom, "https://shop.se", &title);

        let result = selective_exec(&tree, html);

        assert!(
            result.js_analysis.total_inline_scripts >= 1,
            "Borde detektera inline script"
        );
        assert!(!result.js_bindings.is_empty(), "Borde hitta JS-bindningar");

        // Kolla att uttrycket evaluerades och matchade noden
        let buy_binding = result
            .js_bindings
            .iter()
            .find(|b| b.target_selector == "buy");
        assert!(buy_binding.is_some(), "Borde ha binding för buy-elementet");
        assert!(
            buy_binding.unwrap().computed_value.is_some(),
            "Borde ha ett beräknat värde"
        );
    }

    #[cfg(feature = "js-eval")]
    #[test]
    fn test_selective_exec_no_js() {
        use crate::parser::parse_html;
        use crate::semantic::{extract_title, SemanticBuilder};

        let html = r#"<html><body>
            <p>Statisk sida</p>
            <button>Köp</button>
        </body></html>"#;

        let dom = parse_html(html);
        let title = extract_title(&dom);
        let mut builder = SemanticBuilder::new("köp");
        let tree = builder.build(&dom, "https://shop.se", &title);

        let result = selective_exec(&tree, html);

        assert_eq!(result.total_evals, 0, "Ingen JS att evaluera");
        assert!(result.js_bindings.is_empty(), "Inga bindningar utan JS");
        assert_eq!(result.js_analysis.total_inline_scripts, 0);
    }

    #[cfg(feature = "js-eval")]
    #[test]
    fn test_selective_exec_multiple_targets() {
        use crate::parser::parse_html;
        use crate::semantic::{extract_title, SemanticBuilder};

        let html = r##"<html><head><title>Kassa</title></head><body>
            <h1>Betalning</h1>
            <script>
                document.getElementById('pay-btn').textContent = 'Betala ' + (199 * 3).toString() + ' kr';
                document.getElementById('cart-link').textContent = (199 * 3 * 0.25).toFixed(2) + ' kr moms';
            </script>
            <button id="pay-btn">Betala</button>
            <a id="cart-link" href="#">Kundvagn</a>
        </body></html>"##;

        let dom = parse_html(html);
        let title = extract_title(&dom);
        let mut builder = SemanticBuilder::new("betala");
        let tree = builder.build(&dom, "https://shop.se", &title);

        let result = selective_exec(&tree, html);

        assert!(
            result.js_bindings.len() >= 2,
            "Borde hitta minst 2 bindningar, fick {}",
            result.js_bindings.len()
        );
        assert!(
            result.successful_evals >= 2,
            "Borde ha minst 2 lyckade evalueringar, fick {}",
            result.successful_evals
        );
    }
}
