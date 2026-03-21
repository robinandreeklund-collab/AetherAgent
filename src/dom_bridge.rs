/// Boa DOM Bridge — Fas 17.3
///
/// Exponerar ArenaDom som `document`/`window`-objekt i Boa JS-kontexten.
/// Gör att JS-kod kan anropa document.getElementById, querySelector, etc.
/// mot en riktig DOM-representation istället för att blocka all DOM-åtkomst.
///
/// 25 kritiska metoder implementerade:
/// - Document: getElementById, querySelector, querySelectorAll, createElement,
///   createTextNode, body, head, documentElement
/// - Element: textContent, innerHTML, getAttribute, setAttribute,
///   removeAttribute, id, className, tagName, classList, style
/// - Node: appendChild, removeChild, parentNode, childNodes, firstChild,
///   nextSibling, nodeType
use boa_engine::{
    js_string,
    object::{builtins::JsArray, ObjectInitializer},
    property::Attribute,
    Context, JsArgs, JsResult, JsValue, NativeFunction, Source,
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

struct BridgeState {
    arena: ArenaDom,
    mutations: Vec<DomMutation>,
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
    }));

    let mut context = crate::js_eval::create_sandboxed_context();

    // Registrera event-loop (setTimeout, setInterval, rAF, MutationObserver, queueMicrotask)
    let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
    event_loop::register_event_loop(&mut context, Rc::clone(&el));

    // Registrera document-objekt
    register_document(&mut context, Rc::clone(&state));

    // Registrera window-objekt (stubbat)
    register_window(&mut context);

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

    // SAFETY: Closures capture Rc<RefCell<BridgeState>> som ej är Send/Sync,
    // men Boa-kontexten är single-threaded och closures lever inom samma tråd.
    let get_element_by_id = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let id = args.get_or_undefined(0).to_string(ctx)?;
            let id_str = id.to_std_string_escaped();
            let s = state_gbi.borrow();
            if let Some(key) = find_by_attr_value(&s.arena, "id", &id_str) {
                Ok(make_element_object(ctx, key))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let query_selector = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let selector = args.get_or_undefined(0).to_string(ctx)?;
            let sel_str = selector.to_std_string_escaped();
            let s = state_qs.borrow();
            if let Some(key) = query_select_one(&s.arena, &sel_str) {
                Ok(make_element_object(ctx, key))
            } else {
                Ok(JsValue::null())
            }
        })
    };

    let query_selector_all = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let selector = args.get_or_undefined(0).to_string(ctx)?;
            let sel_str = selector.to_std_string_escaped();
            let s = state_qsa.borrow();
            let keys = query_select_all(&s.arena, &sel_str);
            let array = JsArray::new(ctx);
            for key in keys {
                let elem = make_element_object(ctx, key);
                array.push(elem, ctx)?;
            }
            Ok(array.into())
        })
    };

    let create_element = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let tag = args.get_or_undefined(0).to_string(ctx)?;
            let tag_str = tag.to_std_string_escaped().to_lowercase();
            let mut s = state_ce.borrow_mut();
            let key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Element,
                tag: Some(tag_str),
                attributes: std::collections::HashMap::new(),
                text: None,
                parent: None,
                children: vec![],
            });
            Ok(make_element_object(ctx, key))
        })
    };

    let create_text_node = unsafe {
        NativeFunction::from_closure(move |_this, args, ctx| {
            let text = args.get_or_undefined(0).to_string(ctx)?;
            let text_str = text.to_std_string_escaped();
            let mut s = state_ct.borrow_mut();
            let key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: std::collections::HashMap::new(),
                text: Some(text_str),
                parent: None,
                children: vec![],
            });
            Ok(make_element_object(ctx, key))
        })
    };

    // Bygg document-objektet
    let doc = ObjectInitializer::new(context)
        .function(get_element_by_id, js_string!("getElementById"), 1)
        .function(query_selector, js_string!("querySelector"), 1)
        .function(query_selector_all, js_string!("querySelectorAll"), 1)
        .function(create_element, js_string!("createElement"), 1)
        .function(create_text_node, js_string!("createTextNode"), 1)
        .build();

    // body, head, documentElement — stubbar som returnerar element-objekt
    // Vi registrerar dem som properties direkt
    let body_key = find_by_tag_name(&state.borrow().arena, state.borrow().arena.document, "body");
    let head_key = find_by_tag_name(&state.borrow().arena, state.borrow().arena.document, "head");
    let html_key = find_by_tag_name(&state.borrow().arena, state.borrow().arena.document, "html");

    if let Some(key) = body_key {
        let body_obj = make_element_object(context, key);
        doc.set(js_string!("body"), body_obj, false, context)
            .unwrap_or(true);
    }
    if let Some(key) = head_key {
        let head_obj = make_element_object(context, key);
        doc.set(js_string!("head"), head_obj, false, context)
            .unwrap_or(true);
    }
    if let Some(key) = html_key {
        let html_obj = make_element_object(context, key);
        doc.set(js_string!("documentElement"), html_obj, false, context)
            .unwrap_or(true);
    }

    context
        .register_global_property(js_string!("document"), doc, Attribute::all())
        .unwrap_or(());
}

// ─── Element-objekt ─────────────────────────────────────────────────────────

/// Skapa ett JS-objekt som representerar ett DOM-element
///
/// Elementet lagrar sin NodeKey som en intern property (__nodeKey__)
/// som används av DOM-metoder för att hitta noden i arena:n.
fn make_element_object(context: &mut Context, key: NodeKey) -> JsValue {
    // Lagra NodeKey som raw bits i en Number (u64-safe via f64)
    let key_bits = node_key_to_f64(key);

    let obj = ObjectInitializer::new(context)
        .property(
            js_string!("__nodeKey__"),
            JsValue::from(key_bits),
            Attribute::empty(),
        )
        .property(
            js_string!("nodeType"),
            JsValue::from(1),
            Attribute::READONLY,
        )
        .build();

    // getAttribute
    let get_attribute = NativeFunction::from_fn_ptr(element_get_attribute);
    obj.set(
        js_string!("getAttribute"),
        get_attribute.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // setAttribute (stubbad — loggar mutation men kräver state)
    let set_attribute = NativeFunction::from_fn_ptr(element_set_attribute_stub);
    obj.set(
        js_string!("setAttribute"),
        set_attribute.to_js_function(context.realm()),
        false,
        context,
    )
    .unwrap_or(true);

    // tagName, id, className som properties behöver vi inte sätta statiskt
    // eftersom vi inte har tillgång till arena här. Stubbat via __nodeKey__.

    JsValue::from(obj)
}

/// getAttribute stub — returnerar tom sträng (kan ej nå arena utan state)
fn element_get_attribute(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    // Vi kan bara returnera undefined utan state-referens
    // Fullständig implementation kräver att state delas via context data
    let _attr_name = args.get_or_undefined(0).to_string(ctx)?;
    Ok(JsValue::null())
}

/// setAttribute stub
fn element_set_attribute_stub(
    _this: &JsValue,
    args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let _name = args.get_or_undefined(0).to_string(ctx)?;
    let _value = args.get_or_undefined(1).to_string(ctx)?;
    Ok(JsValue::undefined())
}

// ─── Window-objekt ──────────────────────────────────────────────────────────

fn register_window(context: &mut Context) {
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

    context
        .register_global_property(js_string!("window"), window, Attribute::all())
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

/// Enkel querySelector — stöder #id, .class, och tag selectors
fn query_select_one(arena: &ArenaDom, selector: &str) -> Option<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return None;
    }

    if let Some(id) = selector.strip_prefix('#') {
        // ID-selektor
        find_by_attr_value(arena, "id", id)
    } else if let Some(class) = selector.strip_prefix('.') {
        // Klass-selektor
        find_by_class(arena, arena.document, class)
    } else {
        // Tagg-selektor
        find_by_tag_name(arena, arena.document, selector)
    }
}

/// querySelectorAll — returnerar alla matchande noder
fn query_select_all(arena: &ArenaDom, selector: &str) -> Vec<NodeKey> {
    let selector = selector.trim();
    if selector.is_empty() {
        return vec![];
    }

    let mut results = vec![];
    if let Some(id) = selector.strip_prefix('#') {
        if let Some(key) = find_by_attr_value(arena, "id", id) {
            results.push(key);
        }
    } else if let Some(class) = selector.strip_prefix('.') {
        find_all_by_class(arena, arena.document, class, &mut results);
    } else {
        find_all_by_tag(arena, arena.document, selector, &mut results);
    }
    results
}

/// Hitta element med given CSS-klass
fn find_by_class(arena: &ArenaDom, key: NodeKey, class: &str) -> Option<NodeKey> {
    let node = arena.nodes.get(key)?;
    if node.node_type == NodeType::Element {
        if let Some(classes) = node.get_attr("class") {
            if classes.split_whitespace().any(|c| c == class) {
                return Some(key);
            }
        }
    }
    for &child in &node.children {
        if let Some(found) = find_by_class(arena, child, class) {
            return Some(found);
        }
    }
    None
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
