// ─── XML Serializer ──────────────────────────────────────────────────────────
//
// Implementerar XMLSerializer.serializeToString() per DOM Parsing spec.
// Portad logik från jsdom XMLSerializer-impl.js + W3C serialization spec.
//
// Spec: https://w3c.github.io/DOM-Parsing/#dom-xmlserializer-serializetostring
//
// Skillnader mot HTML-serialisering:
// - Tomma element: <br/> (self-closing) istället för <br>
// - Namespace-deklarationer bevaras
// - Attributvärden escapas mer strikt
// - CDATA-sektioner stöds
// - Inga void element-specialfall

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};

/// Serialisera en nod till XML-sträng
pub(in crate::dom_bridge) fn serialize_to_xml(arena: &ArenaDom, key: NodeKey) -> String {
    let mut out = String::new();
    serialize_xml_node(arena, key, &mut out);
    out
}

fn serialize_xml_node(arena: &ArenaDom, key: NodeKey, out: &mut String) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };

    match &node.node_type {
        NodeType::Element => {
            let tag = node.tag.as_deref().unwrap_or("div");
            out.push('<');
            out.push_str(tag);

            // Attribut med XML-escaping
            for (k, v) in &node.attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                escape_xml_attr(v, out);
                out.push('"');
            }

            // Tomma element → self-closing
            if node.children.is_empty() {
                out.push_str("/>");
                return;
            }

            out.push('>');
            for &child in &node.children {
                serialize_xml_node(arena, child, out);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        NodeType::Text => {
            if let Some(text) = &node.text {
                escape_xml_text(text, out);
            }
        }
        NodeType::Comment => {
            if let Some(text) = &node.text {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
        }
        NodeType::ProcessingInstruction => {
            let target = node.tag.as_deref().unwrap_or("");
            let data = node.text.as_deref().unwrap_or("");
            out.push_str("<?");
            out.push_str(target);
            if !data.is_empty() {
                out.push(' ');
                out.push_str(data);
            }
            out.push_str("?>");
        }
        NodeType::Document => {
            // Serialisera alla barn
            for &child in &node.children {
                serialize_xml_node(arena, child, out);
            }
        }
        NodeType::DocumentFragment => {
            for &child in &node.children {
                serialize_xml_node(arena, child, out);
            }
        }
        NodeType::Doctype => {
            out.push_str("<!DOCTYPE ");
            out.push_str(node.text.as_deref().unwrap_or("html"));
            out.push('>');
        }
        NodeType::Other => {}
    }
}

/// Escape text content per XML spec
fn escape_xml_text(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
}

/// Escape attribute value per XML spec
fn escape_xml_attr(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            _ => out.push(ch),
        }
    }
}
