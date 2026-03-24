/// Arena DOM — SlotMap-baserad DOM-representation
///
/// Ersätter RcDom med cache-friendly, kontiguös minnesallokering.
/// Generational indices ger stale-reference safety utan Rc-overhead.
///
/// Prestanda: ~5-10x snabbare DFS, 1 allokering istället för ~1000/sida.
use smallvec::SmallVec;

/// Attribut-lagring optimerad för typiska DOM-element (0-4 attribut).
///
/// Använder SmallVec<4> istället för HashMap: inget heap-allokering för ≤4 attribut,
/// linjär sökning snabbare än hashing för <8 element, ~48 byte per nod sparad.
#[derive(Debug, Clone, Default)]
pub struct Attrs {
    inner: SmallVec<[(String, String); 4]>,
}

impl Attrs {
    #[inline]
    pub fn new() -> Self {
        Self {
            inner: SmallVec::new(),
        }
    }

    #[inline]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            inner: SmallVec::with_capacity(cap),
        }
    }

    #[inline]
    pub fn get(&self, name: &str) -> Option<&String> {
        self.inner.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    #[inline]
    pub fn contains_key(&self, name: &str) -> bool {
        self.inner.iter().any(|(k, _)| k == name)
    }

    /// Infoga eller uppdatera. Returnerar Option<String> (gamla värdet) som HashMap.
    pub fn insert(&mut self, name: String, value: String) -> Option<String> {
        for (k, v) in self.inner.iter_mut() {
            if *k == name {
                let old = std::mem::replace(v, value);
                return Some(old);
            }
        }
        self.inner.push((name, value));
        None
    }

    /// Infoga om nyckeln inte redan finns.
    pub fn insert_if_vacant(&mut self, name: String, value: String) {
        if !self.contains_key(&name) {
            self.inner.push((name, value));
        }
    }

    pub fn remove(&mut self, name: &str) -> Option<String> {
        if let Some(pos) = self.inner.iter().position(|(k, _)| k == name) {
            Some(self.inner.swap_remove(pos).1)
        } else {
            None
        }
    }

    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.inner.iter().map(|(k, _)| k)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &String)> {
        self.inner.iter().map(|(k, v)| (k, v))
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// Stöd för `for (k, v) in &attrs` iteration — matchar HashMap-semantik
impl<'a> IntoIterator for &'a Attrs {
    type Item = (&'a String, &'a String);
    type IntoIter = AttrsIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        AttrsIter {
            inner: self.inner.iter(),
        }
    }
}

pub struct AttrsIter<'a> {
    inner: std::slice::Iter<'a, (String, String)>,
}

impl<'a> Iterator for AttrsIter<'a> {
    type Item = (&'a String, &'a String);

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next().map(|(k, v)| (k, v))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.inner.size_hint()
    }
}

use markup5ever_rcdom::{Handle, NodeData};
use slotmap::{new_key_type, SlotMap};

new_key_type! {
    /// Nyckel till en nod i arena:n. Generational index ger stale-ref safety.
    pub struct NodeKey;
}

/// Case-insensitive substring-sökning utan allokering (ASCII only).
/// Undviker to_lowercase()/to_uppercase()-allokeringar i hot paths.
#[inline]
fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.len() > haystack.len() {
        return false;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

/// Snabb style-hidden-check utan allokering.
/// Normaliserar inte hela strängen — söker direkt med case-insensitive bytes.
fn is_style_hidden_fast(style: &str) -> bool {
    // Patterns att söka (redan lowercase, utan whitespace)
    const PATTERNS: &[&[u8]] = &[
        b"display:none",
        b"visibility:hidden",
        b"opacity:0",
        b"left:-9999",
        b"left:-10000",
        b"clip:rect(0",
    ];
    // En allokering: byte-normaliserad (lowercase + strip whitespace)
    let norm: Vec<u8> = style
        .bytes()
        .filter(|b| !b.is_ascii_whitespace())
        .map(|b| b.to_ascii_lowercase())
        .collect();
    for pat in PATTERNS {
        if norm.windows(pat.len()).any(|w| w == *pat) {
            return true;
        }
    }
    false
}

/// Nodtyp i DOM-trädet
#[derive(Debug, Clone, PartialEq)]
pub enum NodeType {
    Document,
    Element,
    Text,
    Comment,
    Other,
}

/// En nod i arena-allokerad DOM
#[derive(Debug, Clone)]
pub struct DomNode {
    pub node_type: NodeType,
    pub tag: Option<String>,
    pub attributes: Attrs,
    pub text: Option<String>,
    pub parent: Option<NodeKey>,
    pub children: Vec<NodeKey>,
}

impl DomNode {
    /// Hämta attributvärde — linjär sökning, snabbare än HashMap för ≤8 attribut
    #[inline]
    pub fn get_attr(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(|v| v.as_str())
    }

    /// Kolla om attribut finns (oavsett värde)
    #[inline]
    pub fn has_attr(&self, name: &str) -> bool {
        self.attributes.contains_key(name)
    }

    /// Sätt attribut. Används av DOM Bridge.
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn set_attr(&mut self, name: &str, value: &str) {
        self.attributes.insert(name.to_string(), value.to_string());
    }

    /// Ta bort attribut. Används av DOM Bridge.
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn remove_attr(&mut self, name: &str) -> bool {
        self.attributes.remove(name).is_some()
    }
}

/// Arena-allokerad DOM. Alla noder lagras i en kontiguös SlotMap.
#[derive(Debug, Clone)]
pub struct ArenaDom {
    pub nodes: SlotMap<NodeKey, DomNode>,
    pub document: NodeKey,
}

impl ArenaDom {
    /// Skapa en ny tom ArenaDom med pre-allokerad kapacitet
    pub fn with_capacity(capacity: usize) -> Self {
        let mut nodes = SlotMap::with_capacity_and_key(capacity);
        let doc_key = nodes.insert(DomNode {
            node_type: NodeType::Document,
            tag: None,
            attributes: Attrs::new(),
            text: None,
            parent: None,
            children: vec![],
        });
        ArenaDom {
            nodes,
            document: doc_key,
        }
    }

    /// Konvertera från html5ever RcDom till ArenaDom (används av dom_bridge, css_cascade, etc.)
    #[allow(dead_code)]
    pub fn from_rcdom(rcdom: &markup5ever_rcdom::RcDom) -> Self {
        let mut arena = ArenaDom::with_capacity(1024);
        let doc_key = arena.document;

        // Traversera RcDom rekursivt och bygg upp arena
        for child in rcdom.document.children.borrow().iter() {
            let child_key = arena.convert_handle(child);
            arena.nodes[doc_key].children.push(child_key);
            arena.nodes[child_key].parent = Some(doc_key);
        }

        arena
    }

    /// Rekursiv konvertering av en Handle till arena-noder
    #[allow(dead_code)]
    fn convert_handle(&mut self, handle: &Handle) -> NodeKey {
        let (node_type, tag, text, attrs) = match &handle.data {
            NodeData::Document => (NodeType::Document, None, None, Attrs::new()),
            NodeData::Element { name, attrs, .. } => {
                let tag = name.local.to_string();
                let raw_attrs = attrs.borrow();
                let mut attributes = Attrs::with_capacity(raw_attrs.len());
                for a in raw_attrs.iter() {
                    attributes.insert(a.name.local.to_string(), a.value.to_string());
                }
                (NodeType::Element, Some(tag), None, attributes)
            }
            NodeData::Text { contents } => {
                let t = contents.borrow().to_string();
                (NodeType::Text, None, Some(t), Attrs::new())
            }
            NodeData::Comment { contents } => {
                let t = contents.to_string();
                (NodeType::Comment, None, Some(t), Attrs::new())
            }
            _ => (NodeType::Other, None, None, Attrs::new()),
        };

        let key = self.nodes.insert(DomNode {
            node_type,
            tag,
            attributes: attrs,
            text,
            parent: None,
            children: vec![],
        });

        // Konvertera barn rekursivt
        for child in handle.children.borrow().iter() {
            let child_key = self.convert_handle(child);
            self.nodes[key].children.push(child_key);
            self.nodes[child_key].parent = Some(key);
        }

        key
    }

    // ─── Accessor-metoder (speglar parser.rs funktioner) ────────────────────

    /// Hämta taggnamn för en nod
    #[inline]
    pub fn tag_name(&self, key: NodeKey) -> Option<&str> {
        self.nodes.get(key)?.tag.as_deref()
    }

    /// Hämta attributvärde
    #[inline]
    pub fn get_attr(&self, key: NodeKey, attr_name: &str) -> Option<&str> {
        self.nodes.get(key)?.get_attr(attr_name)
    }

    /// Kolla om noden har ett attribut
    #[inline]
    pub fn has_attr(&self, key: NodeKey, attr_name: &str) -> bool {
        self.nodes
            .get(key)
            .map(|n| n.has_attr(attr_name))
            .unwrap_or(false)
    }

    /// Sätt attribut på en nod — O(n) linjär sökning, snabb för ≤8 attribut.
    /// Används av DOM Bridge (js-eval) för setAttribute-anrop.
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn set_attr(&mut self, key: NodeKey, attr_name: &str, value: &str) {
        if let Some(node) = self.nodes.get_mut(key) {
            node.set_attr(attr_name, value);
        }
    }

    /// Ta bort attribut från en nod — O(n) linjär sökning, snabb för ≤8 attribut.
    /// Används av DOM Bridge (js-eval) för removeAttribute-anrop.
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn remove_attr(&mut self, key: NodeKey, attr_name: &str) -> bool {
        self.nodes
            .get_mut(key)
            .map(|n| n.remove_attr(attr_name))
            .unwrap_or(false)
    }

    /// Lägg till barn-nod. Uppdaterar parent-referens.
    #[cfg(any(feature = "js-eval", test))]
    pub fn append_child(&mut self, parent: NodeKey, child: NodeKey) {
        // Ta bort från tidigare förälder
        if let Some(old_parent) = self.nodes.get(child).and_then(|n| n.parent) {
            if let Some(old_p) = self.nodes.get_mut(old_parent) {
                old_p.children.retain(|c| *c != child);
            }
        }
        // Lägg till som barn
        if let Some(p) = self.nodes.get_mut(parent) {
            p.children.push(child);
        }
        if let Some(c) = self.nodes.get_mut(child) {
            c.parent = Some(parent);
        }
    }

    /// Ta bort barn-nod. Returnerar true om barnet hittades och togs bort.
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn remove_child(&mut self, parent: NodeKey, child: NodeKey) -> bool {
        let found = self
            .nodes
            .get(parent)
            .map(|n| n.children.contains(&child))
            .unwrap_or(false);
        if found {
            if let Some(p) = self.nodes.get_mut(parent) {
                p.children.retain(|c| *c != child);
            }
            if let Some(c) = self.nodes.get_mut(child) {
                c.parent = None;
            }
        }
        found
    }

    /// Infoga barn-nod före en referensnod. Om ref_child är None → appendChild.
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn insert_before(
        &mut self,
        parent: NodeKey,
        new_child: NodeKey,
        ref_child: Option<NodeKey>,
    ) {
        // Ta bort från tidigare förälder
        if let Some(old_parent) = self.nodes.get(new_child).and_then(|n| n.parent) {
            if let Some(old_p) = self.nodes.get_mut(old_parent) {
                old_p.children.retain(|c| *c != new_child);
            }
        }
        // Infoga på rätt position
        if let Some(p) = self.nodes.get_mut(parent) {
            match ref_child {
                Some(ref_key) => {
                    let pos = p
                        .children
                        .iter()
                        .position(|&c| c == ref_key)
                        .unwrap_or(p.children.len());
                    p.children.insert(pos, new_child);
                }
                None => p.children.push(new_child),
            }
        }
        if let Some(c) = self.nodes.get_mut(new_child) {
            c.parent = Some(parent);
        }
    }

    /// Djup-klona en nod och alla dess barn. Returnerar nyckeln till klonen.
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn clone_node_deep(&mut self, key: NodeKey) -> Option<NodeKey> {
        let node = self.nodes.get(key)?.clone();
        let children_to_clone: Vec<NodeKey> = node.children.clone();
        let clone_key = self.nodes.insert(DomNode {
            node_type: node.node_type,
            tag: node.tag,
            attributes: node.attributes,
            text: node.text,
            parent: None,
            children: vec![],
        });
        // Rekursivt klona barn
        for child in children_to_clone {
            if let Some(child_clone) = self.clone_node_deep(child) {
                self.append_child(clone_key, child_clone);
            }
        }
        Some(clone_key)
    }

    /// Serialisera en nod till HTML-sträng (outerHTML)
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn serialize_html(&self, key: NodeKey) -> String {
        let node = match self.nodes.get(key) {
            Some(n) => n,
            None => return String::new(),
        };
        match &node.node_type {
            NodeType::Text => node.text.clone().unwrap_or_default(),
            NodeType::Comment => format!("<!--{}-->", node.text.as_deref().unwrap_or("")),
            NodeType::Element => {
                let tag = node.tag.as_deref().unwrap_or("div");
                let mut attrs = String::new();
                // Sortera attribut för deterministisk output
                let mut attr_pairs: Vec<(&str, &str)> = node
                    .attributes
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect();
                attr_pairs.sort_by_key(|(k, _)| *k);
                for (k, v) in &attr_pairs {
                    attrs.push_str(&format!(" {}=\"{}\"", k, v));
                }
                let children_html: String = node
                    .children
                    .iter()
                    .map(|&c| self.serialize_html(c))
                    .collect();
                // Void elements
                if matches!(
                    tag,
                    "br" | "hr"
                        | "img"
                        | "input"
                        | "meta"
                        | "link"
                        | "area"
                        | "base"
                        | "col"
                        | "embed"
                        | "source"
                        | "track"
                        | "wbr"
                ) {
                    format!("<{}{} />", tag, attrs)
                } else {
                    format!("<{}{}>{}</{}>", tag, attrs, children_html, tag)
                }
            }
            _ => String::new(),
        }
    }

    /// Serialisera barn av en nod till HTML (innerHTML)
    #[cfg(any(feature = "js-eval", test))]
    #[allow(dead_code)]
    pub fn serialize_inner_html(&self, key: NodeKey) -> String {
        let node = match self.nodes.get(key) {
            Some(n) => n,
            None => return String::new(),
        };
        node.children
            .iter()
            .map(|&c| self.serialize_html(c))
            .collect()
    }

    /// Hämta barn-nycklar
    #[inline]
    pub fn children(&self, key: NodeKey) -> &[NodeKey] {
        self.nodes
            .get(key)
            .map(|n| n.children.as_slice())
            .unwrap_or(&[])
    }

    /// Kontrollera om elementet sannolikt är synligt (speglar parser::is_likely_visible)
    ///
    /// Detekterar: display:none, visibility:hidden, opacity:0, aria-hidden="true",
    /// HTML hidden-attribut, off-screen positioning.
    pub fn is_likely_visible(&self, key: NodeKey) -> bool {
        let node = match self.nodes.get(key) {
            Some(n) => n,
            None => return false,
        };

        // Kolla style-attribut för osynlighet — utan allokering
        if let Some(style) = node.get_attr("style") {
            if is_style_hidden_fast(style) {
                return false;
            }
        }

        // Kolla hidden-attribut
        if node.has_attr("hidden") {
            return false;
        }

        // aria-hidden="true" — semantiskt dold
        if let Some(aria) = node.get_attr("aria-hidden") {
            if aria.trim().eq_ignore_ascii_case("true") {
                return false;
            }
        }

        // nojs-divvar: id="nojs" eller class="no-js" — utan to_lowercase()
        if let Some(id) = node.get_attr("id") {
            if Self::NOJS_IDENTIFIERS
                .iter()
                .any(|pat| id.eq_ignore_ascii_case(pat))
            {
                return false;
            }
        }
        if let Some(class) = node.get_attr("class") {
            if Self::NOJS_IDENTIFIERS.iter().any(|pat| {
                class
                    .split_whitespace()
                    .any(|c| c.eq_ignore_ascii_case(pat))
            }) {
                return false;
            }
        }

        true
    }

    /// Extrahera all text rekursivt (speglar parser::extract_text)
    pub fn extract_text(&self, key: NodeKey) -> String {
        let mut buf = String::new();
        self.extract_text_into(key, &mut buf);
        buf
    }

    /// Samla text i en delad buffer — zero-alloc per rekursiv nivå
    fn extract_text_into(&self, key: NodeKey, buf: &mut String) {
        const TEXT_SKIP_TAGS: &[&str] = &["script", "style", "noscript", "template"];

        let node = match self.nodes.get(key) {
            Some(n) => n,
            None => return,
        };

        match &node.node_type {
            NodeType::Text => {
                let t = node.text.as_deref().unwrap_or("");
                let trimmed = t.trim();
                if !trimmed.is_empty() {
                    buf.push_str(trimmed);
                    buf.push(' ');
                }
            }
            NodeType::Element => {
                if let Some(tag) = &node.tag {
                    if TEXT_SKIP_TAGS.contains(&tag.as_str()) {
                        return;
                    }
                }
                // Iterera med index — undviker Vec::clone() per nod
                let num_children = node.children.len();
                for i in 0..num_children {
                    // Hämta barn-nyckel via index (säkert: i < num_children)
                    let child_key = self.nodes.get(key).and_then(|n| n.children.get(i).copied());
                    if let Some(ck) = child_key {
                        self.extract_text_into(ck, buf);
                    }
                }
            }
            _ => {
                let num_children = node.children.len();
                for i in 0..num_children {
                    let child_key = self.nodes.get(key).and_then(|n| n.children.get(i).copied());
                    if let Some(ck) = child_key {
                        self.extract_text_into(ck, buf);
                    }
                }
            }
        }
    }

    /// ID/klass-mönster som indikerar "JavaScript krävs"-meddelanden
    const NOJS_IDENTIFIERS: &'static [&'static str] =
        &["nojs", "no-js", "noscript-warning", "js-disabled"];

    /// CTA-nyckelord
    const CTA_KEYWORDS: &'static [&'static str] = &[
        "add to cart",
        "buy now",
        "purchase",
        "checkout",
        "sign up",
        "subscribe",
        "get started",
        "download",
        "try free",
        "order now",
        "köp",
        "lägg i varukorg",
        "handla",
        "beställ",
        "registrera",
        "prenumerera",
        "ladda ner",
        "kom igång",
    ];

    /// Valutaindikatorer
    const PRICE_INDICATORS: &'static [&'static str] = &[
        "$", "€", "£", "¥", "₹", "kr", "SEK", "NOK", "DKK", "USD", "EUR", "GBP",
    ];

    /// Inferera semantisk roll (speglar parser::infer_role)
    /// Rolldetektering med pre-extraherad text (undviker dubbla extract_text)
    pub fn infer_role_with_text(&self, key: NodeKey, precomputed_text: &str) -> String {
        // ARIA-roll har högst prioritet
        if let Some(role) = self.get_attr(key, "role") {
            if !role.is_empty() {
                return role.to_string();
            }
        }

        let tag = self.tag_name(key).unwrap_or("");
        let input_type_raw = self.get_attr(key, "type").unwrap_or("");

        let base_role = match tag {
            "button" => "button",
            "a" => "link",
            "input" => {
                if input_type_raw.eq_ignore_ascii_case("checkbox") {
                    "checkbox"
                } else if input_type_raw.eq_ignore_ascii_case("radio") {
                    "radio"
                } else if input_type_raw.eq_ignore_ascii_case("submit")
                    || input_type_raw.eq_ignore_ascii_case("button")
                    || input_type_raw.eq_ignore_ascii_case("reset")
                {
                    "button"
                } else if input_type_raw.eq_ignore_ascii_case("search") {
                    "searchbox"
                } else {
                    "textbox"
                }
            }
            "textarea" => "textarea",
            "select" => "combobox",
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => "heading",
            "img" => "img",
            "nav" => "navigation",
            "main" => "main",
            "header" => "banner",
            "footer" => "contentinfo",
            "form" => "form",
            "table" => "table",
            "li" => "listitem",
            "ul" | "ol" => "list",
            "p" | "span" | "div" => "text",
            _ => "generic",
        };

        // Schema.org product_card
        if let Some(itemtype) = self.get_attr(key, "itemtype") {
            let it = itemtype;
            if contains_ignore_ascii_case(it, "schema.org/product")
                || contains_ignore_ascii_case(it, "schema.org/offer")
            {
                return "product_card".to_string();
            }
        }
        if self.has_attr(key, "data-product-id")
            || self.has_attr(key, "data-product")
            || self.has_attr(key, "data-item-id")
        {
            let class = self.get_attr(key, "class").unwrap_or("");
            if contains_ignore_ascii_case(class, "product")
                || contains_ignore_ascii_case(class, "card")
                || contains_ignore_ascii_case(class, "item")
            {
                return "product_card".to_string();
            }
        }

        // CTA-heuristik — använd pre-extraherad text
        let class_raw = self.get_attr(key, "class").unwrap_or("");
        if base_role == "button" || base_role == "link" {
            let text_lower = precomputed_text.to_ascii_lowercase();
            for kw in Self::CTA_KEYWORDS {
                if text_lower.contains(kw) {
                    return "cta".to_string();
                }
            }
            let class_lower = class_raw.to_ascii_lowercase();
            if class_lower.contains("cta")
                || class_lower.contains("add-to-cart")
                || class_lower.contains("buy-btn")
                || class_lower.contains("checkout")
            {
                return "cta".to_string();
            }
        }

        // Pristext-heuristik
        if matches!(base_role, "text" | "generic")
            && self.looks_like_price_from_text(precomputed_text, class_raw, key)
        {
            return "price".to_string();
        }

        base_role.to_string()
    }

    /// Pris-check med pre-extraherad text (undviker dubbla extract_text)
    fn looks_like_price_from_text(&self, text: &str, class_raw: &str, key: NodeKey) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.len() > 40 {
            return false;
        }
        if !trimmed.chars().any(|c| c.is_ascii_digit()) {
            return false;
        }
        for indicator in Self::PRICE_INDICATORS {
            if contains_ignore_ascii_case(trimmed, indicator) {
                return true;
            }
        }
        if contains_ignore_ascii_case(class_raw, "price")
            || contains_ignore_ascii_case(class_raw, "pris")
            || contains_ignore_ascii_case(class_raw, "cost")
        {
            return true;
        }
        if let Some(itemprop) = self.get_attr(key, "itemprop") {
            if contains_ignore_ascii_case(itemprop, "price") {
                return true;
            }
        }
        false
    }

    /// Rolldetektering (convenience wrapper som extraherar text internt, används i tester)
    #[cfg(test)]
    pub fn infer_role(&self, key: NodeKey) -> String {
        let text = self.extract_text(key);
        self.infer_role_with_text(key, &text)
    }

    /// Extrahera label (WCAG fallback-kedja, speglar parser::extract_label)
    /// Label-extraktion med pre-extraherad text (undviker dubbla extract_text)
    pub fn extract_label_with_text(&self, key: NodeKey, precomputed_text: &str) -> String {
        // 1. aria-label
        if let Some(label) = self.get_attr(key, "aria-label") {
            let trimmed = label.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // 2. aria-labelledby
        if let Some(labelledby) = self.get_attr(key, "aria-labelledby") {
            let trimmed = labelledby.trim();
            if !trimmed.is_empty() {
                return format!("[ref:{}]", trimmed);
            }
        }
        // 3. placeholder
        if let Some(placeholder) = self.get_attr(key, "placeholder") {
            let trimmed = placeholder.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // 4. alt-text
        if let Some(alt) = self.get_attr(key, "alt") {
            let trimmed = alt.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // 5. title-attribut
        if let Some(title) = self.get_attr(key, "title") {
            let trimmed = title.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        // 6. Inre text (pre-extraherad)
        let trimmed = precomputed_text.trim();
        if !trimmed.is_empty() {
            let truncated: String = trimmed.chars().take(80).collect();
            return truncated;
        }
        // 7. name-attribut
        if let Some(name) = self.get_attr(key, "name") {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        String::new()
    }

    /// Label-extraktion (convenience wrapper som extraherar text internt, används i tester)
    #[cfg(test)]
    pub fn extract_label(&self, key: NodeKey) -> String {
        let text = self.extract_text(key);
        self.extract_label_with_text(key, &text)
    }

    /// Extrahera sidtitel (speglar semantic::extract_title)
    pub fn extract_title(&self) -> String {
        self.find_title_recursive(self.document)
    }

    fn find_title_recursive(&self, key: NodeKey) -> String {
        let node = match self.nodes.get(key) {
            Some(n) => n,
            None => return String::new(),
        };

        if node.tag.as_deref() == Some("title") {
            let child_keys: Vec<NodeKey> = node.children.clone();
            let text: String = child_keys
                .iter()
                .filter_map(|ck| {
                    let child = self.nodes.get(*ck)?;
                    if child.node_type == NodeType::Text {
                        child.text.clone()
                    } else {
                        None
                    }
                })
                .collect();
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }

        let child_keys: Vec<NodeKey> = node.children.clone();
        for ck in &child_keys {
            let title = self.find_title_recursive(*ck);
            if !title.is_empty() {
                return title;
            }
        }

        String::new()
    }
    /// Lazy-load attribut som kan innehålla den riktiga bild-URL:en
    const LAZY_SRC_ATTRS: &'static [&'static str] = &[
        "data-src",
        "data-lazy-src",
        "data-original",
        "data-image",
        "data-thumb",
        "data-thumbnail",
    ];

    /// Resolva lazy-loaded bilder: kopiera data-src → src på <img> utan riktig src.
    ///
    /// Många sajter använder lazy-loading (data-src, IntersectionObserver).
    /// Denna metod genomsöker hela DOM och promoterar lazy-attribut till src
    /// så att Blitz-rendering och semantisk extraktion ser bilderna.
    pub fn resolve_lazy_images(&mut self) {
        let keys: Vec<NodeKey> = self.nodes.keys().collect();
        for key in keys {
            let is_img = self
                .nodes
                .get(key)
                .map(|n| n.tag.as_deref() == Some("img"))
                .unwrap_or(false);
            if !is_img {
                continue;
            }

            // Kolla om src redan är riktig (inte placeholder)
            let has_real_src = self
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("src"))
                .map(|src| {
                    let t = src.trim();
                    !t.is_empty() && !t.starts_with("data:")
                })
                .unwrap_or(false);

            if has_real_src {
                continue;
            }

            // Sök i lazy-attribut
            let mut lazy_src = None;
            if let Some(node) = self.nodes.get(key) {
                for attr_name in Self::LAZY_SRC_ATTRS {
                    if let Some(val) = node.get_attr(attr_name) {
                        let trimmed = val.trim();
                        if !trimmed.is_empty() {
                            lazy_src = Some(trimmed.to_string());
                            break;
                        }
                    }
                }
            }

            // Promotera lazy-src → src (direkt via HashMap, utan set_attr som kräver js-eval)
            if let Some(src) = lazy_src {
                if let Some(node) = self.nodes.get_mut(key) {
                    node.attributes.insert("src".to_string(), src);
                }
            }
        }
    }
}

// ─── Tester ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_html;

    fn make_arena(html: &str) -> ArenaDom {
        let rcdom = parse_html(html);
        ArenaDom::from_rcdom(&rcdom)
    }

    /// Hitta första element med given tagg
    fn find_by_tag<'a>(arena: &'a ArenaDom, key: NodeKey, tag: &str) -> Option<NodeKey> {
        if arena.tag_name(key) == Some(tag) {
            return Some(key);
        }
        for &child in arena.children(key) {
            if let Some(found) = find_by_tag(arena, child, tag) {
                return Some(found);
            }
        }
        None
    }

    // === Konvertering ===

    #[test]
    fn test_from_rcdom_basic() {
        let arena = make_arena(r#"<html><body><button>Klicka</button></body></html>"#);
        assert!(
            arena.nodes.len() > 1,
            "Arena borde ha noder efter konvertering"
        );
    }

    #[test]
    fn test_from_rcdom_preserves_structure() {
        let arena = make_arena(r#"<html><body><div><p>Text</p></div></body></html>"#);
        let p = find_by_tag(&arena, arena.document, "p");
        assert!(p.is_some(), "Borde hitta <p> i arena");
    }

    #[test]
    fn test_from_rcdom_preserves_attributes() {
        let arena =
            make_arena(r##"<html><body><a href="#top" class="nav">Länk</a></body></html>"##);
        let a = find_by_tag(&arena, arena.document, "a").expect("Borde hitta <a>");
        assert_eq!(
            arena.get_attr(a, "href"),
            Some("#top"),
            "href borde bevaras"
        );
        assert_eq!(
            arena.get_attr(a, "class"),
            Some("nav"),
            "class borde bevaras"
        );
    }

    #[test]
    fn test_set_attr_and_remove_attr() {
        let mut arena =
            make_arena(r##"<html><body><div id="target" class="old">X</div></body></html>"##);
        let div = find_by_tag(&arena, arena.document, "div").expect("Borde hitta <div>");

        // set_attr — nytt attribut
        arena.nodes[div].set_attr("data-x", "123");
        assert_eq!(
            arena.get_attr(div, "data-x"),
            Some("123"),
            "Nytt attribut borde sättas"
        );

        // set_attr — uppdatera existerande
        arena.nodes[div].set_attr("class", "new");
        assert_eq!(
            arena.get_attr(div, "class"),
            Some("new"),
            "Existerande attribut borde uppdateras"
        );

        // remove_attr
        let removed = arena.nodes[div].remove_attr("data-x");
        assert!(removed, "remove_attr borde returnera true");
        assert_eq!(
            arena.get_attr(div, "data-x"),
            None,
            "Borttaget attribut borde vara None"
        );

        // remove_attr — obefintligt
        let removed = arena.nodes[div].remove_attr("nonexistent");
        assert!(!removed, "remove_attr på obefintligt borde returnera false");
    }

    #[test]
    fn test_from_rcdom_preserves_text() {
        let arena = make_arena(r#"<html><body><p>Hej värld</p></body></html>"#);
        let p = find_by_tag(&arena, arena.document, "p").expect("Borde hitta <p>");
        let text = arena.extract_text(p);
        assert!(
            text.contains("Hej värld"),
            "Text borde bevaras: got '{}'",
            text
        );
    }

    #[test]
    fn test_parent_links() {
        let arena = make_arena(r#"<html><body><div><p>X</p></div></body></html>"#);
        let p = find_by_tag(&arena, arena.document, "p").expect("Borde hitta <p>");
        let parent = arena.nodes[p].parent.expect("p borde ha parent");
        assert_eq!(
            arena.tag_name(parent),
            Some("div"),
            "Förälder borde vara <div>"
        );
    }

    #[test]
    fn test_multibyte_text() {
        let arena = make_arena(r#"<html><body><p>日本語 🎯 Ñoño</p></body></html>"#);
        let p = find_by_tag(&arena, arena.document, "p").expect("Borde hitta <p>");
        let text = arena.extract_text(p);
        assert!(text.contains("日本語"), "Japansk text borde bevaras");
        assert!(text.contains("🎯"), "Emoji borde bevaras");
    }

    // === Synlighet ===

    #[test]
    fn test_visible_normal() {
        let arena = make_arena(r#"<html><body><button>Synlig</button></body></html>"#);
        let btn = find_by_tag(&arena, arena.document, "button").expect("Borde hitta <button>");
        assert!(
            arena.is_likely_visible(btn),
            "Normal button borde vara synlig"
        );
    }

    #[test]
    fn test_hidden_display_none() {
        let arena = make_arena(r#"<html><body><div style="display:none">Dold</div></body></html>"#);
        let div = find_by_tag(&arena, arena.document, "div").expect("Borde hitta <div>");
        assert!(
            !arena.is_likely_visible(div),
            "display:none borde dölja element"
        );
    }

    #[test]
    fn test_hidden_attribute() {
        let arena = make_arena(r#"<html><body><div hidden>Dold</div></body></html>"#);
        let div = find_by_tag(&arena, arena.document, "div").expect("Borde hitta <div>");
        assert!(
            !arena.is_likely_visible(div),
            "hidden-attribut borde dölja element"
        );
    }

    // === Roll-inferens ===

    #[test]
    fn test_infer_role_button() {
        let arena = make_arena(r#"<html><body><button>Klick</button></body></html>"#);
        let btn = find_by_tag(&arena, arena.document, "button").expect("Borde hitta <button>");
        assert_eq!(arena.infer_role(btn), "button", "button ska ge 'button'");
    }

    #[test]
    fn test_infer_role_aria() {
        let arena = make_arena(r#"<html><body><div role="navigation">Nav</div></body></html>"#);
        let div = find_by_tag(&arena, arena.document, "div").expect("Borde hitta <div>");
        assert_eq!(
            arena.infer_role(div),
            "navigation",
            "ARIA role borde ha prioritet"
        );
    }

    #[test]
    fn test_infer_role_cta() {
        let arena = make_arena(r#"<html><body><button>Köp nu</button></body></html>"#);
        let btn = find_by_tag(&arena, arena.document, "button").expect("Borde hitta <button>");
        assert_eq!(arena.infer_role(btn), "cta", "CTA-knapp borde detekteras");
    }

    #[test]
    fn test_infer_role_price() {
        let arena = make_arena(r#"<html><body><span>199 kr</span></body></html>"#);
        let span = find_by_tag(&arena, arena.document, "span").expect("Borde hitta <span>");
        assert_eq!(arena.infer_role(span), "price", "Pris borde detekteras");
    }

    #[test]
    fn test_infer_role_headings() {
        for level in 1..=6 {
            let tag = format!("h{}", level);
            let html = format!(r#"<html><body><{}>Rubrik</{}></body></html>"#, tag, tag);
            let arena = make_arena(&html);
            let h = find_by_tag(&arena, arena.document, &tag)
                .unwrap_or_else(|| panic!("Borde hitta <{}>", tag));
            assert_eq!(
                arena.infer_role(h),
                "heading",
                "<{}> borde ge 'heading'",
                tag
            );
        }
    }

    // === Label-extraktion ===

    #[test]
    fn test_extract_label_aria() {
        let arena =
            make_arena(r#"<html><body><button aria-label="Stäng dialog">X</button></body></html>"#);
        let btn = find_by_tag(&arena, arena.document, "button").expect("Borde hitta <button>");
        assert_eq!(
            arena.extract_label(btn),
            "Stäng dialog",
            "aria-label borde ha prioritet"
        );
    }

    #[test]
    fn test_extract_label_placeholder() {
        let arena = make_arena(r#"<html><body><input placeholder="Sök..." /></body></html>"#);
        let input = find_by_tag(&arena, arena.document, "input").expect("Borde hitta <input>");
        assert_eq!(
            arena.extract_label(input),
            "Sök...",
            "placeholder borde användas"
        );
    }

    #[test]
    fn test_extract_label_inner_text() {
        let arena = make_arena(r#"<html><body><button>Skicka</button></body></html>"#);
        let btn = find_by_tag(&arena, arena.document, "button").expect("Borde hitta <button>");
        assert!(
            arena.extract_label(btn).contains("Skicka"),
            "Inner text borde användas som fallback"
        );
    }

    // === Titel ===

    #[test]
    fn test_extract_title() {
        let arena = make_arena(r#"<html><head><title>Min sida</title></head><body></body></html>"#);
        assert_eq!(arena.extract_title(), "Min sida", "Titel borde extraheras");
    }

    #[test]
    fn test_extract_title_missing() {
        let arena = make_arena(r#"<html><body><p>Ingen titel</p></body></html>"#);
        assert!(
            arena.extract_title().is_empty(),
            "Saknad titel borde ge tom sträng"
        );
    }

    // === Text-extraktion ===

    #[test]
    fn test_extract_text_skips_script() {
        let arena = make_arena(
            r#"<html><body><div>Synlig<script>var x = "dold";</script></div></body></html>"#,
        );
        let div = find_by_tag(&arena, arena.document, "div").expect("Borde hitta <div>");
        let text = arena.extract_text(div);
        assert!(text.contains("Synlig"), "Synlig text borde finnas");
        assert!(!text.contains("dold"), "Script-text borde skippas");
    }

    #[test]
    fn test_extract_text_nested() {
        let arena =
            make_arena(r#"<html><body><div>Yttre <span>inre</span> text</div></body></html>"#);
        let div = find_by_tag(&arena, arena.document, "div").expect("Borde hitta <div>");
        let text = arena.extract_text(div);
        assert!(
            text.contains("Yttre") && text.contains("inre"),
            "Borde extrahera nästlad text: got '{}'",
            text
        );
    }

    // === Prestanda ===

    #[test]
    fn test_large_document_conversion() {
        // 500 element — borde konverteras snabbt
        let mut html = String::from("<html><body>");
        for i in 0..500 {
            html.push_str(&format!(r#"<div id="n{}"><p>Text {}</p></div>"#, i, i));
        }
        html.push_str("</body></html>");

        let start = std::time::Instant::now();
        let arena = make_arena(&html);
        let elapsed = start.elapsed();

        assert!(
            arena.nodes.len() > 500,
            "Borde ha minst 500 noder, fick {}",
            arena.nodes.len()
        );
        assert!(
            elapsed.as_millis() < 200,
            "Konvertering borde ta <200ms, tog {}ms",
            elapsed.as_millis()
        );
    }

    // === Lazy-loaded bilder ===

    #[test]
    fn test_resolve_lazy_images_data_src() {
        let mut arena = make_arena(
            r##"<html><body><img data-src="https://example.com/photo.jpg" alt="Foto" /></body></html>"##,
        );
        let img = find_by_tag(&arena, arena.document, "img").expect("Borde hitta <img>");

        // Före resolve: ingen riktig src
        assert!(
            arena.get_attr(img, "src").is_none(),
            "src borde saknas innan resolve"
        );

        arena.resolve_lazy_images();

        assert_eq!(
            arena.get_attr(img, "src"),
            Some("https://example.com/photo.jpg"),
            "data-src borde promoterats till src"
        );
    }

    #[test]
    fn test_resolve_lazy_images_placeholder_data_uri() {
        let mut arena = make_arena(
            r##"<html><body><img src="data:image/svg+xml;base64,PHN2Zz4=" data-src="https://real.com/img.png" /></body></html>"##,
        );
        arena.resolve_lazy_images();
        let img = find_by_tag(&arena, arena.document, "img").expect("Borde hitta <img>");
        assert_eq!(
            arena.get_attr(img, "src"),
            Some("https://real.com/img.png"),
            "data: placeholder borde bytas ut mot data-src"
        );
    }

    #[test]
    fn test_resolve_lazy_images_keeps_real_src() {
        let mut arena = make_arena(
            r##"<html><body><img src="https://real.com/img.png" data-src="https://lazy.com/other.png" /></body></html>"##,
        );
        arena.resolve_lazy_images();
        let img = find_by_tag(&arena, arena.document, "img").expect("Borde hitta <img>");
        assert_eq!(
            arena.get_attr(img, "src"),
            Some("https://real.com/img.png"),
            "Riktig src borde inte ändras"
        );
    }

    #[test]
    fn test_resolve_lazy_images_multiple() {
        let mut arena = make_arena(
            r##"<html><body>
                <img data-src="https://a.com/1.jpg" />
                <img data-lazy-src="https://b.com/2.jpg" />
                <img src="https://c.com/3.jpg" />
            </body></html>"##,
        );
        arena.resolve_lazy_images();

        // Räkna imgs med riktig src
        let keys: Vec<_> = arena.nodes.keys().collect();
        let imgs_with_src: Vec<_> = keys
            .iter()
            .filter(|k| {
                arena.tag_name(**k) == Some("img")
                    && arena
                        .get_attr(**k, "src")
                        .map(|s| !s.starts_with("data:") && !s.is_empty())
                        .unwrap_or(false)
            })
            .collect();
        assert_eq!(
            imgs_with_src.len(),
            3,
            "Alla 3 bilder borde ha riktig src efter resolve"
        );
    }

    // === NoJS-filtrering ===

    #[test]
    fn test_nojs_div_hidden_by_id() {
        let arena =
            make_arena(r##"<html><body><div id="nojs">Aktivera JavaScript</div></body></html>"##);
        let keys: Vec<_> = arena.nodes.keys().collect();
        let nojs = keys
            .iter()
            .find(|k| arena.get_attr(**k, "id") == Some("nojs"))
            .expect("Borde hitta div#nojs");
        assert!(
            !arena.is_likely_visible(*nojs),
            "div#nojs borde markeras som osynlig"
        );
    }

    #[test]
    fn test_nojs_div_hidden_by_class() {
        let arena = make_arena(
            r##"<html><body><div class="no-js warning">Aktivera JavaScript</div></body></html>"##,
        );
        let keys: Vec<_> = arena.nodes.keys().collect();
        let nojs = keys
            .iter()
            .find(|k| {
                arena
                    .get_attr(**k, "class")
                    .map(|c| c.contains("no-js"))
                    .unwrap_or(false)
            })
            .expect("Borde hitta div.no-js");
        assert!(
            !arena.is_likely_visible(*nojs),
            "div.no-js borde markeras som osynlig"
        );
    }

    #[test]
    fn test_normal_div_visible() {
        let arena = make_arena(r##"<html><body><div id="content">Innehåll</div></body></html>"##);
        let keys: Vec<_> = arena.nodes.keys().collect();
        let content = keys
            .iter()
            .find(|k| arena.get_attr(**k, "id") == Some("content"))
            .expect("Borde hitta div#content");
        assert!(
            arena.is_likely_visible(*content),
            "Vanlig div borde vara synlig"
        );
    }
}
