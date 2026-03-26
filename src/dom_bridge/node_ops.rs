// Node tree manipulation: appendChild, removeChild, insertBefore, etc.

use rquickjs::{Ctx, Value};

use crate::arena_dom::{NodeKey, NodeType};
use crate::event_loop::JsHandler;

use super::state::SharedState;
use super::{
    args_to_node_keys, clone_node_recursive, extract_node_key, make_element_object,
    notify_range_mutation, throw_dom_exception, validate_pre_insertion,
};

pub(super) struct NormalizeNode {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for NormalizeNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        // DOM spec: merge adjacent text nodes, remove empty text nodes
        let children: Vec<NodeKey> = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.children.clone())
            .unwrap_or_default();

        let mut new_children: Vec<NodeKey> = Vec::new();
        let mut last_text_key: Option<NodeKey> = None;

        for child in children {
            let is_text = s
                .arena
                .nodes
                .get(child)
                .map(|n| matches!(n.node_type, NodeType::Text))
                .unwrap_or(false);

            if is_text {
                let text = s
                    .arena
                    .nodes
                    .get(child)
                    .and_then(|n| n.text.as_ref())
                    .map(|t| t.to_string())
                    .unwrap_or_default();

                if text.is_empty() {
                    // Ta bort tom textnod
                    if let Some(n) = s.arena.nodes.get_mut(child) {
                        n.parent = None;
                    }
                    continue;
                }

                if let Some(prev_key) = last_text_key {
                    // Merge med föregående textnod
                    let prev_text = s
                        .arena
                        .nodes
                        .get(prev_key)
                        .and_then(|n| n.text.as_ref())
                        .map(|t| t.to_string())
                        .unwrap_or_default();
                    let merged = format!("{}{}", prev_text, text);
                    if let Some(n) = s.arena.nodes.get_mut(prev_key) {
                        n.text = Some(merged.into());
                    }
                    if let Some(n) = s.arena.nodes.get_mut(child) {
                        n.parent = None;
                    }
                } else {
                    new_children.push(child);
                    last_text_key = Some(child);
                }
            } else {
                new_children.push(child);
                last_text_key = None;
            }
        }

        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.children = new_children;
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct AppendChild {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for AppendChild {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let child_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(
                        ctx.clone(),
                        "TypeError: Failed to execute 'appendChild': parameter 1 is not of type 'Node'.",
                    )?
                    .into(),
                ));
            }
        };
        let old_parent_key;
        let old_index;
        {
            let mut s = self.state.borrow_mut();
            // Validera pre-insertion (DOM spec)
            if let Err(msg) = validate_pre_insertion(&s.arena, self.key, child_key, None) {
                drop(s);
                let err_name = if msg.starts_with("HierarchyRequestError") {
                    "HierarchyRequestError"
                } else {
                    "NotFoundError"
                };
                return Err(throw_dom_exception(ctx, err_name, msg));
            }
            // Spara gammal position för Range-mutation
            old_parent_key = s.arena.nodes.get(child_key).and_then(|n| n.parent);
            old_index = old_parent_key.and_then(|pk| {
                s.arena
                    .nodes
                    .get(pk)
                    .and_then(|p| p.children.iter().position(|&c| c == child_key))
            });
            // Hantera DocumentFragment: flytta alla barn istället
            let is_fragment = s
                .arena
                .nodes
                .get(child_key)
                .map(|n| matches!(n.node_type, NodeType::DocumentFragment))
                .unwrap_or(false);
            if is_fragment {
                let frag_children: Vec<NodeKey> = s
                    .arena
                    .nodes
                    .get(child_key)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                // Töm fragmentet
                if let Some(frag) = s.arena.nodes.get_mut(child_key) {
                    frag.children.clear();
                }
                for fc in &frag_children {
                    s.arena.append_child(self.key, *fc);
                }
                s.mutations.push(std::borrow::Cow::Borrowed("appendChild"));
                drop(s);
                // Returnera fragmentet
                return make_element_object(ctx, child_key, &self.state);
            }
            // Ta bort från gammal förälder
            if let Some(old_parent) = old_parent_key {
                if let Some(parent_node) = s.arena.nodes.get_mut(old_parent) {
                    parent_node.children.retain(|&c| c != child_key);
                }
            }
            s.arena.append_child(self.key, child_key);
            s.mutations.push(std::borrow::Cow::Borrowed("appendChild"));
        }
        // Notifiera Range-objekt om mutationen
        notify_range_mutation(
            ctx,
            "appendChild",
            self.key,
            child_key,
            old_parent_key,
            old_index,
        );
        make_element_object(ctx, child_key, &self.state)
    }
}

pub(super) struct RemoveChild {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for RemoveChild {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let child_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: argument is not a Node")?
                        .into(),
                ));
            }
        };
        let old_index;
        {
            let mut s = self.state.borrow_mut();
            let is_child = s
                .arena
                .nodes
                .get(self.key)
                .map(|n| n.children.contains(&child_key))
                .unwrap_or(false);
            if !is_child {
                drop(s);
                return Err(ctx.throw(
                    rquickjs::String::from_str(
                        ctx.clone(),
                        "NotFoundError: child is not a child of this node",
                    )?
                    .into(),
                ));
            }
            old_index = s
                .arena
                .nodes
                .get(self.key)
                .and_then(|n| n.children.iter().position(|&c| c == child_key));
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.retain(|&c| c != child_key);
            }
            if let Some(child) = s.arena.nodes.get_mut(child_key) {
                child.parent = None;
            }
            s.mutations.push(std::borrow::Cow::Borrowed("removeChild"));
        }
        notify_range_mutation(ctx, "removeChild", self.key, child_key, None, old_index);
        make_element_object(ctx, child_key, &self.state)
    }
}

pub(super) struct InsertBefore {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for InsertBefore {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(
                        ctx.clone(),
                        "TypeError: Failed to execute 'insertBefore': parameter 1 is not of type 'Node'.",
                    )?
                    .into(),
                ));
            }
        };
        let ref_key = args.get(1).and_then(extract_node_key);
        let old_parent_key;
        let old_index;
        {
            let mut s = self.state.borrow_mut();
            // Validera pre-insertion (DOM spec)
            if let Err(msg) = validate_pre_insertion(&s.arena, self.key, new_key, ref_key) {
                drop(s);
                let err_name = if msg.starts_with("NotFoundError") {
                    "NotFoundError"
                } else {
                    "HierarchyRequestError"
                };
                return Err(throw_dom_exception(ctx, err_name, msg));
            }
            old_parent_key = s.arena.nodes.get(new_key).and_then(|n| n.parent);
            old_index = old_parent_key.and_then(|pk| {
                s.arena
                    .nodes
                    .get(pk)
                    .and_then(|p| p.children.iter().position(|&c| c == new_key))
            });
            // Hantera DocumentFragment
            let is_fragment = s
                .arena
                .nodes
                .get(new_key)
                .map(|n| matches!(n.node_type, NodeType::DocumentFragment))
                .unwrap_or(false);
            if is_fragment {
                let frag_children: Vec<NodeKey> = s
                    .arena
                    .nodes
                    .get(new_key)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                if let Some(frag) = s.arena.nodes.get_mut(new_key) {
                    frag.children.clear();
                }
                for fc in &frag_children {
                    // Detach from old parent
                    if let Some(old_p) = s.arena.nodes.get(*fc).and_then(|n| n.parent) {
                        if let Some(pn) = s.arena.nodes.get_mut(old_p) {
                            pn.children.retain(|&c| c != *fc);
                        }
                    }
                    if let Some(node) = s.arena.nodes.get_mut(self.key) {
                        if let Some(rk) = ref_key {
                            if let Some(pos) = node.children.iter().position(|&c| c == rk) {
                                node.children.insert(pos, *fc);
                            } else {
                                node.children.push(*fc);
                            }
                        } else {
                            node.children.push(*fc);
                        }
                    }
                    if let Some(n) = s.arena.nodes.get_mut(*fc) {
                        n.parent = Some(self.key);
                    }
                }
                s.mutations.push(std::borrow::Cow::Borrowed("insertBefore"));
                drop(s);
                return make_element_object(ctx, new_key, &self.state);
            }
            if let Some(old_parent) = old_parent_key {
                if let Some(p) = s.arena.nodes.get_mut(old_parent) {
                    p.children.retain(|&c| c != new_key);
                }
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                if let Some(rk) = ref_key {
                    if let Some(pos) = node.children.iter().position(|&c| c == rk) {
                        node.children.insert(pos, new_key);
                    } else {
                        node.children.push(new_key);
                    }
                } else {
                    node.children.push(new_key);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(new_key) {
                n.parent = Some(self.key);
            }
            s.mutations.push(std::borrow::Cow::Borrowed("insertBefore"));
        }
        notify_range_mutation(
            ctx,
            "insertBefore",
            self.key,
            new_key,
            old_parent_key,
            old_index,
        );
        make_element_object(ctx, new_key, &self.state)
    }
}

pub(super) struct ReplaceChild {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ReplaceChild {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Spec: replaceChild(null, ...) och replaceChild(.., null) kastar TypeError
        let new_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: argument is not a Node")?
                        .into(),
                ));
            }
        };
        let old_key = match args.get(1).and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: argument is not a Node")?
                        .into(),
                ));
            }
        };
        {
            let mut s = self.state.borrow_mut();
            // Validera pre-insertion (DOM spec) — child = old_key
            if let Err(msg) = validate_pre_insertion(&s.arena, self.key, new_key, Some(old_key)) {
                drop(s);
                let err_name = if msg.starts_with("NotFoundError") {
                    "NotFoundError"
                } else {
                    "HierarchyRequestError"
                };
                return Err(throw_dom_exception(ctx, err_name, msg));
            }
            // Hitta gammal child — NotFoundError om den inte finns
            let pos = s
                .arena
                .nodes
                .get(self.key)
                .and_then(|n| n.children.iter().position(|&c| c == old_key));
            let _pos = match pos {
                Some(p) => p,
                None => {
                    drop(s);
                    return Err(throw_dom_exception(
                        ctx,
                        "NotFoundError",
                        "NotFoundError: old child is not a child of this node",
                    ));
                }
            };
            // Detach new_key från gammal parent
            if let Some(old_parent) = s.arena.nodes.get(new_key).and_then(|n| n.parent) {
                if let Some(parent_node) = s.arena.nodes.get_mut(old_parent) {
                    parent_node.children.retain(|&c| c != new_key);
                }
            }
            // Ersätt — recalkulera pos efter detach
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                let pos = node
                    .children
                    .iter()
                    .position(|&c| c == old_key)
                    .unwrap_or(0);
                if pos < node.children.len() {
                    node.children[pos] = new_key;
                } else {
                    node.children.push(new_key);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(new_key) {
                n.parent = Some(self.key);
            }
            if let Some(n) = s.arena.nodes.get_mut(old_key) {
                n.parent = None;
            }
            s.mutations.push(std::borrow::Cow::Borrowed("replaceChild"));
        }
        make_element_object(ctx, old_key, &self.state)
    }
}

pub(super) struct CloneNode {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for CloneNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let deep = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let new_key = clone_node_recursive(&self.state, self.key, deep);
        make_element_object(ctx, new_key, &self.state)
    }
}

// ─── Migration 1: element.remove() ──────────────────────────────────────────
pub(super) struct Remove {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for Remove {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                parent.children.retain(|&c| c != self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.parent = None;
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 2: element.before(...nodes) ──────────────────────────────────
pub(super) struct Before {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for Before {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parent_key = {
            let s = self.state.borrow();
            s.arena.nodes.get(self.key).and_then(|n| n.parent)
        };
        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Detach ALLA nya noder först (förhindrar position-shift vid sibling-args)
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        // Hitta position EFTER detach
        let pos = s
            .arena
            .nodes
            .get(parent_key)
            .and_then(|n| n.children.iter().position(|&c| c == self.key))
            .unwrap_or(0);
        for (i, nk) in new_keys.into_iter().enumerate() {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(parent_key);
            }
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                let insert_pos = (pos + i).min(parent.children.len());
                parent.children.insert(insert_pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 3: element.after(...nodes) ───────────────────────────────────
pub(super) struct After {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for After {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parent_key = {
            let s = self.state.borrow();
            s.arena.nodes.get(self.key).and_then(|n| n.parent)
        };
        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Detach ALLA först
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        // Hitta position EFTER detach
        let pos = s
            .arena
            .nodes
            .get(parent_key)
            .and_then(|n| n.children.iter().position(|&c| c == self.key))
            .map(|p| p + 1)
            .unwrap_or(0);
        for (i, nk) in new_keys.into_iter().enumerate() {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(parent_key);
            }
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                let insert_pos = (pos + i).min(parent.children.len());
                parent.children.insert(insert_pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 4: element.replaceWith(...nodes) ─────────────────────────────
pub(super) struct ReplaceWith {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ReplaceWith {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parent_key = {
            let s = self.state.borrow();
            s.arena.nodes.get(self.key).and_then(|n| n.parent)
        };
        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Detach alla nya noder + self
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        let pos = s
            .arena
            .nodes
            .get(parent_key)
            .and_then(|n| n.children.iter().position(|&c| c == self.key))
            .unwrap_or(0);
        if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
            parent.children.retain(|&c| c != self.key);
        }
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.parent = None;
        }
        for (i, nk) in new_keys.into_iter().enumerate() {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(parent_key);
            }
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                let insert_pos = (pos + i).min(parent.children.len());
                parent.children.insert(insert_pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration: prepend/append/replaceChildren ──────────────────────────────
pub(super) struct Prepend {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for Prepend {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        for (i, nk) in new_keys.into_iter().enumerate() {
            // Detachera precis innan infogning
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                let pos = i.min(node.children.len());
                node.children.insert(pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct Append {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for Append {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        for nk in new_keys {
            // Detachera precis innan infogning (hanterar append(same, same))
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.push(nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct ReplaceChildren {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ReplaceChildren {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Ta bort alla befintliga barn
        let old_children: Vec<NodeKey> = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for ck in old_children {
            if let Some(c) = s.arena.nodes.get_mut(ck) {
                c.parent = None;
            }
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.children.clear();
        }
        // Detach + append nya
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        for nk in new_keys {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.push(nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}
