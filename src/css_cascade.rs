/// CSS Cascade Engine — Fas 19
///
/// Parses `<style>` tags and inline styles from ArenaDom, resolves
/// specificity-ordered cascade, and computes styles per node with
/// CSS inheritance support.
///
/// Används av `dom_bridge.rs` för `window.getComputedStyle()`.
use std::collections::HashMap;

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};

/// A parsed CSS rule with selector, specificity, and declarations
#[derive(Debug, Clone)]
struct CascadeRule {
    selector_text: String,
    specificity: (u32, u32, u32),
    properties: Vec<(String, String)>,
    source_order: usize,
}

/// Computed style properties for a single DOM node
#[derive(Debug, Clone, Default)]
pub struct ComputedStyle {
    pub properties: HashMap<String, String>,
}

/// CSS cascade context — holds all parsed rules and a computed style cache
pub struct CssContext {
    rules: Vec<CascadeRule>,
    cache: HashMap<u64, ComputedStyle>,
}

// Ärvda CSS-egenskaper (propagerar från förälder till barn)
const INHERITED_PROPERTIES: &[&str] = &[
    "color",
    "font-family",
    "font-size",
    "font-weight",
    "font-style",
    "line-height",
    "text-align",
    "text-decoration",
    "text-transform",
    "visibility",
    "cursor",
    "letter-spacing",
    "word-spacing",
    "white-space",
    "direction",
    "list-style-type",
    "quotes",
];

impl CssContext {
    /// Build a CssContext by extracting and parsing all `<style>` tags from the DOM.
    pub fn from_arena(arena: &ArenaDom) -> Self {
        let mut rules = Vec::new();
        let mut source_order = 0usize;

        // Traversera hela DOM:en och extrahera <style>-taggar
        Self::collect_style_tags(arena, arena.document, &mut rules, &mut source_order);

        CssContext {
            rules,
            cache: HashMap::new(),
        }
    }

    /// Compute the resolved style for a given node, using cascade + inheritance.
    pub fn get_computed_style(&mut self, key: NodeKey, arena: &ArenaDom) -> ComputedStyle {
        use slotmap::Key;
        let cache_key = key.data().as_ffi();
        if let Some(cached) = self.cache.get(&cache_key) {
            return cached.clone();
        }

        // Steg 1: Börja med tag-defaults
        let tag = arena.tag_name(key).unwrap_or("div");
        let mut computed = get_tag_defaults(tag);

        // Spåra vilka properties som explicit sattes (av CSS-regler eller inline)
        let mut explicit: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Steg 2: Applicera alla matchande CSS-regler (sorterade: specificitet → source_order)
        let mut matching: Vec<&CascadeRule> = self
            .rules
            .iter()
            .filter(|rule| self.matches_node(&rule.selector_text, key, arena))
            .collect();

        // Sortera: lägst specificitet först (senare overridar)
        matching.sort_by(|a, b| {
            a.specificity
                .cmp(&b.specificity)
                .then(a.source_order.cmp(&b.source_order))
        });

        for rule in &matching {
            for (prop, val) in &rule.properties {
                computed.insert(prop.clone(), val.clone());
                explicit.insert(prop.clone());
            }
        }

        // Steg 3: Applicera inline styles (högst specificitet)
        if let Some(node) = arena.nodes.get(key) {
            if let Some(style_attr) = node.get_attr("style") {
                for part in style_attr.split(';') {
                    let part = part.trim();
                    if let Some(colon) = part.find(':') {
                        let prop = part[..colon].trim().to_lowercase();
                        let val = part[colon + 1..].trim().to_string();
                        if !prop.is_empty() {
                            computed.insert(prop.clone(), val);
                            explicit.insert(prop);
                        }
                    }
                }
            }
        }

        // Steg 4: Ärv properties från förälder
        // Ärvda properties overridar tag-defaults om ej explicit satt av CSS/inline
        if let Some(parent_key) = arena.nodes.get(key).and_then(|n| n.parent) {
            let parent_style = self.get_computed_style(parent_key, arena);
            for &prop in INHERITED_PROPERTIES {
                if !explicit.contains(prop) {
                    if let Some(val) = parent_style.properties.get(prop) {
                        computed.insert(prop.to_string(), val.clone());
                    }
                }
            }
        }

        let result = ComputedStyle {
            properties: computed,
        };
        self.cache.insert(cache_key, result.clone());
        result
    }

    /// Apply computed styles as inline `style` attributes on all element nodes.
    ///
    /// Walks the DOM, computes cascade for each element, and merges
    /// the resolved properties into the node's `style` attribute so that
    /// Blitz (which only sees inline/HTML styles) renders with full cascade.
    ///
    /// Returns the number of nodes that got style attributes updated.
    pub fn apply_computed_styles_inline(&mut self, arena: &mut ArenaDom) -> usize {
        // Samla alla element-nycklar först (undvik borrow-konflikt med mutable arena)
        let element_keys: Vec<NodeKey> = arena
            .nodes
            .iter()
            .filter(|(_, n)| n.node_type == NodeType::Element)
            .map(|(k, _)| k)
            .collect();

        let mut updated = 0usize;
        for key in element_keys {
            let style = self.get_computed_style(key, arena);

            // Filtrera bort tomma/default-värden — vi vill bara injecta
            // properties som faktiskt har en effekt
            let style_str: String = style
                .properties
                .iter()
                .filter(|(prop, val)| !val.is_empty() && is_render_relevant(prop))
                .map(|(prop, val)| format!("{prop}: {val}"))
                .collect::<Vec<_>>()
                .join("; ");

            if !style_str.is_empty() {
                // Merga med existerande inline style (befintliga har högre prio)
                let existing = arena
                    .nodes
                    .get(key)
                    .and_then(|n| n.get_attr("style"))
                    .unwrap_or("")
                    .to_string();

                let final_style = if existing.is_empty() {
                    style_str
                } else {
                    // Existerande inline-styles overridar computed
                    let mut merged = parse_inline_style(&style_str);
                    for (k, v) in parse_inline_style(&existing) {
                        merged.insert(k, v);
                    }
                    merged
                        .iter()
                        .map(|(k, v)| format!("{k}: {v}"))
                        .collect::<Vec<_>>()
                        .join("; ")
                };

                arena.set_attr(key, "style", &final_style);
                updated += 1;
            }
        }
        updated
    }

    // ─── Intern: samla <style>-taggar ────────────────────────────────────────

    fn collect_style_tags(
        arena: &ArenaDom,
        key: NodeKey,
        rules: &mut Vec<CascadeRule>,
        source_order: &mut usize,
    ) {
        let node = match arena.nodes.get(key) {
            Some(n) => n,
            None => return,
        };

        // Om det är en <style>-tagg, extrahera textinnehåll och parsa
        if node.node_type == NodeType::Element && node.tag.as_deref() == Some("style") {
            let css_text = Self::extract_text_content(arena, key);
            if !css_text.is_empty() {
                Self::parse_css_rules(&css_text, rules, source_order);
            }
        }

        // Rekursera till barn
        let children: Vec<NodeKey> = node.children.clone();
        for child in children {
            Self::collect_style_tags(arena, child, rules, source_order);
        }
    }

    /// Extrahera textinnehåll från alla text-barn (för <style>-taggar)
    fn extract_text_content(arena: &ArenaDom, key: NodeKey) -> String {
        let node = match arena.nodes.get(key) {
            Some(n) => n,
            None => return String::new(),
        };
        let mut text = String::new();
        for &child in &node.children {
            if let Some(child_node) = arena.nodes.get(child) {
                if child_node.node_type == NodeType::Text {
                    if let Some(t) = &child_node.text {
                        text.push_str(t);
                    }
                }
            }
        }
        text
    }

    // ─── Intern: parsa CSS-text till regler ──────────────────────────────────

    /// Enkel CSS-parser: splitta vid '}', extrahera selektor + declarations
    fn parse_css_rules(css: &str, rules: &mut Vec<CascadeRule>, source_order: &mut usize) {
        // Ta bort CSS-kommentarer
        let css = remove_css_comments(css);

        // Splitta vid '}' för att hitta regelblock
        for block in css.split('}') {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }

            // Hitta '{' som separerar selektor från declarations
            let brace_pos = match block.find('{') {
                Some(p) => p,
                None => continue,
            };

            let selector_part = block[..brace_pos].trim();
            let decl_part = block[brace_pos + 1..].trim();

            // Hoppa över @-regler (t.ex. @media, @keyframes)
            if selector_part.starts_with('@') {
                continue;
            }

            // Parsa declarations
            let properties = parse_declarations(decl_part);
            if properties.is_empty() {
                continue;
            }

            // Hantera komma-separerade selektorer: "h1, h2, .foo" → tre regler
            for sel in selector_part.split(',') {
                let sel = sel.trim();
                if sel.is_empty() {
                    continue;
                }
                let specificity = calculate_specificity(sel);
                rules.push(CascadeRule {
                    selector_text: sel.to_string(),
                    specificity,
                    properties: properties.clone(),
                    source_order: *source_order,
                });
                *source_order += 1;
            }
        }
    }

    // ─── Intern: selektor-matchning ──────────────────────────────────────────

    /// Matcha om en CSS-selektor matchar en specifik nod
    fn matches_node(&self, selector: &str, key: NodeKey, arena: &ArenaDom) -> bool {
        let selector = selector.trim();
        if selector.is_empty() {
            return false;
        }

        // Komma-separerade selektorer
        if selector.contains(',') {
            return selector
                .split(',')
                .any(|s| self.matches_node(s.trim(), key, arena));
        }

        // Kombinator-selektorer (mellanslag, >, +, ~)
        if selector.contains(' ') || selector.contains('>') {
            return self.matches_combinator(selector, key, arena);
        }

        self.matches_simple(selector, key, arena)
    }

    /// Matcha en enkel selektor (utan kombinatorer)
    fn matches_simple(&self, selector: &str, key: NodeKey, arena: &ArenaDom) -> bool {
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

        // Parsa selektor-delar
        let mut remaining = selector;
        let mut required_tag: Option<&str> = None;
        let mut required_id: Option<&str> = None;
        let mut required_classes: Vec<&str> = Vec::new();
        let mut required_attrs: Vec<(&str, Option<&str>)> = Vec::new();

        // Universell start
        if remaining.starts_with('*') {
            remaining = &remaining[1..];
        } else if remaining.starts_with(|c: char| c.is_ascii_alphabetic()) {
            let end = remaining
                .find(|c: char| ['#', '.', '[', ':'].contains(&c))
                .unwrap_or(remaining.len());
            required_tag = Some(&remaining[..end]);
            remaining = &remaining[end..];
        }

        while !remaining.is_empty() {
            if let Some(rest) = remaining.strip_prefix('#') {
                let end = rest
                    .find(|c: char| ['.', '[', ':'].contains(&c))
                    .unwrap_or(rest.len());
                required_id = Some(&rest[..end]);
                remaining = &rest[end..];
            } else if let Some(rest) = remaining.strip_prefix('.') {
                let end = rest
                    .find(|c: char| ['#', '.', '[', ':'].contains(&c))
                    .unwrap_or(rest.len());
                required_classes.push(&rest[..end]);
                remaining = &rest[end..];
            } else if let Some(rest) = remaining.strip_prefix('[') {
                let bracket_end = match rest.find(']') {
                    Some(e) => e,
                    None => break,
                };
                let attr_spec = &rest[..bracket_end];
                if let Some(eq_pos) = attr_spec.find('=') {
                    let attr_name = &attr_spec[..eq_pos];
                    let attr_val = attr_spec[eq_pos + 1..].trim_matches('"').trim_matches('\'');
                    required_attrs.push((attr_name, Some(attr_val)));
                } else {
                    required_attrs.push((attr_spec, None));
                }
                remaining = &rest[bracket_end + 1..];
            } else {
                // Skippa okända pseudo-selektorer
                break;
            }
        }

        // Verifiera
        if let Some(tag) = required_tag {
            if node.tag.as_deref() != Some(tag) {
                return false;
            }
        }
        if let Some(id) = required_id {
            if node.get_attr("id") != Some(id) {
                return false;
            }
        }
        for cls in &required_classes {
            let has = node
                .get_attr("class")
                .map(|c| c.split_whitespace().any(|x| x == *cls))
                .unwrap_or(false);
            if !has {
                return false;
            }
        }
        for (attr, val) in &required_attrs {
            match val {
                Some(v) => {
                    if node.get_attr(attr) != Some(v) {
                        return false;
                    }
                }
                None => {
                    if !node.has_attr(attr) {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Matcha selektor med kombinatorer (>, mellanslag)
    fn matches_combinator(&self, selector: &str, key: NodeKey, arena: &ArenaDom) -> bool {
        let parts: Vec<&str> = selector.split_whitespace().collect();
        if parts.is_empty() {
            return false;
        }

        let last = parts[parts.len() - 1];
        if last == ">" {
            return false;
        }
        if !self.matches_simple(last, key, arena) {
            return false;
        }

        if parts.len() == 1 {
            return true;
        }

        let is_child = parts.len() >= 2 && parts[parts.len() - 2] == ">";
        let ancestor_sel = if is_child {
            if parts.len() < 3 {
                return false;
            }
            parts[..parts.len() - 2].join(" ")
        } else {
            parts[..parts.len() - 1].join(" ")
        };

        if is_child {
            if let Some(parent) = arena.nodes.get(key).and_then(|n| n.parent) {
                return self.matches_node(&ancestor_sel, parent, arena);
            }
            false
        } else {
            let mut current = arena.nodes.get(key).and_then(|n| n.parent);
            while let Some(ancestor) = current {
                if self.matches_node(&ancestor_sel, ancestor, arena) {
                    return true;
                }
                current = arena.nodes.get(ancestor).and_then(|n| n.parent);
            }
            false
        }
    }
}

// ─── Specificitet ────────────────────────────────────────────────────────────

/// Beräkna specificitet för en CSS-selektor som (id, class, type)-tupel
fn calculate_specificity(selector: &str) -> (u32, u32, u32) {
    let mut ids = 0u32;
    let mut classes = 0u32;
    let mut types = 0u32;

    // Enkel specificitet-parser baserad på karaktärer
    let mut i = 0;
    let bytes = selector.as_bytes();
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                ids += 1;
                i += 1;
                // Skippa ID-namn
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
            }
            b'.' => {
                classes += 1;
                i += 1;
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
            }
            b'[' => {
                classes += 1;
                // Skippa till ']'
                while i < bytes.len() && bytes[i] != b']' {
                    i += 1;
                }
                if i < bytes.len() {
                    i += 1;
                }
            }
            b':' => {
                i += 1;
                // :not() — specificitet av innehållet, inte av :not
                if i + 4 <= bytes.len() && &bytes[i..i + 4] == b"not(" {
                    i += 4;
                    let start = i;
                    let mut depth = 1u32;
                    while i < bytes.len() && depth > 0 {
                        if bytes[i] == b'(' {
                            depth += 1;
                        }
                        if bytes[i] == b')' {
                            depth -= 1;
                        }
                        i += 1;
                    }
                    let inner = if i > start + 1 {
                        &selector[start..i - 1]
                    } else {
                        ""
                    };
                    let (a, b, c) = calculate_specificity(inner);
                    ids += a;
                    classes += b;
                    types += c;
                } else if i < bytes.len() && bytes[i] == b':' {
                    // Pseudo-element (::before etc) — typ-specificitet
                    types += 1;
                    i += 1;
                    while i < bytes.len() && is_ident_char(bytes[i]) {
                        i += 1;
                    }
                } else {
                    // Pseudo-klass — klass-specificitet
                    classes += 1;
                    while i < bytes.len() && is_ident_char(bytes[i]) {
                        i += 1;
                    }
                    // Skippa parenteser (t.ex. :nth-child(2n+1))
                    if i < bytes.len() && bytes[i] == b'(' {
                        let mut depth = 1u32;
                        i += 1;
                        while i < bytes.len() && depth > 0 {
                            if bytes[i] == b'(' {
                                depth += 1;
                            }
                            if bytes[i] == b')' {
                                depth -= 1;
                            }
                            i += 1;
                        }
                    }
                }
            }
            b' ' | b'>' | b'+' | b'~' => {
                i += 1;
            }
            b'*' => {
                i += 1;
            }
            _ if is_ident_start(bytes[i]) => {
                types += 1;
                while i < bytes.len() && is_ident_char(bytes[i]) {
                    i += 1;
                }
            }
            _ => {
                i += 1;
            }
        }
    }

    (ids, classes, types)
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'-'
}

fn is_ident_char(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'-'
}

// ─── CSS-parser hjälpfunktioner ──────────────────────────────────────────────

/// Ta bort /* ... */ kommentarer från CSS
fn remove_css_comments(css: &str) -> String {
    let mut result = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '/' && chars.peek() == Some(&'*') {
            chars.next(); // konsumera '*'
                          // Sök efter '*/'
            loop {
                match chars.next() {
                    Some('*') if chars.peek() == Some(&'/') => {
                        chars.next();
                        break;
                    }
                    Some(_) => continue,
                    None => break,
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parsa CSS-declarations ("color: red; font-size: 16px") till vektor
fn parse_declarations(decl_text: &str) -> Vec<(String, String)> {
    let mut props = Vec::new();
    for part in decl_text.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon) = part.find(':') {
            let prop = part[..colon].trim().to_lowercase();
            let mut val = part[colon + 1..].trim().to_string();
            // Ta bort !important-flagga (vi hanterar den inte separat ännu)
            if let Some(imp_pos) = val.to_lowercase().find("!important") {
                val = val[..imp_pos].trim().to_string();
            }
            if !prop.is_empty() && !val.is_empty() {
                props.push((prop, val));
            }
        }
    }
    props
}

// ─── Tag-defaults (UA stylesheet) ────────────────────────────────────────────

/// Returnera standard-CSS-defaults för en HTML-tagg (user-agent stylesheet)
fn get_tag_defaults(tag: &str) -> HashMap<String, String> {
    let mut d = HashMap::new();

    let display = match tag {
        "span" | "a" | "strong" | "em" | "b" | "i" | "label" | "code" | "small" | "sub" | "sup"
        | "abbr" | "cite" | "time" | "mark" | "q" => "inline",
        "button" | "input" | "select" | "textarea" => "inline-block",
        "li" => "list-item",
        "table" => "table",
        "tr" => "table-row",
        "td" | "th" => "table-cell",
        "thead" => "table-header-group",
        "tbody" => "table-row-group",
        "tfoot" => "table-footer-group",
        "col" => "table-column",
        "colgroup" => "table-column-group",
        "caption" => "table-caption",
        "none" | "script" | "style" | "head" | "meta" | "link" | "title" => "none",
        _ => "block",
    };

    let font_size = match tag {
        "h1" => "2em",
        "h2" => "1.5em",
        "h3" => "1.17em",
        "h4" => "1em",
        "h5" => "0.83em",
        "h6" => "0.67em",
        "small" | "sub" | "sup" => "smaller",
        _ => "16px",
    };

    let font_weight = match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "b" | "strong" | "th" => "bold",
        _ => "normal",
    };

    d.insert("display".into(), display.into());
    d.insert("visibility".into(), "visible".into());
    d.insert("position".into(), "static".into());
    d.insert("opacity".into(), "1".into());
    d.insert("overflow".into(), "visible".into());
    d.insert("font-size".into(), font_size.into());
    d.insert("font-weight".into(), font_weight.into());
    d.insert(
        "font-style".into(),
        if tag == "em" || tag == "i" {
            "italic"
        } else {
            "normal"
        }
        .into(),
    );
    d.insert("color".into(), "rgb(0, 0, 0)".into());
    d.insert("background-color".into(), "rgba(0, 0, 0, 0)".into());
    d.insert("width".into(), "auto".into());
    d.insert("height".into(), "auto".into());
    d.insert("margin".into(), "0px".into());
    d.insert("padding".into(), "0px".into());
    d.insert("z-index".into(), "auto".into());
    d.insert("pointer-events".into(), "auto".into());
    d.insert("box-sizing".into(), "content-box".into());
    d.insert("text-align".into(), "start".into());
    d.insert(
        "text-decoration".into(),
        if tag == "a" { "underline" } else { "none" }.into(),
    );
    d.insert(
        "cursor".into(),
        if tag == "a" || tag == "button" {
            "pointer"
        } else {
            "auto"
        }
        .into(),
    );
    d.insert(
        "white-space".into(),
        if tag == "pre" || tag == "code" {
            "pre"
        } else {
            "normal"
        }
        .into(),
    );

    // Heading-marginaler
    match tag {
        "h1" => {
            d.insert("margin".into(), "0.67em 0".into());
        }
        "h2" => {
            d.insert("margin".into(), "0.83em 0".into());
        }
        "h3" => {
            d.insert("margin".into(), "1em 0".into());
        }
        "p" => {
            d.insert("margin".into(), "1em 0".into());
        }
        _ => {}
    }

    d
}

/// Parsa inline style-sträng till HashMap (prop → value)
fn parse_inline_style(style: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for part in style.split(';') {
        let part = part.trim();
        if let Some(colon) = part.find(':') {
            let prop = part[..colon].trim().to_lowercase();
            let val = part[colon + 1..].trim().to_string();
            if !prop.is_empty() {
                map.insert(prop, val);
            }
        }
    }
    map
}

/// CSS-properties som påverkar visuell rendering (filtrera bort inherited defaults)
fn is_render_relevant(prop: &str) -> bool {
    matches!(
        prop,
        "display"
            | "visibility"
            | "opacity"
            | "color"
            | "background-color"
            | "background"
            | "background-image"
            | "font-size"
            | "font-weight"
            | "font-family"
            | "font-style"
            | "text-align"
            | "text-decoration"
            | "text-transform"
            | "line-height"
            | "letter-spacing"
            | "word-spacing"
            | "margin"
            | "margin-top"
            | "margin-right"
            | "margin-bottom"
            | "margin-left"
            | "padding"
            | "padding-top"
            | "padding-right"
            | "padding-bottom"
            | "padding-left"
            | "border"
            | "border-top"
            | "border-right"
            | "border-bottom"
            | "border-left"
            | "border-radius"
            | "border-color"
            | "border-width"
            | "border-style"
            | "width"
            | "height"
            | "min-width"
            | "min-height"
            | "max-width"
            | "max-height"
            | "position"
            | "top"
            | "right"
            | "bottom"
            | "left"
            | "z-index"
            | "overflow"
            | "overflow-x"
            | "overflow-y"
            | "float"
            | "clear"
            | "flex"
            | "flex-direction"
            | "flex-wrap"
            | "justify-content"
            | "align-items"
            | "align-self"
            | "gap"
            | "grid-template-columns"
            | "grid-template-rows"
            | "box-shadow"
            | "text-shadow"
            | "transform"
            | "transition"
            | "cursor"
            | "white-space"
            | "list-style-type"
    )
}

/// Apply CSS cascade computed styles as inline styles on HTML.
///
/// Parses the HTML into an ArenaDom, resolves the CSS cascade
/// (specificity, inheritance), and writes computed properties as
/// inline `style` attributes so that Blitz rendering picks them up.
///
/// Returns the modified HTML with inlined computed styles and
/// the number of nodes updated.
pub fn apply_cascade_to_html(html: &str) -> (String, usize) {
    let rcdom = crate::parser::parse_html(html);
    let mut arena = ArenaDom::from_rcdom(&rcdom);
    let mut ctx = CssContext::from_arena(&arena);

    // Applicera bara om det finns CSS-regler
    if ctx.rules.is_empty() {
        return (html.to_string(), 0);
    }

    let updated = ctx.apply_computed_styles_inline(&mut arena);
    let result_html = arena.serialize_inner_html(arena.document);
    (result_html, updated)
}

// ─── Tester ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_html;

    fn make_arena(html: &str) -> ArenaDom {
        let rcdom = parse_html(html);
        ArenaDom::from_rcdom(&rcdom)
    }

    #[test]
    fn test_specificity_calculation() {
        assert_eq!(
            calculate_specificity("div"),
            (0, 0, 1),
            "Tag-selektor ska ge (0,0,1)"
        );
        assert_eq!(
            calculate_specificity(".foo"),
            (0, 1, 0),
            "Klass-selektor ska ge (0,1,0)"
        );
        assert_eq!(
            calculate_specificity("#bar"),
            (1, 0, 0),
            "ID-selektor ska ge (1,0,0)"
        );
        assert_eq!(
            calculate_specificity("div.foo#bar"),
            (1, 1, 1),
            "Kombinerad selektor ska ge (1,1,1)"
        );
        assert_eq!(
            calculate_specificity("div span"),
            (0, 0, 2),
            "Descendant ska ge (0,0,2)"
        );
        assert_eq!(
            calculate_specificity(".a .b"),
            (0, 2, 0),
            "Två klasser ska ge (0,2,0)"
        );
        assert_eq!(
            calculate_specificity(":not(.foo)"),
            (0, 1, 0),
            ":not() ska ge specificiteten av innehållet"
        );
    }

    #[test]
    fn test_parse_css_rules() {
        let css = r#"
            .foo { color: red; font-size: 14px; }
            #bar { color: blue; }
            div { margin: 0; }
        "#;
        let mut rules = Vec::new();
        let mut order = 0;
        CssContext::parse_css_rules(css, &mut rules, &mut order);
        assert_eq!(rules.len(), 3, "Ska parsa tre regler");
        assert_eq!(rules[0].selector_text, ".foo");
        assert_eq!(rules[1].selector_text, "#bar");
        assert_eq!(rules[2].selector_text, "div");
    }

    #[test]
    fn test_css_comments_removed() {
        let css = "/* kommentar */ .foo { color: red; } /* slut */";
        let cleaned = remove_css_comments(css);
        assert!(!cleaned.contains("kommentar"), "Kommentarer ska tas bort");
        assert!(cleaned.contains(".foo"), "Regler ska bevaras");
    }

    #[test]
    fn test_cascade_specificity_order() {
        let html = r##"<html><head><style>
            .highlight { color: red; }
            #main { color: blue; }
            div { color: green; }
        </style></head><body>
            <div id="main" class="highlight">Test</div>
        </body></html>"##;

        let arena = make_arena(html);
        let mut ctx = CssContext::from_arena(&arena);

        // Hitta div#main.highlight
        let key = find_element(&arena, arena.document, "div");
        if let Some(k) = key {
            let style = ctx.get_computed_style(k, &arena);
            assert_eq!(
                style.properties.get("color").map(|s| s.as_str()),
                Some("blue"),
                "ID-selektor (#main) ska vinna över klass (.highlight) och tag (div)"
            );
        }
    }

    #[test]
    fn test_inline_style_wins() {
        let html = r##"<html><head><style>
            .red { color: red; }
        </style></head><body>
            <div class="red" style="color: green">Test</div>
        </body></html>"##;

        let arena = make_arena(html);
        let mut ctx = CssContext::from_arena(&arena);

        let key = find_element(&arena, arena.document, "div");
        if let Some(k) = key {
            let style = ctx.get_computed_style(k, &arena);
            assert_eq!(
                style.properties.get("color").map(|s| s.as_str()),
                Some("green"),
                "Inline style ska vinna över stylesheet"
            );
        }
    }

    #[test]
    fn test_inheritance() {
        let html = r##"<html><head><style>
            .parent { color: red; font-size: 20px; }
        </style></head><body>
            <div class="parent"><span>Child</span></div>
        </body></html>"##;

        let arena = make_arena(html);
        let mut ctx = CssContext::from_arena(&arena);

        // Hitta span (barnet)
        let key = find_element(&arena, arena.document, "span");
        if let Some(k) = key {
            let style = ctx.get_computed_style(k, &arena);
            assert_eq!(
                style.properties.get("color").map(|s| s.as_str()),
                Some("red"),
                "color ska ärvas från förälder"
            );
        }
    }

    #[test]
    fn test_tag_defaults() {
        let defaults = get_tag_defaults("h1");
        assert_eq!(defaults.get("display").map(|s| s.as_str()), Some("block"));
        assert_eq!(defaults.get("font-size").map(|s| s.as_str()), Some("2em"));
        assert_eq!(
            defaults.get("font-weight").map(|s| s.as_str()),
            Some("bold")
        );
    }

    // Hjälpfunktion: hitta element med tagg
    fn find_element(arena: &ArenaDom, key: NodeKey, tag: &str) -> Option<NodeKey> {
        let node = arena.nodes.get(key)?;
        if node.tag.as_deref() == Some(tag) && node.node_type == NodeType::Element {
            return Some(key);
        }
        for &child in &node.children {
            if let Some(found) = find_element(arena, child, tag) {
                return Some(found);
            }
        }
        None
    }

    #[test]
    fn test_apply_computed_styles_inline() {
        let html = r##"<html><head><style>
            .red { color: red; font-size: 16px; }
            p { margin: 10px; }
        </style></head><body>
            <p class="red">Hello</p>
        </body></html>"##;

        let rcdom = parse_html(html);
        let mut arena = ArenaDom::from_rcdom(&rcdom);
        let mut ctx = CssContext::from_arena(&arena);

        let updated = ctx.apply_computed_styles_inline(&mut arena);
        assert!(
            updated > 0,
            "Borde ha uppdaterat minst en nod med inline styles"
        );

        // Hitta <p>-elementet och verifiera att det har inline styles
        let p_key = find_element(&arena, arena.document, "p");
        assert!(p_key.is_some(), "Borde hitta p-element");
        if let Some(k) = p_key {
            let style_attr = arena.nodes.get(k).and_then(|n| n.get_attr("style"));
            assert!(
                style_attr.is_some(),
                "p-element borde ha style-attribut efter cascade"
            );
            let style = style_attr.unwrap();
            assert!(
                style.contains("color: red"),
                "Style borde innehålla 'color: red', fick: {style}"
            );
        }
    }

    #[test]
    fn test_apply_cascade_to_html() {
        let html = r##"<html><head><style>
            h1 { color: blue; font-weight: bold; }
        </style></head><body>
            <h1>Rubrik</h1>
        </body></html>"##;

        let (result, updated) = apply_cascade_to_html(html);
        assert!(updated > 0, "Borde ha uppdaterat noder");
        assert!(
            result.contains("color: blue"),
            "Resultat-HTML borde innehålla 'color: blue', fick: {}",
            &result[..result.len().min(500)]
        );
    }

    #[test]
    fn test_apply_cascade_no_rules() {
        let html = "<html><body><p>No CSS</p></body></html>";
        let (result, updated) = apply_cascade_to_html(html);
        assert_eq!(updated, 0, "Utan CSS-regler ska inget uppdateras");
        assert_eq!(result, html, "HTML ska vara oförändrad utan CSS-regler");
    }

    #[test]
    fn test_is_render_relevant() {
        assert!(is_render_relevant("color"), "color ska vara relevant");
        assert!(is_render_relevant("display"), "display ska vara relevant");
        assert!(
            is_render_relevant("font-size"),
            "font-size ska vara relevant"
        );
        assert!(
            !is_render_relevant("animation"),
            "animation ska inte vara relevant"
        );
        assert!(
            !is_render_relevant("content"),
            "content ska inte vara relevant"
        );
    }

    #[test]
    fn test_parse_inline_style() {
        let parsed = parse_inline_style("color: red; font-size: 14px; margin: 0");
        assert_eq!(parsed.get("color").map(|s| s.as_str()), Some("red"));
        assert_eq!(parsed.get("font-size").map(|s| s.as_str()), Some("14px"));
        assert_eq!(parsed.get("margin").map(|s| s.as_str()), Some("0"));
    }
}
