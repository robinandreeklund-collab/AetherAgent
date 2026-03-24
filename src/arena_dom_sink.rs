/// ArenaDomSink — Custom html5ever TreeSink som bygger ArenaDom direkt
///
/// Eliminerar RcDom-mellansteget: html5ever → ArenaDom i ett steg.
/// Sparar ~1.5ms per sida genom att undvika Rc-allokeringar och
/// en hel trädtraversering (from_rcdom).
///
/// html5ever 0.38+ kräver &self (inte &mut self) i TreeSink — vi använder
/// RefCell för interior mutability, precis som RcDom gör.
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;

use html5ever::tendril::{StrTendril, TendrilSink};
use html5ever::tree_builder::{ElementFlags, NodeOrText, QuirksMode, TreeSink};
use html5ever::{Attribute, ExpandedName, QualName};
use slotmap::SecondaryMap;

use crate::arena_dom::{ArenaDom, Attrs, DomNode, NodeKey, NodeType};

/// Håller template-contents-mappning (NodeKey → fragment NodeKey)
pub struct ArenaDomSink {
    arena: RefCell<ArenaDom>,
    /// template-element → template-contents fragment
    template_contents: RefCell<HashMap<NodeKey, NodeKey>>,
    /// mathml annotation-xml integration point flaggor
    mathml_integration_points: RefCell<HashMap<NodeKey, bool>>,
    /// Taggnamn per nod — O(1) array-indexerad lookup via SecondaryMap
    /// (elem_name anropas hundratals gånger, SecondaryMap undviker hashing)
    tag_names: RefCell<SecondaryMap<NodeKey, QualName>>,
    /// Dokumentnod — sparas vid konstruktion, kopieras inte via RefCell
    document: NodeKey,
}

impl Default for ArenaDomSink {
    fn default() -> Self {
        Self::new()
    }
}

impl ArenaDomSink {
    pub fn new() -> Self {
        Self::with_estimated_capacity(1024)
    }

    pub fn with_estimated_capacity(estimated_nodes: usize) -> Self {
        let arena = ArenaDom::with_capacity(estimated_nodes);
        let document = arena.document;
        ArenaDomSink {
            arena: RefCell::new(arena),
            template_contents: RefCell::new(HashMap::new()),
            mathml_integration_points: RefCell::new(HashMap::new()),
            tag_names: RefCell::new(SecondaryMap::with_capacity(estimated_nodes / 2)),
            document,
        }
    }

    /// Hjälpfunktion: hämta sista barnet till en nod
    fn last_child(&self, parent: &NodeKey) -> Option<NodeKey> {
        self.arena
            .borrow()
            .nodes
            .get(*parent)
            .and_then(|n| n.children.last().copied())
    }

    /// Kolla om ett element är "inert" (text-innehåll irrelevant semantiskt)
    #[inline]
    fn is_inert_tag(tag: &str) -> bool {
        matches!(tag, "script" | "style" | "noscript")
    }

    /// Kolla om text enbart innehåller whitespace (newlines, spaces, tabs).
    /// Dessa noder fyller ingen semantisk funktion och kan skippas.
    #[inline]
    fn is_whitespace_only(text: &str) -> bool {
        text.bytes()
            .all(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
    }

    /// Hjälpfunktion: försök addera text till sista syskon-textnod
    fn append_to_existing_text(&self, parent: &NodeKey, text: &StrTendril) -> bool {
        if let Some(last_key) = self.last_child(parent) {
            let mut arena = self.arena.borrow_mut();
            if let Some(node) = arena.nodes.get_mut(last_key) {
                if node.node_type == NodeType::Text {
                    if let Some(ref mut existing) = node.text {
                        existing.push_slice(text);
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Hjälpfunktion: skapa textnod och koppla till parent
    fn create_and_append_text(&self, parent: NodeKey, text: StrTendril) {
        let mut arena = self.arena.borrow_mut();
        let key = arena.nodes.insert(DomNode {
            node_type: NodeType::Text,
            tag: None,
            attributes: Attrs::new(),
            text: Some(text),
            parent: Some(parent),
            children: vec![],
        });
        if let Some(parent_node) = arena.nodes.get_mut(parent) {
            parent_node.children.push(key);
        }
    }

    /// Hjälpfunktion: koppla en barnnod till ny parent
    fn append_node(&self, parent: NodeKey, child: NodeKey) {
        let mut arena = self.arena.borrow_mut();
        // Sätt parent
        if let Some(child_node) = arena.nodes.get_mut(child) {
            child_node.parent = Some(parent);
        }
        // Lägg till i parents children
        if let Some(parent_node) = arena.nodes.get_mut(parent) {
            parent_node.children.push(child);
        }
    }

    /// Hjälpfunktion: ta bort nod från sin parent
    fn detach(&self, target: NodeKey) {
        let mut arena = self.arena.borrow_mut();
        let parent_key = arena.nodes.get(target).and_then(|n| n.parent);
        if let Some(pk) = parent_key {
            if let Some(parent) = arena.nodes.get_mut(pk) {
                parent.children.retain(|&k| k != target);
            }
            if let Some(node) = arena.nodes.get_mut(target) {
                node.parent = None;
            }
        }
    }
}

impl TreeSink for ArenaDomSink {
    type Handle = NodeKey;
    type Output = ArenaDom;

    type ElemName<'a> = ExpandedName<'a>;

    fn finish(self) -> ArenaDom {
        self.arena.into_inner()
    }

    fn parse_error(&self, _msg: Cow<'static, str>) {
        // Ignorera parse-errors — AetherAgent hanterar trasig HTML graciöst
    }

    fn get_document(&self) -> NodeKey {
        self.document
    }

    fn elem_name<'a>(&'a self, target: &'a NodeKey) -> ExpandedName<'a> {
        // SAFETY: tag_names borrow lever bara under denna metod,
        // och html5ever garanterar att elem_name inte anropas rekursivt.
        // Vi använder unsafe för att undvika RefCell::borrow() lifetime-problem
        // (ExpandedName refererar in i tag_names som annars släpps vid borrow-drop).
        let tag_names = self.tag_names.as_ptr();
        unsafe {
            (*tag_names)
                .get(*target)
                .expect("elem_name called on non-element node")
                .expanded()
        }
    }

    fn create_element(
        &self,
        name: QualName,
        attrs: Vec<Attribute>,
        flags: ElementFlags,
    ) -> NodeKey {
        let tag = name.local.to_string();
        let mut attributes = Attrs::with_capacity(attrs.len());
        for attr in &attrs {
            attributes.insert(attr.name.local.to_string(), attr.value.to_string());
        }

        let mut arena = self.arena.borrow_mut();
        let key = arena.nodes.insert(DomNode {
            node_type: NodeType::Element,
            tag: Some(tag),
            attributes,
            text: None,
            parent: None,
            children: vec![],
        });
        drop(arena);

        // Spara QualName för elem_name()
        self.tag_names.borrow_mut().insert(key, name);

        // Template-element: skapa document fragment
        if flags.template {
            let fragment = self.arena.borrow_mut().nodes.insert(DomNode {
                node_type: NodeType::Document,
                tag: None,
                attributes: Attrs::new(),
                text: None,
                parent: None,
                children: vec![],
            });
            self.template_contents.borrow_mut().insert(key, fragment);
        }

        // MathML integration point
        if flags.mathml_annotation_xml_integration_point {
            self.mathml_integration_points
                .borrow_mut()
                .insert(key, true);
        }

        key
    }

    fn create_comment(&self, text: StrTendril) -> NodeKey {
        // Bevara kommentarstext — behövs av CharacterData WPT-tester
        self.arena.borrow_mut().nodes.insert(DomNode {
            node_type: NodeType::Comment,
            tag: None,
            attributes: Attrs::new(),
            text: Some(text),
            parent: None,
            children: vec![],
        })
    }

    fn create_pi(&self, target: StrTendril, data: StrTendril) -> NodeKey {
        self.arena.borrow_mut().nodes.insert(DomNode {
            node_type: NodeType::Other,
            tag: Some(format!("?{}", target)),
            attributes: Attrs::new(),
            text: Some(data),
            parent: None,
            children: vec![],
        })
    }

    fn append(&self, parent: &NodeKey, child: NodeOrText<NodeKey>) {
        match child {
            NodeOrText::AppendNode(child_key) => {
                self.append_node(*parent, child_key);
            }
            NodeOrText::AppendText(text) => {
                // Skippa whitespace-only textnoder (41% av alla noder i typisk HTML)
                if Self::is_whitespace_only(&text) {
                    return;
                }
                // Skippa text i script/style/noscript — aldrig semantiskt relevant
                {
                    let arena = self.arena.borrow();
                    if let Some(parent_node) = arena.nodes.get(*parent) {
                        if let Some(ref tag) = parent_node.tag {
                            if Self::is_inert_tag(tag) {
                                return;
                            }
                        }
                    }
                }
                // Försök merga med sista textnod (spec: adjacent text merging)
                if !self.append_to_existing_text(parent, &text) {
                    self.create_and_append_text(*parent, text);
                }
            }
        }
    }

    fn append_based_on_parent_node(
        &self,
        element: &NodeKey,
        prev_element: &NodeKey,
        child: NodeOrText<NodeKey>,
    ) {
        // Om element har en parent, använd den. Annars prev_element.
        let has_parent = self
            .arena
            .borrow()
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
        &self,
        _name: StrTendril,
        _public_id: StrTendril,
        _system_id: StrTendril,
    ) {
        // DOCTYPE ignoreras — AetherAgent behöver inte doctype-info
    }

    fn get_template_contents(&self, target: &NodeKey) -> NodeKey {
        *self
            .template_contents
            .borrow()
            .get(target)
            .expect("get_template_contents: inte ett template-element")
    }

    fn same_node(&self, x: &NodeKey, y: &NodeKey) -> bool {
        *x == *y
    }

    fn set_quirks_mode(&self, _mode: QuirksMode) {
        // Ignorera quirks mode — AetherAgent parsear semantiskt
    }

    fn append_before_sibling(&self, sibling: &NodeKey, new_node: NodeOrText<NodeKey>) {
        let parent_key = self
            .arena
            .borrow()
            .nodes
            .get(*sibling)
            .and_then(|n| n.parent);

        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return, // Inget parent = kan inte infoga
        };

        // Hitta siblingens index
        let sibling_idx = self
            .arena
            .borrow()
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
                let mut arena = self.arena.borrow_mut();
                // Sätt ny parent
                if let Some(child) = arena.nodes.get_mut(child_key) {
                    child.parent = Some(parent_key);
                }
                // Infoga före sibling
                if let Some(parent) = arena.nodes.get_mut(parent_key) {
                    parent.children.insert(sibling_idx, child_key);
                }
            }
            NodeOrText::AppendText(text) => {
                // Skippa whitespace-only textnoder
                if Self::is_whitespace_only(&text) {
                    return;
                }
                // Skippa text i script/style/noscript
                {
                    let arena = self.arena.borrow();
                    if let Some(parent_node) = arena.nodes.get(parent_key) {
                        if let Some(ref tag) = parent_node.tag {
                            if Self::is_inert_tag(tag) {
                                return;
                            }
                        }
                    }
                }
                // Försök merga med föregående syskon
                if sibling_idx > 0 {
                    let mut arena = self.arena.borrow_mut();
                    if let Some(parent) = arena.nodes.get(parent_key) {
                        let prev_key = parent.children[sibling_idx - 1];
                        if let Some(prev_node) = arena.nodes.get_mut(prev_key) {
                            if prev_node.node_type == NodeType::Text {
                                if let Some(ref mut existing) = prev_node.text {
                                    existing.push_slice(&text);
                                    return;
                                }
                            }
                        }
                    }
                    drop(arena);
                }
                // Skapa ny textnod och infoga
                let mut arena = self.arena.borrow_mut();
                let text_key = arena.nodes.insert(DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: Attrs::new(),
                    text: Some(text),
                    parent: Some(parent_key),
                    children: vec![],
                });
                if let Some(parent) = arena.nodes.get_mut(parent_key) {
                    parent.children.insert(sibling_idx, text_key);
                }
            }
        }
    }

    fn add_attrs_if_missing(&self, target: &NodeKey, attrs: Vec<Attribute>) {
        let mut arena = self.arena.borrow_mut();
        if let Some(node) = arena.nodes.get_mut(*target) {
            for attr in attrs {
                node.attributes
                    .insert_if_vacant(attr.name.local.to_string(), attr.value.to_string());
            }
        }
    }

    fn remove_from_parent(&self, target: &NodeKey) {
        self.detach(*target);
    }

    fn reparent_children(&self, node: &NodeKey, new_parent: &NodeKey) {
        let mut arena = self.arena.borrow_mut();
        // Samla children (klonar Vec för att undvika borrow-konflikt)
        let children = arena
            .nodes
            .get(*node)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        for child_key in &children {
            if let Some(child) = arena.nodes.get_mut(*child_key) {
                child.parent = Some(*new_parent);
            }
            if let Some(np) = arena.nodes.get_mut(*new_parent) {
                np.children.push(*child_key);
            }
        }

        if let Some(node) = arena.nodes.get_mut(*node) {
            node.children.clear();
        }
    }

    fn is_mathml_annotation_xml_integration_point(&self, handle: &NodeKey) -> bool {
        self.mathml_integration_points
            .borrow()
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
