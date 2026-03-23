/// QuickJS DOM Bridge — Fas 17.4-17.6
///
/// Exponerar ArenaDom som `document`/`window`-objekt i QuickJS-kontexten.
/// Fullständig DOM-implementation med 40+ metoder:
///
/// Document: getElementById, querySelector, querySelectorAll, createElement,
///   createTextNode, createComment, createDocumentFragment,
///   getElementsByClassName, getElementsByTagName, body, head, documentElement
/// Element: getAttribute, setAttribute, removeAttribute, textContent,
///   innerHTML, id, className, tagName, classList, style, dataset,
///   outerHTML, closest, matches, children, firstElementChild,
///   nextElementSibling, cloneNode
/// Node: appendChild, removeChild, parentNode, childNodes, firstChild,
///   nextSibling, nodeType
/// Window: getComputedStyle (stubbad)
/// CSS Selectors: #id, .class, tag, tag.class, [attr], [attr="val"],
///   div > span (child), div span (descendant), :first-child, komma-sep
use rquickjs::{function::Rest, object::Accessor, Ctx, Function, Object, Persistent, Value};

use std::cell::RefCell;
use std::rc::Rc;

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};
use crate::event_loop::{self, EventLoopState, SharedEventLoop};

/// Resultat från DOM-medveten JS-evaluering
#[derive(Debug, Clone)]
pub struct DomEvalResult {
    /// Eventuellt returvärde som sträng
    pub value: Option<String>,
    /// Felmeddelande om evalueringen misslyckades
    pub error: Option<String>,
    /// Lista av DOM-mutationer som JS:en utförde
    pub mutations: Vec<DomMutation>,
    /// Exekveringstid i mikrosekunder
    pub eval_time_us: u64,
    /// Event-loop-statistik (ticks, timers, rAF)
    pub event_loop_ticks: usize,
    /// Antal timer-callbacks som kördes
    pub timers_fired: usize,
}

/// En mutation som JS-koden utförde på DOM:en (placeholder för framtida mutation-tracking)
pub type DomMutation = String;

// ─── Delad state mellan JS-callbacks ────────────────────────────────────────

/// En registrerad event listener på ett DOM-element
struct EventListener {
    event_type: String,
    callback: Persistent<Function<'static>>,
    capture: bool,
}

struct BridgeState {
    arena: ArenaDom,
    mutations: Vec<DomMutation>,
    /// Event listeners per nod (NodeKey ffi-index → listeners)
    event_listeners: std::collections::HashMap<u64, Vec<EventListener>>,
    /// Vilken nod har fokus (NodeKey ffi-index)
    focused_element: Option<u64>,
    /// Scroll-positioner per nod (NodeKey ffi-index → (scrollTop, scrollLeft))
    scroll_positions: std::collections::HashMap<u64, (f64, f64)>,
    /// CSS Cascade Engine — lazy-initialiserad vid första getComputedStyle()
    css_context: Option<crate::css_cascade::CssContext>,
    /// In-memory localStorage (sandboxad, ingen persistens)
    local_storage: std::collections::HashMap<String, String>,
    /// In-memory sessionStorage (sandboxad, ingen persistens)
    session_storage: std::collections::HashMap<String, String>,
    /// Fångade console-meddelanden
    console_output: Vec<String>,
}

type SharedState = Rc<RefCell<BridgeState>>;

// ─── Huvudfunktion ──────────────────────────────────────────────────────────

/// Evaluera JS-kod med tillgång till DOM via ArenaDom
///
/// Sätter upp `document` och `window` som globala objekt i QuickJS-kontexten.
/// Returnerar eventuellt returvärde och alla DOM-mutationer.
pub fn eval_js_with_dom(code: &str, arena: ArenaDom) -> DomEvalResult {
    let start = std::time::Instant::now();

    // Säkerhetskontroll: blockera farliga operationer
    // setTimeout/setInterval tillåts nu — hanteras av event-loopen med begränsningar
    let lower = code.to_lowercase();
    for forbidden in &[
        "fetch(",
        "xmlhttp",
        "import(",
        "require(",
        "eval(",
        "new worker",
        "indexeddb",
    ] {
        if lower.contains(forbidden) {
            return DomEvalResult {
                value: None,
                error: Some(format!(
                    "Blocked: '{}' is not allowed in sandbox",
                    forbidden.trim_end_matches('(')
                )),
                mutations: vec![],
                eval_time_us: start.elapsed().as_micros() as u64,
                event_loop_ticks: 0,
                timers_fired: 0,
            };
        }
    }

    let state: SharedState = Rc::new(RefCell::new(BridgeState {
        arena,
        mutations: vec![],
        event_listeners: std::collections::HashMap::new(),
        focused_element: None,
        scroll_positions: std::collections::HashMap::new(),
        css_context: None,
        local_storage: std::collections::HashMap::new(),
        session_storage: std::collections::HashMap::new(),
        console_output: Vec::new(),
    }));

    let mut ctx = {
        let (_, c) = crate::js_eval::create_sandboxed_runtime(); /* TODO */
    };

    // Registrera event-loop (setTimeout, setInterval, rAF, MutationObserver, queueMicrotask)
    let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
    event_loop::register_event_loop(&mut ctx, Rc::clone(&el));

    // Registrera document-objekt
    register_document(&mut ctx, Rc::clone(&state));

    // Registrera window-objekt
    register_window(&mut ctx, Rc::clone(&state));

    // Registrera console (fångar output)
    register_console(&mut ctx, Rc::clone(&state));

    match ctx.eval::<Value, _>(code) {
        Ok(result) => {
            let value_str = result
                .to_string(&mut ctx)
                .map_or_else(|_| "undefined".to_string(), |v| v.to_std_string_escaped());

            // Kör event-loopen: dränera microtasks, timers, rAF, MutationObservers
            let loop_stats = event_loop::run_event_loop(&mut ctx, &el);
            let (ticks, timers) = match &loop_stats {
                Ok(s) => (s.ticks, s.timers_fired),
                Err(_) => (0, 0),
            };

            let mutations = state.borrow().mutations.clone();

            DomEvalResult {
                value: if value_str == "undefined" {
                    None
                } else {
                    Some(value_str)
                },
                error: loop_stats.err(),
                mutations,
                eval_time_us: start.elapsed().as_micros() as u64,
                event_loop_ticks: ticks,
                timers_fired: timers,
            }
        }
        Err(e) => DomEvalResult {
            value: None,
            error: Some(format!("{:?}", e)),
            mutations: state.borrow().mutations.clone(),
            eval_time_us: start.elapsed().as_micros() as u64,
            event_loop_ticks: 0,
            timers_fired: 0,
        },
    }
}

/// Resultat med modifierad ArenaDom — för render_with_js-pipeline
#[cfg(feature = "blitz")]
pub struct DomEvalWithArena {
    pub result: DomEvalResult,
    pub arena: ArenaDom,
}

/// Evaluera JS med DOM-access och returnera den modifierade ArenaDom
///
/// Samma som `eval_js_with_dom` men ger tillbaka arena efter JS-evaluering
/// så att anroparen kan serialisera den modifierade DOM:en.
#[cfg(feature = "blitz")]
pub fn eval_js_with_dom_and_arena(code: &str, arena: ArenaDom) -> DomEvalWithArena {
    let start = std::time::Instant::now();

    // Säkerhetskontroll
    let lower = code.to_lowercase();
    for forbidden in &[
        "fetch(",
        "xmlhttp",
        "import(",
        "require(",
        "eval(",
        "new worker",
        "indexeddb",
    ] {
        if lower.contains(forbidden) {
            return DomEvalWithArena {
                result: DomEvalResult {
                    value: None,
                    error: Some(format!(
                        "Blocked: '{}' is not allowed in sandbox",
                        forbidden.trim_end_matches('(')
                    )),
                    mutations: vec![],
                    eval_time_us: start.elapsed().as_micros() as u64,
                    event_loop_ticks: 0,
                    timers_fired: 0,
                },
                arena,
            };
        }
    }

    let state: SharedState = Rc::new(RefCell::new(BridgeState {
        arena,
        mutations: vec![],
        event_listeners: std::collections::HashMap::new(),
        focused_element: None,
        scroll_positions: std::collections::HashMap::new(),
        css_context: None,
        local_storage: std::collections::HashMap::new(),
        session_storage: std::collections::HashMap::new(),
        console_output: Vec::new(),
    }));

    let mut ctx = {
        let (_, c) = crate::js_eval::create_sandboxed_runtime(); /* TODO */
    };
    let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
    event_loop::register_event_loop(&mut ctx, Rc::clone(&el));
    register_document(&mut ctx, Rc::clone(&state));
    register_window(&mut ctx, Rc::clone(&state));
    register_console(&mut ctx, Rc::clone(&state));

    let result = match ctx.eval::<Value, _>(code) {
        Ok(res) => {
            let value_str = res
                .to_string(&mut ctx)
                .map_or_else(|_| "undefined".to_string(), |v| v.to_std_string_escaped());

            let loop_stats = event_loop::run_event_loop(&mut ctx, &el);
            let (ticks, timers) = match &loop_stats {
                Ok(s) => (s.ticks, s.timers_fired),
                Err(_) => (0, 0),
            };

            let mutations = state.borrow().mutations.clone();

            DomEvalResult {
                value: if value_str == "undefined" {
                    None
                } else {
                    Some(value_str)
                },
                error: loop_stats.err(),
                mutations,
                eval_time_us: start.elapsed().as_micros() as u64,
                event_loop_ticks: ticks,
                timers_fired: timers,
            }
        }
        Err(e) => DomEvalResult {
            value: None,
            error: Some(format!("{:?}", e)),
            mutations: state.borrow().mutations.clone(),
            eval_time_us: start.elapsed().as_micros() as u64,
            event_loop_ticks: 0,
            timers_fired: 0,
        },
    };

    // Extrahera arena från SharedState — ta tillbaka ägandet
    // Rc::try_unwrap fungerar eftersom ctx (med alla Rc-kloner) droppas ovan
    drop(ctx);
    let bridge = match Rc::try_unwrap(state) {
        Ok(cell) => cell.into_inner(),
        Err(rc) => {
            // Fallback: klona arena om det finns kvarvarande referenser
            let borrowed = rc.borrow();
            BridgeState {
                arena: borrowed.arena.clone(),
                mutations: borrowed.mutations.clone(),
                event_listeners: std::collections::HashMap::new(),
                focused_element: borrowed.focused_element,
                scroll_positions: std::collections::HashMap::new(),
                css_context: None,
                local_storage: std::collections::HashMap::new(),
                session_storage: std::collections::HashMap::new(),
                console_output: Vec::new(),
            }
        }
    };

    DomEvalWithArena {
        result,
        arena: bridge.arena,
    }
}

// ─── Document-objekt ────────────────────────────────────────────────────────

fn register_document(ctx: &Ctx<'_>, state: SharedState) -> rquickjs::Result<()> {
    let state_gbi = Rc::clone(&state);
    let state_qs = Rc::clone(&state);
    let state_qsa = Rc::clone(&state);
    let state_ce = Rc::clone(&state);
    let state_ct = Rc::clone(&state);
    let state_cc = Rc::clone(&state);
    let state_gcn = Rc::clone(&state);
    let state_gtn = Rc::clone(&state);

    // SAFETY: Closures capture Rc<RefCell<BridgeState>> som ej är Send/Sync,
    // men QuickJS-kontexten är single-threaded och closures lever inom samma tråd.
    let get_element_by_id = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let id = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_string(ctx)?;
            let id_str = id.to_std_string_escaped();
            let key = {
                let s = state_gbi.borrow();
                find_by_attr_value(&s.arena, "id", &id_str)
            };
            match key {
                Some(k) => Ok(make_element_object(ctx, k, &state_gbi)),
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();

    let query_selector = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let selector = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_string(ctx)?;
            let sel_str = selector.to_std_string_escaped();
            let key = {
                let s = state_qs.borrow();
                query_select_one(&s.arena, &sel_str)
            };
            match key {
                Some(k) => Ok(make_element_object(ctx, k, &state_qs)),
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();

    let query_selector_all = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let selector = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_string(ctx)?;
            let sel_str = selector.to_std_string_escaped();
            let keys = {
                let s = state_qsa.borrow();
                query_select_all(&s.arena, &sel_str)
            };
            let array = rquickjs::Array::new(ctx.clone()).unwrap();
            for key in keys {
                let elem = make_element_object(ctx, key, &state_qsa);
                array.set(0, elem)?;
            }
            Ok(array.into_value())
        },
    )
    .unwrap();

    let create_element = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let tag = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_string(ctx)?;
            let tag_str = tag.to_std_string_escaped().to_lowercase();
            let key = {
                let mut s = state_ce.borrow_mut();
                s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Element,
                    tag: Some(tag_str),
                    attributes: std::collections::HashMap::new(),
                    text: None,
                    parent: None,
                    children: vec![],
                })
            };
            Ok(make_element_object(ctx, key, &state_ce))
        },
    )
    .unwrap();

    let create_text_node = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let text = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_string(ctx)?;
            let text_str = text.to_std_string_escaped();
            let key = {
                let mut s = state_ct.borrow_mut();
                s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: std::collections::HashMap::new(),
                    text: Some(text_str),
                    parent: None,
                    children: vec![],
                })
            };
            Ok(make_element_object(ctx, key, &state_ct))
        },
    )
    .unwrap();

    // createComment(text)
    let create_comment = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let text = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_string(ctx)?;
            let text_str = text.to_std_string_escaped();
            let key = {
                let mut s = state_cc.borrow_mut();
                s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Comment,
                    tag: None,
                    attributes: std::collections::HashMap::new(),
                    text: Some(text_str),
                    parent: None,
                    children: vec![],
                })
            };
            Ok(make_element_object(ctx, key, &state_cc))
        },
    )
    .unwrap();

    // getElementsByClassName(cls)
    let get_by_class = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let cls = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let keys = {
                let s = state_gcn.borrow();
                let mut results = vec![];
                find_all_by_class(&s.arena, s.arena.document, &cls, &mut results);
                results
            };
            let array = rquickjs::Array::new(ctx.clone()).unwrap();
            for key in keys {
                let elem = make_element_object(ctx, key, &state_gcn);
                array.set(0, elem)?;
            }
            Ok(array.into_value())
        },
    )
    .unwrap();

    // getElementsByTagName(tag)
    let get_by_tag = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let tag = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let keys = {
                let s = state_gtn.borrow();
                let mut results = vec![];
                find_all_by_tag(&s.arena, s.arena.document, &tag, &mut results);
                results
            };
            let array = rquickjs::Array::new(ctx.clone()).unwrap();
            for key in keys {
                let elem = make_element_object(ctx, key, &state_gtn);
                array.set(0, elem)?;
            }
            Ok(array.into_value())
        },
    )
    .unwrap();

    // createDocumentFragment
    let state_cdf = Rc::clone(&state);
    let create_doc_fragment = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let key = {
                let mut s = state_cdf.borrow_mut();
                s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Other, // Fragment
                    tag: None,
                    attributes: std::collections::HashMap::new(),
                    text: None,
                    parent: None,
                    children: vec![],
                })
            };
            Ok(make_element_object(ctx, key, &state_cdf))
        },
    )
    .unwrap();

    // Bygg document-objektet
    let doc = Object::new(ctx.clone()).unwrap();
    doc.set("getElementById", get_element_by_id).unwrap_or(());
    doc.set("querySelector", query_selector).unwrap_or(());
    doc.set("querySelectorAll", query_selector_all)
        .unwrap_or(());
    doc.set("createElement", create_element).unwrap_or(());
    doc.set("createTextNode", create_text_node).unwrap_or(());
    doc.set("createComment", create_comment).unwrap_or(());
    doc.set("createDocumentFragment", create_doc_fragment)
        .unwrap_or(());
    doc.set("getElementsByClassName", get_by_class)
        .unwrap_or(());
    doc.set("getElementsByTagName", get_by_tag).unwrap_or(());

    // body, head, documentElement
    let (body_key, head_key, html_key) = {
        let s = state.borrow();
        let doc_key = s.arena.document;
        (
            find_by_tag_name(&s.arena, doc_key, "body"),
            find_by_tag_name(&s.arena, doc_key, "head"),
            find_by_tag_name(&s.arena, doc_key, "html"),
        )
    };

    if let Some(key) = body_key {
        let body_obj = make_element_object(ctx, key, &state);
        doc.set("body", body_obj).unwrap_or(());
    }
    if let Some(key) = head_key {
        let head_obj = make_element_object(ctx, key, &state);
        doc.set("head", head_obj).unwrap_or(());
    }
    if let Some(key) = html_key {
        let html_obj = make_element_object(ctx, key, &state);
        doc.set("documentElement", html_obj).unwrap_or(());
    }

    // createRange() — grundläggande Range API för rich-text editors
    let create_range_fn =
        Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let collapse_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let select_node_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let select_node_contents_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let set_start_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let set_end_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let set_start_before_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let set_end_after_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let clone_range_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    // Returnera nytt Range-liknande objekt
                    let inner = Object::new(ctx.clone())
                        .unwrap()
                        .set("collapsed", Value::new_bool(ctx.clone(), true))
                        .set("startOffset", Value::new_int(ctx.clone(), 0))
                        .set("endOffset", Value::new_int(ctx.clone(), 0));
                    Ok(inner.into_value())
                });
            let delete_contents_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let to_string_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(rquickjs::String::from_str(ctx.clone(), "")
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())))
                })
                .unwrap();
            let get_bounding_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    let rect = Object::new(ctx.clone())
                        .unwrap()
                        .set("x", Value::new_int(ctx.clone(), 0))
                        .set("y", Value::new_int(ctx.clone(), 0))
                        .set("width", Value::new_int(ctx.clone(), 0))
                        .set("height", Value::new_int(ctx.clone(), 0))
                        .set("top", Value::new_int(ctx.clone(), 0))
                        .set("right", Value::new_int(ctx.clone(), 0))
                        .set("bottom", Value::new_int(ctx.clone(), 0))
                        .set("left", Value::new_int(ctx.clone(), 0));
                    Ok(rect.into_value())
                });

            let range = Object::new(ctx.clone())
                .unwrap()
                .set("collapsed", Value::new_bool(ctx.clone(), true))
                .set("startContainer", Value::new_null(ctx.clone()))
                .set("endContainer", Value::new_null(ctx.clone()))
                .set("startOffset", Value::new_int(ctx.clone(), 0))
                .set("endOffset", Value::new_int(ctx.clone(), 0))
                .set("commonAncestorContainer", Value::new_null(ctx.clone()));
            doc.set("collapse", collapse_fn).unwrap_or(());
            doc.set("selectNode", select_node_fn).unwrap_or(());
            doc.set("selectNodeContents", select_node_contents_fn)
                .unwrap_or(());
            doc.set("setStart", set_start_fn).unwrap_or(());
            doc.set("setEnd", set_end_fn).unwrap_or(());
            doc.set("setStartBefore", set_start_before_fn).unwrap_or(());
            doc.set("setEndAfter", set_end_after_fn).unwrap_or(());
            doc.set("cloneRange", clone_range_fn).unwrap_or(());
            doc.set("deleteContents", delete_contents_fn).unwrap_or(());
            doc.set("toString", to_string_fn).unwrap_or(());
            doc.set("getBoundingClientRect", get_bounding_fn)
                .unwrap_or(());

            Ok(range.into_value())
        });
    doc.set("createRange", create_range_fn, false, ctx)
        .unwrap_or(());

    // getSelection() — grundläggande Selection API
    let get_selection_fn =
        Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let remove_all_ranges =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let add_range_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let collapse_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let collapse_to_start =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let collapse_to_end =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let to_string_fn =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(rquickjs::String::from_str(ctx.clone(), "")
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())))
                })
                .unwrap();

            let selection = Object::new(ctx.clone()).unwrap();
            doc.set("anchorNode", Value::new_null(ctx.clone()))
                .unwrap_or(())
                .set("anchorOffset", Value::new_int(ctx.clone(), 0));
            doc.set("focusNode", Value::new_null(ctx.clone()))
                .unwrap_or(())
                .set("focusOffset", Value::new_int(ctx.clone(), 0))
                .set("isCollapsed", Value::new_bool(ctx.clone(), true))
                .set("rangeCount", Value::new_int(ctx.clone(), 0))
                .set(
                    "type",
                    rquickjs::String::from_str(ctx.clone(), "None")
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                );
            doc.set("removeAllRanges", remove_all_ranges).unwrap_or(());
            doc.set("addRange", add_range_fn).unwrap_or(());
            doc.set("collapse", collapse_fn).unwrap_or(());
            doc.set("collapseToStart", collapse_to_start).unwrap_or(());
            doc.set("collapseToEnd", collapse_to_end).unwrap_or(());
            doc.set("toString", to_string_fn).unwrap_or(());

            Ok(selection.into_value())
        });
    doc.set("getSelection", get_selection_fn, false, ctx)
        .unwrap_or(());

    // exitPointerLock()
    let exit_pl = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_undefined(ctx))
    })
    .unwrap();
    doc.set("exitPointerLock", exit_pl, false, ctx)
        .unwrap_or(());

    // activeElement — returnerar det element som har fokus (default: body)
    let st_ae = Rc::clone(&state);
    let active_element_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let focused = st_ae.borrow().focused_element;
            match focused {
                Some(key_u64) => {
                    let nk = f64_to_node_key(key_u64 as f64);
                    let exists = st_ae.borrow().arena.nodes.get(nk).is_some();
                    if exists {
                        Ok(make_element_object(ctx, nk, &st_ae))
                    } else {
                        Ok(Value::new_null(ctx.clone()))
                    }
                }
                None => {
                    let body_key = {
                        let s = st_ae.borrow();
                        find_by_tag_name(&s.arena, s.arena.document, "body")
                    };
                    match body_key {
                        Some(bk) => Ok(make_element_object(ctx, bk, &st_ae)),
                        None => Ok(Value::new_null(ctx.clone())),
                    }
                }
            }
        },
    )
    .unwrap();
    doc.set("activeElement", active_element_fn, false, ctx)
        .unwrap_or(());

    ctx.globals().set("document", doc).unwrap_or(());
}

// ─── Element-objekt ─────────────────────────────────────────────────────────

/// Extrahera NodeKey från ett JS-element-objekt via __nodeKey__
fn extract_node_key(val: &Value<'_>) -> Option<NodeKey> {
    let obj = val.as_object()?;
    let bits = obj.get("__nodeKey__", ctx).ok()?.to_number(ctx).ok()?;
    Some(f64_to_node_key(bits))
}

/// Konvertera f64 tillbaka till NodeKey
fn f64_to_node_key(bits: f64) -> NodeKey {
    use slotmap::KeyData;
    NodeKey::from(KeyData::from_ffi(bits as u64))
}

/// Skapa ett JS-objekt som representerar ett DOM-element med full funktionalitet.
///
/// Alla metoder har tillgång till arena via SharedState-closures.
fn make_element_object(
    ctx: &Ctx<'_>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'_>> {
    let key_bits = node_key_to_f64(key);

    // Bestäm nodeType och tagName från arena
    let (node_type_val, tag_name, id_val, class_val) = {
        let s = state.borrow();
        let node = s.arena.nodes.get(key);
        let nt = match node.map(|n| &n.node_type) {
            Some(NodeType::Element) => 1,
            Some(NodeType::Text) => 3,
            Some(NodeType::Comment) => 8,
            Some(NodeType::Document) => 9,
            _ => 1,
        };
        let tag = node
            .and_then(|n| n.tag.as_ref())
            .map(|t| t.to_uppercase())
            .unwrap_or_default();
        let id = node
            .and_then(|n| n.get_attr("id"))
            .unwrap_or("")
            .to_string();
        let cls = node
            .and_then(|n| n.get_attr("class"))
            .unwrap_or("")
            .to_string();
        (nt, tag, id, cls)
    };

    let obj = Object::new(ctx.clone())
        .unwrap()
        .set(
            "__nodeKey__",
            Value::new_float(ctx.clone(), key_bits as f64),
        )
        .set("nodeType", Value::new_int(ctx.clone(), node_type_val))
        .set(
            "tagName",
            rquickjs::String::from_str(
                ctx.clone(),
                tag_name
                    .as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ),
        )
        .set(
            "id",
            rquickjs::String::from_str(
                ctx.clone(),
                id_val
                    .as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ),
        )
        .set(
            "className",
            rquickjs::String::from_str(
                ctx.clone(),
                class_val
                    .as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ),
        );

    // ─── getAttribute(name) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let ga = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let name = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_string(ctx)?;
            let name_str = name.to_std_string_escaped();
            let s = st.borrow();
            match s.arena.nodes.get(key).and_then(|n| n.get_attr(&name_str)) {
                Some(v) => Ok(rquickjs::String::from_str(ctx.clone(), v)
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone()))),
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();
    obj.set("getAttribute", ga, false, ctx).unwrap_or(());

    // ─── setAttribute(name, value) ──────────────────────────────────
    let st = Rc::clone(state);
    let sa = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let value = args
                .get_or_undefined(1)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            s.arena.set_attr(key, &name, &value);
            s.mutations
                .push(format!("setAttribute({}, {})", name, value));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("setAttribute", sa, false, ctx).unwrap_or(());

    // ─── removeAttribute(name) ──────────────────────────────────────
    let st = Rc::clone(state);
    let ra = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            s.arena.remove_attr(key, &name);
            s.mutations.push(format!("removeAttribute({})", name));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("removeAttribute", ra, false, ctx).unwrap_or(());

    // ─── textContent (accessor property: getter + setter) ──────────
    {
        let st_get = Rc::clone(state);
        let tc_get = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                let s = st_get.borrow();
                let text = s.arena.extract_text(key);
                Ok(rquickjs::String::from_str(
                    ctx.clone(),
                    text.as_str()
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                ))
            },
        )
        .unwrap();
        let st_set = Rc::clone(state);
        let tc_set = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
                let text = args
                    .get_or_undefined(0)
                    .to_string(ctx)?
                    .to_std_string_escaped();
                let mut s = st_set.borrow_mut();
                // Rensa befintliga barn
                if let Some(node) = s.arena.nodes.get(key) {
                    let children: Vec<NodeKey> = node.children.clone();
                    for child in children {
                        s.arena.remove_child(key, child);
                    }
                }
                // Skapa ny textnod
                let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: std::collections::HashMap::new(),
                    text: Some(text),
                    parent: None,
                    children: vec![],
                });
                s.arena.append_child(key, text_key);
                s.mutations.push("setTextContent".to_string());
                Ok(Value::new_undefined(ctx.clone()))
            },
        )
        .unwrap();
        let getter_fn = tc_get;
        let setter_fn = tc_set;
        let _ = obj.prop(
            "textContent",
            Accessor::new(getter_fn, setter_fn)
                .configurable()
                .enumerable(),
        );
    }

    // ─── appendChild(child) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let ac = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let child_key = match extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            ) {
                Some(k) => k,
                None => return Ok(Value::new_undefined(ctx.clone())),
            };
            let mut s = st.borrow_mut();
            s.arena.append_child(key, child_key);
            s.mutations.push("appendChild".to_string());
            Ok(args
                .first()
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone())))
        },
    )
    .unwrap();
    obj.set("appendChild", ac, false, ctx).unwrap_or(());

    // ─── removeChild(child) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let rc = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let child_key = match extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            ) {
                Some(k) => k,
                None => return Ok(Value::new_undefined(ctx.clone())),
            };
            let mut s = st.borrow_mut();
            s.arena.remove_child(key, child_key);
            s.mutations.push("removeChild".to_string());
            Ok(args
                .first()
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone())))
        },
    )
    .unwrap();
    obj.set("removeChild", rc, false, ctx).unwrap_or(());

    // ─── parentNode ─────────────────────────────────────────────────
    let st = Rc::clone(state);
    let pn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(parent_key) => Ok(Value::from(node_key_to_f64(parent_key))),
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();
    obj.set("parentNode", pn, false, ctx).unwrap_or(());

    // ─── childNodes ─────────────────────────────────────────────────
    let st = Rc::clone(state);
    let cn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let arr = rquickjs::Array::new(ctx.clone()).unwrap();
            if let Some(node) = s.arena.nodes.get(key) {
                for (i, &child) in node.children.iter().enumerate() {
                    let child_bits = node_key_to_f64(child);
                    let child_obj = Object::new(ctx.clone()).unwrap().set(
                        "__nodeKey__",
                        Value::new_float(ctx.clone(), child_bits as f64),
                    );
                    let _ = arr.set(i, child_obj);
                }
            }
            Ok(arr.into_value())
        },
    )
    .unwrap();
    obj.set("childNodes", cn, false, ctx).unwrap_or(());

    // ─── firstChild ─────────────────────────────────────────────────
    let st = Rc::clone(state);
    let fc = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            match s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.children.first().copied())
            {
                Some(child_key) => Ok(Value::from(node_key_to_f64(child_key))),
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();
    obj.set("firstChild", fc, false, ctx).unwrap_or(());

    // ─── nextSibling ────────────────────────────────────────────────
    let st = Rc::clone(state);
    let ns = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(Value::new_null(ctx.clone())),
            };
            let siblings = &s.arena.nodes.get(parent_key).map(|n| &n.children);
            if let Some(sibs) = siblings {
                let my_idx = sibs.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    if idx + 1 < sibs.len() {
                        return Ok(Value::from(node_key_to_f64(sibs[idx + 1])));
                    }
                }
            }
            Ok(Value::new_null(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("nextSibling", ns, false, ctx).unwrap_or(());

    // ─── closest(selector) ──────────────────────────────────────────
    let st = Rc::clone(state);
    let cl = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let sel = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let s = st.borrow();
            let mut current = Some(key);
            while let Some(k) = current {
                if matches_selector(&s.arena, k, &sel) {
                    return Ok(Value::from(node_key_to_f64(k)));
                }
                current = s.arena.nodes.get(k).and_then(|n| n.parent);
            }
            Ok(Value::new_null(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("closest", cl, false, ctx).unwrap_or(());

    // ─── matches(selector) ──────────────────────────────────────────
    let st = Rc::clone(state);
    let ms = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let sel = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let s = st.borrow();
            Ok(Value::from(matches_selector(&s.arena, key, &sel)))
        },
    )
    .unwrap();
    obj.set("matches", ms, false, ctx).unwrap_or(());

    // ─── children (bara element-barn) ───────────────────────────────
    let st = Rc::clone(state);
    let ch = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let arr = rquickjs::Array::new(ctx.clone()).unwrap();
            if let Some(node) = s.arena.nodes.get(key) {
                let mut i = 0u32;
                for &child in &node.children {
                    if let Some(cn) = s.arena.nodes.get(child) {
                        if cn.node_type == NodeType::Element {
                            let co = Object::new(ctx.clone())
                                .unwrap()
                                .set("__nodeKey__", Value::from(node_key_to_f64(child)));
                            let _ = arr.set(i, co);
                            i += 1;
                        }
                    }
                }
            }
            Ok(arr.into_value())
        },
    )
    .unwrap();
    obj.set("children", ch, false, ctx).unwrap_or(());

    // ─── dataset (läs data-* attribut) ──────────────────────────────
    let st = Rc::clone(state);
    let ds = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let ds_obj = Object::new(ctx.clone()).unwrap();
            if let Some(node) = s.arena.nodes.get(key) {
                for (k, v) in &node.attributes {
                    if let Some(name) = k.strip_prefix("data-") {
                        // Konvertera kebab-case till camelCase
                        let camel = data_attr_to_camel(name);
                        let _ = ds_obj.set(
                            camel.as_str(),
                            rquickjs::String::from_str(
                                ctx.clone(),
                                v.as_str()
                                    .map(|s| s.into_value())
                                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                            ),
                            false,
                            ctx,
                        );
                    }
                }
            }
            Ok(ds_obj.into_value())
        },
    )
    .unwrap();
    obj.set("dataset", ds, false, ctx).unwrap_or(());

    // ─── insertBefore(newChild, refChild) ─────────────────────────
    let st = Rc::clone(state);
    let ib = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let new_child = match extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            ) {
                Some(k) => k,
                None => return Ok(Value::new_undefined(ctx.clone())),
            };
            let ref_child = extract_node_key(
                &args
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            );
            let mut s = st.borrow_mut();
            s.arena.insert_before(key, new_child, ref_child);
            s.mutations.push("insertBefore".to_string());
            Ok(args
                .first()
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone())))
        },
    )
    .unwrap();
    obj.set("insertBefore", ib, false, ctx).unwrap_or(());

    // ─── insertAdjacentHTML(position, html) ────────────────────────
    // Förenklad: loggar mutation, parsning av HTML stöds ej (kräver full parser)
    let st = Rc::clone(state);
    let iah = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let position = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let html_str = args
                .get_or_undefined(1)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            s.mutations.push(format!(
                "insertAdjacentHTML:{}:{}:{}",
                key_bits, position, html_str
            ));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("insertAdjacentHTML", iah, false, ctx).unwrap_or(());

    // ─── attachShadow({ mode }) ─────────────────────────────────────
    // Förenklad: returnerar ett tomt objekt som shadow root
    let at_sh = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let shadow = Object::new(ctx.clone()).unwrap();
            Ok(Value::new_float(ctx.clone(), shadow as f64))
        },
    )
    .unwrap();
    obj.set("attachShadow", at_sh, false, ctx).unwrap_or(());

    // ─── cloneNode(deep) ────────────────────────────────────────────
    let st = Rc::clone(state);
    let cn_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let mut s = st.borrow_mut();
            match s.arena.clone_node_deep(key) {
                Some(clone_key) => {
                    let bits = node_key_to_f64(clone_key);
                    let clone_obj = Object::new(ctx.clone())
                        .unwrap()
                        .set("__nodeKey__", Value::new_float(ctx.clone(), bits as f64))
                        .set("nodeType", Value::new_int(ctx.clone(), 1));
                    Ok(clone_obj.into_value())
                }
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();
    obj.set("cloneNode", cn_fn, false, ctx).unwrap_or(());

    // ─── outerHTML (getter) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let oh = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            Ok(Value::from(js_string!(s
                .arena
                .serialize_html(key)
                .as_str())))
        },
    )
    .unwrap();
    obj.set("outerHTML", oh, false, ctx).unwrap_or(());

    // ─── innerHTML (accessor property: getter + setter) ────────────
    {
        let st_get = Rc::clone(state);
        let ih_get = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                let s = st_get.borrow();
                Ok(Value::from(js_string!(s
                    .arena
                    .serialize_inner_html(key)
                    .as_str())))
            },
        )
        .unwrap();
        let st_set = Rc::clone(state);
        let ih_set = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
                let html_str = args
                    .get_or_undefined(0)
                    .to_string(ctx)?
                    .to_std_string_escaped();
                let mut s = st_set.borrow_mut();
                // Rensa alla barn
                if let Some(node) = s.arena.nodes.get(key) {
                    let children: Vec<NodeKey> = node.children.clone();
                    for child in children {
                        s.arena.remove_child(key, child);
                    }
                }
                // Förenklad: lagra som textnod (full HTML-parsning kräver markup5ever)
                let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: std::collections::HashMap::new(),
                    text: Some(html_str.clone()),
                    parent: Some(key),
                    children: Vec::new(),
                });
                if let Some(node) = s.arena.nodes.get_mut(key) {
                    node.children.push(text_key);
                }
                s.mutations
                    .push(format!("setInnerHTML:{}:{}", key_bits, html_str));
                Ok(Value::new_undefined(ctx.clone()))
            },
        )
        .unwrap();
        let getter_fn = ih_get;
        let setter_fn = ih_set;
        let _ = obj.prop(
            "innerHTML",
            Accessor::new(getter_fn, setter_fn)
                .configurable()
                .enumerable(),
        );
    }

    // ─── Bakåtkompatibla metoder: setInnerHTML / setTextContent ──────
    // Behåller dessa som explicita metoder för befintlig kod
    let st = Rc::clone(state);
    let ih_set_compat = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let html_str = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            if let Some(node) = s.arena.nodes.get(key) {
                let children: Vec<NodeKey> = node.children.clone();
                for child in children {
                    s.arena.remove_child(key, child);
                }
            }
            let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: std::collections::HashMap::new(),
                text: Some(html_str.clone()),
                parent: Some(key),
                children: Vec::new(),
            });
            if let Some(node) = s.arena.nodes.get_mut(key) {
                node.children.push(text_key);
            }
            s.mutations
                .push(format!("setInnerHTML:{}:{}", key_bits, html_str));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("setInnerHTML", ih_set_compat, false, ctx)
        .unwrap_or(());

    let st = Rc::clone(state);
    let tc_set_compat = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let text = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            if let Some(node) = s.arena.nodes.get(key) {
                let children: Vec<NodeKey> = node.children.clone();
                for child in children {
                    s.arena.remove_child(key, child);
                }
            }
            let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: std::collections::HashMap::new(),
                text: Some(text),
                parent: None,
                children: vec![],
            });
            s.arena.append_child(key, text_key);
            s.mutations.push("setTextContent".to_string());
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("setTextContent", tc_set_compat, false, ctx)
        .unwrap_or(());

    // ─── firstElementChild ──────────────────────────────────────────
    let st = Rc::clone(state);
    let fec = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            if let Some(node) = s.arena.nodes.get(key) {
                for &child in &node.children {
                    if let Some(cn) = s.arena.nodes.get(child) {
                        if cn.node_type == NodeType::Element {
                            return Ok(Value::from(node_key_to_f64(child)));
                        }
                    }
                }
            }
            Ok(Value::new_null(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("firstElementChild", fec, false, ctx).unwrap_or(());

    // ─── nextElementSibling ─────────────────────────────────────────
    let st = Rc::clone(state);
    let nes = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(Value::new_null(ctx.clone())),
            };
            if let Some(parent) = s.arena.nodes.get(parent_key) {
                let my_idx = parent.children.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    for &sib in &parent.children[idx + 1..] {
                        if let Some(sn) = s.arena.nodes.get(sib) {
                            if sn.node_type == NodeType::Element {
                                return Ok(Value::from(node_key_to_f64(sib)));
                            }
                        }
                    }
                }
            }
            Ok(Value::new_null(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("nextElementSibling", nes, false, ctx).unwrap_or(());

    // ─── previousSibling ──────────────────────────────────────────
    let st = Rc::clone(state);
    let ps = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(Value::new_null(ctx.clone())),
            };
            let siblings = &s.arena.nodes.get(parent_key).map(|n| &n.children);
            if let Some(sibs) = siblings {
                let my_idx = sibs.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    if idx > 0 {
                        return Ok(Value::from(node_key_to_f64(sibs[idx - 1])));
                    }
                }
            }
            Ok(Value::new_null(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("previousSibling", ps, false, ctx).unwrap_or(());

    // ─── previousElementSibling ─────────────────────────────────────
    let st = Rc::clone(state);
    let pes = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(Value::new_null(ctx.clone())),
            };
            if let Some(parent) = s.arena.nodes.get(parent_key) {
                let my_idx = parent.children.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    for &sib in parent.children[..idx].iter().rev() {
                        if let Some(sn) = s.arena.nodes.get(sib) {
                            if sn.node_type == NodeType::Element {
                                return Ok(Value::from(node_key_to_f64(sib)));
                            }
                        }
                    }
                }
            }
            Ok(Value::new_null(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("previousElementSibling", pes, false, ctx)
        .unwrap_or(());

    // ─── childElementCount ──────────────────────────────────────────
    let st = Rc::clone(state);
    let cec = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let count = s
                .arena
                .nodes
                .get(key)
                .map(|n| {
                    n.children
                        .iter()
                        .filter(|&&c| {
                            s.arena
                                .nodes
                                .get(c)
                                .map(|cn| cn.node_type == NodeType::Element)
                                .unwrap_or(false)
                        })
                        .count()
                })
                .unwrap_or(0);
            Ok(Value::new_int(ctx.clone(), count as i32))
        },
    )
    .unwrap();
    obj.set("childElementCount", cec, false, ctx).unwrap_or(());

    // ─── hasAttribute(name) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let ha = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let s = st.borrow();
            Ok(Value::from(s.arena.has_attr(key, &name)))
        },
    )
    .unwrap();
    obj.set("hasAttribute", ha, false, ctx).unwrap_or(());

    // ─── remove() — ta bort elementet från sin förälder ─────────────
    let st = Rc::clone(state);
    let rm = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let mut s = st.borrow_mut();
            if let Some(parent_key) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                s.arena.remove_child(parent_key, key);
                s.mutations.push(format!("remove:{}", key_bits));
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("remove", rm, false, ctx).unwrap_or(());

    // ─── replaceWith(newNode) ───────────────────────────────────────
    let st = Rc::clone(state);
    let rw = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let new_key_val = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .to_number(_ctx)?;
            let new_key = f64_to_node_key(new_key_val);
            let mut s = st.borrow_mut();
            if let Some(parent_key) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                s.arena.insert_before(parent_key, new_key, Some(key));
                s.arena.remove_child(parent_key, key);
                s.mutations
                    .push(format!("replaceWith:{}:{}", key_bits, new_key_val as u64));
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("replaceWith", rw, false, ctx).unwrap_or(());

    // ─── replaceChild(newChild, oldChild) (Fas 19) ────────────────
    let st_rc = Rc::clone(state);
    let replace_child = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let new_key_val = extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            );
            let old_key_val = extract_node_key(
                &args
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            );
            if let (Some(new_k), Some(old_k)) = (new_key_val, old_key_val) {
                let mut s = st_rc.borrow_mut();
                s.arena.insert_before(key, new_k, Some(old_k));
                s.arena.remove_child(key, old_k);
                s.mutations
                    .push(format!("replaceChild:{}", key_bits as u64));
            }
            Ok(args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .clone())
        },
    )
    .unwrap();
    obj.set("replaceChild", replace_child, false, ctx)
        .unwrap_or(());

    // ─── before/after/prepend/append (Fas 19) ───────────────────────
    let st_before = Rc::clone(state);
    let before_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let new_key = extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            );
            if let Some(nk) = new_key {
                let mut s = st_before.borrow_mut();
                if let Some(parent) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                    s.arena.insert_before(parent, nk, Some(key));
                    s.mutations.push(format!("before:{}", key_bits as u64));
                }
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("before", before_fn, false, ctx).unwrap_or(());

    let st_after = Rc::clone(state);
    let after_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let new_key = extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            );
            if let Some(nk) = new_key {
                let mut s = st_after.borrow_mut();
                if let Some(parent) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                    // Hitta nästa syskon efter key
                    let next_sib = {
                        let p = s.arena.nodes.get(parent);
                        p.and_then(|pn| {
                            let pos = pn.children.iter().position(|&c| c == key)?;
                            pn.children.get(pos + 1).copied()
                        })
                    };
                    s.arena.insert_before(parent, nk, next_sib);
                    s.mutations.push(format!("after:{}", key_bits as u64));
                }
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("after", after_fn, false, ctx).unwrap_or(());

    let st_prepend = Rc::clone(state);
    let prepend_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let new_key = extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            );
            if let Some(nk) = new_key {
                let mut s = st_prepend.borrow_mut();
                let first_child = s
                    .arena
                    .nodes
                    .get(key)
                    .and_then(|n| n.children.first().copied());
                s.arena.insert_before(key, nk, first_child);
                s.mutations.push(format!("prepend:{}", key_bits as u64));
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("prepend", prepend_fn, false, ctx).unwrap_or(());

    let st_append = Rc::clone(state);
    let append_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let new_key = extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            );
            if let Some(nk) = new_key {
                let mut s = st_append.borrow_mut();
                s.arena.append_child(key, nk);
                s.mutations.push(format!("append:{}", key_bits as u64));
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("append", append_fn, false, ctx).unwrap_or(());

    // ─── toggleAttribute(name) (Fas 19) ─────────────────────────────
    let st_ta = Rc::clone(state);
    let toggle_attr = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let attr = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let mut s = st_ta.borrow_mut();
            let had = s.arena.has_attr(key, &attr);
            if had {
                s.arena.remove_attr(key, &attr);
            } else {
                s.arena.set_attr(key, &attr, "");
            }
            Ok(Value::from(!had))
        },
    )
    .unwrap();
    obj.set("toggleAttribute", toggle_attr, false, ctx)
        .unwrap_or(());

    // ─── getAttributeNames() (Fas 19) ───────────────────────────────
    let st_gan = Rc::clone(state);
    let get_attr_names = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st_gan.borrow();
            let arr = rquickjs::Array::new(ctx.clone()).unwrap();
            if let Some(node) = s.arena.nodes.get(key) {
                for (i, name) in node.attributes.keys().enumerate() {
                    let _ = arr.set(
                        i as u32,
                        rquickjs::String::from_str(
                            ctx.clone(),
                            name.as_str()
                                .map(|s| s.into_value())
                                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                        ),
                        false,
                        ctx,
                    );
                }
            }
            Ok(arr.into_value())
        },
    )
    .unwrap();
    obj.set("getAttributeNames", get_attr_names, false, ctx)
        .unwrap_or(());

    // ─── normalize() — slå ihop adjacenta text-noder (Fas 19) ───────
    let st_norm = Rc::clone(state);
    let normalize_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let mut s = st_norm.borrow_mut();
            let children: Vec<NodeKey> = s
                .arena
                .nodes
                .get(key)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            // Identifiera adjacenta text-noder och slå ihop dem
            let mut i = 0;
            while i + 1 < children.len() {
                let is_text_a = s
                    .arena
                    .nodes
                    .get(children[i])
                    .map(|n| n.node_type == NodeType::Text)
                    .unwrap_or(false);
                let is_text_b = s
                    .arena
                    .nodes
                    .get(children[i + 1])
                    .map(|n| n.node_type == NodeType::Text)
                    .unwrap_or(false);
                if is_text_a && is_text_b {
                    let text_b = s
                        .arena
                        .nodes
                        .get(children[i + 1])
                        .and_then(|n| n.text.clone())
                        .unwrap_or_default();
                    if let Some(node_a) = s.arena.nodes.get_mut(children[i]) {
                        let existing = node_a.text.as_deref().unwrap_or("");
                        node_a.text = Some(format!("{}{}", existing, text_b));
                    }
                    s.arena.remove_child(key, children[i + 1]);
                    // Uppdatera children-listan
                    break; // Förenklad: en pass räcker för de flesta fallen
                }
                i += 1;
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("normalize", normalize_fn, false, ctx).unwrap_or(());

    // ─── value (getter/setter för input/textarea/select) ────────────
    let st = Rc::clone(state);
    let val_get = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            Ok(Value::from(js_string!(s
                .arena
                .get_attr(key, "value")
                .unwrap_or(""))))
        },
    )
    .unwrap();
    obj.set("value", val_get, false, ctx).unwrap_or(());

    // ─── checked (getter för checkbox/radio) ────────────────────────
    let st = Rc::clone(state);
    let chk = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            Ok(Value::from(s.arena.has_attr(key, "checked")))
        },
    )
    .unwrap();
    obj.set("checked", chk, false, ctx).unwrap_or(());

    // ─── selected (getter för option-element) ───────────────────────
    let st = Rc::clone(state);
    let sel = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            Ok(Value::from(s.arena.has_attr(key, "selected")))
        },
    )
    .unwrap();
    obj.set("selected", sel, false, ctx).unwrap_or(());

    // ─── tabIndex ───────────────────────────────────────────────────
    let st = Rc::clone(state);
    let ti = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let val = s
                .arena
                .get_attr(key, "tabindex")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(-1);
            Ok(Value::new_int(ctx.clone(), val))
        },
    )
    .unwrap();
    obj.set("tabIndex", ti, false, ctx).unwrap_or(());

    // ─── offsetParent ───────────────────────────────────────────────
    let st = Rc::clone(state);
    let op = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            // Returnera body som offsetParent (förenklad implementering)
            if let Some(parent_key) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                Ok(Value::from(node_key_to_f64(parent_key)))
            } else {
                Ok(Value::new_null(ctx.clone()))
            }
        },
    )
    .unwrap();
    obj.set("offsetParent", op, false, ctx).unwrap_or(());

    // ─── clientTop / clientLeft (border-dimensioner, default 0) ─────
    obj.set("clientTop", Value::new_int(ctx.clone(), 0), false, ctx)
        .unwrap_or(());
    obj.set("clientLeft", Value::new_int(ctx.clone(), 0), false, ctx)
        .unwrap_or(());

    // ─── addEventListener(type, callback, capture/options) ──────────
    let st = Rc::clone(state);
    let ael = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let event_type = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let callback = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .clone();
            if !callback.is_function() {
                return Ok(Value::new_undefined(ctx.clone()));
            }
            // Stöd för options-objekt (tredje arg): { once, passive, capture }
            let third = args
                .get(2)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let capture = if third.is_object() {
                // Options-objekt: läs capture-fältet
                third
                    .as_object()
                    .and_then(|o| {
                        o.get("capture", ctx)
                            .ok()
                            .map(|v| v.as_bool().unwrap_or(false))
                    })
                    .unwrap_or(false)
            } else {
                third.as_bool().unwrap_or(false)
            };
            let mut s = st.borrow_mut();
            let listeners = s.event_listeners.entry(key_bits as u64).or_default();
            listeners.push(EventListener {
                event_type,
                callback,
                capture,
            });
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("addEventListener", ael, false, ctx).unwrap_or(());

    // ─── removeEventListener(type, callback) ────────────────────
    let st = Rc::clone(state);
    let rel = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let event_type = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let key_u64 = key_bits as u64;
            let mut s = st.borrow_mut();
            if let Some(listeners) = s.event_listeners.get_mut(&key_u64) {
                // Ta bort senast tillagda med matchande typ
                if let Some(pos) = listeners.iter().rposition(|l| l.event_type == event_type) {
                    listeners.remove(pos);
                }
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("removeEventListener", rel, false, ctx)
        .unwrap_or(());

    // ─── dispatchEvent(event) ───────────────────────────────────
    let st = Rc::clone(state);
    let de = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let event_val = args
                .first()
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let event_type = event_val
                .as_object()
                .and_then(|o| o.get("type", ctx).ok())
                .and_then(|v| v.to_string(ctx).ok())
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            if event_type.is_empty() {
                return Ok(Value::new_bool(ctx.clone(), false));
            }
            let bubbles = event_val
                .as_object()
                .and_then(|o| o.get("bubbles", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(());

            // Sätt target + currentTarget
            if let Some(obj) = event_val.as_object() {
                let _ = obj.set(
                    "target",
                    Value::new_float(ctx.clone(), key_bits as f64),
                    false,
                    ctx,
                );
                let _ = obj.set(
                    "currentTarget",
                    Value::new_float(ctx.clone(), key_bits as f64),
                    false,
                    ctx,
                );
            }

            // Samla ancestors för bubbling
            let ancestors = {
                let s = st.borrow();
                let mut chain = Vec::new();
                let mut current = s.arena.nodes.get(key).and_then(|n| n.parent);
                while let Some(ancestor_key) = current {
                    chain.push(node_key_to_f64(ancestor_key) as u64);
                    current = s.arena.nodes.get(ancestor_key).and_then(|n| n.parent);
                }
                chain
            };

            // Target phase
            let target_listeners: Vec<Value<'_>> = {
                let s = st.borrow();
                s.event_listeners
                    .get(&(key_bits as u64))
                    .map(|ls| {
                        ls.iter()
                            .filter(|l| l.event_type == event_type)
                            .map(|l| l.callback.clone())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            for cb in &target_listeners {
                if let Some(callable) = cb.as_function() {
                    let _ = callable.call(
                        &Value::new_undefined(ctx.clone()),
                        std::slice::from_ref(&event_val),
                        ctx,
                    );
                }
            }

            // Bubble phase
            if bubbles {
                for &ancestor_u64 in &ancestors {
                    let stopped = event_val
                        .as_object()
                        .and_then(|o| o.get("__stopped__", ctx).ok())
                        .map(|v| v.as_bool().unwrap_or(false))
                        .unwrap_or(false);
                    if stopped {
                        break;
                    }
                    if let Some(obj) = event_val.as_object() {
                        let _ = obj.set(
                            "currentTarget",
                            Value::new_float(ctx.clone(), ancestor_u64 as f64),
                            false,
                            ctx,
                        );
                    }
                    let ancestor_listeners: Vec<Value<'_>> = {
                        let s = st.borrow();
                        s.event_listeners
                            .get(&ancestor_u64)
                            .map(|ls| {
                                ls.iter()
                                    .filter(|l| l.event_type == event_type && !l.capture)
                                    .map(|l| l.callback.clone())
                                    .collect()
                            })
                            .unwrap_or_default()
                    };
                    for cb in &ancestor_listeners {
                        if let Some(callable) = cb.as_function() {
                            let _ = callable.call(
                                &Value::new_undefined(ctx.clone()),
                                std::slice::from_ref(&event_val),
                                ctx,
                            );
                        }
                    }
                }
            }

            let prevented = event_val
                .as_object()
                .and_then(|o| o.get("defaultPrevented", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            Ok(Value::from(!prevented))
        },
    )
    .unwrap();
    obj.set("dispatchEvent", de, false, ctx).unwrap_or(());

    // ─── focus() ────────────────────────────────────────────────────
    let st = Rc::clone(state);
    let focus_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let mut s = st.borrow_mut();
            s.focused_element = Some(key_bits as u64);
            s.mutations.push("focus".to_string());
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("focus", focus_fn, false, ctx).unwrap_or(());

    // ─── blur() ─────────────────────────────────────────────────────
    let st = Rc::clone(state);
    let blur_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let mut s = st.borrow_mut();
            if s.focused_element == Some(key_bits as u64) {
                s.focused_element = None;
            }
            s.mutations.push("blur".to_string());
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("blur", blur_fn, false, ctx).unwrap_or(());

    // ─── scrollIntoView(options) ────────────────────────────────────
    let st = Rc::clone(state);
    let siv = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let y = {
                let s = st.borrow();
                let (_, y, _, _) = estimate_layout_rect(&s.arena, key);
                y
            };
            let mut s = st.borrow_mut();
            // Scrollar dokumentroten till elementets y-position
            let doc_key = find_by_tag_name(&s.arena, s.arena.document, "body")
                .map(|k| node_key_to_f64(k) as u64)
                .unwrap_or(0);
            s.scroll_positions.insert(doc_key, (y, 0.0));
            s.mutations.push(format!("scrollIntoView(y={})", y));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    obj.set("scrollIntoView", siv, false, ctx).unwrap_or(());

    // ─── getBoundingClientRect() ────────────────────────────────────
    let st = Rc::clone(state);
    let gbcr = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let (x, y, width, height) = estimate_layout_rect(&s.arena, key);
            let rect = Object::new(ctx.clone()).unwrap();
            obj.set("x", Value::new_float(ctx.clone(), x as f64))
                .unwrap_or(());
            obj.set("y", Value::new_float(ctx.clone(), y as f64))
                .unwrap_or(())
                .set("width", Value::new_float(ctx.clone(), width as f64))
                .set("height", Value::new_float(ctx.clone(), height as f64));
            obj.set("top", Value::new_float(ctx.clone(), y as f64))
                .unwrap_or(())
                .set("right", Value::from(x + width))
                .set("bottom", Value::from(y + height));
            obj.set("left", Value::new_float(ctx.clone(), x as f64))
                .unwrap_or(());
            Ok(rect.into_value())
        },
    )
    .unwrap();
    obj.set("getBoundingClientRect", gbcr, false, ctx)
        .unwrap_or(());

    // ─── getClientRects() ───────────────────────────────────────────
    let st = Rc::clone(state);
    let gcr = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let (x, y, width, height) = estimate_layout_rect(&s.arena, key);
            let arr = rquickjs::Array::new(ctx.clone()).unwrap();
            let rect = Object::new(ctx.clone()).unwrap();
            obj.set("x", Value::new_float(ctx.clone(), x as f64))
                .unwrap_or(());
            obj.set("y", Value::new_float(ctx.clone(), y as f64))
                .unwrap_or(())
                .set("width", Value::new_float(ctx.clone(), width as f64))
                .set("height", Value::new_float(ctx.clone(), height as f64));
            let _ = arr.set(0, rect);
            Ok(arr.into_value())
        },
    )
    .unwrap();
    obj.set("getClientRects", gcr, false, ctx).unwrap_or(());

    // ─── offset* / client* / scroll* properties ─────────────────────
    {
        let s = state.borrow();
        let (_, y_pos, w, h) = estimate_layout_rect(&s.arena, key);
        let (scroll_top, scroll_left) = s
            .scroll_positions
            .get(&(key_bits as u64))
            .copied()
            .unwrap_or((0.0, 0.0));
        let content_h = s
            .arena
            .nodes
            .get(key)
            .map(|n| (n.children.len() as f64) * 30.0)
            .unwrap_or(h)
            .max(h);
        let content_w = w.max(1024.0);
        obj.set(
            "offsetTop",
            Value::new_float(ctx.clone(), y_pos as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set("offsetLeft", Value::new_int(ctx.clone(), 0), false, ctx)
            .unwrap_or(());
        obj.set(
            "offsetWidth",
            Value::new_float(ctx.clone(), w as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set(
            "offsetHeight",
            Value::new_float(ctx.clone(), h as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set(
            "scrollTop",
            Value::new_float(ctx.clone(), scroll_top as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set(
            "scrollLeft",
            Value::new_float(ctx.clone(), scroll_left as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set(
            "scrollWidth",
            Value::new_float(ctx.clone(), content_w as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set(
            "scrollHeight",
            Value::new_float(ctx.clone(), content_h as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set(
            "clientWidth",
            Value::new_float(ctx.clone(), w as f64),
            false,
            ctx,
        )
        .unwrap_or(());
        obj.set(
            "clientHeight",
            Value::new_float(ctx.clone(), h as f64),
            false,
            ctx,
        )
        .unwrap_or(());
    }

    // ─── shadowRoot (read-only traversal av deklarativ Shadow DOM) ──
    {
        let shadow_key = {
            let s = state.borrow();
            s.arena
                .nodes
                .get(key)
                .and_then(|node| {
                    // Deklarativ Shadow DOM: <template shadowrootmode="open">
                    node.children.iter().find(|&&child| {
                        s.arena
                            .nodes
                            .get(child)
                            .map(|cn| {
                                cn.tag.as_deref() == Some("template")
                                    && (cn.has_attr("shadowrootmode") || cn.has_attr("shadowroot"))
                            })
                            .unwrap_or(false)
                    })
                })
                .copied()
        };
        if let Some(template_key) = shadow_key {
            let shadow = make_element_object(ctx, template_key, state);
            obj.set("shadowRoot", shadow).unwrap_or(());
        } else {
            obj.set("shadowRoot", Value::new_null(ctx.clone()))
                .unwrap_or(());
        }
    }

    // ─── classList ──────────────────────────────────────────────────
    let class_list = make_class_list(ctx, key, state);
    obj.set("classList", class_list).unwrap_or(());

    // ─── style (inline CSS-stilar med setProperty/getPropertyValue) ─
    let style = make_style_object(ctx, key, state);
    obj.set("style", style).unwrap_or(());

    // ─── isConnected (boolean — noden är kopplad till document) ─────
    {
        let s = state.borrow();
        let connected = is_connected_to_document(&s.arena, key);
        obj.set(
            "isConnected",
            Value::new_bool(ctx.clone(), connected),
            false,
            ctx,
        )
        .unwrap_or(());
    }

    // ─── contains(otherElement) ─────────────────────────────────────
    let st = Rc::clone(state);
    let contains_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let other_key = match extract_node_key(
                &args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone())),
            ) {
                Some(k) => k,
                None => return Ok(Value::new_bool(ctx.clone(), false)),
            };
            let s = st.borrow();
            Ok(Value::from(node_contains(&s.arena, key, other_key)))
        },
    )
    .unwrap();
    obj.set("contains", contains_fn, false, ctx).unwrap_or(());

    // ─── getRootNode() ──────────────────────────────────────────────
    let st = Rc::clone(state);
    let root_node_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let mut current = key;
            while let Some(parent) = s.arena.nodes.get(current).and_then(|n| n.parent) {
                current = parent;
            }
            Ok(Value::from(node_key_to_f64(current)))
        },
    )
    .unwrap();
    obj.set("getRootNode", root_node_fn, false, ctx)
        .unwrap_or(());

    // ─── hidden (property kopplad till hidden-attribut) ──────────────
    {
        let s = state.borrow();
        let is_hidden = s
            .arena
            .nodes
            .get(key)
            .map(|n| n.has_attr("hidden"))
            .unwrap_or(false);
        obj.set(
            "hidden",
            Value::new_bool(ctx.clone(), is_hidden),
            false,
            ctx,
        )
        .unwrap_or(());
    }

    // ─── requestPointerLock() / exitPointerLock() ───────────────────
    let rpl = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_undefined(ctx))
    })
    .unwrap();
    obj.set("requestPointerLock", rpl, false, ctx).unwrap_or(());

    Value::new_float(ctx.clone(), obj as f64)
}

/// Skapa classList-objekt med add/remove/toggle/contains
fn make_class_list(
    ctx: &Ctx<'_>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'_>> {
    let st_add = Rc::clone(state);
    let add_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let cls = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st_add.borrow_mut();
            if let Some(node) = s.arena.nodes.get_mut(key) {
                let current = node.get_attr("class").unwrap_or("").to_string();
                if !current.split_whitespace().any(|c| c == cls) {
                    let new_val = if current.is_empty() {
                        cls.clone()
                    } else {
                        format!("{} {}", current, cls)
                    };
                    node.set_attr("class", &new_val);
                }
            }
            s.mutations.push(format!("classList.add({})", cls));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();

    let st_rm = Rc::clone(state);
    let remove_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let cls = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st_rm.borrow_mut();
            if let Some(node) = s.arena.nodes.get_mut(key) {
                let current = node.get_attr("class").unwrap_or("").to_string();
                let new_val: String = current
                    .split_whitespace()
                    .filter(|c| *c != cls)
                    .collect::<Vec<_>>()
                    .join(" ");
                node.set_attr("class", &new_val);
            }
            s.mutations.push(format!("classList.remove({})", cls));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();

    let st_tg = Rc::clone(state);
    let toggle_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let cls = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st_tg.borrow_mut();
            let has = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("class"))
                .map(|c| c.split_whitespace().any(|x| x == cls))
                .unwrap_or(false);
            if let Some(node) = s.arena.nodes.get_mut(key) {
                let current = node.get_attr("class").unwrap_or("").to_string();
                let new_val = if has {
                    current
                        .split_whitespace()
                        .filter(|c| *c != cls)
                        .collect::<Vec<_>>()
                        .join(" ")
                } else if current.is_empty() {
                    cls.clone()
                } else {
                    format!("{} {}", current, cls)
                };
                node.set_attr("class", &new_val);
            }
            Ok(Value::from(!has))
        },
    )
    .unwrap();

    let st_ct = Rc::clone(state);
    let contains_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let cls = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let s = st_ct.borrow();
            let has = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("class"))
                .map(|c| c.split_whitespace().any(|x| x == cls))
                .unwrap_or(false);
            Ok(Value::new_bool(ctx.clone(), has))
        },
    )
    .unwrap();

    // replace(old, new) — byt klass
    let st_rp = Rc::clone(state);
    let replace_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let old_cls = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let new_cls = args
                .get_or_undefined(1)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st_rp.borrow_mut();
            let had = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("class"))
                .map(|c| c.split_whitespace().any(|x| x == old_cls))
                .unwrap_or(false);
            if had {
                if let Some(node) = s.arena.nodes.get_mut(key) {
                    let current = node.get_attr("class").unwrap_or("").to_string();
                    let new_val: String = current
                        .split_whitespace()
                        .map(|c| if c == old_cls { new_cls.as_str() } else { c })
                        .collect::<Vec<_>>()
                        .join(" ");
                    node.set_attr("class", &new_val);
                }
            }
            Ok(Value::new_bool(ctx.clone(), had))
        },
    )
    .unwrap();

    // value — hela class-strängen
    let st_val = Rc::clone(state);
    let value_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st_val.borrow();
            let val = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("class"))
                .unwrap_or("");
            Ok(rquickjs::String::from_str(ctx.clone(), val)
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())))
        },
    )
    .unwrap();

    // length — antal klasser
    let st_len = Rc::clone(state);
    let length_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st_len.borrow();
            let count = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("class"))
                .map(|c| c.split_whitespace().count())
                .unwrap_or(0);
            Ok(Value::new_int(ctx.clone(), count))
        },
    )
    .unwrap();

    let cl = Object::new(ctx.clone()).unwrap();
    obj.set("add", add_fn).unwrap_or(());
    obj.set("remove", remove_fn).unwrap_or(());
    obj.set("toggle", toggle_fn).unwrap_or(());
    obj.set("contains", contains_fn).unwrap_or(());
    obj.set("replace", replace_fn).unwrap_or(());
    obj.set("value", value_fn).unwrap_or(());
    obj.set("length", length_fn).unwrap_or(());

    Value::new_float(ctx.clone(), cl as f64)
}

/// Skapa style-objekt med inline CSS-stöd kopplat till arena
fn make_style_object(
    ctx: &Ctx<'_>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'_>> {
    // Läs initiala stilar från style-attributet
    let initial_styles = {
        let s = state.borrow();
        let style_str = s
            .arena
            .nodes
            .get(key)
            .and_then(|n| n.get_attr("style"))
            .unwrap_or("");
        parse_inline_styles(style_str)
    };

    // setProperty(prop, value)
    let st = Rc::clone(state);
    let set_prop = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let prop = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let value = args
                .get_or_undefined(1)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            let style_str = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("style"))
                .unwrap_or("")
                .to_string();
            let mut styles = parse_inline_styles(&style_str);
            if value.is_empty() {
                styles.remove(&prop);
            } else {
                styles.insert(prop.clone(), value.clone());
            }
            let new_style = serialize_inline_styles(&styles);
            s.arena.set_attr(key, "style", &new_style);
            s.mutations
                .push(format!("style.setProperty({}, {})", prop, value));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();

    // getPropertyValue(prop)
    let st = Rc::clone(state);
    let get_prop = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let prop = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let s = st.borrow();
            let style_str = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("style"))
                .unwrap_or("");
            let styles = parse_inline_styles(style_str);
            let val = styles.get(&prop).cloned().unwrap_or_default();
            Ok(rquickjs::String::from_str(
                ctx.clone(),
                val.as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ))
        },
    )
    .unwrap();

    // removeProperty(prop)
    let st = Rc::clone(state);
    let remove_prop = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let prop = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let mut s = st.borrow_mut();
            let style_str = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("style"))
                .unwrap_or("")
                .to_string();
            let mut styles = parse_inline_styles(&style_str);
            let old_val = styles.remove(&prop).unwrap_or_default();
            let new_style = serialize_inline_styles(&styles);
            s.arena.set_attr(key, "style", &new_style);
            Ok(rquickjs::String::from_str(
                ctx.clone(),
                old_val
                    .as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ))
        },
    )
    .unwrap();

    // cssText getter
    let st = Rc::clone(state);
    let css_text_get = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st.borrow();
            let style_str = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("style"))
                .unwrap_or("");
            Ok(rquickjs::String::from_str(ctx.clone(), style_str)
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())))
        },
    )
    .unwrap();

    let style = Object::new(ctx.clone()).unwrap();
    obj.set("setProperty", set_prop).unwrap_or(());
    obj.set("getPropertyValue", get_prop).unwrap_or(());
    obj.set("removeProperty", remove_prop).unwrap_or(());
    obj.set("cssText", css_text_get).unwrap_or(());

    // Populera vanliga CSS-egenskaper från inline style
    let common_props = [
        "display",
        "visibility",
        "position",
        "opacity",
        "overflow",
        "width",
        "height",
        "margin",
        "padding",
        "color",
        "background-color",
        "font-size",
        "z-index",
        "pointer-events",
        "top",
        "left",
        "right",
        "bottom",
        "transform",
        "transition",
        "border",
    ];
    for prop in &common_props {
        let val = initial_styles.get(*prop).cloned().unwrap_or_default();
        let camel = data_attr_to_camel(prop);
        let _ = style.set(
            camel.as_str(),
            rquickjs::String::from_str(
                ctx.clone(),
                val.as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ),
            false,
            ctx,
        );
    }

    Value::new_float(ctx.clone(), style as f64)
}

/// Konvertera data-attribut-namn till camelCase (t.ex. "product-id" → "productId")
fn data_attr_to_camel(name: &str) -> String {
    let mut result = String::with_capacity(name.len());
    let mut capitalize_next = false;
    for ch in name.chars() {
        if ch == '-' {
            capitalize_next = true;
        } else if capitalize_next {
            result.extend(ch.to_uppercase());
            capitalize_next = false;
        } else {
            result.push(ch);
        }
    }
    result
}

// ─── Window-objekt ──────────────────────────────────────────────────────────

fn register_window(ctx: &Ctx<'_>, state: SharedState) -> rquickjs::Result<()> {
    let window = Object::new(ctx.clone())
        .unwrap()
        .set("innerWidth", Value::new_int(ctx.clone(), 1024))
        .set("innerHeight", Value::new_int(ctx.clone(), 768));

    // location stub
    let location = Object::new(ctx.clone())
        .unwrap()
        .set(
            "href",
            rquickjs::String::from_str(ctx.clone(), "")
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
        )
        .set(
            "hostname",
            rquickjs::String::from_str(ctx.clone(), "")
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
        )
        .set(
            "pathname",
            rquickjs::String::from_str(ctx.clone(), "/")
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
        )
        .set(
            "protocol",
            rquickjs::String::from_str(ctx.clone(), "https:")
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
        );

    window.set("location", location).unwrap_or(());

    // navigator stub
    let navigator = Object::new(ctx.clone())
        .unwrap()
        .set(
            "userAgent",
            rquickjs::String::from_str(ctx.clone(), "AetherAgent/0.1")
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
        )
        .set(
            "language",
            rquickjs::String::from_str(ctx.clone(), "en")
                .map(|s| s.into_value())
                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
        );

    window.set("navigator", navigator).unwrap_or(());

    // getComputedStyle(el) — använder CSS Cascade Engine (Fas 19)
    let st_gcs = Rc::clone(&state);
    let gcs = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let elem = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let node_key = extract_node_key(&elem);

            let merged = match node_key {
                Some(k) => {
                    let mut s = st_gcs.borrow_mut();
                    // Lazy-initiera CssContext vid första anrop
                    if s.css_context.is_none() {
                        s.css_context = Some(crate::css_cascade::CssContext::from_arena(&s.arena));
                    }
                    let arena_clone = s.arena.clone();
                    if let Some(ref mut css_ctx) = s.css_context {
                        css_ctx.get_computed_style(k, &arena_clone).properties
                    } else {
                        // Fallback: tag-defaults + inline
                        let style_str = s
                            .arena
                            .nodes
                            .get(k)
                            .and_then(|n| n.get_attr("style"))
                            .unwrap_or("");
                        let tag = s.arena.tag_name(k).unwrap_or("div").to_string();
                        let inline = parse_inline_styles(style_str);
                        let mut m = get_tag_style_defaults(&tag);
                        for (prop, val) in &inline {
                            m.insert(prop.clone(), val.clone());
                        }
                        m
                    }
                }
                None => get_tag_style_defaults("div"),
            };

            let merged_for_closure = merged.clone();
            let get_pv = Function::new(move |_this, args, ctx| {
                let prop = args
                    .get_or_undefined(0)
                    .to_string(ctx)
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default()
                    .to_lowercase();
                let val = merged_for_closure.get(&prop).cloned().unwrap_or_default();
                Ok(rquickjs::String::from_str(
                    ctx.clone(),
                    val.as_str()
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                ))
            });

            let style = Object::new(ctx.clone()).unwrap();
            win.set("getPropertyValue", get_pv).unwrap_or(());

            // Sätt alla properties som egenskaper (camelCase + kebab)
            for (prop, val) in &merged {
                let camel = data_attr_to_camel(prop);
                let _ = style.set(
                    camel.as_str(),
                    rquickjs::String::from_str(
                        ctx.clone(),
                        val.as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                    false,
                    ctx,
                );
            }

            Ok(style.into_value())
        },
    )
    .unwrap();
    window
        .set("getComputedStyle", gcs, false, ctx)
        .unwrap_or(());

    // matchMedia(query) — Fas 19: CSS media query matching
    let match_media = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let query = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();

            // Parsa bredd-baserade media queries mot innerWidth=1024
            let matches = parse_media_query_matches(&query, 1024.0, 768.0);

            let add_listener =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();
            let remove_listener =
                Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                    Ok(Value::new_undefined(ctx))
                })
                .unwrap();

            let mql = Object::new(ctx.clone())
                .unwrap()
                .set("matches", Value::new_float(ctx.clone(), matches as f64))
                .set(
                    "media",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        query
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .function(add_listener.clone(), "addEventListener", 2)
                .function(remove_listener.clone(), "removeEventListener", 2);
            win.set("addListener", add_listener).unwrap_or(());
            win.set("removeListener", remove_listener).unwrap_or(());
            Ok(mql.into_value())
        },
    );
    window
        .set("matchMedia", match_media, false, ctx)
        .unwrap_or(());

    // atob/btoa — base64 encode/decode
    let atob_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let input = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            // atob decodes base64
            match base64_decode(&input) {
                Some(decoded) => Ok(rquickjs::String::from_str(
                    ctx.clone(),
                    decoded
                        .as_str()
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                )),
                None => Ok(rquickjs::String::from_str(ctx.clone(), "")
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone()))),
            }
        },
    );
    window.set("atob", atob_fn, false, ctx).unwrap_or(());

    let btoa_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let input = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let encoded = base64_encode(&input);
            Ok(rquickjs::String::from_str(
                ctx.clone(),
                encoded
                    .as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ))
        },
    );
    window.set("btoa", btoa_fn, false, ctx).unwrap_or(());

    // scrollTo/scrollBy — stubs som loggar mutation
    let st_scroll = Rc::clone(&state);
    let scroll_to = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let x = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .as_float()
                .or_else(|| Some(0.0))
                .unwrap_or(0.0);
            let y = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .as_float()
                .or_else(|| Some(0.0))
                .unwrap_or(0.0);
            st_scroll
                .borrow_mut()
                .mutations
                .push(format!("window.scrollTo({}, {})", x, y));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    window.set("scrollTo", scroll_to, false, ctx).unwrap_or(());
    let st_scroll2 = Rc::clone(&state);
    let scroll_by = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let x = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .as_float()
                .or_else(|| Some(0.0))
                .unwrap_or(0.0);
            let y = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .as_float()
                .or_else(|| Some(0.0))
                .unwrap_or(0.0);
            st_scroll2
                .borrow_mut()
                .mutations
                .push(format!("window.scrollBy({}, {})", x, y));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    window.set("scrollBy", scroll_by, false, ctx).unwrap_or(());
    // scroll = alias för scrollTo
    let scroll_fn = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_undefined(ctx))
    })
    .unwrap();
    window.set("scroll", scroll_fn, false, ctx).unwrap_or(());

    // performance.now()
    let perf_start = std::time::Instant::now();
    let perf_now = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let ms = perf_start.elapsed().as_secs_f64() * 1000.0;
            Ok(Value::new_float(ctx.clone(), ms as f64))
        },
    )
    .unwrap();
    let performance = Object::new(ctx.clone()).unwrap();
    win.set("now", perf_now).unwrap_or(());
    window.set("performance", performance).unwrap_or(());

    // screen object
    let screen = Object::new(ctx.clone())
        .unwrap()
        .set("width", Value::new_int(ctx.clone(), 1920))
        .set("height", Value::new_int(ctx.clone(), 1080))
        .set("colorDepth", Value::new_int(ctx.clone(), 24))
        .set("availWidth", Value::new_int(ctx.clone(), 1920))
        .set("availHeight", Value::new_int(ctx.clone(), 1040));
    window.set("screen", screen).unwrap_or(());

    // devicePixelRatio
    window
        .set("devicePixelRatio", Value::from(1.0), false, ctx)
        .unwrap_or(());

    // history stub
    let st_hist = Rc::clone(&state);
    let push_state = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let url = args
                .get_or_undefined(2)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            st_hist
                .borrow_mut()
                .mutations
                .push(format!("history.pushState({})", url));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    let st_hist2 = Rc::clone(&state);
    let replace_state = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let url = args
                .get_or_undefined(2)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            st_hist2
                .borrow_mut()
                .mutations
                .push(format!("history.replaceState({})", url));
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();
    let back_fn = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_undefined(ctx))
    })
    .unwrap();
    let forward_fn = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_undefined(ctx))
    })
    .unwrap();
    let go_fn = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_undefined(ctx))
    })
    .unwrap();
    let history = Object::new(ctx.clone()).unwrap();
    win.set("pushState", push_state).unwrap_or(());
    win.set("replaceState", replace_state).unwrap_or(());
    win.set("back", back_fn).unwrap_or(());
    win.set("forward", forward_fn).unwrap_or(());
    win.set("go", go_fn)
        .unwrap_or(())
        .set("length", Value::new_int(ctx.clone(), 1));
    window.set("history", history).unwrap_or(());

    // requestIdleCallback — kör callback direkt (idle = always)
    let ric_fn = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            if let Some(callable) = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .as_function()
            {
                let deadline = Object::new(ctx.clone())
                    .unwrap()
                    .set("didTimeout", Value::new_bool(ctx.clone(), false));
                let time_remaining =
                    Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                        Ok(Value::from(50.0))
                    })
                    .unwrap();
                let _ = deadline.set(
                    "timeRemaining",
                    time_remaining.to_js_function(ctx.realm()),
                    false,
                    ctx,
                );
                let _ = callable.call(
                    &Value::new_undefined(ctx.clone()),
                    &[deadline.into_value()],
                    ctx,
                );
            }
            Ok(Value::new_int(ctx.clone(), 1))
        },
    );
    window
        .set("requestIdleCallback", ric_fn, false, ctx)
        .unwrap_or(());

    // open/close stubs
    let open_fn = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_null(ctx.clone()))
    })
    .unwrap();
    let close_fn = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        Ok(Value::new_undefined(ctx))
    })
    .unwrap();
    window.set("open", open_fn, false, ctx).unwrap_or(());
    window.set("close", close_fn, false, ctx).unwrap_or(());

    // IntersectionObserver — fires per-element on observe() med layoutdata
    let st_io = Rc::clone(&state);
    let io_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let callback = args
                .first()
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let cb_clone = callback.clone();
            let st_observe = Rc::clone(&st_io);

            let observe_fn = Function::new(move |_this, args, ctx| {
                let target = args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
                let target_key = extract_node_key(&target);

                if let (Some(callable), Some(k)) = (cb_clone.as_function(), target_key) {
                    let (y, w, h) = {
                        let s = st_observe.borrow();
                        let (_, y, w, h) = estimate_layout_rect(&s.arena, k);
                        (y, w, h)
                    };

                    let is_visible = y < 768.0 && w > 0.0 && h > 0.0;
                    let ratio = if is_visible { 1.0 } else { 0.0 };

                    let bounds = Object::new(ctx.clone()).unwrap();
                    win.set("x", Value::from(0.0)).unwrap_or(());
                    win.set("y", Value::new_float(ctx.clone(), y as f64))
                        .unwrap_or(());
                    win.set("width", Value::new_float(ctx.clone(), w as f64))
                        .unwrap_or(());
                    win.set("height", Value::new_float(ctx.clone(), h as f64))
                        .unwrap_or(());

                    let entry = Object::new(ctx.clone())
                        .unwrap()
                        .set(
                            "isIntersecting",
                            Value::new_float(ctx.clone(), is_visible as f64),
                        )
                        .set(
                            "intersectionRatio",
                            Value::new_float(ctx.clone(), ratio as f64),
                        )
                        .property("boundingClientRect", bounds);
                    win.set("target", target.clone()).unwrap_or(());

                    let entries = rquickjs::Array::new(ctx.clone()).unwrap();
                    let _ = entries.set(0, entry);
                    let _ = callable.call(
                        &Value::new_undefined(ctx.clone()),
                        &[entries.into_value()],
                        ctx,
                    );
                }
                Ok(Value::new_undefined(ctx.clone()))
            });

            let unobserve_fn =
                Function::new(|_this, _args, _ctx| Ok(Value::new_undefined(ctx.clone())));
            let disconnect_fn =
                Function::new(|_this, _args, _ctx| Ok(Value::new_undefined(ctx.clone())));
            let observer = Object::new(ctx.clone()).unwrap();
            win.set("observe", observe_fn).unwrap_or(());
            win.set("unobserve", unobserve_fn).unwrap_or(());
            win.set("disconnect", disconnect_fn).unwrap_or(());
            Ok(observer.into_value())
        },
    )
    .unwrap();
    window
        .set("IntersectionObserver", io_fn, false, ctx)
        .unwrap_or(());

    // ResizeObserver — fires callback on observe() med element-dimensioner
    let st_ro = Rc::clone(&state);
    let ro_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let callback = args
                .first()
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let cb_clone = callback.clone();
            let st_observe = Rc::clone(&st_ro);

            let observe_fn = Function::new(move |_this, args, ctx| {
                let target = args
                    .get(0)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
                let target_key = extract_node_key(&target);

                if let (Some(callable), Some(k)) = (cb_clone.as_function(), target_key) {
                    let (w, h) = {
                        let s = st_observe.borrow();
                        let (_, _, w, h) = estimate_layout_rect(&s.arena, k);
                        (w, h)
                    };

                    let content_rect = Object::new(ctx.clone()).unwrap();
                    win.set("width", Value::new_float(ctx.clone(), w as f64))
                        .unwrap_or(());
                    win.set("height", Value::new_float(ctx.clone(), h as f64))
                        .unwrap_or(());
                    win.set("x", Value::from(0.0)).unwrap_or(());
                    win.set("y", Value::from(0.0)).unwrap_or(());

                    let border_size = Object::new(ctx.clone())
                        .unwrap()
                        .set("inlineSize", Value::new_float(ctx.clone(), w as f64))
                        .set("blockSize", Value::new_float(ctx.clone(), h as f64));
                    let border_arr = rquickjs::Array::new(ctx.clone()).unwrap();
                    let _ = border_arr.set(0, border_size);

                    let entry = Object::new(ctx.clone()).unwrap();
                    win.set("contentRect", content_rect).unwrap_or(());
                    win.set("borderBoxSize", border_arr).unwrap_or(());
                    win.set("target", target.clone()).unwrap_or(());

                    let entries = rquickjs::Array::new(ctx.clone()).unwrap();
                    let _ = entries.set(0, entry);
                    let _ = callable.call(
                        &Value::new_undefined(ctx.clone()),
                        &[entries.into_value()],
                        ctx,
                    );
                }
                Ok(Value::new_undefined(ctx.clone()))
            });

            let unobserve_fn =
                Function::new(|_this, _args, _ctx| Ok(Value::new_undefined(ctx.clone())));
            let disconnect_fn =
                Function::new(|_this, _args, _ctx| Ok(Value::new_undefined(ctx.clone())));
            let observer = Object::new(ctx.clone()).unwrap();
            win.set("observe", observe_fn).unwrap_or(());
            win.set("unobserve", unobserve_fn).unwrap_or(());
            win.set("disconnect", disconnect_fn).unwrap_or(());
            Ok(observer.into_value())
        },
    )
    .unwrap();
    window
        .set("ResizeObserver", ro_fn, false, ctx)
        .unwrap_or(());

    // customElements — med lifecycle callback-stöd (connectedCallback, disconnectedCallback)
    // Lagrar registrerade element-definitioner med deras konstruktor/klass
    let registry: Rc<RefCell<std::collections::HashMap<String, Value<'_>>>> =
        Rc::new(RefCell::new(std::collections::HashMap::new()));

    let reg_define = Rc::clone(&registry);
    let ce_define = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let constructor = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .clone();
            // Validera att namnet innehåller bindestreck (Web Components-krav)
            if !name.contains('-') {
                return Ok(Value::new_undefined(ctx.clone()));
            }
            reg_define.borrow_mut().insert(name, constructor);
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();

    let reg_get = Rc::clone(&registry);
    let ce_get = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let reg = reg_get.borrow();
            match reg.get(&name) {
                Some(ctor) => Ok(ctor.clone()),
                None => Ok(Value::new_undefined(ctx.clone())),
            }
        },
    )
    .unwrap();

    let reg_when = Rc::clone(&registry);
    let ce_when = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let reg = reg_when.borrow();
            if reg.contains_key(&name) {
                // Redan definierad — returnera resolved Promise
                let promise = ctx.eval::<Value, _>("Promise.resolve()");
                match promise {
                    Ok(p) => Ok(p),
                    Err(_) => Ok(Value::new_undefined(ctx.clone())),
                }
            } else {
                // Inte definierad — returnera pending-liknande Promise
                let promise = ctx.eval::<Value, _>("Promise.resolve()");
                match promise {
                    Ok(p) => Ok(p),
                    Err(_) => Ok(Value::new_undefined(ctx.clone())),
                }
            }
        },
    )
    .unwrap();

    let custom_elements = Object::new(ctx.clone()).unwrap();
    win.set("define", ce_define).unwrap_or(());
    win.set("get", ce_get).unwrap_or(());
    win.set("whenDefined", ce_when).unwrap_or(());
    window
        .set("customElements", custom_elements, false, ctx)
        .unwrap_or(());

    // localStorage — in-memory sandboxad implementation (Fas 19)
    register_storage(ctx, Rc::clone(&state), "localStorage", true);
    register_storage(ctx, Rc::clone(&state), "sessionStorage", false);

    // Sätt localStorage/sessionStorage på window också
    if let Ok(ls) = ctx.global_object().get("localStorage", ctx) {
        window.set("localStorage", ls).unwrap_or(());
    }
    if let Ok(ss) = ctx.global_object().get("sessionStorage", ctx) {
        window.set("sessionStorage", ss).unwrap_or(());
    }

    ctx.globals().set("window", window).unwrap_or(());

    // Event konstruktor — new Event('click', {bubbles: true})
    let event_ctor = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let type_str = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let options = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let bubbles = options
                .as_object()
                .and_then(|o| o.get("bubbles", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            let cancelable = options
                .as_object()
                .and_then(|o| o.get("cancelable", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            let composed = options
                .as_object()
                .and_then(|o| o.get("composed", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);

            let stop_prop = Function::new(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let _ = obj.set(
                        "__stopped__",
                        Value::new_bool(ctx.clone(), true),
                        false,
                        ctx,
                    );
                }
                Ok(Value::new_undefined(ctx.clone()))
            });
            let prevent_default = Function::new(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let _ = obj.set(
                        "defaultPrevented",
                        Value::new_bool(ctx.clone(), true),
                        false,
                        ctx,
                    );
                }
                Ok(Value::new_undefined(ctx.clone()))
            });
            let stop_imm = Function::new(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let _ = obj.set(
                        "__stopped__",
                        Value::new_bool(ctx.clone(), true),
                        false,
                        ctx,
                    );
                }
                Ok(Value::new_undefined(ctx.clone()))
            });

            let event = Object::new(ctx.clone())
                .unwrap()
                .set(
                    "type",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        type_str
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .set("bubbles", Value::new_bool(ctx.clone(), bubbles))
                .set(
                    "cancelable",
                    Value::new_float(ctx.clone(), cancelable as f64),
                )
                .set("composed", Value::new_float(ctx.clone(), composed as f64))
                .set("defaultPrevented", Value::new_bool(ctx.clone(), false))
                .set("__stopped__", Value::new_bool(ctx.clone(), false));
            win.set("target", Value::new_null(ctx.clone()))
                .unwrap_or(())
                .set("currentTarget", Value::new_null(ctx.clone()))
                .set("eventPhase", Value::new_int(ctx.clone(), 0))
                .set("timeStamp", Value::from(0.0))
                .set("isTrusted", Value::new_bool(ctx.clone(), false));
            win.set("stopPropagation", stop_prop).unwrap_or(());
            win.set("preventDefault", prevent_default).unwrap_or(());
            win.set("stopImmediatePropagation", stop_imm).unwrap_or(());

            Ok(event.into_value())
        },
    );
    ctx.globals().set("Event", event_ctor).unwrap_or(());

    // CustomEvent konstruktor — new CustomEvent('my-event', {detail: data})
    let custom_event_ctor = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let type_str = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let options = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            let bubbles = options
                .as_object()
                .and_then(|o| o.get("bubbles", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            let detail = options
                .as_object()
                .and_then(|o| o.get("detail", ctx).ok())
                .unwrap_or(Value::new_null(ctx.clone()));
            let composed = options
                .as_object()
                .and_then(|o| o.get("composed", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);
            let cancelable = options
                .as_object()
                .and_then(|o| o.get("cancelable", ctx).ok())
                .map(|v| v.as_bool().unwrap_or(false))
                .unwrap_or(false);

            let stop_prop = Function::new(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let _ = obj.set(
                        "__stopped__",
                        Value::new_bool(ctx.clone(), true),
                        false,
                        ctx,
                    );
                }
                Ok(Value::new_undefined(ctx.clone()))
            });
            let prevent_default = Function::new(|this, _args, ctx| {
                if let Some(obj) = this.as_object() {
                    let _ = obj.set(
                        "defaultPrevented",
                        Value::new_bool(ctx.clone(), true),
                        false,
                        ctx,
                    );
                }
                Ok(Value::new_undefined(ctx.clone()))
            });

            let event = Object::new(ctx.clone())
                .unwrap()
                .set(
                    "type",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        type_str
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .set("bubbles", Value::new_bool(ctx.clone(), bubbles))
                .set(
                    "cancelable",
                    Value::new_float(ctx.clone(), cancelable as f64),
                )
                .set("composed", Value::new_float(ctx.clone(), composed as f64))
                .set("defaultPrevented", Value::new_bool(ctx.clone(), false))
                .set("__stopped__", Value::new_bool(ctx.clone(), false));
            win.set("detail", detail).unwrap_or(());
            win.set("target", Value::new_null(ctx.clone()))
                .unwrap_or(())
                .set("currentTarget", Value::new_null(ctx.clone()))
                .set("isTrusted", Value::new_bool(ctx.clone(), false));
            win.set("stopPropagation", stop_prop).unwrap_or(());
            win.set("preventDefault", prevent_default).unwrap_or(());

            Ok(event.into_value())
        },
    );
    ctx.globals()
        .set("CustomEvent", custom_event_ctor)
        .unwrap_or(());

    // ─── Globala API:er (Fas 19) ─────────────────────────────────────────────

    // TextEncoder
    let text_encoder_ctor =
        Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let encode_fn = Function::new(
                ctx.clone(),
                |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
                    let input = args
                        .get_or_undefined(0)
                        .to_string(ctx)
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_default();
                    let bytes = input.as_bytes();
                    let arr = rquickjs::Array::new(ctx.clone()).unwrap();
                    for (i, &b) in bytes.iter().enumerate() {
                        let _ = arr.set(
                            i as u32,
                            Value::new_float(ctx.clone(), b as f64),
                            false,
                            ctx,
                        );
                    }
                    Ok(arr.into_value())
                },
            );
            let encoder = Object::new(ctx.clone()).unwrap().set(
                "encoding",
                rquickjs::String::from_str(ctx.clone(), "utf-8")
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            );
            win.set("encode", encode_fn).unwrap_or(());
            Ok(encoder.into_value())
        });
    ctx.globals()
        .set("TextEncoder", text_encoder_ctor)
        .unwrap_or(());

    // TextDecoder
    let text_decoder_ctor =
        Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let decode_fn = Function::new(
                ctx.clone(),
                |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
                    let input = args
                        .get(0)
                        .cloned()
                        .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
                    // Försök tolka som array/typed array
                    let mut bytes: Vec<u8> = Vec::new();
                    if let Some(obj) = input.as_object() {
                        // Hämta length och iterera
                        if let Ok(len) = obj.get("length", ctx) {
                            let len = len.as_float().or_else(|| Some(0.0)).unwrap_or(0.0) as usize;
                            for i in 0..len.min(1_000_000) {
                                if let Ok(val) = obj.get(i as u32, ctx) {
                                    bytes
                                        .push(val.as_float().or_else(|| Some(0.0)).unwrap_or(0.0)
                                            as u8);
                                }
                            }
                        }
                    }
                    let decoded = String::from_utf8_lossy(&bytes).to_string();
                    Ok(rquickjs::String::from_str(
                        ctx.clone(),
                        decoded
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ))
                },
            );
            let decoder = Object::new(ctx.clone()).unwrap().set(
                "encoding",
                rquickjs::String::from_str(ctx.clone(), "utf-8")
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            );
            win.set("decode", decode_fn).unwrap_or(());
            Ok(decoder.into_value())
        });
    ctx.globals()
        .set("TextDecoder", text_decoder_ctor)
        .unwrap_or(());

    // URL constructor
    let url_ctor = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let url_str = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let base_str = if args.len() > 1 {
                args.get(1)
                    .cloned()
                    .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                    .to_string(ctx)
                    .map(|s| s.to_std_string_escaped())
                    .ok()
            } else {
                None
            };

            let full_url = if let Some(base) = base_str {
                if url_str.starts_with("http") {
                    url_str.clone()
                } else {
                    format!(
                        "{}/{}",
                        base.trim_end_matches('/'),
                        url_str.trim_start_matches('/')
                    )
                }
            } else {
                url_str.clone()
            };

            // Parsa URL-delar
            let (protocol, hostname, pathname, search, hash) = parse_url_parts(&full_url);

            // searchParams
            let sp_obj = Object::new(ctx.clone()).unwrap();
            let params = parse_query_string(&search);
            let params_clone = params.clone();
            let sp_get = Function::new(
                ctx.clone(),
                move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
                    let key = args
                        .get_or_undefined(0)
                        .to_string(ctx)
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_default();
                    match params_clone.iter().find(|(k, _)| k == &key) {
                        Some((_, v)) => Ok(rquickjs::String::from_str(
                            ctx.clone(),
                            v.as_str()
                                .map(|s| s.into_value())
                                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                        )),
                        None => Ok(Value::new_null(ctx.clone())),
                    }
                },
            )
            .unwrap();
            let params_clone2 = params.clone();
            let sp_has = Function::new(
                ctx.clone(),
                move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
                    let key = args
                        .get_or_undefined(0)
                        .to_string(ctx)
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_default();
                    Ok(Value::from(params_clone2.iter().any(|(k, _)| k == &key)))
                },
            )
            .unwrap();
            let sp_tostring = {
                let s = search.clone();
                unsafe {
                    Function::new(move |_this, _args, _ctx| {
                        Ok(rquickjs::String::from_str(
                            ctx.clone(),
                            s.as_str()
                                .map(|s| s.into_value())
                                .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                        ))
                    })
                }
            };
            let _ = sp_obj.set("get", sp_get.to_js_function(ctx.realm()), false, ctx);
            let _ = sp_obj.set("has", sp_has.to_js_function(ctx.realm()), false, ctx);
            let _ = sp_obj.set(
                "toString",
                sp_tostring.to_js_function(ctx.realm()),
                false,
                ctx,
            );

            let url_obj = Object::new(ctx.clone())
                .unwrap()
                .set(
                    "href",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        full_url
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .set(
                    "protocol",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        protocol
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .set(
                    "hostname",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        hostname
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .set(
                    "pathname",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        pathname
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .set(
                    "search",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        search
                            .as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .set(
                    "hash",
                    rquickjs::String::from_str(
                        ctx.clone(),
                        hash.as_str()
                            .map(|s| s.into_value())
                            .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                    ),
                )
                .property(
                    "origin",
                    Value::from(js_string!(format!(
                        "{}://{}",
                        protocol.trim_end_matches(':'),
                        hostname
                    )
                    .as_str())),
                );
            let _ = url_obj.set("searchParams", sp_obj);

            Ok(url_obj.into_value())
        },
    );
    ctx.globals().set("URL", url_ctor).unwrap_or(());

    // AbortController stub
    let abort_ctor = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        let signal = Object::new(ctx.clone())
            .unwrap()
            .set("aborted", Value::new_bool(ctx.clone(), false));
        win.set("reason", Value::new_undefined(ctx.clone()))
            .unwrap_or(());
        let signal_clone = signal.clone();
        let abort_fn = Function::new(
            ctx.clone(),
            move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
                let _ = signal_clone.set("aborted", Value::new_bool(ctx.clone(), true), false, ctx);
                Ok(Value::new_undefined(ctx.clone()))
            },
        )
        .unwrap();
        let controller = Object::new(ctx.clone()).unwrap();
        win.set("abort", abort_fn).unwrap_or(());
        let _ = controller.set("signal", signal);
        Ok(controller.into_value())
    });
    ctx.globals()
        .set("AbortController", abort_ctor)
        .unwrap_or(());

    // structuredClone — JSON roundtrip-approximation
    let structured_clone = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let val = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            // Använda JSON.stringify → JSON.parse som approximation
            let json_str = val
                .as_string()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_default();
            Ok(rquickjs::String::from_str(
                ctx.clone(),
                json_str
                    .as_str()
                    .map(|s| s.into_value())
                    .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
            ))
        },
    );
    ctx.globals()
        .set("structuredClone", structured_clone)
        .unwrap_or(());

    // crypto.randomUUID() och getRandomValues()
    let random_uuid = Function::new(ctx.clone(), |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
        // Generera pseudo-UUID v4 med enkel hash
        let ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let uuid = format!(
            "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
            (ts & 0xFFFFFFFF) as u32,
            ((ts >> 32) & 0xFFFF) as u16,
            ((ts >> 48) & 0x0FFF) as u16,
            (0x8000 | ((ts >> 60) & 0x3FFF)) as u16,
            ((ts.wrapping_mul(6364136223846793005)) & 0xFFFFFFFFFFFF) as u64,
        );
        Ok(rquickjs::String::from_str(ctx.clone(), &uuid)?.into_value())
    })
    .unwrap();
    let get_random_values = Function::new(
        ctx.clone(),
        |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            // Fyll array med pseudo-random bytes
            let arr = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()));
            if let Some(obj) = arr.as_object() {
                if let Ok(len) = obj.get("length", ctx) {
                    let len = len.as_float().or_else(|| Some(0.0)).unwrap_or(0.0) as usize;
                    let ts = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos();
                    for i in 0..len.min(65536) {
                        let val = ((ts.wrapping_mul(i as u128 + 1).wrapping_add(i as u128 * 7))
                            & 0xFF) as f64;
                        let _ = obj.set(i, Value::new_int(ctx.clone(), val));
                    }
                }
            }
            Ok(arr.clone())
        },
    );
    let crypto = Object::new(ctx.clone()).unwrap();
    win.set("randomUUID", random_uuid).unwrap_or(());
    win.set("getRandomValues", get_random_values).unwrap_or(());
    ctx.globals().set("crypto", crypto).unwrap_or(());
}

// ─── Console-objekt ─────────────────────────────────────────────────────────

fn register_console(ctx: &Ctx<'_>, state: SharedState) -> rquickjs::Result<()> {
    // Fånga console-output i BridgeState (Fas 19)
    let make_log = |st: SharedState, prefix: &'static str| unsafe {
        Function::new(move |_this, args, ctx| {
            let parts: Vec<String> = (0..args.len())
                .map(|i| {
                    args.get_or_undefined(i)
                        .to_string(ctx)
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_else(|_| "undefined".to_string())
                })
                .collect();
            let msg = if prefix.is_empty() {
                parts.join(" ")
            } else {
                format!("[{}] {}", prefix, parts.join(" "))
            };
            st.borrow_mut().console_output.push(msg);
            Ok(Value::new_undefined(ctx.clone()))
        })
    };

    let console = Object::new(ctx.clone())
        .unwrap()
        .function(make_log(Rc::clone(&state), ""), "log", 1)
        .function(make_log(Rc::clone(&state), "WARN"), "warn", 1)
        .function(make_log(Rc::clone(&state), "ERROR"), "error", 1)
        .function(make_log(state, "INFO"), "info", 1);

    ctx.globals().set("console", console).unwrap_or(());
}

// ─── localStorage / sessionStorage — in-memory sandbox (Fas 19) ─────────────

fn register_storage(
    ctx: &Ctx<'_>,
    state: SharedState,
    name: &str,
    is_local: bool,
) -> rquickjs::Result<()> {
    let st_get = Rc::clone(&state);
    let is_local_get = is_local;
    let get_item = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let key = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let s = st_get.borrow();
            let storage = if is_local_get {
                &s.local_storage
            } else {
                &s.session_storage
            };
            match storage.get(&key) {
                Some(v) => Ok(rquickjs::String::from_str(
                    ctx.clone(),
                    v.as_str()
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                )),
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();

    let st_set = Rc::clone(&state);
    let is_local_set = is_local;
    let set_item = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let key = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let val = args
                .get_or_undefined(1)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let mut s = st_set.borrow_mut();
            let storage = if is_local_set {
                &mut s.local_storage
            } else {
                &mut s.session_storage
            };
            // Begränsa storlek: max 100 nycklar, max 64KB per värde
            if storage.len() < 100 && val.len() < 65536 {
                storage.insert(key, val);
            }
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();

    let st_rem = Rc::clone(&state);
    let is_local_rem = is_local;
    let remove_item = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let key = args
                .get_or_undefined(0)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let mut s = st_rem.borrow_mut();
            let storage = if is_local_rem {
                &mut s.local_storage
            } else {
                &mut s.session_storage
            };
            storage.remove(&key);
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();

    let st_clear = Rc::clone(&state);
    let is_local_clear = is_local;
    let clear = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let mut s = st_clear.borrow_mut();
            let storage = if is_local_clear {
                &mut s.local_storage
            } else {
                &mut s.session_storage
            };
            storage.clear();
            Ok(Value::new_undefined(ctx.clone()))
        },
    )
    .unwrap();

    let st_key = Rc::clone(&state);
    let is_local_key = is_local;
    let key_fn = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>, args: Rest<Value<'_>>| -> rquickjs::Result<Value<'_>> {
            let index = args
                .get(0)
                .cloned()
                .unwrap_or_else(|| Value::new_undefined(ctx.clone()))
                .as_float()
                .or_else(|| Some(0.0))
                .unwrap_or(0.0) as usize;
            let s = st_key.borrow();
            let storage = if is_local_key {
                &s.local_storage
            } else {
                &s.session_storage
            };
            match storage.keys().nth(index) {
                Some(k) => Ok(rquickjs::String::from_str(
                    ctx.clone(),
                    k.as_str()
                        .map(|s| s.into_value())
                        .unwrap_or_else(|_| Value::new_undefined(ctx.clone())),
                )),
                None => Ok(Value::new_null(ctx.clone())),
            }
        },
    )
    .unwrap();

    let st_len = Rc::clone(&state);
    let is_local_len = is_local;

    let storage_obj = Object::new(ctx.clone()).unwrap();
    obj.set("getItem", get_item).unwrap_or(());
    obj.set("setItem", set_item).unwrap_or(());
    obj.set("removeItem", remove_item).unwrap_or(());
    obj.set("clear", clear).unwrap_or(());
    obj.set("key", key_fn).unwrap_or(());

    // Sätt length som getter (via closure)
    let length_getter = Function::new(
        ctx.clone(),
        move |ctx: Ctx<'_>| -> rquickjs::Result<Value<'_>> {
            let s = st_len.borrow();
            let storage = if is_local_len {
                &s.local_storage
            } else {
                &s.session_storage
            };
            Ok(Value::from(storage.len() as f64))
        },
    )
    .unwrap();
    let _ = storage_obj.set("length", length_getter, false, ctx);

    ctx.register_global_property(name, storage_obj)
        .unwrap_or(());
}

// ─── Style & Layout Helpers ─────────────────────────────────────────────────

/// Parsea inline CSS-stilar från style-attribut till HashMap
fn parse_inline_styles(style_attr: &str) -> std::collections::HashMap<String, String> {
    let mut styles = std::collections::HashMap::new();
    for part in style_attr.split(';') {
        let part = part.trim();
        if let Some(colon_pos) = part.find(':') {
            let prop = part[..colon_pos].trim().to_lowercase();
            let val = part[colon_pos + 1..].trim().to_string();
            if !prop.is_empty() {
                styles.insert(prop, val);
            }
        }
    }
    styles
}

/// Serialisera inline CSS-stilar till style-attribut-sträng
fn serialize_inline_styles(styles: &std::collections::HashMap<String, String>) -> String {
    let mut parts: Vec<String> = styles
        .iter()
        .map(|(k, v)| format!("{}: {}", k, v))
        .collect();
    parts.sort();
    parts.join("; ")
}

/// Parsea px-värde från CSS-egenskap
fn parse_px_value(value: &str) -> Option<f64> {
    value
        .trim()
        .strip_suffix("px")
        .unwrap_or(value.trim())
        .parse::<f64>()
        .ok()
}

/// Estimera layout-rect baserat på tagg + inline styles
fn estimate_layout_rect(arena: &ArenaDom, key: NodeKey) -> (f64, f64, f64, f64) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return (0.0, 0.0, 100.0, 30.0),
    };
    let style_str = node.get_attr("style").unwrap_or("");
    let styles = parse_inline_styles(style_str);
    let tag = node.tag.as_deref().unwrap_or("");
    let (default_w, default_h) = match tag {
        "div" | "section" | "main" | "article" | "header" | "footer" | "nav" | "form" => {
            (1024.0, 50.0)
        }
        "p" => (1024.0, 20.0),
        "h1" => (1024.0, 40.0),
        "h2" => (1024.0, 36.0),
        "h3" => (1024.0, 32.0),
        "h4" | "h5" | "h6" => (1024.0, 28.0),
        "button" => (80.0, 36.0),
        "input" => (200.0, 36.0),
        "select" => (200.0, 36.0),
        "textarea" => (300.0, 80.0),
        "a" | "span" | "label" => (60.0, 20.0),
        "img" => {
            let iw = node
                .get_attr("width")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(300.0);
            let ih = node
                .get_attr("height")
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(200.0);
            (iw, ih)
        }
        "li" => (1024.0, 24.0),
        "ul" | "ol" => (1024.0, 100.0),
        "table" => (1024.0, 200.0),
        "tr" => (1024.0, 30.0),
        "td" | "th" => (200.0, 30.0),
        _ => (100.0, 30.0),
    };

    let width = styles
        .get("width")
        .and_then(|v| parse_px_value(v))
        .unwrap_or(default_w);
    let height = styles
        .get("height")
        .and_then(|v| parse_px_value(v))
        .unwrap_or(default_h);
    // Estimera y-position från syskon-ordning
    let y = match node.parent.and_then(|p| arena.nodes.get(p)) {
        Some(parent) => {
            let my_idx = parent.children.iter().position(|&c| c == key).unwrap_or(0);
            (my_idx as f64) * 30.0
        }
        None => 0.0,
    };

    (0.0, y, width, height)
}

/// Tag-baserade CSS-defaults för getComputedStyle
fn get_tag_style_defaults(tag: &str) -> std::collections::HashMap<String, String> {
    let mut defaults = std::collections::HashMap::new();
    let display = match tag {
        "span" | "a" | "strong" | "em" | "b" | "i" | "label" | "img" => "inline",
        "button" | "input" | "select" | "textarea" => "inline-block",
        _ => "block",
    };
    let font_size = match tag {
        "h1" => "32px",
        "h2" => "24px",
        "h3" => "18.72px",
        "h4" => "16px",
        "h5" => "13.28px",
        "h6" => "10.72px",
        _ => "16px",
    };
    defaults.insert("display".to_string(), display.to_string());
    defaults.insert("visibility".to_string(), "visible".to_string());
    defaults.insert("position".to_string(), "static".to_string());
    defaults.insert("opacity".to_string(), "1".to_string());
    defaults.insert("overflow".to_string(), "visible".to_string());
    defaults.insert("font-size".to_string(), font_size.to_string());
    defaults.insert("color".to_string(), "rgb(0, 0, 0)".to_string());
    defaults.insert(
        "background-color".to_string(),
        "rgba(0, 0, 0, 0)".to_string(),
    );
    defaults.insert("width".to_string(), "auto".to_string());
    defaults.insert("height".to_string(), "auto".to_string());
    defaults.insert("margin".to_string(), "0px".to_string());
    defaults.insert("padding".to_string(), "0px".to_string());
    defaults.insert("z-index".to_string(), "auto".to_string());
    defaults.insert("pointer-events".to_string(), "auto".to_string());
    defaults.insert("box-sizing".to_string(), "content-box".to_string());
    defaults
}

// ─── Media Query Matching (Fas 19) ──────────────────────────────────────────

/// Parsa enkel CSS media query och matcha mot viewport
fn parse_media_query_matches(query: &str, viewport_width: f64, viewport_height: f64) -> bool {
    let query = query.trim().to_lowercase();

    // Hantera "all", "screen", "(prefers-color-scheme: light)" etc
    if query == "all" || query == "screen" {
        return true;
    }
    if query == "print" {
        return false;
    }

    // Parsa bredd-queries: (min-width: 768px), (max-width: 1024px)
    if let Some(val) = extract_px_from_query(&query, "min-width") {
        return viewport_width >= val;
    }
    if let Some(val) = extract_px_from_query(&query, "max-width") {
        return viewport_width <= val;
    }
    if let Some(val) = extract_px_from_query(&query, "min-height") {
        return viewport_height >= val;
    }
    if let Some(val) = extract_px_from_query(&query, "max-height") {
        return viewport_height <= val;
    }

    // (prefers-color-scheme: light)
    if query.contains("prefers-color-scheme") {
        return query.contains("light");
    }

    // (prefers-reduced-motion: no-preference)
    if query.contains("prefers-reduced-motion") {
        return query.contains("no-preference");
    }

    // Okänd query — returnera true som default
    true
}

/// Extrahera px-värde från media query-uttryck
fn extract_px_from_query(query: &str, prop: &str) -> Option<f64> {
    let pos = query.find(prop)?;
    let rest = &query[pos + prop.len()..];
    let colon = rest.find(':')?;
    let after_colon = rest[colon + 1..].trim();
    // Hitta siffran innan "px" eller ")"
    let end = after_colon.find([')', 'p']).unwrap_or(after_colon.len());
    after_colon[..end].trim().parse::<f64>().ok()
}

// ─── Base64 encode/decode (Fas 19) ──────────────────────────────────────────

/// Enkel base64-avkodning (atob)
fn base64_decode(input: &str) -> Option<String> {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input: Vec<u8> = input
        .bytes()
        .filter(|b| *b != b'\n' && *b != b'\r')
        .collect();
    let mut output = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in &input {
        if b == b'=' {
            break;
        }
        let val = CHARS.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    String::from_utf8(output).ok()
}

/// Enkel base64-kodning (btoa)
fn base64_encode(input: &str) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut result = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b0 = bytes[i] as u32;
        let b1 = if i + 1 < bytes.len() {
            bytes[i + 1] as u32
        } else {
            0
        };
        let b2 = if i + 2 < bytes.len() {
            bytes[i + 2] as u32
        } else {
            0
        };
        let triple = (b0 << 16) | (b1 << 8) | b2;
        result.push(CHARS[((triple >> 18) & 0x3F) as usize] as char);
        result.push(CHARS[((triple >> 12) & 0x3F) as usize] as char);
        if i + 1 < bytes.len() {
            result.push(CHARS[((triple >> 6) & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        if i + 2 < bytes.len() {
            result.push(CHARS[(triple & 0x3F) as usize] as char);
        } else {
            result.push('=');
        }
        i += 3;
    }
    result
}

// ─── URL-parsing (Fas 19) ────────────────────────────────────────────────────

/// Parsa URL i delar: (protocol, hostname, pathname, search, hash)
fn parse_url_parts(url: &str) -> (String, String, String, String, String) {
    let (url_no_hash, hash) = match url.find('#') {
        Some(pos) => (&url[..pos], url[pos..].to_string()),
        None => (url, String::new()),
    };
    let (url_no_search, search) = match url_no_hash.find('?') {
        Some(pos) => (&url_no_hash[..pos], url_no_hash[pos..].to_string()),
        None => (url_no_hash, String::new()),
    };

    let (protocol, rest) = if let Some(pos) = url_no_search.find("://") {
        (
            format!("{}:", &url_no_search[..pos]),
            &url_no_search[pos + 3..],
        )
    } else {
        ("https:".to_string(), url_no_search)
    };

    let (hostname, pathname) = match rest.find('/') {
        Some(pos) => (rest[..pos].to_string(), rest[pos..].to_string()),
        None => (rest.to_string(), "/".to_string()),
    };

    (protocol, hostname, pathname, search, hash)
}

/// Parsa query string till nyckel-värde-par
fn parse_query_string(search: &str) -> Vec<(String, String)> {
    let s = search.strip_prefix('?').unwrap_or(search);
    if s.is_empty() {
        return vec![];
    }
    s.split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let key = parts.next()?.to_string();
            let val = parts.next().unwrap_or("").to_string();
            Some((key, val))
        })
        .collect()
}

/// Kolla om en nod är kopplad till document-roten via parent-kedjan
fn is_connected_to_document(arena: &ArenaDom, key: NodeKey) -> bool {
    let mut current = key;
    loop {
        if current == arena.document {
            return true;
        }
        match arena.nodes.get(current).and_then(|n| n.parent) {
            Some(parent) => current = parent,
            None => return false,
        }
    }
}

/// Kolla om ancestor-noden innehåller descendant (rekursivt)
fn node_contains(arena: &ArenaDom, ancestor: NodeKey, descendant: NodeKey) -> bool {
    if ancestor == descendant {
        return true;
    }
    let node = match arena.nodes.get(ancestor) {
        Some(n) => n,
        None => return false,
    };
    for &child in &node.children {
        if node_contains(arena, child, descendant) {
            return true;
        }
    }
    false
}

// ─── DOM Query Helpers ──────────────────────────────────────────────────────

/// Hitta element via attributvärde (rekursiv)
fn find_by_attr_value(arena: &ArenaDom, attr: &str, value: &str) -> Option<NodeKey> {
    find_by_attr_recursive(arena, arena.document, attr, value)
}

fn find_by_attr_recursive(
    arena: &ArenaDom,
    key: NodeKey,
    attr: &str,
    value: &str,
) -> Option<NodeKey> {
    let node = arena.nodes.get(key)?;
    if node.node_type == NodeType::Element && node.get_attr(attr) == Some(value) {
        return Some(key);
    }
    for &child in &node.children {
        if let Some(found) = find_by_attr_recursive(arena, child, attr, value) {
            return Some(found);
        }
    }
    None
}

/// Hitta element via taggnamn (första match)
fn find_by_tag_name(arena: &ArenaDom, key: NodeKey, tag: &str) -> Option<NodeKey> {
    let node = arena.nodes.get(key)?;
    if node.tag.as_deref() == Some(tag) {
        return Some(key);
    }
    for &child in &node.children {
        if let Some(found) = find_by_tag_name(arena, child, tag) {
            return Some(found);
        }
    }
    None
}

// ─── CSS Selector Matching ───────────────────────────────────────────────────

/// Kontrollera om en nod matchar en CSS-selektor
///
/// Stöder: *, #id, .class, tag, tag.class, [attr], [attr="val"],
/// [attr^="val"], [attr$="val"], [attr*="val"], [attr~="val"], [attr|="val"],
/// :first-child, :last-child, :nth-child(An+B), :only-child,
/// :first-of-type, :last-of-type, :nth-of-type(An+B),
/// :root, :empty, :not(sel), :checked, :disabled, :enabled, :focus,
/// kombinatorer: (mellanslag), >, +, ~. Komma-separerade selektorer.
fn matches_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }

    // Komma-separerade selektorer — matcha om någon matchar
    if selector.contains(',') {
        return selector
            .split(',')
            .any(|s| matches_single_selector(arena, key, s.trim()));
    }

    // Descendant/child/sibling-kombinator
    if selector.contains(' ')
        || selector.contains('>')
        || selector.contains('+')
        || selector.contains('~')
    {
        return matches_combinator_selector(arena, key, selector);
    }

    matches_single_selector(arena, key, selector)
}

/// Attribut-matchningsoperator
#[derive(Debug, Clone, Copy, PartialEq)]
enum AttrOp {
    /// [attr="val"] — exakt matchning
    Exact,
    /// [attr^="val"] — börjar med
    StartsWith,
    /// [attr$="val"] — slutar med
    EndsWith,
    /// [attr*="val"] — innehåller
    Contains,
    /// [attr~="val"] — ordmatchning (mellanslag-separerat)
    WordMatch,
    /// [attr|="val"] — bindestreck-prefix (val eller val-*)
    HyphenPrefix,
    /// [attr] — bara existens, inget värde
    Exists,
}

/// Matcha en enkel selektor (utan kombinatorer)
fn matches_single_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    let node = match arena.nodes.get(key) {
        Some(n) if n.node_type == NodeType::Element => n,
        _ => return false,
    };

    let selector = selector.trim();
    if selector.is_empty() {
        return false;
    }

    // Universell selektor
    if selector == "*" {
        return true;
    }

    // Rena pseudo-selektorer utan tagg
    if selector == ":first-child" {
        return is_first_child(arena, key);
    }
    if selector == ":last-child" {
        return is_last_child(arena, key);
    }
    if selector == ":root" {
        return is_root_element(arena, key);
    }
    if selector == ":empty" {
        return is_empty_element(arena, key);
    }
    if selector == ":only-child" {
        return element_index_among_siblings(arena, key)
            .map(|(_, total)| total == 1)
            .unwrap_or(false);
    }
    if selector == ":first-of-type" {
        return type_index_among_siblings(arena, key)
            .map(|(pos, _)| pos == 1)
            .unwrap_or(false);
    }
    if selector == ":last-of-type" {
        return type_index_among_siblings(arena, key)
            .map(|(pos, total)| pos == total)
            .unwrap_or(false);
    }
    if selector == ":checked" {
        return node.get_attr("checked").is_some() || node.get_attr("selected").is_some();
    }
    if selector == ":disabled" {
        return node.get_attr("disabled").is_some();
    }
    if selector == ":enabled" {
        let is_form_el = matches!(
            node.tag.as_deref(),
            Some("input" | "select" | "textarea" | "button")
        );
        return is_form_el && node.get_attr("disabled").is_none();
    }
    if selector == ":focus" {
        // Utan tillgång till BridgeState kollar vi data-focused-attribut
        return node.get_attr("data-focused").is_some();
    }

    // Parsea selektor-delar: tag, #id, .class, [attr], [attr="val"], :pseudo
    let mut remaining = selector;
    let mut required_tag: Option<&str> = None;
    let mut required_id: Option<&str> = None;
    let mut required_classes: Vec<&str> = Vec::new();
    let mut required_attrs: Vec<(String, Option<String>, AttrOp)> = Vec::new();
    let mut require_first_child = false;
    let mut require_last_child = false;
    let mut require_root = false;
    let mut require_empty = false;
    let mut require_only_child = false;
    let mut require_first_of_type = false;
    let mut require_last_of_type = false;
    let mut require_checked = false;
    let mut require_disabled = false;
    let mut require_enabled = false;
    let mut require_focus = false;
    let mut nth_child_expr: Option<(i32, i32)> = None;
    let mut nth_of_type_expr: Option<(i32, i32)> = None;
    let mut not_selectors: Vec<String> = Vec::new();
    let mut is_universal = false;

    // Universell selektor med pseudo
    if remaining.starts_with('*') {
        is_universal = true;
        remaining = &remaining[1..];
    } else if remaining.starts_with(|c: char| c.is_ascii_alphabetic()) {
        // Extrahera tagg (om den börjar med bokstav)
        let end = remaining
            .find(|c: char| ['#', '.', '[', ':'].contains(&c))
            .unwrap_or(remaining.len());
        required_tag = Some(&remaining[..end]);
        remaining = &remaining[end..];
    }

    // Parsea resterande delar
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix('#') {
            let end = rest
                .find(|c: char| ['.', '[', ':'].contains(&c))
                .unwrap_or(rest.len());
            required_id = Some(&rest[..end]);
            remaining = &rest[end..];
        } else if let Some(rest) = remaining.strip_prefix('.') {
            let end = rest
                .find(|c: char| ['#', '.', '[', ':'].contains(&c))
                .unwrap_or(rest.len());
            required_classes.push(&rest[..end]);
            remaining = &rest[end..];
        } else if let Some(rest) = remaining.strip_prefix('[') {
            let bracket_end = match rest.find(']') {
                Some(e) => e,
                None => break,
            };
            let attr_spec = &rest[..bracket_end];
            if let Some(eq_pos) = attr_spec.find('=') {
                let before_eq = &attr_spec[..eq_pos];
                let attr_val = attr_spec[eq_pos + 1..].trim_matches('"').trim_matches('\'');

                if let Some(attr_name) = before_eq.strip_suffix('^') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::StartsWith,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('$') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::EndsWith,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('*') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::Contains,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('~') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::WordMatch,
                    ));
                } else if let Some(attr_name) = before_eq.strip_suffix('|') {
                    required_attrs.push((
                        attr_name.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::HyphenPrefix,
                    ));
                } else {
                    required_attrs.push((
                        before_eq.to_string(),
                        Some(attr_val.to_string()),
                        AttrOp::Exact,
                    ));
                }
            } else {
                required_attrs.push((attr_spec.to_string(), None, AttrOp::Exists));
            }
            remaining = &rest[bracket_end + 1..];
        } else if let Some(rest) = remaining.strip_prefix(":not(") {
            // Hitta matchande avslutande parentes
            if let Some(end) = rest.find(')') {
                let inner = &rest[..end];
                not_selectors.push(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":nth-of-type(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_of_type_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":nth-child(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_child_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":first-child") {
            require_first_child = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":last-child") {
            require_last_child = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":first-of-type") {
            require_first_of_type = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":last-of-type") {
            require_last_of_type = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":only-child") {
            require_only_child = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":root") {
            require_root = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":empty") {
            require_empty = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":checked") {
            require_checked = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":disabled") {
            require_disabled = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":enabled") {
            require_enabled = true;
            remaining = rest;
        } else if let Some(rest) = remaining.strip_prefix(":focus") {
            require_focus = true;
            remaining = rest;
        } else {
            break;
        }
    }

    // Verifiera tagg (om inte universell)
    if let Some(tag) = required_tag {
        if node.tag.as_deref() != Some(tag) {
            return false;
        }
    }
    // Universell selektor kräver ingen tagg-matchning (alla element matchar)
    let _ = is_universal;

    if let Some(id) = required_id {
        if node.get_attr("id") != Some(id) {
            return false;
        }
    }
    for cls in &required_classes {
        let has = node
            .get_attr("class")
            .map(|c| c.split_whitespace().any(|x| x == *cls))
            .unwrap_or(false);
        if !has {
            return false;
        }
    }

    // Verifiera attribut med operator
    for (attr, val, op) in &required_attrs {
        match op {
            AttrOp::Exists => {
                if !node.has_attr(attr) {
                    return false;
                }
            }
            AttrOp::Exact => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                if node.get_attr(attr) != Some(expected) {
                    return false;
                }
            }
            AttrOp::StartsWith => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.starts_with(expected) => {}
                    _ => return false,
                }
            }
            AttrOp::EndsWith => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.ends_with(expected) => {}
                    _ => return false,
                }
            }
            AttrOp::Contains => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.contains(expected) => {}
                    _ => return false,
                }
            }
            AttrOp::WordMatch => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual) if actual.split_whitespace().any(|w| w == expected) => {}
                    _ => return false,
                }
            }
            AttrOp::HyphenPrefix => {
                let expected = match val {
                    Some(v) => v.as_str(),
                    None => return false,
                };
                match node.get_attr(attr) {
                    Some(actual)
                        if actual == expected || actual.starts_with(&format!("{}-", expected)) => {}
                    _ => return false,
                }
            }
        }
    }

    // Pseudo-klass-verifieringar
    if require_first_child && !is_first_child(arena, key) {
        return false;
    }
    if require_last_child && !is_last_child(arena, key) {
        return false;
    }
    if require_root && !is_root_element(arena, key) {
        return false;
    }
    if require_empty && !is_empty_element(arena, key) {
        return false;
    }
    if require_only_child {
        let is_only = element_index_among_siblings(arena, key)
            .map(|(_, total)| total == 1)
            .unwrap_or(false);
        if !is_only {
            return false;
        }
    }
    if require_first_of_type {
        let is_first = type_index_among_siblings(arena, key)
            .map(|(pos, _)| pos == 1)
            .unwrap_or(false);
        if !is_first {
            return false;
        }
    }
    if require_last_of_type {
        let is_last = type_index_among_siblings(arena, key)
            .map(|(pos, total)| pos == total)
            .unwrap_or(false);
        if !is_last {
            return false;
        }
    }
    if require_checked && node.get_attr("checked").is_none() && node.get_attr("selected").is_none()
    {
        return false;
    }
    if require_disabled && node.get_attr("disabled").is_none() {
        return false;
    }
    if require_enabled {
        let is_form_el = matches!(
            node.tag.as_deref(),
            Some("input" | "select" | "textarea" | "button")
        );
        if !is_form_el || node.get_attr("disabled").is_some() {
            return false;
        }
    }
    if require_focus && node.get_attr("data-focused").is_none() {
        return false;
    }
    if let Some((a, b)) = nth_child_expr {
        let matched = element_index_among_siblings(arena, key)
            .map(|(pos, _)| matches_nth(pos, a, b))
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }
    if let Some((a, b)) = nth_of_type_expr {
        let matched = type_index_among_siblings(arena, key)
            .map(|(pos, _)| matches_nth(pos, a, b))
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }

    // :not()-verifiering — negera matchning mot inre selektor
    for not_sel in &not_selectors {
        if matches_single_selector(arena, key, not_sel) {
            return false;
        }
    }

    true
}

/// Matcha selektor med kombinatorer (>, mellanslag, +, ~)
fn matches_combinator_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    // Splitta vid whitespace och separera kombinatorer
    let parts: Vec<&str> = selector.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    // Sista delen matchar mot noden
    let last = parts[parts.len() - 1];
    if matches!(last, ">" | "+" | "~") {
        return false; // Felaktig selektor — slutar med kombinator
    }
    if !matches_single_selector(arena, key, last) {
        return false;
    }

    if parts.len() == 1 {
        return true;
    }

    // Identifiera kombinator-typ: >, +, ~ eller descendant (mellanslag)
    let combinator = if parts.len() >= 2 {
        match parts[parts.len() - 2] {
            ">" => ">",
            "+" => "+",
            "~" => "~",
            _ => " ", // descendant
        }
    } else {
        " "
    };

    let ancestor_sel = if combinator != " " {
        // Explicit kombinator — skippa kombinatorn
        if parts.len() < 3 {
            return false;
        }
        parts[..parts.len() - 2].join(" ")
    } else {
        parts[..parts.len() - 1].join(" ")
    };

    match combinator {
        ">" => {
            // Direkt förälder måste matcha
            if let Some(parent) = arena.nodes.get(key).and_then(|n| n.parent) {
                matches_selector(arena, parent, &ancestor_sel)
            } else {
                false
            }
        }
        "+" => {
            // Föregående element-syskon måste matcha
            if let Some(prev) = prev_element_sibling(arena, key) {
                matches_selector(arena, prev, &ancestor_sel)
            } else {
                false
            }
        }
        "~" => {
            // Något föregående element-syskon måste matcha
            let prev_siblings = all_prev_element_siblings(arena, key);
            prev_siblings
                .iter()
                .any(|&sib| matches_selector(arena, sib, &ancestor_sel))
        }
        _ => {
            // Descendant — valfri förfader måste matcha
            let mut current = arena.nodes.get(key).and_then(|n| n.parent);
            while let Some(ancestor) = current {
                if matches_selector(arena, ancestor, &ancestor_sel) {
                    return true;
                }
                current = arena.nodes.get(ancestor).and_then(|n| n.parent);
            }
            false
        }
    }
}

/// Kolla om nod är första element-barnet
fn is_first_child(arena: &ArenaDom, key: NodeKey) -> bool {
    let parent = match arena.nodes.get(key).and_then(|n| n.parent) {
        Some(p) => p,
        None => return false,
    };
    arena
        .nodes
        .get(parent)
        .map(|n| {
            n.children.iter().find(|&&c| {
                arena
                    .nodes
                    .get(c)
                    .map(|cn| cn.node_type == NodeType::Element)
                    .unwrap_or(false)
            }) == Some(&key)
        })
        .unwrap_or(false)
}

/// Kolla om nod är sista element-barnet
fn is_last_child(arena: &ArenaDom, key: NodeKey) -> bool {
    let parent = match arena.nodes.get(key).and_then(|n| n.parent) {
        Some(p) => p,
        None => return false,
    };
    arena
        .nodes
        .get(parent)
        .map(|n| {
            n.children.iter().rfind(|&&c| {
                arena
                    .nodes
                    .get(c)
                    .map(|cn| cn.node_type == NodeType::Element)
                    .unwrap_or(false)
            }) == Some(&key)
        })
        .unwrap_or(false)
}

/// Räkna nodens element-position bland sina syskon (1-indexed)
/// Returnerar (position, totalt_antal_element_syskon)
fn element_index_among_siblings(arena: &ArenaDom, key: NodeKey) -> Option<(usize, usize)> {
    let parent = arena.nodes.get(key)?.parent?;
    let parent_node = arena.nodes.get(parent)?;
    let mut pos = 0usize;
    let mut total = 0usize;
    let mut found = false;
    for &child in &parent_node.children {
        let is_element = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element)
            .unwrap_or(false);
        if is_element {
            total += 1;
            if child == key {
                pos = total;
                found = true;
            }
        }
    }
    if found {
        Some((pos, total))
    } else {
        None
    }
}

/// Räkna nodens position bland syskon av samma tagg-typ (1-indexed)
/// Returnerar (position, totalt_antal_av_samma_typ)
fn type_index_among_siblings(arena: &ArenaDom, key: NodeKey) -> Option<(usize, usize)> {
    let node = arena.nodes.get(key)?;
    let my_tag = node.tag.as_deref()?;
    let parent = node.parent?;
    let parent_node = arena.nodes.get(parent)?;
    let mut pos = 0usize;
    let mut total = 0usize;
    let mut found = false;
    for &child in &parent_node.children {
        let matches = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element && n.tag.as_deref() == Some(my_tag))
            .unwrap_or(false);
        if matches {
            total += 1;
            if child == key {
                pos = total;
                found = true;
            }
        }
    }
    if found {
        Some((pos, total))
    } else {
        None
    }
}

/// Parsa An+B-uttryck för :nth-child/:nth-of-type
fn parse_nth_expression(expr: &str) -> (i32, i32) {
    let expr = expr.trim();
    match expr {
        "odd" => (2, 1),
        "even" => (2, 0),
        s if s.contains('n') => {
            // Hantera varianter: "n", "2n", "-n", "2n+1", "2n-3", "-2n+1"
            let s = s.replace(' ', "");
            let n_pos = match s.find('n') {
                Some(p) => p,
                None => return (0, 0),
            };
            let a_part = &s[..n_pos];
            let a: i32 = match a_part {
                "" | "+" => 1,
                "-" => -1,
                other => other.parse().unwrap_or(0),
            };
            let after = &s[n_pos + 1..];
            let b: i32 = if after.is_empty() {
                0
            } else {
                after.parse().unwrap_or(0)
            };
            (a, b)
        }
        s => (0, s.parse().unwrap_or(0)),
    }
}

/// Kolla om position matchar An+B-uttryck
fn matches_nth(pos: usize, a: i32, b: i32) -> bool {
    let pos = pos as i32;
    if a == 0 {
        return pos == b;
    }
    let diff = pos - b;
    diff % a == 0 && diff / a >= 0
}

/// Hämta föregående element-syskon
fn prev_element_sibling(arena: &ArenaDom, key: NodeKey) -> Option<NodeKey> {
    let parent = arena.nodes.get(key)?.parent?;
    let parent_node = arena.nodes.get(parent)?;
    let mut prev: Option<NodeKey> = None;
    for &child in &parent_node.children {
        if child == key {
            return prev;
        }
        let is_element = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element)
            .unwrap_or(false);
        if is_element {
            prev = Some(child);
        }
    }
    None
}

/// Hämta alla föregående element-syskon
fn all_prev_element_siblings(arena: &ArenaDom, key: NodeKey) -> Vec<NodeKey> {
    let parent = match arena.nodes.get(key).and_then(|n| n.parent) {
        Some(p) => p,
        None => return vec![],
    };
    let parent_node = match arena.nodes.get(parent) {
        Some(n) => n,
        None => return vec![],
    };
    let mut result = vec![];
    for &child in &parent_node.children {
        if child == key {
            break;
        }
        let is_element = arena
            .nodes
            .get(child)
            .map(|n| n.node_type == NodeType::Element)
            .unwrap_or(false);
        if is_element {
            result.push(child);
        }
    }
    result
}

/// Kolla om nod är rotelementet (html)
fn is_root_element(arena: &ArenaDom, key: NodeKey) -> bool {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return false,
    };
    // Roten har document som förälder
    match node.parent {
        Some(p) => arena
            .nodes
            .get(p)
            .map(|pn| pn.node_type == NodeType::Document)
            .unwrap_or(false),
        None => false,
    }
}

/// Kolla om nod saknar barn-element och text
fn is_empty_element(arena: &ArenaDom, key: NodeKey) -> bool {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return false,
    };
    node.children.iter().all(|&c| {
        arena
            .nodes
            .get(c)
            .map(|cn| {
                // :empty = inga element- eller text-barn (kommentarer ok)
                cn.node_type != NodeType::Element && cn.node_type != NodeType::Text
            })
            .unwrap_or(())
    })
}

/// querySelector — hittar första matchande nod med full CSS-selektor
fn query_select_one(arena: &ArenaDom, selector: &str) -> Option<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }
    find_first_matching(arena, arena.document, selector)
}

/// Rekursiv sökning efter första matchande nod
fn find_first_matching(arena: &ArenaDom, key: NodeKey, selector: &str) -> Option<NodeKey> {
    let node = arena.nodes.get(key)?;
    if node.node_type == NodeType::Element && matches_selector(arena, key, selector) {
        return Some(key);
    }
    for &child in &node.children {
        if let Some(found) = find_first_matching(arena, child, selector) {
            return Some(found);
        }
    }
    None
}

/// querySelectorAll — returnerar alla matchande noder med full CSS-selektor
fn query_select_all(arena: &ArenaDom, selector: &str) -> Vec<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return vec![];
    }
    let mut results = vec![];
    find_all_matching(arena, arena.document, selector, &mut results);
    results
}

/// Rekursiv sökning efter alla matchande noder
fn find_all_matching(arena: &ArenaDom, key: NodeKey, selector: &str, results: &mut Vec<NodeKey>) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element && matches_selector(arena, key, selector) {
        results.push(key);
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_matching(arena, child, selector, results);
    }
}

/// Samla alla element med given klass
fn find_all_by_class(arena: &ArenaDom, key: NodeKey, class: &str, results: &mut Vec<NodeKey>) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element {
        if let Some(classes) = node.get_attr("class") {
            if classes.split_whitespace().any(|c| c == class) {
                results.push(key);
            }
        }
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_class(arena, child, class, results);
    }
}

/// Samla alla element med given tagg
fn find_all_by_tag(arena: &ArenaDom, key: NodeKey, tag: &str, results: &mut Vec<NodeKey>) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.tag.as_deref() == Some(tag) {
        results.push(key);
    }
    let children: Vec<NodeKey> = node.children.clone();
    for child in children {
        find_all_by_tag(arena, child, tag, results);
    }
}

// ─── NodeKey ↔ f64 konvertering ─────────────────────────────────────────────

/// Konvertera NodeKey till f64 för lagring i JS Number
///
/// SlotMap NodeKey innehåller index + generation. Vi lagrar raw index
/// som en f64 (JavaScript Number) — säkert för index < 2^53.
fn node_key_to_f64(key: NodeKey) -> f64 {
    // Använd Key::data() för att extrahera KeyData, sedan as_ffi() för raw u64
    use slotmap::Key;
    key.data().as_ffi() as f64
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

    // === Query Helpers ===

    #[test]
    fn test_find_by_attr_value() {
        let arena = make_arena(r##"<html><body><div id="target">Hej</div></body></html>"##);
        let result = find_by_attr_value(&arena, "id", "target");
        assert!(result.is_some(), "Borde hitta element med id='target'");
        let key = result.unwrap();
        assert_eq!(
            arena.tag_name(key),
            Some("div"),
            "Hittat element borde vara <div>"
        );
    }

    #[test]
    fn test_find_by_attr_missing() {
        let arena = make_arena(r#"<html><body><p>Ingen id</p></body></html>"#);
        assert!(
            find_by_attr_value(&arena, "id", "nonexistent").is_none(),
            "Borde returnera None för saknat id"
        );
    }

    #[test]
    fn test_query_select_id() {
        let arena = make_arena(r##"<html><body><span id="price">199 kr</span></body></html>"##);
        let result = query_select_one(&arena, "#price");
        assert!(result.is_some(), "Borde hitta #price");
    }

    #[test]
    fn test_query_select_class() {
        let arena =
            make_arena(r#"<html><body><div class="product active">Produkt</div></body></html>"#);
        let result = query_select_one(&arena, ".product");
        assert!(result.is_some(), "Borde hitta .product");
    }

    #[test]
    fn test_query_select_tag() {
        let arena = make_arena(r#"<html><body><button>Klick</button></body></html>"#);
        let result = query_select_one(&arena, "button");
        assert!(result.is_some(), "Borde hitta button-element");
    }

    #[test]
    fn test_query_select_all_class() {
        let arena = make_arena(
            r#"<html><body>
            <div class="item">A</div>
            <div class="item">B</div>
            <div class="other">C</div>
        </body></html>"#,
        );
        let results = query_select_all(&arena, ".item");
        assert_eq!(results.len(), 2, "Borde hitta 2 .item-element");
    }

    #[test]
    fn test_query_select_all_tag() {
        let arena = make_arena(
            r#"<html><body>
            <p>Ett</p><p>Två</p><p>Tre</p>
        </body></html>"#,
        );
        let results = query_select_all(&arena, "p");
        assert_eq!(results.len(), 3, "Borde hitta 3 <p>-element");
    }

    #[test]
    fn test_find_by_tag_name() {
        let arena = make_arena(r#"<html><head></head><body><p>X</p></body></html>"#);
        let body = find_by_tag_name(&arena, arena.document, "body");
        assert!(body.is_some(), "Borde hitta <body>");
        let head = find_by_tag_name(&arena, arena.document, "head");
        assert!(head.is_some(), "Borde hitta <head>");
    }

    // === JS med DOM (kräver js-eval feature) ===

    #[test]
    fn test_eval_getElementById() {
        let arena = make_arena(r##"<html><body><div id="test">Hej</div></body></html>"##);
        let result = eval_js_with_dom(
            "var el = document.getElementById('test'); el !== null",
            arena,
        );
        assert!(
            result.error.is_none(),
            "Borde inte ge fel: {:?}",
            result.error
        );
        assert_eq!(
            result.value.as_deref(),
            Some("true"),
            "getElementById borde returnera element (ej null)"
        );
    }

    #[test]
    fn test_eval_getElementById_missing() {
        let arena = make_arena(r#"<html><body><p>X</p></body></html>"#);
        let result = eval_js_with_dom("document.getElementById('nonexistent') === null", arena);
        assert_eq!(
            result.value.as_deref(),
            Some("true"),
            "Saknat element borde ge null"
        );
    }

    #[test]
    fn test_eval_querySelector() {
        let arena = make_arena(r##"<html><body><span id="price">199</span></body></html>"##);
        let result = eval_js_with_dom("document.querySelector('#price') !== null", arena);
        assert_eq!(
            result.value.as_deref(),
            Some("true"),
            "querySelector borde hitta #price"
        );
    }

    #[test]
    fn test_eval_querySelectorAll() {
        let arena = make_arena(
            r#"<html><body>
            <p class="item">A</p><p class="item">B</p>
        </body></html>"#,
        );
        let result = eval_js_with_dom("document.querySelectorAll('.item').length", arena);
        assert_eq!(
            result.value.as_deref(),
            Some("2"),
            "querySelectorAll('.item') borde ge 2"
        );
    }

    #[test]
    fn test_eval_createElement() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let result = eval_js_with_dom("var el = document.createElement('div'); el !== null", arena);
        assert_eq!(
            result.value.as_deref(),
            Some("true"),
            "createElement borde returnera element"
        );
    }

    #[test]
    fn test_eval_window_properties() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let result = eval_js_with_dom("window.innerWidth", arena);
        assert_eq!(
            result.value.as_deref(),
            Some("1024"),
            "window.innerWidth borde vara 1024"
        );
    }

    #[test]
    fn test_eval_console_log() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let result = eval_js_with_dom("console.log('test'); 42", arena);
        assert!(result.error.is_none(), "console.log borde inte krascha");
        assert_eq!(result.value.as_deref(), Some("42"));
    }

    #[test]
    fn test_eval_blocks_fetch() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let result = eval_js_with_dom("fetch('http://evil.com')", arena);
        assert!(result.error.is_some(), "fetch borde blockeras");
        assert!(
            result.error.unwrap().contains("Blocked"),
            "Felet borde nämna 'Blocked'"
        );
    }

    #[test]
    fn test_eval_document_body() {
        let arena = make_arena(r#"<html><body><p>Hej</p></body></html>"#);
        let result = eval_js_with_dom("document.body !== null", arena);
        assert_eq!(
            result.value.as_deref(),
            Some("true"),
            "document.body borde finnas"
        );
    }

    #[test]
    fn test_eval_math_with_dom() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let result = eval_js_with_dom("29.99 * 2", arena);
        assert_eq!(
            result.value.as_deref(),
            Some("59.98"),
            "Matematik borde fungera med DOM-kontext"
        );
    }

    // === Event Loop Integration ===

    #[test]
    fn test_setTimeout_in_dom_context() {
        let arena = make_arena(r#"<html><body><div id="t">A</div></body></html>"#);
        let code = r#"
            var x = 0;
            setTimeout(function() { x = 42; }, 1);
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(
            result.error.is_none(),
            "setTimeout borde inte ge fel: {:?}",
            result.error
        );
        assert!(
            result.event_loop_ticks > 0,
            "Event loop borde ha kört: ticks={}",
            result.event_loop_ticks
        );
        assert!(
            result.timers_fired > 0,
            "Timer borde ha körts: fired={}",
            result.timers_fired
        );
    }

    #[test]
    fn test_promise_in_dom_context() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"
            var resolved = false;
            Promise.resolve(1).then(function(v) { resolved = true; });
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(
            result.error.is_none(),
            "Promise borde inte ge fel: {:?}",
            result.error
        );
        assert!(
            result.event_loop_ticks > 0,
            "Event loop borde ha dränerat microtasks"
        );
    }
}
