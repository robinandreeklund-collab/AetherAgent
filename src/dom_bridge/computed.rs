// ─── Computed DOM Properties & Behaviors ──────────────────────────────────────
//
// Riktiga DOM-beteenden som genererad kod anropar.
// Varje funktion tar &BridgeState + NodeKey och beräknar resultat.

use crate::arena_dom::{NodeKey, NodeType};

use super::state::BridgeState;

// ─── Element Value ───────────────────────────────────────────────────────────

/// Hämta effektivt value för ett form control (state → attribut fallback)
pub(super) fn get_effective_value(state: &BridgeState, key: NodeKey) -> String {
    let key_bits = super::node_key_to_f64(key) as u64;
    if let Some(es) = state.element_state.get(&key_bits) {
        if let Some(ref val) = es.value {
            return val.clone();
        }
    }
    state
        .arena
        .nodes
        .get(key)
        .and_then(|n| n.get_attr("value"))
        .unwrap_or("")
        .to_string()
}

/// Hämta effektivt checked-state för input
#[allow(dead_code)]
pub(super) fn get_effective_checked(state: &BridgeState, key: NodeKey) -> bool {
    let key_bits = super::node_key_to_f64(key) as u64;
    if let Some(es) = state.element_state.get(&key_bits) {
        if let Some(checked) = es.checked {
            return checked;
        }
    }
    state
        .arena
        .nodes
        .get(key)
        .map(|n| n.has_attr("checked"))
        .unwrap_or(false)
}

// ─── Form Association ────────────────────────────────────────────────────────

/// Hitta form-owner (närmaste <form> ancestor, eller explicit form=id)
#[allow(dead_code)]
pub(super) fn find_form_owner(state: &BridgeState, key: NodeKey) -> Option<NodeKey> {
    let node = state.arena.nodes.get(key)?;
    // 1. Explicit form= attribut (form override)
    if let Some(form_id) = node.get_attr("form") {
        return find_element_by_id(state, form_id);
    }
    // 2. Närmaste <form> ancestor
    let mut current = node.parent;
    while let Some(pk) = current {
        let parent = state.arena.nodes.get(pk)?;
        if parent.tag.as_deref() == Some("form") {
            return Some(pk);
        }
        current = parent.parent;
    }
    None
}

/// Hitta element by id i hela DOM
#[allow(dead_code)]
fn find_element_by_id(state: &BridgeState, id: &str) -> Option<NodeKey> {
    for (key, node) in &state.arena.nodes {
        if node.node_type == NodeType::Element && node.get_attr("id") == Some(id) {
            return Some(key);
        }
    }
    None
}

// ─── Constraint Validation ───────────────────────────────────────────────────

/// willValidate — true om elementet är kandidat för constraint validation
pub(super) fn compute_will_validate(state: &BridgeState, key: NodeKey) -> bool {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return false,
    };
    let tag = node.tag.as_deref().unwrap_or("");
    // Bara form-element kan valideras
    if !matches!(tag, "input" | "select" | "textarea" | "button") {
        return false;
    }
    if node.has_attr("disabled") {
        return false;
    }
    if tag == "input" && node.has_attr("readonly") {
        return false;
    }
    if tag == "input" && node.get_attr("type") == Some("hidden") {
        return false;
    }
    if tag == "button" {
        let btn_type = node.get_attr("type").unwrap_or("submit");
        if btn_type != "submit" {
            return false;
        }
    }
    true
}

/// checkValidity — returnerar true om inga constraint violations
pub(super) fn check_validity(state: &BridgeState, key: NodeKey) -> bool {
    if !compute_will_validate(state, key) {
        return true;
    }
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return true,
    };
    let key_bits = super::node_key_to_f64(key) as u64;

    // Custom validity tar företräde
    if let Some(es) = state.element_state.get(&key_bits) {
        if !es.custom_validity.is_empty() {
            return false;
        }
    }

    let value = get_effective_value(state, key);
    let tag = node.tag.as_deref().unwrap_or("");

    // required
    if node.has_attr("required") && value.is_empty() {
        if tag == "select" {
            // select: required means selectedIndex must not be -1 or first option empty
            return false;
        }
        return false;
    }

    if tag == "input" {
        // minLength
        if let Some(ml) = node
            .get_attr("minlength")
            .and_then(|v| v.parse::<usize>().ok())
        {
            if !value.is_empty() && value.len() < ml {
                return false;
            }
        }
        // maxLength
        if let Some(ml) = node
            .get_attr("maxlength")
            .and_then(|v| v.parse::<usize>().ok())
        {
            if value.len() > ml {
                return false;
            }
        }
        // min/max för numeriska typer
        let input_type = node.get_attr("type").unwrap_or("text");
        if matches!(input_type, "number" | "range") {
            if let Ok(num_val) = value.parse::<f64>() {
                if let Some(min) = node.get_attr("min").and_then(|v| v.parse::<f64>().ok()) {
                    if num_val < min {
                        return false;
                    }
                }
                if let Some(max) = node.get_attr("max").and_then(|v| v.parse::<f64>().ok()) {
                    if num_val > max {
                        return false;
                    }
                }
            }
        }
    }

    true
}

/// validationMessage — returnerar aktuellt valideringsmeddelande
pub(super) fn get_validation_message(state: &BridgeState, key: NodeKey) -> String {
    let key_bits = super::node_key_to_f64(key) as u64;
    if let Some(es) = state.element_state.get(&key_bits) {
        if !es.custom_validity.is_empty() {
            return es.custom_validity.clone();
        }
    }
    if !check_validity(state, key) {
        return "Please fill out this field.".to_string();
    }
    String::new()
}

// ─── Select Element ──────────────────────────────────────────────────────────

/// select.type — "select-one" eller "select-multiple"
pub(super) fn compute_select_type(state: &BridgeState, key: NodeKey) -> &'static str {
    state
        .arena
        .nodes
        .get(key)
        .map(|n| {
            if n.has_attr("multiple") {
                "select-multiple"
            } else {
                "select-one"
            }
        })
        .unwrap_or("select-one")
}

/// select.length — antal <option> barn
pub(super) fn compute_select_length(state: &BridgeState, key: NodeKey) -> i32 {
    state
        .arena
        .nodes
        .get(key)
        .map(|n| {
            n.children
                .iter()
                .filter(|&&ck| {
                    state
                        .arena
                        .nodes
                        .get(ck)
                        .and_then(|cn| cn.tag.as_deref())
                        .map(|t| t == "option" || t == "optgroup")
                        .unwrap_or(false)
                })
                .count() as i32
        })
        .unwrap_or(0)
}

// ─── Textarea ────────────────────────────────────────────────────────────────

/// textarea.textLength
pub(super) fn compute_text_length(state: &BridgeState, key: NodeKey) -> i32 {
    get_effective_value(state, key).len() as i32
}

// ─── HTMLAnchorElement URL Decomposition ─────────────────────────────────────

fn get_href(state: &BridgeState, key: NodeKey) -> String {
    state
        .arena
        .nodes
        .get(key)
        .and_then(|n| n.get_attr("href"))
        .unwrap_or("")
        .to_string()
}

/// Parse URL-delar från href. Returnerar (protocol, host, hostname, port, pathname, search, hash)
fn parse_url_parts(href: &str) -> (String, String, String, String, String, String, String) {
    // Enkel URL-parser
    let default = (
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );
    if href.is_empty() {
        return default;
    }

    // protocol
    let (protocol, rest) = if let Some(pos) = href.find("://") {
        (format!("{}:", &href[..pos]), &href[pos + 3..])
    } else if let Some(rest) = href.strip_prefix("//") {
        (String::new(), rest)
    } else {
        return default;
    };

    // host (hostname:port)
    let (host_part, path_part) = if let Some(pos) = rest.find('/') {
        (&rest[..pos], &rest[pos..])
    } else {
        (rest, "")
    };

    // hostname, port
    let (hostname, port) = if let Some(pos) = host_part.rfind(':') {
        (
            host_part[..pos].to_string(),
            host_part[pos + 1..].to_string(),
        )
    } else {
        (host_part.to_string(), String::new())
    };

    let host = host_part.to_string();

    // pathname, search, hash
    let (pathname, rest2) = if let Some(pos) = path_part.find('?') {
        (&path_part[..pos], &path_part[pos..])
    } else if let Some(pos) = path_part.find('#') {
        (&path_part[..pos], &path_part[pos..])
    } else {
        (path_part, "")
    };

    let (search, hash) = if let Some(pos) = rest2.find('#') {
        (rest2[..pos].to_string(), rest2[pos..].to_string())
    } else if rest2.starts_with('?') {
        (rest2.to_string(), String::new())
    } else if rest2.starts_with('#') {
        (String::new(), rest2.to_string())
    } else {
        (String::new(), String::new())
    };

    (
        protocol,
        host,
        hostname,
        port,
        pathname.to_string(),
        search,
        hash,
    )
}

pub(super) fn compute_url_origin(state: &BridgeState, key: NodeKey) -> String {
    let href = get_href(state, key);
    let (protocol, host, ..) = parse_url_parts(&href);
    if protocol.is_empty() {
        return "null".to_string();
    }
    format!("{}//{}", protocol, host)
}

pub(super) fn compute_url_protocol(state: &BridgeState, key: NodeKey) -> String {
    let (protocol, ..) = parse_url_parts(&get_href(state, key));
    protocol
}

pub(super) fn compute_url_hostname(state: &BridgeState, key: NodeKey) -> String {
    let (_, _, hostname, ..) = parse_url_parts(&get_href(state, key));
    hostname
}

pub(super) fn compute_url_host(state: &BridgeState, key: NodeKey) -> String {
    let (_, host, ..) = parse_url_parts(&get_href(state, key));
    host
}

pub(super) fn compute_url_port(state: &BridgeState, key: NodeKey) -> String {
    let (_, _, _, port, ..) = parse_url_parts(&get_href(state, key));
    port
}

pub(super) fn compute_url_pathname(state: &BridgeState, key: NodeKey) -> String {
    let (_, _, _, _, pathname, ..) = parse_url_parts(&get_href(state, key));
    pathname
}

pub(super) fn compute_url_search(state: &BridgeState, key: NodeKey) -> String {
    let (_, _, _, _, _, search, _) = parse_url_parts(&get_href(state, key));
    search
}

pub(super) fn compute_url_hash(state: &BridgeState, key: NodeKey) -> String {
    let (_, _, _, _, _, _, hash) = parse_url_parts(&get_href(state, key));
    hash
}

// ─── Form Element ────────────────────────────────────────────────────────────

/// form.length — antal form controls
pub(super) fn compute_form_length(state: &BridgeState, key: NodeKey) -> i32 {
    let mut count = 0i32;
    count_form_controls(state, key, &mut count);
    count
}

fn count_form_controls(state: &BridgeState, key: NodeKey, count: &mut i32) {
    if let Some(node) = state.arena.nodes.get(key) {
        for &child in &node.children {
            if let Some(cn) = state.arena.nodes.get(child) {
                if matches!(
                    cn.tag.as_deref(),
                    Some("input" | "select" | "textarea" | "button" | "output" | "fieldset")
                ) {
                    *count += 1;
                }
                count_form_controls(state, child, count);
            }
        }
    }
}

// ─── Progress Element ────────────────────────────────────────────────────────

/// progress.position
pub(super) fn compute_progress_position(state: &BridgeState, key: NodeKey) -> f64 {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return -1.0,
    };
    let max = node
        .get_attr("max")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(1.0);
    let value = node.get_attr("value").and_then(|v| v.parse::<f64>().ok());
    match value {
        Some(v) => {
            if max > 0.0 {
                v / max
            } else {
                -1.0
            }
        }
        None => -1.0,
    }
}

// ─── Labels ──────────────────────────────────────────────────────────────────

/// Hitta alla <label> som pekar på detta element (via for=id)
pub(super) fn find_labels(state: &BridgeState, key: NodeKey) -> Vec<NodeKey> {
    let id = match state.arena.nodes.get(key).and_then(|n| n.get_attr("id")) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => return vec![],
    };

    let mut labels = vec![];
    find_labels_recursive(state, state.arena.document, &id, &mut labels);
    labels
}

fn find_labels_recursive(
    state: &BridgeState,
    key: NodeKey,
    target_id: &str,
    results: &mut Vec<NodeKey>,
) {
    if let Some(node) = state.arena.nodes.get(key) {
        if node.tag.as_deref() == Some("label") && node.get_attr("for") == Some(target_id) {
            results.push(key);
        }
        for &child in &node.children {
            find_labels_recursive(state, child, target_id, results);
        }
    }
}

// ─── URL-komplettering ───────────────────────────────────────────────────────

pub(super) fn compute_url_username(state: &BridgeState, key: NodeKey) -> String {
    // URL username/password kräver full URL-parsning — stub för nu
    let _ = (state, key);
    String::new()
}

pub(super) fn compute_url_password(state: &BridgeState, key: NodeKey) -> String {
    let _ = (state, key);
    String::new()
}

// ─── HTMLImageElement ─────────────────────────────────────────────────────────

pub(super) fn compute_img_natural_width(_state: &BridgeState, _key: NodeKey) -> i32 {
    0 // Kräver bildladdning — returnerar 0 i headless
}

pub(super) fn compute_img_natural_height(_state: &BridgeState, _key: NodeKey) -> i32 {
    0
}

pub(super) fn compute_img_complete(_state: &BridgeState, _key: NodeKey) -> bool {
    false // Kräver bildladdning
}

pub(super) fn compute_img_current_src(state: &BridgeState, key: NodeKey) -> String {
    state
        .arena
        .nodes
        .get(key)
        .and_then(|n| n.get_attr("src"))
        .unwrap_or("")
        .to_string()
}

// ─── HTMLOptionElement ────────────────────────────────────────────────────────

pub(super) fn compute_option_index(state: &BridgeState, key: NodeKey) -> i32 {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return 0,
    };
    let parent = match node.parent {
        Some(p) => p,
        None => return 0,
    };
    let parent_node = match state.arena.nodes.get(parent) {
        Some(n) => n,
        None => return 0,
    };
    let mut idx = 0i32;
    for &child in &parent_node.children {
        if child == key {
            return idx;
        }
        if state.arena.nodes.get(child).and_then(|n| n.tag.as_deref()) == Some("option") {
            idx += 1;
        }
    }
    0
}

// ─── HTMLTableElement ─────────────────────────────────────────────────────────

pub(super) fn compute_row_index(state: &BridgeState, key: NodeKey) -> i32 {
    // Räkna TR-position i hela tabellen
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return -1,
    };
    // Hitta tabellens rot
    let mut table_key = node.parent;
    while let Some(pk) = table_key {
        if state.arena.nodes.get(pk).and_then(|n| n.tag.as_deref()) == Some("table") {
            break;
        }
        table_key = state.arena.nodes.get(pk).and_then(|n| n.parent);
    }
    let table_key = match table_key {
        Some(k) => k,
        None => return -1,
    };
    let mut idx = 0i32;
    count_rows_before(state, table_key, key, &mut idx, &mut false)
}

fn count_rows_before(
    state: &BridgeState,
    node: NodeKey,
    target: NodeKey,
    idx: &mut i32,
    found: &mut bool,
) -> i32 {
    if *found {
        return *idx;
    }
    if let Some(n) = state.arena.nodes.get(node) {
        for &child in &n.children {
            if child == target {
                *found = true;
                return *idx;
            }
            if state
                .arena
                .nodes
                .get(child)
                .and_then(|cn| cn.tag.as_deref())
                == Some("tr")
            {
                *idx += 1;
            }
            count_rows_before(state, child, target, idx, found);
            if *found {
                return *idx;
            }
        }
    }
    *idx
}

pub(super) fn compute_section_row_index(state: &BridgeState, key: NodeKey) -> i32 {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return -1,
    };
    let parent = match node.parent {
        Some(p) => p,
        None => return -1,
    };
    let parent_node = match state.arena.nodes.get(parent) {
        Some(n) => n,
        None => return -1,
    };
    let mut idx = 0i32;
    for &child in &parent_node.children {
        if child == key {
            return idx;
        }
        if state.arena.nodes.get(child).and_then(|n| n.tag.as_deref()) == Some("tr") {
            idx += 1;
        }
    }
    -1
}

pub(super) fn compute_cell_index(state: &BridgeState, key: NodeKey) -> i32 {
    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return -1,
    };
    let parent = match node.parent {
        Some(p) => p,
        None => return -1,
    };
    let parent_node = match state.arena.nodes.get(parent) {
        Some(n) => n,
        None => return -1,
    };
    let mut idx = 0i32;
    for &child in &parent_node.children {
        if child == key {
            return idx;
        }
        if let Some(tag) = state.arena.nodes.get(child).and_then(|n| n.tag.as_deref()) {
            if tag == "td" || tag == "th" {
                idx += 1;
            }
        }
    }
    -1
}

// ─── Textarea type (alltid "textarea") ───────────────────────────────────────

pub(super) fn compute_textarea_type(_state: &BridgeState, _key: NodeKey) -> &'static str {
    "textarea"
}
