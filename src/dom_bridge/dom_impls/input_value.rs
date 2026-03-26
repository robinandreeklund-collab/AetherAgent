// ─── Input Value Modes & Dirty State ─────────────────────────────────────────
//
// Implementerar HTML spec value modes för <input>:
//   "value"      → text, search, url, tel, email, password, number, range, color
//   "default"    → hidden, submit, image, reset, button
//   "default/on" → checkbox, radio
//   "filename"   → file
//
// Portad från jsdom HTMLInputElement-impl.js value mode-logik.
//
// Spec: https://html.spec.whatwg.org/multipage/input.html#dom-input-value

use crate::arena_dom::NodeKey;

use super::super::element_state::ElementState;
use super::super::state::BridgeState;

/// Bestäm value mode baserat på input type
pub(in crate::dom_bridge) fn get_value_mode(input_type: &str) -> &'static str {
    match input_type {
        "hidden" | "submit" | "image" | "reset" | "button" => "default",
        "checkbox" | "radio" => "default/on",
        "file" => "filename",
        _ => "value", // text, search, url, tel, email, password, number, range, color, date, time, etc.
    }
}

/// Hämta input.value respekterande value mode + dirty flag
pub(in crate::dom_bridge) fn get_input_value(state: &BridgeState, key: NodeKey) -> String {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return String::new(),
    };
    let input_type = node.get_attr("type").unwrap_or("text");
    let mode = get_value_mode(input_type);
    let key_bits = super::super::node_key_to_f64(key) as u64;

    match mode {
        "value" => {
            // "value" mode: dirty flag → använd intern state, annars attribut
            if let Some(es) = state.element_state.get(&key_bits) {
                if es.value_dirty {
                    return es.value.clone().unwrap_or_default();
                }
            }
            // Icke-dirty: läs default value (value-attribut)
            node.get_attr("value").unwrap_or("").to_string()
        }
        "default" => {
            // "default" mode: alltid content attribute value
            node.get_attr("value").unwrap_or("").to_string()
        }
        "default/on" => {
            // "default/on" mode: content attribute, default "on"
            node.get_attr("value").unwrap_or("on").to_string()
        }
        "filename" => {
            // "filename" mode: kan inte sättas programmatiskt, returnerar "C:\fakepath\filename"
            if let Some(es) = state.element_state.get(&key_bits) {
                if let Some(ref v) = es.value {
                    return v.clone();
                }
            }
            String::new()
        }
        _ => String::new(),
    }
}

/// Sätt input.value respekterande value mode
pub(in crate::dom_bridge) fn set_input_value(state: &mut BridgeState, key: NodeKey, val: &str) {
    let input_type = state
        .arena
        .nodes
        .get(key)
        .and_then(|n| n.get_attr("type"))
        .unwrap_or("text")
        .to_string();
    let mode = get_value_mode(&input_type);
    let key_bits = super::super::node_key_to_f64(key) as u64;

    match mode {
        "value" => {
            // Sätt intern state + dirty flag
            let es = state.element_state.entry(key_bits).or_default();
            es.value = Some(val.to_string());
            es.value_dirty = true;
        }
        "default" => {
            // Skriver till content attribute
            if let Some(node) = state.arena.nodes.get_mut(key) {
                node.set_attr("value", val);
            }
        }
        "default/on" => {
            // Skriver till content attribute
            if let Some(node) = state.arena.nodes.get_mut(key) {
                node.set_attr("value", val);
            }
        }
        "filename" => {
            // file input: kan bara sättas till tom sträng programmatiskt (per spec)
            if val.is_empty() {
                let es = state.element_state.entry(key_bits).or_default();
                es.value = Some(String::new());
            }
            // Icke-tom sträng → InvalidStateError (hanteras av JS-lager)
        }
        _ => {}
    }
}

/// Hämta input.defaultValue (alltid content attribute)
pub(in crate::dom_bridge) fn get_default_value(state: &BridgeState, key: NodeKey) -> String {
    state
        .arena
        .nodes
        .get(key)
        .and_then(|n| n.get_attr("value"))
        .unwrap_or("")
        .to_string()
}

/// Hämta input.checked respekterande dirty checkedness flag
pub(in crate::dom_bridge) fn get_input_checked(state: &BridgeState, key: NodeKey) -> bool {
    let key_bits = super::super::node_key_to_f64(key) as u64;
    if let Some(es) = state.element_state.get(&key_bits) {
        if es.checked_dirty {
            return es.checked.unwrap_or(false);
        }
    }
    // Icke-dirty: läs content attribute
    state
        .arena
        .nodes
        .get(key)
        .map(|n| n.has_attr("checked"))
        .unwrap_or(false)
}

/// Sätt input.checked + hantera radio group unchecking
pub(in crate::dom_bridge) fn set_input_checked(
    state: &mut BridgeState,
    key: NodeKey,
    checked: bool,
) {
    let key_bits = super::super::node_key_to_f64(key) as u64;

    // Kontrollera om det är en radio button
    let (is_radio, radio_name) = {
        let node = match state.arena.nodes.get(key) {
            Some(n) => n,
            None => return,
        };
        let input_type = node.get_attr("type").unwrap_or("text");
        if input_type == "radio" {
            (true, node.get_attr("name").unwrap_or("").to_string())
        } else {
            (false, String::new())
        }
    };

    // Om radio och checked=true, unchecka alla andra i samma grupp
    if is_radio && checked && !radio_name.is_empty() {
        uncheck_radio_group(state, key, &radio_name);
    }

    let es = state.element_state.entry(key_bits).or_default();
    es.checked = Some(checked);
    es.checked_dirty = true;
}

/// Unchecka alla radio buttons i samma grupp utom den givna
fn uncheck_radio_group(state: &mut BridgeState, except_key: NodeKey, name: &str) {
    // Samla alla radio-nyckar i gruppen
    let form_key = super::super::computed::find_form_owner(state, except_key);
    let search_root = form_key.unwrap_or(state.arena.document);
    let mut radios = vec![];
    collect_radio_keys(state, search_root, name, except_key, &mut radios);

    // Unchecka alla
    for radio_key in radios {
        let radio_bits = super::super::node_key_to_f64(radio_key) as u64;
        let es = state.element_state.entry(radio_bits).or_default();
        es.checked = Some(false);
        es.checked_dirty = true;
    }
}

fn collect_radio_keys(
    state: &BridgeState,
    node: NodeKey,
    name: &str,
    except: NodeKey,
    results: &mut Vec<NodeKey>,
) {
    if let Some(n) = state.arena.nodes.get(node) {
        if node != except
            && n.tag.as_deref() == Some("input")
            && n.get_attr("type") == Some("radio")
            && n.get_attr("name") == Some(name)
        {
            results.push(node);
        }
        for &child in &n.children {
            collect_radio_keys(state, child, name, except, results);
        }
    }
}

/// Resetform: rensa dirty flags för alla controls i ett formulär
pub(in crate::dom_bridge) fn reset_element_state(es: &mut ElementState) {
    es.value = None;
    es.checked = None;
    es.value_dirty = false;
    es.checked_dirty = false;
    es.selected_index = None;
    es.selected = None;
}
