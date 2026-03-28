// Attribute handlers: getAttribute, setAttribute, removeAttribute, etc.

use std::rc::Rc;

use rquickjs::{Ctx, Object, Value};

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};
use crate::event_loop::JsHandler;

#[cfg(feature = "blitz")]
use super::invalidate_blitz_cache;
use super::state::SharedState;
use super::{
    copy_subtree_return_key, extract_node_key, make_element_object, node_key_to_f64,
    throw_dom_exception,
};

/// Validera att en sträng matchar XML Name-produktionen.
/// NameStartChar: ":" | [A-Z] | "_" | [a-z] | diverse Unicode-range
/// NameChar: NameStartChar | "-" | "." | [0-9] | diverse Unicode
fn is_valid_xml_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().unwrap();
    if !is_name_start_char(first) {
        return false;
    }
    chars.all(is_name_char)
}

fn is_name_start_char(c: char) -> bool {
    matches!(c, ':' | 'A'..='Z' | '_' | 'a'..='z'
        | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}' | '\u{F8}'..='\u{2FF}'
        | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}'
        | '\u{2C00}'..='\u{2FEF}' | '\u{3001}'..='\u{D7FF}'
        | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}')
}

fn is_name_char(c: char) -> bool {
    is_name_start_char(c)
        || matches!(c, '-' | '.' | '0'..='9' | '\u{B7}'
            | '\u{0300}'..='\u{036F}' | '\u{203F}'..='\u{2040}')
}

// ─── JsHandler-structs för element-metoder ──────────────────────────────────

pub(super) struct GetAttribute {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for GetAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let lc_name = name.to_ascii_lowercase();
        let s = self.state.borrow();
        match s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr(&lc_name))
        {
            Some(val) => Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value()),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

pub(super) struct SetAttribute {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for SetAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        // Spec: validera Name-produktion
        if !is_valid_xml_name(&name) {
            return Err(throw_dom_exception(
                ctx,
                "InvalidCharacterError",
                "The string contains invalid characters.",
            ));
        }
        let val = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        {
            let mut s = self.state.borrow_mut();
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                // HTML-spec: attributnamn lowercasas på HTML-element
                let lc_name = name.to_ascii_lowercase();
                node.attributes.insert(lc_name, val);
            }
            s.mutations.push(std::borrow::Cow::Owned(format!(
                "setAttribute:{}:{}",
                node_key_to_f64(self.key),
                name
            )));
        }
        #[cfg(feature = "blitz")]
        invalidate_blitz_cache(&self.state);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct RemoveAttribute {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for RemoveAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let lc_name = name.to_ascii_lowercase();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.attributes.remove(&lc_name);
        }
        drop(s);
        #[cfg(feature = "blitz")]
        invalidate_blitz_cache(&self.state);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct HasAttribute {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for HasAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let lc_name = name.to_ascii_lowercase();
        let s = self.state.borrow();
        let has = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr(&lc_name))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), has))
    }
}

// ─── hasChildNodes ──────────────────────────────────────────────────────────

pub(super) struct HasChildNodes {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for HasChildNodes {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let has = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| !n.children.is_empty())
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), has))
    }
}

// ─── getAttributeNames ──────────────────────────────────────────────────────

pub(super) struct GetAttributeNames {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for GetAttributeNames {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let arr = rquickjs::Array::new(ctx.clone())?;
        if let Some(node) = s.arena.nodes.get(self.key) {
            // Behåll dokumentordning (insertion order), filtrera interna __-attribut
            let names: Vec<&String> = node
                .attributes
                .keys()
                .filter(|k| !k.starts_with("__"))
                .collect();
            for (i, name) in names.iter().enumerate() {
                arr.set(
                    i,
                    rquickjs::String::from_str(ctx.clone(), name)?.into_value(),
                )?;
            }
        }
        Ok(arr.into_value())
    }
}

// ─── insertAdjacentHTML ─────────────────────────────────────────────────────

pub(super) struct InsertAdjacentHTML {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for InsertAdjacentHTML {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let position = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_lowercase();
        let html_str = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();

        if html_str.is_empty() {
            return Ok(Value::new_undefined(ctx.clone()));
        }

        {
            let mut s = self.state.borrow_mut();

            // Parsa HTML-fragmentet
            let parsed_keys = if !html_str.contains('<') {
                // Enkel textnode
                let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: crate::arena_dom::Attrs::new(),
                    text: Some(html_str.into()),
                    parent: None,
                    children: vec![],
                    owner_doc: None,
                });
                vec![text_key]
            } else {
                let rcdom = crate::parser::parse_html(&html_str);
                let fragment = ArenaDom::from_rcdom(&rcdom);
                // Extrahera body-barn (html5ever wrappar i html/head/body)
                let body_key = {
                    let doc_key = fragment.document;
                    fragment
                        .nodes
                        .get(doc_key)
                        .and_then(|doc| {
                            doc.children.iter().find(|&&c| {
                                fragment.nodes.get(c).and_then(|n| n.tag.as_deref()) == Some("html")
                            })
                        })
                        .and_then(|&html_key| {
                            fragment.nodes.get(html_key).and_then(|html| {
                                html.children.iter().find(|&&c| {
                                    fragment.nodes.get(c).and_then(|n| n.tag.as_deref())
                                        == Some("body")
                                })
                            })
                        })
                        .copied()
                };
                let children = body_key
                    .and_then(|bk| fragment.nodes.get(bk))
                    .map(|n| n.children.clone())
                    .unwrap_or_else(|| {
                        // Fallback: document-barn
                        fragment
                            .nodes
                            .get(fragment.document)
                            .map(|n| n.children.clone())
                            .unwrap_or_default()
                    });
                // Kopiera alla fragment-barn till vår arena (utan förälder ännu)
                let mut new_keys = Vec::new();
                // Använd temporär nyckel — vi sätter rätt förälder nedan
                let temp_parent = s.arena.document;
                for &child in &children {
                    let nk = copy_subtree_return_key(&fragment, child, &mut s.arena);
                    new_keys.push(nk);
                }
                // Ta bort från temp-förälder (copy_subtree_return_key lägger inte till)
                let _ = temp_parent;
                new_keys
            };

            match position.as_str() {
                "beforebegin" => {
                    // Infoga före detta element (syskon)
                    if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                        if let Some(parent_node) = s.arena.nodes.get_mut(parent_key) {
                            if let Some(pos) =
                                parent_node.children.iter().position(|&c| c == self.key)
                            {
                                for (i, &nk) in parsed_keys.iter().enumerate() {
                                    parent_node.children.insert(pos + i, nk);
                                }
                            }
                        }
                        for &nk in &parsed_keys {
                            if let Some(n) = s.arena.nodes.get_mut(nk) {
                                n.parent = Some(parent_key);
                            }
                        }
                    }
                }
                "afterbegin" => {
                    // Infoga som första barn
                    if let Some(node) = s.arena.nodes.get_mut(self.key) {
                        for (i, &nk) in parsed_keys.iter().enumerate() {
                            node.children.insert(i, nk);
                        }
                    }
                    for &nk in &parsed_keys {
                        if let Some(n) = s.arena.nodes.get_mut(nk) {
                            n.parent = Some(self.key);
                        }
                    }
                }
                "beforeend" => {
                    // Infoga som sista barn
                    for &nk in &parsed_keys {
                        s.arena.append_child(self.key, nk);
                    }
                }
                "afterend" => {
                    // Infoga efter detta element (syskon)
                    if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                        if let Some(parent_node) = s.arena.nodes.get_mut(parent_key) {
                            if let Some(pos) =
                                parent_node.children.iter().position(|&c| c == self.key)
                            {
                                for (i, &nk) in parsed_keys.iter().enumerate() {
                                    parent_node.children.insert(pos + 1 + i, nk);
                                }
                            }
                        }
                        for &nk in &parsed_keys {
                            if let Some(n) = s.arena.nodes.get_mut(nk) {
                                n.parent = Some(parent_key);
                            }
                        }
                    }
                }
                _ => {} // Ogiltig position — ignorera tyst
            }

            s.mutations
                .push(std::borrow::Cow::Borrowed("insertAdjacentHTML"));
        }

        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 5: toggleAttribute(name [, force]) ──────────────────────────
pub(super) struct ToggleAttribute {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ToggleAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        // Spec: om name inte matchar Name-produktionen → InvalidCharacterError
        let invalid_name = name.is_empty() || name.contains(char::is_whitespace);
        if invalid_name {
            return Err(throw_dom_exception(
                ctx,
                "InvalidCharacterError",
                "The string contains invalid characters.",
            ));
        }
        let has_force = args.len() > 1 && !args.get(1).map(|v| v.is_undefined()).unwrap_or(true);
        let force = if has_force {
            args.get(1)
                .map(|v| {
                    v.as_bool().unwrap_or_else(|| {
                        !v.is_null()
                            && !v.is_undefined()
                            && v.as_int().map(|n| n != 0).unwrap_or(true)
                    })
                })
                .unwrap_or(false)
        } else {
            false
        };
        let mut s = self.state.borrow_mut();
        let has_attr = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr(&name))
            .unwrap_or(false);
        if has_force {
            if force {
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.attributes.insert(name, String::new());
                }
                return Ok(Value::new_bool(ctx.clone(), true));
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.attributes.remove(&name);
            }
            return Ok(Value::new_bool(ctx.clone(), false));
        }
        if has_attr {
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.attributes.remove(&name);
            }
            Ok(Value::new_bool(ctx.clone(), false))
        } else {
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.attributes.insert(name, String::new());
            }
            Ok(Value::new_bool(ctx.clone(), true))
        }
    }
}

// ─── Migration: insertAdjacentElement/Text ──────────────────────────────────
pub(super) struct InsertAdjacentElement {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for InsertAdjacentElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let position = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_lowercase();
        let new_key = match args.get(1).and_then(extract_node_key) {
            Some(k) => k,
            None => return Ok(Value::new_null(ctx.clone())),
        };
        let mut s = self.state.borrow_mut();
        // Detach
        if let Some(old_p) = s.arena.nodes.get(new_key).and_then(|n| n.parent) {
            if let Some(p) = s.arena.nodes.get_mut(old_p) {
                p.children.retain(|&c| c != new_key);
            }
        }
        match position.as_str() {
            "beforebegin" => {
                if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                    let pos = s
                        .arena
                        .nodes
                        .get(parent_key)
                        .and_then(|n| n.children.iter().position(|&c| c == self.key))
                        .unwrap_or(0);
                    if let Some(n) = s.arena.nodes.get_mut(new_key) {
                        n.parent = Some(parent_key);
                    }
                    if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                        parent.children.insert(pos, new_key);
                    }
                }
            }
            "afterbegin" => {
                if let Some(n) = s.arena.nodes.get_mut(new_key) {
                    n.parent = Some(self.key);
                }
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.children.insert(0, new_key);
                }
            }
            "beforeend" => {
                if let Some(n) = s.arena.nodes.get_mut(new_key) {
                    n.parent = Some(self.key);
                }
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.children.push(new_key);
                }
            }
            "afterend" => {
                if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                    let pos = s
                        .arena
                        .nodes
                        .get(parent_key)
                        .and_then(|n| n.children.iter().position(|&c| c == self.key))
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    if let Some(n) = s.arena.nodes.get_mut(new_key) {
                        n.parent = Some(parent_key);
                    }
                    if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                        let p = pos.min(parent.children.len());
                        parent.children.insert(p, new_key);
                    }
                }
            }
            _ => {}
        }
        drop(s);
        make_element_object(ctx, new_key, &self.state)
    }
}

pub(super) struct InsertAdjacentText {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for InsertAdjacentText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let position = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let text = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        // Skapa textnod
        let text_key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: Some(text.into()),
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        // Delegera till InsertAdjacentElement-logik
        let pos_val = rquickjs::String::from_str(ctx.clone(), &position)?.into_value();
        let elem_val = make_element_object(ctx, text_key, &self.state)?;
        let handler = InsertAdjacentElement {
            state: Rc::clone(&self.state),
            key: self.key,
        };
        handler.handle(ctx, &[pos_val, elem_val])
    }
}

// ─── Migration: NS attribute methods ────────────────────────────────────────
pub(super) struct SetAttributeNS {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for SetAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let ns = args.first().and_then(|v| {
            if v.is_null() || v.is_undefined() {
                None
            } else {
                v.as_string().and_then(|s| s.to_string().ok())
            }
        });
        let qname = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        // Validera Name-produktion
        if !is_valid_xml_name(&qname) {
            return Err(throw_dom_exception(
                ctx,
                "InvalidCharacterError",
                "The string contains invalid characters.",
            ));
        }
        // Validera QName (om kolon finns, prefix och local måste vara giltiga)
        if let Some(colon) = qname.find(':') {
            let prefix = &qname[..colon];
            let local = &qname[colon + 1..];
            if prefix.is_empty() || local.is_empty() || local.contains(':') {
                return Err(throw_dom_exception(
                    ctx,
                    "InvalidCharacterError",
                    "The string contains invalid characters.",
                ));
            }
            // Namespace-validering
            if ns.is_none() {
                return Err(throw_dom_exception(
                    ctx,
                    "NamespaceError",
                    "A namespace is required to use a prefix.",
                ));
            }
            if prefix == "xml" && ns.as_deref() != Some("http://www.w3.org/XML/1998/namespace") {
                return Err(throw_dom_exception(
                    ctx,
                    "NamespaceError",
                    "The xml prefix requires the XML namespace.",
                ));
            }
            if (prefix == "xmlns" || qname == "xmlns")
                && ns.as_deref() != Some("http://www.w3.org/2000/xmlns/")
            {
                return Err(throw_dom_exception(
                    ctx,
                    "NamespaceError",
                    "The xmlns prefix requires the XMLNS namespace.",
                ));
            }
        }
        // xmlns qname utan xmlns namespace
        if qname == "xmlns" && ns.as_deref() != Some("http://www.w3.org/2000/xmlns/") {
            return Err(throw_dom_exception(
                ctx,
                "NamespaceError",
                "The xmlns qualified name requires the XMLNS namespace.",
            ));
        }
        // XMLNS namespace kräver xmlns prefix
        if ns.as_deref() == Some("http://www.w3.org/2000/xmlns/")
            && !qname.starts_with("xmlns:")
            && qname != "xmlns"
        {
            return Err(throw_dom_exception(
                ctx,
                "NamespaceError",
                "The XMLNS namespace requires the xmlns prefix.",
            ));
        }
        let val = args
            .get(2)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let local = if let Some(colon) = qname.find(':') {
            &qname[colon + 1..]
        } else {
            &qname
        };
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            // NS-attribut bevarar case (lowercas ej)
            node.attributes.insert(local.to_string(), val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct GetAttributeNS {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for GetAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let local = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        match s.arena.nodes.get(self.key).and_then(|n| n.get_attr(&local)) {
            Some(val) => Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value()),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

pub(super) struct HasAttributeNS {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for HasAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let local = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        let has = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr(&local))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), has))
    }
}

pub(super) struct RemoveAttributeNS {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for RemoveAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let local = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.attributes.remove(&local);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct GetAttributeNode {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for GetAttributeNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr(&name))
            .map(|v| v.to_string());
        drop(s);
        match val {
            Some(v) => {
                let obj = Object::new(ctx.clone())?;
                obj.set("name", name.as_str())?;
                obj.set("localName", name.as_str())?;
                obj.set("value", v.as_str())?;
                obj.set("namespaceURI", Value::new_null(ctx.clone()))?;
                obj.set("prefix", Value::new_null(ctx.clone()))?;
                obj.set("specified", true)?;
                obj.set("nodeType", 2)?;
                obj.set("nodeName", name.as_str())?;
                Ok(obj.into_value())
            }
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}
