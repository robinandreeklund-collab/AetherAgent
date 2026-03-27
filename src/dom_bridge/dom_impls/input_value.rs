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

/// Hämta input.value respekterande value mode + dirty flag + sanitization
pub(in crate::dom_bridge) fn get_input_value(state: &BridgeState, key: NodeKey) -> String {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return String::new(),
    };
    let input_type = node.get_attr("type").unwrap_or("text");
    let mode = get_value_mode(input_type);
    let key_bits = super::super::node_key_to_f64(key) as u64;

    let raw = match mode {
        "value" => {
            // "value" mode: dirty flag → använd intern state, annars attribut
            if let Some(es) = state.element_state.get(&key_bits) {
                if es.value_dirty {
                    return sanitize_value(input_type, &es.value.clone().unwrap_or_default(), node);
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
            // "filename" mode: kan inte sättas programmatiskt
            if let Some(es) = state.element_state.get(&key_bits) {
                if let Some(ref v) = es.value {
                    return v.clone();
                }
            }
            return String::new();
        }
        _ => return String::new(),
    };
    sanitize_value(input_type, &raw, node)
}

/// Value Sanitization Algorithm per HTML spec
/// https://html.spec.whatwg.org/multipage/input.html#value-sanitization-algorithm
fn sanitize_value(input_type: &str, value: &str, node: &crate::arena_dom::DomNode) -> String {
    match input_type {
        // Text/Search/Tel/Password: strip newlines
        "text" | "search" | "tel" | "password" => value.replace(['\n', '\r'], ""),
        // URL: strip newlines + leading/trailing whitespace
        "url" => value.replace(['\n', '\r'], "").trim().to_string(),
        // Email: strip newlines + leading/trailing whitespace
        // Multiple: also per-address
        "email" => value.replace(['\n', '\r'], "").trim().to_string(),
        // Color: lowercase #rrggbb, default #000000
        "color" => sanitize_color(value),
        // Number: must be valid floating-point number per spec, else empty
        // Per spec: no leading +, no trailing dot, must be parseable
        "number" => {
            if value.is_empty() {
                return String::new();
            }
            // Ogiltig: börjar med +, slutar med ., eller inte parserbart
            if value.starts_with('+') || value.ends_with('.') {
                return String::new();
            }
            match value.parse::<f64>() {
                Ok(n) if n.is_finite() => value.to_string(),
                _ => String::new(),
            }
        }
        // Range: must be valid float, clamped to [min, max], default = midpoint
        "range" => {
            let min = node
                .get_attr("min")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            let max = node
                .get_attr("max")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(100.0);
            let max = if max < min { min } else { max };
            let step = node
                .get_attr("step")
                .and_then(|v| {
                    if v == "any" {
                        None
                    } else {
                        v.parse::<f64>().ok()
                    }
                })
                .unwrap_or(1.0);

            let num = value.parse::<f64>().ok().filter(|n| n.is_finite());
            let val = match num {
                Some(n) => {
                    // Clamp till [min, max]
                    let clamped = n.max(min).min(max);
                    // Step-align (närmaste steg från min)
                    if step > 0.0 {
                        let steps = ((clamped - min) / step).round();
                        let aligned = min + steps * step;
                        aligned.min(max)
                    } else {
                        clamped
                    }
                }
                None => {
                    // Default value: midpoint
                    let mid = min + (max - min) / 2.0;
                    // Step-align midpoint
                    if step > 0.0 {
                        let steps = ((mid - min) / step).round();
                        (min + steps * step).min(max)
                    } else {
                        mid
                    }
                }
            };
            if val == val.floor() {
                format!("{}", val as i64)
            } else {
                format!("{}", val)
            }
        }
        // Date: YYYY-MM-DD format validation
        "date" => {
            if is_valid_date_string(value) {
                value.to_string()
            } else {
                String::new()
            }
        }
        // Time: HH:MM[:SS[.mmm]] validation
        "time" => {
            if is_valid_time_string(value) {
                value.to_string()
            } else {
                String::new()
            }
        }
        // Month: YYYY-MM validation
        "month" => {
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() == 2 {
                if let (Ok(y), Ok(m)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                    if y > 0 && (1..=12).contains(&m) {
                        return value.to_string();
                    }
                }
            }
            String::new()
        }
        // Week: YYYY-Www validation
        "week" => {
            if value.len() >= 8 && value.contains("-W") {
                let parts: Vec<&str> = value.split("-W").collect();
                if parts.len() == 2 {
                    if let (Ok(y), Ok(w)) = (parts[0].parse::<i32>(), parts[1].parse::<u32>()) {
                        if y > 0 && (1..=53).contains(&w) {
                            return value.to_string();
                        }
                    }
                }
            }
            String::new()
        }
        // Datetime-local: YYYY-MM-DDTHH:MM[:SS[.mmm]] (T eller mellanslag som separator)
        "datetime-local" => {
            // Normalisera: mellanslag → T
            let normalized = value.replace(' ', "T");
            let parts: Vec<&str> = normalized.splitn(2, 'T').collect();
            if parts.len() == 2 && is_valid_date_string(parts[0]) && is_valid_time_string(parts[1])
            {
                // Returnera med T som separator (kanonisk form)
                format!("{}T{}", parts[0], parts[1])
            } else {
                String::new()
            }
        }
        _ => value.to_string(),
    }
}

/// Sanitize color value: lowercase valid #rrggbb, else #000000
fn sanitize_color(value: &str) -> String {
    let v = value.trim();
    if v.len() == 7 && v.starts_with('#') {
        let hex = &v[1..];
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("#{}", hex.to_ascii_lowercase());
        }
    }
    "#000000".to_string()
}

fn is_valid_date_string(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    let y = parts[0].parse::<i32>().unwrap_or(0);
    let m = parts[1].parse::<u32>().unwrap_or(0);
    let d = parts[2].parse::<u32>().unwrap_or(0);
    y > 0 && (1..=12).contains(&m) && (1..=31).contains(&d)
}

fn is_valid_time_string(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() < 2 {
        return false;
    }
    let h = parts[0].parse::<u32>().unwrap_or(99);
    let m = parts[1].parse::<u32>().unwrap_or(99);
    h <= 23 && m <= 59
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
