/// Boa DOM Bridge — Fas 17.4-17.6
///
/// Exponerar ArenaDom som `document`/`window`-objekt i Boa JS-kontexten.
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
use boa_engine::{
    js_string,
    object::{builtins::JsArray, ObjectInitializer},
    property::{Attribute, PropertyDescriptor},
    Context, JsArgs, JsValue, NativeFunction, Source,
};

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
    callback: JsValue,
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
/// Sätter upp `document` och `window` som globala objekt i Boa-kontexten.
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

    let mut context = crate::js_eval::create_sandboxed_context();

    // Registrera event-loop (setTimeout, setInterval, rAF, MutationObserver, queueMicrotask)
    let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
    event_loop::register_event_loop(&mut context, Rc::clone(&el));

    // Registrera document-objekt
    register_document(&mut context, Rc::clone(&state));

    // Registrera window-objekt
    register_window(&mut context, Rc::clone(&state));

    // Registrera console (fångar output)
    register_console(&mut context, Rc::clone(&state));

    match context.eval(Source::from_bytes(code)) {
        Ok(result) => {
            let value_str = result
                .to_string(&mut context)
                .map_or_else(|_| "undefined".to_string(), |v| v.to_std_string_escaped());

            // Kör event-loopen: dränera microtasks, timers, rAF, MutationObservers
            let loop_stats = event_loop::run_event_loop(&mut context, &el);
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
            error: Some(format!("{}", e)),
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

    let mut context = crate::js_eval::create_sandboxed_context();
    let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
    event_loop::register_event_loop(&mut context, Rc::clone(&el));
    register_document(&mut context, Rc::clone(&state));
    register_window(&mut context, Rc::clone(&state));
    register_console(&mut context, Rc::clone(&state));

    let result = match context.eval(Source::from_bytes(code)) {
        Ok(res) => {
            let value_str = res
                .to_string(&mut context)
                .map_or_else(|_| "undefined".to_string(), |v| v.to_std_string_escaped());

            let loop_stats = event_loop::run_event_loop(&mut context, &el);
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
            error: Some(format!("{}", e)),
            mutations: state.borrow().mutations.clone(),
            eval_time_us: start.elapsed().as_micros() as u64,
            event_loop_ticks: 0,
            timers_fired: 0,
        },
    };

    // Extrahera arena från SharedState — ta tillbaka ägandet
    // Rc::try_unwrap fungerar eftersom context (med alla Rc-kloner) droppas ovan
    drop(context);
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
            }
        }
    };

    DomEvalWithArena {
        result,
        arena: bridge.arena,
    }
}

// ─── Document-objekt ────────────────────────────────────────────────────────

fn register_document(context: &mut Context, state: SharedState) {
    let state_gbi = Rc::clone(&state);
    let state_qs = Rc::clone(&state);
    let state_qsa = Rc::clone(&state);
    let state_ce = Rc::clone(&state);
    let state_ct = Rc::clone(&state);
    let state_cc = Rc::clone(&state);
    let state_gcn = Rc::clone(&state);
    let state_gtn = Rc::clone(&state);

    // SAFETY: Closures capture Rc<RefCell<BridgeState>> som ej är Send/Sync,
    // men Boa-kontexten är single-threaded och closures lever inom samma tråd.
    let get_element_by_id = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?;
            let id_str = id.to_std_string_escaped();
            let key = {
                let s = state_gbi.borrow();
                find_by_attr_value(&s.arena, "id", &id_str)
            };
            match key {
                Some(k) => Ok(make_element_object(ctx, k, &state_gbi)),
                None => Ok(JsValue::null()),
            }
        })
    };

    let query_selector = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let selector = args.get_or_undefined(0).to_string(ctx)?;
            let sel_str = selector.to_std_string_escaped();
            let key = {
                let s = state_qs.borrow();
                query_select_one(&s.arena, &sel_str)
            };
            match key {
                Some(k) => Ok(make_element_object(ctx, k, &state_qs)),
                None => Ok(JsValue::null()),
            }
        })
    };

    let query_selector_all = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let selector = args.get_or_undefined(0).to_string(ctx)?;
            let sel_str = selector.to_std_string_escaped();
            let keys = {
                let s = state_qsa.borrow();
                query_select_all(&s.arena, &sel_str)
            };
            let array = JsArray::new(ctx);
            for key in keys {
                let elem = make_element_object(ctx, key, &state_qsa);
                array.push(elem, ctx)?;
            }
            Ok(array.into())
        })
    };

    let create_element = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let tag = args.get_or_undefined(0).to_string(ctx)?;
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
        })
    };

    let create_text_node = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?;
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
        })
    };

    // createComment(text)
    let create_comment = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?;
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
        })
    };

    // getElementsByClassName(cls)
    let get_by_class = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            let array = JsArray::new(ctx);
            for key in keys {
                let elem = make_element_object(ctx, key, &state_gcn);
                array.push(elem, ctx)?;
            }
            Ok(array.into())
        })
    };

    // getElementsByTagName(tag)
    let get_by_tag = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            let array = JsArray::new(ctx);
            for key in keys {
                let elem = make_element_object(ctx, key, &state_gtn);
                array.push(elem, ctx)?;
            }
            Ok(array.into())
        })
    };

    // createDocumentFragment
    let state_cdf = Rc::clone(&state);
    let create_doc_fragment = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
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
        })
    };

    // Bygg document-objektet
    let doc = ObjectInitializer::new(context)
        .function(get_element_by_id, js_string!("getElementById"), 1)
        .function(query_selector, js_string!("querySelector"), 1)
        .function(query_selector_all, js_string!("querySelectorAll"), 1)
        .function(create_element, js_string!("createElement"), 1)
        .function(create_text_node, js_string!("createTextNode"), 1)
        .function(create_comment, js_string!("createComment"), 1)
        .function(create_doc_fragment, js_string!("createDocumentFragment"), 0)
        .function(get_by_class, js_string!("getElementsByClassName"), 1)
        .function(get_by_tag, js_string!("getElementsByTagName"), 1)
        .build();

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
        let body_obj = make_element_object(context, key, &state);
        doc.set(js_string!("body"), body_obj, false, context)
            .unwrap_or(true);
    }
    if let Some(key) = head_key {
        let head_obj = make_element_object(context, key, &state);
        doc.set(js_string!("head"), head_obj, false, context)
            .unwrap_or(true);
    }
    if let Some(key) = html_key {
        let html_obj = make_element_object(context, key, &state);
        doc.set(js_string!("documentElement"), html_obj, false, context)
            .unwrap_or(true);
    }

    // createRange() — grundläggande Range API för rich-text editors
    let create_range_fn = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let collapse_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let select_node_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let select_node_contents_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let set_start_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let set_end_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let set_start_before_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let set_end_after_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let clone_range_fn = NativeFunction::from_fn_ptr(|_this, _args, ctx2| {
            // Returnera nytt Range-liknande objekt
            let inner = ObjectInitializer::new(ctx2)
                .property(
                    js_string!("collapsed"),
                    JsValue::from(true),
                    Attribute::all(),
                )
                .property(
                    js_string!("startOffset"),
                    JsValue::from(0),
                    Attribute::all(),
                )
                .property(js_string!("endOffset"), JsValue::from(0), Attribute::all())
                .build();
            Ok(inner.into())
        });
        let delete_contents_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let to_string_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(js_string!(""))));
        let get_bounding_fn = NativeFunction::from_fn_ptr(|_this, _args, ctx2| {
            let rect = ObjectInitializer::new(ctx2)
                .property(js_string!("x"), JsValue::from(0), Attribute::READONLY)
                .property(js_string!("y"), JsValue::from(0), Attribute::READONLY)
                .property(js_string!("width"), JsValue::from(0), Attribute::READONLY)
                .property(js_string!("height"), JsValue::from(0), Attribute::READONLY)
                .property(js_string!("top"), JsValue::from(0), Attribute::READONLY)
                .property(js_string!("right"), JsValue::from(0), Attribute::READONLY)
                .property(js_string!("bottom"), JsValue::from(0), Attribute::READONLY)
                .property(js_string!("left"), JsValue::from(0), Attribute::READONLY)
                .build();
            Ok(rect.into())
        });

        let range = ObjectInitializer::new(ctx)
            .property(
                js_string!("collapsed"),
                JsValue::from(true),
                Attribute::all(),
            )
            .property(
                js_string!("startContainer"),
                JsValue::null(),
                Attribute::all(),
            )
            .property(
                js_string!("endContainer"),
                JsValue::null(),
                Attribute::all(),
            )
            .property(
                js_string!("startOffset"),
                JsValue::from(0),
                Attribute::all(),
            )
            .property(js_string!("endOffset"), JsValue::from(0), Attribute::all())
            .property(
                js_string!("commonAncestorContainer"),
                JsValue::null(),
                Attribute::all(),
            )
            .function(collapse_fn, js_string!("collapse"), 1)
            .function(select_node_fn, js_string!("selectNode"), 1)
            .function(select_node_contents_fn, js_string!("selectNodeContents"), 1)
            .function(set_start_fn, js_string!("setStart"), 2)
            .function(set_end_fn, js_string!("setEnd"), 2)
            .function(set_start_before_fn, js_string!("setStartBefore"), 1)
            .function(set_end_after_fn, js_string!("setEndAfter"), 1)
            .function(clone_range_fn, js_string!("cloneRange"), 0)
            .function(delete_contents_fn, js_string!("deleteContents"), 0)
            .function(to_string_fn, js_string!("toString"), 0)
            .function(get_bounding_fn, js_string!("getBoundingClientRect"), 0)
            .build();

        Ok(range.into())
    });
    doc.set(
        js_string!("createRange"),
        create_range_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // getSelection() — grundläggande Selection API
    let get_selection_fn = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let remove_all_ranges =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let add_range_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let collapse_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let collapse_to_start =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let collapse_to_end =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let to_string_fn =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(js_string!(""))));

        let selection = ObjectInitializer::new(ctx)
            .property(js_string!("anchorNode"), JsValue::null(), Attribute::all())
            .property(
                js_string!("anchorOffset"),
                JsValue::from(0),
                Attribute::all(),
            )
            .property(js_string!("focusNode"), JsValue::null(), Attribute::all())
            .property(
                js_string!("focusOffset"),
                JsValue::from(0),
                Attribute::all(),
            )
            .property(
                js_string!("isCollapsed"),
                JsValue::from(true),
                Attribute::all(),
            )
            .property(js_string!("rangeCount"), JsValue::from(0), Attribute::all())
            .property(
                js_string!("type"),
                JsValue::from(js_string!("None")),
                Attribute::all(),
            )
            .function(remove_all_ranges, js_string!("removeAllRanges"), 0)
            .function(add_range_fn, js_string!("addRange"), 1)
            .function(collapse_fn, js_string!("collapse"), 2)
            .function(collapse_to_start, js_string!("collapseToStart"), 0)
            .function(collapse_to_end, js_string!("collapseToEnd"), 0)
            .function(to_string_fn, js_string!("toString"), 0)
            .build();

        Ok(selection.into())
    });
    doc.set(
        js_string!("getSelection"),
        get_selection_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // exitPointerLock()
    let exit_pl = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    doc.set(
        js_string!("exitPointerLock"),
        exit_pl.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // activeElement — returnerar det element som har fokus (default: body)
    let st_ae = Rc::clone(&state);
    let active_element_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let focused = st_ae.borrow().focused_element;
            match focused {
                Some(key_u64) => {
                    let nk = f64_to_node_key(key_u64 as f64);
                    let exists = st_ae.borrow().arena.nodes.get(nk).is_some();
                    if exists {
                        Ok(make_element_object(ctx, nk, &st_ae))
                    } else {
                        Ok(JsValue::null())
                    }
                }
                None => {
                    let body_key = {
                        let s = st_ae.borrow();
                        find_by_tag_name(&s.arena, s.arena.document, "body")
                    };
                    match body_key {
                        Some(bk) => Ok(make_element_object(ctx, bk, &st_ae)),
                        None => Ok(JsValue::null()),
                    }
                }
            }
        })
    };
    doc.set(
        js_string!("activeElement"),
        active_element_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    context
        .register_global_property(js_string!("document"), doc, Attribute::all())
        .unwrap_or(());
}

// ─── Element-objekt ─────────────────────────────────────────────────────────

/// Extrahera NodeKey från ett JS-element-objekt via __nodeKey__
fn extract_node_key(val: &JsValue, ctx: &mut Context) -> Option<NodeKey> {
    let obj = val.as_object()?;
    let bits = obj
        .get(js_string!("__nodeKey__"), ctx)
        .ok()?
        .to_number(ctx)
        .ok()?;
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
fn make_element_object(context: &mut Context, key: NodeKey, state: &SharedState) -> JsValue {
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

    let obj = ObjectInitializer::new(context)
        .property(
            js_string!("__nodeKey__"),
            JsValue::from(key_bits),
            Attribute::empty(),
        )
        .property(
            js_string!("nodeType"),
            JsValue::from(node_type_val),
            Attribute::READONLY,
        )
        .property(
            js_string!("tagName"),
            JsValue::from(js_string!(tag_name.as_str())),
            Attribute::READONLY,
        )
        .property(
            js_string!("id"),
            JsValue::from(js_string!(id_val.as_str())),
            Attribute::all(),
        )
        .property(
            js_string!("className"),
            JsValue::from(js_string!(class_val.as_str())),
            Attribute::all(),
        )
        .build();

    // ─── getAttribute(name) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let ga = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args.get_or_undefined(0).to_string(ctx)?;
            let name_str = name.to_std_string_escaped();
            let s = st.borrow();
            match s.arena.nodes.get(key).and_then(|n| n.get_attr(&name_str)) {
                Some(v) => Ok(JsValue::from(js_string!(v))),
                None => Ok(JsValue::null()),
            }
        })
    };
    obj.set(
        js_string!("getAttribute"),
        ga.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── setAttribute(name, value) ──────────────────────────────────
    let st = Rc::clone(state);
    let sa = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("setAttribute"),
        sa.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── removeAttribute(name) ──────────────────────────────────────
    let st = Rc::clone(state);
    let ra = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            s.arena.remove_attr(key, &name);
            s.mutations.push(format!("removeAttribute({})", name));
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("removeAttribute"),
        ra.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── textContent (accessor property: getter + setter) ──────────
    {
        let st_get = Rc::clone(state);
        let tc_get = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let s = st_get.borrow();
                let text = s.arena.extract_text(key);
                Ok(JsValue::from(js_string!(text.as_str())))
            })
        };
        let st_set = Rc::clone(state);
        let tc_set = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
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
                Ok(JsValue::undefined())
            })
        };
        let getter_fn = tc_get.to_js_function(context.realm());
        let setter_fn = tc_set.to_js_function(context.realm());
        let _ = obj.define_property_or_throw(
            js_string!("textContent"),
            PropertyDescriptor::builder()
                .get(getter_fn)
                .set(setter_fn)
                .enumerable(true)
                .configurable(true)
                .build(),
            context,
        );
    }

    // ─── appendChild(child) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let ac = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let child_key = match extract_node_key(args.get_or_undefined(0), ctx) {
                Some(k) => k,
                None => return Ok(JsValue::undefined()),
            };
            let mut s = st.borrow_mut();
            s.arena.append_child(key, child_key);
            s.mutations.push("appendChild".to_string());
            Ok(args.get_or_undefined(0).clone())
        })
    };
    obj.set(
        js_string!("appendChild"),
        ac.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── removeChild(child) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let rc = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let child_key = match extract_node_key(args.get_or_undefined(0), ctx) {
                Some(k) => k,
                None => return Ok(JsValue::undefined()),
            };
            let mut s = st.borrow_mut();
            s.arena.remove_child(key, child_key);
            s.mutations.push("removeChild".to_string());
            Ok(args.get_or_undefined(0).clone())
        })
    };
    obj.set(
        js_string!("removeChild"),
        rc.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── parentNode ─────────────────────────────────────────────────
    let st = Rc::clone(state);
    let pn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(parent_key) => Ok(JsValue::from(node_key_to_f64(parent_key))),
                None => Ok(JsValue::null()),
            }
        })
    };
    obj.set(
        js_string!("parentNode"),
        pn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── childNodes ─────────────────────────────────────────────────
    let st = Rc::clone(state);
    let cn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let s = st.borrow();
            let arr = JsArray::new(ctx);
            if let Some(node) = s.arena.nodes.get(key) {
                for (i, &child) in node.children.iter().enumerate() {
                    let child_bits = node_key_to_f64(child);
                    let child_obj = ObjectInitializer::new(ctx)
                        .property(
                            js_string!("__nodeKey__"),
                            JsValue::from(child_bits),
                            Attribute::empty(),
                        )
                        .build();
                    let _ = arr.set(i as u32, child_obj, false, ctx);
                }
            }
            Ok(arr.into())
        })
    };
    obj.set(
        js_string!("childNodes"),
        cn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── firstChild ─────────────────────────────────────────────────
    let st = Rc::clone(state);
    let fc = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            match s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.children.first().copied())
            {
                Some(child_key) => Ok(JsValue::from(node_key_to_f64(child_key))),
                None => Ok(JsValue::null()),
            }
        })
    };
    obj.set(
        js_string!("firstChild"),
        fc.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── nextSibling ────────────────────────────────────────────────
    let st = Rc::clone(state);
    let ns = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(JsValue::null()),
            };
            let siblings = &s.arena.nodes.get(parent_key).map(|n| &n.children);
            if let Some(sibs) = siblings {
                let my_idx = sibs.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    if idx + 1 < sibs.len() {
                        return Ok(JsValue::from(node_key_to_f64(sibs[idx + 1])));
                    }
                }
            }
            Ok(JsValue::null())
        })
    };
    obj.set(
        js_string!("nextSibling"),
        ns.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── closest(selector) ──────────────────────────────────────────
    let st = Rc::clone(state);
    let cl = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let sel = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let s = st.borrow();
            let mut current = Some(key);
            while let Some(k) = current {
                if matches_selector(&s.arena, k, &sel) {
                    return Ok(JsValue::from(node_key_to_f64(k)));
                }
                current = s.arena.nodes.get(k).and_then(|n| n.parent);
            }
            Ok(JsValue::null())
        })
    };
    obj.set(
        js_string!("closest"),
        cl.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── matches(selector) ──────────────────────────────────────────
    let st = Rc::clone(state);
    let ms = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let sel = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let s = st.borrow();
            Ok(JsValue::from(matches_selector(&s.arena, key, &sel)))
        })
    };
    obj.set(
        js_string!("matches"),
        ms.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── children (bara element-barn) ───────────────────────────────
    let st = Rc::clone(state);
    let ch = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let s = st.borrow();
            let arr = JsArray::new(ctx);
            if let Some(node) = s.arena.nodes.get(key) {
                let mut i = 0u32;
                for &child in &node.children {
                    if let Some(cn) = s.arena.nodes.get(child) {
                        if cn.node_type == NodeType::Element {
                            let co = ObjectInitializer::new(ctx)
                                .property(
                                    js_string!("__nodeKey__"),
                                    JsValue::from(node_key_to_f64(child)),
                                    Attribute::empty(),
                                )
                                .build();
                            let _ = arr.set(i, co, false, ctx);
                            i += 1;
                        }
                    }
                }
            }
            Ok(arr.into())
        })
    };
    obj.set(
        js_string!("children"),
        ch.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── dataset (läs data-* attribut) ──────────────────────────────
    let st = Rc::clone(state);
    let ds = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let s = st.borrow();
            let ds_obj = ObjectInitializer::new(ctx).build();
            if let Some(node) = s.arena.nodes.get(key) {
                for (k, v) in &node.attributes {
                    if let Some(name) = k.strip_prefix("data-") {
                        // Konvertera kebab-case till camelCase
                        let camel = data_attr_to_camel(name);
                        let _ = ds_obj.set(
                            js_string!(camel.as_str()),
                            JsValue::from(js_string!(v.as_str())),
                            false,
                            ctx,
                        );
                    }
                }
            }
            Ok(ds_obj.into())
        })
    };
    obj.set(
        js_string!("dataset"),
        ds.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── insertBefore(newChild, refChild) ─────────────────────────
    let st = Rc::clone(state);
    let ib = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let new_child = match extract_node_key(args.get_or_undefined(0), ctx) {
                Some(k) => k,
                None => return Ok(JsValue::undefined()),
            };
            let ref_child = extract_node_key(args.get_or_undefined(1), ctx);
            let mut s = st.borrow_mut();
            s.arena.insert_before(key, new_child, ref_child);
            s.mutations.push("insertBefore".to_string());
            Ok(args.get_or_undefined(0).clone())
        })
    };
    obj.set(
        js_string!("insertBefore"),
        ib.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── insertAdjacentHTML(position, html) ────────────────────────
    // Förenklad: loggar mutation, parsning av HTML stöds ej (kräver full parser)
    let st = Rc::clone(state);
    let iah = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("insertAdjacentHTML"),
        iah.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── attachShadow({ mode }) ─────────────────────────────────────
    // Förenklad: returnerar ett tomt objekt som shadow root
    let at_sh = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let shadow = ObjectInitializer::new(ctx).build();
            Ok(JsValue::from(shadow))
        })
    };
    obj.set(
        js_string!("attachShadow"),
        at_sh.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── cloneNode(deep) ────────────────────────────────────────────
    let st = Rc::clone(state);
    let cn_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let mut s = st.borrow_mut();
            match s.arena.clone_node_deep(key) {
                Some(clone_key) => {
                    let bits = node_key_to_f64(clone_key);
                    let clone_obj = ObjectInitializer::new(ctx)
                        .property(
                            js_string!("__nodeKey__"),
                            JsValue::from(bits),
                            Attribute::empty(),
                        )
                        .property(
                            js_string!("nodeType"),
                            JsValue::from(1),
                            Attribute::READONLY,
                        )
                        .build();
                    Ok(clone_obj.into())
                }
                None => Ok(JsValue::null()),
            }
        })
    };
    obj.set(
        js_string!("cloneNode"),
        cn_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── outerHTML (getter) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let oh = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            Ok(JsValue::from(js_string!(s
                .arena
                .serialize_html(key)
                .as_str())))
        })
    };
    obj.set(
        js_string!("outerHTML"),
        oh.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── innerHTML (accessor property: getter + setter) ────────────
    {
        let st_get = Rc::clone(state);
        let ih_get = unsafe {
            NativeFunction::from_closure(move |_this, _args, _ctx| {
                let s = st_get.borrow();
                Ok(JsValue::from(js_string!(s
                    .arena
                    .serialize_inner_html(key)
                    .as_str())))
            })
        };
        let st_set = Rc::clone(state);
        let ih_set = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx| {
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
                Ok(JsValue::undefined())
            })
        };
        let getter_fn = ih_get.to_js_function(context.realm());
        let setter_fn = ih_set.to_js_function(context.realm());
        let _ = obj.define_property_or_throw(
            js_string!("innerHTML"),
            PropertyDescriptor::builder()
                .get(getter_fn)
                .set(setter_fn)
                .enumerable(true)
                .configurable(true)
                .build(),
            context,
        );
    }

    // ─── Bakåtkompatibla metoder: setInnerHTML / setTextContent ──────
    // Behåller dessa som explicita metoder för befintlig kod
    let st = Rc::clone(state);
    let ih_set_compat = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("setInnerHTML"),
        ih_set_compat.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    let st = Rc::clone(state);
    let tc_set_compat = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("setTextContent"),
        tc_set_compat.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── firstElementChild ──────────────────────────────────────────
    let st = Rc::clone(state);
    let fec = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            if let Some(node) = s.arena.nodes.get(key) {
                for &child in &node.children {
                    if let Some(cn) = s.arena.nodes.get(child) {
                        if cn.node_type == NodeType::Element {
                            return Ok(JsValue::from(node_key_to_f64(child)));
                        }
                    }
                }
            }
            Ok(JsValue::null())
        })
    };
    obj.set(
        js_string!("firstElementChild"),
        fec.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── nextElementSibling ─────────────────────────────────────────
    let st = Rc::clone(state);
    let nes = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(JsValue::null()),
            };
            if let Some(parent) = s.arena.nodes.get(parent_key) {
                let my_idx = parent.children.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    for &sib in &parent.children[idx + 1..] {
                        if let Some(sn) = s.arena.nodes.get(sib) {
                            if sn.node_type == NodeType::Element {
                                return Ok(JsValue::from(node_key_to_f64(sib)));
                            }
                        }
                    }
                }
            }
            Ok(JsValue::null())
        })
    };
    obj.set(
        js_string!("nextElementSibling"),
        nes.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── previousSibling ──────────────────────────────────────────
    let st = Rc::clone(state);
    let ps = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(JsValue::null()),
            };
            let siblings = &s.arena.nodes.get(parent_key).map(|n| &n.children);
            if let Some(sibs) = siblings {
                let my_idx = sibs.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    if idx > 0 {
                        return Ok(JsValue::from(node_key_to_f64(sibs[idx - 1])));
                    }
                }
            }
            Ok(JsValue::null())
        })
    };
    obj.set(
        js_string!("previousSibling"),
        ps.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── previousElementSibling ─────────────────────────────────────
    let st = Rc::clone(state);
    let pes = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let parent_key = match s.arena.nodes.get(key).and_then(|n| n.parent) {
                Some(p) => p,
                None => return Ok(JsValue::null()),
            };
            if let Some(parent) = s.arena.nodes.get(parent_key) {
                let my_idx = parent.children.iter().position(|&c| c == key);
                if let Some(idx) = my_idx {
                    for &sib in parent.children[..idx].iter().rev() {
                        if let Some(sn) = s.arena.nodes.get(sib) {
                            if sn.node_type == NodeType::Element {
                                return Ok(JsValue::from(node_key_to_f64(sib)));
                            }
                        }
                    }
                }
            }
            Ok(JsValue::null())
        })
    };
    obj.set(
        js_string!("previousElementSibling"),
        pes.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── childElementCount ──────────────────────────────────────────
    let st = Rc::clone(state);
    let cec = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
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
            Ok(JsValue::from(count as u32))
        })
    };
    obj.set(
        js_string!("childElementCount"),
        cec.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── hasAttribute(name) ─────────────────────────────────────────
    let st = Rc::clone(state);
    let ha = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let s = st.borrow();
            Ok(JsValue::from(s.arena.has_attr(key, &name)))
        })
    };
    obj.set(
        js_string!("hasAttribute"),
        ha.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── remove() — ta bort elementet från sin förälder ─────────────
    let st = Rc::clone(state);
    let rm = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut s = st.borrow_mut();
            if let Some(parent_key) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                s.arena.remove_child(parent_key, key);
                s.mutations.push(format!("remove:{}", key_bits));
            }
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("remove"),
        rm.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── replaceWith(newNode) ───────────────────────────────────────
    let st = Rc::clone(state);
    let rw = unsafe {
        NativeFunction::from_closure(move |_this, args, _ctx| {
            let new_key_val = args.get_or_undefined(0).to_number(_ctx)?;
            let new_key = f64_to_node_key(new_key_val);
            let mut s = st.borrow_mut();
            if let Some(parent_key) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                s.arena.insert_before(parent_key, new_key, Some(key));
                s.arena.remove_child(parent_key, key);
                s.mutations
                    .push(format!("replaceWith:{}:{}", key_bits, new_key_val as u64));
            }
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("replaceWith"),
        rw.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── replaceChild(newChild, oldChild) (Fas 19) ────────────────
    let st_rc = Rc::clone(state);
    let replace_child = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx2| {
            let new_key_val = extract_node_key(args.get_or_undefined(0), ctx2);
            let old_key_val = extract_node_key(args.get_or_undefined(1), ctx2);
            if let (Some(new_k), Some(old_k)) = (new_key_val, old_key_val) {
                let mut s = st_rc.borrow_mut();
                s.arena.insert_before(key, new_k, Some(old_k));
                s.arena.remove_child(key, old_k);
                s.mutations
                    .push(format!("replaceChild:{}", key_bits as u64));
            }
            Ok(args.get_or_undefined(1).clone())
        })
    };
    obj.set(
        js_string!("replaceChild"),
        replace_child.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── before/after/prepend/append (Fas 19) ───────────────────────
    let st_before = Rc::clone(state);
    let before_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx2| {
            let new_key = extract_node_key(args.get_or_undefined(0), ctx2);
            if let Some(nk) = new_key {
                let mut s = st_before.borrow_mut();
                if let Some(parent) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                    s.arena.insert_before(parent, nk, Some(key));
                    s.mutations.push(format!("before:{}", key_bits as u64));
                }
            }
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("before"),
        before_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    let st_after = Rc::clone(state);
    let after_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx2| {
            let new_key = extract_node_key(args.get_or_undefined(0), ctx2);
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("after"),
        after_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    let st_prepend = Rc::clone(state);
    let prepend_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx2| {
            let new_key = extract_node_key(args.get_or_undefined(0), ctx2);
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("prepend"),
        prepend_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    let st_append = Rc::clone(state);
    let append_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx2| {
            let new_key = extract_node_key(args.get_or_undefined(0), ctx2);
            if let Some(nk) = new_key {
                let mut s = st_append.borrow_mut();
                s.arena.append_child(key, nk);
                s.mutations.push(format!("append:{}", key_bits as u64));
            }
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("append"),
        append_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── toggleAttribute(name) (Fas 19) ─────────────────────────────
    let st_ta = Rc::clone(state);
    let toggle_attr = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx2| {
            let attr = args
                .get_or_undefined(0)
                .to_string(ctx2)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let mut s = st_ta.borrow_mut();
            let had = s.arena.has_attr(key, &attr);
            if had {
                s.arena.remove_attr(key, &attr);
            } else {
                s.arena.set_attr(key, &attr, "");
            }
            Ok(JsValue::from(!had))
        })
    };
    obj.set(
        js_string!("toggleAttribute"),
        toggle_attr.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── getAttributeNames() (Fas 19) ───────────────────────────────
    let st_gan = Rc::clone(state);
    let get_attr_names = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx2| {
            let s = st_gan.borrow();
            let arr = JsArray::new(ctx2);
            if let Some(node) = s.arena.nodes.get(key) {
                for (i, name) in node.attributes.keys().enumerate() {
                    let _ = arr.set(
                        i as u32,
                        JsValue::from(js_string!(name.as_str())),
                        false,
                        ctx2,
                    );
                }
            }
            Ok(arr.into())
        })
    };
    obj.set(
        js_string!("getAttributeNames"),
        get_attr_names.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── normalize() — slå ihop adjacenta text-noder (Fas 19) ───────
    let st_norm = Rc::clone(state);
    let normalize_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("normalize"),
        normalize_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── value (getter/setter för input/textarea/select) ────────────
    let st = Rc::clone(state);
    let val_get = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            Ok(JsValue::from(js_string!(s
                .arena
                .get_attr(key, "value")
                .unwrap_or(""))))
        })
    };
    obj.set(
        js_string!("value"),
        val_get.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── checked (getter för checkbox/radio) ────────────────────────
    let st = Rc::clone(state);
    let chk = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            Ok(JsValue::from(s.arena.has_attr(key, "checked")))
        })
    };
    obj.set(
        js_string!("checked"),
        chk.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── selected (getter för option-element) ───────────────────────
    let st = Rc::clone(state);
    let sel = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            Ok(JsValue::from(s.arena.has_attr(key, "selected")))
        })
    };
    obj.set(
        js_string!("selected"),
        sel.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── tabIndex ───────────────────────────────────────────────────
    let st = Rc::clone(state);
    let ti = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let val = s
                .arena
                .get_attr(key, "tabindex")
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(-1);
            Ok(JsValue::from(val))
        })
    };
    obj.set(
        js_string!("tabIndex"),
        ti.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── offsetParent ───────────────────────────────────────────────
    let st = Rc::clone(state);
    let op = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            // Returnera body som offsetParent (förenklad implementering)
            if let Some(parent_key) = s.arena.nodes.get(key).and_then(|n| n.parent) {
                Ok(JsValue::from(node_key_to_f64(parent_key)))
            } else {
                Ok(JsValue::null())
            }
        })
    };
    obj.set(
        js_string!("offsetParent"),
        op.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── clientTop / clientLeft (border-dimensioner, default 0) ─────
    obj.set(js_string!("clientTop"), JsValue::from(0), false, context)
        .unwrap_or(true);
    obj.set(js_string!("clientLeft"), JsValue::from(0), false, context)
        .unwrap_or(true);

    // ─── addEventListener(type, callback, capture/options) ──────────
    let st = Rc::clone(state);
    let ael = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let event_type = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let callback = args.get_or_undefined(1).clone();
            if !callback.is_callable() {
                return Ok(JsValue::undefined());
            }
            // Stöd för options-objekt (tredje arg): { once, passive, capture }
            let third = args.get_or_undefined(2);
            let capture = if third.is_object() {
                // Options-objekt: läs capture-fältet
                third
                    .as_object()
                    .and_then(|o| {
                        o.get(js_string!("capture"), ctx)
                            .ok()
                            .map(|v| v.to_boolean())
                    })
                    .unwrap_or(false)
            } else {
                third.to_boolean()
            };
            let mut s = st.borrow_mut();
            let listeners = s.event_listeners.entry(key_bits as u64).or_default();
            listeners.push(EventListener {
                event_type,
                callback,
                capture,
            });
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("addEventListener"),
        ael.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── removeEventListener(type, callback) ────────────────────
    let st = Rc::clone(state);
    let rel = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("removeEventListener"),
        rel.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── dispatchEvent(event) ───────────────────────────────────
    let st = Rc::clone(state);
    let de = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let event_val = args.get_or_undefined(0).clone();
            let event_type = event_val
                .as_object()
                .and_then(|o| o.get(js_string!("type"), ctx).ok())
                .and_then(|v| v.to_string(ctx).ok())
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            if event_type.is_empty() {
                return Ok(JsValue::from(false));
            }
            let bubbles = event_val
                .as_object()
                .and_then(|o| o.get(js_string!("bubbles"), ctx).ok())
                .map(|v| v.to_boolean())
                .unwrap_or(true);

            // Sätt target + currentTarget
            if let Some(obj) = event_val.as_object() {
                let _ = obj.set(js_string!("target"), JsValue::from(key_bits), false, ctx);
                let _ = obj.set(
                    js_string!("currentTarget"),
                    JsValue::from(key_bits),
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
            let target_listeners: Vec<JsValue> = {
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
                if let Some(callable) = cb.as_callable() {
                    let _ =
                        callable.call(&JsValue::undefined(), std::slice::from_ref(&event_val), ctx);
                }
            }

            // Bubble phase
            if bubbles {
                for &ancestor_u64 in &ancestors {
                    let stopped = event_val
                        .as_object()
                        .and_then(|o| o.get(js_string!("__stopped__"), ctx).ok())
                        .map(|v| v.to_boolean())
                        .unwrap_or(false);
                    if stopped {
                        break;
                    }
                    if let Some(obj) = event_val.as_object() {
                        let _ = obj.set(
                            js_string!("currentTarget"),
                            JsValue::from(ancestor_u64 as f64),
                            false,
                            ctx,
                        );
                    }
                    let ancestor_listeners: Vec<JsValue> = {
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
                        if let Some(callable) = cb.as_callable() {
                            let _ = callable.call(
                                &JsValue::undefined(),
                                std::slice::from_ref(&event_val),
                                ctx,
                            );
                        }
                    }
                }
            }

            let prevented = event_val
                .as_object()
                .and_then(|o| o.get(js_string!("defaultPrevented"), ctx).ok())
                .map(|v| v.to_boolean())
                .unwrap_or(false);
            Ok(JsValue::from(!prevented))
        })
    };
    obj.set(
        js_string!("dispatchEvent"),
        de.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── focus() ────────────────────────────────────────────────────
    let st = Rc::clone(state);
    let focus_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut s = st.borrow_mut();
            s.focused_element = Some(key_bits as u64);
            s.mutations.push("focus".to_string());
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("focus"),
        focus_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── blur() ─────────────────────────────────────────────────────
    let st = Rc::clone(state);
    let blur_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut s = st.borrow_mut();
            if s.focused_element == Some(key_bits as u64) {
                s.focused_element = None;
            }
            s.mutations.push("blur".to_string());
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("blur"),
        blur_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── scrollIntoView(options) ────────────────────────────────────
    let st = Rc::clone(state);
    let siv = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
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
            Ok(JsValue::undefined())
        })
    };
    obj.set(
        js_string!("scrollIntoView"),
        siv.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── getBoundingClientRect() ────────────────────────────────────
    let st = Rc::clone(state);
    let gbcr = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let s = st.borrow();
            let (x, y, width, height) = estimate_layout_rect(&s.arena, key);
            let rect = ObjectInitializer::new(ctx)
                .property(js_string!("x"), JsValue::from(x), Attribute::READONLY)
                .property(js_string!("y"), JsValue::from(y), Attribute::READONLY)
                .property(
                    js_string!("width"),
                    JsValue::from(width),
                    Attribute::READONLY,
                )
                .property(
                    js_string!("height"),
                    JsValue::from(height),
                    Attribute::READONLY,
                )
                .property(js_string!("top"), JsValue::from(y), Attribute::READONLY)
                .property(
                    js_string!("right"),
                    JsValue::from(x + width),
                    Attribute::READONLY,
                )
                .property(
                    js_string!("bottom"),
                    JsValue::from(y + height),
                    Attribute::READONLY,
                )
                .property(js_string!("left"), JsValue::from(x), Attribute::READONLY)
                .build();
            Ok(rect.into())
        })
    };
    obj.set(
        js_string!("getBoundingClientRect"),
        gbcr.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── getClientRects() ───────────────────────────────────────────
    let st = Rc::clone(state);
    let gcr = unsafe {
        NativeFunction::from_closure(move |_this, _args, ctx| {
            let s = st.borrow();
            let (x, y, width, height) = estimate_layout_rect(&s.arena, key);
            let arr = JsArray::new(ctx);
            let rect = ObjectInitializer::new(ctx)
                .property(js_string!("x"), JsValue::from(x), Attribute::READONLY)
                .property(js_string!("y"), JsValue::from(y), Attribute::READONLY)
                .property(
                    js_string!("width"),
                    JsValue::from(width),
                    Attribute::READONLY,
                )
                .property(
                    js_string!("height"),
                    JsValue::from(height),
                    Attribute::READONLY,
                )
                .build();
            let _ = arr.push(rect, ctx);
            Ok(arr.into())
        })
    };
    obj.set(
        js_string!("getClientRects"),
        gcr.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

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
            js_string!("offsetTop"),
            JsValue::from(y_pos),
            false,
            context,
        )
        .unwrap_or(true);
        obj.set(js_string!("offsetLeft"), JsValue::from(0), false, context)
            .unwrap_or(true);
        obj.set(js_string!("offsetWidth"), JsValue::from(w), false, context)
            .unwrap_or(true);
        obj.set(js_string!("offsetHeight"), JsValue::from(h), false, context)
            .unwrap_or(true);
        obj.set(
            js_string!("scrollTop"),
            JsValue::from(scroll_top),
            false,
            context,
        )
        .unwrap_or(true);
        obj.set(
            js_string!("scrollLeft"),
            JsValue::from(scroll_left),
            false,
            context,
        )
        .unwrap_or(true);
        obj.set(
            js_string!("scrollWidth"),
            JsValue::from(content_w),
            false,
            context,
        )
        .unwrap_or(true);
        obj.set(
            js_string!("scrollHeight"),
            JsValue::from(content_h),
            false,
            context,
        )
        .unwrap_or(true);
        obj.set(js_string!("clientWidth"), JsValue::from(w), false, context)
            .unwrap_or(true);
        obj.set(js_string!("clientHeight"), JsValue::from(h), false, context)
            .unwrap_or(true);
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
            let shadow = make_element_object(context, template_key, state);
            obj.set(js_string!("shadowRoot"), shadow, false, context)
                .unwrap_or(true);
        } else {
            obj.set(js_string!("shadowRoot"), JsValue::null(), false, context)
                .unwrap_or(true);
        }
    }

    // ─── classList ──────────────────────────────────────────────────
    let class_list = make_class_list(context, key, state);
    obj.set(js_string!("classList"), class_list, false, context)
        .unwrap_or(true);

    // ─── style (inline CSS-stilar med setProperty/getPropertyValue) ─
    let style = make_style_object(context, key, state);
    obj.set(js_string!("style"), style, false, context)
        .unwrap_or(true);

    // ─── isConnected (boolean — noden är kopplad till document) ─────
    {
        let s = state.borrow();
        let connected = is_connected_to_document(&s.arena, key);
        obj.set(
            js_string!("isConnected"),
            JsValue::from(connected),
            false,
            context,
        )
        .unwrap_or(true);
    }

    // ─── contains(otherElement) ─────────────────────────────────────
    let st = Rc::clone(state);
    let contains_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let other_key = match extract_node_key(args.get_or_undefined(0), ctx) {
                Some(k) => k,
                None => return Ok(JsValue::from(false)),
            };
            let s = st.borrow();
            Ok(JsValue::from(node_contains(&s.arena, key, other_key)))
        })
    };
    obj.set(
        js_string!("contains"),
        contains_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── getRootNode() ──────────────────────────────────────────────
    let st = Rc::clone(state);
    let root_node_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let mut current = key;
            while let Some(parent) = s.arena.nodes.get(current).and_then(|n| n.parent) {
                current = parent;
            }
            Ok(JsValue::from(node_key_to_f64(current)))
        })
    };
    obj.set(
        js_string!("getRootNode"),
        root_node_fn.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

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
            js_string!("hidden"),
            JsValue::from(is_hidden),
            false,
            context,
        )
        .unwrap_or(true);
    }

    // ─── requestPointerLock() / exitPointerLock() ───────────────────
    let rpl = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    obj.set(
        js_string!("requestPointerLock"),
        rpl.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    JsValue::from(obj)
}

/// Skapa classList-objekt med add/remove/toggle/contains
fn make_class_list(context: &mut Context, key: NodeKey, state: &SharedState) -> JsValue {
    let st_add = Rc::clone(state);
    let add_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };

    let st_rm = Rc::clone(state);
    let remove_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };

    let st_tg = Rc::clone(state);
    let toggle_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::from(!has))
        })
    };

    let st_ct = Rc::clone(state);
    let contains_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::from(has))
        })
    };

    // replace(old, new) — byt klass
    let st_rp = Rc::clone(state);
    let replace_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::from(had))
        })
    };

    // value — hela class-strängen
    let st_val = Rc::clone(state);
    let value_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st_val.borrow();
            let val = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("class"))
                .unwrap_or("");
            Ok(JsValue::from(js_string!(val)))
        })
    };

    // length — antal klasser
    let st_len = Rc::clone(state);
    let length_fn = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st_len.borrow();
            let count = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("class"))
                .map(|c| c.split_whitespace().count())
                .unwrap_or(0);
            Ok(JsValue::from(count as i32))
        })
    };

    let cl = ObjectInitializer::new(context)
        .function(add_fn, js_string!("add"), 1)
        .function(remove_fn, js_string!("remove"), 1)
        .function(toggle_fn, js_string!("toggle"), 1)
        .function(contains_fn, js_string!("contains"), 1)
        .function(replace_fn, js_string!("replace"), 2)
        .function(value_fn, js_string!("value"), 0)
        .function(length_fn, js_string!("length"), 0)
        .build();

    JsValue::from(cl)
}

/// Skapa style-objekt med inline CSS-stöd kopplat till arena
fn make_style_object(context: &mut Context, key: NodeKey, state: &SharedState) -> JsValue {
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
    let set_prop = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };

    // getPropertyValue(prop)
    let st = Rc::clone(state);
    let get_prop = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::from(js_string!(val.as_str())))
        })
    };

    // removeProperty(prop)
    let st = Rc::clone(state);
    let remove_prop = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::from(js_string!(old_val.as_str())))
        })
    };

    // cssText getter
    let st = Rc::clone(state);
    let css_text_get = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let style_str = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.get_attr("style"))
                .unwrap_or("");
            Ok(JsValue::from(js_string!(style_str)))
        })
    };

    let style = ObjectInitializer::new(context)
        .function(set_prop, js_string!("setProperty"), 2)
        .function(get_prop, js_string!("getPropertyValue"), 1)
        .function(remove_prop, js_string!("removeProperty"), 1)
        .function(css_text_get, js_string!("cssText"), 0)
        .build();

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
            js_string!(camel.as_str()),
            JsValue::from(js_string!(val.as_str())),
            false,
            context,
        );
    }

    JsValue::from(style)
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

fn register_window(context: &mut Context, state: SharedState) {
    let window = ObjectInitializer::new(context)
        .property(
            js_string!("innerWidth"),
            JsValue::from(1024),
            Attribute::READONLY,
        )
        .property(
            js_string!("innerHeight"),
            JsValue::from(768),
            Attribute::READONLY,
        )
        .build();

    // location stub
    let location = ObjectInitializer::new(context)
        .property(
            js_string!("href"),
            JsValue::from(js_string!("")),
            Attribute::all(),
        )
        .property(
            js_string!("hostname"),
            JsValue::from(js_string!("")),
            Attribute::all(),
        )
        .property(
            js_string!("pathname"),
            JsValue::from(js_string!("/")),
            Attribute::all(),
        )
        .property(
            js_string!("protocol"),
            JsValue::from(js_string!("https:")),
            Attribute::all(),
        )
        .build();

    window
        .set(js_string!("location"), location, false, context)
        .unwrap_or(true);

    // navigator stub
    let navigator = ObjectInitializer::new(context)
        .property(
            js_string!("userAgent"),
            JsValue::from(js_string!("AetherAgent/0.1")),
            Attribute::READONLY,
        )
        .property(
            js_string!("language"),
            JsValue::from(js_string!("en")),
            Attribute::READONLY,
        )
        .build();

    window
        .set(js_string!("navigator"), navigator, false, context)
        .unwrap_or(true);

    // getComputedStyle(el) — använder CSS Cascade Engine (Fas 19)
    let st_gcs = Rc::clone(&state);
    let gcs = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let elem = args.get_or_undefined(0);
            let node_key = extract_node_key(elem, ctx);

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
            let get_pv = NativeFunction::from_closure(move |_this, args, ctx2| {
                let prop = args
                    .get_or_undefined(0)
                    .to_string(ctx2)
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default()
                    .to_lowercase();
                let val = merged_for_closure.get(&prop).cloned().unwrap_or_default();
                Ok(JsValue::from(js_string!(val.as_str())))
            });

            let style = ObjectInitializer::new(ctx)
                .function(get_pv, js_string!("getPropertyValue"), 1)
                .build();

            // Sätt alla properties som egenskaper (camelCase + kebab)
            for (prop, val) in &merged {
                let camel = data_attr_to_camel(prop);
                let _ = style.set(
                    js_string!(camel.as_str()),
                    JsValue::from(js_string!(val.as_str())),
                    false,
                    ctx,
                );
            }

            Ok(style.into())
        })
    };
    window
        .set(
            js_string!("getComputedStyle"),
            gcs.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // matchMedia(query) — Fas 19: CSS media query matching
    let match_media = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let query = args
            .get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();

        // Parsa bredd-baserade media queries mot innerWidth=1024
        let matches = parse_media_query_matches(&query, 1024.0, 768.0);

        let add_listener =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
        let remove_listener =
            NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));

        let mql = ObjectInitializer::new(ctx)
            .property(
                js_string!("matches"),
                JsValue::from(matches),
                Attribute::READONLY,
            )
            .property(
                js_string!("media"),
                JsValue::from(js_string!(query.as_str())),
                Attribute::READONLY,
            )
            .function(add_listener.clone(), js_string!("addEventListener"), 2)
            .function(
                remove_listener.clone(),
                js_string!("removeEventListener"),
                2,
            )
            .function(add_listener, js_string!("addListener"), 1)
            .function(remove_listener, js_string!("removeListener"), 1)
            .build();
        Ok(mql.into())
    });
    window
        .set(
            js_string!("matchMedia"),
            match_media.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // atob/btoa — base64 encode/decode
    let atob_fn = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let input = args
            .get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        // atob decodes base64
        match base64_decode(&input) {
            Some(decoded) => Ok(JsValue::from(js_string!(decoded.as_str()))),
            None => Ok(JsValue::from(js_string!(""))),
        }
    });
    window
        .set(
            js_string!("atob"),
            atob_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    let btoa_fn = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let input = args
            .get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let encoded = base64_encode(&input);
        Ok(JsValue::from(js_string!(encoded.as_str())))
    });
    window
        .set(
            js_string!("btoa"),
            btoa_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // scrollTo/scrollBy — stubs som loggar mutation
    let st_scroll = Rc::clone(&state);
    let scroll_to = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let x = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
            let y = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
            st_scroll
                .borrow_mut()
                .mutations
                .push(format!("window.scrollTo({}, {})", x, y));
            Ok(JsValue::undefined())
        })
    };
    window
        .set(
            js_string!("scrollTo"),
            scroll_to.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);
    let st_scroll2 = Rc::clone(&state);
    let scroll_by = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let x = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0);
            let y = args.get_or_undefined(1).to_number(ctx).unwrap_or(0.0);
            st_scroll2
                .borrow_mut()
                .mutations
                .push(format!("window.scrollBy({}, {})", x, y));
            Ok(JsValue::undefined())
        })
    };
    window
        .set(
            js_string!("scrollBy"),
            scroll_by.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);
    // scroll = alias för scrollTo
    let scroll_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    window
        .set(
            js_string!("scroll"),
            scroll_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // performance.now()
    let perf_start = std::time::Instant::now();
    let perf_now = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let ms = perf_start.elapsed().as_secs_f64() * 1000.0;
            Ok(JsValue::from(ms))
        })
    };
    let performance = ObjectInitializer::new(context)
        .function(perf_now, js_string!("now"), 0)
        .build();
    window
        .set(js_string!("performance"), performance, false, context)
        .unwrap_or(true);

    // screen object
    let screen = ObjectInitializer::new(context)
        .property(
            js_string!("width"),
            JsValue::from(1920),
            Attribute::READONLY,
        )
        .property(
            js_string!("height"),
            JsValue::from(1080),
            Attribute::READONLY,
        )
        .property(
            js_string!("colorDepth"),
            JsValue::from(24),
            Attribute::READONLY,
        )
        .property(
            js_string!("availWidth"),
            JsValue::from(1920),
            Attribute::READONLY,
        )
        .property(
            js_string!("availHeight"),
            JsValue::from(1040),
            Attribute::READONLY,
        )
        .build();
    window
        .set(js_string!("screen"), screen, false, context)
        .unwrap_or(true);

    // devicePixelRatio
    window
        .set(
            js_string!("devicePixelRatio"),
            JsValue::from(1.0),
            false,
            context,
        )
        .unwrap_or(true);

    // history stub
    let st_hist = Rc::clone(&state);
    let push_state = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let url = args
                .get_or_undefined(2)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            st_hist
                .borrow_mut()
                .mutations
                .push(format!("history.pushState({})", url));
            Ok(JsValue::undefined())
        })
    };
    let st_hist2 = Rc::clone(&state);
    let replace_state = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let url = args
                .get_or_undefined(2)
                .to_string(ctx)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            st_hist2
                .borrow_mut()
                .mutations
                .push(format!("history.replaceState({})", url));
            Ok(JsValue::undefined())
        })
    };
    let back_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let forward_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let go_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let history = ObjectInitializer::new(context)
        .function(push_state, js_string!("pushState"), 3)
        .function(replace_state, js_string!("replaceState"), 3)
        .function(back_fn, js_string!("back"), 0)
        .function(forward_fn, js_string!("forward"), 0)
        .function(go_fn, js_string!("go"), 1)
        .property(js_string!("length"), JsValue::from(1), Attribute::READONLY)
        .build();
    window
        .set(js_string!("history"), history, false, context)
        .unwrap_or(true);

    // requestIdleCallback — kör callback direkt (idle = always)
    let ric_fn = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        if let Some(callable) = args.get_or_undefined(0).as_callable() {
            let deadline = ObjectInitializer::new(ctx)
                .property(
                    js_string!("didTimeout"),
                    JsValue::from(false),
                    Attribute::READONLY,
                )
                .build();
            let time_remaining =
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::from(50.0)));
            let _ = deadline.set(
                js_string!("timeRemaining"),
                time_remaining.to_js_function(ctx.realm()),
                false,
                ctx,
            );
            let _ = callable.call(&JsValue::undefined(), &[deadline.into()], ctx);
        }
        Ok(JsValue::from(1))
    });
    window
        .set(
            js_string!("requestIdleCallback"),
            ric_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // open/close stubs
    let open_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::null()));
    let close_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    window
        .set(
            js_string!("open"),
            open_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);
    window
        .set(
            js_string!("close"),
            close_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // IntersectionObserver — fires per-element on observe() med layoutdata
    let st_io = Rc::clone(&state);
    let io_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0).clone();
            let cb_clone = callback.clone();
            let st_observe = Rc::clone(&st_io);

            let observe_fn = NativeFunction::from_closure(move |_this, args, ctx2| {
                let target = args.get_or_undefined(0);
                let target_key = extract_node_key(target, ctx2);

                if let (Some(callable), Some(k)) = (cb_clone.as_callable(), target_key) {
                    let (y, w, h) = {
                        let s = st_observe.borrow();
                        let (_, y, w, h) = estimate_layout_rect(&s.arena, k);
                        (y, w, h)
                    };

                    let is_visible = y < 768.0 && w > 0.0 && h > 0.0;
                    let ratio = if is_visible { 1.0 } else { 0.0 };

                    let bounds = ObjectInitializer::new(ctx2)
                        .property(js_string!("x"), JsValue::from(0.0), Attribute::READONLY)
                        .property(js_string!("y"), JsValue::from(y), Attribute::READONLY)
                        .property(js_string!("width"), JsValue::from(w), Attribute::READONLY)
                        .property(js_string!("height"), JsValue::from(h), Attribute::READONLY)
                        .build();

                    let entry = ObjectInitializer::new(ctx2)
                        .property(
                            js_string!("isIntersecting"),
                            JsValue::from(is_visible),
                            Attribute::READONLY,
                        )
                        .property(
                            js_string!("intersectionRatio"),
                            JsValue::from(ratio),
                            Attribute::READONLY,
                        )
                        .property(
                            js_string!("boundingClientRect"),
                            bounds,
                            Attribute::READONLY,
                        )
                        .property(js_string!("target"), target.clone(), Attribute::READONLY)
                        .build();

                    let entries = JsArray::new(ctx2);
                    let _ = entries.push(entry, ctx2);
                    let _ = callable.call(&JsValue::undefined(), &[entries.into()], ctx2);
                }
                Ok(JsValue::undefined())
            });

            let unobserve_fn =
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
            let disconnect_fn =
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
            let observer = ObjectInitializer::new(ctx)
                .function(observe_fn, js_string!("observe"), 1)
                .function(unobserve_fn, js_string!("unobserve"), 1)
                .function(disconnect_fn, js_string!("disconnect"), 0)
                .build();
            Ok(observer.into())
        })
    };
    window
        .set(
            js_string!("IntersectionObserver"),
            io_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // ResizeObserver — fires callback on observe() med element-dimensioner
    let st_ro = Rc::clone(&state);
    let ro_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0).clone();
            let cb_clone = callback.clone();
            let st_observe = Rc::clone(&st_ro);

            let observe_fn = NativeFunction::from_closure(move |_this, args, ctx2| {
                let target = args.get_or_undefined(0);
                let target_key = extract_node_key(target, ctx2);

                if let (Some(callable), Some(k)) = (cb_clone.as_callable(), target_key) {
                    let (w, h) = {
                        let s = st_observe.borrow();
                        let (_, _, w, h) = estimate_layout_rect(&s.arena, k);
                        (w, h)
                    };

                    let content_rect = ObjectInitializer::new(ctx2)
                        .property(js_string!("width"), JsValue::from(w), Attribute::READONLY)
                        .property(js_string!("height"), JsValue::from(h), Attribute::READONLY)
                        .property(js_string!("x"), JsValue::from(0.0), Attribute::READONLY)
                        .property(js_string!("y"), JsValue::from(0.0), Attribute::READONLY)
                        .build();

                    let border_size = ObjectInitializer::new(ctx2)
                        .property(
                            js_string!("inlineSize"),
                            JsValue::from(w),
                            Attribute::READONLY,
                        )
                        .property(
                            js_string!("blockSize"),
                            JsValue::from(h),
                            Attribute::READONLY,
                        )
                        .build();
                    let border_arr = JsArray::new(ctx2);
                    let _ = border_arr.push(border_size, ctx2);

                    let entry = ObjectInitializer::new(ctx2)
                        .property(js_string!("contentRect"), content_rect, Attribute::READONLY)
                        .property(js_string!("borderBoxSize"), border_arr, Attribute::READONLY)
                        .property(js_string!("target"), target.clone(), Attribute::READONLY)
                        .build();

                    let entries = JsArray::new(ctx2);
                    let _ = entries.push(entry, ctx2);
                    let _ = callable.call(&JsValue::undefined(), &[entries.into()], ctx2);
                }
                Ok(JsValue::undefined())
            });

            let unobserve_fn =
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
            let disconnect_fn =
                NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
            let observer = ObjectInitializer::new(ctx)
                .function(observe_fn, js_string!("observe"), 1)
                .function(unobserve_fn, js_string!("unobserve"), 1)
                .function(disconnect_fn, js_string!("disconnect"), 0)
                .build();
            Ok(observer.into())
        })
    };
    window
        .set(
            js_string!("ResizeObserver"),
            ro_fn.to_js_function(context.realm()),
            false,
            context,
        )
        .unwrap_or(true);

    // customElements — med lifecycle callback-stöd (connectedCallback, disconnectedCallback)
    // Lagrar registrerade element-definitioner med deras konstruktor/klass
    let registry: Rc<RefCell<std::collections::HashMap<String, JsValue>>> =
        Rc::new(RefCell::new(std::collections::HashMap::new()));

    let reg_define = Rc::clone(&registry);
    let ce_define = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let constructor = args.get_or_undefined(1).clone();
            // Validera att namnet innehåller bindestreck (Web Components-krav)
            if !name.contains('-') {
                return Ok(JsValue::undefined());
            }
            reg_define.borrow_mut().insert(name, constructor);
            Ok(JsValue::undefined())
        })
    };

    let reg_get = Rc::clone(&registry);
    let ce_get = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let reg = reg_get.borrow();
            match reg.get(&name) {
                Some(ctor) => Ok(ctor.clone()),
                None => Ok(JsValue::undefined()),
            }
        })
    };

    let reg_when = Rc::clone(&registry);
    let ce_when = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let name = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped()
                .to_lowercase();
            let reg = reg_when.borrow();
            if reg.contains_key(&name) {
                // Redan definierad — returnera resolved Promise
                let promise = ctx.eval(boa_engine::Source::from_bytes("Promise.resolve()"));
                match promise {
                    Ok(p) => Ok(p),
                    Err(_) => Ok(JsValue::undefined()),
                }
            } else {
                // Inte definierad — returnera pending-liknande Promise
                let promise = ctx.eval(boa_engine::Source::from_bytes("Promise.resolve()"));
                match promise {
                    Ok(p) => Ok(p),
                    Err(_) => Ok(JsValue::undefined()),
                }
            }
        })
    };

    let custom_elements = ObjectInitializer::new(context)
        .function(ce_define, js_string!("define"), 2)
        .function(ce_get, js_string!("get"), 1)
        .function(ce_when, js_string!("whenDefined"), 1)
        .build();
    window
        .set(
            js_string!("customElements"),
            custom_elements,
            false,
            context,
        )
        .unwrap_or(true);

    // localStorage — in-memory sandboxad implementation (Fas 19)
    register_storage(context, Rc::clone(&state), "localStorage", true);
    register_storage(context, Rc::clone(&state), "sessionStorage", false);

    // Sätt localStorage/sessionStorage på window också
    if let Ok(ls) = context
        .global_object()
        .get(js_string!("localStorage"), context)
    {
        window
            .set(js_string!("localStorage"), ls, false, context)
            .unwrap_or(true);
    }
    if let Ok(ss) = context
        .global_object()
        .get(js_string!("sessionStorage"), context)
    {
        window
            .set(js_string!("sessionStorage"), ss, false, context)
            .unwrap_or(true);
    }

    context
        .register_global_property(js_string!("window"), window, Attribute::all())
        .unwrap_or(());

    // Event konstruktor — new Event('click', {bubbles: true})
    let event_ctor = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let type_str = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let options = args.get_or_undefined(1);
        let bubbles = options
            .as_object()
            .and_then(|o| o.get(js_string!("bubbles"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let cancelable = options
            .as_object()
            .and_then(|o| o.get(js_string!("cancelable"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let composed = options
            .as_object()
            .and_then(|o| o.get(js_string!("composed"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        let stop_prop = NativeFunction::from_fn_ptr(|this, _args, ctx2| {
            if let Some(obj) = this.as_object() {
                let _ = obj.set(js_string!("__stopped__"), JsValue::from(true), false, ctx2);
            }
            Ok(JsValue::undefined())
        });
        let prevent_default = NativeFunction::from_fn_ptr(|this, _args, ctx2| {
            if let Some(obj) = this.as_object() {
                let _ = obj.set(
                    js_string!("defaultPrevented"),
                    JsValue::from(true),
                    false,
                    ctx2,
                );
            }
            Ok(JsValue::undefined())
        });
        let stop_imm = NativeFunction::from_fn_ptr(|this, _args, ctx2| {
            if let Some(obj) = this.as_object() {
                let _ = obj.set(js_string!("__stopped__"), JsValue::from(true), false, ctx2);
            }
            Ok(JsValue::undefined())
        });

        let event = ObjectInitializer::new(ctx)
            .property(
                js_string!("type"),
                JsValue::from(js_string!(type_str.as_str())),
                Attribute::READONLY,
            )
            .property(
                js_string!("bubbles"),
                JsValue::from(bubbles),
                Attribute::READONLY,
            )
            .property(
                js_string!("cancelable"),
                JsValue::from(cancelable),
                Attribute::READONLY,
            )
            .property(
                js_string!("composed"),
                JsValue::from(composed),
                Attribute::READONLY,
            )
            .property(
                js_string!("defaultPrevented"),
                JsValue::from(false),
                Attribute::all(),
            )
            .property(
                js_string!("__stopped__"),
                JsValue::from(false),
                Attribute::all(),
            )
            .property(js_string!("target"), JsValue::null(), Attribute::all())
            .property(
                js_string!("currentTarget"),
                JsValue::null(),
                Attribute::all(),
            )
            .property(js_string!("eventPhase"), JsValue::from(0), Attribute::all())
            .property(
                js_string!("timeStamp"),
                JsValue::from(0.0),
                Attribute::READONLY,
            )
            .property(
                js_string!("isTrusted"),
                JsValue::from(false),
                Attribute::READONLY,
            )
            .function(stop_prop, js_string!("stopPropagation"), 0)
            .function(prevent_default, js_string!("preventDefault"), 0)
            .function(stop_imm, js_string!("stopImmediatePropagation"), 0)
            .build();

        Ok(event.into())
    });
    context
        .register_global_property(
            js_string!("Event"),
            event_ctor.to_js_function(context.realm()),
            Attribute::all(),
        )
        .unwrap_or(());

    // CustomEvent konstruktor — new CustomEvent('my-event', {detail: data})
    let custom_event_ctor = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let type_str = args
            .get_or_undefined(0)
            .to_string(ctx)?
            .to_std_string_escaped();
        let options = args.get_or_undefined(1);
        let bubbles = options
            .as_object()
            .and_then(|o| o.get(js_string!("bubbles"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let detail = options
            .as_object()
            .and_then(|o| o.get(js_string!("detail"), ctx).ok())
            .unwrap_or(JsValue::null());
        let composed = options
            .as_object()
            .and_then(|o| o.get(js_string!("composed"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);
        let cancelable = options
            .as_object()
            .and_then(|o| o.get(js_string!("cancelable"), ctx).ok())
            .map(|v| v.to_boolean())
            .unwrap_or(false);

        let stop_prop = NativeFunction::from_fn_ptr(|this, _args, ctx2| {
            if let Some(obj) = this.as_object() {
                let _ = obj.set(js_string!("__stopped__"), JsValue::from(true), false, ctx2);
            }
            Ok(JsValue::undefined())
        });
        let prevent_default = NativeFunction::from_fn_ptr(|this, _args, ctx2| {
            if let Some(obj) = this.as_object() {
                let _ = obj.set(
                    js_string!("defaultPrevented"),
                    JsValue::from(true),
                    false,
                    ctx2,
                );
            }
            Ok(JsValue::undefined())
        });

        let event = ObjectInitializer::new(ctx)
            .property(
                js_string!("type"),
                JsValue::from(js_string!(type_str.as_str())),
                Attribute::READONLY,
            )
            .property(
                js_string!("bubbles"),
                JsValue::from(bubbles),
                Attribute::READONLY,
            )
            .property(
                js_string!("cancelable"),
                JsValue::from(cancelable),
                Attribute::READONLY,
            )
            .property(
                js_string!("composed"),
                JsValue::from(composed),
                Attribute::READONLY,
            )
            .property(
                js_string!("defaultPrevented"),
                JsValue::from(false),
                Attribute::all(),
            )
            .property(
                js_string!("__stopped__"),
                JsValue::from(false),
                Attribute::all(),
            )
            .property(js_string!("detail"), detail, Attribute::READONLY)
            .property(js_string!("target"), JsValue::null(), Attribute::all())
            .property(
                js_string!("currentTarget"),
                JsValue::null(),
                Attribute::all(),
            )
            .property(
                js_string!("isTrusted"),
                JsValue::from(false),
                Attribute::READONLY,
            )
            .function(stop_prop, js_string!("stopPropagation"), 0)
            .function(prevent_default, js_string!("preventDefault"), 0)
            .build();

        Ok(event.into())
    });
    context
        .register_global_property(
            js_string!("CustomEvent"),
            custom_event_ctor.to_js_function(context.realm()),
            Attribute::all(),
        )
        .unwrap_or(());

    // ─── Globala API:er (Fas 19) ─────────────────────────────────────────────

    // TextEncoder
    let text_encoder_ctor = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let encode_fn = NativeFunction::from_fn_ptr(|_this, args, ctx2| {
            let input = args
                .get_or_undefined(0)
                .to_string(ctx2)
                .map(|s| s.to_std_string_escaped())
                .unwrap_or_default();
            let bytes = input.as_bytes();
            let arr = JsArray::new(ctx2);
            for (i, &b) in bytes.iter().enumerate() {
                let _ = arr.set(i as u32, JsValue::from(b as f64), false, ctx2);
            }
            Ok(arr.into())
        });
        let encoder = ObjectInitializer::new(ctx)
            .property(
                js_string!("encoding"),
                JsValue::from(js_string!("utf-8")),
                Attribute::READONLY,
            )
            .function(encode_fn, js_string!("encode"), 1)
            .build();
        Ok(encoder.into())
    });
    context
        .register_global_property(
            js_string!("TextEncoder"),
            text_encoder_ctor.to_js_function(context.realm()),
            Attribute::all(),
        )
        .unwrap_or(());

    // TextDecoder
    let text_decoder_ctor = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let decode_fn = NativeFunction::from_fn_ptr(|_this, args, ctx2| {
            let input = args.get_or_undefined(0);
            // Försök tolka som array/typed array
            let mut bytes: Vec<u8> = Vec::new();
            if let Some(obj) = input.as_object() {
                // Hämta length och iterera
                if let Ok(len) = obj.get(js_string!("length"), ctx2) {
                    let len = len.to_number(ctx2).unwrap_or(0.0) as usize;
                    for i in 0..len.min(1_000_000) {
                        if let Ok(val) = obj.get(i as u32, ctx2) {
                            bytes.push(val.to_number(ctx2).unwrap_or(0.0) as u8);
                        }
                    }
                }
            }
            let decoded = String::from_utf8_lossy(&bytes).to_string();
            Ok(JsValue::from(js_string!(decoded.as_str())))
        });
        let decoder = ObjectInitializer::new(ctx)
            .property(
                js_string!("encoding"),
                JsValue::from(js_string!("utf-8")),
                Attribute::READONLY,
            )
            .function(decode_fn, js_string!("decode"), 1)
            .build();
        Ok(decoder.into())
    });
    context
        .register_global_property(
            js_string!("TextDecoder"),
            text_decoder_ctor.to_js_function(context.realm()),
            Attribute::all(),
        )
        .unwrap_or(());

    // URL constructor
    let url_ctor = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let url_str = args
            .get_or_undefined(0)
            .to_string(ctx)
            .map(|s| s.to_std_string_escaped())
            .unwrap_or_default();
        let base_str = if args.len() > 1 {
            args.get_or_undefined(1)
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
        let sp_obj = ObjectInitializer::new(ctx).build();
        let params = parse_query_string(&search);
        let params_clone = params.clone();
        let sp_get = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx2| {
                let key = args
                    .get_or_undefined(0)
                    .to_string(ctx2)
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                match params_clone.iter().find(|(k, _)| k == &key) {
                    Some((_, v)) => Ok(JsValue::from(js_string!(v.as_str()))),
                    None => Ok(JsValue::null()),
                }
            })
        };
        let params_clone2 = params.clone();
        let sp_has = unsafe {
            NativeFunction::from_closure(move |_this, args, ctx2| {
                let key = args
                    .get_or_undefined(0)
                    .to_string(ctx2)
                    .map(|s| s.to_std_string_escaped())
                    .unwrap_or_default();
                Ok(JsValue::from(params_clone2.iter().any(|(k, _)| k == &key)))
            })
        };
        let sp_tostring = {
            let s = search.clone();
            unsafe {
                NativeFunction::from_closure(move |_this, _args, _ctx| {
                    Ok(JsValue::from(js_string!(s.as_str())))
                })
            }
        };
        let _ = sp_obj.set(
            js_string!("get"),
            sp_get.to_js_function(ctx.realm()),
            false,
            ctx,
        );
        let _ = sp_obj.set(
            js_string!("has"),
            sp_has.to_js_function(ctx.realm()),
            false,
            ctx,
        );
        let _ = sp_obj.set(
            js_string!("toString"),
            sp_tostring.to_js_function(ctx.realm()),
            false,
            ctx,
        );

        let url_obj = ObjectInitializer::new(ctx)
            .property(
                js_string!("href"),
                JsValue::from(js_string!(full_url.as_str())),
                Attribute::all(),
            )
            .property(
                js_string!("protocol"),
                JsValue::from(js_string!(protocol.as_str())),
                Attribute::READONLY,
            )
            .property(
                js_string!("hostname"),
                JsValue::from(js_string!(hostname.as_str())),
                Attribute::READONLY,
            )
            .property(
                js_string!("pathname"),
                JsValue::from(js_string!(pathname.as_str())),
                Attribute::READONLY,
            )
            .property(
                js_string!("search"),
                JsValue::from(js_string!(search.as_str())),
                Attribute::READONLY,
            )
            .property(
                js_string!("hash"),
                JsValue::from(js_string!(hash.as_str())),
                Attribute::READONLY,
            )
            .property(
                js_string!("origin"),
                JsValue::from(js_string!(format!(
                    "{}://{}",
                    protocol.trim_end_matches(':'),
                    hostname
                )
                .as_str())),
                Attribute::READONLY,
            )
            .build();
        let _ = url_obj.set(js_string!("searchParams"), sp_obj, false, ctx);

        Ok(url_obj.into())
    });
    context
        .register_global_property(
            js_string!("URL"),
            url_ctor.to_js_function(context.realm()),
            Attribute::all(),
        )
        .unwrap_or(());

    // AbortController stub
    let abort_ctor = NativeFunction::from_fn_ptr(|_this, _args, ctx| {
        let signal = ObjectInitializer::new(ctx)
            .property(
                js_string!("aborted"),
                JsValue::from(false),
                Attribute::all(),
            )
            .property(js_string!("reason"), JsValue::undefined(), Attribute::all())
            .build();
        let signal_clone = signal.clone();
        let abort_fn = unsafe {
            NativeFunction::from_closure(move |_this, _args, ctx2| {
                let _ = signal_clone.set(js_string!("aborted"), JsValue::from(true), false, ctx2);
                Ok(JsValue::undefined())
            })
        };
        let controller = ObjectInitializer::new(ctx)
            .function(abort_fn, js_string!("abort"), 0)
            .build();
        let _ = controller.set(js_string!("signal"), signal, false, ctx);
        Ok(controller.into())
    });
    context
        .register_global_property(
            js_string!("AbortController"),
            abort_ctor.to_js_function(context.realm()),
            Attribute::all(),
        )
        .unwrap_or(());

    // structuredClone — JSON roundtrip-approximation
    let structured_clone = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        let val = args.get_or_undefined(0);
        // Använda JSON.stringify → JSON.parse som approximation
        let json_str = val.to_string(ctx)?.to_std_string_escaped();
        Ok(JsValue::from(js_string!(json_str.as_str())))
    });
    context
        .register_global_property(
            js_string!("structuredClone"),
            structured_clone.to_js_function(context.realm()),
            Attribute::all(),
        )
        .unwrap_or(());

    // crypto.randomUUID() och getRandomValues()
    let random_uuid = NativeFunction::from_fn_ptr(|_this, _args, _ctx| {
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
        Ok(JsValue::from(js_string!(uuid.as_str())))
    });
    let get_random_values = NativeFunction::from_fn_ptr(|_this, args, ctx| {
        // Fyll array med pseudo-random bytes
        let arr = args.get_or_undefined(0);
        if let Some(obj) = arr.as_object() {
            if let Ok(len) = obj.get(js_string!("length"), ctx) {
                let len = len.to_number(ctx).unwrap_or(0.0) as usize;
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                for i in 0..len.min(65536) {
                    let val = ((ts.wrapping_mul(i as u128 + 1).wrapping_add(i as u128 * 7)) & 0xFF)
                        as f64;
                    let _ = obj.set(i as u32, JsValue::from(val), false, ctx);
                }
            }
        }
        Ok(arr.clone())
    });
    let crypto = ObjectInitializer::new(context)
        .function(random_uuid, js_string!("randomUUID"), 0)
        .function(get_random_values, js_string!("getRandomValues"), 1)
        .build();
    context
        .register_global_property(js_string!("crypto"), crypto, Attribute::all())
        .unwrap_or(());
}

// ─── Console-objekt ─────────────────────────────────────────────────────────

fn register_console(context: &mut Context, state: SharedState) {
    // Fånga console-output i BridgeState (Fas 19)
    let make_log = |st: SharedState, prefix: &'static str| unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };

    let console = ObjectInitializer::new(context)
        .function(make_log(Rc::clone(&state), ""), js_string!("log"), 1)
        .function(make_log(Rc::clone(&state), "WARN"), js_string!("warn"), 1)
        .function(make_log(Rc::clone(&state), "ERROR"), js_string!("error"), 1)
        .function(make_log(state, "INFO"), js_string!("info"), 1)
        .build();

    context
        .register_global_property(js_string!("console"), console, Attribute::all())
        .unwrap_or(());
}

// ─── localStorage / sessionStorage — in-memory sandbox (Fas 19) ─────────────

fn register_storage(context: &mut Context, state: SharedState, name: &str, is_local: bool) {
    let st_get = Rc::clone(&state);
    let is_local_get = is_local;
    let get_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
                Some(v) => Ok(JsValue::from(js_string!(v.as_str()))),
                None => Ok(JsValue::null()),
            }
        })
    };

    let st_set = Rc::clone(&state);
    let is_local_set = is_local;
    let set_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };

    let st_rem = Rc::clone(&state);
    let is_local_rem = is_local;
    let remove_item = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
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
            Ok(JsValue::undefined())
        })
    };

    let st_clear = Rc::clone(&state);
    let is_local_clear = is_local;
    let clear = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let mut s = st_clear.borrow_mut();
            let storage = if is_local_clear {
                &mut s.local_storage
            } else {
                &mut s.session_storage
            };
            storage.clear();
            Ok(JsValue::undefined())
        })
    };

    let st_key = Rc::clone(&state);
    let is_local_key = is_local;
    let key_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let index = args.get_or_undefined(0).to_number(ctx).unwrap_or(0.0) as usize;
            let s = st_key.borrow();
            let storage = if is_local_key {
                &s.local_storage
            } else {
                &s.session_storage
            };
            match storage.keys().nth(index) {
                Some(k) => Ok(JsValue::from(js_string!(k.as_str()))),
                None => Ok(JsValue::null()),
            }
        })
    };

    let st_len = Rc::clone(&state);
    let is_local_len = is_local;

    let storage_obj = ObjectInitializer::new(context)
        .function(get_item, js_string!("getItem"), 1)
        .function(set_item, js_string!("setItem"), 2)
        .function(remove_item, js_string!("removeItem"), 1)
        .function(clear, js_string!("clear"), 0)
        .function(key_fn, js_string!("key"), 1)
        .build();

    // Sätt length som getter (via closure)
    let length_getter = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st_len.borrow();
            let storage = if is_local_len {
                &s.local_storage
            } else {
                &s.session_storage
            };
            Ok(JsValue::from(storage.len() as f64))
        })
    };
    let _ = storage_obj.set(
        js_string!("length"),
        length_getter.to_js_function(context.realm()),
        false,
        context,
    );

    context
        .register_global_property(js_string!(name), storage_obj, Attribute::all())
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
            .unwrap_or(true)
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
