// Event handling: addEventListener, removeEventListener, dispatchEvent, click, focus, blur, contains

use rquickjs::{Ctx, Function, Persistent, Value};

/// Unik nyckel för window event listeners — separerad från document
pub(super) const WINDOW_EVENT_KEY: u64 = u64::MAX - 1;

/// Konvertera JS-värde till boolean per JS truthiness-regler
fn js_truthy(v: &Value) -> bool {
    if let Some(b) = v.as_bool() {
        return b;
    }
    if v.is_null() || v.is_undefined() {
        return false;
    }
    if let Some(n) = v.as_number() {
        return n != 0.0 && !n.is_nan();
    }
    if let Some(s) = v.as_string() {
        return !s.to_string().unwrap_or_default().is_empty();
    }
    // Objects, functions, arrays → truthy
    true
}

/// (registrerings-index, callback, passive, once, capture, callback_id)
type ListenerEntry = (
    usize,
    Persistent<Function<'static>>,
    Option<bool>,
    bool,
    bool,
    u64,
);

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
            if let Some(obj) = opts.as_object() {
                let cap = obj
                    .get::<_, Value>("capture")
                    .ok()
                    .map(|v| js_truthy(&v))
                    .unwrap_or(false);
                let pas = obj
                    .get::<_, Value>("passive")
                    .ok()
                    .map(|v| {
                        if v.is_undefined() {
                            None
                        } else {
                            Some(js_truthy(&v))
                        }
                    })
                    .unwrap_or(None);
                let onc = obj
                    .get::<_, Value>("once")
                    .ok()
                    .map(|v| js_truthy(&v))
                    .unwrap_or(false);
                (cap, pas, onc)
            } else {
                // Boolean/number/string → JS truthiness
                (js_truthy(opts), None, false)
            }
        } else {
            (false, None, false)
        };
        // Tilldela unik callback_id till funktionen (för removeEventListener-matchning)
        // Function.0 är ett Object — vi kan accessa det via into_value + as_object
        let func_val: Value = func.clone().into_value();
        let callback_id: u64 = if let Some(func_obj) = func_val.as_object() {
            if let Ok(existing_id) = func_obj.get::<_, f64>("__ael_id") {
                existing_id as u64
            } else {
                let mut s = self.state.borrow_mut();
                let id = s.next_callback_id;
                s.next_callback_id += 1;
                drop(s);
                let _ = func_obj.set("__ael_id", id as f64);
                id
            }
        } else {
            let mut s = self.state.borrow_mut();
            let id = s.next_callback_id;
            s.next_callback_id += 1;
            id
        };
        let persistent = Persistent::save(ctx, func);
        let key_bits = self
            .override_key
            .unwrap_or(node_key_to_f64(self.key) as u64);
        let mut s = self.state.borrow_mut();
        // Per spec: om samma callback+capture redan finns, ignorera (deduplicering)
        let listeners = s.event_listeners.entry(key_bits).or_default();
        let already_exists = listeners.iter().any(|l| {
            l.event_type == event_type && l.callback_id == callback_id && l.capture == capture
        });
        if !already_exists {
            listeners.push(EventListener {
                event_type,
                callback: persistent,
                capture,
                passive,
                once,
                callback_id,
            });
        }
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
        // Hämta callback_id från funktionens __ael_id property
        let callback_id: Option<u64> = args
            .get(1)
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get::<_, f64>("__ael_id").ok())
            .map(|id| id as u64);
        let capture = if let Some(opts) = args.get(2) {
            if let Some(obj) = opts.as_object() {
                obj.get::<_, Value>("capture")
                    .ok()
                    .map(|v| js_truthy(&v))
                    .unwrap_or(false)
            } else {
                js_truthy(opts)
            }
        } else {
            false
        };
        let key_bits = self
            .override_key
            .unwrap_or(node_key_to_f64(self.key) as u64);
        let mut s = self.state.borrow_mut();
        if let Some(listeners) = s.event_listeners.get_mut(&key_bits) {
            if let Some(cb_id) = callback_id {
                // Matcha per spec: event_type + callback_id + capture
                listeners.retain(|l| {
                    !(l.event_type == event_type && l.callback_id == cb_id && l.capture == capture)
                });
            } else {
                // Inget callback — ta bort alla av den typen (fallback)
                listeners.retain(|l| l.event_type != event_type);
            }
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
        // Spec: dispatchEvent(null) kastar TypeError
        let first = args.first();
        if first.is_none() || first.is_some_and(|v| v.is_null() || v.is_undefined()) {
            return Err(ctx.throw(
                rquickjs::String::from_str(
                    ctx.clone(),
                    "TypeError: Failed to execute 'dispatchEvent': parameter 1 is not of type 'Event'.",
                )?
                .into(),
            ));
        }
        let event_obj = first.and_then(|v| v.as_object());
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
            // OBS: Resätt INTE _stopPropagationFlag/_stopImmediatePropagationFlag
            // Per spec: om stopPropagation() anropades före dispatch, ska flaggan respekteras

            // Bygg composedPath: target → ... → document → window
            if let Ok(cp_arr) = rquickjs::Array::new(ctx.clone()) {
                let mut cp_idx: usize = 0;
                for &node_bits in &path {
                    if node_bits == WINDOW_SENTINEL {
                        // Lägg till window-objektet
                        if let Ok(win) = ctx.globals().get::<_, Value>("window") {
                            let _ = cp_arr.set(cp_idx, win);
                            cp_idx += 1;
                        }
                    } else {
                        let nk = crate::dom_bridge::f64_to_node_key(node_bits as f64);
                        if let Ok(obj) = make_element_object(ctx, nk, &self.state) {
                            let _ = cp_arr.set(cp_idx, obj);
                            cp_idx += 1;
                        }
                    }
                }
                let _ = ev.set("_composedPath", cp_arr);
            }
        }

        // Sätt window.event (legacy) under dispatch
        let prev_window_event = ctx.globals().get::<_, Value>("event").ok();
        if let Some(ev_val) = args.first() {
            let _ = ctx.globals().set("event", ev_val.clone());
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

            let callbacks: Vec<ListenerEntry> = {
                let s = state.borrow();
                s.event_listeners
                    .get(&listener_key)
                    .map(|listeners| {
                        listeners
                            .iter()
                            .enumerate()
                            .filter(|(_, l)| l.event_type == event_type)
                            .map(|(i, l)| {
                                (
                                    i,
                                    l.callback.clone(),
                                    l.passive,
                                    l.once,
                                    l.capture,
                                    l.callback_id,
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default()
            };

            // AT_TARGET (phase 2): capture-listeners körs först, sedan bubble
            // (target nås under capture-fasen FÖRE bubble-fasen)
            let sorted_callbacks: Vec<ListenerEntry> = if phase == 2 {
                let mut capture_list: Vec<_> = callbacks.iter().filter(|c| c.4).cloned().collect();
                let mut bubble_list: Vec<_> = callbacks.iter().filter(|c| !c.4).cloned().collect();
                capture_list.append(&mut bubble_list);
                capture_list
            } else {
                callbacks
            };

            let mut once_to_remove = vec![];
            let mut stop_prop = false;
            let mut stop_immediate = false;

            // Flagga: har vi växlat från capture→bubble vid AT_TARGET?
            let mut prev_was_capture = true;

            for (reg_idx, cb, passive, once, capture, cb_id) in sorted_callbacks {
                // Capture listeners körs bara i capture-fas (phase 1)
                // Bubble listeners körs bara i bubble-fas (phase 3)
                // AT_TARGET (phase 2): kör alla (redan sorterade ovan)
                if phase == 1 && !capture {
                    continue;
                }
                if phase == 3 && capture {
                    continue;
                }

                if stop_immediate {
                    break;
                }

                // AT_TARGET: om vi växlar från capture→bubble, kolla stopPropagation
                if phase == 2 && prev_was_capture && !capture && stop_prop {
                    break;
                }
                prev_was_capture = capture;

                // Per spec: kontrollera att listenern fortfarande finns i live-listan
                // (en tidigare listener kan ha anropat removeEventListener)
                {
                    let s = state.borrow();
                    let still_exists = s
                        .event_listeners
                        .get(&listener_key)
                        .map(|ls| {
                            ls.iter()
                                .any(|l| l.callback_id == cb_id && l.event_type == event_type)
                        })
                        .unwrap_or(false);
                    if !still_exists {
                        continue;
                    }
                }

                if let Ok(func) = cb.restore(ctx) {
                    let is_passive = passive.unwrap_or(is_passive_default);
                    if is_passive {
                        if let Some(obj) = event_val.as_object() {
                            let _ = obj.set("__passive", true);
                        }
                    }
                    let call_result = func.call::<_, Value>((event_val.clone(),));
                    if let Err(e) = call_result {
                        // Per spec: rapportera exception till window.onerror och fortsätt
                        let err_msg = format!("{}", e);
                        if let Ok(onerror) = ctx.globals().get::<_, Value>("onerror") {
                            if let Some(onerror_fn) = onerror.as_function() {
                                let _ = onerror_fn.call::<_, Value>((rquickjs::String::from_str(
                                    ctx.clone(),
                                    &err_msg,
                                )
                                .map(|s| s.into_value())
                                .unwrap_or(Value::new_undefined(ctx.clone())),));
                            }
                        }
                    }
                    if is_passive {
                        if let Some(obj) = event_val.as_object() {
                            let _ = obj.set("__passive", false);
                        }
                    }
                    if once {
                        once_to_remove.push(reg_idx);
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

            // Ta bort once-listeners (once_to_remove innehåller globala index i listeners-vektorn)
            if !once_to_remove.is_empty() {
                let mut s = state.borrow_mut();
                if let Some(listeners) = s.event_listeners.get_mut(&listener_key) {
                    once_to_remove.sort_unstable();
                    once_to_remove.dedup();
                    for &i in once_to_remove.iter().rev() {
                        if i < listeners.len() {
                            listeners.remove(i);
                        }
                    }
                }
            }
            (stop_prop, stop_immediate)
        };

        // Kolla om stopPropagation anropades före dispatch
        let mut stopped = event_obj
            .and_then(|ev| ev.get::<_, bool>("_stopPropagationFlag").ok())
            .unwrap_or(false);

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

        // Återställ window.event
        match prev_window_event {
            Some(prev) => {
                let _ = ctx.globals().set("event", prev);
            }
            None => {
                let _ = ctx
                    .globals()
                    .set("event", Value::new_undefined(ctx.clone()));
            }
        }

        // ── Cleanup: resätt event state per spec ──
        // OBS: defaultPrevented, target, srcElement ska BEVARAS efter dispatch
        if let Some(ev) = args.first().and_then(|v| v.as_object()) {
            let _ = ev.set("eventPhase", 0i32);
            let _ = ev.set("currentTarget", Value::new_null(ctx.clone()));
            let _ = ev.set("_dispatching", false);
            let _ = ev.set("_stopPropagationFlag", false);
            let _ = ev.set("_stopImmediatePropagationFlag", false);
            // Rensa composedPath (per spec: tom array efter dispatch)
            if let Ok(empty_arr) = rquickjs::Array::new(ctx.clone()) {
                let _ = ev.set("_composedPath", empty_arr);
            }
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
