// ─── Constraint Validation API ───────────────────────────────────────────────
//
// Implementerar ValidityState-objektet med 11 boolean-egenskaper per HTML spec.
// Portad från jsdom DefaultConstraintValidation-impl.js + ValidityState-impl.js.
//
// Spec: https://html.spec.whatwg.org/multipage/form-control-infrastructure.html#the-constraint-validation-api
//
// Gäller: HTMLInputElement, HTMLSelectElement, HTMLTextAreaElement, HTMLButtonElement

use crate::arena_dom::{NodeKey, NodeType};

use super::super::computed::get_effective_value;
use super::super::state::BridgeState;

/// ValidityState — de 11 constraint-flaggorna per HTML spec
#[derive(Debug, Clone, Default)]
pub(in crate::dom_bridge) struct ValidityState {
    pub value_missing: bool,
    pub type_mismatch: bool,
    pub pattern_mismatch: bool,
    pub too_long: bool,
    pub too_short: bool,
    pub range_underflow: bool,
    pub range_overflow: bool,
    pub step_mismatch: bool,
    pub bad_input: bool,
    pub custom_error: bool,
    pub valid: bool,
}

/// Beräkna fullständigt ValidityState för ett form control.
/// Tar hänsyn till elementtyp, attribut, och internt state.
pub(in crate::dom_bridge) fn compute_validity(state: &BridgeState, key: NodeKey) -> ValidityState {
    let mut vs = ValidityState {
        valid: true,
        ..Default::default()
    };

    let node = match state.arena.nodes.get(key) {
        Some(n) => n,
        None => return vs,
    };
    let tag = node.tag.as_deref().unwrap_or("");
    let key_bits = super::super::node_key_to_f64(key) as u64;

    // Custom validity (setCustomValidity med icke-tom sträng)
    if let Some(es) = state.element_state.get(&key_bits) {
        if !es.custom_validity.is_empty() {
            vs.custom_error = true;
            vs.valid = false;
        }
    }

    let value = get_effective_value(state, key);

    match tag {
        "input" => compute_input_validity(state, key, node, &value, &mut vs),
        "textarea" => compute_textarea_validity(node, &value, &mut vs),
        "select" => compute_select_validity(state, key, node, &mut vs),
        // button: bara customError kan göra den invalid
        _ => {}
    }

    // valid = true om ALLA andra flaggor är false
    if vs.value_missing
        || vs.type_mismatch
        || vs.pattern_mismatch
        || vs.too_long
        || vs.too_short
        || vs.range_underflow
        || vs.range_overflow
        || vs.step_mismatch
        || vs.bad_input
        || vs.custom_error
    {
        vs.valid = false;
    }

    vs
}

fn compute_input_validity(
    state: &BridgeState,
    key: NodeKey,
    node: &crate::arena_dom::DomNode,
    value: &str,
    vs: &mut ValidityState,
) {
    let input_type = node.get_attr("type").unwrap_or("text");

    // ─── valueMissing ────────────────────────────────────────────────
    if node.has_attr("required") {
        match input_type {
            "checkbox" => {
                let key_bits = super::super::node_key_to_f64(key) as u64;
                let checked = state
                    .element_state
                    .get(&key_bits)
                    .and_then(|es| es.checked)
                    .unwrap_or_else(|| node.has_attr("checked"));
                if !checked {
                    vs.value_missing = true;
                }
            }
            "radio" => {
                // Radio: required + ingen i gruppen checked → valueMissing
                let name = node.get_attr("name").unwrap_or("");
                if !is_radio_group_checked(state, key, name) {
                    vs.value_missing = true;
                }
            }
            "file" => {
                // file: required + inget filnamn → valueMissing
                if value.is_empty() {
                    vs.value_missing = true;
                }
            }
            _ => {
                if value.is_empty() {
                    vs.value_missing = true;
                }
            }
        }
    }

    // ─── typeMismatch ────────────────────────────────────────────────
    if !value.is_empty() {
        match input_type {
            "email" => {
                if node.has_attr("multiple") {
                    // multiple emails: komma-separerade
                    for part in value.split(',') {
                        if !is_valid_email(part.trim()) {
                            vs.type_mismatch = true;
                            break;
                        }
                    }
                } else if !is_valid_email(value) {
                    vs.type_mismatch = true;
                }
            }
            "url" => {
                if !is_valid_url(value) {
                    vs.type_mismatch = true;
                }
            }
            _ => {}
        }
    }

    // ─── patternMismatch ─────────────────────────────────────────────
    if !value.is_empty() {
        if let Some(pattern) = node.get_attr("pattern") {
            // Per spec: pattern matchas mot hela värdet (^pattern$)
            if !matches_pattern(value, pattern) {
                vs.pattern_mismatch = true;
            }
        }
    }

    // ─── tooLong / tooShort ──────────────────────────────────────────
    // Per spec: tooLong/tooShort räknas i UTF-16 code units
    let char_count = value.encode_utf16().count();
    if let Some(maxlen) = node
        .get_attr("maxlength")
        .and_then(|v| v.parse::<usize>().ok())
    {
        // tooLong: bara om dirty flag satt (user-redigerat)
        let key_bits = super::super::node_key_to_f64(key) as u64;
        let dirty = state
            .element_state
            .get(&key_bits)
            .map(|es| es.value_dirty)
            .unwrap_or(false);
        if dirty && char_count > maxlen {
            vs.too_long = true;
        }
    }
    if let Some(minlen) = node
        .get_attr("minlength")
        .and_then(|v| v.parse::<usize>().ok())
    {
        if !value.is_empty() && char_count < minlen {
            vs.too_short = true;
        }
    }

    // ─── rangeUnderflow / rangeOverflow / stepMismatch ───────────────
    if matches!(
        input_type,
        "number" | "range" | "date" | "time" | "datetime-local" | "month" | "week"
    ) {
        let num_val = parse_input_as_number(input_type, value);
        if !num_val.is_nan() {
            let min_val = node
                .get_attr("min")
                .map(|v| parse_input_as_number(input_type, v))
                .filter(|n| !n.is_nan());
            let max_val = node
                .get_attr("max")
                .map(|v| parse_input_as_number(input_type, v))
                .filter(|n| !n.is_nan());
            if let Some(min) = min_val {
                if num_val < min {
                    vs.range_underflow = true;
                }
            }
            if let Some(max) = max_val {
                if num_val > max {
                    vs.range_overflow = true;
                }
            }
            // stepMismatch
            if let Some(step_str) = node.get_attr("step") {
                if step_str != "any" {
                    if let Ok(step) = step_str.parse::<f64>() {
                        if step > 0.0 {
                            let base = min_val.unwrap_or(0.0);
                            let diff = num_val - base;
                            let remainder = diff % step;
                            let epsilon = step * 1e-10;
                            if remainder.abs() > epsilon && (step - remainder.abs()).abs() > epsilon
                            {
                                vs.step_mismatch = true;
                            }
                        }
                    }
                }
            }
        } else if !value.is_empty() && matches!(input_type, "number" | "range") {
            // Bara number/range ger badInput vid ogiltig parsning
            vs.bad_input = true;
        }
    }
}

fn compute_textarea_validity(
    node: &crate::arena_dom::DomNode,
    value: &str,
    vs: &mut ValidityState,
) {
    // valueMissing
    if node.has_attr("required") && value.is_empty() {
        vs.value_missing = true;
    }

    let char_count = value.encode_utf16().count();

    // tooLong
    if let Some(maxlen) = node
        .get_attr("maxlength")
        .and_then(|v| v.parse::<usize>().ok())
    {
        if char_count > maxlen {
            vs.too_long = true;
        }
    }

    // tooShort
    if let Some(minlen) = node
        .get_attr("minlength")
        .and_then(|v| v.parse::<usize>().ok())
    {
        if !value.is_empty() && char_count < minlen {
            vs.too_short = true;
        }
    }
}

fn compute_select_validity(
    state: &BridgeState,
    key: NodeKey,
    node: &crate::arena_dom::DomNode,
    vs: &mut ValidityState,
) {
    // select: required + value tom → valueMissing
    if node.has_attr("required") {
        let value = get_effective_value(state, key);
        if value.is_empty() {
            vs.value_missing = true;
        }
    }
}

// ─── Hjälpfunktioner ─────────────────────────────────────────────────────────

/// Kontrollera om en radio-grupp har minst en checked-knapp
fn is_radio_group_checked(state: &BridgeState, key: NodeKey, name: &str) -> bool {
    // Hitta form owner (eller document root)
    let form_key = super::super::computed::find_form_owner(state, key);
    let search_root = form_key.unwrap_or(state.arena.document);
    search_radio_group(state, search_root, name)
}

fn search_radio_group(state: &BridgeState, node: NodeKey, name: &str) -> bool {
    if let Some(n) = state.arena.nodes.get(node) {
        if n.node_type == NodeType::Element
            && n.tag.as_deref() == Some("input")
            && n.get_attr("type") == Some("radio")
            && n.get_attr("name") == Some(name)
        {
            let key_bits = super::super::node_key_to_f64(node) as u64;
            let checked = state
                .element_state
                .get(&key_bits)
                .and_then(|es| es.checked)
                .unwrap_or_else(|| n.has_attr("checked"));
            if checked {
                return true;
            }
        }
        for &child in &n.children {
            if search_radio_group(state, child, name) {
                return true;
            }
        }
    }
    false
}

/// Konvertera input value till nummer baserat på typ (för range/step-validering)
fn parse_input_as_number(input_type: &str, value: &str) -> f64 {
    if value.is_empty() {
        return f64::NAN;
    }
    match input_type {
        "number" | "range" => value.parse::<f64>().unwrap_or(f64::NAN),
        "date" => {
            // YYYY-MM-DD → ms since epoch
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() == 3 {
                let y = parts[0].parse::<i64>().unwrap_or(0);
                let m = parts[1].parse::<u32>().unwrap_or(0);
                let d = parts[2].parse::<u32>().unwrap_or(0);
                if y == 0 || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
                    return f64::NAN;
                }
                days_from_civil(y, m, d) as f64 * 86_400_000.0
            } else {
                f64::NAN
            }
        }
        "time" => {
            let parts: Vec<&str> = value.split(':').collect();
            if parts.len() >= 2 {
                let h = parts[0].parse::<f64>().unwrap_or(-1.0);
                let m = parts[1].parse::<f64>().unwrap_or(-1.0);
                if !(0.0..=23.0).contains(&h) || !(0.0..=59.0).contains(&m) {
                    return f64::NAN;
                }
                let s = if parts.len() > 2 {
                    parts[2].parse::<f64>().unwrap_or(0.0)
                } else {
                    0.0
                };
                h * 3_600_000.0 + m * 60_000.0 + s * 1000.0
            } else {
                f64::NAN
            }
        }
        "datetime-local" => {
            let parts: Vec<&str> = value.splitn(2, 'T').collect();
            if parts.len() == 2 {
                let date_ms = parse_input_as_number("date", parts[0]);
                let time_ms = parse_input_as_number("time", parts[1]);
                if date_ms.is_nan() || time_ms.is_nan() {
                    f64::NAN
                } else {
                    date_ms + time_ms
                }
            } else {
                f64::NAN
            }
        }
        "month" => {
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() == 2 {
                let y = parts[0].parse::<f64>().unwrap_or(f64::NAN);
                let m = parts[1].parse::<f64>().unwrap_or(f64::NAN);
                if y.is_nan() || m.is_nan() {
                    return f64::NAN;
                }
                (y - 1970.0) * 12.0 + (m - 1.0)
            } else {
                f64::NAN
            }
        }
        "week" => {
            if value.len() >= 8 && value.contains("-W") {
                let parts: Vec<&str> = value.split("-W").collect();
                if parts.len() == 2 {
                    let y = parts[0].parse::<i64>().unwrap_or(0);
                    let w = parts[1].parse::<i64>().unwrap_or(0);
                    if y == 0 || !(1..=53).contains(&w) {
                        return f64::NAN;
                    }
                    let jan4 = days_from_civil(y, 1, 4);
                    let dow_jan4 = (jan4 + 4) % 7;
                    let week1_monday = jan4 - dow_jan4;
                    let target_day = week1_monday + (w - 1) * 7;
                    target_day as f64 * 86_400_000.0
                } else {
                    f64::NAN
                }
            } else {
                f64::NAN
            }
        }
        _ => f64::NAN,
    }
}

/// Dagar sedan epoch — Howard Hinnants algoritm
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

/// Enkel email-validering per HTML spec
fn is_valid_email(email: &str) -> bool {
    // Spec: localpart@domain, båda icke-tomma
    let parts: Vec<&str> = email.splitn(2, '@').collect();
    if parts.len() != 2 {
        return false;
    }
    let local = parts[0];
    let domain = parts[1];
    !local.is_empty() && !domain.is_empty() && domain.contains('.')
}

/// Enkel URL-validering
fn is_valid_url(url: &str) -> bool {
    // Per spec: giltig URL med scheme
    url.contains("://") || url.starts_with("//")
}

/// Matcha pattern mot värde (per spec: ^(?:pattern)$)
/// Utan regex-crate stöds: literal match, .* (any), enkla character classes.
/// För komplexa mönster returnerar None (okänt) — behandlas som "no mismatch".
fn matches_pattern(value: &str, pattern: &str) -> bool {
    // Exakt literal match
    if value == pattern {
        return true;
    }

    // Enkel .* (matchar allt)
    if pattern == ".*" || pattern == ".+" && !value.is_empty() {
        return true;
    }

    // Pattern "." matchar exakt ett tecken
    if pattern == "." {
        return value.chars().count() == 1;
    }

    // Enkel alternativ: "a|b|c"
    if !pattern.contains('[') && !pattern.contains('(') && pattern.contains('|') {
        return pattern.split('|').any(|alt| value == alt.trim());
    }

    // Enkla vanliga mönster: [a-zA-Z]+ etc.
    // Implementera grundläggande karaktärsklasser
    if let Some(result) = try_simple_pattern_match(value, pattern) {
        return result;
    }

    // Okänt mönster — konservativt: anta att det matchar
    // (undviker falska patternMismatch-rapporter)
    true
}

/// Försök matcha enkla mönster utan full regex.
/// Returnerar Some(true/false) om mönstret förstås, None annars.
fn try_simple_pattern_match(value: &str, pattern: &str) -> Option<bool> {
    // [a-z]+ / [a-zA-Z0-9]+ etc.
    let trimmed = pattern.trim();

    // Enkel character class + quantifier: [chars]+ eller [chars]*
    if trimmed.starts_with('[') {
        if let Some(bracket_end) = trimmed.find(']') {
            let class_spec = &trimmed[1..bracket_end];
            let after = &trimmed[bracket_end + 1..];
            let (min_count, max_count) = match after {
                "+" => (1usize, usize::MAX),
                "*" => (0, usize::MAX),
                "?" => (0, 1),
                "" => (1, 1),
                _ => return None, // Komplex quantifier
            };

            let char_matches = |ch: char| -> bool { char_in_class(ch, class_spec) };

            let matching: usize = value.chars().filter(|&c| char_matches(c)).count();
            let total: usize = value.chars().count();

            // Alla tecken måste matcha klassen, och count i rätt range
            if total == matching && total >= min_count && total <= max_count {
                return Some(true);
            }
            return Some(false);
        }
    }

    None
}

/// Kolla om ett tecken matchar en character class spec (t.ex. "a-zA-Z0-9_")
fn char_in_class(ch: char, spec: &str) -> bool {
    let mut chars = spec.chars().peekable();
    let mut negated = false;
    if chars.peek() == Some(&'^') {
        negated = true;
        chars.next();
    }
    let mut matched = false;
    while let Some(c) = chars.next() {
        if chars.peek() == Some(&'-') {
            chars.next(); // konsumera '-'
            if let Some(end) = chars.next() {
                if ch >= c && ch <= end {
                    matched = true;
                }
                continue;
            }
        }
        if ch == c {
            matched = true;
        }
    }
    if negated {
        !matched
    } else {
        matched
    }
}
