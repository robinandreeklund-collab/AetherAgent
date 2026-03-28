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
/// Hanterar #rgb (kort) och #rrggbb (full), samt namngivna CSS-färger.
fn sanitize_color(value: &str) -> String {
    let v = value.trim();
    // #rrggbb
    if v.len() == 7 && v.starts_with('#') {
        let hex = &v[1..];
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return format!("#{}", hex.to_ascii_lowercase());
        }
    }
    // #rgb → #rrggbb
    if v.len() == 4 && v.starts_with('#') {
        let hex = &v[1..];
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let chars: Vec<char> = hex.chars().collect();
            return format!(
                "#{0}{0}{1}{1}{2}{2}",
                chars[0].to_ascii_lowercase(),
                chars[1].to_ascii_lowercase(),
                chars[2].to_ascii_lowercase()
            );
        }
    }
    // Namngivna CSS-färger (vanligaste)
    match v.to_ascii_lowercase().as_str() {
        "aliceblue" => "#f0f8ff",
        "antiquewhite" => "#faebd7",
        "aqua" | "cyan" => "#00ffff",
        "aquamarine" => "#7fffd4",
        "azure" => "#f0ffff",
        "beige" => "#f5f5dc",
        "bisque" => "#ffe4c4",
        "black" => "#000000",
        "blanchedalmond" => "#ffebcd",
        "blue" => "#0000ff",
        "blueviolet" => "#8a2be2",
        "brown" => "#a52a2a",
        "burlywood" => "#deb887",
        "cadetblue" => "#5f9ea0",
        "chartreuse" => "#7fff00",
        "chocolate" => "#d2691e",
        "coral" => "#ff7f50",
        "cornflowerblue" => "#6495ed",
        "cornsilk" => "#fff8dc",
        "crimson" => "#dc143c",
        "darkblue" => "#00008b",
        "darkcyan" => "#008b8b",
        "darkgoldenrod" => "#b8860b",
        "darkgray" | "darkgrey" => "#a9a9a9",
        "darkgreen" => "#006400",
        "darkkhaki" => "#bdb76b",
        "darkmagenta" => "#8b008b",
        "darkolivegreen" => "#556b2f",
        "darkorange" => "#ff8c00",
        "darkorchid" => "#9932cc",
        "darkred" => "#8b0000",
        "darksalmon" => "#e9967a",
        "darkseagreen" => "#8fbc8f",
        "darkslateblue" => "#483d8b",
        "darkslategray" | "darkslategrey" => "#2f4f4f",
        "darkturquoise" => "#00ced1",
        "darkviolet" => "#9400d3",
        "deeppink" => "#ff1493",
        "deepskyblue" => "#00bfff",
        "dimgray" | "dimgrey" => "#696969",
        "dodgerblue" => "#1e90ff",
        "firebrick" => "#b22222",
        "floralwhite" => "#fffaf0",
        "forestgreen" => "#228b22",
        "fuchsia" | "magenta" => "#ff00ff",
        "gainsboro" => "#dcdcdc",
        "ghostwhite" => "#f8f8ff",
        "gold" => "#ffd700",
        "goldenrod" => "#daa520",
        "gray" | "grey" => "#808080",
        "green" => "#008000",
        "greenyellow" => "#adff2f",
        "honeydew" => "#f0fff0",
        "hotpink" => "#ff69b4",
        "indianred" => "#cd5c5c",
        "indigo" => "#4b0082",
        "ivory" => "#fffff0",
        "khaki" => "#f0e68c",
        "lavender" => "#e6e6fa",
        "lavenderblush" => "#fff0f5",
        "lawngreen" => "#7cfc00",
        "lemonchiffon" => "#fffacd",
        "lightblue" => "#add8e6",
        "lightcoral" => "#f08080",
        "lightcyan" => "#e0ffff",
        "lightgoldenrodyellow" => "#fafad2",
        "lightgray" | "lightgrey" => "#d3d3d3",
        "lightgreen" => "#90ee90",
        "lightpink" => "#ffb6c1",
        "lightsalmon" => "#ffa07a",
        "lightseagreen" => "#20b2aa",
        "lightskyblue" => "#87cefa",
        "lightslategray" | "lightslategrey" => "#778899",
        "lightsteelblue" => "#b0c4de",
        "lightyellow" => "#ffffe0",
        "lime" => "#00ff00",
        "limegreen" => "#32cd32",
        "linen" => "#faf0e6",
        "maroon" => "#800000",
        "mediumaquamarine" => "#66cdaa",
        "mediumblue" => "#0000cd",
        "mediumorchid" => "#ba55d3",
        "mediumpurple" => "#9370db",
        "mediumseagreen" => "#3cb371",
        "mediumslateblue" => "#7b68ee",
        "mediumspringgreen" => "#00fa9a",
        "mediumturquoise" => "#48d1cc",
        "mediumvioletred" => "#c71585",
        "midnightblue" => "#191970",
        "mintcream" => "#f5fffa",
        "mistyrose" => "#ffe4e1",
        "moccasin" => "#ffe4b5",
        "navajowhite" => "#ffdead",
        "navy" => "#000080",
        "oldlace" => "#fdf5e6",
        "olive" => "#808000",
        "olivedrab" => "#6b8e23",
        "orange" => "#ffa500",
        "orangered" => "#ff4500",
        "orchid" => "#da70d6",
        "palegoldenrod" => "#eee8aa",
        "palegreen" => "#98fb98",
        "paleturquoise" => "#afeeee",
        "palevioletred" => "#db7093",
        "papayawhip" => "#ffefd5",
        "peachpuff" => "#ffdab9",
        "peru" => "#cd853f",
        "pink" => "#ffc0cb",
        "plum" => "#dda0dd",
        "powderblue" => "#b0e0e6",
        "purple" => "#800080",
        "rebeccapurple" => "#663399",
        "red" => "#ff0000",
        "rosybrown" => "#bc8f8f",
        "royalblue" => "#4169e1",
        "saddlebrown" => "#8b4513",
        "salmon" => "#fa8072",
        "sandybrown" => "#f4a460",
        "seagreen" => "#2e8b57",
        "seashell" => "#fff5ee",
        "sienna" => "#a0522d",
        "silver" => "#c0c0c0",
        "skyblue" => "#87ceeb",
        "slateblue" => "#6a5acd",
        "slategray" | "slategrey" => "#708090",
        "snow" => "#fffafa",
        "springgreen" => "#00ff7f",
        "steelblue" => "#4682b4",
        "tan" => "#d2b48c",
        "teal" => "#008080",
        "thistle" => "#d8bfd8",
        "tomato" => "#ff6347",
        "turquoise" => "#40e0d0",
        "violet" => "#ee82ee",
        "wheat" => "#f5deb3",
        "white" => "#ffffff",
        "whitesmoke" => "#f5f5f5",
        "yellow" => "#ffff00",
        "yellowgreen" => "#9acd32",
        _ => "#000000",
    }
    .to_string()
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
