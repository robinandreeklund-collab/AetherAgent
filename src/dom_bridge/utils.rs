// ─── Hjälpfunktioner (oberoende av QuickJS) ──────────────────────────────────
//
// Utbruten från dom_bridge/mod.rs: style-parsing, base64, URL, media query, layout.

use crate::arena_dom::{ArenaDom, NodeKey};

// ─── Hjälpfunktioner (boa-oberoende) ──────────────────────────────────────

pub(super) fn parse_inline_styles(style_attr: &str) -> std::collections::HashMap<String, String> {
    let mut styles = std::collections::HashMap::new();
    for part in style_attr.split(';') {
        let part = part.trim();
        if let Some(colon_pos) = part.find(':') {
            let prop = part[..colon_pos].trim().to_lowercase();
            let val = part[colon_pos + 1..].trim().to_string();
            if !prop.is_empty() {
                styles.insert(prop, val);
            }
        }
    }
    styles
}

/// Serialisera inline CSS-stilar till style-attribut-sträng
pub(super) fn serialize_inline_styles(
    styles: &std::collections::HashMap<String, String>,
) -> String {
    let mut parts: Vec<String> = styles
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect();
    parts.sort();
    parts.join("; ")
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
