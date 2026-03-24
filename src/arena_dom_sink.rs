/// ArenaDomSink — Custom html5ever TreeSink som bygger ArenaDom direkt
///
/// Eliminerar RcDom-mellansteget: html5ever → ArenaDom i ett steg.
/// Sparar ~1.5ms per sida genom att undvika Rc-allokeringar och
/// en hel trädtraversering (from_rcdom).
use std::borrow::Cow;
use std::collections::HashMap;

use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, ExpandedName, QualName};

use crate::arena_dom::{ArenaDom, DomNode, NodeKey, NodeType};

/// Håller template-contents-mappning (NodeKey → fragment NodeKey)
pub struct ArenaDomSink {
    arena: ArenaDom,
    /// template-element → template-contents fragment
    template_contents: HashMap<NodeKey, NodeKey>,
    /// mathml annotation-xml integration point flaggor
    mathml_integration_points: HashMap<NodeKey, bool>,
    /// Taggnamn per nod — O(1) lookup via HashMap (elem_name anropas hundratals gånger)
    tag_names: HashMap<NodeKey, QualName>,
}

impl ArenaDomSink {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::with_estimated_capacity(1024)
    }

    pub fn with_estimated_capacity(estimated_nodes: usize) -> Self {
        ArenaDomSink {
            arena: ArenaDom::with_capacity(estimated_nodes),
            template_contents: HashMap::new(),
            mathml_integration_points: HashMap::new(),
            tag_names: HashMap::with_capacity(estimated_nodes / 2),
        }
    }

    /// Hjälpfunktion: hämta sista barnet till en nod
    fn last_child(&self, parent: &NodeKey) -> Option<NodeKey> {
        self.arena
            .nodes
            .get(*parent)
            .and_then(|n| n.children.last().copied())
    }

    /// Hjälpfunktion: försök addera text till sista syskon-textnod
    fn append_to_existing_text(&mut self, parent: &NodeKey, text: &str) -> bool {
        if let Some(last_key) = self.last_child(parent) {
            if let Some(node) = self.arena.nodes.get_mut(last_key) {
                if node.node_type == NodeType::Text {
                    if let Some(ref mut existing) = node.text {
                        existing.push_str(text);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Hjälpfunktion: skapa textnod och koppla till parent
    fn create_and_append_text(&mut self, parent: NodeKey, text: String) {
        let key = self.arena.nodes.insert(DomNode {
            node_type: NodeType::Text,
            tag: None,
            attributes: HashMap::default(),
            text: Some(text),
            parent: Some(parent),
            children: vec![],
        });
        if let Some(parent_node) = self.arena.nodes.get_mut(parent) {
            parent_node.children.push(key);
        }
    }

    /// Hjälpfunktion: koppla en barnnod till ny parent
    fn append_node(&mut self, parent: NodeKey, child: NodeKey) {
        // Sätt parent
        if let Some(child_node) = self.arena.nodes.get_mut(child) {
            child_node.parent = Some(parent);
        }
        // Lägg till i parents children
        if let Some(parent_node) = self.arena.nodes.get_mut(parent) {
            parent_node.children.push(child);
        }
    }

    /// Hjälpfunktion: ta bort nod från sin parent
    fn detach(&mut self, target: NodeKey) {
        let parent_key = self.arena.nodes.get(target).and_then(|n| n.parent);
        if let Some(pk) = parent_key {
            if let Some(parent) = self.arena.nodes.get_mut(pk) {
                parent.children.retain(|&k| k != target);
            }
            if let Some(node) = self.arena.nodes.get_mut(target) {
                node.parent = None;
            }
        }
    }
}

impl TreeSink for ArenaDomSink {
    type Handle = NodeKey;
    type Output = ArenaDom;

    fn finish(self) -> ArenaDom {
        self.arena
    }

    fn parse_error(&mut self, _msg: Cow<'static, str>) {
        // Ignorera parse-errors — AetherAgent hanterar trasig HTML graciöst
    }

    fn get_document(&mut self) -> NodeKey {
        self.arena.document
    }

    fn elem_name<'a>(&'a self, target: &'a NodeKey) -> ExpandedName<'a> {
        self.tag_names
            .get(target)
            .expect("elem_name called on non-element node")
            .expanded()
    }

    fn create_element(
        &mut self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> NodeKey {
        let tag = name.local.to_string();
        let mut attributes = HashMap::with_capacity(attrs.len());
        for attr in &attrs {
            attributes.insert(attr.name.local.to_string(), attr.value.to_string());
        }

        let key = self.arena.nodes.insert(DomNode {
            node_type: NodeType::Element,
            tag: Some(tag),
            attributes,
            text: None,
            parent: None,
            children: vec![],
        });

        // Spara QualName för elem_name()
        self.tag_names.insert(key, name);

        // Template-element: skapa document fragment
        if flags.template {
            let fragment = self.arena.nodes.insert(DomNode {
                node_type: NodeType::Document,
                tag: None,
                attributes: HashMap::default(),
                text: None,
                parent: None,
                children: vec![],
            });
            self.template_contents.insert(key, fragment);
        }

        // MathML integration point
        if flags.mathml_annotation_xml_integration_point {
            self.mathml_integration_points.insert(key, true);
        }

        key
    }

    fn create_comment(&mut self, text: StrTendril) -> NodeKey {
        self.arena.nodes.insert(DomNode {
            node_type: NodeType::Comment,
            tag: None,
            attributes: HashMap::default(),
            text: Some(text.to_string()),
            parent: None,
            children: vec![],
        })
    }

    fn create_pi(&mut self, target: StrTendril, data: StrTendril) -> NodeKey {
        self.arena.nodes.insert(DomNode {
            node_type: NodeType::Other,
            tag: Some(format!("?{}", target)),
            attributes: HashMap::default(),
            text: Some(data.to_string()),
            parent: None,
            children: vec![],
        })
    }

    fn append(&mut self, parent: &NodeKey, child: NodeOrText<NodeKey>) {
        match child {
            NodeOrText::AppendNode(child_key) => {
                self.append_node(*parent, child_key);
            }
            NodeOrText::AppendText(text) => {
                let text_str = text.to_string();
                // Försök merga med sista textnod (spec: adjacent text merging)
                if !self.append_to_existing_text(parent, &text_str) {
                    self.create_and_append_text(*parent, text_str);
                }
            }
        }
    }

    fn append_based_on_parent_node(
        &mut self,
        element: &NodeKey,
        prev_element: &NodeKey,
        child: NodeOrText<NodeKey>,
    ) {
        // Om element har en parent, använd den. Annars prev_element.
        let has_parent = self
            .arena
            .nodes
            .get(*element)
            .and_then(|n| n.parent)
            .is_some();

        if has_parent {
            self.append_before_sibling(element, child);
        } else {
            self.append(prev_element, child);
        }
    }

    fn append_doctype_to_document(
        &mut self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // DOCTYPE ignoreras — AetherAgent behöver inte doctype-info
    }

    fn get_template_contents(&mut self, target: &NodeKey) -> NodeKey {
        *self
            .template_contents
            .get(target)
            .expect("get_template_contents: inte ett template-element")
    }

    fn same_node(&self, x: &NodeKey, y: &NodeKey) -> bool {
        *x == *y
    }

    fn set_quirks_mode(&mut self, _mode: QuirksMode) {
        // Ignorera quirks mode — AetherAgent parsear semantiskt
    }

    fn append_before_sibling(&mut self, sibling: &NodeKey, new_node: NodeOrText<NodeKey>) {
        let parent_key = self.arena.nodes.get(*sibling).and_then(|n| n.parent);

        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return, // Inget parent = kan inte infoga
        };

        // Hitta siblingens index
        let sibling_idx = self
            .arena
            .nodes
            .get(parent_key)
            .map(|p| {
                p.children
                    .iter()
                    .position(|&k| k == *sibling)
                    .unwrap_or(p.children.len())
            })
            .unwrap_or(0);

        match new_node {
            NodeOrText::AppendNode(child_key) => {
                // Ta bort från gammal parent om den finns
                self.detach(child_key);
                // Sätt ny parent
                if let Some(child) = self.arena.nodes.get_mut(child_key) {
                    child.parent = Some(parent_key);
                }
                // Infoga före sibling
                if let Some(parent) = self.arena.nodes.get_mut(parent_key) {
                    parent.children.insert(sibling_idx, child_key);
                }
            }
            NodeOrText::AppendText(text) => {
                let text_str = text.to_string();
                // Försök merga med föregående syskon
                if sibling_idx > 0 {
                    if let Some(parent) = self.arena.nodes.get(parent_key) {
                        let prev_key = parent.children[sibling_idx - 1];
                        if let Some(prev_node) = self.arena.nodes.get_mut(prev_key) {
                            if prev_node.node_type == NodeType::Text {
                                if let Some(ref mut existing) = prev_node.text {
                                    existing.push_str(&text_str);
                                    return;
                                }
                            }
                        }
                    }
                }
                // Skapa ny textnod och infoga
                let text_key = self.arena.nodes.insert(DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: HashMap::default(),
                    text: Some(text_str),
                    parent: Some(parent_key),
                    children: vec![],
                });
                if let Some(parent) = self.arena.nodes.get_mut(parent_key) {
                    parent.children.insert(sibling_idx, text_key);
                }
            }
        }
    }

    fn add_attrs_if_missing(&mut self, target: &NodeKey, attrs: Vec<Attribute>) {
        if let Some(node) = self.arena.nodes.get_mut(*target) {
            for attr in attrs {
                let name = attr.name.local.to_string();
                if let std::collections::hash_map::Entry::Vacant(e) = node.attributes.entry(name) {
                    e.insert(attr.value.to_string());
                }
            }
        }
    }

    fn remove_from_parent(&mut self, target: &NodeKey) {
        self.detach(*target);
    }

    fn reparent_children(&mut self, node: &NodeKey, new_parent: &NodeKey) {
        // Samla children (klonar Vec för att undvika borrow-konflikt)
        let children = self
            .arena
            .nodes
            .get(*node)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        for child_key in &children {
            if let Some(child) = self.arena.nodes.get_mut(*child_key) {
                child.parent = Some(*new_parent);
            }
            if let Some(np) = self.arena.nodes.get_mut(*new_parent) {
                np.children.push(*child_key);
            }
        }

        if let Some(node) = self.arena.nodes.get_mut(*node) {
            node.children.clear();
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &NodeKey) -> bool {
        self.mathml_integration_points
            .get(handle)
            .copied()
            .unwrap_or(false)
    }
}

/// Parsa HTML direkt till ArenaDom — skippar RcDom-mellansteget
///
/// Använder `.one(StrTendril)` istället för `.from_utf8().read_from()`:
/// - Undviker 4KB chunking (hela strängen matas in direkt)
/// - Undviker UTF-8 lossy decoding (input är redan validerad &str)
/// - Undviker multipla tendril-allokeringar per chunk
pub fn parse_html_to_arena(html: &str) -> ArenaDom {
    let estimated_nodes = (html.len() / 60).max(256);
    let sink = ArenaDomSink::with_estimated_capacity(estimated_nodes);
    html5ever::parse_document(sink, Default::default()).one(StrTendril::from(html))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_html;

    #[test]
    fn test_basic_parse() {
        let html = r#"<html><head><title>Test</title></head><body><p>Hello</p></body></html>"#;
        let arena = parse_html_to_arena(html);
        // Ska ha document + html + head + title + text + body + p + text = 8+ noder
        assert!(
            arena.nodes.len() > 5,
            "Borde ha minst 5 noder, fick {}",
            arena.nodes.len()
        );
    }

    #[test]
    fn test_attributes_preserved() {
        let html = r#"<div class="test" id="main"><a href="/about">About</a></div>"#;
        let arena = parse_html_to_arena(html);
        // Hitta div-noden
        let div_key = arena
            .nodes
            .iter()
            .find(|(_, n)| n.tag.as_deref() == Some("div"))
            .map(|(k, _)| k);
        assert!(div_key.is_some(), "Borde hitta div-element");
        let div = &arena.nodes[div_key.unwrap()];
        assert_eq!(
            div.attributes.get("class").map(|s| s.as_str()),
            Some("test")
        );
        assert_eq!(div.attributes.get("id").map(|s| s.as_str()), Some("main"));
    }

    #[test]
    fn test_text_content() {
        let html = r#"<p>Hello World</p>"#;
        let arena = parse_html_to_arena(html);
        let text_node = arena.nodes.iter().find(|(_, n)| {
            n.node_type == NodeType::Text && n.text.as_deref() == Some("Hello World")
        });
        assert!(
            text_node.is_some(),
            "Borde hitta text-nod med 'Hello World'"
        );
    }

    #[test]
    fn test_parent_child_links() {
        let html = r#"<div><span>Text</span></div>"#;
        let arena = parse_html_to_arena(html);
        let span_key = arena
            .nodes
            .iter()
            .find(|(_, n)| n.tag.as_deref() == Some("span"))
            .map(|(k, _)| k);
        assert!(span_key.is_some(), "Borde hitta span");
        let span = &arena.nodes[span_key.unwrap()];
        assert!(span.parent.is_some(), "span borde ha parent");
        // Spans parent borde vara body (html5ever lägger div i body)
    }

    #[test]
    fn test_matches_rcdom_output() {
        let html = r##"<html><body>
            <h1>Title</h1>
            <p class="price">199 kr</p>
            <button id="buy">Köp</button>
            <a href="/about">Om oss</a>
        </body></html>"##;

        // Parsa via RcDom-vägen
        let rcdom = parse_html(html);
        let arena_old = ArenaDom::from_rcdom(&rcdom);

        // Parsa via ArenaDomSink
        let arena_new = parse_html_to_arena(html);

        // Jämför nodantal (borde vara ungefär lika)
        let old_elements: Vec<_> = arena_old
            .nodes
            .iter()
            .filter(|(_, n)| n.node_type == NodeType::Element)
            .collect();
        let new_elements: Vec<_> = arena_new
            .nodes
            .iter()
            .filter(|(_, n)| n.node_type == NodeType::Element)
            .collect();
        assert_eq!(
            old_elements.len(),
            new_elements.len(),
            "Borde ha samma antal element: old={}, new={}",
            old_elements.len(),
            new_elements.len()
        );

        // Jämför taggar
        let mut old_tags: Vec<_> = old_elements
            .iter()
            .filter_map(|(_, n)| n.tag.as_deref())
            .collect();
        let mut new_tags: Vec<_> = new_elements
            .iter()
            .filter_map(|(_, n)| n.tag.as_deref())
            .collect();
        old_tags.sort();
        new_tags.sort();
        assert_eq!(old_tags, new_tags, "Borde ha samma tag-uppsättning");
    }

    #[test]
    fn test_adjacent_text_merging() {
        // html5ever kan skapa adjacent text tokens som ska mergas
        let html = r#"<p>Hello World</p>"#;
        let arena = parse_html_to_arena(html);
        let p_key = arena
            .nodes
            .iter()
            .find(|(_, n)| n.tag.as_deref() == Some("p"))
            .map(|(k, _)| k)
            .expect("Borde hitta p-element");
        let p = &arena.nodes[p_key];
        // Ska ha exakt 1 barn (text-nod)
        let text_children: Vec<_> = p
            .children
            .iter()
            .filter(|&&k| {
                arena
                    .nodes
                    .get(k)
                    .map(|n| n.node_type == NodeType::Text)
                    .unwrap_or(false)
            })
            .collect();
        assert_eq!(
            text_children.len(),
            1,
            "Borde ha exakt 1 text-barn, fick {}",
            text_children.len()
        );
    }
}
