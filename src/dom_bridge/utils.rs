// ─── Hjälpfunktioner (oberoende av QuickJS) ──────────────────────────────────
//
// Utbruten från dom_bridge/mod.rs: style-parsing, base64, URL, media query, layout.

use crate::arena_dom::{ArenaDom, NodeKey};

// ─── Hjälpfunktioner (boa-oberoende) ──────────────────────────────────────

pub(super) fn parse_inline_styles(style_attr: &str) -> std::collections::HashMap<String, String> {
    let mut styles = std::collections::HashMap::new();
    for part in style_attr.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon_pos) = part.find(':') {
            let prop = part[..colon_pos].trim().to_lowercase();
            let val = part[colon_pos + 1..].trim().to_string();
            if !prop.is_empty() && is_valid_css_declaration(&prop, &val) && !val.is_empty() {
                styles.insert(prop, val);
            } else if !prop.is_empty() && !is_valid_css_declaration(&prop, &val) {
                return std::collections::HashMap::new();
            }
        }
    }
    styles
}

/// Serialisera inline CSS-stilar till style-attribut-sträng
pub(super) fn serialize_inline_styles(
    styles: &std::collections::HashMap<String, String>,
) -> String {
    if styles.is_empty() {
        return String::new();
    }
    let mut parts: Vec<String> = styles
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect();
    parts.sort();
    parts.join("; ")
}

/// Serialisera inline CSS som cssText med bevarad ordning och shorthand-aggregering
pub(super) fn serialize_css_text_ordered(props: &[(String, String)]) -> String {
    if props.is_empty() {
        return String::new();
    }
    // Aggregera longhands till shorthands
    let aggregated = aggregate_shorthands_ordered(props);
    let parts: Vec<String> = aggregated
        .iter()
        .map(|(k, v)| format!("{}: {};", k, v))
        .collect();
    parts.join(" ")
}

/// Aggregera longhands till shorthands, bevarar ordning
fn aggregate_shorthands_ordered(props: &[(String, String)]) -> Vec<(String, String)> {
    let result: Vec<(String, String)> = props.to_vec();
    let mut to_remove = std::collections::HashSet::new();
    let mut to_insert: Vec<(usize, String, String)> = Vec::new();

    // Fyrvärdes-aggregering
    for &(shorthand, ref longhands) in FOUR_VALUE_SHORTHANDS {
        let vals: Vec<Option<(usize, &str)>> = longhands
            .iter()
            .map(|l| {
                result
                    .iter()
                    .enumerate()
                    .find(|(_, (k, _))| k == l && !to_remove.contains(k.as_str()))
                    .map(|(i, (_, v))| (i, v.as_str()))
            })
            .collect();
        if vals.iter().all(|v| v.is_some()) {
            let v: Vec<(usize, &str)> = vals.into_iter().map(|v| v.unwrap()).collect();
            // Alla longhands finns — aggregera
            let min_pos = v.iter().map(|(i, _)| *i).min().unwrap_or(0);
            for l in longhands {
                to_remove.insert(*l);
            }
            // Komprimera: 4→1, 4→2, 4→3
            let aggregated = if v[0].1 == v[1].1 && v[1].1 == v[2].1 && v[2].1 == v[3].1 {
                v[0].1.to_string()
            } else if v[0].1 == v[2].1 && v[1].1 == v[3].1 {
                format!("{} {}", v[0].1, v[1].1)
            } else if v[1].1 == v[3].1 {
                format!("{} {} {}", v[0].1, v[1].1, v[2].1)
            } else {
                format!("{} {} {} {}", v[0].1, v[1].1, v[2].1, v[3].1)
            };
            to_insert.push((min_pos, shorthand.to_string(), aggregated));
        }
    }

    // Tvåvärdes-aggregering
    for &(shorthand, ref longhands) in TWO_VALUE_SHORTHANDS {
        let v0 = result
            .iter()
            .enumerate()
            .find(|(_, (k, _))| k == longhands[0] && !to_remove.contains(k.as_str()));
        let v1 = result
            .iter()
            .enumerate()
            .find(|(_, (k, _))| k == longhands[1] && !to_remove.contains(k.as_str()));
        if let (Some((i0, (_, a))), Some((_, (_, b)))) = (v0, v1) {
            to_remove.insert(longhands[0]);
            to_remove.insert(longhands[1]);
            let aggregated = if a == b {
                a.clone()
            } else {
                format!("{} {}", a, b)
            };
            to_insert.push((i0, shorthand.to_string(), aggregated));
        }
    }

    // Outline-aggregering
    let oc = result
        .iter()
        .enumerate()
        .find(|(_, (k, _))| k == "outline-color" && !to_remove.contains(k.as_str()));
    let os = result
        .iter()
        .enumerate()
        .find(|(_, (k, _))| k == "outline-style" && !to_remove.contains(k.as_str()));
    let ow = result
        .iter()
        .enumerate()
        .find(|(_, (k, _))| k == "outline-width" && !to_remove.contains(k.as_str()));
    if let (Some((ic, (_, c))), Some((_, (_, s))), Some((_, (_, w)))) = (oc, os, ow) {
        to_remove.insert("outline-color");
        to_remove.insert("outline-style");
        to_remove.insert("outline-width");
        to_insert.push((ic, "outline".to_string(), format!("{} {} {}", c, s, w)));
    }

    // List-style-aggregering — bara om minst 2 av 3 sub-properties finns
    let lt = result
        .iter()
        .enumerate()
        .find(|(_, (k, _))| k == "list-style-type" && !to_remove.contains(k.as_str()));
    let lp = result
        .iter()
        .enumerate()
        .find(|(_, (k, _))| k == "list-style-position" && !to_remove.contains(k.as_str()));
    let li = result
        .iter()
        .enumerate()
        .find(|(_, (k, _))| k == "list-style-image" && !to_remove.contains(k.as_str()));
    let ls_count = [lt.is_some(), lp.is_some(), li.is_some()]
        .iter()
        .filter(|&&x| x)
        .count();
    if ls_count >= 2 {
        let mut parts = Vec::new();
        let mut min_pos = usize::MAX;
        if let Some((i, (_, p))) = lp {
            parts.push(p.clone());
            min_pos = min_pos.min(i);
            to_remove.insert("list-style-position");
        }
        if let Some((i, (_, t))) = lt {
            parts.push(t.clone());
            min_pos = min_pos.min(i);
            to_remove.insert("list-style-type");
        }
        if let Some((i, (_, img))) = li {
            if img != "none" {
                parts.push(img.clone());
            }
            min_pos = min_pos.min(i);
            to_remove.insert("list-style-image");
        }
        if !parts.is_empty() {
            to_insert.push((min_pos, "list-style".to_string(), parts.join(" ")));
        }
    }

    // Sortera inserts efter originalposition
    to_insert.sort_by_key(|(pos, _, _)| *pos);

    // Bygg ny ordnad lista: ersätt första longhand med shorthand, ta bort resten
    let mut final_result = Vec::new();
    let mut insert_iter = to_insert.iter().peekable();
    for (i, (k, v)) in result.iter().enumerate() {
        if to_remove.contains(k.as_str()) {
            // Kolla om en shorthand ska infogas på denna position
            while let Some((pos, name, val)) = insert_iter.peek() {
                if *pos == i {
                    final_result.push((name.clone(), val.clone()));
                    insert_iter.next();
                } else {
                    break;
                }
            }
            // Longhand hoppas över
        } else {
            final_result.push((k.clone(), v.clone()));
        }
    }
    // Infoga eventuella kvarvarande shorthands i slutet
    for (_, name, val) in insert_iter {
        final_result.push((name.clone(), val.clone()));
    }

    final_result
}

/// Parsa inline styles med ordning bevarad
pub(super) fn parse_inline_styles_ordered(style_attr: &str) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for part in style_attr.split(';') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(colon_pos) = part.find(':') {
            let prop = part[..colon_pos].trim().to_lowercase();
            let val = part[colon_pos + 1..].trim().to_string();
            if !prop.is_empty() && is_valid_css_declaration(&prop, &val) && !val.is_empty() {
                // Uppdatera om redan finns, annars lägg till
                if let Some(existing) = result.iter_mut().find(|(k, _)| k == &prop) {
                    existing.1 = val;
                } else {
                    result.push((prop, val));
                }
            } else if !prop.is_empty() && !is_valid_css_declaration(&prop, &val) {
                return Vec::new();
            }
        }
    }
    result
}

/// Validera en enskild CSS-deklaration (prop: val)
pub(super) fn is_valid_css_declaration(prop: &str, val: &str) -> bool {
    let prop = prop.trim();
    if prop.is_empty() {
        return false;
    }
    // Reject property med mellanslag eller dubbla kolon
    if prop.contains("::") || prop.contains(' ') {
        return false;
    }
    // Reject value som börjar med kolon (t.ex. "color:: invalid" → prop="color", val=": invalid")
    let val = val.trim();
    if val.starts_with(':') {
        return false;
    }
    true
}



// ─── CSS Shorthand expansion/aggregation ──────────────────────────────────────

/// Fyrvärdes-shorthands: margin, padding, border-width, border-style, border-color
const FOUR_VALUE_SHORTHANDS: &[(&str, [&str; 4])] = &[
    (
        "margin",
        ["margin-top", "margin-right", "margin-bottom", "margin-left"],
    ),
    (
        "padding",
        [
            "padding-top",
            "padding-right",
            "padding-bottom",
            "padding-left",
        ],
    ),
    (
        "border-width",
        [
            "border-top-width",
            "border-right-width",
            "border-bottom-width",
            "border-left-width",
        ],
    ),
    (
        "border-style",
        [
            "border-top-style",
            "border-right-style",
            "border-bottom-style",
            "border-left-style",
        ],
    ),
    (
        "border-color",
        [
            "border-top-color",
            "border-right-color",
            "border-bottom-color",
            "border-left-color",
        ],
    ),
];

/// Tvåvärdes-shorthands: overflow
const TWO_VALUE_SHORTHANDS: &[(&str, [&str; 2])] = &[("overflow", ["overflow-x", "overflow-y"])];

/// Expandera shorthand till longhands (används vid setProperty)
pub(super) fn expand_shorthand(
    prop: &str,
    val: &str,
    styles: &mut std::collections::HashMap<String, String>,
) {
    // Behåll shorthand också (för serialisering)
    for &(shorthand, ref longhands) in FOUR_VALUE_SHORTHANDS {
        if prop == shorthand {
            let parts: Vec<&str> = val.split_whitespace().collect();
            let (t, r, b, l) = match parts.len() {
                1 => (parts[0], parts[0], parts[0], parts[0]),
                2 => (parts[0], parts[1], parts[0], parts[1]),
                3 => (parts[0], parts[1], parts[2], parts[1]),
                4 => (parts[0], parts[1], parts[2], parts[3]),
                _ => return,
            };
            styles.insert(longhands[0].to_string(), t.to_string());
            styles.insert(longhands[1].to_string(), r.to_string());
            styles.insert(longhands[2].to_string(), b.to_string());
            styles.insert(longhands[3].to_string(), l.to_string());
            return;
        }
    }
    for &(shorthand, ref longhands) in TWO_VALUE_SHORTHANDS {
        if prop == shorthand {
            let parts: Vec<&str> = val.split_whitespace().collect();
            let (x, y) = match parts.len() {
                1 => (parts[0], parts[0]),
                2 => (parts[0], parts[1]),
                _ => return,
            };
            styles.insert(longhands[0].to_string(), x.to_string());
            styles.insert(longhands[1].to_string(), y.to_string());
            return;
        }
    }
    // outline: color style width
    if prop == "outline" && val != "none" {
        let parts: Vec<&str> = val.split_whitespace().collect();
        if parts.len() >= 3 {
            styles.insert("outline-color".to_string(), parts[0].to_string());
            styles.insert("outline-style".to_string(), parts[1].to_string());
            styles.insert("outline-width".to_string(), parts[2].to_string());
        }
    }
    // list-style: type position image
    if prop == "list-style" {
        let parts: Vec<&str> = val.split_whitespace().collect();
        // Förenklad: positionsord
        for p in &parts {
            if *p == "inside" || *p == "outside" {
                styles.insert("list-style-position".to_string(), p.to_string());
            } else if *p == "none"
                || *p == "disc"
                || *p == "circle"
                || *p == "square"
                || *p == "decimal"
                || *p == "lower-alpha"
                || *p == "upper-alpha"
                || *p == "lower-roman"
                || *p == "upper-roman"
            {
                styles.insert("list-style-type".to_string(), p.to_string());
            } else if p.starts_with("url(") {
                styles.insert("list-style-image".to_string(), p.to_string());
            }
        }
    }
}

/// Aggregera longhands till shorthands (vid serialisering)
#[allow(dead_code)]
fn aggregate_shorthands(styles: &mut std::collections::HashMap<String, String>) {
    // Fyrvärdes-aggregering
    for &(shorthand, ref longhands) in FOUR_VALUE_SHORTHANDS {
        let vals: Vec<Option<String>> = longhands.iter().map(|l| styles.get(*l).cloned()).collect();
        if vals.iter().all(|v| v.is_some()) {
            let v: Vec<&str> = vals.iter().map(|v| v.as_deref().unwrap_or("")).collect();
            // Ta bort longhands
            for l in longhands {
                styles.remove(*l);
            }
            // Ta bort ev. befintlig shorthand
            styles.remove(shorthand);
            // Komprimera: 4→1, 4→2, 4→3
            let aggregated = if v[0] == v[1] && v[1] == v[2] && v[2] == v[3] {
                v[0].to_string()
            } else if v[0] == v[2] && v[1] == v[3] {
                format!("{} {}", v[0], v[1])
            } else if v[1] == v[3] {
                format!("{} {} {}", v[0], v[1], v[2])
            } else {
                format!("{} {} {} {}", v[0], v[1], v[2], v[3])
            };
            styles.insert(shorthand.to_string(), aggregated);
        }
    }
    // Tvåvärdes-aggregering
    for &(shorthand, ref longhands) in TWO_VALUE_SHORTHANDS {
        let v0 = styles.get(longhands[0]).cloned();
        let v1 = styles.get(longhands[1]).cloned();
        if let (Some(a), Some(b)) = (v0, v1) {
            styles.remove(longhands[0]);
            styles.remove(longhands[1]);
            styles.remove(shorthand);
            let aggregated = if a == b { a } else { format!("{} {}", a, b) };
            styles.insert(shorthand.to_string(), aggregated);
        }
    }
    // outline-aggregering
    let oc = styles.get("outline-color").cloned();
    let os = styles.get("outline-style").cloned();
    let ow = styles.get("outline-width").cloned();
    if let (Some(c), Some(s), Some(w)) = (oc, os, ow) {
        styles.remove("outline-color");
        styles.remove("outline-style");
        styles.remove("outline-width");
        styles.remove("outline");
        styles.insert("outline".to_string(), format!("{} {} {}", c, s, w));
    }
    // list-style-aggregering
    let lt = styles.get("list-style-type").cloned();
    let lp = styles.get("list-style-position").cloned();
    let li = styles.get("list-style-image").cloned();
    if lt.is_some() || lp.is_some() || li.is_some() {
        let mut parts = Vec::new();
        if let Some(ref p) = lp {
            parts.push(p.as_str());
            styles.remove("list-style-position");
        }
        if let Some(ref t) = lt {
            parts.push(t.as_str());
            styles.remove("list-style-type");
        }
        if let Some(ref i) = li {
            if i != "none" {
                parts.push(i.as_str());
            }
            styles.remove("list-style-image");
        }
        if !parts.is_empty() {
            styles.remove("list-style");
            styles.insert("list-style".to_string(), parts.join(" "));
        }
    }
}

/// Rekonstruera shorthand-värde från longhands (för getPropertyValue)
pub(super) fn reconstruct_shorthand(
    prop: &str,
    styles: &std::collections::HashMap<String, String>,
) -> String {
    for &(shorthand, ref longhands) in FOUR_VALUE_SHORTHANDS {
        if prop == shorthand {
            let vals: Vec<Option<&String>> = longhands.iter().map(|l| styles.get(*l)).collect();
            if vals.iter().all(|v| v.is_some()) {
                let v: Vec<&str> = vals.into_iter().map(|v| v.unwrap().as_str()).collect();
                return if v[0] == v[1] && v[1] == v[2] && v[2] == v[3] {
                    v[0].to_string()
                } else if v[0] == v[2] && v[1] == v[3] {
                    format!("{} {}", v[0], v[1])
                } else if v[1] == v[3] {
                    format!("{} {} {}", v[0], v[1], v[2])
                } else {
                    format!("{} {} {} {}", v[0], v[1], v[2], v[3])
                };
            }
            return String::new();
        }
    }
    for &(shorthand, ref longhands) in TWO_VALUE_SHORTHANDS {
        if prop == shorthand {
            let v0 = styles.get(longhands[0]);
            let v1 = styles.get(longhands[1]);
            if let (Some(a), Some(b)) = (v0, v1) {
                return if a == b {
                    a.clone()
                } else {
                    format!("{} {}", a, b)
                };
            }
            return String::new();
        }
    }
    String::new()
}

/// Ta bort longhands om shorthand sätts (vid removeProperty)
pub(super) fn remove_shorthand_longhands(
    prop: &str,
    styles: &mut std::collections::HashMap<String, String>,
) {
    for &(shorthand, ref longhands) in FOUR_VALUE_SHORTHANDS {
        if prop == shorthand {
            for l in longhands {
                styles.remove(*l);
            }
            return;
        }
    }
    for &(shorthand, ref longhands) in TWO_VALUE_SHORTHANDS {
        if prop == shorthand {
            for l in longhands {
                styles.remove(*l);
            }
            return;
        }
    }
}

/// Parsea px-värde från CSS-egenskap
pub(super) fn parse_px_value(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix("px")
        .unwrap_or(value.trim())
        .parse::<f64>()
        .ok()
}

/// Estimera layout-rect baserat på tagg + inline styles
pub(super) fn estimate_layout_rect(arena: &ArenaDom, key: NodeKey) -> (f64, f64, f64, f64) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return (0.0, 0.0, 100.0, 30.0),
    };
    let style_str = node.get_attr("style").unwrap_or("");
    let styles = parse_inline_styles(style_str);
    let tag = node.tag.as_deref().unwrap_or("");
    let (default_w, default_h) = match tag {
        "div" | "section" | "main" | "article" | "header" | "footer" | "nav" | "form" => {
            (1024.0, 50.0)
        }
        "p" => (1024.0, 20.0),
        "h1" => (1024.0, 40.0),
        "h2" => (1024.0, 36.0),
        "h3" => (1024.0, 32.0),
        "h4" | "h5" | "h6" => (1024.0, 28.0),
        "button" => (80.0, 36.0),
        "input" => (200.0, 36.0),
        "select" => (200.0, 36.0),
        "textarea" => (300.0, 80.0),
        "a" | "span" | "label" => (60.0, 20.0),
        "img" => {
            let iw = node
                .get_attr("width")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(300.0);
            let ih = node
                .get_attr("height")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(200.0);
            (iw, ih)
        }
        "li" => (1024.0, 24.0),
        "ul" | "ol" => (1024.0, 100.0),
        "table" => (1024.0, 200.0),
        "tr" => (1024.0, 30.0),
        "td" | "th" => (200.0, 30.0),
        _ => (100.0, 30.0),
    };

    let width = styles
        .get("width")
        .and_then(|v| parse_px_value(v))
        .unwrap_or(default_w);
    let height = styles
        .get("height")
        .and_then(|v| parse_px_value(v))
        .unwrap_or(default_h);
    // Estimera y-position från syskon-ordning
    let y = match node.parent.and_then(|p| arena.nodes.get(p)) {
        Some(parent) => {
            let my_idx = parent.children.iter().position(|&c| c == key).unwrap_or(0);
            (my_idx as f64) * 30.0
        }
        None => 0.0,
    };

    (0.0, y, width, height)
}

/// Tag-baserade CSS-defaults för getComputedStyle
pub(super) fn get_tag_style_defaults(tag: &str) -> std::collections::HashMap<String, String> {
    let mut defaults = std::collections::HashMap::new();
    let display = match tag {
        "span" | "a" | "strong" | "em" | "b" | "i" | "label" | "img" => "inline",
        "button" | "input" | "select" | "textarea" => "inline-block",
        _ => "block",
    };
    let font_size = match tag {
        "h1" => "32px",
        "h2" => "24px",
        "h3" => "18.72px",
        "h4" => "16px",
        "h5" => "13.28px",
        "h6" => "10.72px",
        _ => "16px",
    };
    defaults.insert("display".to_string(), display.to_string());
    defaults.insert("visibility".to_string(), "visible".to_string());
    defaults.insert("position".to_string(), "static".to_string());
    defaults.insert("opacity".to_string(), "1".to_string());
    defaults.insert("overflow".to_string(), "visible".to_string());
    defaults.insert("font-size".to_string(), font_size.to_string());
    defaults.insert("color".to_string(), "rgb(0, 0, 0)".to_string());
    defaults.insert(
        "background-color".to_string(),
        "rgba(0, 0, 0, 0)".to_string(),
    );
    defaults.insert("width".to_string(), "auto".to_string());
    defaults.insert("height".to_string(), "auto".to_string());
    defaults.insert("margin".to_string(), "0px".to_string());
    defaults.insert("padding".to_string(), "0px".to_string());
    defaults.insert("z-index".to_string(), "auto".to_string());
    defaults.insert("pointer-events".to_string(), "auto".to_string());
    defaults.insert("box-sizing".to_string(), "content-box".to_string());
    // Ytterligare CSS-defaults som WPT-tester förväntar sig
    defaults.insert("background-blend-mode".to_string(), "normal".to_string());
    defaults.insert("background-size".to_string(), "auto".to_string());
    defaults.insert("background-position".to_string(), "0% 0%".to_string());
    defaults.insert("background-image".to_string(), "none".to_string());
    defaults.insert("background-repeat".to_string(), "repeat".to_string());
    defaults.insert("box-shadow".to_string(), "none".to_string());
    defaults.insert("clip-path".to_string(), "none".to_string());
    defaults.insert("column-span".to_string(), "none".to_string());
    defaults.insert("filter".to_string(), "none".to_string());
    defaults.insert("mask".to_string(), "none".to_string());
    defaults.insert("object-position".to_string(), "50% 50%".to_string());
    defaults.insert("object-fit".to_string(), "fill".to_string());
    defaults.insert("perspective-origin".to_string(), "50% 50%".to_string());
    defaults.insert("text-shadow".to_string(), "none".to_string());
    defaults.insert("transform-origin".to_string(), "50% 50% 0px".to_string());
    defaults.insert("transform".to_string(), "none".to_string());
    defaults.insert("transition".to_string(), "none".to_string());
    defaults.insert("animation".to_string(), "none".to_string());
    defaults.insert("border-radius".to_string(), "0px".to_string());
    defaults.insert("outline".to_string(), "none".to_string());
    defaults.insert("cursor".to_string(), "auto".to_string());
    defaults.insert("float".to_string(), "none".to_string());
    defaults.insert("clear".to_string(), "none".to_string());
    defaults.insert("text-align".to_string(), "start".to_string());
    defaults.insert("text-decoration".to_string(), "none".to_string());
    defaults.insert("line-height".to_string(), "normal".to_string());
    defaults.insert("letter-spacing".to_string(), "normal".to_string());
    defaults.insert("word-spacing".to_string(), "normal".to_string());
    defaults.insert("white-space".to_string(), "normal".to_string());
    defaults.insert("text-transform".to_string(), "none".to_string());
    defaults.insert("text-indent".to_string(), "0px".to_string());
    defaults.insert("vertical-align".to_string(), "baseline".to_string());
    defaults.insert("list-style-type".to_string(), "disc".to_string());
    defaults.insert("table-layout".to_string(), "auto".to_string());
    defaults.insert("border-collapse".to_string(), "separate".to_string());
    defaults.insert("font-family".to_string(), "serif".to_string());
    defaults.insert("font-weight".to_string(), "400".to_string());
    defaults.insert("font-style".to_string(), "normal".to_string());
    defaults.insert("text-overflow".to_string(), "clip".to_string());
    defaults.insert("word-break".to_string(), "normal".to_string());
    defaults.insert("overflow-wrap".to_string(), "normal".to_string());
    defaults.insert("resize".to_string(), "none".to_string());
    defaults.insert("inset".to_string(), "auto".to_string());
    defaults.insert("top".to_string(), "auto".to_string());
    defaults.insert("right".to_string(), "auto".to_string());
    defaults.insert("bottom".to_string(), "auto".to_string());
    defaults.insert("left".to_string(), "auto".to_string());
    defaults.insert("min-width".to_string(), "auto".to_string());
    defaults.insert("max-width".to_string(), "none".to_string());
    defaults.insert("min-height".to_string(), "auto".to_string());
    defaults.insert("max-height".to_string(), "none".to_string());
    defaults.insert("flex-direction".to_string(), "row".to_string());
    defaults.insert("flex-wrap".to_string(), "nowrap".to_string());
    defaults.insert("justify-content".to_string(), "normal".to_string());
    defaults.insert("align-items".to_string(), "normal".to_string());
    defaults.insert("align-content".to_string(), "normal".to_string());
    defaults.insert("flex-grow".to_string(), "0".to_string());
    defaults.insert("flex-shrink".to_string(), "1".to_string());
    defaults.insert("flex-basis".to_string(), "auto".to_string());
    defaults.insert("order".to_string(), "0".to_string());
    defaults.insert("gap".to_string(), "normal".to_string());
    defaults.insert("user-select".to_string(), "auto".to_string());
    defaults.insert("will-change".to_string(), "auto".to_string());
    defaults.insert("contain".to_string(), "none".to_string());
    defaults.insert("isolation".to_string(), "auto".to_string());
    defaults.insert("mix-blend-mode".to_string(), "normal".to_string());
    defaults.insert("writing-mode".to_string(), "horizontal-tb".to_string());
    defaults.insert("direction".to_string(), "ltr".to_string());
    defaults.insert("unicode-bidi".to_string(), "normal".to_string());
    defaults.insert("appearance".to_string(), "none".to_string());
    defaults.insert("content".to_string(), "normal".to_string());
    defaults.insert("accent-color".to_string(), "auto".to_string());
    defaults.insert("touch-action".to_string(), "auto".to_string());
    defaults.insert("scroll-behavior".to_string(), "auto".to_string());
    defaults
}

// ─── Media Query Matching (Fas 19) ──────────────────────────────────────────

/// Parsa enkel CSS media query och matcha mot viewport
pub(super) fn parse_media_query_matches(
    query: &str,
    viewport_width: f64,
    viewport_height: f64,
) -> bool {
    let query = query.trim().to_lowercase();

    // Hantera "all", "screen", "(prefers-color-scheme: light)" etc
    if query == "all" || query == "screen" {
        return true;
    }
    if query == "print" {
        return false;
    }

    // Parsa bredd-queries: (min-width: 768px), (max-width: 1024px)
    if let Some(val) = extract_px_from_query(&query, "min-width") {
        return viewport_width >= val;
    }
    if let Some(val) = extract_px_from_query(&query, "max-width") {
        return viewport_width <= val;
    }
    if let Some(val) = extract_px_from_query(&query, "min-height") {
        return viewport_height >= val;
    }
    if let Some(val) = extract_px_from_query(&query, "max-height") {
        return viewport_height <= val;
    }

    // (prefers-color-scheme: light)
    if query.contains("prefers-color-scheme") {
        return query.contains("light");
    }

    // (prefers-reduced-motion: no-preference)
    if query.contains("prefers-reduced-motion") {
        return query.contains("no-preference");
    }

    // Okänd query — returnera true som default
    true
}

/// Extrahera px-värde från media query-uttryck
fn extract_px_from_query(query: &str, prop: &str) -> Option<f64> {
    let pos = query.find(prop)?;
    let rest = &query[pos + prop.len()..];
    let colon = rest.find(':')?;
    let after_colon = rest[colon + 1..].trim();
    // Hitta siffran innan "px" eller ")"
    let end = after_colon.find([')', 'p']).unwrap_or(after_colon.len());
    after_colon[..end].trim().parse::<f64>().ok()
}

// ─── Base64 encode/decode (Fas 19) ──────────────────────────────────────────

/// Enkel base64-avkodning (atob)
#[allow(dead_code)]
pub(super) fn base64_decode(input: &str) -> Option<String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input: Vec<u8> = input
        .bytes()
        .filter(|b| *b != b'\n' && *b != b'\r')
        .collect();
    let mut output = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in &input {
        if b == b'=' {
            break;
        }
        let val = CHARS.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    String::from_utf8(output).ok()
}

/// Enkel base64-kodning (btoa)
#[allow(dead_code)]
pub(super) fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() {
            bytes[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < bytes.len() {
            bytes[i + 2] as u32
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < bytes.len() {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if i + 2 < bytes.len() {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

// ─── URL-parsing (Fas 19) ────────────────────────────────────────────────────

/// Parsa URL i delar: (protocol, hostname, pathname, search, hash)
#[allow(dead_code)]
pub(super) fn parse_url_parts(url: &str) -> (String, String, String, String, String) {
    let (url_no_hash, hash) = match url.find('#') {
        Some(pos) => (&url[..pos], url[pos..].to_string()),
        None => (url, String::new()),
    };
    let (url_no_search, search) = match url_no_hash.find('?') {
        Some(pos) => (&url_no_hash[..pos], url_no_hash[pos..].to_string()),
        None => (url_no_hash, String::new()),
    };

    let (protocol, rest) = if let Some(pos) = url_no_search.find("://") {
        (
            format!("{}:", &url_no_search[..pos]),
            &url_no_search[pos + 3..],
        )
    } else {
        ("https:".to_string(), url_no_search)
    };

    let (hostname, pathname) = match rest.find('/') {
        Some(pos) => (rest[..pos].to_string(), rest[pos..].to_string()),
        None => (rest.to_string(), "/".to_string()),
    };

    (protocol, hostname, pathname, search, hash)
}

/// Parsa query string till nyckel-värde-par
#[allow(dead_code)]
fn parse_query_string(search: &str) -> Vec<(String, String)> {
    let s = search.strip_prefix('?').unwrap_or(search);
    if s.is_empty() {
        return vec![];
    }
    s.split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.to_string();
            let val = parts.next().unwrap_or("").to_string();
            Some((key, val))
        })
        .collect()
}
