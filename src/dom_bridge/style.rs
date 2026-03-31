// classList + style: DOMTokenList och CSSStyleDeclaration

use std::rc::Rc;

use rquickjs::{Ctx, Function, Object, Value};

use crate::arena_dom::NodeKey;
use crate::event_loop::{JsFn, JsHandler};

#[cfg(feature = "blitz")]
use super::invalidate_blitz_cache;
use super::state::SharedState;
use super::utils::{
    expand_shorthand, parse_inline_styles, parse_inline_styles_ordered, reconstruct_shorthand,
    remove_shorthand_longhands, serialize_css_text_ordered, serialize_inline_styles,
};
use super::{js_value_to_dom_string, validate_token};

pub(super) struct ClassListAdd {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListAdd {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Stöd för flera tokens: classList.add("a", "b", "c")
        let mut tokens = Vec::new();
        for arg in args {
            let t = js_value_to_dom_string(Some(arg));
            validate_token(ctx, &t)?;
            tokens.push(t);
        }
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            // Ordered set: deduplicera existerande
            let mut seen = std::collections::HashSet::new();
            let mut classes: Vec<String> = current
                .split_whitespace()
                .filter(|c| seen.insert(c.to_string()))
                .map(|s| s.to_string())
                .collect();
            for t in tokens {
                if !classes.iter().any(|c| c == &t) {
                    classes.push(t);
                }
            }
            // Ordered set serialize: unika, single-space, ingen leading/trailing
            node.attributes
                .insert("class".to_string(), classes.join(" "));
        }
        #[cfg(feature = "blitz")]
        invalidate_blitz_cache(&self.state);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct ClassListRemove {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListRemove {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut tokens = Vec::new();
        for arg in args {
            let t = js_value_to_dom_string(Some(arg));
            validate_token(ctx, &t)?;
            tokens.push(t);
        }
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            // Ordered set: dedup + remove
            let mut seen = std::collections::HashSet::new();
            let new_cls: Vec<&str> = current
                .split_whitespace()
                .filter(|c| seen.insert(*c))
                .filter(|&c| !tokens.iter().any(|t| t == c))
                .collect();
            node.attributes
                .insert("class".to_string(), new_cls.join(" "));
        }
        #[cfg(feature = "blitz")]
        invalidate_blitz_cache(&self.state);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct ClassListContains {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListContains {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let cls = js_value_to_dom_string(args.first());
        validate_token(ctx, &cls)?;
        let s = self.state.borrow();
        let has = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("class"))
            .map(|c| c.split_whitespace().any(|cl| cl == cls))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), has))
    }
}

pub(super) struct ClassListToggle {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListToggle {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let cls = js_value_to_dom_string(args.first());
        validate_token(ctx, &cls)?;
        // Stöd för force-argument
        let has_force = args.len() > 1 && !args.get(1).map(|v| v.is_undefined()).unwrap_or(true);
        let force = args.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
        if has_force {
            let mut s = self.state.borrow_mut();
            if force {
                // toggle(token, true): om token finns i ordered set → no-op
                // om ej → kör "ordered set" add (deduplicate + normalize + append)
                let current = s
                    .arena
                    .nodes
                    .get(self.key)
                    .map(|n| n.get_attr("class").unwrap_or("").to_string())
                    .unwrap_or_default();
                let mut seen = std::collections::HashSet::new();
                let unique: Vec<&str> = current
                    .split_whitespace()
                    .filter(|c| seen.insert(*c))
                    .collect();
                if !unique.contains(&cls.as_str()) {
                    // Token saknas — lägg till och normalisera (ordered set serializer)
                    let mut result = unique;
                    result.push(&cls);
                    if let Some(node) = s.arena.nodes.get_mut(self.key) {
                        node.attributes
                            .insert("class".to_string(), result.join(" "));
                    }
                }
                // Om token redan finns → ändra INTE attributet
                return Ok(Value::new_bool(ctx.clone(), true));
            }
            // force=false → ta bort ALLA förekomster, normalisera resten
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                let current = node.get_attr("class").unwrap_or("").to_string();
                let has_token = current.split_whitespace().any(|c| c == cls);
                if has_token {
                    // Ordered set remove: ta bort token, deduplicate resten
                    let mut seen = std::collections::HashSet::new();
                    let new_cls: Vec<&str> = current
                        .split_whitespace()
                        .filter(|&c| c != cls && seen.insert(c))
                        .collect();
                    node.attributes
                        .insert("class".to_string(), new_cls.join(" "));
                }
            }
            return Ok(Value::new_bool(ctx.clone(), false));
        }
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            // Ordered set: deduplicera
            let mut seen = std::collections::HashSet::new();
            let unique: Vec<&str> = current
                .split_whitespace()
                .filter(|c| seen.insert(*c))
                .collect();
            if unique.iter().any(|&c| c == cls) {
                // Ta bort → ordered set serialize (normaliserat)
                let new_cls: Vec<&str> = unique.into_iter().filter(|&c| c != cls).collect();
                node.attributes
                    .insert("class".to_string(), new_cls.join(" "));
                return Ok(Value::new_bool(ctx.clone(), false));
            } else {
                // Lägg till → ordered set serialize med ny token
                let mut result: Vec<&str> = unique;
                result.push(&cls);
                node.attributes
                    .insert("class".to_string(), result.join(" "));
                return Ok(Value::new_bool(ctx.clone(), true));
            }
        }
        Ok(Value::new_bool(ctx.clone(), false))
    }
}

pub(super) struct ClassListReplace {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListReplace {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let old_cls = js_value_to_dom_string(args.first());
        validate_token(ctx, &old_cls)?;
        let new_cls_str = js_value_to_dom_string(args.get(1));
        validate_token(ctx, &new_cls_str)?;
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            // Ordered set: dedup, first occurrence wins
            let mut seen = std::collections::HashSet::new();
            let unique: Vec<&str> = current
                .split_whitespace()
                .filter(|c| seen.insert(*c))
                .collect();
            if unique.iter().any(|&c| c == old_cls) {
                // Replace first occurrence, dedup result
                let mut result_seen = std::collections::HashSet::new();
                let replaced: Vec<&str> = unique
                    .into_iter()
                    .map(|c| {
                        if c == old_cls {
                            new_cls_str.as_str()
                        } else {
                            c
                        }
                    })
                    .filter(|c| result_seen.insert(*c))
                    .collect();
                node.attributes
                    .insert("class".to_string(), replaced.join(" "));
                return Ok(Value::new_bool(ctx.clone(), true));
            }
        }
        Ok(Value::new_bool(ctx.clone(), false))
    }
}

pub(super) struct ClassListItem {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let idx = args.first().and_then(|v| v.as_int()).unwrap_or(-1) as usize;
        let s = self.state.borrow();
        let classes: Vec<&str> = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("class"))
            .map(|c| c.split_whitespace().collect())
            .unwrap_or_default();
        if idx < classes.len() {
            Ok(rquickjs::String::from_str(ctx.clone(), classes[idx])?.into_value())
        } else {
            Ok(Value::new_null(ctx.clone()))
        }
    }
}

pub(super) struct ClassListGetRawClass {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListGetRawClass {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let raw = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("class"))
            .unwrap_or("")
            .to_string();
        drop(s);
        Ok(rquickjs::String::from_str(ctx.clone(), &raw)?.into_value())
    }
}

pub(super) struct ClassListGetClasses {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListGetClasses {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let raw = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("class"))
            .unwrap_or("")
            .to_string();
        drop(s);
        // DOMTokenList: unika tokens i ordning (behåll första förekomst)
        let mut seen = std::collections::HashSet::new();
        let classes: Vec<&str> = raw.split_whitespace().filter(|t| seen.insert(*t)).collect();
        let arr = rquickjs::Array::new(ctx.clone())?;
        for (i, cls) in classes.iter().enumerate() {
            arr.set(i, rquickjs::String::from_str(ctx.clone(), cls)?)?;
        }
        Ok(arr.into_value())
    }
}

pub(super) struct ClassListSetValue {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClassListSetValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = js_value_to_dom_string(args.first());
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.attributes.insert("class".to_string(), val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) fn make_class_list<'js>(
    ctx: &Ctx<'js>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set(
        "add",
        Function::new(
            ctx.clone(),
            JsFn(ClassListAdd {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "remove",
        Function::new(
            ctx.clone(),
            JsFn(ClassListRemove {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "contains",
        Function::new(
            ctx.clone(),
            JsFn(ClassListContains {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "toggle",
        Function::new(
            ctx.clone(),
            JsFn(ClassListToggle {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "replace",
        Function::new(
            ctx.clone(),
            JsFn(ClassListReplace {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;

    // item(index) — dynamisk (läser från arena)
    obj.set(
        "item",
        Function::new(
            ctx.clone(),
            JsFn(ClassListItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;

    // Uppdatera length/value/index via JS-helper
    // getRawClass returnerar rå class-attributet (med whitespace och duplicates)
    let get_raw_class_fn = Function::new(
        ctx.clone(),
        JsFn(ClassListGetRawClass {
            state: Rc::clone(state),
            key,
        }),
    )?;
    let set_value_fn = Function::new(
        ctx.clone(),
        JsFn(ClassListSetValue {
            state: Rc::clone(state),
            key,
        }),
    )?;
    let update_fn_code = r#"(function(obj, getClasses, getRawClass, setValue) {
        Object.defineProperty(obj, 'length', { get: function(){ return getClasses().length; }, configurable: true });
        Object.defineProperty(obj, 'value', {
            get: function(){ return getRawClass(); },
            set: function(v){ setValue(String(v)); },
            configurable: true
        });
        Object.defineProperty(obj, Symbol.toStringTag, { value: 'DOMTokenList' });
        obj.toString = function(){ return getRawClass(); };
        obj.forEach = function(cb, thisArg) {
            var cls = getClasses();
            for (var i = 0; i < cls.length; i++) cb.call(thisArg, cls[i], i, obj);
        };
        obj.entries = function() {
            var cls = getClasses(); var i = 0;
            return { next: function(){ return i < cls.length ? {value:[i,cls[i++]],done:false} : {done:true}; }, [Symbol.iterator]: function(){return this;} };
        };
        obj.keys = function() {
            var cls = getClasses(); var i = 0;
            return { next: function(){ return i < cls.length ? {value:i++,done:false} : {done:true}; }, [Symbol.iterator]: function(){return this;} };
        };
        obj.values = function() {
            var cls = getClasses(); var i = 0;
            return { next: function(){ return i < cls.length ? {value:cls[i++],done:false} : {done:true}; }, [Symbol.iterator]: function(){return this;} };
        };
        obj[Symbol.iterator] = obj.values;
        obj.supports = function(){ throw new TypeError("DOMTokenList has no supported tokens"); };
        // Index-access: uppdatera [0], [1], etc. dynamiskt
        var origAdd = obj.add, origRemove = obj.remove, origToggle = obj.toggle, origReplace = obj.replace;
        function syncIndices() {
            var cls = getClasses();
            for (var j = 0; j < 20; j++) { delete obj[j]; }
            for (var j = 0; j < cls.length; j++) { obj[j] = cls[j]; }
        }
        syncIndices();
        obj.add = function() { origAdd.apply(this, arguments); syncIndices(); };
        obj.remove = function() { origRemove.apply(this, arguments); syncIndices(); };
        obj.toggle = function() { var r = origToggle.apply(this, arguments); syncIndices(); return r; };
        obj.replace = function() { var r = origReplace.apply(this, arguments); syncIndices(); return r; };
        return obj;
    })"#;
    let get_classes_fn = Function::new(
        ctx.clone(),
        JsFn(ClassListGetClasses {
            state: Rc::clone(state),
            key,
        }),
    )?;
    if let Ok(update_fn) = ctx.eval::<Function, _>(update_fn_code) {
        let _ = update_fn.call::<_, Value>((
            obj.clone(),
            get_classes_fn,
            get_raw_class_fn,
            set_value_fn,
        ));
    }

    Ok(obj.into_value())
}

// ─── style ──────────────────────────────────────────────────────────────────

pub(super) struct StyleSetProperty {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleSetProperty {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let prop = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let val = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let css_prop = camel_to_kebab(&prop);
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let style_str = node.get_attr("style").unwrap_or("").to_string();
            let mut ordered = parse_inline_styles_ordered(&style_str);
            if val.is_empty() {
                ordered.retain(|(k, _)| k != &css_prop);
                // Ta bort longhands vid shorthand-removal
                let mut as_map: std::collections::HashMap<String, String> =
                    ordered.iter().cloned().collect();
                let before = as_map.len();
                remove_shorthand_longhands(&css_prop, &mut as_map);
                if as_map.len() < before {
                    ordered.retain(|(k, _)| as_map.contains_key(k));
                }
            } else {
                // Expandera shorthand → ersätt shorthand med longhands
                let mut expanded = std::collections::HashMap::new();
                expand_shorthand(&css_prop, &val, &mut expanded);
                if !expanded.is_empty() {
                    // Shorthand: ta bort shorthand-propertyn, sätt longhands
                    ordered.retain(|(k, _)| k != &css_prop);
                    // Ta bort gamla longhands, bevara position
                    let insert_pos = ordered.len();
                    for (k, v) in &expanded {
                        if let Some(existing) = ordered.iter_mut().find(|(ek, _)| ek == k) {
                            existing.1 = v.clone();
                        } else {
                            ordered.insert(insert_pos.min(ordered.len()), (k.clone(), v.clone()));
                        }
                    }
                } else {
                    // Longhand: sätt direkt
                    if let Some(existing) = ordered.iter_mut().find(|(k, _)| k == &css_prop) {
                        existing.1 = val;
                    } else {
                        ordered.push((css_prop, val));
                    }
                }
            }
            let serialized = serialize_css_text_ordered(&ordered);
            node.attributes.insert("style".to_string(), serialized);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct StyleGetPropertyValue {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleGetPropertyValue {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let prop = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let css_prop = camel_to_kebab(&prop);
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("style"))
            .map(|style_str| {
                let styles = parse_inline_styles(style_str);
                // Direkt match
                if let Some(v) = styles.get(&css_prop) {
                    return v.clone();
                }
                // Rekonstruera shorthand från longhands
                reconstruct_shorthand(&css_prop, &styles)
            })
            .unwrap_or_default();
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

pub(super) struct StyleRemoveProperty {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleRemoveProperty {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let prop = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let css_prop = camel_to_kebab(&prop);
        let mut s = self.state.borrow_mut();
        let old_val = if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let style_str = node.get_attr("style").unwrap_or("").to_string();
            let mut styles = parse_inline_styles(&style_str);
            let old = styles.remove(&css_prop).unwrap_or_default();
            remove_shorthand_longhands(&css_prop, &mut styles);
            node.attributes
                .insert("style".to_string(), serialize_inline_styles(&styles));
            old
        } else {
            String::new()
        };
        Ok(rquickjs::String::from_str(ctx.clone(), &old_val)?.into_value())
    }
}

pub(super) fn camel_to_kebab(name: &str) -> String {
    let mut result = String::new();
    for ch in name.chars() {
        if ch.is_uppercase() {
            result.push('-');
            result.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            result.push(ch);
        }
    }
    result
}

// ─── Nya live-handlers för style-proxy ─────────────────────────────────────

pub(super) struct StyleGetCssText {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleGetCssText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let css_text = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("style"))
            .map(|style_str| {
                let ordered = parse_inline_styles_ordered(style_str);
                serialize_css_text_ordered(&ordered)
            })
            .unwrap_or_default();
        Ok(rquickjs::String::from_str(ctx.clone(), &css_text)?.into_value())
    }
}

pub(super) struct StyleSetCssText {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleSetCssText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            // Parsa med ordning bevarad — ingen shorthand-expansion
            let ordered = parse_inline_styles_ordered(&val);
            let serialized = serialize_css_text_ordered(&ordered);
            node.attributes.insert("style".to_string(), serialized);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct StyleGetLength {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleGetLength {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("style"))
            .map(|style_str| parse_inline_styles(style_str).len())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), len as i32))
    }
}

pub(super) struct StyleItem {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let idx = args.first().and_then(|v| v.as_int()).unwrap_or(-1);
        if idx < 0 {
            return Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value());
        }
        let s = self.state.borrow();
        let prop = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("style"))
            .map(|style_str| {
                let styles = parse_inline_styles(style_str);
                let mut keys: Vec<&String> = styles.keys().collect();
                keys.sort();
                keys.get(idx as usize)
                    .map(|k| k.to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        Ok(rquickjs::String::from_str(ctx.clone(), &prop)?.into_value())
    }
}

pub(super) struct StyleGetPropertyPriority {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for StyleGetPropertyPriority {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let prop = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let css_prop = camel_to_kebab(&prop);
        let s = self.state.borrow();
        let priority = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("style"))
            .map(|style_str| {
                // Kolla om property har !important
                for part in style_str.split(';') {
                    let part = part.trim();
                    if let Some(colon) = part.find(':') {
                        let p = part[..colon].trim().to_lowercase();
                        if p == css_prop {
                            let v = part[colon + 1..].trim();
                            if v.ends_with("!important") || v.ends_with("! important") {
                                return "important".to_string();
                            }
                        }
                    }
                }
                String::new()
            })
            .unwrap_or_default();
        Ok(rquickjs::String::from_str(ctx.clone(), &priority)?.into_value())
    }
}

pub(super) fn make_style_object<'js>(
    ctx: &Ctx<'js>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    // Registrera Rust-backade metoder
    obj.set(
        "__setProperty",
        Function::new(
            ctx.clone(),
            JsFn(StyleSetProperty {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "__getPropertyValue",
        Function::new(
            ctx.clone(),
            JsFn(StyleGetPropertyValue {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "__removeProperty",
        Function::new(
            ctx.clone(),
            JsFn(StyleRemoveProperty {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "__getCssText",
        Function::new(
            ctx.clone(),
            JsFn(StyleGetCssText {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "__setCssText",
        Function::new(
            ctx.clone(),
            JsFn(StyleSetCssText {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "__getLength",
        Function::new(
            ctx.clone(),
            JsFn(StyleGetLength {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "__item",
        Function::new(
            ctx.clone(),
            JsFn(StyleItem {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "__getPropertyPriority",
        Function::new(
            ctx.clone(),
            JsFn(StyleGetPropertyPriority {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;

    // Wrap i JS Proxy för live get/set — global factory (eval:as en gång)
    let globals = ctx.globals();
    let proxy_fn: Function = if let Ok(f) = globals.get::<_, Function>("__makeStyleProxy") {
        f
    } else {
        let proxy_code = r#"(function() {
            function camelToKebab(s) {
                return s.replace(/[A-Z]/g, function(m){return '-'+m.toLowerCase();});
            }
            var handler_proto = {
                get: function(t, prop) {
                    if (prop === 'cssText') return t.__getCssText();
                    if (prop === 'length') return t.__getLength();
                    if (prop === 'parentRule') return null;
                    if (prop === 'setProperty') return function(p,v,pri) { return t.__setProperty(p,v||'',pri||''); };
                    if (prop === 'getPropertyValue') return function(p) { return t.__getPropertyValue(p); };
                    if (prop === 'removeProperty') return function(p) { return t.__removeProperty(p); };
                    if (prop === 'getPropertyPriority') return function(p) { return t.__getPropertyPriority(p); };
                    if (prop === 'item') return function(i) { return t.__item(i); };
                    if (prop === Symbol.toStringTag) return 'CSSStyleDeclaration';
                    if (typeof prop === 'symbol') return undefined;
                    if (typeof prop === 'number' || /^\d+$/.test(prop)) return t.__item(Number(prop));
                    var kebab = camelToKebab(String(prop));
                    return t.__getPropertyValue(kebab);
                },
                set: function(t, prop, value) {
                    if (prop === 'cssText') { t.__setCssText(String(value)); return true; }
                    var kebab = camelToKebab(String(prop));
                    if (value === '' || value === null || value === undefined) {
                        t.__removeProperty(kebab);
                    } else {
                        t.__setProperty(kebab, String(value));
                    }
                    return true;
                }
            };
            return function(target) { return new Proxy(target, handler_proto); };
        })()"#;
        let f: Function = ctx.eval(proxy_code)?;
        globals.set("__makeStyleProxy", f.clone())?;
        f
    };
    let result = proxy_fn.call::<_, Value>((obj,))?;
    Ok(result)
}

pub(super) fn kebab_to_camel(name: &str) -> String {
    let mut result = String::new();
    let mut next_upper = false;
    for ch in name.chars() {
        if ch == '-' {
            next_upper = true;
        } else if next_upper {
            result.push(ch.to_uppercase().next().unwrap_or(ch));
            next_upper = false;
        } else {
            result.push(ch);
        }
    }
    result
}
