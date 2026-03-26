// Event handling: addEventListener, removeEventListener, dispatchEvent, click, focus, blur, contains

use rquickjs::{Ctx, Function, Persistent, Value};

use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;

use super::state::{EventListener, SharedState};
use super::{extract_node_key, make_element_object, node_contains, node_key_to_f64};

pub(super) struct AddEventListenerHandler {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for AddEventListenerHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let event_type = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let func = match args.get(1).and_then(|v| v.as_function()) {
            Some(f) => f.clone(),
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let (capture, passive, once) = if let Some(opts) = args.get(2) {
            if let Some(b) = opts.as_bool() {
                (b, None, false)
            } else if let Some(obj) = opts.as_object() {
                let cap = obj
                    .get::<_, Value>("capture")
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let pas = obj
                    .get::<_, Value>("passive")
                    .ok()
                    .and_then(|v| v.as_bool());
                let onc = obj
                    .get::<_, Value>("once")
                    .ok()
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                (cap, pas, onc)
            } else {
                (false, None, false)
            }
        } else {
            (false, None, false)
        };
        let persistent = Persistent::save(ctx, func);
        let key_bits = node_key_to_f64(self.key) as u64;
        let mut s = self.state.borrow_mut();
        s.event_listeners
            .entry(key_bits)
            .or_default()
            .push(EventListener {
                event_type,
                callback: persistent,
                capture,
                passive,
                once,
            });
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct RemoveEventListenerHandler {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for RemoveEventListenerHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let event_type = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let key_bits = node_key_to_f64(self.key) as u64;
        let mut s = self.state.borrow_mut();
        if let Some(listeners) = s.event_listeners.get_mut(&key_bits) {
            listeners.retain(|l| l.event_type != event_type);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct DispatchEventHandler {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for DispatchEventHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let event_obj = args.first().and_then(|v| v.as_object());
        let event_type = event_obj
            .and_then(|obj| obj.get::<_, String>("type").ok())
            .unwrap_or_default();

        // Passive-by-default: touchstart, touchmove, wheel, mousewheel på window/document/body
        let passive_default_types = ["touchstart", "touchmove", "wheel", "mousewheel"];
        let is_passive_default = passive_default_types.contains(&event_type.as_str());

        let key_bits = node_key_to_f64(self.key) as u64;
        let callbacks: Vec<(Persistent<Function<'static>>, Option<bool>, bool)> = {
            let s = self.state.borrow();
            s.event_listeners
                .get(&key_bits)
                .map(|listeners| {
                    listeners
                        .iter()
                        .filter(|l| l.event_type == event_type)
                        .map(|l| (l.callback.clone(), l.passive, l.once))
                        .collect()
                })
                .unwrap_or_default()
        };
        // Sätt event.target och eventPhase per spec
        let target_obj = args
            .first()
            .and_then(|_| {
                // Skapa target-referens till detta element
                make_element_object(ctx, self.key, &self.state).ok()
            })
            .unwrap_or(Value::new_null(ctx.clone()));
        if let Some(ev) = event_obj {
            let _ = ev.set("target", target_obj.clone());
            let _ = ev.set("srcElement", target_obj.clone());
            let _ = ev.set("currentTarget", target_obj);
            let _ = ev.set("eventPhase", 2i32); // AT_TARGET
        }
        let mut once_indices = vec![];
        for (idx, (cb, passive, once)) in callbacks.into_iter().enumerate() {
            if let Ok(func) = cb.restore(ctx) {
                let event = args
                    .first()
                    .cloned()
                    .unwrap_or(Value::new_undefined(ctx.clone()));
                let is_passive = passive.unwrap_or(is_passive_default);
                if is_passive {
                    if let Some(obj) = event.as_object() {
                        let _ = obj.set("__passive", true);
                    }
                }
                let _ = func.call::<_, Value>((event,));
                if once {
                    once_indices.push(idx);
                }
            }
        }
        // Ta bort once-listeners efter dispatch
        if !once_indices.is_empty() {
            let mut s = self.state.borrow_mut();
            if let Some(listeners) = s.event_listeners.get_mut(&key_bits) {
                // Räkna ut vilka listeners-index som matchar (baserat på event_type och position)
                let mut type_idx = 0usize;
                let mut remove_set: Vec<usize> = vec![];
                for (i, l) in listeners.iter().enumerate() {
                    if l.event_type == event_type {
                        if once_indices.contains(&type_idx) {
                            remove_set.push(i);
                        }
                        type_idx += 1;
                    }
                }
                // Ta bort bakifrån för att bevara index
                for &i in remove_set.iter().rev() {
                    listeners.remove(i);
                }
            }
        }
        // Resätt eventPhase och propagation flags efter dispatch
        if let Some(ev) = args.first().and_then(|v| v.as_object()) {
            let _ = ev.set("eventPhase", 0i32); // NONE
            let _ = ev.set("currentTarget", Value::new_null(ctx.clone()));
            let _ = ev.set("_stopPropagationFlag", false);
            let _ = ev.set("_stopImmediatePropagationFlag", false);
        }
        let default_prevented = args
            .first()
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get::<_, bool>("defaultPrevented").ok())
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), !default_prevented))
    }
}

pub(super) struct ClickHandler {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ClickHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Skapa element och dispatcha via global helper
        let elem = make_element_object(ctx, self.key, &self.state)?;
        let code = r#"
        (function(el) {
            if (el && el.dispatchEvent) {
                var evt = new MouseEvent('click', {bubbles:true, cancelable:true, composed:true});
                el.dispatchEvent(evt);
            }
        })
        "#;
        let f: Function = ctx.eval(code)?;
        let _ = f.call::<_, Value>((elem,));
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct FocusHandler {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for FocusHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        s.focused_element = Some(node_key_to_f64(self.key) as u64);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct BlurHandler {
    pub(super) state: SharedState,
}
impl JsHandler for BlurHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        s.focused_element = None;
        Ok(Value::new_undefined(ctx.clone()))
    }
}

pub(super) struct ContainsHandler {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
}
impl JsHandler for ContainsHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let other_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => return Ok(Value::new_bool(ctx.clone(), false)),
        };
        let s = self.state.borrow();
        Ok(Value::new_bool(
            ctx.clone(),
            node_contains(&s.arena, self.key, other_key),
        ))
    }
}
