// classList + style: DOMTokenList och CSSStyleDeclaration

use std::rc::Rc;

use rquickjs::{Ctx, Function, Object, Value};

use crate::arena_dom::NodeKey;
use crate::event_loop::{JsFn, JsHandler};

#[cfg(feature = "blitz")]
use super::invalidate_blitz_cache;
use super::state::SharedState;
use super::utils::{parse_inline_styles, serialize_inline_styles};
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
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    let current = node.get_attr("class").unwrap_or("").to_string();
                    // Ordered set: dedup + append om saknas
                    let mut seen = std::collections::HashSet::new();
                    let mut classes: Vec<&str> = current
                        .split_whitespace()
                        .filter(|c| seen.insert(*c))
                        .collect();
                    if !classes.contains(&cls.as_str()) {
                        classes.push(&cls);
                    }
                    node.attributes
                        .insert("class".to_string(), classes.join(" "));
                }
                return Ok(Value::new_bool(ctx.clone(), true));
            }
            // force=false → ta bort token om den finns, annars no-op
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                let current = node.get_attr("class").unwrap_or("").to_string();
                let has_token = current.split_whitespace().any(|c| c == cls);
                if has_token {
                    // Per spec: kör "ordered set remove" — normaliserar
                    let new_cls: Vec<&str> =
                        current.split_whitespace().filter(|&c| c != cls).collect();
                    node.attributes
                        .insert("class".to_string(), new_cls.join(" "));
                }
                // Om token inte fanns, skriv INTE (bevarar whitespace)
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
            let mut styles = parse_inline_styles(&style_str);
            if val.is_empty() {
                styles.remove(&css_prop);
            } else {
                styles.insert(css_prop, val);
            }
            node.attributes
                .insert("style".to_string(), serialize_inline_styles(&styles));
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
                styles.get(&css_prop).cloned().unwrap_or_default()
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
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let style_str = node.get_attr("style").unwrap_or("").to_string();
            let mut styles = parse_inline_styles(&style_str);
            styles.remove(&css_prop);
            node.attributes
                .insert("style".to_string(), serialize_inline_styles(&styles));
        }
        Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
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

pub(super) fn make_style_object<'js>(
    ctx: &Ctx<'js>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'js>> {
    let obj = Object::new(ctx.clone())?;
    obj.set(
        "setProperty",
        Function::new(
            ctx.clone(),
            JsFn(StyleSetProperty {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getPropertyValue",
        Function::new(
            ctx.clone(),
            JsFn(StyleGetPropertyValue {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "removeProperty",
        Function::new(
            ctx.clone(),
            JsFn(StyleRemoveProperty {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;

    // Sätt inline-stilar som egenskaper
    let s = state.borrow();
    let styles = s
        .arena
        .nodes
        .get(key)
        .and_then(|n| n.get_attr("style"))
        .map(parse_inline_styles)
        .unwrap_or_default();
    obj.set(
        "cssText",
        s.arena
            .nodes
            .get(key)
            .and_then(|n| n.get_attr("style"))
            .unwrap_or(""),
    )?;
    for (prop, val) in &styles {
        // Konvertera kebab-case till camelCase
        let camel = kebab_to_camel(prop);
        obj.set(camel.as_str(), val.as_str())?;
    }
    Ok(obj.into_value())
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
