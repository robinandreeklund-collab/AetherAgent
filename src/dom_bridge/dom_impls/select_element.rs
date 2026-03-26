// ─── HTMLSelectElement Behavior ──────────────────────────────────────────────
//
// Implementerar selectedIndex, options, value, och add/remove.
//
// Spec: https://html.spec.whatwg.org/multipage/form-elements.html#the-select-element

use crate::arena_dom::NodeKey;

use super::super::state::BridgeState;

/// Hämta alla <option> barn (rekursivt genom <optgroup>)
pub(in crate::dom_bridge) fn get_options(state: &BridgeState, key: NodeKey) -> Vec<NodeKey> {
    let mut options = vec![];
    collect_options(state, key, &mut options);
    options
}

fn collect_options(state: &BridgeState, key: NodeKey, results: &mut Vec<NodeKey>) {
    if let Some(node) = state.arena.nodes.get(key) {
        for &child in &node.children {
            if let Some(cn) = state.arena.nodes.get(child) {
                match cn.tag.as_deref() {
                    Some("option") => results.push(child),
                    Some("optgroup") => collect_options(state, child, results),
                    _ => {}
                }
            }
        }
    }
}

/// Hämta selectedIndex (första selected option, eller 0 för select-one utan explicit selected)
pub(in crate::dom_bridge) fn get_selected_index(state: &BridgeState, key: NodeKey) -> i32 {
    let key_bits = super::super::node_key_to_f64(key) as u64;
    // Kolla om vi har en programmatisk selectedIndex
    if let Some(es) = state.element_state.get(&key_bits) {
        if let Some(idx) = es.selected_index {
            return idx;
        }
    }

    let options = get_options(state, key);
    if options.is_empty() {
        return -1;
    }

    // Hitta första option med selected-attribut
    for (i, &opt_key) in options.iter().enumerate() {
        let opt_bits = super::super::node_key_to_f64(opt_key) as u64;
        // Kolla intern state först
        if let Some(es) = state.element_state.get(&opt_bits) {
            if let Some(selected) = es.selected {
                if selected {
                    return i as i32;
                }
                continue;
            }
        }
        // Annars content attribute
        if let Some(n) = state.arena.nodes.get(opt_key) {
            if n.has_attr("selected") {
                return i as i32;
            }
        }
    }

    // Per spec: select-one utan explicit selected → selectedIndex = 0
    let is_multiple = state
        .arena
        .nodes
        .get(key)
        .map(|n| n.has_attr("multiple"))
        .unwrap_or(false);
    if !is_multiple && !options.is_empty() {
        return 0;
    }

    -1
}

/// Sätt selectedIndex
pub(in crate::dom_bridge) fn set_selected_index(state: &mut BridgeState, key: NodeKey, index: i32) {
    let options = get_options(state, key);

    // Avmarkera alla options
    for &opt_key in &options {
        let opt_bits = super::super::node_key_to_f64(opt_key) as u64;
        let es = state.element_state.entry(opt_bits).or_default();
        es.selected = Some(false);
    }

    // Markera den valda
    if index >= 0 && (index as usize) < options.len() {
        let opt_key = options[index as usize];
        let opt_bits = super::super::node_key_to_f64(opt_key) as u64;
        let es = state.element_state.entry(opt_bits).or_default();
        es.selected = Some(true);
    }

    // Spara selectedIndex i select-elementets state
    let key_bits = super::super::node_key_to_f64(key) as u64;
    let es = state.element_state.entry(key_bits).or_default();
    es.selected_index = Some(index);
}

/// Hämta select.value (value av den valda option)
pub(in crate::dom_bridge) fn get_select_value(state: &BridgeState, key: NodeKey) -> String {
    let idx = get_selected_index(state, key);
    if idx < 0 {
        return String::new();
    }
    let options = get_options(state, key);
    if let Some(&opt_key) = options.get(idx as usize) {
        get_option_value(state, opt_key)
    } else {
        String::new()
    }
}

/// Hämta option.value (value-attribut, eller textContent om value saknas)
pub(in crate::dom_bridge) fn get_option_value(state: &BridgeState, key: NodeKey) -> String {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return String::new(),
    };
    // Per spec: om value-attribut finns, använd det. Annars textContent.
    if let Some(val) = node.get_attr("value") {
        return val.to_string();
    }
    // Fallback: textContent (strippat av inledande/avslutande whitespace)
    get_text_content(state, key).trim().to_string()
}

/// Hämta option.text (textContent strippat)
#[allow(dead_code)]
pub(in crate::dom_bridge) fn get_option_text(state: &BridgeState, key: NodeKey) -> String {
    get_text_content(state, key)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rekursivt hämta textinnehåll
fn get_text_content(state: &BridgeState, key: NodeKey) -> String {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return String::new(),
    };
    if let Some(ref text) = node.text {
        return text.to_string();
    }
    let mut result = String::new();
    for &child in &node.children {
        result.push_str(&get_text_content(state, child));
    }
    result
}

/// Sätt select.value (hitta matchande option och sätt selectedIndex)
pub(in crate::dom_bridge) fn set_select_value(state: &mut BridgeState, key: NodeKey, value: &str) {
    let options = get_options(state, key);
    for (i, &opt_key) in options.iter().enumerate() {
        if get_option_value(state, opt_key) == value {
            set_selected_index(state, key, i as i32);
            return;
        }
    }
    // Ingen matchande option → selectedIndex = -1
    set_selected_index(state, key, -1);
}
