// Event handling: addEventListener, removeEventListener, dispatchEvent, click, focus, blur, contains

use rquickjs::{Ctx, Function, Persistent, Value};

/// Unik nyckel för window event listeners — separerad från document
pub(super) const WINDOW_EVENT_KEY: u64 = u64::MAX - 1;

use crate::arena_dom::NodeKey;
use crate::event_loop::JsHandler;

use super::state::{EventListener, SharedState};
use super::{extract_node_key, make_element_object, node_contains, node_key_to_f64};

pub(super) struct AddEventListenerHandler {
    pub(super) state: SharedState,
    pub(super) key: NodeKey,
    /// Om satt, använd denna istället för node_key_to_f64(key) som event listener key
    pub(super) override_key: Option<u64>,
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
        let key_bits = self
            .override_key
            .unwrap_or(node_key_to_f64(self.key) as u64);
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
    pub(super) override_key: Option<u64>,
}
impl JsHandler for RemoveEventListenerHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let event_type = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let key_bits = self
            .override_key
            .unwrap_or(node_key_to_f64(self.key) as u64);
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
        let bubbles = event_obj
            .and_then(|obj| obj.get::<_, bool>("bubbles").ok())
            .unwrap_or(false);

        // Passive-by-default: touchstart, touchmove, wheel, mousewheel
        // Per spec gäller BARA på window, document, och document.body targets
        let passive_default_types = ["touchstart", "touchmove", "wheel", "mousewheel"];
        let is_passive_event_type = passive_default_types.contains(&event_type.as_str());
        let body_key_bits = {
            let s = self.state.borrow();
            // Hitta <body> — första "body" child av document
            let doc_key = s.arena.document;
            s.arena
                .nodes
                .get(doc_key)
                .map(|doc| {
                    doc.children
                        .iter()
                        .find_map(|&child| {
                            s.arena
                                .nodes
                                .get(child)
                                .filter(|n| n.tag.as_deref() == Some("html"))
                                .and_then(|html| {
                                    html.children.iter().find_map(|&hc| {
                                        s.arena
                                            .nodes
                                            .get(hc)
                                            .filter(|n| n.tag.as_deref() == Some("body"))
                                            .map(|_| node_key_to_f64(hc) as u64)
                                    })
                                })
                        })
                        .unwrap_or(0)
                })
                .unwrap_or(0)
        };
        let doc_key_bits = {
            let s = self.state.borrow();
            node_key_to_f64(s.arena.document) as u64
        };

        // ── Bygg propagation path: target → parent → ... → document → window ──
        let mut path: Vec<u64> = Vec::new();
        // Sentinel-värde för window — skiljer från document-noden
        const WINDOW_SENTINEL: u64 = u64::MAX;
        let window_key_bits: u64;
        {
            let s = self.state.borrow();
            let mut current = Some(self.key);
            while let Some(key) = current {
                path.push(node_key_to_f64(key) as u64);
                current = s.arena.nodes.get(key).and_then(|n| n.parent);
            }
            // Window-event-listeners lagras med WINDOW_EVENT_KEY
            window_key_bits = WINDOW_EVENT_KEY;
        }
        // Lägg till window-sentinel i slutet av pathen
        if !path.is_empty() {
            path.push(WINDOW_SENTINEL);
        }
        // path[0] = target, path[last] = window sentinel
        // Capture order: root → ... → target (reversed path)
        // Bubble order: target → ... → root (path as-is)

        // Sätt event.target
        let target_val =
            make_element_object(ctx, self.key, &self.state).unwrap_or(Value::new_null(ctx.clone()));
        if let Some(ev) = event_obj {
            let _ = ev.set("target", target_val.clone());
            let _ = ev.set("srcElement", target_val);
            let _ = ev.set("_dispatching", true);
            let _ = ev.set("_stopPropagationFlag", false);
            let _ = ev.set("_stopImmediatePropagationFlag", false);
        }

        let event_val = args
            .first()
            .cloned()
            .unwrap_or(Value::new_undefined(ctx.clone()));

        // Helper: Kör listeners på en nod, returnera true om stopImmediate
        let run_listeners = |ctx: &Ctx<'js>,
                             node_bits: u64,
                             phase: i32,
                             state: &SharedState,
                             event_val: &Value<'js>,
                             event_type: &str,
                             is_passive_default: bool,
                             window_bits: u64|
         -> (bool, bool) {
            // Hämta listeners med rätt key — window sentinel → doc key
            let listener_key = if node_bits == WINDOW_SENTINEL {
                window_bits
            } else {
                node_bits
            };
            // Sätt currentTarget och eventPhase
            if let Some(ev) = event_val.as_object() {
                let key = crate::dom_bridge::f64_to_node_key(listener_key as f64);
                // Sätt currentTarget — återanvänd cachade objekt för window/document
                let is_doc_node = {
                    let s = state.borrow();
                    node_key_to_f64(s.arena.document) as u64 == node_bits
                };
                let ct_val = if node_bits == WINDOW_SENTINEL {
                    ctx.globals()
                        .get::<_, Value>("window")
                        .unwrap_or_else(|_| ctx.globals().into_value())
                } else if is_doc_node {
                    ctx.globals()
                        .get::<_, Value>("document")
                        .unwrap_or_else(|_| Value::new_null(ctx.clone()))
                } else if let Ok(ct) = make_element_object(ctx, key, state) {
                    ct
                } else {
                    Value::new_null(ctx.clone())
                };
                let _ = ev.set("currentTarget", ct_val);
                let _ = ev.set("eventPhase", phase);
            }

            let callbacks: Vec<(Persistent<Function<'static>>, Option<bool>, bool, bool)> = {
                let s = state.borrow();
                s.event_listeners
                    .get(&listener_key)
                    .map(|listeners| {
                        listeners
                            .iter()
                            .filter(|l| l.event_type == event_type)
                            .map(|l| (l.callback.clone(), l.passive, l.once, l.capture))
                            .collect()
                    })
                    .unwrap_or_default()
            };

            let mut once_to_remove = vec![];
            let mut stop_prop = false;
            let mut stop_immediate = false;

            for (idx, (cb, passive, once, capture)) in callbacks.into_iter().enumerate() {
                // Capture listeners körs bara i capture-fas (phase 1)
                // Bubble listeners körs bara i bubble-fas (phase 3)
                // AT_TARGET (phase 2): kör alla oavsett capture-flagga
                if phase == 1 && !capture {
                    continue;
                }
                if phase == 3 && capture {
                    continue;
                }

                if stop_immediate {
                    break;
                }

                if let Ok(func) = cb.restore(ctx) {
                    let is_passive = passive.unwrap_or(is_passive_default);
                    if is_passive {
                        if let Some(obj) = event_val.as_object() {
                            let _ = obj.set("__passive", true);
                        }
                    }
                    let _ = func.call::<_, Value>((event_val.clone(),));
                    if is_passive {
                        if let Some(obj) = event_val.as_object() {
                            let _ = obj.set("__passive", false);
                        }
                    }
                    if once {
                        once_to_remove.push(idx);
                    }
                }

                // Kolla propagation flags efter varje listener
                if let Some(ev) = event_val.as_object() {
                    stop_immediate = ev
                        .get::<_, bool>("_stopImmediatePropagationFlag")
                        .unwrap_or(false);
                    stop_prop = ev.get::<_, bool>("_stopPropagationFlag").unwrap_or(false);
                }
            }

            // Ta bort once-listeners
            if !once_to_remove.is_empty() {
                let mut s = state.borrow_mut();
                if let Some(listeners) = s.event_listeners.get_mut(&listener_key) {
                    let mut type_idx = 0usize;
                    let mut remove_set: Vec<usize> = vec![];
                    for (i, l) in listeners.iter().enumerate() {
                        if l.event_type == event_type {
                            if once_to_remove.contains(&type_idx) {
                                remove_set.push(i);
                            }
                            type_idx += 1;
                        }
                    }
                    for &i in remove_set.iter().rev() {
                        listeners.remove(i);
                    }
                }
            }
            (stop_prop, stop_immediate)
        };

        let mut stopped = false;

        // ── CAPTURE PHASE: root → ... → parent (exkludera target) ──
        for &node_bits in path.iter().rev() {
            if stopped {
                break;
            }
            let phase = if node_bits == path[0] { 2 } else { 1 };
            let passive_for_node = is_passive_event_type
                && (node_bits == WINDOW_SENTINEL
                    || node_bits == doc_key_bits
                    || node_bits == body_key_bits);
            let (stop_prop, _stop_imm) = run_listeners(
                ctx,
                node_bits,
                phase,
                &self.state,
                &event_val,
                &event_type,
                passive_for_node,
                window_key_bits,
            );
            if stop_prop {
                stopped = true;
            }
        }

        // ── BUBBLE PHASE: parent → ... → root (exkludera target, den kördes redan) ──
        if bubbles && !stopped {
            for &node_bits in path.iter().skip(1) {
                if stopped {
                    break;
                }
                let passive_for_node = is_passive_event_type
                    && (node_bits == WINDOW_SENTINEL
                        || node_bits == doc_key_bits
                        || node_bits == body_key_bits);
                let (stop_prop, _stop_imm) = run_listeners(
                    ctx,
                    node_bits,
                    3,
                    &self.state,
                    &event_val,
                    &event_type,
                    passive_for_node,
                    window_key_bits,
                );
                if stop_prop {
                    stopped = true;
                }
            }
        }

        // ── Cleanup: resätt event state ──
        if let Some(ev) = args.first().and_then(|v| v.as_object()) {
            let _ = ev.set("eventPhase", 0i32);
            let _ = ev.set("currentTarget", Value::new_null(ctx.clone()));
            let _ = ev.set("_dispatching", false);
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
                var evt = new PointerEvent('click', {bubbles:true, cancelable:true, composed:true, pointerId:-1, pointerType:''});
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
