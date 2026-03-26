// ─── CSS Selector Matching ───────────────────────────────────────────────────
//
// Utbruten från dom_bridge/mod.rs för bättre modularitet.

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};

/// Kontrollera om en nod matchar en CSS-selektor
///
/// Stöder: *, #id, .class, tag, tag.class, [attr], [attr="val"],
/// [attr^="val"], [attr$="val"], [attr*="val"], [attr~="val"], [attr|="val"],
/// Hitta matchande ) med hänsyn till nestade parenteser
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// :first-child, :last-child, :nth-child(An+B), :only-child,
/// :first-of-type, :last-of-type, :nth-of-type(An+B),
/// :root, :empty, :not(sel), :checked, :disabled, :enabled, :focus,
/// kombinatorer: (mellanslag), >, +, ~. Komma-separerade selektorer.
pub(super) fn matches_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }

    // Komma-separerade selektorer — matcha om någon matchar
    if find_unescaped_delimiter(selector, &[',']) < selector.len() {
        return selector
            .split(',')
            .any(|s| matches_single_selector(arena, key, s.trim()));
    }

    // Descendant/child/sibling-kombinator — kolla bara oescaped combinators
    {
        let has_combinator =
            find_unescaped_delimiter(selector, &[' ', '>', '+', '~']) < selector.len();
        if has_combinator {
            // Dubbelkolla: hitta den faktiska split-punkten
            let split = find_unescaped_delimiter(selector, &[' ', '>', '+', '~']);
            // Om split == selector.len() → ingen combinator
            if split < selector.len() {
                return matches_combinator_selector(arena, key, selector);
            }
        }
    }

    matches_single_selector(arena, key, selector)
}

/// Attribut-matchningsoperator
#[derive(Debug, Clone, Copy, PartialEq)]
enum AttrOp {
    /// [attr="val"] — exakt matchning
    Exact,
    /// [attr^="val"] — börjar med
    StartsWith,
    /// [attr$="val"] — slutar med
    EndsWith,
    /// [attr*="val"] — innehåller
    Contains,
    /// [attr~="val"] — ordmatchning (mellanslag-separerat)
    WordMatch,
    /// [attr|="val"] — bindestreck-prefix (val eller val-*)
    HyphenPrefix,
    /// [attr] — bara existens, inget värde
    Exists,
}

/// Matcha en enkel selektor (utan kombinatorer)
/// Hitta nästa oescaped delimiter i CSS-selektor.
/// Hoppar över escaped tecken (\X, \XXXXXX).
fn find_unescaped_delimiter(s: &str, delimiters: &[char]) -> usize {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 1; // hoppa över backslash
                    // Hoppa över hex-sekvens (1-6 hex + optional whitespace)
            if i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                let mut hex_count = 0;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() && hex_count < 6 {
                    i += 1;
                    hex_count += 1;
                }
                // Optional trailing whitespace
                if i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0C') {
                    i += 1;
                }
            } else if i < bytes.len() {
                i += 1; // hoppa över escaped tecken
            }
            continue;
        }
        // Kolla om det är en delimiter (men bara om vi är på en char boundary)
        if s.is_char_boundary(i) {
            let ch = s[i..].chars().next().unwrap();
            if delimiters.contains(&ch) {
                return i;
            }
            i += ch.len_utf8();
        } else {
            i += 1;
        }
    }
    s.len()
}

/// CSS escape-unescape per CSS syntax spec.
/// Hanterar: \XX (hex 1-6 siffror + optional space), \c (escaped tecken)
fn css_unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        // Backslash — kolla nästa tecken
        match chars.peek() {
            None => {
                // Backslash i slutet — ignorera per spec
            }
            Some(&next) if next.is_ascii_hexdigit() => {
                // Hex escape: 1-6 hex siffror
                let mut hex = String::with_capacity(6);
                for _ in 0..6 {
                    if let Some(&h) = chars.peek() {
                        if h.is_ascii_hexdigit() {
                            hex.push(h);
                            chars.next();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                // Optional trailing whitespace (konsumeras)
                if let Some(&ws) = chars.peek() {
                    if ws == ' ' || ws == '\t' || ws == '\n' || ws == '\r' || ws == '\x0C' {
                        chars.next();
                    }
                }
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if cp == 0 || (0xD800..=0xDFFF).contains(&cp) || cp > 0x10FFFF {
                        result.push('\u{FFFD}');
                    } else if let Some(ch) = char::from_u32(cp) {
                        result.push(ch);
                    } else {
                        result.push('\u{FFFD}');
                    }
                }
            }
            Some(_) => {
                // Escaped tecken — ta bokstavligt
                result.push(chars.next().unwrap());
            }
        }
    }
    result
}

fn matches_single_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    let node = match arena.nodes.get(key) {
        Some(n) if n.node_type == NodeType::Element => n,
        _ => return false,
    };

    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }

    // Universell selektor
    if selector == "*" {
        return true;
    }

    // Rena pseudo-selektorer utan tagg
    if selector == ":first-child" {
        return is_first_child(arena, key);
    }
    if selector == ":last-child" {
        return is_last_child(arena, key);
    }
    if selector == ":root" {
        return is_root_element(arena, key);
    }
    if selector == ":empty" {
        return is_empty_element(arena, key);
    }
    if selector == ":only-child" {
        return element_index_among_siblings(arena, key)
            .map(|(_, total)| total == 1)
            .unwrap_or(false);
    }
    if selector == ":first-of-type" {
        return type_index_among_siblings(arena, key)
            .map(|(pos, _)| pos == 1)
            .unwrap_or(false);
    }
    if selector == ":last-of-type" {
        return type_index_among_siblings(arena, key)
            .map(|(pos, total)| pos == total)
            .unwrap_or(false);
    }
    if selector == ":checked" {
        return node.get_attr("checked").is_some() || node.get_attr("selected").is_some();
    }
    if selector == ":disabled" {
        return node.get_attr("disabled").is_some();
    }
    if selector == ":enabled" {
        let is_form_el = matches!(
            node.tag.as_deref(),
            Some("input" | "select" | "textarea" | "button")
        );
        return is_form_el && node.get_attr("disabled").is_none();
    }
    if selector == ":focus" {
        // Utan tillgång till BridgeState kollar vi data-focused-attribut
        return node.get_attr("data-focused").is_some();
    }

    // Parsea selektor-delar: tag, #id, .class, [attr], [attr="val"], :pseudo
    let mut remaining = selector;
    let mut required_tag: Option<&str> = None;
    let mut required_id: Option<&str> = None;
    let mut required_classes: Vec<&str> = Vec::new();
    let mut required_attrs: Vec<(String, Option<String>, AttrOp)> = Vec::new();
    let mut require_first_child = false;
    let mut require_last_child = false;
    let mut require_root = false;
    let mut require_empty = false;
    let mut require_only_child = false;
    let mut require_first_of_type = false;
    let mut require_last_of_type = false;
    let mut require_checked = false;
    let mut require_disabled = false;
    let mut require_enabled = false;
    let mut require_focus = false;
    let mut nth_child_expr: Option<(i32, i32)> = None;
    let mut nth_of_type_expr: Option<(i32, i32)> = None;
    let mut nth_last_child_expr: Option<(i32, i32)> = None;
    let mut nth_last_of_type_expr: Option<(i32, i32)> = None;
    let mut require_is: Option<String> = None;
    let mut require_where: Option<String> = None;
    let mut require_has: Option<String> = None;
    let mut require_heading = false;
    let mut require_heading_levels: Option<Vec<u32>> = None;
    let mut require_lang: Option<String> = None;
    let mut require_dir: Option<String> = None;
    let mut require_placeholder_shown = false;
    let mut require_any_link = false;
    let mut not_selectors: Vec<String> = Vec::new();
    let mut is_universal = false;

    // Universell selektor med pseudo
    if remaining.starts_with('*') {
        is_universal = true;
        remaining = &remaining[1..];
    } else if remaining.starts_with(|c: char| c.is_ascii_alphabetic()) {
        // Extrahera tagg (om den börjar med bokstav)
        let end = remaining
            .find(|c: char| ['#', '.', '[', ':'].contains(&c))
            .unwrap_or(remaining.len());
        required_tag = Some(&remaining[..end]);
        remaining = &remaining[end..];
    }

    // Parsea resterande delar
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix('#') {
            let end = find_unescaped_delimiter(rest, &['.', '[', ':']);
            required_id = Some(&rest[..end]);
            remaining = &rest[end..];
        } else if let Some(rest) = remaining.strip_prefix('.') {
            let end = find_unescaped_delimiter(rest, &['#', '.', '[', ':']);
            required_classes.push(&rest[..end]);
            remaining = &rest[end..];
        } else if let Some(rest) = remaining.strip_prefix('[') {
            let bracket_end = match rest.find(']') {
                Some(e) => e,
                None => break,
            };
            let attr_spec = &rest[..bracket_end];
            if let Some(eq_pos) = attr_spec.find('=') {
                let before_eq = &attr_spec[..eq_pos];
                let attr_val = attr_spec[eq_pos + 1..].trim_matches('"').trim_matches('\'');

                if let Some(attr_name) = before_eq.strip_suffix('^') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::StartsWith,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('$') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::EndsWith,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('*') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::Contains,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('~') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::WordMatch,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('|') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::HyphenPrefix,
                    ));
                } else {
                    required_attrs.push((
                        before_eq.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::Exact,
                    ));
                }
            } else {
                required_attrs.push((attr_spec.to_string(), None, AttrOp::Exists));
            }
            remaining = &rest[bracket_end + 1..];
        } else if let Some(rest) = remaining.strip_prefix(":not(") {
            // Hitta matchande avslutande parentes
            if let Some(end) = rest.find(')') {
                let inner = &rest[..end];
                not_selectors.push(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":nth-of-type(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_of_type_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":nth-child(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_child_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":nth-last-child(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_last_child_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":nth-last-of-type(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_last_of_type_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":is(") {
            if let Some(end) = find_matching_paren(rest) {
                let inner = &rest[..end];
                require_is = Some(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":where(") {
            if let Some(end) = find_matching_paren(rest) {
                let inner = &rest[..end];
                require_where = Some(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":has(") {
            if let Some(end) = find_matching_paren(rest) {
                let inner = &rest[..end];
                require_has = Some(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":dir(") {
            if let Some(end) = rest.find(')') {
                let dir_val = rest[..end].trim();
                let node_dir = node
                    .get_attr("dir")
                    .map(|d| d.to_ascii_lowercase())
                    .unwrap_or_else(|| "ltr".to_string());
                if node_dir != dir_val {
                    return false;
                }
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":first-child") {
            require_first_child = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":last-child") {
            require_last_child = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":first-of-type") {
            require_first_of_type = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":last-of-type") {
            require_last_of_type = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":only-child") {
            require_only_child = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":root") {
            require_root = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":empty") {
            require_empty = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":checked") {
            require_checked = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":disabled") {
            require_disabled = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":enabled") {
            require_enabled = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":focus") {
            require_focus = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":heading(") {
            // :heading(n, m, ...) — matchar h<n>, h<m>, etc.
            if let Some(end) = rest.find(')') {
                let args = &rest[..end];
                require_heading_levels = Some(
                    args.split(',')
                        .filter_map(|s| s.trim().parse::<u32>().ok())
                        .collect(),
                );
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if remaining.starts_with(":heading") {
            // :heading (utan parentes) — matchar alla h1-h6
            require_heading = true;
            remaining = &remaining[8..]; // len(":heading") = 8
        } else if let Some(rest) = remaining.strip_prefix(":lang(") {
            // :lang(xx) — matchar element med lang-attribut
            if let Some(end) = find_matching_paren(rest) {
                let lang_arg = rest[..end].trim().trim_matches('"').trim_matches('\'');
                require_lang = Some(lang_arg.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":dir(") {
            // :dir(ltr|rtl)
            if let Some(end) = rest.find(')') {
                let dir_arg = rest[..end].trim();
                require_dir = Some(dir_arg.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if remaining.starts_with(":placeholder-shown") {
            require_placeholder_shown = true;
            remaining = &remaining[18..];
        } else if remaining.starts_with(":any-link") {
            require_any_link = true;
            remaining = &remaining[9..];
        } else if remaining.starts_with(":link") {
            require_any_link = true; // :link ≈ :any-link i vår kontext
            remaining = &remaining[5..];
        } else if remaining.starts_with(":visited") {
            // :visited — aldrig matchad (ingen browsing history)
            return false;
        } else if remaining.starts_with(":hover") || remaining.starts_with(":active") {
            // Dynamiska pseudo-klasser — aldrig matchade i statisk parse
            return false;
        } else if remaining.starts_with(':') {
            // Okänd pseudo-klass
            return false;
        } else {
            break;
        }
    }

    // Verifiera tagg (om inte universell)
    if let Some(tag) = required_tag {
        if node.tag.as_deref() != Some(tag) {
            return false;
        }
    }
    // Universell selektor kräver ingen tagg-matchning (alla element matchar)
    let _ = is_universal;

    if let Some(id) = required_id {
        let unesc = css_unescape(id);
        if node.get_attr("id") != Some(unesc.as_str()) {
            return false;
        }
    }
    for cls in &required_classes {
        let unesc = css_unescape(cls);
        let has = node
            .get_attr("class")
            .map(|c| split_ascii_whitespace(c).any(|x| x == unesc))
            .unwrap_or(false);
        if !has {
            return false;
        }
    }

    // Verifiera attribut med operator
    for (attr, val, op) in &required_attrs {
        match op {
            AttrOp::Exists => {
                if !node.has_attr(attr) {
                    return false;
                }
            }
            AttrOp::Exact => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                if node.get_attr(attr) != Some(expected) {
                    return false;
                }
            }
            AttrOp::StartsWith => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.starts_with(expected) => {}
                    _ => return false,
                }
            }
            AttrOp::EndsWith => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.ends_with(expected) => {}
                    _ => return false,
                }
            }
            AttrOp::Contains => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.contains(expected) => {}
                    _ => return false,
                }
            }
            AttrOp::WordMatch => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.split_whitespace().any(|w| w == expected) => {}
                    _ => return false,
                }
            }
            AttrOp::HyphenPrefix => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual)
                        if actual == expected || actual.starts_with(&format!("{}-", expected)) => {}
                    _ => return false,
                }
            }
        }
    }

    // Pseudo-klass-verifieringar
    if require_first_child && !is_first_child(arena, key) {
        return false;
    }
    if require_last_child && !is_last_child(arena, key) {
        return false;
    }
    if require_root && !is_root_element(arena, key) {
        return false;
    }
    if require_empty && !is_empty_element(arena, key) {
        return false;
    }
    if require_only_child {
        let is_only = element_index_among_siblings(arena, key)
            .map(|(_, total)| total == 1)
            .unwrap_or(false);
        if !is_only {
            return false;
        }
    }
    if require_first_of_type {
        let is_first = type_index_among_siblings(arena, key)
            .map(|(pos, _)| pos == 1)
            .unwrap_or(false);
        if !is_first {
            return false;
        }
    }
    if require_last_of_type {
        let is_last = type_index_among_siblings(arena, key)
            .map(|(pos, total)| pos == total)
            .unwrap_or(false);
        if !is_last {
            return false;
        }
    }
    if require_checked && node.get_attr("checked").is_none() && node.get_attr("selected").is_none()
    {
        return false;
    }
    if require_disabled && node.get_attr("disabled").is_none() {
        return false;
    }
    if require_enabled {
        let is_form_el = matches!(
            node.tag.as_deref(),
            Some("input" | "select" | "textarea" | "button")
        );
        if !is_form_el || node.get_attr("disabled").is_some() {
            return false;
        }
    }
    if require_focus && node.get_attr("data-focused").is_none() {
        return false;
    }
    if let Some((a, b)) = nth_child_expr {
        let matched = element_index_among_siblings(arena, key)
            .map(|(pos, _)| matches_nth(pos, a, b))
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }
    if let Some((a, b)) = nth_of_type_expr {
        let matched = type_index_among_siblings(arena, key)
            .map(|(pos, _)| matches_nth(pos, a, b))
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }

    // :nth-last-child
    if let Some((a, b)) = nth_last_child_expr {
        let matched = element_index_among_siblings(arena, key)
            .map(|(pos, total)| {
                let from_last = total - pos + 1;
                matches_nth(from_last, a, b)
            })
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }
    // :nth-last-of-type
    if let Some((a, b)) = nth_last_of_type_expr {
        let matched = type_index_among_siblings(arena, key)
            .map(|(pos, total)| {
                let from_last = total - pos + 1;
                matches_nth(from_last, a, b)
            })
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }
    // :is() / :where() — matcha mot kommaseparerade inre selektorer
    if let Some(ref inner) = require_is {
        let any_match = inner
            .split(',')
            .any(|s| matches_selector(arena, key, s.trim()));
        if !any_match {
            return false;
        }
    }
    if let Some(ref inner) = require_where {
        let any_match = inner
            .split(',')
            .any(|s| matches_selector(arena, key, s.trim()));
        if !any_match {
            return false;
        }
    }
    // :has() — matcha om elementet har efterkommande som matchar
    if let Some(ref inner) = require_has {
        let has_match = inner.split(',').any(|sel| {
            let sel = sel.trim();
            // Sök bland alla efterkommande
            fn check_descendants(arena: &ArenaDom, parent: NodeKey, sel: &str) -> bool {
                if let Some(node) = arena.nodes.get(parent) {
                    for &child in &node.children {
                        if matches_selector(arena, child, sel) {
                            return true;
                        }
                        if check_descendants(arena, child, sel) {
                            return true;
                        }
                    }
                }
                false
            }
            check_descendants(arena, key, sel)
        });
        if !has_match {
            return false;
        }
    }
    // :heading / :heading(n) — matchar h1-h6
    if require_heading {
        let tag = node.tag.as_deref().unwrap_or("");
        let is_heading = matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
        if !is_heading {
            return false;
        }
    }
    if let Some(ref levels) = require_heading_levels {
        let tag = node.tag.as_deref().unwrap_or("");
        let heading_level: u32 = match tag {
            "h1" => 1,
            "h2" => 2,
            "h3" => 3,
            "h4" => 4,
            "h5" => 5,
            "h6" => 6,
            _ => 0,
        };
        if heading_level == 0 || !levels.contains(&heading_level) {
            return false;
        }
    }
    // :lang(xx) — matchar element eller ancestors med lang-attribut
    if let Some(ref lang) = require_lang {
        let lang_lower = lang.to_lowercase();
        let mut found = false;
        let mut current = Some(key);
        while let Some(k) = current {
            if let Some(n) = arena.nodes.get(k) {
                if let Some(node_lang) = n.get_attr("lang").or_else(|| n.get_attr("xml:lang")) {
                    let node_lang_lower = node_lang.to_lowercase();
                    // :lang(en) matchar "en", "en-US", "en-GB" etc.
                    if node_lang_lower == lang_lower
                        || node_lang_lower.starts_with(&format!("{}-", lang_lower))
                    {
                        found = true;
                    }
                    break; // Närmaste lang-attribut bestämmer
                }
                current = n.parent;
            } else {
                break;
            }
        }
        if !found {
            return false;
        }
    }
    // :dir(ltr|rtl)
    if let Some(ref dir) = require_dir {
        let mut found_dir = "ltr".to_string(); // default
        let mut current = Some(key);
        while let Some(k) = current {
            if let Some(n) = arena.nodes.get(k) {
                if let Some(d) = n.get_attr("dir") {
                    found_dir = d.to_lowercase();
                    break;
                }
                current = n.parent;
            } else {
                break;
            }
        }
        if found_dir != dir.to_lowercase() {
            return false;
        }
    }
    // :placeholder-shown
    if require_placeholder_shown {
        let has_placeholder = node.get_attr("placeholder").is_some();
        let is_input =
            node.tag.as_deref() == Some("input") || node.tag.as_deref() == Some("textarea");
        let value_empty = node.get_attr("value").is_none_or(|v| v.is_empty());
        if !(is_input && has_placeholder && value_empty) {
            return false;
        }
    }
    // :any-link / :link
    if require_any_link {
        let is_link = (node.tag.as_deref() == Some("a") || node.tag.as_deref() == Some("area"))
            && node.has_attr("href");
        if !is_link {
            return false;
        }
    }
    // :not()-verifiering — negera matchning mot inre selektor
    for not_sel in &not_selectors {
        if matches_single_selector(arena, key, not_sel) {
            return false;
        }
    }

    true
}

/// Matcha selektor med kombinatorer (>, mellanslag, +, ~)
fn matches_combinator_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    // Splitta vid whitespace och separera kombinatorer
    let parts: Vec<&str> = selector.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    // Sista delen matchar mot noden
    let last = parts[parts.len() - 1];
    if matches!(last, ">" | "+" | "~") {
        return false; // Felaktig selektor — slutar med kombinator
    }
    if !matches_single_selector(arena, key, last) {
        return false;
    }

    if parts.len() == 1 {
        return true;
    }

    // Identifiera kombinator-typ: >, +, ~ eller descendant (mellanslag)
    let combinator = if parts.len() >= 2 {
        match parts[parts.len() - 2] {
            ">" => ">",
            "+" => "+",
            "~" => "~",
            _ => " ", // descendant
        }
    } else {
        " "
    };

    let ancestor_sel = if combinator != " " {
        // Explicit kombinator — skippa kombinatorn
        if parts.len() < 3 {
            return false;
        }
        parts[..parts.len() - 2].join(" ")
    } else {
        parts[..parts.len() - 1].join(" ")
    };

    match combinator {
        ">" => {
            // Direkt förälder måste matcha
            if let Some(parent) = arena.nodes.get(key).and_then(|n| n.parent) {
                matches_selector(arena, parent, &ancestor_sel)
            } else {
                false
            }
        }
        "+" => {
            // Föregående element-syskon måste matcha
            if let Some(prev) = prev_element_sibling(arena, key) {
                matches_selector(arena, prev, &ancestor_sel)
            } else {
                false
            }
        }
        "~" => {
            // Något föregående element-syskon måste matcha
            let prev_siblings = all_prev_element_siblings(arena, key);
            prev_siblings
                .iter()
                .any(|&sib| matches_selector(arena, sib, &ancestor_sel))
        }
        _ => {
            // Descendant — valfri förfader måste matcha
            let mut current = arena.nodes.get(key).and_then(|n| n.parent);
            while let Some(ancestor) = current {
                if matches_selector(arena, ancestor, &ancestor_sel) {
                    return true;
                }
                current = arena.nodes.get(ancestor).and_then(|n| n.parent);
            }
            false
        }
    }
}

/// Kolla om nod är första element-barnet
fn is_first_child(arena: &ArenaDom, key: NodeKey) -> bool {
    let parent = match arena.nodes.get(key).and_then(|n| n.parent) {
        Some(p) => p,
        None => return false,
    };
    // Spec: :first-child gäller bara element med element-parent (inte Document)
    let parent_node = match arena.nodes.get(parent) {
        Some(n)
            if n.node_type == NodeType::Element || n.node_type == NodeType::DocumentFragment =>
        {
            n
        }
        _ => return false,
    };
    parent_node.children.iter().find(|&&c| {
        arena
            .nodes
            .get(c)
            .map(|cn| cn.node_type == NodeType::Element)
            .unwrap_or(false)
    }) == Some(&key)
}

/// Kolla om nod är sista element-barnet
fn is_last_child(arena: &ArenaDom, key: NodeKey) -> bool {
    let parent = match arena.nodes.get(key).and_then(|n| n.parent) {
        Some(p) => p,
        None => return false,
    };
    // Spec: :last-child gäller bara element med element-parent
    let parent_node = match arena.nodes.get(parent) {
        Some(n)
            if n.node_type == NodeType::Element || n.node_type == NodeType::DocumentFragment =>
        {
            n
        }
        _ => return false,
    };
    parent_node.children.iter().rfind(|&&c| {
        arena
            .nodes
            .get(c)
            .map(|cn| cn.node_type == NodeType::Element)
            .unwrap_or(false)
    }) == Some(&key)
}

/// Räkna nodens element-position bland sina syskon (1-indexed)
/// Returnerar (position, totalt_antal_element_syskon)
fn element_index_among_siblings(arena: &ArenaDom, key: NodeKey) -> Option<(usize, usize)> {
    let parent = arena.nodes.get(key)?.parent?;
    let parent_node = arena.nodes.get(parent)?;
    // Spec: child-indexed pseudo-klasser kräver element-parent
    if !matches!(
        parent_node.node_type,
        NodeType::Element | NodeType::DocumentFragment
    ) {
        return None;
    }
    let mut pos = 0usize;
    let mut total = 0usize;
    let mut found = false;
    for &child in &parent_node.children {
        let is_element = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element)
            .unwrap_or(false);
        if is_element {
            total += 1;
            if child == key {
                pos = total;
                found = true;
            }
        }
    }
    if found {
        Some((pos, total))
    } else {
        None
    }
}

/// Räkna nodens position bland syskon av samma tagg-typ (1-indexed)
/// Returnerar (position, totalt_antal_av_samma_typ)
fn type_index_among_siblings(arena: &ArenaDom, key: NodeKey) -> Option<(usize, usize)> {
    let node = arena.nodes.get(key)?;
    let my_tag = node.tag.as_deref()?;
    let parent = node.parent?;
    let parent_node = arena.nodes.get(parent)?;
    // Spec: :*-of-type kräver element-parent
    if !matches!(
        parent_node.node_type,
        NodeType::Element | NodeType::DocumentFragment
    ) {
        return None;
    }
    let mut pos = 0usize;
    let mut total = 0usize;
    let mut found = false;
    for &child in &parent_node.children {
        let matches = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element && n.tag.as_deref() == Some(my_tag))
            .unwrap_or(false);
        if matches {
            total += 1;
            if child == key {
                pos = total;
                found = true;
            }
        }
    }
    if found {
        Some((pos, total))
    } else {
        None
    }
}

/// Parsa An+B-uttryck för :nth-child/:nth-of-type
fn parse_nth_expression(expr: &str) -> (i32, i32) {
    let expr = expr.trim();
    match expr {
        "odd" => (2, 1),
        "even" => (2, 0),
        s if s.contains('n') => {
            // Hantera varianter: "n", "2n", "-n", "2n+1", "2n-3", "-2n+1"
            let s = s.replace(' ', "");
            let n_pos = match s.find('n') {
                Some(p) => p,
                None => return (0, 0),
            };
            let a_part = &s[..n_pos];
            let a: i32 = match a_part {
                "" | "+" => 1,
                "-" => -1,
                other => other.parse().unwrap_or(0),
            };
            let after = &s[n_pos + 1..];
            let b: i32 = if after.is_empty() {
                0
            } else {
                after.parse().unwrap_or(0)
            };
            (a, b)
        }
        s => (0, s.parse().unwrap_or(0)),
    }
}

/// Kolla om position matchar An+B-uttryck
fn matches_nth(pos: usize, a: i32, b: i32) -> bool {
    let pos = pos as i32;
    if a == 0 {
        return pos == b;
    }
    let diff = pos - b;
    diff % a == 0 && diff / a >= 0
}

/// Hämta föregående element-syskon
fn prev_element_sibling(arena: &ArenaDom, key: NodeKey) -> Option<NodeKey> {
    let parent = arena.nodes.get(key)?.parent?;
    let parent_node = arena.nodes.get(parent)?;
    let mut prev: Option<NodeKey> = None;
    for &child in &parent_node.children {
        if child == key {
            return prev;
        }
        let is_element = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element)
            .unwrap_or(false);
        if is_element {
            prev = Some(child);
        }
    }
    None
}

/// Hämta alla föregående element-syskon
fn all_prev_element_siblings(arena: &ArenaDom, key: NodeKey) -> Vec<NodeKey> {
    let parent = match arena.nodes.get(key).and_then(|n| n.parent) {
        Some(p) => p,
        None => return vec![],
    };
    let parent_node = match arena.nodes.get(parent) {
        Some(n) => n,
        None => return vec![],
    };
    let mut result = vec![];
    for &child in &parent_node.children {
        if child == key {
            break;
        }
        let is_element = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element)
            .unwrap_or(false);
        if is_element {
            result.push(child);
        }
    }
    result
}

/// Kolla om nod är rotelementet (html)
fn is_root_element(arena: &ArenaDom, key: NodeKey) -> bool {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return false,
    };
    // Roten har document som förälder
    match node.parent {
        Some(p) => arena
            .nodes
            .get(p)
            .map(|pn| pn.node_type == NodeType::Document)
            .unwrap_or(false),
        None => false,
    }
}

/// Kolla om nod saknar barn-element och text
fn is_empty_element(arena: &ArenaDom, key: NodeKey) -> bool {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return false,
    };
    node.children.iter().all(|&c| {
        arena
            .nodes
            .get(c)
            .map(|cn| {
                // :empty = inga element- eller text-barn (kommentarer ok)
                cn.node_type != NodeType::Element && cn.node_type != NodeType::Text
            })
            .unwrap_or(true)
    })
}

/// querySelector — hittar första matchande nod med full CSS-selektor
pub(super) fn query_select_one(arena: &ArenaDom, selector: &str) -> Option<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }
    find_first_matching(arena, arena.document, selector)
}

/// Rekursiv sökning efter första matchande nod
pub(super) fn find_first_matching(
    arena: &ArenaDom,
    key: NodeKey,
    selector: &str,
) -> Option<NodeKey> {
    let node = arena.nodes.get(key)?;
    if node.node_type == NodeType::Element && matches_selector(arena, key, selector) {
        return Some(key);
    }
    for &child in &node.children {
        if let Some(found) = find_first_matching(arena, child, selector) {
            return Some(found);
        }
    }
    None
}

/// querySelectorAll — returnerar alla matchande noder med full CSS-selektor
pub(super) fn query_select_all(arena: &ArenaDom, selector: &str) -> Vec<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return vec![];
    }
    let mut results = vec![];
    find_all_matching(arena, arena.document, selector, &mut results);
    results
}

/// Rekursiv sökning efter alla matchande noder
pub(super) fn find_all_matching(
    arena: &ArenaDom,
    key: NodeKey,
    selector: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element && matches_selector(arena, key, selector) {
        results.push(key);
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_matching(arena, child, selector, results);
    }
}

/// Samla alla element med given klass
/// Splitta sträng på ASCII whitespace per HTML-spec (space, tab, LF, FF, CR).
/// Unicode-whitespace som \u{00A0} (NBSP) är INTE separatorer — de är giltiga class-tecken.
fn split_ascii_whitespace(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t', '\n', '\x0C', '\r'])
        .filter(|s| !s.is_empty())
}

pub(super) fn find_all_by_class(
    arena: &ArenaDom,
    key: NodeKey,
    class: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element {
        if let Some(attr_classes) = node.get_attr("class") {
            // getElementsByClassName("a b") matchar element med BÅDA "a" och "b"
            let search_tokens: Vec<&str> = split_ascii_whitespace(class).collect();
            if !search_tokens.is_empty() {
                let elem_tokens: Vec<&str> = split_ascii_whitespace(attr_classes).collect();
                if search_tokens.iter().all(|t| elem_tokens.contains(t)) {
                    results.push(key);
                }
            }
        }
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_class(arena, child, class, results);
    }
}

/// Samla alla element med given tagg
pub(super) fn find_all_by_tag(
    arena: &ArenaDom,
    key: NodeKey,
    tag: &str,
    results: &mut Vec<NodeKey>,
) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element
        && (tag == "*"
            || node
                .tag
                .as_deref()
                .is_some_and(|t| t.eq_ignore_ascii_case(tag)))
    {
        results.push(key);
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_tag(arena, child, tag, results);
    }
}
