/// Semantic DOM Diffing – Fas 4a
///
/// Jämför två SemanticTree och producerar en minimal delta.
/// Agenten skickar bara förändringarna till LLM:en,
/// vilket minskar token-kostnaden med 80–95% för multi-step flows.
use std::collections::HashMap;

use crate::types::{
    ChangeType, FieldChange, NodeChange, SemanticDelta, SemanticNode, SemanticTree,
};

/// Samla alla noder platt från ett träd (rekursivt)
fn flatten_nodes(nodes: &[SemanticNode]) -> Vec<&SemanticNode> {
    let mut result = vec![];
    for node in nodes {
        result.push(node);
        result.extend(flatten_nodes(&node.children));
    }
    result
}

/// Skapa en matchningsnyckel för en nod (roll + id/name/label)
fn node_key(node: &SemanticNode) -> String {
    // Primärt: html_id (unikt)
    if let Some(ref id) = node.html_id {
        return format!("id:{}", id);
    }
    // Sekundärt: roll + name
    if let Some(ref name) = node.name {
        return format!("{}:name:{}", node.role, name);
    }
    // Tertiärt: roll + label (trunkerad för stabilitet)
    let label_key = if node.label.len() > 60 {
        &node.label[..node.label.floor_char_boundary(60)]
    } else {
        &node.label
    };
    format!("{}:{}", node.role, label_key)
}

/// Jämför två noder och returnera fältförändringar
fn diff_node(old: &SemanticNode, new: &SemanticNode) -> Vec<FieldChange> {
    let mut changes = vec![];

    if old.label != new.label {
        changes.push(FieldChange {
            field: "label".to_string(),
            before: old.label.clone(),
            after: new.label.clone(),
        });
    }

    if old.relevance != new.relevance {
        changes.push(FieldChange {
            field: "relevance".to_string(),
            before: format!("{:.2}", old.relevance),
            after: format!("{:.2}", new.relevance),
        });
    }

    if old.state != new.state {
        if old.state.disabled != new.state.disabled {
            changes.push(FieldChange {
                field: "state.disabled".to_string(),
                before: old.state.disabled.to_string(),
                after: new.state.disabled.to_string(),
            });
        }
        if old.state.visible != new.state.visible {
            changes.push(FieldChange {
                field: "state.visible".to_string(),
                before: old.state.visible.to_string(),
                after: new.state.visible.to_string(),
            });
        }
        if old.state.checked != new.state.checked {
            changes.push(FieldChange {
                field: "state.checked".to_string(),
                before: format!("{:?}", old.state.checked),
                after: format!("{:?}", new.state.checked),
            });
        }
        if old.state.expanded != new.state.expanded {
            changes.push(FieldChange {
                field: "state.expanded".to_string(),
                before: format!("{:?}", old.state.expanded),
                after: format!("{:?}", new.state.expanded),
            });
        }
    }

    if old.value != new.value {
        changes.push(FieldChange {
            field: "value".to_string(),
            before: old.value.clone().unwrap_or_default(),
            after: new.value.clone().unwrap_or_default(),
        });
    }

    if old.action != new.action {
        changes.push(FieldChange {
            field: "action".to_string(),
            before: old.action.clone().unwrap_or_default(),
            after: new.action.clone().unwrap_or_default(),
        });
    }

    if old.trust != new.trust {
        changes.push(FieldChange {
            field: "trust".to_string(),
            before: format!("{:?}", old.trust),
            after: format!("{:?}", new.trust),
        });
    }

    if old.role != new.role {
        changes.push(FieldChange {
            field: "role".to_string(),
            before: old.role.clone(),
            after: new.role.clone(),
        });
    }

    changes
}

/// Beräkna semantisk diff mellan två träd
pub fn diff_trees(old: &SemanticTree, new: &SemanticTree) -> SemanticDelta {
    let old_flat = flatten_nodes(&old.nodes);
    let new_flat = flatten_nodes(&new.nodes);

    let total_before = old_flat.len() as u32;
    let total_after = new_flat.len() as u32;

    // Bygg index: key → nod
    let mut old_map: HashMap<String, &SemanticNode> = HashMap::new();
    for node in &old_flat {
        old_map.insert(node_key(node), node);
    }

    let mut new_map: HashMap<String, &SemanticNode> = HashMap::new();
    for node in &new_flat {
        new_map.insert(node_key(node), node);
    }

    let mut changes = vec![];

    // Hitta modifierade och borttagna noder
    for (key, old_node) in &old_map {
        if let Some(new_node) = new_map.get(key) {
            // Noden finns i båda – kolla om den förändrats
            let field_changes = diff_node(old_node, new_node);
            if !field_changes.is_empty() {
                changes.push(NodeChange {
                    node_id: new_node.id,
                    change_type: ChangeType::Modified,
                    role: new_node.role.clone(),
                    label: new_node.label.clone(),
                    changes: field_changes,
                });
            }
        } else {
            // Noden finns bara i old → borttagen
            changes.push(NodeChange {
                node_id: old_node.id,
                change_type: ChangeType::Removed,
                role: old_node.role.clone(),
                label: old_node.label.clone(),
                changes: vec![],
            });
        }
    }

    // Hitta tillagda noder
    for (key, new_node) in &new_map {
        if !old_map.contains_key(key) {
            changes.push(NodeChange {
                node_id: new_node.id,
                change_type: ChangeType::Added,
                role: new_node.role.clone(),
                label: new_node.label.clone(),
                changes: vec![],
            });
        }
    }

    // Sortera: Modified först (viktigast), sedan Added, sedan Removed
    changes.sort_by(|a, b| {
        let type_order = |t: &ChangeType| -> u8 {
            match t {
                ChangeType::Modified => 0,
                ChangeType::Added => 1,
                ChangeType::Removed => 2,
            }
        };
        type_order(&a.change_type).cmp(&type_order(&b.change_type))
    });

    // Beräkna token-besparing
    let max_nodes = total_before.max(total_after) as f32;
    let token_savings_ratio = if max_nodes > 0.0 {
        1.0 - (changes.len() as f32 / max_nodes)
    } else {
        0.0
    }
    .clamp(0.0, 1.0);

    // Generera sammanfattning
    let modified_count = changes
        .iter()
        .filter(|c| c.change_type == ChangeType::Modified)
        .count();
    let added_count = changes
        .iter()
        .filter(|c| c.change_type == ChangeType::Added)
        .count();
    let removed_count = changes
        .iter()
        .filter(|c| c.change_type == ChangeType::Removed)
        .count();

    let summary = if changes.is_empty() {
        "No changes detected".to_string()
    } else {
        let mut parts = vec![];
        if modified_count > 0 {
            parts.push(format!("{} modified", modified_count));
        }
        if added_count > 0 {
            parts.push(format!("{} added", added_count));
        }
        if removed_count > 0 {
            parts.push(format!("{} removed", removed_count));
        }
        format!(
            "{} changes ({}), {:.0}% token savings",
            changes.len(),
            parts.join(", "),
            token_savings_ratio * 100.0
        )
    };

    SemanticDelta {
        url: new.url.clone(),
        goal: new.goal.clone(),
        total_nodes_before: total_before,
        total_nodes_after: total_after,
        changes,
        token_savings_ratio,
        summary,
        diff_time_ms: 0, // sätts av anroparen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_html;
    use crate::semantic::{extract_title, SemanticBuilder};

    fn build_tree(html: &str, goal: &str) -> SemanticTree {
        let dom = parse_html(html);
        let title = extract_title(&dom);
        let mut builder = SemanticBuilder::new(goal);
        builder.build(&dom, "https://test.com", &title)
    }

    #[test]
    fn test_identical_trees_no_changes() {
        let html = r#"<html><body><button>Köp</button></body></html>"#;
        let tree1 = build_tree(html, "köp");
        let tree2 = build_tree(html, "köp");
        let delta = diff_trees(&tree1, &tree2);

        assert!(
            delta.changes.is_empty(),
            "Identiska träd borde ge 0 förändringar"
        );
        assert_eq!(delta.summary, "No changes detected");
        assert!((delta.token_savings_ratio - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_added_node() {
        let html1 = r#"<html><body><button>Köp</button></body></html>"#;
        let html2 =
            r#"<html><body><button>Köp</button><button id="new">Nytt</button></body></html>"#;
        let tree1 = build_tree(html1, "köp");
        let tree2 = build_tree(html2, "köp");
        let delta = diff_trees(&tree1, &tree2);

        let added: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Added)
            .collect();
        assert!(!added.is_empty(), "Borde detektera tillagd nod");
    }

    #[test]
    fn test_removed_node() {
        let html1 =
            r#"<html><body><button>Köp</button><button id="old">Gammal</button></body></html>"#;
        let html2 = r#"<html><body><button>Köp</button></body></html>"#;
        let tree1 = build_tree(html1, "köp");
        let tree2 = build_tree(html2, "köp");
        let delta = diff_trees(&tree1, &tree2);

        let removed: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Removed)
            .collect();
        assert!(!removed.is_empty(), "Borde detektera borttagen nod");
    }

    #[test]
    fn test_modified_label() {
        let html1 = r#"<html><body><button id="cart">0 i varukorg</button></body></html>"#;
        let html2 = r#"<html><body><button id="cart">1 i varukorg</button></body></html>"#;
        let tree1 = build_tree(html1, "köp");
        let tree2 = build_tree(html2, "köp");
        let delta = diff_trees(&tree1, &tree2);

        let modified: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Modified)
            .collect();
        assert!(!modified.is_empty(), "Borde detektera ändrad label");

        let label_change = modified[0]
            .changes
            .iter()
            .find(|c| c.field == "label")
            .expect("Borde ha label-förändring");
        assert_eq!(label_change.before, "0 i varukorg");
        assert_eq!(label_change.after, "1 i varukorg");
    }

    #[test]
    fn test_modified_state_disabled() {
        let html1 = r#"<html><body><button id="pay" disabled>Betala</button></body></html>"#;
        let html2 = r#"<html><body><button id="pay">Betala</button></body></html>"#;
        let tree1 = build_tree(html1, "betala");
        let tree2 = build_tree(html2, "betala");
        let delta = diff_trees(&tree1, &tree2);

        let modified: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Modified)
            .collect();
        assert!(!modified.is_empty(), "Borde detektera state-förändring");

        let state_change = modified
            .iter()
            .flat_map(|c| &c.changes)
            .find(|c| c.field == "state.disabled");
        assert!(state_change.is_some(), "Borde ha disabled-förändring");
        assert_eq!(state_change.unwrap().before, "true");
        assert_eq!(state_change.unwrap().after, "false");
    }

    #[test]
    fn test_goal_change_affects_relevance() {
        let html = r#"<html><body>
            <button id="buy">Köp nu</button>
            <button id="cancel">Avbryt</button>
        </body></html>"#;
        let tree1 = build_tree(html, "köp");
        let tree2 = build_tree(html, "avbryt");
        let delta = diff_trees(&tree1, &tree2);

        let relevance_changes: Vec<_> = delta
            .changes
            .iter()
            .filter(|c| c.changes.iter().any(|f| f.field == "relevance"))
            .collect();
        assert!(
            !relevance_changes.is_empty(),
            "Byte av goal borde ändra relevans"
        );
    }

    #[test]
    fn test_token_savings_ratio() {
        // Stort träd med liten förändring → hög besparing
        let html1 = r##"<html><body>
            <button id="b1">Knapp 1</button>
            <button id="b2">Knapp 2</button>
            <button id="b3">Knapp 3</button>
            <button id="b4">Knapp 4</button>
            <button id="b5">Knapp 5</button>
            <a href="#">Länk 1</a>
            <a href="#">Länk 2</a>
            <input id="i1" placeholder="Fält 1" />
            <input id="i2" placeholder="Fält 2" />
            <p>Text som inte ändras</p>
        </body></html>"##;
        let html2 = r##"<html><body>
            <button id="b1">Knapp 1 (ändrad)</button>
            <button id="b2">Knapp 2</button>
            <button id="b3">Knapp 3</button>
            <button id="b4">Knapp 4</button>
            <button id="b5">Knapp 5</button>
            <a href="#">Länk 1</a>
            <a href="#">Länk 2</a>
            <input id="i1" placeholder="Fält 1" />
            <input id="i2" placeholder="Fält 2" />
            <p>Text som inte ändras</p>
        </body></html>"##;

        let tree1 = build_tree(html1, "test");
        let tree2 = build_tree(html2, "test");
        let delta = diff_trees(&tree1, &tree2);

        assert!(
            delta.token_savings_ratio > 0.5,
            "En förändring av 10+ noder borde ge >50% besparing, fick {}",
            delta.token_savings_ratio
        );
    }

    #[test]
    fn test_ecommerce_checkout_flow() {
        // Simulera: produktsida → kassan
        let product_html = r##"<html><body>
            <h1>iPhone 16 Pro</h1>
            <p>13 990 kr</p>
            <button id="add-cart">Lägg i varukorg</button>
            <a href="/kassa">Gå till kassan</a>
        </body></html>"##;
        let checkout_html = r##"<html><body>
            <h1>Kassa</h1>
            <p>1 artikel – 13 990 kr</p>
            <input id="email" name="email" placeholder="E-post" />
            <input id="address" name="address" placeholder="Adress" />
            <button id="pay-btn">Betala 13 990 kr</button>
        </body></html>"##;

        let tree1 = build_tree(product_html, "köp produkt");
        let tree2 = build_tree(checkout_html, "köp produkt");
        let delta = diff_trees(&tree1, &tree2);

        assert!(
            !delta.changes.is_empty(),
            "Sidnavigering borde ge förändringar"
        );
        assert!(
            delta.summary.contains("changes"),
            "Sammanfattning borde beskriva ändringarna"
        );
    }

    #[test]
    fn test_empty_trees() {
        let tree1 = SemanticTree {
            url: "https://test.com".to_string(),
            title: String::new(),
            goal: "test".to_string(),
            nodes: vec![],
            injection_warnings: vec![],
            parse_time_ms: 0,
        };
        let tree2 = tree1.clone();
        let delta = diff_trees(&tree1, &tree2);

        assert!(delta.changes.is_empty());
        assert_eq!(delta.summary, "No changes detected");
    }
}
