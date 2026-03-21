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
    property::Attribute,
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
        "localstorage",
        "sessionstorage",
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
    }));

    let mut context = crate::js_eval::create_sandboxed_context();

    // Registrera event-loop (setTimeout, setInterval, rAF, MutationObserver, queueMicrotask)
    let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
    event_loop::register_event_loop(&mut context, Rc::clone(&el));

    // Registrera document-objekt
    register_document(&mut context, Rc::clone(&state));

    // Registrera window-objekt
    register_window(&mut context, Rc::clone(&state));

    // Registrera console.log (no-op, fångar inte output)
    register_console(&mut context);

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

    // ─── textContent (getter) ───────────────────────────────────────
    let st = Rc::clone(state);
    let tc_get = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            let text = s.arena.extract_text(key);
            Ok(JsValue::from(js_string!(text.as_str())))
        })
    };
    obj.set(
        js_string!("textContent"),
        tc_get.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

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

    // ─── innerHTML (getter/setter) ──────────────────────────────────
    let st = Rc::clone(state);
    let ih = unsafe {
        NativeFunction::from_closure(move |_this, _args, _ctx| {
            let s = st.borrow();
            Ok(JsValue::from(js_string!(s
                .arena
                .serialize_inner_html(key)
                .as_str())))
        })
    };
    obj.set(
        js_string!("innerHTML"),
        ih.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // ─── textContent setter (via setTextContent) ────────────────────
    let st = Rc::clone(state);
    let tc_set = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args
                .get_or_undefined(0)
                .to_string(ctx)?
                .to_std_string_escaped();
            let mut s = st.borrow_mut();
            // Rensa barn
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
    obj.set(
        js_string!("setTextContent"),
        tc_set.to_js_function(context.realm()),
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

    // ─── addEventListener(type, callback, capture) ────────────────
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
            let capture = args.get_or_undefined(2).to_boolean();
            let mut s = st.borrow_mut();
            let listeners = s
                .event_listeners
                .entry(key_bits as u64)
                .or_insert_with(Vec::new);
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

            // Sätt target
            if let Some(obj) = event_val.as_object() {
                let _ = obj.set(js_string!("target"), JsValue::from(key_bits), false, ctx);
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
                    let _ = callable.call(&JsValue::undefined(), &[event_val.clone()], ctx);
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
                            let _ = callable.call(&JsValue::undefined(), &[event_val.clone()], ctx);
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

    // getComputedStyle(el) — mergar inline styles med tag-defaults
    let st_gcs = Rc::clone(&state);
    let gcs = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let elem = args.get_or_undefined(0);
            let node_key = extract_node_key(elem, ctx);

            let (inline_styles, tag_name) = match node_key {
                Some(k) => {
                    let s = st_gcs.borrow();
                    let style_str = s
                        .arena
                        .nodes
                        .get(k)
                        .and_then(|n| n.get_attr("style"))
                        .unwrap_or("");
                    let tag = s.arena.tag_name(k).unwrap_or("div").to_string();
                    (parse_inline_styles(style_str), tag)
                }
                None => (std::collections::HashMap::new(), "div".to_string()),
            };

            // Merge: inline overridar tag-defaults
            let mut merged = get_tag_style_defaults(&tag_name);
            for (k, v) in &inline_styles {
                merged.insert(k.clone(), v.clone());
            }

            let merged_for_closure = merged.clone();
            let get_pv = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx2| {
                    let prop = args
                        .get_or_undefined(0)
                        .to_string(ctx2)
                        .map(|s| s.to_std_string_escaped())
                        .unwrap_or_default()
                        .to_lowercase();
                    let val = merged_for_closure.get(&prop).cloned().unwrap_or_default();
                    Ok(JsValue::from(js_string!(val.as_str())))
                })
            };

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

    // IntersectionObserver — fires per-element on observe() med layoutdata
    let st_io = Rc::clone(&state);
    let io_fn = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let callback = args.get_or_undefined(0).clone();
            let cb_clone = callback.clone();
            let st_observe = Rc::clone(&st_io);

            let observe_fn = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx2| {
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
                })
            };

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

            let observe_fn = unsafe {
                NativeFunction::from_closure(move |_this, args, ctx2| {
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
                })
            };

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

    // customElements — stubbad (Web Components)
    let ce_define = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let ce_get = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
    let ce_when = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));
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
}

// ─── Console-objekt ─────────────────────────────────────────────────────────

fn register_console(context: &mut Context) {
    let log_fn = NativeFunction::from_fn_ptr(|_this, _args, _ctx| Ok(JsValue::undefined()));

    let console = ObjectInitializer::new(context)
        .function(log_fn.clone(), js_string!("log"), 1)
        .function(log_fn.clone(), js_string!("warn"), 1)
        .function(log_fn.clone(), js_string!("error"), 1)
        .function(log_fn, js_string!("info"), 1)
        .build();

    context
        .register_global_property(js_string!("console"), console, Attribute::all())
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
/// Stöder: #id, .class, tag, tag.class, [attr], [attr="val"],
/// :first-child, och kombinationer. Komma-separerade selektorer.
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

    // Descendant/child-kombinator
    if selector.contains(' ') || selector.contains('>') {
        return matches_combinator_selector(arena, key, selector);
    }

    matches_single_selector(arena, key, selector)
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

    // :first-child pseudo
    if selector == ":first-child" {
        return is_first_child(arena, key);
    }

    // Parsea selektor-delar: tag, #id, .class, [attr], [attr="val"], :pseudo
    let mut remaining = selector;
    let mut required_tag: Option<&str> = None;
    let mut required_id: Option<&str> = None;
    let mut required_classes: Vec<&str> = Vec::new();
    let mut required_attrs: Vec<(&str, Option<&str>)> = Vec::new();
    let mut require_first_child = false;

    // Extrahera tagg (om den börjar med bokstav)
    if remaining.starts_with(|c: char| c.is_ascii_alphabetic()) {
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
                let attr_name = &attr_spec[..eq_pos];
                let attr_val = attr_spec[eq_pos + 1..].trim_matches('"').trim_matches('\'');
                required_attrs.push((attr_name, Some(attr_val)));
            } else {
                required_attrs.push((attr_spec, None));
            }
            remaining = &rest[bracket_end + 1..];
        } else if let Some(rest) = remaining.strip_prefix(":first-child") {
            require_first_child = true;
            remaining = rest;
        } else {
            break;
        }
    }

    // Verifiera alla krav
    if let Some(tag) = required_tag {
        if node.tag.as_deref() != Some(tag) {
            return false;
        }
    }
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
    for (attr, val) in &required_attrs {
        match val {
            Some(v) => {
                if node.get_attr(attr) != Some(v) {
                    return false;
                }
            }
            None => {
                if !node.has_attr(attr) {
                    return false;
                }
            }
        }
    }
    if require_first_child && !is_first_child(arena, key) {
        return false;
    }

    true
}

/// Matcha selektor med kombinatorer (> och mellanslag)
fn matches_combinator_selector(arena: &ArenaDom, key: NodeKey, selector: &str) -> bool {
    // Splitta vid > (child) eller mellanslag (descendant)
    // Enkel implementation: matcha sista delen mot noden, resten mot föräldrar
    let parts: Vec<&str> = selector.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }

    // Sista delen matchar mot noden
    let last = parts[parts.len() - 1];
    if last == ">" {
        return false; // Felaktig selektor
    }
    if !matches_single_selector(arena, key, last) {
        return false;
    }

    if parts.len() == 1 {
        return true;
    }

    // Kolla föräldrar
    let is_child_combinator = parts.len() >= 2 && parts[parts.len() - 2] == ">";
    let ancestor_sel = if is_child_combinator {
        // "div > span" — bara direkt förälder
        if parts.len() < 3 {
            return false;
        }
        parts[..parts.len() - 2].join(" ")
    } else {
        parts[..parts.len() - 1].join(" ")
    };

    if is_child_combinator {
        // Direkt förälder måste matcha
        if let Some(parent) = arena.nodes.get(key).and_then(|n| n.parent) {
            return matches_selector(arena, parent, &ancestor_sel);
        }
        false
    } else {
        // Valfri förfader måste matcha
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

/// Kolla om nod är första barnet
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
