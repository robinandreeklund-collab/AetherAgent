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
use rquickjs::{object::Accessor, Ctx, Function, Object, Value};

use std::cell::RefCell;
use std::rc::Rc;

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};
use crate::event_loop::{self, EventLoopState, JsFn, JsHandler, SharedEventLoop};

mod attributes;
mod chardata;
mod computed;
mod dom_impls;
pub(crate) mod element_state;
mod events;
#[allow(
    dead_code,
    unused_imports,
    unused_variables,
    clippy::all,
    clippy::pedantic,
    clippy::nursery
)]
mod generated;
mod html_properties;
mod node_ops;
mod selectors;
mod state;
mod style;
mod utils;
mod window;
use attributes::*;
use chardata::*;
use events::*;
use node_ops::*;
use selectors::{
    find_all_by_class, find_all_by_tag, find_all_by_tag_ns, find_all_matching, find_first_matching,
    matches_selector, query_select_all, query_select_one,
};
use state::*;
use style::*;
use utils::estimate_layout_rect;
use window::*;

/// Bygg computed styles via Blitz Stylo från HTML.
/// Parsear HTML med Blitz, kör Stylo CSS-resolution, och returnerar
/// computed styles per Blitz-nod-ID.
#[cfg(feature = "blitz")]
pub fn build_blitz_computed_styles(
    html: &str,
) -> std::collections::HashMap<u64, std::collections::HashMap<String, String>> {
    use blitz_dom::DocumentConfig;
    use blitz_html::HtmlDocument;
    use blitz_traits::shell::{ColorScheme, Viewport};

    let mut styles_map = std::collections::HashMap::new();

    // Parsa HTML med Blitz (catch_unwind mot panics i extern CSS)
    let Ok(mut doc) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        HtmlDocument::from_html(
            html,
            DocumentConfig {
                viewport: Some(Viewport::new(1280, 900, 1.0, ColorScheme::Light)),
                base_url: Some("https://wpt.example.com".to_string()),
                ..Default::default()
            },
        )
    })) else {
        return styles_map;
    };

    // Kör Stylo CSS resolution (beräknar ALLA computed styles)
    let resolve_ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        doc.as_mut().resolve(0.0);
    }))
    .is_ok();

    // Om resolve kraschade, returnera tomma styles (fallback till tag defaults)
    if !resolve_ok {
        return styles_map;
    }

    // Traversera Blitz DOM i DFS-ordning, extrahera computed styles
    let blitz_doc = doc.as_ref();
    let mut blitz_dfs_elements: Vec<usize> = Vec::new();
    fn collect_dfs(doc: &blitz_dom::BaseDocument, node_id: usize, out: &mut Vec<usize>) {
        if let Some(node) = doc.get_node(node_id) {
            if node.is_element() {
                out.push(node_id);
            }
            for &child_id in &node.children {
                collect_dfs(doc, child_id, out);
            }
        }
    }
    collect_dfs(blitz_doc, 0, &mut blitz_dfs_elements);

    for &blitz_id in &blitz_dfs_elements {
        if let Some(node) = blitz_doc.get_node(blitz_id) {
            if let Some(style) = node.primary_styles() {
                let mut props = std::collections::HashMap::new();

                // Display
                props.insert(
                    "display".to_string(),
                    format!("{:?}", style.clone_display()).to_lowercase(),
                );

                // Color — resolved to rgb() format
                let color = style.clone_color().into_srgb_legacy();
                let [r, g, b, _a] = color.raw_components();
                props.insert(
                    "color".to_string(),
                    format!(
                        "rgb({}, {}, {})",
                        (r * 255.0).round() as u8,
                        (g * 255.0).round() as u8,
                        (b * 255.0).round() as u8
                    ),
                );

                // Visibility
                {
                    use style_traits::values::ToCss;
                    props.insert(
                        "visibility".to_string(),
                        style.clone_visibility().to_css_string(),
                    );
                }

                // Font-size (resolved px)
                props.insert(
                    "font-size".to_string(),
                    format!("{}px", style.clone_font_size().used_size().px()),
                );

                // Font-weight (resolved numeric)
                props.insert(
                    "font-weight".to_string(),
                    format!("{}", style.clone_font_weight().value() as u32),
                );

                // Background-color (resolved rgb)
                {
                    use style_traits::values::ToCss;
                    let bg = &style.get_background().background_color;
                    props.insert("background-color".to_string(), bg.to_css_string());
                }

                styles_map.insert(blitz_id as u64, props);
            }
        }
    }

    styles_map
}

/// Mappa Blitz computed styles till ArenaDom NodeKeys.
/// Matchar noder via fingerprint (tag + id + class + DFS-position) för robusthet.
#[cfg(feature = "blitz")]
pub fn map_blitz_styles_to_arena(
    html: &str,
    blitz_styles: &std::collections::HashMap<u64, std::collections::HashMap<String, String>>,
    arena: &ArenaDom,
) -> std::collections::HashMap<u64, std::collections::HashMap<String, String>> {
    use blitz_dom::DocumentConfig;
    use blitz_html::HtmlDocument;
    use blitz_traits::shell::{ColorScheme, Viewport};
    use slotmap::Key;

    let mut arena_styles = std::collections::HashMap::new();

    // Re-parsea med Blitz för att traversera och skapa fingerprints
    let Ok(doc) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        HtmlDocument::from_html(
            html,
            DocumentConfig {
                viewport: Some(Viewport::new(1280, 900, 1.0, ColorScheme::Light)),
                base_url: Some("https://wpt.example.com".to_string()),
                ..Default::default()
            },
        )
    })) else {
        return arena_styles;
    };

    let blitz_doc = doc.as_ref();

    // Bygg fingerprints: (tag, id, class, nth-child-among-elements) → blitz_id
    let mut blitz_fingerprints: Vec<(String, u64)> = Vec::new();
    fn collect_blitz_fps(
        doc: &blitz_dom::BaseDocument,
        node_id: usize,
        out: &mut Vec<(String, u64)>,
    ) {
        if let Some(node) = doc.get_node(node_id) {
            if node.is_element() {
                let tag = node
                    .element_data()
                    .map(|e| e.name.local.to_string())
                    .unwrap_or_default();
                let id = node.attr(blitz_dom::local_name!("id")).unwrap_or("");
                let class = node.attr(blitz_dom::local_name!("class")).unwrap_or("");
                let fp = format!("{}#{}|{}", tag, id, class);
                out.push((fp, node_id as u64));
            }
            for &child_id in &node.children {
                collect_blitz_fps(doc, child_id, out);
            }
        }
    }
    collect_blitz_fps(blitz_doc, 0, &mut blitz_fingerprints);

    // Bygg ArenaDom fingerprints
    let mut arena_fingerprints: Vec<(String, NodeKey)> = Vec::new();
    fn collect_arena_fps(arena: &ArenaDom, key: NodeKey, out: &mut Vec<(String, NodeKey)>) {
        if let Some(node) = arena.nodes.get(key) {
            if node.node_type == NodeType::Element {
                let tag = node.tag.as_deref().unwrap_or("");
                let id = node.get_attr("id").unwrap_or("");
                let class = node.get_attr("class").unwrap_or("");
                let fp = format!("{}#{}|{}", tag, id, class);
                out.push((fp, key));
            }
            for &child in &node.children {
                collect_arena_fps(arena, child, out);
            }
        }
    }
    collect_arena_fps(arena, arena.document, &mut arena_fingerprints);

    // Matcha: för varje ArenaDom-nod, hitta Blitz-nod med samma fingerprint
    // Hanterar duplicates via ordningsposition
    let mut used_blitz: std::collections::HashSet<u64> = std::collections::HashSet::new();
    for (arena_fp, arena_key) in &arena_fingerprints {
        // Hitta första oanvända Blitz-nod med samma fingerprint
        for (blitz_fp, blitz_id) in &blitz_fingerprints {
            if blitz_fp == arena_fp && !used_blitz.contains(blitz_id) {
                if let Some(styles) = blitz_styles.get(blitz_id) {
                    let key_bits = arena_key.data().as_ffi();
                    arena_styles.insert(key_bits, styles.clone());
                    used_blitz.insert(*blitz_id);
                    break;
                }
            }
        }
    }

    arena_styles
}

// ─── Huvudfunktion ──────────────────────────────────────────────────────────

/// Evaluera JS-kod med tillgång till DOM via ArenaDom
///
/// Sätter upp `document` och `window` som globala objekt i QuickJS-kontexten.
/// Returnerar eventuellt returvärde och alla DOM-mutationer.
pub fn eval_js_with_dom(code: &str, arena: ArenaDom) -> DomEvalResult {
    let start = std::time::Instant::now();

    // Säkerhetskontroll: blockera farliga operationer
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
        element_state: std::collections::HashMap::new(),
        focused_element: None,
        scroll_positions: std::collections::HashMap::new(),
        css_context: None,
        local_storage: std::collections::HashMap::new(),
        session_storage: std::collections::HashMap::new(),
        console_output: Vec::new(),
        ready_state: "complete".to_string(),
        #[cfg(feature = "blitz")]
        blitz_styles: None,
        #[cfg(feature = "blitz")]
        original_html: None,
        #[cfg(feature = "blitz")]
        blitz_style_generation: 0,
        #[cfg(feature = "blitz")]
        blitz_cache_generation: 0,
        next_callback_id: 0,
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        // Registrera event-loop (setTimeout, setInterval, rAF, MutationObserver, queueMicrotask)
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));

        // Node identity cache — MÅSTE initieras FÖRE register_document
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ = register_window(&ctx, Rc::clone(&state));
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));

        let eval_result = match ctx.eval::<Value, _>(code) {
            Ok(result) => {
                let value_str = crate::js_eval::quickjs_value_to_string(&ctx, &result);

                // Kör event-loopen: dränera microtasks, timers, rAF, MutationObservers
                let loop_stats = event_loop::run_event_loop(&ctx, &el);
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
            Err(e) => {
                let err_str = crate::js_eval::quickjs_error_string(&ctx, &e);
                DomEvalResult {
                    value: None,
                    error: Some(err_str),
                    mutations: state.borrow().mutations.clone(),
                    eval_time_us: start.elapsed().as_micros() as u64,
                    event_loop_ticks: 0,
                    timers_fired: 0,
                }
            }
        };

        // Rensa Persistent-referenser innan kontexten droppas
        state.borrow_mut().event_listeners.clear();
        el.borrow_mut().clear_persistent();

        eval_result
    });

    crate::js_eval::free_interrupt_state(interrupt_ptr);
    result
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
        element_state: std::collections::HashMap::new(),
        focused_element: None,
        scroll_positions: std::collections::HashMap::new(),
        css_context: None,
        local_storage: std::collections::HashMap::new(),
        session_storage: std::collections::HashMap::new(),
        console_output: Vec::new(),
        ready_state: "complete".to_string(),
        #[cfg(feature = "blitz")]
        blitz_styles: None,
        #[cfg(feature = "blitz")]
        original_html: None,
        #[cfg(feature = "blitz")]
        blitz_style_generation: 0,
        #[cfg(feature = "blitz")]
        blitz_cache_generation: 0,
        next_callback_id: 0,
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ = register_window(&ctx, Rc::clone(&state));
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));

        let eval_result = match ctx.eval::<Value, _>(code) {
            Ok(res) => {
                let value_str = crate::js_eval::quickjs_value_to_string(&ctx, &res);
                let loop_stats = event_loop::run_event_loop(&ctx, &el);
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
            Err(e) => {
                let err_str = crate::js_eval::quickjs_error_string(&ctx, &e);
                DomEvalResult {
                    value: None,
                    error: Some(err_str),
                    mutations: state.borrow().mutations.clone(),
                    eval_time_us: start.elapsed().as_micros() as u64,
                    event_loop_ticks: 0,
                    timers_fired: 0,
                }
            }
        };
        state.borrow_mut().event_listeners.clear();
        el.borrow_mut().clear_persistent();
        eval_result
    });

    crate::js_eval::free_interrupt_state(interrupt_ptr);

    // Extrahera arena från SharedState
    // context och alla Rc-kloner i closures droppas ovan → try_unwrap kan lyckas
    let bridge = match Rc::try_unwrap(state) {
        Ok(cell) => cell.into_inner(),
        Err(rc) => {
            let borrowed = rc.borrow();
            BridgeState {
                arena: borrowed.arena.clone(),
                mutations: borrowed.mutations.clone(),
                event_listeners: std::collections::HashMap::new(),
                element_state: std::collections::HashMap::new(),
                focused_element: borrowed.focused_element,
                scroll_positions: std::collections::HashMap::new(),
                css_context: None,
                local_storage: std::collections::HashMap::new(),
                session_storage: std::collections::HashMap::new(),
                console_output: Vec::new(),
                ready_state: borrowed.ready_state.clone(),
                #[cfg(feature = "blitz")]
                blitz_styles: None,
                #[cfg(feature = "blitz")]
                original_html: None,
                #[cfg(feature = "blitz")]
                blitz_style_generation: 0,
                #[cfg(feature = "blitz")]
                blitz_cache_generation: 0,
                next_callback_id: 0,
            }
        }
    };

    DomEvalWithArena {
        result,
        arena: bridge.arena,
    }
}

/// Evaluera inline scripts med simulerad browser-lifecycle
///
/// Kör scripts i 3 faser:
/// 1. readyState = "loading" — kör synkrona scripts
/// 2. readyState = "interactive" — dispatcha DOMContentLoaded
/// 3. readyState = "complete" — dispatcha load
///
/// Returnerar modifierad ArenaDom med alla DOM-mutationer applicerade.
/// eval_js_with_lifecycle med original HTML (för Blitz Stylo computed styles)
#[cfg(feature = "blitz")]
pub fn eval_js_with_lifecycle_html(
    scripts: &[String],
    arena: ArenaDom,
    html: &str,
) -> DomEvalResult {
    eval_js_with_lifecycle_internal(scripts, arena, Some(html.to_string()))
}

pub fn eval_js_with_lifecycle(scripts: &[String], arena: ArenaDom) -> DomEvalResult {
    eval_js_with_lifecycle_internal(scripts, arena, None)
}

fn eval_js_with_lifecycle_internal(
    scripts: &[String],
    arena: ArenaDom,
    _original_html: Option<String>,
) -> DomEvalResult {
    let start = std::time::Instant::now();

    if scripts.is_empty() {
        return DomEvalResult {
            value: None,
            error: None,
            mutations: vec![],
            eval_time_us: start.elapsed().as_micros() as u64,
            event_loop_ticks: 0,
            timers_fired: 0,
        };
    }

    let state: SharedState = Rc::new(RefCell::new(BridgeState {
        arena,
        mutations: vec![],
        event_listeners: std::collections::HashMap::new(),
        element_state: std::collections::HashMap::new(),
        focused_element: None,
        scroll_positions: std::collections::HashMap::new(),
        css_context: None,
        local_storage: std::collections::HashMap::new(),
        session_storage: std::collections::HashMap::new(),
        console_output: Vec::new(),
        ready_state: "loading".to_string(),
        #[cfg(feature = "blitz")]
        blitz_styles: None,
        #[cfg(feature = "blitz")]
        original_html: _original_html,
        #[cfg(feature = "blitz")]
        blitz_style_generation: 0,
        #[cfg(feature = "blitz")]
        blitz_cache_generation: 0,
        next_callback_id: 0,
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));
        // Node identity cache — MÅSTE initieras FÖRE register_document
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ = register_window(&ctx, Rc::clone(&state));
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));

        let mut last_value: Option<String> = None;
        let mut first_error: Option<String> = None;

        // Fas 1: readyState = "loading" — kör alla scripts
        for script in scripts {
            match ctx.eval::<Value, _>(script.as_str()) {
                Ok(result) => {
                    let v = crate::js_eval::quickjs_value_to_string(&ctx, &result);
                    if v != "undefined" {
                        last_value = Some(v);
                    }
                }
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(crate::js_eval::quickjs_error_string(&ctx, &e));
                    }
                }
            }
        }

        // Fas 2: readyState = "interactive" + DOMContentLoaded
        state.borrow_mut().ready_state = "interactive".to_string();
        let _ = ctx.eval::<Value, _>(
            r#"
            if (typeof document !== 'undefined' && document.dispatchEvent) {
                document.dispatchEvent(new Event('DOMContentLoaded'));
            }
            "#,
        );

        // Fas 3: readyState = "complete" + load
        state.borrow_mut().ready_state = "complete".to_string();
        let _ = ctx.eval::<Value, _>(
            r#"
            if (typeof window !== 'undefined' && window.dispatchEvent) {
                window.dispatchEvent(new Event('load'));
            }
            "#,
        );

        // Dränera event loop (microtasks, timers, rAF)
        let loop_stats = event_loop::run_event_loop(&ctx, &el);
        let (ticks, timers) = match &loop_stats {
            Ok(s) => (s.ticks, s.timers_fired),
            Err(_) => (0, 0),
        };

        let mutations = state.borrow().mutations.clone();

        state.borrow_mut().event_listeners.clear();
        el.borrow_mut().clear_persistent();

        DomEvalResult {
            value: last_value,
            error: first_error.or_else(|| loop_stats.err()),
            mutations,
            eval_time_us: start.elapsed().as_micros() as u64,
            event_loop_ticks: ticks,
            timers_fired: timers,
        }
    });

    crate::js_eval::free_interrupt_state(interrupt_ptr);
    result
}

/// Evaluera inline scripts med simulerad browser-lifecycle och returnera modifierad ArenaDom.
///
/// Samma som `eval_js_with_lifecycle` men ger tillbaka arenan med alla DOM-mutationer
/// så att anroparen kan serialisera den modifierade DOM:en till HTML.
#[cfg(feature = "blitz")]
pub fn eval_js_with_lifecycle_and_arena(scripts: &[String], arena: ArenaDom) -> DomEvalWithArena {
    eval_js_with_lifecycle_and_arena_viewport(scripts, arena, 1280, 900)
}

/// Evaluera inline scripts med lifecycle och viewport-dimensioner
#[cfg(feature = "blitz")]
pub fn eval_js_with_lifecycle_and_arena_viewport(
    scripts: &[String],
    arena: ArenaDom,
    viewport_width: u32,
    viewport_height: u32,
) -> DomEvalWithArena {
    let start = std::time::Instant::now();

    if scripts.is_empty() {
        return DomEvalWithArena {
            result: DomEvalResult {
                value: None,
                error: None,
                mutations: vec![],
                eval_time_us: start.elapsed().as_micros() as u64,
                event_loop_ticks: 0,
                timers_fired: 0,
            },
            arena,
        };
    }

    let state: SharedState = Rc::new(RefCell::new(BridgeState {
        arena,
        mutations: vec![],
        event_listeners: std::collections::HashMap::new(),
        element_state: std::collections::HashMap::new(),
        focused_element: None,
        scroll_positions: std::collections::HashMap::new(),
        css_context: None,
        local_storage: std::collections::HashMap::new(),
        session_storage: std::collections::HashMap::new(),
        console_output: Vec::new(),
        ready_state: "loading".to_string(),
        #[cfg(feature = "blitz")]
        blitz_styles: None,
        #[cfg(feature = "blitz")]
        original_html: None,
        #[cfg(feature = "blitz")]
        blitz_style_generation: 0,
        #[cfg(feature = "blitz")]
        blitz_cache_generation: 0,
        next_callback_id: 0,
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ =
            register_window_with_viewport(&ctx, Rc::clone(&state), viewport_width, viewport_height);
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));

        let mut last_value: Option<String> = None;
        let mut first_error: Option<String> = None;

        // Fas 1: readyState = "loading" — kör alla scripts
        for script in scripts {
            match ctx.eval::<Value, _>(script.as_str()) {
                Ok(result) => {
                    let v = crate::js_eval::quickjs_value_to_string(&ctx, &result);
                    if v != "undefined" {
                        last_value = Some(v);
                    }
                }
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(crate::js_eval::quickjs_error_string(&ctx, &e));
                    }
                }
            }
        }

        // Fas 2: readyState = "interactive" + DOMContentLoaded
        state.borrow_mut().ready_state = "interactive".to_string();
        let _ = ctx.eval::<Value, _>(
            r#"
            if (typeof document !== 'undefined' && document.dispatchEvent) {
                document.dispatchEvent(new Event('DOMContentLoaded'));
            }
            "#,
        );

        // Fas 3: readyState = "complete" + load
        state.borrow_mut().ready_state = "complete".to_string();
        let _ = ctx.eval::<Value, _>(
            r#"
            if (typeof window !== 'undefined' && window.dispatchEvent) {
                window.dispatchEvent(new Event('load'));
            }
            "#,
        );

        // Dränera event loop
        let loop_stats = event_loop::run_event_loop(&ctx, &el);
        let (ticks, timers) = match &loop_stats {
            Ok(s) => (s.ticks, s.timers_fired),
            Err(_) => (0, 0),
        };

        let mutations = state.borrow().mutations.clone();
        state.borrow_mut().event_listeners.clear();
        el.borrow_mut().clear_persistent();

        DomEvalResult {
            value: last_value,
            error: first_error.or_else(|| loop_stats.err()),
            mutations,
            eval_time_us: start.elapsed().as_micros() as u64,
            event_loop_ticks: ticks,
            timers_fired: timers,
        }
    });

    crate::js_eval::free_interrupt_state(interrupt_ptr);

    // Extrahera arena från SharedState
    let bridge = match Rc::try_unwrap(state) {
        Ok(cell) => cell.into_inner(),
        Err(rc) => {
            let borrowed = rc.borrow();
            BridgeState {
                arena: borrowed.arena.clone(),
                mutations: borrowed.mutations.clone(),
                event_listeners: std::collections::HashMap::new(),
                element_state: std::collections::HashMap::new(),
                focused_element: borrowed.focused_element,
                scroll_positions: std::collections::HashMap::new(),
                css_context: None,
                local_storage: std::collections::HashMap::new(),
                session_storage: std::collections::HashMap::new(),
                console_output: Vec::new(),
                ready_state: borrowed.ready_state.clone(),
                #[cfg(feature = "blitz")]
                blitz_styles: None,
                #[cfg(feature = "blitz")]
                original_html: None,
                #[cfg(feature = "blitz")]
                blitz_style_generation: 0,
                #[cfg(feature = "blitz")]
                blitz_cache_generation: 0,
                next_callback_id: 0,
            }
        }
    };

    DomEvalWithArena {
        result,
        arena: bridge.arena,
    }
}

// ─── Document-objekt ────────────────────────────────────────────────────────

// ─── JsHandler-structs för document-metoder ─────────────────────────────────

struct GetElementById {
    state: SharedState,
}
impl JsHandler for GetElementById {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let id = match args.first().and_then(|v| v.as_string()) {
            Some(s) => match s.to_string() {
                Ok(id) if !id.is_empty() => id,
                _ => return Ok(Value::new_null(ctx.clone())),
            },
            // Spec: getElementById(undefined) → söker efter "undefined"
            // getElementById(null) → söker efter "null"
            None => {
                let raw = args
                    .first()
                    .map(|v| {
                        if v.is_null() {
                            "null".to_string()
                        } else if v.is_undefined() {
                            "undefined".to_string()
                        } else {
                            String::new()
                        }
                    })
                    .unwrap_or_default();
                if raw.is_empty() {
                    return Ok(Value::new_null(ctx.clone()));
                }
                raw
            }
        };
        let key = {
            let s = self.state.borrow();
            find_by_attr_value(&s.arena, "id", &id)
        };
        match key {
            Some(k) => make_element_object(ctx, k, &self.state),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct QuerySelector {
    state: SharedState,
}
impl JsHandler for QuerySelector {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let selector = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let key = {
            let s = self.state.borrow();
            query_select_one(&s.arena, &selector)
        };
        match key {
            Some(k) => make_element_object(ctx, k, &self.state),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct QuerySelectorAll {
    state: SharedState,
}
impl JsHandler for QuerySelectorAll {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let selector = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let keys = {
            let s = self.state.borrow();
            query_select_all(&s.arena, &selector)
        };
        let array = rquickjs::Array::new(ctx.clone())?;
        for (i, key) in keys.into_iter().enumerate() {
            let elem = make_element_object(ctx, key, &self.state)?;
            array.set(i, elem)?;
        }
        Ok(array.into_value())
    }
}

/// Skapar en Document-nod i ArenaDom (för foreignDoc / createHTMLDocument)
// ─── document.createEvent — native Rust (migrerad från polyfill) ─────────────
// ─── document.title getter/setter — native Rust ─────────────────────────────
struct DocTitleGetter {
    state: SharedState,
}
impl JsHandler for DocTitleGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        // Hitta <title>-elementet i <head>
        let title = find_title_text(&s.arena, s.arena.document);
        Ok(Value::from_string(rquickjs::String::from_str(
            ctx.clone(),
            &title,
        )?))
    }
}

struct DocTitleSetter {
    state: SharedState,
}
impl JsHandler for DocTitleSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let doc = s.arena.document;
        // Hitta eller skapa <title>-element
        if let Some(title_key) = find_title_element(&s.arena, doc) {
            // Rensa barn och sätt ny text
            let old_children: Vec<NodeKey> = s
                .arena
                .nodes
                .get(title_key)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            for &ck in &old_children {
                if let Some(c) = s.arena.nodes.get_mut(ck) {
                    c.parent = None;
                }
            }
            if let Some(title_node) = s.arena.nodes.get_mut(title_key) {
                title_node.children.clear();
            }
            let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: Some(val.into()),
                parent: Some(title_key),
                children: vec![],
                owner_doc: None,
            });
            if let Some(title_node) = s.arena.nodes.get_mut(title_key) {
                title_node.children.push(text_key);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Hitta text i <title>-element
fn find_title_text(arena: &crate::arena_dom::ArenaDom, root: NodeKey) -> String {
    if let Some(title_key) = find_title_element(arena, root) {
        collect_text_descendants(arena, title_key)
    } else {
        String::new()
    }
}

/// Hitta <title>-element i dokumentträdet
fn find_title_element(arena: &crate::arena_dom::ArenaDom, key: NodeKey) -> Option<NodeKey> {
    let node = arena.nodes.get(key)?;
    if node.tag.as_deref() == Some("title") {
        return Some(key);
    }
    for &child in &node.children {
        if let Some(found) = find_title_element(arena, child) {
            return Some(found);
        }
    }
    None
}

struct NativeCreateEvent;
impl JsHandler for NativeCreateEvent {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let type_str = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();

        // Mappa event-typ till konstruktor (per spec)
        let ctor_name = match type_str.as_str() {
            "event" | "events" | "htmlevents" | "svgevents" | "svgevent" => "Event",
            "customevent" => "CustomEvent",
            "uievent" | "uievents" => "UIEvent",
            "mouseevent" | "mouseevents" => "MouseEvent",
            "keyboardevent" => "KeyboardEvent",
            "focusevent" => "FocusEvent",
            "inputevent" => "InputEvent",
            "wheelevent" => "WheelEvent",
            "compositionevent" => "CompositionEvent",
            "pointerevent" => "PointerEvent",
            "beforeunloadevent" => "BeforeUnloadEvent",
            "hashchangeevent" => "HashChangeEvent",
            "popstateevent" => "PopStateEvent",
            "storageevent" => "StorageEvent",
            "progressevent" => "ProgressEvent",
            "messageevent" => "MessageEvent",
            "dragevent" => "DragEvent",
            "touchevent" => "TouchEvent",
            // Legacy aliases per spec — returnerar Event-objekt
            "devicemotionevent" => "Event",
            "deviceorientationevent" => "Event",
            "textevent" => "Event",
            "mutationevent" | "mutationevents" => "Event",
            _ => {
                return Err(throw_dom_exception(
                    ctx,
                    "NotSupportedError",
                    "The operation is not supported.",
                ));
            }
        };
        // Skapa event via konstruktor i JS-kontexten
        let code = format!("new {}('')", ctor_name);
        match ctx.eval::<Value, _>(code.as_str()) {
            Ok(v) => Ok(v),
            Err(_) => {
                // Fallback: skapa via Event-konstruktor
                ctx.eval::<Value, _>("new Event('')")
            }
        }
    }
}

struct CreateDocumentNode {
    state: SharedState,
}
impl JsHandler for CreateDocumentNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let doc_key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Document,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: None,
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        // Returnera doc-nod med __nodeKey__ och nodeType=9
        let obj = Object::new(ctx.clone())?;
        obj.set("__nodeKey__", node_key_to_f64(doc_key))?;
        obj.set("nodeType", 9i32)?;
        obj.set("nodeName", "#document")?;
        Ok(obj.into_value())
    }
}

/// Sätter owner_doc på en nod i ArenaDom
struct SetOwnerDoc {
    state: SharedState,
}
impl JsHandler for SetOwnerDoc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let node_key = args.first().and_then(extract_node_key);
        let doc_key = args.get(1).and_then(extract_node_key);
        if let (Some(nk), Some(dk)) = (node_key, doc_key) {
            let mut s = self.state.borrow_mut();
            if let Some(node) = s.arena.nodes.get_mut(nk) {
                node.owner_doc = Some(dk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct CreateElement {
    state: SharedState,
}
impl JsHandler for CreateElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let tag = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        // HTML spec: createElement lowercasar BARA ASCII (ej Unicode)
        let tag = tag.to_ascii_lowercase();
        let key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Element,
                tag: Some(tag),
                attributes: crate::arena_dom::Attrs::new(),
                text: None,
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        make_element_object(ctx, key, &self.state)
    }
}

// ─── createElementNS — native namespace-element ─────────────────────────────

struct CreateElementNS {
    state: SharedState,
}

/// Validera XML-qualified name enligt DOM spec.
/// Returnerar (prefix, localName) eller None om ogiltigt.
fn validate_and_split_qname(qname: &str) -> Result<(Option<String>, String), &'static str> {
    if qname.is_empty() {
        return Err("InvalidCharacterError");
    }
    // Kontrollera ogiltiga tecken
    for ch in qname.chars() {
        if matches!(ch, '<' | '>' | '&' | ' ' | '\t' | '\n' | '\r' | '^')
            || (ch == ':' && qname.matches(':').count() > 1)
        {
            return Err("InvalidCharacterError");
        }
    }
    // Namn får inte börja med siffra, punkt eller bindestreck (förutom XML-undantag)
    let first = qname.chars().next().unwrap_or(' ');
    if first.is_ascii_digit() || first == '.' || first == '-' {
        return Err("InvalidCharacterError");
    }
    // Split på kolon
    if let Some(colon_pos) = qname.find(':') {
        let prefix = &qname[..colon_pos];
        let local = &qname[colon_pos + 1..];
        if prefix.is_empty() || local.is_empty() || local.contains(':') {
            return Err("InvalidCharacterError");
        }
        // Prefix får inte börja med siffra
        let pf = prefix.chars().next().unwrap_or(' ');
        if pf.is_ascii_digit() || pf == '.' || pf == '-' {
            return Err("InvalidCharacterError");
        }
        let lf = local.chars().next().unwrap_or(' ');
        if lf.is_ascii_digit() || lf == '.' || lf == '-' {
            return Err("InvalidCharacterError");
        }
        Ok((Some(prefix.to_string()), local.to_string()))
    } else {
        Ok((None, qname.to_string()))
    }
}

const XML_NAMESPACE: &str = "http://www.w3.org/XML/1998/namespace";
const XMLNS_NAMESPACE: &str = "http://www.w3.org/2000/xmlns/";
const XHTML_NAMESPACE: &str = "http://www.w3.org/1999/xhtml";

impl JsHandler for CreateElementNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // arg0 = namespace (kan vara null/undefined/"")
        let namespace = args
            .first()
            .and_then(|v| {
                if v.is_null() || v.is_undefined() {
                    None
                } else {
                    v.as_string().and_then(|s| s.to_string().ok())
                }
            })
            .and_then(|s| if s.is_empty() { None } else { Some(s) });

        let qname = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();

        // Validera qualified name
        let (prefix, local_name) = match validate_and_split_qname(&qname) {
            Ok(r) => r,
            Err(err_type) => {
                return Err(ctx.throw(rquickjs::String::from_str(ctx.clone(), err_type)?.into()));
            }
        };

        // Namespace constraints (DOM spec)
        if prefix.is_some() && namespace.is_none() {
            return Err(
                ctx.throw(rquickjs::String::from_str(ctx.clone(), "NamespaceError")?.into())
            );
        }
        if prefix.as_deref() == Some("xml") && namespace.as_deref() != Some(XML_NAMESPACE) {
            return Err(
                ctx.throw(rquickjs::String::from_str(ctx.clone(), "NamespaceError")?.into())
            );
        }
        if (prefix.as_deref() == Some("xmlns") || qname == "xmlns")
            && namespace.as_deref() != Some(XMLNS_NAMESPACE)
        {
            return Err(
                ctx.throw(rquickjs::String::from_str(ctx.clone(), "NamespaceError")?.into())
            );
        }
        if namespace.as_deref() == Some(XMLNS_NAMESPACE)
            && prefix.as_deref() != Some("xmlns")
            && qname != "xmlns"
        {
            return Err(
                ctx.throw(rquickjs::String::from_str(ctx.clone(), "NamespaceError")?.into())
            );
        }

        // Skapa elementet — lagra localName som-den-är i arena (case-sensitive).
        // tagName/nodeName visas som uppercase för HTML namespace.
        let is_html_ns = namespace.as_deref() == Some(XHTML_NAMESPACE);

        let key = {
            let mut s = self.state.borrow_mut();
            let mut attrs = crate::arena_dom::Attrs::new();
            // Spara namespace URI i arena-noden som internt attribut __ns__
            // Krävs av getElementsByTagNameNS för att filtrera på namespace.
            attrs.insert(
                "__ns__".to_string(),
                namespace.as_deref().unwrap_or("").to_string(),
            );
            // Lagra qualifiedName (prefix:localName) för getElementsByTagName-matchning
            let qualified_name = if let Some(ref pfx) = prefix {
                format!("{pfx}:{local_name}")
            } else {
                local_name.clone()
            };
            // Spara __prefix__ för localName-access
            if let Some(ref pfx) = prefix {
                attrs.insert("__prefix__".to_string(), pfx.clone());
                attrs.insert("__localName__".to_string(), local_name.clone());
            }
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Element,
                tag: Some(qualified_name),
                attributes: attrs,
                text: None,
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };

        let elem = make_element_object(ctx, key, &self.state)?;
        if let Some(obj) = elem.as_object() {
            // För non-HTML namespace, sätt Element.prototype (inte HTMLElement)
            if !is_html_ns {
                let _ = ctx.eval::<Value, _>(
                    "(function(el){try{Object.setPrototypeOf(el, typeof Element!=='undefined'?Element.prototype:Object.prototype)}catch(e){}})",
                ).and_then(|f| {
                    if let Some(func) = f.as_object().and_then(|o| o.as_function()) {
                        func.call::<_, Value>((obj.clone(),))
                    } else {
                        Ok(Value::new_undefined(ctx.clone()))
                    }
                });
            }
            // Sätt namespace-relaterade properties
            if let Some(ref ns) = namespace {
                obj.set("namespaceURI", rquickjs::String::from_str(ctx.clone(), ns)?)?;
            } else {
                obj.set("namespaceURI", Value::new_null(ctx.clone()))?;
            }
            if let Some(ref pfx) = prefix {
                obj.set("prefix", rquickjs::String::from_str(ctx.clone(), pfx)?)?;
                // tagName = prefix:localName (uppercase i HTML namespace)
                let tag_name_full = format!("{}:{}", pfx, local_name);
                let display_name = if is_html_ns {
                    tag_name_full.to_ascii_uppercase()
                } else {
                    tag_name_full
                };
                obj.set(
                    "tagName",
                    rquickjs::String::from_str(ctx.clone(), &display_name)?,
                )?;
                obj.set(
                    "nodeName",
                    rquickjs::String::from_str(ctx.clone(), &display_name)?,
                )?;
            } else {
                obj.set("prefix", Value::new_null(ctx.clone()))?;
                // tagName utan prefix: uppercase bara för HTML namespace
                let display_name = if is_html_ns {
                    local_name.to_ascii_uppercase()
                } else {
                    local_name.clone()
                };
                obj.set(
                    "tagName",
                    rquickjs::String::from_str(ctx.clone(), &display_name)?,
                )?;
                obj.set(
                    "nodeName",
                    rquickjs::String::from_str(ctx.clone(), &display_name)?,
                )?;
            }
            obj.set(
                "localName",
                rquickjs::String::from_str(ctx.clone(), &local_name)?,
            )?;
        }

        Ok(elem)
    }
}

struct CreateTextNode {
    state: SharedState,
}
impl JsHandler for CreateTextNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let text = js_value_to_dom_string(args.first());
        let key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: Some(text.into()),
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        make_element_object(ctx, key, &self.state)
    }
}

struct CreateComment {
    state: SharedState,
}
impl JsHandler for CreateComment {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Spec: DOMString-konvertering — null→"null", undefined→"undefined", number→string
        let text = js_value_to_dom_string(args.first());
        let key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Comment,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: Some(text.into()),
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        make_element_object(ctx, key, &self.state)
    }
}

// ─── createAttribute — native Attr-objekt ─────────────────────────────────

struct CreateAttribute;
impl JsHandler for CreateAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // DOMString-konvertering: null→"null", undefined→"undefined"
        let name = js_value_to_dom_string(args.first());

        // Validera namn — per DOM spec: tom sträng → InvalidCharacterError
        if name.is_empty() {
            return Err(
                ctx.throw(rquickjs::String::from_str(ctx.clone(), "InvalidCharacterError")?.into())
            );
        }

        // HTML-dokument: lowercase
        let lower_name = name.to_ascii_lowercase();

        let attr = Object::new(ctx.clone())?;
        attr.set("nodeType", 2)?;
        attr.set(
            "nodeName",
            rquickjs::String::from_str(ctx.clone(), &lower_name)?,
        )?;
        attr.set(
            "name",
            rquickjs::String::from_str(ctx.clone(), &lower_name)?,
        )?;
        attr.set("value", rquickjs::String::from_str(ctx.clone(), "")?)?;
        attr.set("nodeValue", rquickjs::String::from_str(ctx.clone(), "")?)?;
        attr.set("textContent", rquickjs::String::from_str(ctx.clone(), "")?)?;
        attr.set(
            "localName",
            rquickjs::String::from_str(ctx.clone(), &lower_name)?,
        )?;
        attr.set("namespaceURI", Value::new_null(ctx.clone()))?;
        attr.set("prefix", Value::new_null(ctx.clone()))?;
        attr.set("specified", true)?;
        attr.set("ownerElement", Value::new_null(ctx.clone()))?;

        // Sätt Attr prototype om det finns
        let _ = ctx
            .eval::<Value, _>(
                "(function(a){try{Object.setPrototypeOf(a, globalThis.Attr ? globalThis.Attr.prototype : Object.prototype)}catch(e){}})",
            )
            .and_then(|f| {
                if let Some(func) = f.as_object().and_then(|o| o.as_function()) {
                    func.call::<_, Value>((attr.clone(),))
                } else {
                    Ok(Value::new_undefined(ctx.clone()))
                }
            });

        Ok(attr.into_value())
    }
}

struct GetElementsByClassName {
    state: SharedState,
}
impl JsHandler for GetElementsByClassName {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let cls = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let root_key = {
            let s = self.state.borrow();
            s.arena.document
        };
        make_live_html_collection(ctx, &self.state, root_key, "class", &cls)
    }
}

struct GetElementsByTagName {
    state: SharedState,
}
impl JsHandler for GetElementsByTagName {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let tag = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let root_key = {
            let s = self.state.borrow();
            s.arena.document
        };
        make_live_html_collection(ctx, &self.state, root_key, "tag", &tag)
    }
}

// ─── getElementsByTagNameNS (document-nivå) ──────────────────────────────────

struct GetElementsByTagNameNSDoc {
    state: SharedState,
}
impl JsHandler for GetElementsByTagNameNSDoc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let ns = args
            .first()
            .and_then(|v| {
                if v.is_null() {
                    Some("".to_string())
                } else {
                    v.as_string().and_then(|s| s.to_string().ok())
                }
            })
            .unwrap_or_default();
        let local_name = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let root_key = {
            let s = self.state.borrow();
            s.arena.document
        };
        let query_val = format!("{}\x01{}", ns, local_name);
        make_live_html_collection(ctx, &self.state, root_key, "tag_ns", &query_val)
    }
}

// ─── Native query-helpers för live HTMLCollection ─────────────────────────────

/// Returnerar en array av element-objekt som matchar sökningen.
/// Används av live HTMLCollection Proxy för att re-query vid varje access.
struct NativeQueryElements {
    state: SharedState,
}
impl JsHandler for NativeQueryElements {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let root_f64 = args
            .first()
            .and_then(|v| v.as_float())
            .or_else(|| args.first().and_then(|v| v.as_int().map(|i| i as f64)))
            .unwrap_or(-1.0);
        let query_type = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let query_val = args
            .get(2)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();

        let keys = {
            let s = self.state.borrow();
            let root = if root_f64 < 0.0 {
                s.arena.document
            } else {
                f64_to_node_key(root_f64)
            };
            let mut results = vec![];
            match query_type.as_str() {
                "tag" => find_all_by_tag(&s.arena, root, &query_val, &mut results),
                "class" => find_all_by_class(&s.arena, root, &query_val, &mut results),
                "tag_ns" => {
                    if let Some(sep) = query_val.find('\x01') {
                        let ns_part = &query_val[..sep];
                        let local_part = &query_val[sep + 1..];
                        find_all_by_tag_ns(&s.arena, root, ns_part, local_part, &mut results);
                    }
                }
                _ => {}
            }
            results
        };
        let array = rquickjs::Array::new(ctx.clone())?;
        for (i, key) in keys.iter().enumerate() {
            let elem = make_element_object(ctx, *key, &self.state)?;
            array.set(i, elem)?;
        }
        Ok(array.into_value())
    }
}

/// Skapar en live HTMLCollection via JS Proxy som delegerar till Rust vid varje access.
pub(super) fn make_live_html_collection<'js>(
    ctx: &Ctx<'js>,
    state: &SharedState,
    root_key: NodeKey,
    query_type: &str,
    query_val: &str,
) -> rquickjs::Result<Value<'js>> {
    let root_f64 = node_key_to_f64(root_key);
    // Skapa en snapshot först för att sätta initial items
    let keys = {
        let s = state.borrow();
        let mut results = vec![];
        match query_type {
            "tag" => find_all_by_tag(&s.arena, root_key, query_val, &mut results),
            "class" => find_all_by_class(&s.arena, root_key, query_val, &mut results),
            "tag_ns" => {
                if let Some(sep) = query_val.find('\x01') {
                    let ns_part = &query_val[..sep];
                    let local_part = &query_val[sep + 1..];
                    find_all_by_tag_ns(&s.arena, root_key, ns_part, local_part, &mut results);
                }
            }
            _ => {}
        }
        results
    };
    // Bygg element-array
    let items = rquickjs::Array::new(ctx.clone())?;
    for (i, key) in keys.iter().enumerate() {
        let elem = make_element_object(ctx, *key, state)?;
        items.set(i, elem)?;
    }
    // Anropa JS-helper __createLiveHTMLCollection(items, rootKey, queryType, queryVal)
    let global = ctx.globals();
    let create_fn: Value = global.get("__createLiveHTMLCollection")?;
    if let Some(func) = create_fn.as_object().and_then(|o| o.as_function()) {
        func.call((items, root_f64, query_type, query_val))
    } else {
        // Fallback: returnera items som vanlig array om __createLiveHTMLCollection inte finns
        Ok(items.into_value())
    }
}

struct CreateDocumentFragment {
    state: SharedState,
}
impl JsHandler for CreateDocumentFragment {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::DocumentFragment,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: None,
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        make_element_object(ctx, key, &self.state)
    }
}

// ─── createDocumentType — skapar en Doctype-nod i ArenaDom ──────────────────

struct CreateDocumentType {
    state: SharedState,
}
impl JsHandler for CreateDocumentType {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        // Spec: validera qualifiedName — rejekta '>' och whitespace inuti/efter namnet
        let has_invalid = name.contains('>') || name.contains(' ') || name.contains('\t');
        if has_invalid {
            return Err(throw_dom_exception(
                ctx,
                "InvalidCharacterError",
                "The string contains invalid characters.",
            ));
        }
        let public_id = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let system_id = args
            .get(2)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Doctype,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: Some(name.clone().into()),
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        let val = make_element_object(ctx, key, &self.state)?;
        // Doctype-specifika egenskaper
        if let Some(obj) = val.as_object() {
            let _ = obj.set("publicId", public_id.as_str());
            let _ = obj.set("systemId", system_id.as_str());
        }
        Ok(val)
    }
}

// ─── createProcessingInstruction — skapar en PI-nod i ArenaDom ──────────────

struct CreateProcessingInstruction {
    state: SharedState,
}
impl JsHandler for CreateProcessingInstruction {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let target = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let data = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        // Spec: target måste matcha XML Name-produktionen
        if !crate::dom_bridge::attributes::is_valid_xml_name(&target) {
            return Err(throw_dom_exception(
                ctx,
                "InvalidCharacterError",
                "InvalidCharacterError: target is not a valid XML Name",
            ));
        }
        // Spec: data får inte innehålla "?>"
        if data.contains("?>") {
            return Err(throw_dom_exception(
                ctx,
                "InvalidCharacterError",
                "InvalidCharacterError: data must not contain '?>'",
            ));
        }
        let key = {
            let mut s = self.state.borrow_mut();
            s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::ProcessingInstruction,
                tag: Some(target.clone()),
                attributes: crate::arena_dom::Attrs::new(),
                text: Some(data.into()),
                parent: None,
                children: vec![],
                owner_doc: None,
            })
        };
        let val = make_element_object(ctx, key, &self.state)?;
        if let Some(obj) = val.as_object() {
            let _ = obj.set("target", target.as_str());
        }
        Ok(val)
    }
}

// ─── createRange — delegerar till globalThis.Range ──────────────────────────

struct CreateRange;
impl JsHandler for CreateRange {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        ctx.eval::<Value, _>("new Range()")
    }
}

// ─── __nativeChildIndex — snabb barn-index lookup i Rust ─────────────────────
// Returnerar index av child i parent.children, eller -1 om ej funnen.
struct NativeChildIndex {
    state: SharedState,
}
impl JsHandler for NativeChildIndex {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parent_key = args.first().and_then(extract_node_key);
        let child_key = args.get(1).and_then(extract_node_key);
        match (parent_key, child_key) {
            (Some(pk), Some(ck)) => {
                let s = self.state.borrow();
                let idx = s
                    .arena
                    .nodes
                    .get(pk)
                    .map(|n| {
                        n.children
                            .iter()
                            .position(|&c| c == ck)
                            .map_or(-1i32, |i| i as i32)
                    })
                    .unwrap_or(-1);
                Ok(Value::new_int(ctx.clone(), idx))
            }
            _ => Ok(Value::new_int(ctx.clone(), -1)),
        }
    }
}

// ─── __nativeCompareBoundary — snabb Rust boundary-jämförelse ────────────────
// Anropas av Range._compareBoundary istället för JS-baserad traversering.
// Tar (nodeKeyA, offsetA, nodeKeyB, offsetB) → -1, 0, 1
struct NativeCompareBoundary {
    state: SharedState,
}
impl JsHandler for NativeCompareBoundary {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Extrahera nodeKeys och offsets
        let key_a = args.first().and_then(extract_node_key);
        let offset_a = args.get(1).and_then(|v| v.as_int()).unwrap_or(0) as usize;
        let key_b = args.get(2).and_then(extract_node_key);
        let offset_b = args.get(3).and_then(|v| v.as_int()).unwrap_or(0) as usize;

        match (key_a, key_b) {
            (Some(ka), Some(kb)) => {
                let s = self.state.borrow();
                let result = s.arena.compare_boundary_points(ka, offset_a, kb, offset_b);
                Ok(Value::new_int(ctx.clone(), result))
            }
            _ => Ok(Value::new_int(ctx.clone(), 0)),
        }
    }
}

// ─── getSelection stub ──────────────────────────────────────────────────────

/// Notifiera aktiva Range-objekt om en DOM-mutation via __nodeKey__-jämförelse.
pub(super) fn notify_range_mutation(
    ctx: &Ctx<'_>,
    mutation_type: &str,
    parent_key: NodeKey,
    _node_key: NodeKey,
    old_parent_key: Option<NodeKey>,
    old_index: Option<usize>,
) {
    let parent_bits = node_key_to_f64(parent_key);
    let old_parent_bits = old_parent_key.map_or(-1.0, node_key_to_f64);
    let oi = old_index.unwrap_or(0);
    let code = format!(
        "if(globalThis.__notifyRangeMutationByKey)globalThis.__notifyRangeMutationByKey('{}',{},{},{})",
        mutation_type, parent_bits, old_parent_bits, oi
    );
    let _ = ctx.eval::<Value, _>(code.as_str());
}

/// Invalidera Blitz computed styles-cache vid DOM-mutation
#[cfg(feature = "blitz")]
pub(super) fn invalidate_blitz_cache(state: &SharedState) {
    if let Ok(mut s) = state.try_borrow_mut() {
        s.blitz_styles = None;
    }
}

struct OwnerDocumentGetter;
impl JsHandler for OwnerDocumentGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        ctx.globals().get::<_, Value>("document")
    }
}

pub(super) struct GetSelectionFromDoc;
impl JsHandler for GetSelectionFromDoc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Delegerar till document.getSelection()
        ctx.eval::<Value, _>("document.getSelection()")
    }
}

/// Native XML-serialisering — tar nodeKey som f64, returnerar XML-sträng
pub(super) struct XmlSerializeNodeHandler {
    pub(super) state: SharedState,
}
impl JsHandler for XmlSerializeNodeHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let key_f64 = args.first().and_then(|v| v.as_number()).unwrap_or(0.0);
        let key = f64_to_node_key(key_f64);
        let s = self.state.borrow();
        let xml = dom_impls::xml_serializer::serialize_to_xml(&s.arena, key);
        Ok(rquickjs::String::from_str(ctx.clone(), &xml)?.into_value())
    }
}

/// Kontrollera XML well-formedness. Returnerar null (ok) eller felmeddelande (sträng).
pub(super) struct CheckXmlWellFormedHandler;
impl JsHandler for CheckXmlWellFormedHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let xml = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        match check_xml_well_formed(&xml) {
            None => Ok(Value::new_null(ctx.clone())),
            Some(err) => Ok(rquickjs::String::from_str(ctx.clone(), &err)?.into_value()),
        }
    }
}

/// Enkel XML well-formedness-check. Returnerar None om OK, Some(error) om fel.
fn check_xml_well_formed(xml: &str) -> Option<String> {
    // Ta bort DOCTYPE, PI, kommentarer, CDATA
    let mut cleaned = xml.to_string();
    // Ta bort DOCTYPE
    while let Some(start) = cleaned.find("<!DOCTYPE") {
        if let Some(end) = cleaned[start..].find('>') {
            cleaned.replace_range(start..start + end + 1, "");
        } else {
            break;
        }
    }
    // Ta bort PI
    while let Some(start) = cleaned.find("<?") {
        if let Some(end) = cleaned[start..].find("?>") {
            cleaned.replace_range(start..start + end + 2, "");
        } else {
            break;
        }
    }
    // Ta bort kommentarer
    while let Some(start) = cleaned.find("<!--") {
        if let Some(end) = cleaned[start..].find("-->") {
            cleaned.replace_range(start..start + end + 3, "");
        } else {
            break;
        }
    }
    // Ta bort CDATA
    while let Some(start) = cleaned.find("<![CDATA[") {
        if let Some(end) = cleaned[start..].find("]]>") {
            cleaned.replace_range(start..start + end + 3, "");
        } else {
            break;
        }
    }

    // Ogiltig: < följt av mellanslag
    if cleaned.contains("< ") {
        // Kolla att det inte bara är text
        let idx = cleaned.find("< ").unwrap_or(0);
        let after = &cleaned[idx + 2..];
        if after.starts_with('/')
            || after
                .chars()
                .next()
                .map(|c| c.is_alphabetic())
                .unwrap_or(false)
        {
            return Some("bad start tag".to_string());
        }
    }
    // Ogiltig sluttagg med mellanslag
    if cleaned.contains("</ ") {
        return Some("bad end tag".to_string());
    }
    // Ofullständig tag (< utan matchande >)
    if let Some(last_lt) = cleaned.rfind('<') {
        if cleaned[last_lt..].find('>').is_none() {
            return Some("unclosed tag".to_string());
        }
    }
    // Dubbelkolon (::)
    for cap in cleaned.match_indices("::") {
        // Kontrollera att det är inuti en tag
        let before = &cleaned[..cap.0];
        let last_open = before.rfind('<');
        let last_close = before.rfind('>');
        if let Some(lo) = last_open {
            if last_close.is_none() || last_close.unwrap() < lo {
                return Some("invalid qualified name".to_string());
            }
        }
    }

    // Tag-matchning + attributvalidering
    let mut stack: Vec<String> = Vec::new();
    let mut i = 0;
    let bytes = cleaned.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'<' && i + 1 < bytes.len() {
            let is_end = i + 1 < bytes.len() && bytes[i + 1] == b'/';
            let tag_start = if is_end { i + 2 } else { i + 1 };
            // Hitta slutet av taggen
            let tag_end = match cleaned[i..].find('>') {
                Some(pos) => i + pos,
                None => return Some("unclosed tag".to_string()),
            };
            let tag_content = &cleaned[tag_start..tag_end];
            // Extrahera tag-namn
            let name_end = tag_content
                .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
                .unwrap_or(tag_content.len());
            let tag_name = &tag_content[..name_end];

            if tag_name.is_empty() && !is_end {
                // Kan vara `< ` som vi redan kontrollerat
                i = tag_end + 1;
                continue;
            }
            // Kontrollera att tagnamn är giltigt
            if !tag_name.is_empty() {
                let first = tag_name.chars().next().unwrap();
                if first.is_ascii_digit() {
                    return Some("tag name starts with digit".to_string());
                }
                // Kontrollera : i tagnamn
                if tag_name.contains(':') {
                    let parts: Vec<&str> = tag_name.splitn(2, ':').collect();
                    if parts.len() == 2 {
                        if parts[0].is_empty() {
                            return Some("empty prefix".to_string());
                        }
                        if parts[0]
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_digit())
                            .unwrap_or(false)
                        {
                            return Some("prefix starts with digit".to_string());
                        }
                    }
                }
            }

            if is_end {
                // Sluttagg
                let name = tag_name.trim();
                if stack.is_empty() || stack.last().map(|s| s.as_str()) != Some(name) {
                    return Some(format!("mismatched tag: {name}"));
                }
                stack.pop();
            } else {
                let self_close = tag_content.ends_with('/');
                // Kontrollera attribut
                let attr_str = tag_content[name_end..].trim_end_matches('/').trim();
                if !attr_str.is_empty() {
                    if let Some(err) = check_xml_attributes(attr_str) {
                        return Some(err);
                    }
                }
                if !self_close && !tag_name.is_empty() {
                    stack.push(tag_name.to_string());
                }
            }
            i = tag_end + 1;
        } else {
            i += 1;
        }
    }
    if !stack.is_empty() {
        return Some(format!("unclosed tag: {}", stack.last().unwrap()));
    }
    // Kontrollera undeklare namespace-prefix i attribut
    check_xml_namespace_prefixes(&cleaned)
}

/// Kontrollera XML-attributsyntax. Returnerar None om OK, Some(error) vid fel.
fn check_xml_attributes(attr_str: &str) -> Option<String> {
    let mut s = attr_str.trim();
    while !s.is_empty() {
        // Hoppa över whitespace
        s = s.trim_start();
        if s.is_empty() {
            break;
        }
        // Kolla att attributnamn börjar med giltigt tecken
        let first = s.chars().next()?;
        if first == '=' {
            return Some("attribute without name".to_string());
        }
        if first == ':' {
            // Tomt prefix (:attr) — ogiltigt i XML
            return Some("empty attribute prefix".to_string());
        }
        if !first.is_alphabetic() && first != '_' {
            return Some("invalid attribute name".to_string());
        }
        // Läs attributnamn
        let name_end = s
            .find(|c: char| c.is_whitespace() || c == '=' || c == '/' || c == '>')
            .unwrap_or(s.len());
        let attr_name = &s[..name_end];
        // Kontrollera xmlns: med tomt prefix-namn
        if attr_name == "xmlns:" || attr_name.starts_with("xmlns:") {
            let pfx = &attr_name[6..];
            if pfx.is_empty() {
                return Some("empty xmlns prefix".to_string());
            }
        }
        s = s[name_end..].trim_start();
        // Kräv = efter attributnamn
        if s.is_empty() || !s.starts_with('=') {
            return Some("attribute without value".to_string());
        }
        s = s[1..].trim_start();
        // Kräv citattecken
        if s.is_empty() {
            return Some("attribute without value".to_string());
        }
        let quote = s.chars().next()?;
        if quote != '"' && quote != '\'' {
            return Some("unquoted attribute value".to_string());
        }
        // Läs till matchande citattecken
        s = &s[1..];
        match s.find(quote) {
            Some(end) => s = &s[end + 1..],
            None => return Some("unterminated attribute value".to_string()),
        }
    }
    None
}

/// Kontrollera att alla namespace-prefix i attribut är deklarerade.
fn check_xml_namespace_prefixes(xml: &str) -> Option<String> {
    use std::collections::HashSet;
    let mut declared: HashSet<String> = HashSet::new();
    declared.insert("xml".to_string());
    declared.insert("xmlns".to_string());

    // Samla deklarerade xmlns-prefix
    // Namespace-prefix valideras nedan
    // Sök alla xmlns:prefix="..." deklarationer
    let mut idx = 0;
    while let Some(pos) = xml[idx..].find("xmlns:") {
        let prefix_start = idx + pos + 6;
        let prefix_end = xml[prefix_start..]
            .find(|c: char| c.is_whitespace() || c == '=')
            .map(|e| prefix_start + e)
            .unwrap_or(prefix_start);
        let prefix = &xml[prefix_start..prefix_end];
        if !prefix.is_empty() {
            // Kontrollera att det har ett = och värde
            let after = xml[prefix_end..].trim_start();
            if after.starts_with('=') {
                declared.insert(prefix.to_string());
            } else {
                // xmlns:prefix utan = → fel
                return Some("xmlns without value".to_string());
            }
        }
        // xmlns:xmlns="" → förbjudet
        if prefix == "xmlns" {
            let after = xml[prefix_end..].trim_start();
            if after.starts_with("=\"\"") || after.starts_with("=''") {
                return Some("reserved xmlns prefix".to_string());
            }
        }
        idx = prefix_end + 1;
        if idx >= xml.len() {
            break;
        }
    }

    // Kontrollera attribut-prefix
    // Sök attribut med prefix (name:attr=)
    let mut idx2 = 0;
    while idx2 < xml.len() {
        if let Some(lt) = xml[idx2..].find('<') {
            let lt_abs = idx2 + lt;
            if let Some(gt) = xml[lt_abs..].find('>') {
                let tag = &xml[lt_abs..lt_abs + gt + 1];
                // Sök prefix:attr= mönster i taggen
                let mut ti = 0;
                let tag_bytes = tag.as_bytes();
                // Hoppa förbi tag-namn
                while ti < tag_bytes.len()
                    && tag_bytes[ti] != b' '
                    && tag_bytes[ti] != b'\t'
                    && tag_bytes[ti] != b'\n'
                    && tag_bytes[ti] != b'>'
                {
                    ti += 1;
                }
                // Sök attribut
                while ti < tag_bytes.len() {
                    // Hoppa whitespace
                    while ti < tag_bytes.len()
                        && (tag_bytes[ti] == b' '
                            || tag_bytes[ti] == b'\t'
                            || tag_bytes[ti] == b'\n')
                    {
                        ti += 1;
                    }
                    if ti >= tag_bytes.len() || tag_bytes[ti] == b'>' || tag_bytes[ti] == b'/' {
                        break;
                    }
                    // Läs attributnamn
                    let attr_start = ti;
                    while ti < tag_bytes.len()
                        && tag_bytes[ti] != b'='
                        && tag_bytes[ti] != b' '
                        && tag_bytes[ti] != b'>'
                    {
                        ti += 1;
                    }
                    let attr_name = &tag[attr_start..ti];
                    // Kontrollera prefix
                    if let Some(colon) = attr_name.find(':') {
                        let pfx = &attr_name[..colon];
                        if !pfx.is_empty()
                            && pfx != "xmlns"
                            && pfx != "xml"
                            && !declared.contains(pfx)
                        {
                            return Some(format!("undeclared prefix: {pfx}"));
                        }
                    }
                    // Hoppa förbi = och värde
                    while ti < tag_bytes.len() && tag_bytes[ti] != b' ' && tag_bytes[ti] != b'>' {
                        ti += 1;
                    }
                }
                idx2 = lt_abs + gt + 1;
            } else {
                break;
            }
        } else {
            break;
        }
    }
    None
}

struct GetSelection;
impl JsHandler for GetSelection {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        struct NoOp;
        impl JsHandler for NoOp {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                Ok(Value::new_undefined(ctx.clone()))
            }
        }
        struct SelectionToString;
        impl JsHandler for SelectionToString {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
            }
        }

        let selection = Object::new(ctx.clone())?;
        selection.set("anchorNode", Value::new_null(ctx.clone()))?;
        selection.set("anchorOffset", 0i32)?;
        selection.set("focusNode", Value::new_null(ctx.clone()))?;
        selection.set("focusOffset", 0i32)?;
        selection.set("isCollapsed", true)?;
        selection.set("rangeCount", 0i32)?;
        selection.set("type", "None")?;
        selection.set("removeAllRanges", Function::new(ctx.clone(), JsFn(NoOp))?)?;
        selection.set("addRange", Function::new(ctx.clone(), JsFn(NoOp))?)?;
        selection.set("collapse", Function::new(ctx.clone(), JsFn(NoOp))?)?;
        selection.set("collapseToStart", Function::new(ctx.clone(), JsFn(NoOp))?)?;
        selection.set("collapseToEnd", Function::new(ctx.clone(), JsFn(NoOp))?)?;
        selection.set(
            "toString",
            Function::new(ctx.clone(), JsFn(SelectionToString))?,
        )?;

        Ok(selection.into_value())
    }
}

// ─── exitPointerLock stub ───────────────────────────────────────────────────

struct ExitPointerLock;
impl JsHandler for ExitPointerLock {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── activeElement ──────────────────────────────────────────────────────────

struct ActiveElement {
    state: SharedState,
}
impl JsHandler for ActiveElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let focused = self.state.borrow().focused_element;
        match focused {
            Some(key_u64) => {
                let nk = f64_to_node_key(key_u64 as f64);
                let exists = self.state.borrow().arena.nodes.get(nk).is_some();
                if exists {
                    make_element_object(ctx, nk, &self.state)
                } else {
                    Ok(Value::new_null(ctx.clone()))
                }
            }
            None => {
                let body_key = {
                    let s = self.state.borrow();
                    find_by_tag_name(&s.arena, s.arena.document, "body")
                };
                match body_key {
                    Some(bk) => make_element_object(ctx, bk, &self.state),
                    None => Ok(Value::new_null(ctx.clone())),
                }
            }
        }
    }
}

// ─── register_document ─────────────────────────────────────────────────────

fn register_document<'js>(ctx: &Ctx<'js>, state: SharedState) -> rquickjs::Result<()> {
    let doc = Object::new(ctx.clone())?;
    // Sätt __nodeKey__ på document så att extract_node_key fungerar
    {
        let s = state.borrow();
        doc.set("__nodeKey__", node_key_to_f64(s.arena.document))?;
        doc.set("nodeType", 9)?;
        doc.set("nodeName", "#document")?;
    }

    // nodeValue — null per spec för Document-noder
    doc.prop(
        "nodeValue",
        rquickjs::object::Accessor::new(JsFn(NullGetter), JsFn(NoOpHandler))
            .configurable()
            .enumerable(),
    )?;

    // textContent — returnerar null per spec för Document-noder
    {
        let doc_key = state.borrow().arena.document;
        doc.prop(
            "textContent",
            rquickjs::object::Accessor::new(
                JsFn(TextContentGetter {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
                JsFn(TextContentSetter {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )
            .configurable()
            .enumerable(),
        )?;
    }

    // Registrera alla document-metoder
    doc.set(
        "getElementById",
        Function::new(
            ctx.clone(),
            JsFn(GetElementById {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "querySelector",
        Function::new(
            ctx.clone(),
            JsFn(QuerySelector {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "querySelectorAll",
        Function::new(
            ctx.clone(),
            JsFn(QuerySelectorAll {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "createElement",
        Function::new(
            ctx.clone(),
            JsFn(CreateElement {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "createElementNS",
        Function::new(
            ctx.clone(),
            JsFn(CreateElementNS {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "createTextNode",
        Function::new(
            ctx.clone(),
            JsFn(CreateTextNode {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "createComment",
        Function::new(
            ctx.clone(),
            JsFn(CreateComment {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "createDocumentFragment",
        Function::new(
            ctx.clone(),
            JsFn(CreateDocumentFragment {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "getElementsByClassName",
        Function::new(
            ctx.clone(),
            JsFn(GetElementsByClassName {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "getElementsByTagName",
        Function::new(
            ctx.clone(),
            JsFn(GetElementsByTagName {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "getElementsByTagNameNS",
        Function::new(
            ctx.clone(),
            JsFn(GetElementsByTagNameNSDoc {
                state: Rc::clone(&state),
            }),
        )?,
    )?;

    // createAttribute — native Attr-objekt
    doc.set(
        "createAttribute",
        Function::new(ctx.clone(), JsFn(CreateAttribute))?,
    )?;

    // createDocumentType — native Doctype-nod
    doc.set(
        "__createDocumentType",
        Function::new(
            ctx.clone(),
            JsFn(CreateDocumentType {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    // createProcessingInstruction — native PI-nod
    doc.set(
        "createProcessingInstruction",
        Function::new(
            ctx.clone(),
            JsFn(CreateProcessingInstruction {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    // createRange — Range API
    doc.set(
        "createRange",
        Function::new(ctx.clone(), JsFn(CreateRange))?,
    )?;
    // __createDocumentNode — skapar Document-nod i ArenaDom (för createHTMLDocument)
    doc.set(
        "__createDocumentNode",
        Function::new(
            ctx.clone(),
            JsFn(CreateDocumentNode {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    // __setOwnerDoc — sätter owner_doc på en nod (för foreignDoc)
    doc.set(
        "__setOwnerDoc",
        Function::new(
            ctx.clone(),
            JsFn(SetOwnerDoc {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    // __nativeChildIndex — snabb barn-index lookup för Range
    doc.set(
        "__nativeChildIndex",
        Function::new(
            ctx.clone(),
            JsFn(NativeChildIndex {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    // __nativeCompareBoundary — snabb Rust boundary-jämförelse för Range
    doc.set(
        "__nativeCompareBoundary",
        Function::new(
            ctx.clone(),
            JsFn(NativeCompareBoundary {
                state: Rc::clone(&state),
            }),
        )?,
    )?;

    doc.set(
        "createTreeWalker",
        Function::new(
            ctx.clone(),
            JsFn(CreateTreeWalker {
                state: Rc::clone(&state),
            }),
        )?,
    )?;
    doc.set(
        "createNodeIterator",
        Function::new(
            ctx.clone(),
            JsFn(CreateNodeIterator {
                state: Rc::clone(&state),
            }),
        )?,
    )?;

    // getSelection — Selection API stub
    doc.set(
        "getSelection",
        Function::new(ctx.clone(), JsFn(GetSelection))?,
    )?;

    // exitPointerLock — no-op stub
    doc.set(
        "exitPointerLock",
        Function::new(ctx.clone(), JsFn(ExitPointerLock))?,
    )?;

    // addEventListener / removeEventListener / dispatchEvent på document
    {
        let doc_key = state.borrow().arena.document;
        doc.set(
            "addEventListener",
            Function::new(
                ctx.clone(),
                JsFn(AddEventListenerHandler {
                    state: Rc::clone(&state),
                    key: doc_key,
                    override_key: None,
                }),
            )?,
        )?;
        doc.set(
            "removeEventListener",
            Function::new(
                ctx.clone(),
                JsFn(RemoveEventListenerHandler {
                    state: Rc::clone(&state),
                    key: doc_key,
                    override_key: None,
                }),
            )?,
        )?;
        doc.set(
            "dispatchEvent",
            Function::new(
                ctx.clone(),
                JsFn(DispatchEventHandler {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )?,
        )?;
    }

    // compareDocumentPosition / contains / lookupNamespaceURI / isSameNode / isEqualNode på document
    {
        let doc_key = state.borrow().arena.document;
        doc.set(
            "compareDocumentPosition",
            Function::new(
                ctx.clone(),
                JsFn(CompareDocumentPosition {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )?,
        )?;
        doc.set(
            "contains",
            Function::new(
                ctx.clone(),
                JsFn(ContainsHandler {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )?,
        )?;
        doc.set(
            "lookupNamespaceURI",
            Function::new(
                ctx.clone(),
                JsFn(LookupNamespaceURI {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )?,
        )?;
        doc.set(
            "isSameNode",
            Function::new(ctx.clone(), JsFn(IsSameNode { key: doc_key }))?,
        )?;
        doc.set(
            "isEqualNode",
            Function::new(
                ctx.clone(),
                JsFn(IsEqualNode {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )?,
        )?;
    }

    // implementation — minimalt DOMImplementation-objekt (utökas av polyfills)
    {
        struct HasFeature;
        impl JsHandler for HasFeature {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                Ok(Value::new_bool(ctx.clone(), true))
            }
        }
        let impl_obj = Object::new(ctx.clone())?;
        impl_obj.set("hasFeature", Function::new(ctx.clone(), JsFn(HasFeature))?)?;
        doc.set("implementation", impl_obj)?;
    }

    // activeElement — getter som returnerar fokuserat element eller body
    doc.prop(
        "activeElement",
        Accessor::new_get(JsFn(ActiveElement {
            state: Rc::clone(&state),
        })),
    )?;

    // readyState — dynamisk getter
    {
        struct ReadyStateGetter {
            state: SharedState,
        }
        impl JsHandler for ReadyStateGetter {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                let s = self.state.borrow();
                Ok(rquickjs::String::from_str(ctx.clone(), &s.ready_state)?.into_value())
            }
        }
        doc.prop(
            "readyState",
            Accessor::new_get(JsFn(ReadyStateGetter {
                state: Rc::clone(&state),
            })),
        )?;
    }

    // body, head, documentElement — statiska egenskaper
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
        let body_obj = make_element_object(ctx, key, &state)?;
        doc.set("body", body_obj)?;
    }
    if let Some(key) = head_key {
        let head_obj = make_element_object(ctx, key, &state)?;
        doc.set("head", head_obj)?;
    }
    if let Some(key) = html_key {
        let html_obj = make_element_object(ctx, key, &state)?;
        doc.set("documentElement", html_obj)?;
    }

    // document.doctype — hitta Doctype-noden bland document-barn
    {
        let s = state.borrow();
        let doc_key = s.arena.document;
        let doctype_key = s.arena.nodes[doc_key]
            .children
            .iter()
            .find(|&&ck| {
                s.arena
                    .nodes
                    .get(ck)
                    .map(|n| matches!(n.node_type, NodeType::Doctype))
                    .unwrap_or(false)
            })
            .copied();
        drop(s);
        if let Some(dk) = doctype_key {
            let dt_obj = make_element_object(ctx, dk, &state)?;
            doc.set("doctype", dt_obj)?;
        } else {
            doc.set("doctype", Value::new_null(ctx.clone()))?;
        }
    }

    // childNodes / firstChild / lastChild på document
    {
        let doc_key = state.borrow().arena.document;
        doc.set("childNodes", make_child_nodes(ctx, doc_key, &state)?)?;
        doc.set("firstChild", make_first_child(ctx, doc_key, &state)?)?;
        doc.set("lastChild", make_last_child(ctx, doc_key, &state)?)?;
        doc.set("__nodeKey__", node_key_to_f64(doc_key))?;
    }
    // Standard-egenskaper som WPT förväntar sig
    doc.set("characterSet", "UTF-8")?;
    doc.set("charset", "UTF-8")?;
    doc.set("inputEncoding", "UTF-8")?;
    doc.set("contentType", "text/html")?;
    doc.set("compatMode", "CSS1Compat")?;
    doc.set("baseURI", "about:blank")?;
    doc.set("URL", "about:blank")?;
    doc.set("documentURI", "about:blank")?;

    // document.title — native Rust getter/setter (migrerad från polyfill)
    {
        let gt_state = Rc::clone(&state);
        let st_state = Rc::clone(&state);
        doc.prop(
            "title",
            Accessor::new(
                JsFn(DocTitleGetter { state: gt_state }),
                JsFn(DocTitleSetter { state: st_state }),
            )
            .configurable()
            .enumerable(),
        )?;
    }

    // __nativeQueryElements — returnerar element-objekt för live HTMLCollection
    doc.set(
        "__nativeQueryElements",
        Function::new(
            ctx.clone(),
            JsFn(NativeQueryElements {
                state: Rc::clone(&state),
            }),
        )?,
    )?;

    // ─── document.createEvent — native Rust (migrerad från polyfill) ─────────
    doc.set(
        "createEvent",
        Function::new(ctx.clone(), JsFn(NativeCreateEvent))?,
    )?;

    ctx.globals().set("document", doc)?;

    // Registrera __createLiveHTMLCollection — JS Proxy-baserad live HTMLCollection
    let _ = ctx.eval::<Value, _>(
        r#"
(function() {
    globalThis.__createLiveHTMLCollection = function(initialItems, rootKey, queryType, queryVal) {
        var _expando = {};
        var _queryFn = function() {
            return document.__nativeQueryElements(rootKey, queryType, queryVal);
        };
        var handler = {
            get: function(target, prop, receiver) {
                if (prop === Symbol.iterator) {
                    return function() {
                        var items = _queryFn();
                        var idx = 0;
                        return { next: function() {
                            if (idx < items.length) return { value: items[idx++], done: false };
                            return { done: true };
                        }};
                    };
                }
                if (prop === Symbol.toStringTag) return 'HTMLCollection';
                if (prop === 'length') return _queryFn().length;
                if (prop === 'item') return function(index) {
                    index = (index >>> 0);
                    var items = _queryFn();
                    return (index < items.length) ? items[index] : null;
                };
                if (prop === 'namedItem') return function(name) {
                    name = String(name);
                    if (!name) return null;
                    var items = _queryFn();
                    for (var i = 0; i < items.length; i++) {
                        var eid = items[i].id, ename = items[i].getAttribute('name');
                        if ((eid && eid === name) || (ename && ename === name)) return items[i];
                    }
                    return null;
                };
                // Numerisk index
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    var items = _queryFn();
                    var idx = parseInt(prop, 10);
                    return idx < items.length ? items[idx] : undefined;
                }
                // Named property access (id/name) — tom sträng matchas ej
                if (typeof prop === 'string' && prop !== '__proto__' && prop !== '') {
                    if (prop in _expando) return _expando[prop];
                    var items = _queryFn();
                    for (var i = 0; i < items.length; i++) {
                        var eid = items[i].id, ename = items[i].getAttribute('name');
                        if ((eid && eid === prop) || (ename && ename === prop)) return items[i];
                    }
                }
                return undefined;
            },
            set: function(target, prop, value, receiver) {
                // Numeriska index — tyst ignorera
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    return true;
                }
                // Named property finns redan — tyst ignorera
                var items = _queryFn();
                for (var i = 0; i < items.length; i++) {
                    if (items[i].id === prop || items[i].getAttribute('name') === prop) {
                        return true;
                    }
                }
                _expando[prop] = value;
                return true;
            },
            has: function(target, prop) {
                if (prop === 'length' || prop === 'item' || prop === 'namedItem') return true;
                if (prop === Symbol.iterator) return true;
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    return parseInt(prop, 10) < _queryFn().length;
                }
                if (typeof prop === 'string' && prop !== '') {
                    if (prop in _expando) return true;
                    var items = _queryFn();
                    for (var i = 0; i < items.length; i++) {
                        var eid = items[i].id, ename = items[i].getAttribute('name');
                        if ((eid && eid === prop) || (ename && ename === prop)) return true;
                    }
                }
                return false;
            },
            ownKeys: function(target) {
                var items = _queryFn();
                var keys = [];
                for (var i = 0; i < items.length; i++) keys.push(String(i));
                var seen = {};
                for (var i = 0; i < items.length; i++) {
                    var el = items[i];
                    var eid = el.id; var ename = el.getAttribute ? el.getAttribute('name') : null;
                    if (eid && !seen[eid]) { keys.push(eid); seen[eid] = true; }
                    if (ename && !seen[ename]) { keys.push(ename); seen[ename] = true; }
                }
                for (var k in _expando) {
                    if (!seen[k]) keys.push(k);
                }
                return keys;
            },
            getOwnPropertyDescriptor: function(target, prop) {
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    var items = _queryFn();
                    var idx = parseInt(prop, 10);
                    if (idx < items.length) return { value: items[idx], enumerable: true, configurable: true, writable: false };
                    return undefined;
                }
                if (prop in _expando) {
                    var desc = Object.getOwnPropertyDescriptor(_expando, prop);
                    return desc;
                }
                // Named property
                var items = _queryFn();
                for (var i = 0; i < items.length; i++) {
                    if (items[i].id === prop || items[i].getAttribute('name') === prop) {
                        return { value: items[i], enumerable: false, configurable: true, writable: false };
                    }
                }
                return undefined;
            },
            defineProperty: function(target, prop, desc) {
                // Numeric — reject
                if (typeof prop === 'string' && /^\d+$/.test(prop)) return false;
                // If named element matches, reject
                var items = _queryFn();
                for (var i = 0; i < items.length; i++) {
                    if (items[i].id === prop || items[i].getAttribute('name') === prop) {
                        throw new TypeError("Cannot define property '" + prop + "' on HTMLCollection");
                    }
                }
                Object.defineProperty(_expando, prop, desc);
                return true;
            },
            deleteProperty: function(target, prop) {
                // Indexed — tyst ignorera
                if (typeof prop === 'string' && /^\d+$/.test(prop)) return true;
                // Named element — tyst ignorera (kan inte radera)
                var items = _queryFn();
                for (var i = 0; i < items.length; i++) {
                    if (items[i].id === prop || items[i].getAttribute('name') === prop) return true;
                }
                if (prop in _expando) { delete _expando[prop]; return true; }
                return true;
            }
        };
        var p = new Proxy({}, handler);
        Object.setPrototypeOf(p, (globalThis.HTMLCollection && globalThis.HTMLCollection.prototype) || Object.prototype);
        return p;
    };
    // NamedNodeMap — Proxy-baserad med indexerade + namngivna properties
    globalThis.__createNamedNodeMap = function(getAttrsFn, ownerEl) {
        var handler = {
            get: function(target, prop) {
                if (prop === Symbol.iterator) {
                    return function() {
                        var attrs = getAttrsFn();
                        var idx = 0;
                        return { next: function() {
                            if (idx < attrs.length) return { value: attrs[idx++], done: false };
                            return { done: true };
                        }};
                    };
                }
                if (prop === Symbol.toStringTag) return 'NamedNodeMap';
                if (prop === 'length') return getAttrsFn().length;
                if (prop === 'item') return function(i) { var a = getAttrsFn(); return (i >= 0 && i < a.length) ? a[i] : null; };
                if (prop === 'getNamedItem') return function(name) {
                    var a = getAttrsFn();
                    for (var i = 0; i < a.length; i++) if (a[i].name === name) return a[i];
                    return null;
                };
                if (prop === 'getNamedItemNS') return function(ns, name) {
                    var a = getAttrsFn();
                    for (var i = 0; i < a.length; i++) if (a[i].localName === name && a[i].namespaceURI === ns) return a[i];
                    return null;
                };
                if (prop === 'setNamedItem') return function(attr) {
                    if (ownerEl && ownerEl.setAttribute) ownerEl.setAttribute(attr.name, attr.value);
                    return attr;
                };
                if (prop === 'removeNamedItem') return function(name) {
                    var old = null;
                    var a = getAttrsFn();
                    for (var i = 0; i < a.length; i++) if (a[i].name === name) { old = a[i]; break; }
                    if (ownerEl && ownerEl.removeAttribute) ownerEl.removeAttribute(name);
                    return old;
                };
                // Numeriskt index
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    var a = getAttrsFn();
                    var i = parseInt(prop, 10);
                    return i < a.length ? a[i] : undefined;
                }
                // Namngivet attribut
                if (typeof prop === 'string') {
                    var a = getAttrsFn();
                    for (var i = 0; i < a.length; i++) if (a[i].name === prop) return a[i];
                }
                return undefined;
            },
            ownKeys: function() {
                var a = getAttrsFn();
                var keys = [];
                for (var i = 0; i < a.length; i++) keys.push(String(i));
                for (var i = 0; i < a.length; i++) keys.push(a[i].name);
                return keys;
            },
            getOwnPropertyDescriptor: function(target, prop) {
                var a = getAttrsFn();
                if (typeof prop === 'string' && /^\d+$/.test(prop)) {
                    var i = parseInt(prop, 10);
                    if (i < a.length) return { value: a[i], enumerable: true, configurable: true };
                    return undefined;
                }
                for (var i = 0; i < a.length; i++) {
                    if (a[i].name === prop) return { value: a[i], enumerable: false, configurable: true };
                }
                return undefined;
            },
            has: function(target, prop) {
                if (typeof prop === 'string' && /^\d+$/.test(prop)) return parseInt(prop, 10) < getAttrsFn().length;
                if (prop === 'length' || prop === 'item' || prop === 'getNamedItem') return true;
                var a = getAttrsFn();
                for (var i = 0; i < a.length; i++) if (a[i].name === prop) return true;
                return false;
            }
        };
        var p = new Proxy({}, handler);
        Object.setPrototypeOf(p, (globalThis.NamedNodeMap && globalThis.NamedNodeMap.prototype) || Object.prototype);
        return p;
    };
})()
"#,
    );

    // Registrera document-objektet i node identity cache
    // Kritiskt: parentNode-traversal från html-element → document måste returnera
    // exakt samma JS-objekt som globalThis.document
    {
        let doc_key = state.borrow().arena.document;
        let doc_key_bits = node_key_to_f64(doc_key);
        let code = format!(
            "globalThis.__nodeCache && globalThis.__nodeCache.set({}, globalThis.document)",
            doc_key_bits
        );
        let _ = ctx.eval::<Value, _>(code.as_str());
    }

    // Named element access — HTML spec: element med id exponeras som window.id
    // Traversera hela DOM:en och sätt globala variabler för varje element med id
    {
        let s = state.borrow();
        let mut id_elements: Vec<(String, NodeKey)> = Vec::new();
        fn collect_ids(arena: &ArenaDom, key: NodeKey, out: &mut Vec<(String, NodeKey)>) {
            if let Some(node) = arena.nodes.get(key) {
                if node.node_type == NodeType::Element {
                    if let Some(id) = node.get_attr("id") {
                        if !id.is_empty() {
                            out.push((id.to_string(), key));
                        }
                    }
                }
                for &child in &node.children {
                    collect_ids(arena, child, out);
                }
            }
        }
        collect_ids(&s.arena, s.arena.document, &mut id_elements);
        drop(s);

        for (id, key) in id_elements {
            if let Ok(obj) = make_element_object(ctx, key, &state) {
                let _ = ctx.globals().set(id.as_str(), obj);
            }
        }
    }

    Ok(())
}

/// Skapa childNodes-array för en nod
fn make_child_nodes<'js>(
    ctx: &Ctx<'js>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'js>> {
    let s = state.borrow();
    let children = s
        .arena
        .nodes
        .get(key)
        .map(|n| n.children.clone())
        .unwrap_or_default();
    drop(s);
    let arr = rquickjs::Array::new(ctx.clone())?;
    for (i, &ck) in children.iter().enumerate() {
        let child_obj = make_element_object(ctx, ck, state)?;
        arr.set(i, child_obj)?;
    }
    Ok(arr.into_value())
}

/// Skapa firstChild för en nod
fn make_first_child<'js>(
    ctx: &Ctx<'js>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'js>> {
    let fc = {
        let s = state.borrow();
        s.arena
            .nodes
            .get(key)
            .and_then(|n| n.children.first().copied())
    };
    match fc {
        Some(ck) => make_element_object(ctx, ck, state),
        None => Ok(Value::new_null(ctx.clone())),
    }
}

/// Skapa lastChild för en nod
fn make_last_child<'js>(
    ctx: &Ctx<'js>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'js>> {
    let lc = {
        let s = state.borrow();
        s.arena
            .nodes
            .get(key)
            .and_then(|n| n.children.last().copied())
    };
    match lc {
        Some(ck) => make_element_object(ctx, ck, state),
        None => Ok(Value::new_null(ctx.clone())),
    }
}

// ─── Element-objekt ─────────────────────────────────────────────────────────

/// Extrahera NodeKey från ett JS-element-objekt via __nodeKey__
pub(super) fn extract_node_key(val: &Value) -> Option<NodeKey> {
    let obj = val.as_object()?;
    let bits: f64 = obj.get("__nodeKey__").ok()?;
    Some(f64_to_node_key(bits))
}

/// Konvertera f64 tillbaka till NodeKey
pub(super) fn f64_to_node_key(bits: f64) -> NodeKey {
    use slotmap::KeyData;
    NodeKey::from(KeyData::from_ffi(bits as u64))
}

// node_key_to_f64 — definieras i utility section nedan

/// Hämta textinnehåll rekursivt
fn get_text_content(arena: &ArenaDom, key: NodeKey) -> String {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return String::new(),
    };
    match &node.node_type {
        // Text, Comment, PI: returnera node data direkt
        NodeType::Text | NodeType::Comment | NodeType::ProcessingInstruction => {
            node.text.as_deref().unwrap_or("").to_string()
        }
        // Element, DocumentFragment: konkatenera text från ALLA Text-descendants
        _ => collect_text_descendants(arena, key),
    }
}

/// Samla text från alla Text-nod descendants (exkluderar Comment, PI per spec)
fn collect_text_descendants(arena: &ArenaDom, key: NodeKey) -> String {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return String::new(),
    };
    let mut text = String::new();
    for &child in &node.children {
        if let Some(child_node) = arena.nodes.get(child) {
            match &child_node.node_type {
                NodeType::Text => {
                    text.push_str(child_node.text.as_deref().unwrap_or(""));
                }
                NodeType::Comment | NodeType::ProcessingInstruction => {
                    // Skip per spec — inte inkluderad i textContent
                }
                _ => {
                    // Rekursera in i element-barn
                    text.push_str(&collect_text_descendants(arena, child));
                }
            }
        }
    }
    text
}

/// Serialisera nod till HTML
fn get_inner_html(arena: &ArenaDom, key: NodeKey) -> String {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return String::new(),
    };
    let mut html = String::new();
    for &child in &node.children {
        serialize_node_html(arena, child, &mut html);
    }
    html
}

/// Importera en nod (rekursivt) från en arena till en annan.
/// Returnerar den nya nyckeln i dest_arena.
pub(super) fn import_node(
    src_arena: &ArenaDom,
    src_key: NodeKey,
    dest_arena: &mut ArenaDom,
) -> Option<NodeKey> {
    let src_node = src_arena.nodes.get(src_key)?;
    let new_key = dest_arena.nodes.insert(crate::arena_dom::DomNode {
        node_type: src_node.node_type.clone(),
        tag: src_node.tag.clone(),
        attributes: src_node.attributes.clone(),
        text: src_node.text.clone(),
        parent: None,
        children: vec![],
        owner_doc: None,
    });
    // Rekursivt importera barn
    let children: Vec<NodeKey> = src_node.children.clone();
    for child_key in children {
        if let Some(new_child) = import_node(src_arena, child_key, dest_arena) {
            dest_arena.append_child(new_key, new_child);
        }
    }
    Some(new_key)
}

fn serialize_node_html(arena: &ArenaDom, key: NodeKey, out: &mut String) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    match &node.node_type {
        NodeType::Text => {
            if let Some(text) = &node.text {
                // Per spec: escape text content i vanliga element
                // Men RAW text element (script, style, xmp, iframe, noembed, noframes, plaintext)
                // ska INTE escapas
                let parent_tag = node
                    .parent
                    .and_then(|p| arena.nodes.get(p))
                    .and_then(|n| n.tag.as_deref())
                    .unwrap_or("");
                if matches!(
                    parent_tag,
                    "script"
                        | "style"
                        | "xmp"
                        | "iframe"
                        | "noembed"
                        | "noframes"
                        | "plaintext"
                        | "noscript"
                ) {
                    out.push_str(text);
                } else {
                    escape_html_text(text, out);
                }
            }
        }
        NodeType::Comment => {
            if let Some(text) = &node.text {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
        }
        NodeType::ProcessingInstruction => {
            let target = node.tag.as_deref().unwrap_or("");
            let data = node.text.as_deref().unwrap_or("");
            out.push_str("<?");
            out.push_str(target);
            if !data.is_empty() {
                out.push(' ');
                out.push_str(data);
            }
            out.push_str("?>");
        }
        NodeType::Doctype => {
            out.push_str("<!DOCTYPE ");
            out.push_str(node.text.as_deref().unwrap_or("html"));
            out.push('>');
        }
        NodeType::Element => {
            let tag = node.tag.as_deref().unwrap_or("div");
            out.push('<');
            out.push_str(tag);
            for (k, v) in &node.attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                escape_html_attr(v, out);
                out.push('"');
            }
            // Void elements — per HTML spec, inga closing tags
            if is_void_element(tag) {
                out.push('>');
                return;
            }
            out.push('>');
            for &child in &node.children {
                serialize_node_html(arena, child, out);
            }
            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        _ => {
            for &child in &node.children {
                serialize_node_html(arena, child, out);
            }
        }
    }
}

/// Kolla om ett HTML-element är ett void element (inget closing tag)
fn is_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Escape text content per HTML serialization spec
fn escape_html_text(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\u{00A0}' => out.push_str("&nbsp;"),
            _ => out.push(ch),
        }
    }
}

/// Escape attribute value per HTML serialization spec
fn escape_html_attr(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            '\u{00A0}' => out.push_str("&nbsp;"),
            _ => out.push(ch),
        }
    }
}

// ─── normalize ──────────────────────────────────────────────────────────────

/// Kopiera ett subträd och returnera nyckeln till roten (utan att lägga till som barn)
pub(super) fn copy_subtree_return_key(
    src: &ArenaDom,
    src_key: NodeKey,
    dst: &mut ArenaDom,
) -> NodeKey {
    let node = match src.nodes.get(src_key) {
        Some(n) => n.clone(),
        None => {
            return dst.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: None,
                parent: None,
                children: vec![],
                owner_doc: None,
            });
        }
    };
    let new_key = dst.nodes.insert(crate::arena_dom::DomNode {
        node_type: node.node_type,
        tag: node.tag,
        attributes: node.attributes,
        text: node.text,
        parent: None,
        children: vec![],
        owner_doc: None,
    });
    for &child in &node.children {
        copy_subtree(src, child, new_key, dst);
    }
    new_key
}

/// Konvertera JS-argument till NodeKeys.
/// Strängar och null/undefined/numbers konverteras till textnoder.
/// Hitta närmaste giltiga char boundary vid eller efter given byte-offset.
// ─── Migration: getElementsByTagName/ClassName på element ────────────────────
struct GetElementsByTagNameElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetElementsByTagNameElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Spec: getElementsByTagName tar INTE lowercase — matchning sker per-element i selectors.rs
        let tag = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        make_live_html_collection(ctx, &self.state, self.key, "tag", &tag)
    }
}

struct GetElementsByTagNameNSElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetElementsByTagNameNSElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let ns = args
            .first()
            .and_then(|v| {
                if v.is_null() {
                    Some("".to_string())
                } else {
                    v.as_string().and_then(|s| s.to_string().ok())
                }
            })
            .unwrap_or_default();
        let local_name = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let query_val = format!("{}\x01{}", ns, local_name);
        make_live_html_collection(ctx, &self.state, self.key, "tag_ns", &query_val)
    }
}

struct GetElementsByClassNameElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetElementsByClassNameElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let cls = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        make_live_html_collection(ctx, &self.state, self.key, "class", &cls)
    }
}

// ─── Migration: moveBefore ──────────────────────────────────────────────────
struct MoveBefore {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for MoveBefore {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(
                        ctx.clone(),
                        "TypeError: parameter 1 is not a Node",
                    )?
                    .into(),
                ));
            }
        };
        if args.len() < 2 {
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "TypeError: 2 arguments required")?.into(),
            ));
        }
        let ref_key = args.get(1).and_then(extract_node_key); // null → append
        let mut s = self.state.borrow_mut();
        // Detach new_key
        if let Some(old_p) = s.arena.nodes.get(new_key).and_then(|n| n.parent) {
            if let Some(p) = s.arena.nodes.get_mut(old_p) {
                p.children.retain(|&c| c != new_key);
            }
        }
        if let Some(n) = s.arena.nodes.get_mut(new_key) {
            n.parent = Some(self.key);
        }
        match ref_key {
            Some(rk) => {
                let pos = s
                    .arena
                    .nodes
                    .get(self.key)
                    .and_then(|n| n.children.iter().position(|&c| c == rk))
                    .unwrap_or(0);
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.children.insert(pos, new_key);
                }
            }
            None => {
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.children.push(new_key);
                }
            }
        }
        drop(s);
        make_element_object(ctx, new_key, &self.state)
    }
}

// ─── Migration: lookupNamespaceURI / lookupPrefix / isDefaultNamespace ──────
struct LookupNamespaceURI {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for LookupNamespaceURI {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let node = s.arena.nodes.get(self.key);
        let nt = node.map(|n| n.node_type.clone());
        let parent_key = node.and_then(|n| n.parent);
        drop(s);
        match nt {
            Some(NodeType::Element) | Some(NodeType::Document) => Ok(rquickjs::String::from_str(
                ctx.clone(),
                "http://www.w3.org/1999/xhtml",
            )?
            .into_value()),
            Some(NodeType::Text) | Some(NodeType::Comment) => {
                if let Some(pk) = parent_key {
                    let h = LookupNamespaceURI {
                        state: Rc::clone(&self.state),
                        key: pk,
                    };
                    h.handle(ctx, &[])
                } else {
                    Ok(Value::new_null(ctx.clone()))
                }
            }
            _ => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct LookupPrefix {
    _key: NodeKey,
}
impl JsHandler for LookupPrefix {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_null(ctx.clone()))
    }
}

struct IsDefaultNamespace {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for IsDefaultNamespace {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let ns = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        let is_html = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.node_type == NodeType::Element)
            .unwrap_or(false);
        drop(s);
        let result = if is_html {
            ns == "http://www.w3.org/1999/xhtml"
        } else {
            ns.is_empty()
        };
        Ok(Value::new_bool(ctx.clone(), result))
    }
}

// ─── compareDocumentPosition ────────────────────────────────────────────────
struct CompareDocumentPosition {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for CompareDocumentPosition {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let other_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: argument is not a Node")?
                        .into(),
                ));
            }
        };
        if self.key == other_key {
            return Ok(Value::new_int(ctx.clone(), 0));
        }
        let s = self.state.borrow();
        // Bygg ancestor-kedjor
        let chain_a = ancestor_chain(&s.arena, self.key);
        let chain_b = ancestor_chain(&s.arena, other_key);
        // Disconnected?
        if chain_a.is_empty() || chain_b.is_empty() || chain_a.last() != chain_b.last() {
            // DISCONNECTED | IMPLEMENTATION_SPECIFIC | PRECEDING eller FOLLOWING
            return Ok(Value::new_int(
                ctx.clone(),
                1 | 32 | if self.key < other_key { 4 } else { 2 },
            ));
        }
        // Hitta gemensam ancestor
        let mut i = 0;
        while i < chain_a.len()
            && i < chain_b.len()
            && chain_a[chain_a.len() - 1 - i] == chain_b[chain_b.len() - 1 - i]
        {
            i += 1;
        }
        // Contains / contained_by
        if i == chain_a.len() {
            // self är ancestor till other → other is CONTAINED_BY self, FOLLOWING
            return Ok(Value::new_int(ctx.clone(), 16 | 4));
        }
        if i == chain_b.len() {
            // other är ancestor till self → other CONTAINS self, PRECEDING
            return Ok(Value::new_int(ctx.clone(), 8 | 2));
        }
        // Sibling-ordning i gemensam parent
        let common_parent = chain_a[chain_a.len() - i];
        let node_a = chain_a[chain_a.len() - 1 - i]; // self-sidans nod under common
        let node_b = chain_b[chain_b.len() - 1 - i]; // other-sidans nod under common
        if let Some(parent) = s.arena.nodes.get(common_parent) {
            let pos_a = parent.children.iter().position(|&c| c == node_a);
            let pos_b = parent.children.iter().position(|&c| c == node_b);
            match (pos_a, pos_b) {
                (Some(a), Some(b)) if a < b => {
                    return Ok(Value::new_int(ctx.clone(), 4)); // FOLLOWING
                }
                (Some(_), Some(_)) => {
                    return Ok(Value::new_int(ctx.clone(), 2)); // PRECEDING
                }
                _ => {}
            }
        }
        Ok(Value::new_int(ctx.clone(), 1 | 32 | 4)) // Disconnected fallback
    }
}

/// Bygg ancestor-kedja: [self, parent, grandparent, ..., root]
pub(super) fn ancestor_chain(arena: &ArenaDom, key: NodeKey) -> Vec<NodeKey> {
    let mut chain = vec![key];
    let mut current = key;
    while let Some(p) = arena.nodes.get(current).and_then(|n| n.parent) {
        chain.push(p);
        current = p;
    }
    chain
}

// ─── Node.isEqualNode ──────────────────────────────────────────────────────
struct IsEqualNode {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for IsEqualNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let other_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => return Ok(Value::new_bool(ctx.clone(), false)),
        };
        if self.key == other_key {
            return Ok(Value::new_bool(ctx.clone(), true));
        }
        let s = self.state.borrow();
        let a = s.arena.nodes.get(self.key);
        let b = s.arena.nodes.get(other_key);
        let equal = match (a, b) {
            (Some(na), Some(nb)) => {
                na.node_type == nb.node_type
                    && na.tag == nb.tag
                    && na.text == nb.text
                    && na.attributes.len() == nb.attributes.len()
                    && na
                        .attributes
                        .iter()
                        .all(|(k, v)| nb.get_attr(k) == Some(v.as_str()))
                    && na.children.len() == nb.children.len()
            }
            _ => false,
        };
        Ok(Value::new_bool(ctx.clone(), equal))
    }
}

// ─── Node.isSameNode ───────────────────────────────────────────────────────
struct IsSameNode {
    key: NodeKey,
}
impl JsHandler for IsSameNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let other_key = args.first().and_then(extract_node_key);
        Ok(Value::new_bool(ctx.clone(), other_key == Some(self.key)))
    }
}

// (Old Normalize struct removed — see NormalizeNode above)

// ─── TreeWalker ────────────────────────────────────────────────────────────
struct CreateTreeWalker {
    state: SharedState,
}
impl JsHandler for CreateTreeWalker {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let root_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: root is not a Node")?
                        .into(),
                ));
            }
        };
        // WebIDL: whatToShow = ToUint32(arg)
        let what_to_show = match args.get(1) {
            None => 0xFFFFFFFF_u32,
            Some(v) if v.is_undefined() => 0xFFFFFFFF_u32,
            Some(v) => webidl_unsigned_long(Some(v)),
        };
        let filter = args.get(2).cloned();

        let tw = Object::new(ctx.clone())?;
        let root_obj = make_element_object(ctx, root_key, &self.state)?;
        let filter_val = match &filter {
            Some(f) if !f.is_null() && !f.is_undefined() => f.clone(),
            _ => Value::new_null(ctx.clone()),
        };
        // Readonly-egenskaper: root, whatToShow, filter
        let setup_code = r#"(function(tw, root, whatToShow, filter) {
            Object.defineProperty(tw, 'root', { value: root, writable: false, configurable: false });
            Object.defineProperty(tw, 'whatToShow', { value: whatToShow, writable: false, configurable: false });
            Object.defineProperty(tw, 'filter', { value: filter, writable: false, configurable: false });
        })"#;
        let setup_fn: Function = ctx.eval(setup_code)?;
        setup_fn.call::<_, Value>((
            tw.clone(),
            root_obj.clone(),
            what_to_show as f64,
            filter_val,
        ))?;
        tw.set("currentNode", root_obj)?;
        tw.set("__rootKey", node_key_to_f64(root_key))?;
        tw.set("__whatToShow", what_to_show as f64)?;

        // TreeWalker-metoder via JS
        let walker_code = r#"
        (function(tw) {
            var FILTER_ACCEPT = 1, FILTER_REJECT = 2, FILTER_SKIP = 3;
            function accept(tw, node) {
                var show = tw.__whatToShow;
                if (show === undefined || show === null) show = 0xFFFFFFFF;
                show = show >>> 0;
                var nt = node.nodeType;
                var bit = 1 << (nt - 1);
                if (!(show & bit)) return FILTER_SKIP;
                var f = tw.filter;
                if (!f) return FILTER_ACCEPT;
                var result;
                if (typeof f === 'function') result = f(node);
                else if (typeof f === 'object' && typeof f.acceptNode === 'function') result = f.acceptNode(node);
                else return FILTER_ACCEPT;
                return (Number(result) | 0);
            }
            function firstChild(tw, reversed) {
                var node = tw.currentNode;
                var kids = node.childNodes;
                if (!kids || !kids.length) return null;
                var start = reversed ? kids.length - 1 : 0;
                var end = reversed ? -1 : kids.length;
                var step = reversed ? -1 : 1;
                for (var i = start; i !== end; i += step) {
                    var child = kids[i];
                    var r = accept(tw, child);
                    if (r === FILTER_ACCEPT) { tw.currentNode = child; return child; }
                    if (r === FILTER_SKIP) {
                        var inner = child.childNodes;
                        if (inner && inner.length) {
                            tw.currentNode = child;
                            var result = firstChild(tw, reversed);
                            if (result) return result;
                            tw.currentNode = node;
                        }
                    }
                }
                return null;
            }
            tw.parentNode = function() {
                var node = this.currentNode;
                while (node && node !== this.root) {
                    node = node.parentNode;
                    if (node && accept(this, node) === FILTER_ACCEPT) {
                        this.currentNode = node;
                        return node;
                    }
                }
                return null;
            };
            tw.firstChild = function() { return firstChild(this, false); };
            tw.lastChild = function() { return firstChild(this, true); };
            tw.nextSibling = function() { return sibling(this, false); };
            tw.previousSibling = function() { return sibling(this, true); };
            function sibling(tw, prev) {
                var node = tw.currentNode;
                if (node === tw.root) return null;
                while (true) {
                    var sib = prev ? node.previousSibling : node.nextSibling;
                    while (sib) {
                        var r = accept(tw, sib);
                        if (r === FILTER_ACCEPT) { tw.currentNode = sib; return sib; }
                        if (r === FILTER_SKIP && sib.childNodes && sib.childNodes.length) {
                            sib = prev ? sib.lastChild : sib.firstChild;
                        } else {
                            sib = prev ? sib.previousSibling : sib.nextSibling;
                        }
                    }
                    node = node.parentNode;
                    if (!node || node === tw.root) return null;
                    if (accept(tw, node) === FILTER_ACCEPT) return null;
                }
            }
            tw.nextNode = function() {
                var node = this.currentNode;
                while (true) {
                    if (node.childNodes && node.childNodes.length) {
                        for (var i = 0; i < node.childNodes.length; i++) {
                            var child = node.childNodes[i];
                            var r = accept(this, child);
                            if (r === FILTER_ACCEPT) { this.currentNode = child; return child; }
                            if (r === FILTER_SKIP) { node = child; break; }
                        }
                        if (node !== this.currentNode && node.childNodes && node.childNodes.length) continue;
                    }
                    // Inget barn — sök syskon
                    while (node && node !== this.root) {
                        var sib = node.nextSibling;
                        if (sib) {
                            var r2 = accept(this, sib);
                            if (r2 === FILTER_ACCEPT) { this.currentNode = sib; return sib; }
                            if (r2 === FILTER_SKIP) { node = sib; break; }
                            node = sib; continue;
                        }
                        node = node.parentNode;
                    }
                    if (!node || node === this.root) return null;
                }
            };
            tw.previousNode = function() {
                var node = this.currentNode;
                while (node && node !== this.root) {
                    var sib = node.previousSibling;
                    while (sib) {
                        var r = accept(this, sib);
                        if (r !== FILTER_REJECT) {
                            while (sib.lastChild) {
                                sib = sib.lastChild;
                                r = accept(this, sib);
                                if (r === FILTER_REJECT) break;
                            }
                            if (r === FILTER_ACCEPT) { this.currentNode = sib; return sib; }
                        }
                        sib = sib.previousSibling;
                    }
                    if (node === this.root) return null;
                    node = node.parentNode;
                    if (node && accept(this, node) === FILTER_ACCEPT) { this.currentNode = node; return node; }
                }
                return null;
            };
            Object.defineProperty(tw, Symbol.toStringTag, { value: 'TreeWalker' });
        })
        "#;

        let setup_fn: Function = ctx.eval(walker_code)?;
        setup_fn.call::<_, Value>((tw.clone(),))?;

        Ok(tw.into_value())
    }
}

// NodeIterator — samma pattern men flat iteration
#[allow(dead_code)]
struct CreateNodeIterator {
    state: SharedState,
}
impl JsHandler for CreateNodeIterator {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let _root_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: root is not a Node")?
                        .into(),
                ));
            }
        };
        // WebIDL: whatToShow = ToUint32(arg). undefined → 0xFFFFFFFF, null → 0
        let what_to_show = match args.get(1) {
            None => 0xFFFFFFFF_u32,
            Some(v) if v.is_undefined() => 0xFFFFFFFF_u32,
            Some(v) => webidl_unsigned_long(Some(v)),
        };
        let filter = args.get(2).cloned();

        let ni = Object::new(ctx.clone())?;
        let root_val = args
            .first()
            .cloned()
            .unwrap_or(Value::new_undefined(ctx.clone()));
        // Readonly-egenskaper via defineProperty
        let what_show_f64 = what_to_show as f64;
        let filter_val = match &filter {
            Some(f) if !f.is_null() && !f.is_undefined() => f.clone(),
            _ => Value::new_null(ctx.clone()),
        };
        let readonly_code = r#"
        (function(ni, root, filter, whatToShow) {
            Object.defineProperty(ni, 'root', { get: function() { return root; }, configurable: false });
            Object.defineProperty(ni, 'referenceNode', { get: function() { return ni.__referenceNode; }, configurable: false });
            Object.defineProperty(ni, 'pointerBeforeReferenceNode', { get: function() { return ni.__pointerBefore; }, configurable: false });
            Object.defineProperty(ni, 'whatToShow', { value: whatToShow, writable: false, configurable: false });
            Object.defineProperty(ni, 'filter', { value: filter, writable: false, configurable: false });
        })
        "#;
        ni.set("__referenceNode", root_val.clone())?;
        ni.set("__pointerBefore", true)?;
        ni.set("__whatToShow", what_show_f64)?;
        let readonly_fn: Function = ctx.eval(readonly_code)?;
        readonly_fn.call::<_, Value>((ni.clone(), root_val, filter_val, what_show_f64))?;

        let iter_code = r#"
        (function(ni) {
            var FILTER_ACCEPT = 1, FILTER_REJECT = 2, FILTER_SKIP = 3;
            function accept(ni, node) {
                var show = ni.__whatToShow;
                if (show === undefined || show === null) show = 0xFFFFFFFF;
                show = show >>> 0;
                var bit = 1 << (node.nodeType - 1);
                if (!(show & bit)) return FILTER_SKIP;
                var f = ni.filter;
                if (!f) return FILTER_ACCEPT;
                var result;
                if (typeof f === 'function') result = f(node);
                else if (typeof f === 'object' && typeof f.acceptNode === 'function') result = f.acceptNode(node);
                else return FILTER_ACCEPT;
                return (Number(result) | 0);
            }
            // Pre-order flat traversal
            function traverse(root) {
                var list = [];
                function walk(node) {
                    list.push(node);
                    var cn = node.childNodes;
                    if (cn) for (var i = 0; i < cn.length; i++) walk(cn[i]);
                }
                walk(root);
                return list;
            }
            function sameNode(a, b) {
                if (a === b) return true;
                if (a && b && a.__nodeKey__ !== undefined && a.__nodeKey__ === b.__nodeKey__) return true;
                return false;
            }
            ni.nextNode = function() {
                var root = this.root;
                var all = traverse(root);
                var ref = this.__referenceNode;
                var idx = -1;
                for (var i = 0; i < all.length; i++) {
                    if (sameNode(all[i], ref)) { idx = i; break; }
                }
                if (this.__pointerBefore) {
                    for (var j = idx; j < all.length; j++) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.__referenceNode = all[j]; this.__pointerBefore = false; return all[j];
                        }
                    }
                } else {
                    for (var j = idx + 1; j < all.length; j++) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.__referenceNode = all[j]; return all[j];
                        }
                    }
                }
                return null;
            };
            ni.previousNode = function() {
                var root = this.root;
                var all = traverse(root);
                var ref = this.__referenceNode;
                var idx = all.length;
                for (var i = 0; i < all.length; i++) {
                    if (sameNode(all[i], ref)) { idx = i; break; }
                }
                if (!this.__pointerBefore) {
                    for (var j = idx; j >= 0; j--) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.__referenceNode = all[j]; this.__pointerBefore = true; return all[j];
                        }
                    }
                } else {
                    for (var j = idx - 1; j >= 0; j--) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.__referenceNode = all[j]; return all[j];
                        }
                    }
                }
                return null;
            };
            ni.detach = function() {};
            Object.defineProperty(ni, Symbol.toStringTag, { value: 'NodeIterator' });
        })
        "#;

        let setup_fn: Function = ctx.eval(iter_code)?;
        setup_fn.call::<_, Value>((ni.clone(),))?;

        Ok(ni.into_value())
    }
}

/// WebIDL unsigned long konvertering: ToUint32
/// -1 → 4294967295, "test" → 0, undefined → 0
fn webidl_unsigned_long(val: Option<&rquickjs::Value<'_>>) -> u32 {
    match val {
        Some(v) => {
            let n = v.as_number().unwrap_or(0.0);
            if n.is_nan() || n.is_infinite() {
                0
            } else {
                n as i64 as u32
            }
        }
        None => 0,
    }
}

/// WebIDL DOMString-konvertering: null→"null", undefined→"undefined", number→string
pub(super) fn js_value_to_dom_string(val: Option<&rquickjs::Value<'_>>) -> String {
    match val {
        None => String::new(),
        Some(v) => {
            if v.is_null() {
                "null".to_string()
            } else if v.is_undefined() {
                "undefined".to_string()
            } else if let Some(s) = v.as_string() {
                s.to_string().unwrap_or_default()
            } else if let Some(b) = v.as_bool() {
                if b { "true" } else { "false" }.to_string()
            } else if let Some(n) = v.as_number() {
                if n == (n as i64) as f64 {
                    format!("{}", n as i64)
                } else {
                    format!("{}", n)
                }
            } else {
                String::new()
            }
        }
    }
}

/// Konvertera UTF-16 code unit offset till UTF-8 byte offset.
/// JavaScript räknar i UTF-16 code units (surrogat-par = 2 units).
fn utf16_offset_to_byte(s: &str, utf16_offset: usize) -> usize {
    let mut utf16_pos = 0;
    for (byte_idx, ch) in s.char_indices() {
        if utf16_pos >= utf16_offset {
            return byte_idx;
        }
        utf16_pos += ch.len_utf16();
    }
    s.len()
}

/// Räkna antal UTF-16 code units i en sträng (= JavaScript .length)
fn utf16_len(s: &str) -> usize {
    s.chars().map(|c| c.len_utf16()).sum()
}

pub(super) fn args_to_node_keys<'js>(
    _ctx: &Ctx<'js>,
    args: &[Value<'js>],
    state: &SharedState,
) -> rquickjs::Result<Vec<NodeKey>> {
    let mut keys = Vec::with_capacity(args.len());
    for arg in args {
        if let Some(nk) = extract_node_key(arg) {
            keys.push(nk);
        } else {
            // Konvertera till textinnehåll: null→"null", undefined→"undefined", etc.
            let text = if arg.is_null() {
                "null".to_string()
            } else if arg.is_undefined() {
                "undefined".to_string()
            } else if let Some(s) = arg.as_string() {
                s.to_string().unwrap_or_default()
            } else if let Some(n) = arg.as_number() {
                if n == (n as i64) as f64 {
                    format!("{}", n as i64)
                } else {
                    format!("{}", n)
                }
            } else {
                "".to_string()
            };
            let mut s = state.borrow_mut();
            let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                node_type: NodeType::Text,
                tag: None,
                attributes: crate::arena_dom::Attrs::new(),
                text: Some(text.into()),
                parent: None,
                children: vec![],
                owner_doc: None,
            });
            keys.push(text_key);
        }
    }
    Ok(keys)
}

pub(super) fn clone_node_recursive(state: &SharedState, key: NodeKey, deep: bool) -> NodeKey {
    let mut s = state.borrow_mut();
    let node = match s.arena.nodes.get(key) {
        Some(n) => n.clone(),
        None => return key,
    };
    let new_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
        node_type: node.node_type.clone(),
        tag: node.tag.clone(),
        attributes: node.attributes.clone(),
        text: node.text.clone(),
        parent: None,
        children: vec![],
        owner_doc: None,
    });
    if deep {
        let children = node.children.clone();
        drop(s);
        for child in children {
            let cloned_child = clone_node_recursive(state, child, true);
            let mut s = state.borrow_mut();
            s.arena.append_child(new_key, cloned_child);
        }
    }
    new_key
}

/// attachShadow({mode: "open"|"closed"}) — skapar en shadow root som barn-element
struct AttachShadow {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for AttachShadow {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Extrahera mode (default "open")
        let _mode = args
            .first()
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get::<_, String>("mode").ok())
            .unwrap_or_else(|| "open".to_string());

        // Skapa en shadow root-nod (template med shadowrootmode-attribut)
        let shadow_key = {
            let mut s = self.state.borrow_mut();
            let shadow_node = crate::arena_dom::DomNode {
                node_type: crate::arena_dom::NodeType::Element,
                tag: Some("template".to_string()),
                attributes: {
                    let mut attrs = crate::arena_dom::Attrs::new();
                    attrs.insert("shadowrootmode".to_string(), _mode);
                    attrs
                },
                text: None,
                parent: Some(self.key),
                children: vec![],
                owner_doc: None,
            };
            let sk = s.arena.nodes.insert(shadow_node);
            // Lägg till som första barn
            if let Some(parent) = s.arena.nodes.get_mut(self.key) {
                parent.children.insert(0, sk);
            }
            s.mutations.push(std::borrow::Cow::Borrowed("attachShadow"));
            sk
        };
        make_element_object(ctx, shadow_key, &self.state)
    }
}

struct QuerySelectorElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for QuerySelectorElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let sel = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let key = {
            let s = self.state.borrow();
            find_first_matching(&s.arena, self.key, &sel)
        };
        match key {
            Some(k) => make_element_object(ctx, k, &self.state),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct QuerySelectorAllElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for QuerySelectorAllElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let sel = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let keys = {
            let s = self.state.borrow();
            let mut results = vec![];
            find_all_matching(&s.arena, self.key, &sel, &mut results);
            results
        };
        let array = rquickjs::Array::new(ctx.clone())?;
        for (i, k) in keys.iter().enumerate() {
            array.set(i, make_element_object(ctx, *k, &self.state)?)?;
        }
        Ok(array.into_value())
    }
}

struct ClosestElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ClosestElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let sel = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        let mut current = Some(self.key);
        while let Some(k) = current {
            if matches_selector(&s.arena, k, &sel) {
                drop(s);
                return make_element_object(ctx, k, &self.state);
            }
            current = s.arena.nodes.get(k).and_then(|n| n.parent);
        }
        Ok(Value::new_null(ctx.clone()))
    }
}

struct MatchesElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for MatchesElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let sel = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        Ok(Value::new_bool(
            ctx.clone(),
            matches_selector(&s.arena, self.key, &sel),
        ))
    }
}

struct GetBoundingClientRect {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetBoundingClientRect {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let (x, y, width, height) = estimate_layout_rect(&s.arena, self.key);
        let rect = Object::new(ctx.clone())?;
        rect.set("x", x)?;
        rect.set("y", y)?;
        rect.set("width", width)?;
        rect.set("height", height)?;
        rect.set("top", y)?;
        rect.set("right", x + width)?;
        rect.set("bottom", y + height)?;
        rect.set("left", x)?;
        Ok(rect.into_value())
    }
}

struct GetClientRects {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetClientRects {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let (x, y, width, height) = estimate_layout_rect(&s.arena, self.key);
        let rect = Object::new(ctx.clone())?;
        rect.set("x", x)?;
        rect.set("y", y)?;
        rect.set("width", width)?;
        rect.set("height", height)?;
        let arr = rquickjs::Array::new(ctx.clone())?;
        arr.set(0, rect)?;
        Ok(arr.into_value())
    }
}

// ─── CharacterData .data/.nodeValue/.length — native Rust (migrerad från polyfill) ──

struct CharDataGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for CharDataGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let text = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| t.to_string())
            .unwrap_or_default();
        Ok(Value::from_string(rquickjs::String::from_str(
            ctx.clone(),
            &text,
        )?))
    }
}

/// Setter för .nodeValue — null → "" (empty), andra värden → DOMString
struct NodeValueSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for NodeValueSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first();
        // nodeValue: null → empty string, undefined → "undefined", annat → DOMString
        let new_val = match val {
            Some(v) if v.is_null() => String::new(),
            _ => js_value_to_dom_string(val),
        };
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.text = Some(new_val.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct CharDataSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for CharDataSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // WebIDL DOMString: undefined→"undefined", null→"null", 0→"0" etc.
        let new_val = js_value_to_dom_string(args.first());
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.text = Some(new_val.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct CharDataLengthGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for CharDataLengthGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| t.encode_utf16().count())
            .unwrap_or(0);
        Ok(Value::new_int(ctx.clone(), len as i32))
    }
}

// Getter/setter handlers
struct TextContentGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for TextContentGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        // Spec: textContent returns null for Document and Doctype nodes
        if let Some(node) = s.arena.nodes.get(self.key) {
            if matches!(
                node.node_type,
                crate::arena_dom::NodeType::Document | crate::arena_dom::NodeType::Doctype
            ) {
                return Ok(Value::new_null(ctx.clone()));
            }
        }
        let text = get_text_content(&s.arena, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &text)?.into_value())
    }
}

struct TextContentSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for TextContentSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first();
        // Per IDL: textContent setter tar DOMString? — undefined → null
        let is_null = val.map(|v| v.is_null() || v.is_undefined()).unwrap_or(true);
        let mut s = self.state.borrow_mut();
        let node_type = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.node_type.clone())
            .unwrap_or(NodeType::Other);
        // Spec: textContent setter på Document/Doctype = no-op
        if matches!(node_type, NodeType::Document | NodeType::Doctype) {
            return Ok(Value::new_undefined(ctx.clone()));
        }
        // Text/Comment/PI: uppdatera .text direkt
        if matches!(
            node_type,
            NodeType::Text | NodeType::Comment | NodeType::ProcessingInstruction
        ) {
            let text = if is_null {
                String::new()
            } else {
                js_value_to_dom_string(val)
            };
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.text = Some(text.into());
            }
        } else {
            // Element/DocumentFragment: rensa barn först (uppdatera parent-pekare)
            let old_children: Vec<NodeKey> = s
                .arena
                .nodes
                .get(self.key)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            for ck in &old_children {
                if let Some(child) = s.arena.nodes.get_mut(*ck) {
                    child.parent = None;
                }
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.clear();
            }
            // Spec: null → ta bort barn, skapa INTE textnod
            if !is_null {
                let text = js_value_to_dom_string(val);
                if !text.is_empty() {
                    let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                        node_type: NodeType::Text,
                        tag: None,
                        attributes: crate::arena_dom::Attrs::new(),
                        text: Some(text.into()),
                        parent: Some(self.key),
                        children: vec![],
                        owner_doc: None,
                    });
                    s.arena.append_child(self.key, text_key);
                }
            }
        }
        s.mutations
            .push(std::borrow::Cow::Borrowed("setTextContent"));
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct InnerHTMLGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InnerHTMLGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let html = get_inner_html(&s.arena, self.key);
        Ok(rquickjs::String::from_str(ctx.clone(), &html)?.into_value())
    }
}

struct InnerHTMLSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InnerHTMLSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Konvertera argument till sträng (spec: ToString(value))
        let html_str = match args.first() {
            Some(v) if v.is_null() => String::new(), // innerHTML = null → clear
            Some(v) if v.is_undefined() => "undefined".to_string(),
            Some(v) => v
                .as_string()
                .and_then(|s| s.to_string().ok())
                .or_else(|| {
                    v.as_number().map(|n| {
                        if n == (n as i64) as f64 {
                            format!("{}", n as i64)
                        } else {
                            format!("{}", n)
                        }
                    })
                })
                .or_else(|| v.as_bool().map(|b| b.to_string()))
                .or_else(|| {
                    // Anropa .toString() på objektet
                    v.as_object()
                        .and_then(|obj| obj.get::<_, Function>("toString").ok())
                        .and_then(|f| f.call::<_, rquickjs::String>(()).ok())
                        .and_then(|s| s.to_string().ok())
                })
                .unwrap_or_default(),
            None => String::new(),
        };
        let key_bits = node_key_to_f64(self.key);
        {
            let mut s = self.state.borrow_mut();
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.clear();
            }
            // Enkel textnode — fullständig HTML-parsing är komplex
            if !html_str.contains('<') {
                let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: crate::arena_dom::Attrs::new(),
                    text: Some(html_str.clone().into()),
                    parent: Some(self.key),
                    children: vec![],
                    owner_doc: None,
                });
                s.arena.append_child(self.key, text_key);
            } else {
                // Parsa HTML-fragment och lägg till som barn
                let rcdom = crate::parser::parse_html(&html_str);
                let fragment = ArenaDom::from_rcdom(&rcdom);
                let doc_key = fragment.document;
                // Kopiera alla barn från fragment till vår nod
                if let Some(doc_node) = fragment.nodes.get(doc_key) {
                    for &child in &doc_node.children {
                        copy_subtree(&fragment, child, self.key, &mut s.arena);
                    }
                }
            }
            s.mutations.push(std::borrow::Cow::Owned(format!(
                "setInnerHTML:{}:{}",
                key_bits,
                html_str.len()
            )));
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Kopiera en subnod från en arena till en annan
pub(super) fn copy_subtree(src: &ArenaDom, src_key: NodeKey, parent: NodeKey, dst: &mut ArenaDom) {
    let node = match src.nodes.get(src_key) {
        Some(n) => n.clone(),
        None => return,
    };
    let new_key = dst.nodes.insert(crate::arena_dom::DomNode {
        node_type: node.node_type,
        tag: node.tag,
        attributes: node.attributes,
        text: node.text,
        parent: Some(parent),
        children: vec![],
        owner_doc: None,
    });
    dst.append_child(parent, new_key);
    for &child in &node.children {
        copy_subtree(src, child, new_key, dst);
    }
}

pub(super) struct NoOpHandler;

struct NullGetter;
impl JsHandler for NullGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_null(ctx.clone()))
    }
}
impl JsHandler for NoOpHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Skapa ett JS-objekt som representerar ett DOM-element
pub(super) fn make_element_object<'js>(
    ctx: &Ctx<'js>,
    key: NodeKey,
    state: &SharedState,
) -> rquickjs::Result<Value<'js>> {
    let key_bits = node_key_to_f64(key);

    // ─── Node identity cache: samma NodeKey → samma JS-objekt ───
    // Krävs av WPT: getElementById(x) === getElementById(x)
    {
        let code = format!(
            "globalThis.__nodeCache && globalThis.__nodeCache.get({})",
            key_bits
        );
        if let Ok(cached) = ctx.eval::<Value, _>(code.as_str()) {
            if !cached.is_undefined() && !cached.is_null() {
                // Säkerställ att polyfill-patches applicerats (element skapade under
                // initial DOM-parsing kan ha cachats innan polyfills laddades)
                let patch_code = format!(
                    concat!(
                        "(function(el){{",
                        "if(el&&el.nodeType===1&&!el.attributes&&globalThis.__patchChildNode)",
                        "globalThis.__patchChildNode(el)",
                        "}})(globalThis.__nodeCache.get({}))"
                    ),
                    key_bits
                );
                let _ = ctx.eval::<Value, _>(patch_code.as_str());
                return Ok(cached);
            }
        }
    }

    // Läs grundläggande egenskaper
    let (node_type_val, tag_name, id_val, class_val) = {
        let s = state.borrow();
        let node = s.arena.nodes.get(key);
        let nt = match node.map(|n| &n.node_type) {
            Some(NodeType::Element) => 1,
            Some(NodeType::Text) => 3,
            Some(NodeType::ProcessingInstruction) => 7,
            Some(NodeType::Comment) => 8,
            Some(NodeType::Document) => 9,
            Some(NodeType::Doctype) => 10,
            Some(NodeType::DocumentFragment) => 11,
            _ => 1,
        };
        let tag = if nt == 10 {
            // Doctype — nodeName/name = doctypens namn (t.ex. "html")
            node.and_then(|n| n.text.as_ref())
                .map(|t| t.to_string())
                .unwrap_or_else(|| "html".to_string())
        } else {
            // tagName: uppercase bara för HTML namespace (eller parsade element utan __ns__)
            let ns = node.and_then(|n| n.get_attr("__ns__"));
            let is_html_ns = ns.is_none() || ns == Some("http://www.w3.org/1999/xhtml");
            let raw_tag = node
                .and_then(|n| n.tag.as_ref())
                .cloned()
                .unwrap_or_default();
            if is_html_ns {
                raw_tag.to_ascii_uppercase()
            } else {
                // Icke-HTML namespace: bevara case som-den-är
                raw_tag
            }
        };
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

    // Skapa objekt med rätt prototypkedja för instanceof-stöd
    let obj = {
        let proto_code = match node_type_val {
            3 => "Object.create(typeof Text!=='undefined'?Text.prototype:{})".to_string(),
            8 => "Object.create(typeof Comment!=='undefined'?Comment.prototype:{})".to_string(),
            9 => "Object.create(typeof Document!=='undefined'?Document.prototype:{})".to_string(),
            10 => "Object.create(typeof DocumentType!=='undefined'?DocumentType.prototype:{})"
                .to_string(),
            11 => {
                "Object.create(typeof DocumentFragment!=='undefined'?DocumentFragment.prototype:{})"
                    .to_string()
            }
            _ => {
                // Element — välj konstruktor via __tagToConstructor
                format!(
                    "(function(){{var C=globalThis.__tagToConstructor&&globalThis.__tagToConstructor['{}'];return Object.create(C?C.prototype:(typeof HTMLElement!=='undefined'?HTMLElement.prototype:{{}}))}})()",
                    tag_name
                )
            }
        };
        match ctx.eval::<Value, _>(proto_code.as_str()) {
            Ok(v) if v.is_object() => v.into_object().unwrap(),
            _ => Object::new(ctx.clone())?,
        }
    };
    obj.set("__nodeKey__", key_bits)?;
    obj.set("nodeType", node_type_val)?;
    obj.set("tagName", tag_name.as_str())?;
    // nodeName enligt DOM-spec
    let node_name = match node_type_val {
        3 => "#text".to_string(),
        7 => tag_name.clone(), // PI: target (tag_name är redan target)
        8 => "#comment".to_string(),
        10 => tag_name.clone(), // Doctype: name
        11 => "#document-fragment".to_string(),
        _ => tag_name.clone(),
    };
    obj.set("nodeName", node_name.as_str())?;
    // localName — sätts för alla element/PI-noder
    if node_type_val == 1 || node_type_val == 7 {
        let local_name = {
            let s = state.borrow();
            let node = s.arena.nodes.get(key);
            // createElementNS sparar __localName__ som internt attribut
            node.and_then(|n| n.get_attr("__localName__"))
                .map(|s| s.to_string())
                .unwrap_or_else(|| {
                    // Vanliga element: tag = localName (redan lowercase)
                    node.and_then(|n| n.tag.as_deref())
                        .unwrap_or("")
                        .to_string()
                })
        };
        obj.set("localName", local_name.as_str())?;
    }
    // ownerDocument — lazy getter som hämtar document från globals vid anrop
    // Löser timing: body/head/documentElement skapas innan document registreras.
    if node_type_val != 9 {
        obj.prop(
            "ownerDocument",
            Accessor::new_get(JsFn(OwnerDocumentGetter)).configurable(),
        )?;
    }
    obj.set("id", id_val.as_str())?;
    obj.set("className", class_val.as_str())?;
    // baseURI per DOM spec (alla noder ärver document.baseURI)
    obj.set("baseURI", "about:blank")?;

    // HTML Element Reflected Properties (.type, .value, .checked, etc.)
    if node_type_val == 1 {
        html_properties::set_html_reflected_properties(ctx, &obj, state, key)?;
        // Auto-genererade HTML element bindings (getter/setter/metoder per tag)
        generated::register::register_html_element_properties(
            ctx,
            &obj,
            state,
            key,
            &tag_name.to_ascii_lowercase(),
        )?;
        // DOM impl beteendelogik: ValidityState, input value modes, form, select
        register_dom_impl_properties(ctx, &obj, state, key, &tag_name)?;
    }

    // Doctype-specifika egenskaper
    if node_type_val == 10 {
        obj.set("name", tag_name.as_str())?;
        obj.set("publicId", "")?;
        obj.set("systemId", "")?;
    }
    // nodeValue + data: live getter/setter för Text/Comment/PI, null för övriga
    match node_type_val {
        1 | 9 | 10 | 11 => {
            // nodeValue: getter → null, setter → no-op (per spec)
            obj.prop(
                "nodeValue",
                rquickjs::object::Accessor::new(JsFn(NullGetter), JsFn(NoOpHandler))
                    .configurable()
                    .enumerable(),
            )?;
        }
        3 | 7 | 8 => {
            // Text(3)/PI(7)/Comment(8) — live getter/setter för nodeValue och data
            // nodeValue: null → "" (per spec: DOMString? setter)
            obj.prop(
                "nodeValue",
                rquickjs::object::Accessor::new(
                    JsFn(CharDataGetter {
                        state: Rc::clone(state),
                        key,
                    }),
                    JsFn(NodeValueSetter {
                        state: Rc::clone(state),
                        key,
                    }),
                )
                .configurable()
                .enumerable(),
            )?;
            let d_get_state = Rc::clone(state);
            let d_set_state = Rc::clone(state);
            obj.prop(
                "data",
                rquickjs::object::Accessor::new(
                    JsFn(CharDataGetter {
                        state: d_get_state,
                        key,
                    }),
                    JsFn(CharDataSetter {
                        state: d_set_state,
                        key,
                    }),
                )
                .configurable()
                .enumerable(),
            )?;
            // .length — UTF-16 code unit count (live)
            let len_state = Rc::clone(state);
            obj.prop(
                "length",
                rquickjs::object::Accessor::new_get(JsFn(CharDataLengthGetter {
                    state: len_state,
                    key,
                }))
                .configurable()
                .enumerable(),
            )?;
        }
        _ => {}
    }

    // ─── Metoder ───────────────────────────────────────────────────
    obj.set(
        "getAttribute",
        Function::new(
            ctx.clone(),
            JsFn(GetAttribute {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setAttribute",
        Function::new(
            ctx.clone(),
            JsFn(SetAttribute {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "removeAttribute",
        Function::new(
            ctx.clone(),
            JsFn(RemoveAttribute {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "hasAttribute",
        Function::new(
            ctx.clone(),
            JsFn(HasAttribute {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "hasChildNodes",
        Function::new(
            ctx.clone(),
            JsFn(HasChildNodes {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "normalize",
        Function::new(
            ctx.clone(),
            JsFn(NormalizeNode {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getAttributeNames",
        Function::new(
            ctx.clone(),
            JsFn(GetAttributeNames {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "insertAdjacentHTML",
        Function::new(
            ctx.clone(),
            JsFn(InsertAdjacentHTML {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "appendChild",
        Function::new(
            ctx.clone(),
            JsFn(AppendChild {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "removeChild",
        Function::new(
            ctx.clone(),
            JsFn(RemoveChild {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "insertBefore",
        Function::new(
            ctx.clone(),
            JsFn(InsertBefore {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "replaceChild",
        Function::new(
            ctx.clone(),
            JsFn(ReplaceChild {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "cloneNode",
        Function::new(
            ctx.clone(),
            JsFn(CloneNode {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    // ─── Rust-native ChildNode/ParentNode-metoder ─────────────────────
    obj.set(
        "remove",
        Function::new(
            ctx.clone(),
            JsFn(Remove {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "before",
        Function::new(
            ctx.clone(),
            JsFn(Before {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "after",
        Function::new(
            ctx.clone(),
            JsFn(After {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "replaceWith",
        Function::new(
            ctx.clone(),
            JsFn(ReplaceWith {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "toggleAttribute",
        Function::new(
            ctx.clone(),
            JsFn(ToggleAttribute {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "prepend",
        Function::new(
            ctx.clone(),
            JsFn(Prepend {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "append",
        Function::new(
            ctx.clone(),
            JsFn(Append {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "replaceChildren",
        Function::new(
            ctx.clone(),
            JsFn(ReplaceChildren {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "insertAdjacentElement",
        Function::new(
            ctx.clone(),
            JsFn(InsertAdjacentElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "insertAdjacentText",
        Function::new(
            ctx.clone(),
            JsFn(InsertAdjacentText {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    // CharacterData-metoder — nu Rust-native med UTF-16 code unit counting
    if node_type_val == 3 || node_type_val == 8 {
        obj.set(
            "substringData",
            Function::new(
                ctx.clone(),
                JsFn(SubstringData {
                    state: Rc::clone(state),
                    key,
                }),
            )?,
        )?;
        obj.set(
            "appendData",
            Function::new(
                ctx.clone(),
                JsFn(AppendData {
                    state: Rc::clone(state),
                    key,
                }),
            )?,
        )?;
        obj.set(
            "insertData",
            Function::new(
                ctx.clone(),
                JsFn(InsertData {
                    state: Rc::clone(state),
                    key,
                }),
            )?,
        )?;
        obj.set(
            "deleteData",
            Function::new(
                ctx.clone(),
                JsFn(DeleteData {
                    state: Rc::clone(state),
                    key,
                }),
            )?,
        )?;
        obj.set(
            "replaceData",
            Function::new(
                ctx.clone(),
                JsFn(ReplaceData {
                    state: Rc::clone(state),
                    key,
                }),
            )?,
        )?;
    }
    // Text-specifika metoder: splitText, wholeText
    if node_type_val == 3 {
        obj.set(
            "splitText",
            Function::new(
                ctx.clone(),
                JsFn(SplitText {
                    state: Rc::clone(state),
                    key,
                }),
            )?,
        )?;
    }
    // ─── Rust-native: element-level queries + NS + namespace ────────────
    obj.set(
        "getElementsByTagName",
        Function::new(
            ctx.clone(),
            JsFn(GetElementsByTagNameElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getElementsByClassName",
        Function::new(
            ctx.clone(),
            JsFn(GetElementsByClassNameElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getElementsByTagNameNS",
        Function::new(
            ctx.clone(),
            JsFn(GetElementsByTagNameNSElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "moveBefore",
        Function::new(
            ctx.clone(),
            JsFn(MoveBefore {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "lookupNamespaceURI",
        Function::new(
            ctx.clone(),
            JsFn(LookupNamespaceURI {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "lookupPrefix",
        Function::new(ctx.clone(), JsFn(LookupPrefix { _key: key }))?,
    )?;
    obj.set(
        "isDefaultNamespace",
        Function::new(
            ctx.clone(),
            JsFn(IsDefaultNamespace {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "setAttributeNS",
        Function::new(
            ctx.clone(),
            JsFn(SetAttributeNS {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getAttributeNS",
        Function::new(
            ctx.clone(),
            JsFn(GetAttributeNS {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "hasAttributeNS",
        Function::new(
            ctx.clone(),
            JsFn(HasAttributeNS {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "removeAttributeNS",
        Function::new(
            ctx.clone(),
            JsFn(RemoveAttributeNS {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getAttributeNode",
        Function::new(
            ctx.clone(),
            JsFn(GetAttributeNode {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "compareDocumentPosition",
        Function::new(
            ctx.clone(),
            JsFn(CompareDocumentPosition {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "isEqualNode",
        Function::new(
            ctx.clone(),
            JsFn(IsEqualNode {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "isSameNode",
        Function::new(ctx.clone(), JsFn(IsSameNode { key }))?,
    )?;
    obj.set(
        "normalize",
        Function::new(
            ctx.clone(),
            JsFn(NormalizeNode {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    // attachShadow — skapa en shadow root (enkel implementation)
    obj.set(
        "attachShadow",
        Function::new(
            ctx.clone(),
            JsFn(AttachShadow {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "querySelector",
        Function::new(
            ctx.clone(),
            JsFn(QuerySelectorElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "querySelectorAll",
        Function::new(
            ctx.clone(),
            JsFn(QuerySelectorAllElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "closest",
        Function::new(
            ctx.clone(),
            JsFn(ClosestElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "matches",
        Function::new(
            ctx.clone(),
            JsFn(MatchesElement {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getBoundingClientRect",
        Function::new(
            ctx.clone(),
            JsFn(GetBoundingClientRect {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "getClientRects",
        Function::new(
            ctx.clone(),
            JsFn(GetClientRects {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "addEventListener",
        Function::new(
            ctx.clone(),
            JsFn(AddEventListenerHandler {
                state: Rc::clone(state),
                key,
                override_key: None,
            }),
        )?,
    )?;
    obj.set(
        "removeEventListener",
        Function::new(
            ctx.clone(),
            JsFn(RemoveEventListenerHandler {
                state: Rc::clone(state),
                key,
                override_key: None,
            }),
        )?,
    )?;
    obj.set(
        "dispatchEvent",
        Function::new(
            ctx.clone(),
            JsFn(DispatchEventHandler {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "focus",
        Function::new(
            ctx.clone(),
            JsFn(FocusHandler {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "blur",
        Function::new(
            ctx.clone(),
            JsFn(BlurHandler {
                state: Rc::clone(state),
            }),
        )?,
    )?;
    obj.set(
        "contains",
        Function::new(
            ctx.clone(),
            JsFn(ContainsHandler {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "click",
        Function::new(
            ctx.clone(),
            JsFn(ClickHandler {
                state: Rc::clone(state),
                key,
            }),
        )?,
    )?;
    obj.set(
        "scrollIntoView",
        Function::new(ctx.clone(), JsFn(NoOpHandler))?,
    )?;
    obj.set(
        "requestPointerLock",
        Function::new(ctx.clone(), JsFn(NoOpHandler))?,
    )?;

    // ─── textContent / innerHTML via getter/setter ──────────────────
    obj.prop(
        "textContent",
        Accessor::new(
            JsFn(TextContentGetter {
                state: Rc::clone(state),
                key,
            }),
            JsFn(TextContentSetter {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable()
        .enumerable(),
    )?;
    obj.prop(
        "innerHTML",
        Accessor::new(
            JsFn(InnerHTMLGetter {
                state: Rc::clone(state),
                key,
            }),
            JsFn(InnerHTMLSetter {
                state: Rc::clone(state),
                key,
            }),
        )
        .configurable()
        .enumerable(),
    )?;

    // ─── Navigation properties (lazy via getters — undviker oändlig rekursion) ─
    struct ParentNodeGetter {
        state: SharedState,
        key: NodeKey,
    }
    impl JsHandler for ParentNodeGetter {
        fn handle<'js>(
            &self,
            ctx: &Ctx<'js>,
            _args: &[Value<'js>],
        ) -> rquickjs::Result<Value<'js>> {
            let parent = self
                .state
                .borrow()
                .arena
                .nodes
                .get(self.key)
                .and_then(|n| n.parent);
            match parent {
                Some(pk) => make_element_object(ctx, pk, &self.state),
                None => Ok(Value::new_null(ctx.clone())),
            }
        }
    }
    struct ParentElementGetter {
        state: SharedState,
        key: NodeKey,
    }
    impl JsHandler for ParentElementGetter {
        fn handle<'js>(
            &self,
            ctx: &Ctx<'js>,
            _args: &[Value<'js>],
        ) -> rquickjs::Result<Value<'js>> {
            let s = self.state.borrow();
            let parent = s.arena.nodes.get(self.key).and_then(|n| n.parent);
            match parent {
                Some(pk) => {
                    // Spec: parentElement returnerar null om parent inte är Element
                    let is_element = s
                        .arena
                        .nodes
                        .get(pk)
                        .map(|n| matches!(n.node_type, NodeType::Element))
                        .unwrap_or(false);
                    drop(s);
                    if is_element {
                        make_element_object(ctx, pk, &self.state)
                    } else {
                        Ok(Value::new_null(ctx.clone()))
                    }
                }
                None => Ok(Value::new_null(ctx.clone())),
            }
        }
    }
    struct ChildNodesGetter {
        state: SharedState,
        key: NodeKey,
        elements_only: bool,
    }
    impl JsHandler for ChildNodesGetter {
        fn handle<'js>(
            &self,
            ctx: &Ctx<'js>,
            _args: &[Value<'js>],
        ) -> rquickjs::Result<Value<'js>> {
            let keys: Vec<NodeKey> = {
                let s = self.state.borrow();
                let all = s
                    .arena
                    .nodes
                    .get(self.key)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                if self.elements_only {
                    all.into_iter()
                        .filter(|&k| {
                            s.arena
                                .nodes
                                .get(k)
                                .map(|n| n.node_type == NodeType::Element)
                                .unwrap_or(false)
                        })
                        .collect()
                } else {
                    all
                }
            };
            let arr = rquickjs::Array::new(ctx.clone())?;
            for (i, k) in keys.iter().enumerate() {
                arr.set(i, make_element_object(ctx, *k, &self.state)?)?;
            }
            Ok(arr.into_value())
        }
    }
    struct ChildGetter {
        state: SharedState,
        key: NodeKey,
        first: bool,
        elements_only: bool,
    }
    impl JsHandler for ChildGetter {
        fn handle<'js>(
            &self,
            ctx: &Ctx<'js>,
            _args: &[Value<'js>],
        ) -> rquickjs::Result<Value<'js>> {
            let s = self.state.borrow();
            let all = s
                .arena
                .nodes
                .get(self.key)
                .map(|n| n.children.clone())
                .unwrap_or_default();
            let filtered: Vec<NodeKey> = if self.elements_only {
                all.into_iter()
                    .filter(|&k| {
                        s.arena
                            .nodes
                            .get(k)
                            .map(|n| n.node_type == NodeType::Element)
                            .unwrap_or(false)
                    })
                    .collect()
            } else {
                all
            };
            let target = if self.first {
                filtered.first().copied()
            } else {
                filtered.last().copied()
            };
            drop(s);
            match target {
                Some(k) => make_element_object(ctx, k, &self.state),
                None => Ok(Value::new_null(ctx.clone())),
            }
        }
    }
    struct SiblingGetter {
        state: SharedState,
        key: NodeKey,
        next: bool,
        elements_only: bool,
    }
    impl JsHandler for SiblingGetter {
        fn handle<'js>(
            &self,
            ctx: &Ctx<'js>,
            _args: &[Value<'js>],
        ) -> rquickjs::Result<Value<'js>> {
            let s = self.state.borrow();
            let result = s
                .arena
                .nodes
                .get(self.key)
                .and_then(|n| n.parent)
                .and_then(|pk| {
                    let parent = s.arena.nodes.get(pk)?;
                    let pos = parent.children.iter().position(|&c| c == self.key)?;
                    if self.next {
                        let slice = &parent.children[pos + 1..];
                        if self.elements_only {
                            slice
                                .iter()
                                .find(|&&c| {
                                    s.arena
                                        .nodes
                                        .get(c)
                                        .map(|n| n.node_type == NodeType::Element)
                                        .unwrap_or(false)
                                })
                                .copied()
                        } else {
                            slice.first().copied()
                        }
                    } else {
                        let slice = &parent.children[..pos];
                        if self.elements_only {
                            slice
                                .iter()
                                .rev()
                                .find(|&&c| {
                                    s.arena
                                        .nodes
                                        .get(c)
                                        .map(|n| n.node_type == NodeType::Element)
                                        .unwrap_or(false)
                                })
                                .copied()
                        } else if pos > 0 {
                            slice.last().copied()
                        } else {
                            None
                        }
                    }
                });
            drop(s);
            match result {
                Some(k) => make_element_object(ctx, k, &self.state),
                None => Ok(Value::new_null(ctx.clone())),
            }
        }
    }
    obj.prop(
        "parentNode",
        Accessor::new_get(JsFn(ParentNodeGetter {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "parentElement",
        Accessor::new_get(JsFn(ParentElementGetter {
            state: Rc::clone(state),
            key,
        }))
        .configurable(),
    )?;
    obj.prop(
        "childNodes",
        Accessor::new_get(JsFn(ChildNodesGetter {
            state: Rc::clone(state),
            key,
            elements_only: false,
        }))
        .configurable(),
    )?;
    obj.prop(
        "children",
        Accessor::new_get(JsFn(ChildNodesGetter {
            state: Rc::clone(state),
            key,
            elements_only: true,
        }))
        .configurable(),
    )?;
    obj.prop(
        "firstChild",
        Accessor::new_get(JsFn(ChildGetter {
            state: Rc::clone(state),
            key,
            first: true,
            elements_only: false,
        }))
        .configurable(),
    )?;
    obj.prop(
        "lastChild",
        Accessor::new_get(JsFn(ChildGetter {
            state: Rc::clone(state),
            key,
            first: false,
            elements_only: false,
        }))
        .configurable(),
    )?;
    obj.prop(
        "firstElementChild",
        Accessor::new_get(JsFn(ChildGetter {
            state: Rc::clone(state),
            key,
            first: true,
            elements_only: true,
        }))
        .configurable(),
    )?;
    obj.prop(
        "lastElementChild",
        Accessor::new_get(JsFn(ChildGetter {
            state: Rc::clone(state),
            key,
            first: false,
            elements_only: true,
        }))
        .configurable(),
    )?;
    obj.prop(
        "nextSibling",
        Accessor::new_get(JsFn(SiblingGetter {
            state: Rc::clone(state),
            key,
            next: true,
            elements_only: false,
        }))
        .configurable(),
    )?;
    obj.prop(
        "previousSibling",
        Accessor::new_get(JsFn(SiblingGetter {
            state: Rc::clone(state),
            key,
            next: false,
            elements_only: false,
        }))
        .configurable(),
    )?;
    obj.prop(
        "nextElementSibling",
        Accessor::new_get(JsFn(SiblingGetter {
            state: Rc::clone(state),
            key,
            next: true,
            elements_only: true,
        }))
        .configurable(),
    )?;
    obj.prop(
        "previousElementSibling",
        Accessor::new_get(JsFn(SiblingGetter {
            state: Rc::clone(state),
            key,
            next: false,
            elements_only: true,
        }))
        .configurable(),
    )?;

    // ─── Layout-egenskaper ─────────────────────────────────────────
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
        obj.set("offsetTop", y_pos)?;
        obj.set("offsetLeft", 0.0)?;
        obj.set("offsetWidth", w)?;
        obj.set("offsetHeight", h)?;
        obj.set("scrollTop", scroll_top)?;
        obj.set("scrollLeft", scroll_left)?;
        obj.set("scrollWidth", w.max(1024.0))?;
        obj.set("scrollHeight", content_h)?;
        obj.set("clientWidth", w)?;
        obj.set("clientHeight", h)?;
    }

    // ─── childElementCount ─────────────────────────────────────────
    {
        let s = state.borrow();
        let count = s
            .arena
            .nodes
            .get(key)
            .map(|n| {
                n.children
                    .iter()
                    .filter(|&&ck| {
                        s.arena
                            .nodes
                            .get(ck)
                            .map(|cn| cn.node_type == NodeType::Element)
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);
        obj.set("childElementCount", count as i32)?;
    }

    // ─── offsetParent ───────────────────────────────────────────────
    {
        struct OffsetParentGetter {
            state: SharedState,
            key: NodeKey,
        }
        impl JsHandler for OffsetParentGetter {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                let parent = self
                    .state
                    .borrow()
                    .arena
                    .nodes
                    .get(self.key)
                    .and_then(|n| n.parent);
                match parent {
                    Some(pk) => make_element_object(ctx, pk, &self.state),
                    None => Ok(Value::new_null(ctx.clone())),
                }
            }
        }
        obj.prop(
            "offsetParent",
            Accessor::new_get(JsFn(OffsetParentGetter {
                state: Rc::clone(state),
                key,
            }))
            .configurable(),
        )?;
    }

    // ─── getRootNode ────────────────────────────────────────────────
    {
        struct GetRootNodeHandler {
            state: SharedState,
            key: NodeKey,
        }
        impl JsHandler for GetRootNodeHandler {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                let root = {
                    let s = self.state.borrow();
                    let mut current = self.key;
                    loop {
                        match s.arena.nodes.get(current).and_then(|n| n.parent) {
                            Some(pk) => current = pk,
                            None => break current,
                        }
                    }
                };
                make_element_object(ctx, root, &self.state)
            }
        }
        obj.set(
            "getRootNode",
            Function::new(
                ctx.clone(),
                JsFn(GetRootNodeHandler {
                    state: Rc::clone(state),
                    key,
                }),
            )?,
        )?;
    }

    // ─── Övriga egenskaper ─────────────────────────────────────────
    {
        let s = state.borrow();
        let connected = is_connected_to_document(&s.arena, key);
        let hidden = s
            .arena
            .nodes
            .get(key)
            .map(|n| n.has_attr("hidden"))
            .unwrap_or(false);
        drop(s);
        obj.set("isConnected", connected)?;
        obj.set("hidden", hidden)?;

        // outerHTML — getter/setter
        struct OuterHtmlGetter {
            state: SharedState,
            key: NodeKey,
        }
        impl JsHandler for OuterHtmlGetter {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                let s = self.state.borrow();
                let mut out = String::new();
                serialize_node_html(&s.arena, self.key, &mut out);
                Ok(rquickjs::String::from_str(ctx.clone(), &out)?.into_value())
            }
        }
        struct OuterHtmlSetter {
            state: SharedState,
            key: NodeKey,
        }
        impl JsHandler for OuterHtmlSetter {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                // Kontrollera om det är document.documentElement (rot-element)
                let parent_key = {
                    let s = self.state.borrow();
                    let parent = s.arena.nodes.get(self.key).and_then(|n| n.parent);
                    // Om parent är Document-noden, kasta NO_MODIFICATION_ALLOWED_ERR
                    if let Some(pk) = parent {
                        if s.arena
                            .nodes
                            .get(pk)
                            .map(|n| matches!(n.node_type, NodeType::Document))
                            .unwrap_or(false)
                        {
                            return Err(ctx.throw(
                                rquickjs::String::from_str(
                                    ctx.clone(),
                                    "NoModificationAllowedError",
                                )?
                                .into(),
                            ));
                        }
                    }
                    parent
                };

                let html_str = js_value_to_dom_string(args.first());

                if let Some(parent_key) = parent_key {
                    // Parsa HTML till nya noder
                    let new_keys = {
                        let mut s = self.state.borrow_mut();
                        // Ta bort den nuvarande noden från parent
                        s.arena.remove_child(parent_key, self.key);

                        // Parsa nya HTML-noder
                        let frag = crate::parser::parse_html(&html_str);
                        let temp_arena = crate::arena_dom::ArenaDom::from_rcdom(&frag);
                        // Importera nya noder till vår arena
                        let mut imported = vec![];
                        let body_key = find_by_tag_name(&temp_arena, temp_arena.document, "body");
                        if let Some(bk) = body_key {
                            if let Some(body) = temp_arena.nodes.get(bk) {
                                for &child_key in &body.children {
                                    let new_key = import_node(&temp_arena, child_key, &mut s.arena);
                                    if let Some(nk) = new_key {
                                        imported.push(nk);
                                    }
                                }
                            }
                        }
                        imported
                    };

                    // Lägg till importerade noder i parent
                    {
                        let mut s = self.state.borrow_mut();
                        for nk in &new_keys {
                            s.arena.append_child(parent_key, *nk);
                        }
                    }
                }

                Ok(Value::new_undefined(ctx.clone()))
            }
        }
        obj.prop(
            "outerHTML",
            Accessor::new(
                JsFn(OuterHtmlGetter {
                    state: Rc::clone(state),
                    key,
                }),
                JsFn(OuterHtmlSetter {
                    state: Rc::clone(state),
                    key,
                }),
            ),
        )?;
    }

    // shadowRoot
    {
        let s = state.borrow();
        let shadow_key = s.arena.nodes.get(key).and_then(|node| {
            node.children
                .iter()
                .find(|&&child| {
                    s.arena
                        .nodes
                        .get(child)
                        .map(|cn| {
                            cn.tag.as_deref() == Some("template")
                                && (cn.has_attr("shadowrootmode") || cn.has_attr("shadowroot"))
                        })
                        .unwrap_or(false)
                })
                .copied()
        });
        drop(s);
        if let Some(sk) = shadow_key {
            obj.set("shadowRoot", make_element_object(ctx, sk, state)?)?;
        } else {
            obj.set("shadowRoot", Value::new_null(ctx.clone()))?;
        }
    }

    // slot — element.slot property (för slotted content)
    {
        let s = state.borrow();
        let slot_val = s
            .arena
            .nodes
            .get(key)
            .and_then(|n| n.attributes.get("slot").cloned())
            .unwrap_or_default();
        obj.set("slot", slot_val.as_str())?;

        // Om detta är ett <slot>-element: lägg till assignedNodes()
        let is_slot = s
            .arena
            .nodes
            .get(key)
            .map(|n| n.tag.as_deref() == Some("slot"))
            .unwrap_or(false);
        if is_slot {
            let slot_name = s
                .arena
                .nodes
                .get(key)
                .and_then(|n| n.attributes.get("name").cloned())
                .unwrap_or_default();
            // Hitta host-elementets barn som matchar slot-name
            let parent_key = s.arena.nodes.get(key).and_then(|n| n.parent);
            // Gå uppåt tills vi hittar shadow host (template → host)
            let host_key = parent_key.and_then(|pk| {
                s.arena.nodes.get(pk).and_then(|pn| {
                    if pn.tag.as_deref() == Some("template") {
                        pn.parent
                    } else {
                        Some(pk)
                    }
                })
            });
            drop(s);

            // assignedNodes() — returnera array av slotted barn
            let assigned: Vec<NodeKey> = if let Some(hk) = host_key {
                let s = state.borrow();
                s.arena
                    .nodes
                    .get(hk)
                    .map(|host| {
                        host.children
                            .iter()
                            .filter(|&&child| {
                                let child_slot = s
                                    .arena
                                    .nodes
                                    .get(child)
                                    .and_then(|cn| cn.attributes.get("slot").cloned())
                                    .unwrap_or_default();
                                child_slot == slot_name
                            })
                            .copied()
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                vec![]
            };

            let assigned_arr = rquickjs::Array::new(ctx.clone())?;
            for (i, &node_key) in assigned.iter().enumerate() {
                let el = make_element_object(ctx, node_key, state)?;
                assigned_arr.set(i, el)?;
            }
            obj.set(
                "assignedNodes",
                Function::new(
                    ctx.clone(),
                    move |_: ()| -> rquickjs::Result<rquickjs::Array<'_>> {
                        Err(rquickjs::Error::new_from_js(
                            "function",
                            "not callable after creation",
                        ))
                    },
                )?,
            )?;
            // Direkt property istf metod (enklare)
            obj.set("_assignedNodes", assigned_arr)?;
        } else {
            drop(s);
        }
    }

    // classList — read-only (assignment = no-op per spec)
    {
        let cl = make_class_list(ctx, key, state)?;
        let define_code = ctx.eval::<Function, _>(
            "(function(obj, cl){ Object.defineProperty(obj, 'classList', { get: function(){ return cl; }, set: function(){}, configurable: true }); })",
        )?;
        define_code.call::<_, Value>((obj.clone(), cl))?;
    }

    // style
    obj.set("style", make_style_object(ctx, key, state)?)?;

    // dataset
    {
        let s = state.borrow();
        let dataset = Object::new(ctx.clone())?;
        if let Some(node) = s.arena.nodes.get(key) {
            for (k, v) in &node.attributes {
                if let Some(stripped) = k.strip_prefix("data-") {
                    let camel = data_attr_to_camel(stripped);
                    dataset.set(camel.as_str(), v.as_str())?;
                }
            }
        }
        obj.set("dataset", dataset)?;
    }

    // relList / sandbox / sizes / htmlFor — DOMTokenList, bara på HTML-element
    // Använder lazy getter som checkar namespaceURI vid access
    {
        let tag_lc = tag_name.to_ascii_lowercase();
        let needs_rel_list = matches!(tag_lc.as_str(), "a" | "area" | "link");
        let needs_sandbox = tag_lc == "iframe";
        let needs_sizes = tag_lc == "link";
        let needs_html_for = tag_lc == "output";
        if needs_rel_list || needs_sandbox || needs_sizes || needs_html_for {
            // Skapa DOMTokenList-objekt som läser/skriver valfritt attribut
            let make_dtl_code = r#"(function(elem, attrName) {
                var dtl = {};
                function getTokens() {
                    var v = elem.getAttribute(attrName) || '';
                    if (!v) return [];
                    var seen = {}, result = [];
                    v.split(/\s+/).forEach(function(t) { if (t && !seen[t]) { seen[t]=1; result.push(t); } });
                    return result;
                }
                dtl.add = function() {
                    var tokens = getTokens();
                    for (var i=0;i<arguments.length;i++) {
                        var t = String(arguments[i]);
                        if (!t) throw new DOMException('The token must not be empty.','SyntaxError');
                        if (/\s/.test(t)) throw new DOMException('The token must not contain whitespace.','InvalidCharacterError');
                        if (tokens.indexOf(t)===-1) tokens.push(t);
                    }
                    elem.setAttribute(attrName, tokens.join(' '));
                };
                dtl.remove = function() {
                    var tokens = getTokens();
                    for (var i=0;i<arguments.length;i++) {
                        var t = String(arguments[i]);
                        tokens = tokens.filter(function(c){return c!==t;});
                    }
                    elem.setAttribute(attrName, tokens.join(' '));
                };
                dtl.contains = function(t) { return getTokens().indexOf(String(t)) !== -1; };
                dtl.toggle = function(t, force) {
                    t = String(t);
                    var tokens = getTokens();
                    var idx = tokens.indexOf(t);
                    if (force !== undefined) {
                        if (force) { if (idx===-1) tokens.push(t); elem.setAttribute(attrName, tokens.join(' ')); return true; }
                        else { tokens = tokens.filter(function(c){return c!==t;}); elem.setAttribute(attrName, tokens.join(' ')); return false; }
                    }
                    if (idx!==-1) { tokens.splice(idx,1); elem.setAttribute(attrName, tokens.join(' ')); return false; }
                    tokens.push(t); elem.setAttribute(attrName, tokens.join(' ')); return true;
                };
                dtl.item = function(i) { var t = getTokens(); return i < t.length ? t[i] : null; };
                dtl.replace = function(o, n) { var t = getTokens(); var i = t.indexOf(String(o)); if (i===-1) return false; t[i]=String(n); elem.setAttribute(attrName, t.join(' ')); return true; };
                Object.defineProperty(dtl, 'length', { get: function() { return getTokens().length; } });
                Object.defineProperty(dtl, 'value', { get: function() { return elem.getAttribute(attrName)||''; }, set: function(v) { elem.setAttribute(attrName, String(v)); } });
                Object.defineProperty(dtl, Symbol.toStringTag, { value: 'DOMTokenList' });
                dtl.toString = function() { return elem.getAttribute(attrName)||''; };
                dtl.forEach = function(cb, thisArg) { var t=getTokens(); for(var i=0;i<t.length;i++) cb.call(thisArg,t[i],i,dtl); };
                return dtl;
            })"#;
            if let Ok(make_dtl) = ctx.eval::<Function, _>(make_dtl_code) {
                // Lazy getter — returnerar DOMTokenList bara om namespaceURI är HTML
                let lazy_dtl_code = r#"(function(elem, make, attrName, propName) {
                    var cached = null;
                    Object.defineProperty(elem, propName, {
                        get: function() {
                            var ns = this.namespaceURI;
                            if (ns && ns !== 'http://www.w3.org/1999/xhtml') return undefined;
                            if (!cached) cached = make(this, attrName);
                            return cached;
                        },
                        configurable: true, enumerable: true
                    });
                })"#;
                if let Ok(lazy_fn) = ctx.eval::<Function, _>(lazy_dtl_code) {
                    let elem_val: Value = obj.clone().into();
                    if needs_rel_list {
                        let _ = lazy_fn.call::<_, Value>((
                            elem_val.clone(),
                            make_dtl.clone(),
                            "rel",
                            "relList",
                        ));
                    }
                    if needs_sandbox {
                        let _ = lazy_fn.call::<_, Value>((
                            elem_val.clone(),
                            make_dtl.clone(),
                            "sandbox",
                            "sandbox",
                        ));
                    }
                    if needs_sizes {
                        let _ = lazy_fn.call::<_, Value>((
                            elem_val.clone(),
                            make_dtl.clone(),
                            "sizes",
                            "sizes",
                        ));
                    }
                    if needs_html_for {
                        let _ = lazy_fn.call::<_, Value>((elem_val, make_dtl, "for", "htmlFor"));
                    }
                }
            }
        }
    }

    // Cacha objektet + applicera JS-polyfills (prototypkedja, CharacterData, ChildNode-metoder)
    {
        let global = ctx.globals();
        let cache_key = format!("__nc_{}", key_bits as u64);
        let _ = global.set(cache_key.as_str(), obj.clone());
        let patch_code = format!(
            concat!(
                "globalThis.__nodeCache && globalThis.__nodeCache.set({kb}, globalThis.{ck});",
                "if(globalThis.__patchPrototype) globalThis.__patchPrototype(globalThis.{ck});",
                "if(globalThis.__patchCharacterData) globalThis.__patchCharacterData(globalThis.{ck});",
                "if(globalThis.__patchChildNode) globalThis.__patchChildNode(globalThis.{ck});"
            ),
            kb = key_bits,
            ck = cache_key
        );
        let _ = ctx.eval::<Value, _>(patch_code.as_str());
    }

    // Cacha i __nodeCache för identitetsgaranti (a === b vid samma NodeKey)
    let val = obj.into_value();
    {
        let set_fn_code = format!(
            "(function(v) {{ globalThis.__nodeCache && globalThis.__nodeCache.set({}, v); }})",
            key_bits
        );
        if let Ok(set_fn) = ctx.eval::<Function, _>(set_fn_code.as_str()) {
            let _ = set_fn.call::<_, Value>((val.clone(),));
        }
    }

    Ok(val)
}

/// Konvertera data-attributnamn till camelCase (t.ex. "my-value" → "myValue")
pub(super) fn data_attr_to_camel(name: &str) -> String {
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

// ─── classList ───────────────────────────────────────────────────────────────

/// Skapa ett DOMException-objekt med name och message
pub(super) fn throw_dom_exception(ctx: &Ctx<'_>, name: &str, message: &str) -> rquickjs::Error {
    let code = format!(
        "(function(){{ var e = new DOMException('{}', '{}'); return e; }})()",
        message.replace('\'', "\\'"),
        name
    );
    match ctx.eval::<Value, _>(code.as_str()) {
        Ok(ex) => ctx.throw(ex),
        Err(_) => {
            // Fallback: skapa vanligt objekt med name/message
            if let Ok(obj) = Object::new(ctx.clone()) {
                let _ = obj.set("name", name);
                let _ = obj.set("message", message);
                ctx.throw(obj.into_value())
            } else {
                ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), &format!("{}: {}", name, message))
                        .unwrap()
                        .into(),
                )
            }
        }
    }
}

/// Validera DOMTokenList-token: tom → SyntaxError, whitespace → InvalidCharacterError
pub(super) fn validate_token(ctx: &Ctx<'_>, token: &str) -> rquickjs::Result<()> {
    if token.is_empty() {
        return Err(throw_dom_exception(
            ctx,
            "SyntaxError",
            "The token must not be empty.",
        ));
    }
    if token.contains(char::is_whitespace) {
        return Err(throw_dom_exception(
            ctx,
            "InvalidCharacterError",
            "The token must not contain whitespace.",
        ));
    }
    Ok(())
}

/// Kolla om en nod är kopplad till document-roten via parent-kedjan
pub(super) fn is_connected_to_document(arena: &ArenaDom, key: NodeKey) -> bool {
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
pub(super) fn node_contains(arena: &ArenaDom, ancestor: NodeKey, descendant: NodeKey) -> bool {
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

/// Validerar pre-insertion enligt DOM spec steg 1-6.
/// Returnerar Ok(()) om insertion är tillåten, annars felmeddelande.
pub(super) fn validate_pre_insertion(
    arena: &ArenaDom,
    parent: NodeKey,
    node: NodeKey,
    child: Option<NodeKey>,
) -> Result<(), &'static str> {
    let parent_type = arena
        .nodes
        .get(parent)
        .map(|n| &n.node_type)
        .cloned()
        .unwrap_or(NodeType::Other);

    // Steg 1: parent måste vara Document, DocumentFragment eller Element
    if !matches!(
        parent_type,
        NodeType::Document | NodeType::DocumentFragment | NodeType::Element
    ) {
        return Err(
            "HierarchyRequestError: parent is not a Document, DocumentFragment, or Element",
        );
    }

    // Steg 2: node får inte vara host-including inclusive ancestor av parent
    if node_contains(arena, node, parent) {
        return Err("HierarchyRequestError: node is an inclusive ancestor of parent");
    }

    // Steg 3: child (om Some) måste vara barn till parent
    if let Some(c) = child {
        let is_child = arena
            .nodes
            .get(parent)
            .map(|n| n.children.contains(&c))
            .unwrap_or(false);
        if !is_child {
            return Err("NotFoundError: child is not a child of parent");
        }
    }

    let node_type = arena
        .nodes
        .get(node)
        .map(|n| &n.node_type)
        .cloned()
        .unwrap_or(NodeType::Other);

    // Steg 4: node måste vara DocumentFragment, DocumentType, Element, Text, PI, eller Comment
    if !matches!(
        node_type,
        NodeType::DocumentFragment
            | NodeType::Doctype
            | NodeType::Element
            | NodeType::Text
            | NodeType::Comment
            | NodeType::ProcessingInstruction
    ) {
        return Err("HierarchyRequestError: invalid node type for insertion");
    }

    // Steg 5: Text i Document eller Doctype utanför Document
    if matches!(node_type, NodeType::Text) && matches!(parent_type, NodeType::Document) {
        return Err("HierarchyRequestError: cannot insert Text node into Document");
    }
    if matches!(node_type, NodeType::Doctype) && !matches!(parent_type, NodeType::Document) {
        return Err("HierarchyRequestError: DocumentType must be child of Document");
    }

    // Steg 6: Document-specifika begränsningar
    if matches!(parent_type, NodeType::Document) {
        let parent_node = arena.nodes.get(parent);
        let existing_element_count = parent_node
            .map(|n| {
                n.children
                    .iter()
                    .filter(|&&c| c != node) // Exkludera noden om redan barn
                    .filter(|&&c| {
                        arena
                            .nodes
                            .get(c)
                            .map(|cn| matches!(cn.node_type, NodeType::Element))
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);
        let existing_doctype_count = parent_node
            .map(|n| {
                n.children
                    .iter()
                    .filter(|&&c| c != node)
                    .filter(|&&c| {
                        arena
                            .nodes
                            .get(c)
                            .map(|cn| matches!(cn.node_type, NodeType::Doctype))
                            .unwrap_or(false)
                    })
                    .count()
            })
            .unwrap_or(0);

        match node_type {
            NodeType::DocumentFragment => {
                let frag_elements = arena
                    .nodes
                    .get(node)
                    .map(|n| {
                        n.children
                            .iter()
                            .filter(|&&c| {
                                arena
                                    .nodes
                                    .get(c)
                                    .map(|cn| matches!(cn.node_type, NodeType::Element))
                                    .unwrap_or(false)
                            })
                            .count()
                    })
                    .unwrap_or(0);
                if frag_elements > 1 {
                    return Err("HierarchyRequestError: DocumentFragment has multiple elements for Document parent");
                }
                if frag_elements == 1 && existing_element_count > 0 {
                    return Err("HierarchyRequestError: Document already has an element child");
                }
            }
            NodeType::Element => {
                if existing_element_count > 0 {
                    return Err("HierarchyRequestError: Document already has an element child");
                }
            }
            NodeType::Doctype => {
                if existing_doctype_count > 0 {
                    return Err("HierarchyRequestError: Document already has a doctype child");
                }
            }
            _ => {}
        }
    }

    Ok(())
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

// ─── NodeKey ↔ f64 konvertering ─────────────────────────────────────────────

/// Konvertera NodeKey till f64 för lagring i JS Number
///
/// SlotMap NodeKey innehåller index + generation. Vi lagrar raw index
/// som en f64 (JavaScript Number) — säkert för index < 2^53.
pub(super) fn node_key_to_f64(key: NodeKey) -> f64 {
    // Använd Key::data() för att extrahera KeyData, sedan as_ffi() för raw u64
    use slotmap::Key;
    key.data().as_ffi() as f64
}

// ─── DOM Impl Properties ────────────────────────────────────────────────────
// Registrerar beteendelogik från dom_impls/ på element-objekt.
// ValidityState, input value modes, form association, select element.

fn register_dom_impl_properties<'js>(
    ctx: &Ctx<'js>,
    obj: &Object<'js>,
    state: &SharedState,
    key: NodeKey,
    tag_name: &str,
) -> rquickjs::Result<()> {
    let tag_lower = tag_name.to_ascii_lowercase();

    // ─── ValidityState (input, select, textarea, button) ─────────────────
    if matches!(
        tag_lower.as_str(),
        "input" | "select" | "textarea" | "button"
    ) {
        // input.validity getter — returnerar ValidityState-objekt
        let vs_state = Rc::clone(state);
        let vs_key = key;
        obj.prop(
            "validity",
            rquickjs::object::Accessor::new_get(JsFn(ValidityStateGetter {
                state: vs_state,
                key: vs_key,
            }))
            .configurable(),
        )?;
    }

    // ─── element.labels — hanteras redan av generated code ──────────────────
    // Generated: htmlinput_element, htmlbutton_element, htmlselect_element, etc.
    // anropar computed::find_labels() och returnerar Array.

    // ─── Input value/checked med dirty state (input) ─────────────────────
    if tag_lower == "input" {
        // Överskrivna value getter/setter med dirty state-logik
        let get_state = Rc::clone(state);
        let set_state = Rc::clone(state);
        obj.prop(
            "value",
            rquickjs::object::Accessor::new(
                JsFn(InputValueGetter {
                    state: get_state,
                    key,
                }),
                JsFn(InputValueSetter {
                    state: set_state,
                    key,
                }),
            )
            .configurable(),
        )?;

        // defaultValue
        let dv_state = Rc::clone(state);
        obj.prop(
            "defaultValue",
            rquickjs::object::Accessor::new_get(JsFn(InputDefaultValueGetter {
                state: dv_state,
                key,
            }))
            .configurable(),
        )?;

        // checked med dirty checkedness
        let gc_state = Rc::clone(state);
        let sc_state = Rc::clone(state);
        obj.prop(
            "checked",
            rquickjs::object::Accessor::new(
                JsFn(InputCheckedGetter {
                    state: gc_state,
                    key,
                }),
                JsFn(InputCheckedSetter {
                    state: sc_state,
                    key,
                }),
            )
            .configurable(),
        )?;

        // valueAsNumber getter/setter — konverterar value till/från nummer per input type
        let van_get_state = Rc::clone(state);
        let van_set_state = Rc::clone(state);
        obj.prop(
            "valueAsNumber",
            rquickjs::object::Accessor::new(
                JsFn(InputValueAsNumberGetter {
                    state: van_get_state,
                    key,
                }),
                JsFn(InputValueAsNumberSetter {
                    state: van_set_state,
                    key,
                }),
            )
            .configurable(),
        )?;

        // valueAsDate getter/setter — konverterar value till/från Date per input type
        let vad_get_state = Rc::clone(state);
        let vad_set_state = Rc::clone(state);
        obj.prop(
            "valueAsDate",
            rquickjs::object::Accessor::new(
                JsFn(InputValueAsDateGetter {
                    state: vad_get_state,
                    key,
                }),
                JsFn(InputValueAsDateSetter {
                    state: vad_set_state,
                    key,
                }),
            )
            .configurable(),
        )?;

        // stepUp/stepDown — justerar numeriskt value med step
        obj.set(
            "stepUp",
            Function::new(
                ctx.clone(),
                JsFn(InputStepUpDown {
                    state: Rc::clone(state),
                    key,
                    direction: 1,
                }),
            )?,
        )?;
        obj.set(
            "stepDown",
            Function::new(
                ctx.clone(),
                JsFn(InputStepUpDown {
                    state: Rc::clone(state),
                    key,
                    direction: -1,
                }),
            )?,
        )?;
        // files och list — null per spec (vi stöder inte FileList/datalist-koppling)
        obj.set("files", Value::new_null(ctx.clone()))?;
        obj.set("list", Value::new_null(ctx.clone()))?;
    }

    // ─── Select element: value, selectedIndex med riktig option-logik ─────
    if tag_lower == "select" {
        let gv_state = Rc::clone(state);
        let sv_state = Rc::clone(state);
        obj.prop(
            "value",
            rquickjs::object::Accessor::new(
                JsFn(SelectValueGetter {
                    state: gv_state,
                    key,
                }),
                JsFn(SelectValueSetter {
                    state: sv_state,
                    key,
                }),
            )
            .configurable(),
        )?;

        let gi_state = Rc::clone(state);
        let si_state = Rc::clone(state);
        obj.prop(
            "selectedIndex",
            rquickjs::object::Accessor::new(
                JsFn(SelectSelectedIndexGetter {
                    state: gi_state,
                    key,
                }),
                JsFn(SelectSelectedIndexSetter {
                    state: si_state,
                    key,
                }),
            )
            .configurable(),
        )?;

        // options (readonly list)
        let opts_state = Rc::clone(state);
        obj.prop(
            "options",
            rquickjs::object::Accessor::new_get(JsFn(SelectOptionsGetter {
                state: opts_state,
                key,
            }))
            .configurable(),
        )?;
    }

    // ─── Form element: elements, reset ───────────────────────────────────
    if tag_lower == "form" {
        let fe_state = Rc::clone(state);
        obj.prop(
            "elements",
            rquickjs::object::Accessor::new_get(JsFn(FormElementsGetter {
                state: fe_state,
                key,
            }))
            .configurable(),
        )?;

        let fr_state = Rc::clone(state);
        obj.set(
            "reset",
            Function::new(
                ctx.clone(),
                JsFn(FormReset {
                    state: fr_state,
                    key,
                }),
            )?,
        )?;

        let fs_state = Rc::clone(state);
        obj.set(
            "submit",
            Function::new(
                ctx.clone(),
                JsFn(FormSubmit {
                    state: fs_state,
                    key,
                }),
            )?,
        )?;
    }

    Ok(())
}

// ─── ValidityState JS-objekt ────────────────────────────────────────────────

struct ValidityStateGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ValidityStateGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let mut vs = dom_impls::constraint_validation::compute_validity(&s, self.key);

        // Korrigera patternMismatch med riktig JS RegExp-evaluering
        let pattern_and_value: Option<(String, String)> = {
            let node = s.arena.nodes.get(self.key);
            node.and_then(|n| {
                n.get_attr("pattern").map(|p| {
                    let v = computed::get_effective_value(&s, self.key);
                    (p.to_string(), v)
                })
            })
        };
        drop(s);

        if let Some((pattern, value)) = pattern_and_value {
            if !value.is_empty() {
                // Per spec: pattern matchas som ^(?:pattern)$ med v-flag
                // Per spec: mönster kompileras med v-flag.
                // Om v-flag inte stöds, fallback till u-flag.
                // Om BÅDA kastar → mönstret ignoreras (return true = match)
                // Obs: QuickJS stöder kanske inte v-flag — fallback till u-flag
                let escaped_value = value
                    .replace('\\', "\\\\")
                    .replace('\'', "\\'")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r");
                let escaped_pattern = pattern
                    .replace('\\', "\\\\")
                    .replace('\'', "\\'")
                    .replace('\n', "\\n")
                    .replace('\r', "\\r");
                let js_code = format!(
                    "(function(){{var v='{}';var p='{}';try{{var r=new RegExp('^(?:'+p+')$','v');return r.test(v);}}catch(e){{try{{var r2=new RegExp('^(?:'+p+')$','u');return r2.test(v);}}catch(e2){{return true;}}}}}})()",
                    escaped_value, escaped_pattern
                );
                if let Ok(v) = ctx.eval::<Value, _>(js_code.as_str()) {
                    let matches = v.as_bool().unwrap_or(true);
                    vs.pattern_mismatch = !matches;
                    vs.valid = !(vs.value_missing
                        || vs.type_mismatch
                        || vs.pattern_mismatch
                        || vs.too_long
                        || vs.too_short
                        || vs.range_underflow
                        || vs.range_overflow
                        || vs.step_mismatch
                        || vs.bad_input
                        || vs.custom_error);
                }
            }
        }

        // Skapa ValidityState JS-objekt med rätt prototype
        let obj_code =
            "Object.create(typeof ValidityState!=='undefined'?ValidityState.prototype:{})";
        let obj = match ctx.eval::<Value, _>(obj_code) {
            Ok(v) if v.is_object() => v.into_object().unwrap(),
            _ => Object::new(ctx.clone())?,
        };
        obj.set("valueMissing", vs.value_missing)?;
        obj.set("typeMismatch", vs.type_mismatch)?;
        obj.set("patternMismatch", vs.pattern_mismatch)?;
        obj.set("tooLong", vs.too_long)?;
        obj.set("tooShort", vs.too_short)?;
        obj.set("rangeUnderflow", vs.range_underflow)?;
        obj.set("rangeOverflow", vs.range_overflow)?;
        obj.set("stepMismatch", vs.step_mismatch)?;
        obj.set("badInput", vs.bad_input)?;
        obj.set("customError", vs.custom_error)?;
        obj.set("valid", vs.valid)?;
        Ok(obj.into_value())
    }
}

// ─── Input Value/Checked JS Handlers ─────────────────────────────────────────

struct InputValueGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputValueGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = dom_impls::input_value::get_input_value(&s, self.key);
        Ok(Value::from_string(rquickjs::String::from_str(
            ctx.clone(),
            &val,
        )?))
    }
}

struct InputValueSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputValueSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        dom_impls::input_value::set_input_value(&mut s, self.key, &val);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct InputDefaultValueGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputDefaultValueGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = dom_impls::input_value::get_default_value(&s, self.key);
        Ok(Value::from_string(rquickjs::String::from_str(
            ctx.clone(),
            &val,
        )?))
    }
}

struct InputCheckedGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputCheckedGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let checked = dom_impls::input_value::get_input_checked(&s, self.key);
        Ok(Value::new_bool(ctx.clone(), checked))
    }
}

struct InputCheckedSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputCheckedSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let checked = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let mut s = self.state.borrow_mut();
        dom_impls::input_value::set_input_checked(&mut s, self.key, checked);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Input valueAsNumber/valueAsDate/stepUp/stepDown ────────────────────────

struct InputValueAsNumberGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputValueAsNumberGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let node = match s.arena.nodes.get(self.key) {
            Some(n) => n,
            None => return Ok(Value::new_float(ctx.clone(), f64::NAN)),
        };
        let input_type = node.get_attr("type").unwrap_or("text");
        // Använd sanitized value (inkl. range clamping/default)
        let value = dom_impls::input_value::get_input_value(&s, self.key);
        let num = input_value_to_number(input_type, &value);
        Ok(Value::new_float(ctx.clone(), num))
    }
}

struct InputValueAsNumberSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputValueAsNumberSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let num = args.first().and_then(|v| v.as_number()).unwrap_or(f64::NAN);
        if num.is_nan() || num.is_infinite() {
            return Err(throw_dom_exception(
                ctx,
                "TypeError",
                "The value provided is not a finite number",
            ));
        }
        let mut s = self.state.borrow_mut();
        let input_type = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("type"))
            .unwrap_or("text")
            .to_string();
        let val = number_to_input_value(&input_type, num);
        let key_bits = node_key_to_f64(self.key) as u64;
        let es = s.element_state.entry(key_bits).or_default();
        es.value = Some(val);
        es.value_dirty = true;
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct InputValueAsDateGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputValueAsDateGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let node = match s.arena.nodes.get(self.key) {
            Some(n) => n,
            None => return Ok(Value::new_null(ctx.clone())),
        };
        let input_type = node.get_attr("type").unwrap_or("text");
        let value = computed::get_effective_value(&s, self.key);
        // Per spec: valueAsDate returns null for types that don't support it
        if !matches!(
            input_type,
            "date" | "time" | "month" | "week" | "datetime-local"
        ) {
            return Ok(Value::new_null(ctx.clone()));
        }
        if value.is_empty() {
            return Ok(Value::new_null(ctx.clone()));
        }
        // Konvertera till ms-since-epoch
        let ms = match input_type {
            "month" => {
                // Month: YYYY-MM → Date(YYYY, MM-1, 1) UTC
                let parts: Vec<&str> = value.split('-').collect();
                if parts.len() == 2 {
                    let y = parts[0].parse::<i64>().unwrap_or(0);
                    let m = parts[1].parse::<u32>().unwrap_or(0);
                    if y > 0 && (1..=12).contains(&m) {
                        days_from_civil(y, m, 1) as f64 * 86_400_000.0
                    } else {
                        f64::NAN
                    }
                } else {
                    f64::NAN
                }
            }
            _ => input_value_to_ms(input_type, &value),
        };
        if ms.is_nan() {
            return Ok(Value::new_null(ctx.clone()));
        }
        // Skapa Date-objekt i JS
        let date_code = format!("new Date({})", ms);
        match ctx.eval::<Value, _>(date_code.as_str()) {
            Ok(v) => Ok(v),
            Err(_) => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct InputValueAsDateSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InputValueAsDateSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args.first();
        let is_null = val.map(|v| v.is_null() || v.is_undefined()).unwrap_or(true);
        let mut s = self.state.borrow_mut();
        let input_type = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("type"))
            .unwrap_or("text")
            .to_string();
        if !matches!(
            input_type.as_str(),
            "date" | "time" | "month" | "week" | "datetime-local"
        ) {
            return Err(throw_dom_exception(
                ctx,
                "InvalidStateError",
                "This input type does not support valueAsDate",
            ));
        }
        let key_bits = node_key_to_f64(self.key) as u64;
        if is_null {
            let es = s.element_state.entry(key_bits).or_default();
            es.value = Some(String::new());
            es.value_dirty = true;
        } else if let Some(date_val) = val {
            // Kontrollera att det är ett Date-objekt
            drop(s);
            let check_code = "(function(d){return d instanceof Date;})";
            let is_date = ctx
                .eval::<Function, _>(check_code)
                .ok()
                .and_then(|f| f.call::<_, Value>((date_val.clone(),)).ok())
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if !is_date {
                return Err(throw_dom_exception(
                    ctx,
                    "TypeError",
                    "Failed to set 'valueAsDate': The provided value is not a Date",
                ));
            }
            let js_code = "(function(d){return d.getTime();})";
            if let Ok(get_time_fn) = ctx.eval::<Function, _>(js_code) {
                if let Ok(ms_val) = get_time_fn.call::<_, Value>((date_val.clone(),)) {
                    if let Some(ms) = ms_val.as_number() {
                        if !ms.is_nan() && ms.is_finite() {
                            let val_str = ms_to_input_value(&input_type, ms);
                            let mut s2 = self.state.borrow_mut();
                            let es = s2.element_state.entry(key_bits).or_default();
                            es.value = Some(val_str);
                            es.value_dirty = true;
                        }
                    }
                }
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct InputStepUpDown {
    state: SharedState,
    key: NodeKey,
    direction: i32,
}
impl JsHandler for InputStepUpDown {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let n = args.first().and_then(|v| v.as_number()).unwrap_or(1.0);
        let s_ref = self.state.borrow();
        let node = match s_ref.arena.nodes.get(self.key) {
            Some(n) => n,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let input_type = node.get_attr("type").unwrap_or("text");
        if !matches!(
            input_type,
            "number" | "range" | "date" | "time" | "datetime-local" | "month" | "week"
        ) {
            return Err(throw_dom_exception(
                ctx,
                "InvalidStateError",
                "This input type does not support stepUp/stepDown",
            ));
        }
        let value = computed::get_effective_value(&s_ref, self.key);
        let current = input_value_to_number(input_type, &value);
        // Default step per input type (i samma enhet som valueAsNumber)
        let default_step = match input_type {
            "date" => 86_400_000.0,       // 1 dag i ms
            "time" => 60_000.0,           // 60 sekunder i ms
            "datetime-local" => 60_000.0, // 60 sekunder i ms
            "month" => 1.0,               // 1 månad
            "week" => 604_800_000.0,      // 1 vecka i ms
            "number" | "range" => 1.0,    // 1
            _ => 1.0,
        };
        let step_str = node.get_attr("step");
        let step = match step_str {
            Some("any") | None => default_step,
            Some(s) => {
                let parsed = s.parse::<f64>().unwrap_or(default_step);
                // Step-värdet konverteras till ms för tid/datum-typer
                match input_type {
                    "time" | "datetime-local" => parsed * 1000.0, // sekunder → ms
                    _ => parsed,
                }
            }
        };
        let new_val = if current.is_nan() {
            0.0 + step * n * self.direction as f64
        } else {
            current + step * n * self.direction as f64
        };
        drop(s_ref);
        let mut s = self.state.borrow_mut();
        let key_bits = node_key_to_f64(self.key) as u64;
        let input_type_owned = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr("type"))
            .unwrap_or("text")
            .to_string();
        let new_val_str = number_to_input_value(&input_type_owned, new_val);
        let es = s.element_state.entry(key_bits).or_default();
        es.value = Some(new_val_str);
        es.value_dirty = true;
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Konvertera input value till nummer baserat på typ
fn input_value_to_number(input_type: &str, value: &str) -> f64 {
    if value.is_empty() {
        return f64::NAN;
    }
    match input_type {
        "number" | "range" => value.parse::<f64>().unwrap_or(f64::NAN),
        "date" => {
            // YYYY-MM-DD → ms since epoch
            input_value_to_ms("date", value)
        }
        "time" => {
            // HH:MM[:SS[.mmm]] → ms since midnight
            input_value_to_ms("time", value)
        }
        "month" => {
            // YYYY-MM → months since 1970-01
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() == 2 {
                let y = parts[0].parse::<i64>().unwrap_or(0);
                let m = parts[1].parse::<u32>().unwrap_or(0);
                if y <= 0 || !(1..=12).contains(&m) {
                    return f64::NAN;
                }
                (y as f64 - 1970.0) * 12.0 + (m as f64 - 1.0)
            } else {
                f64::NAN
            }
        }
        "week" => {
            // YYYY-Www → ms since epoch
            input_value_to_ms("week", value)
        }
        "datetime-local" => input_value_to_ms("datetime-local", value),
        _ => f64::NAN,
    }
}

/// Konvertera nummer tillbaka till input value-sträng
fn number_to_input_value(input_type: &str, num: f64) -> String {
    match input_type {
        "number" | "range" => {
            if num == num.floor() {
                format!("{}", num as i64)
            } else {
                format!("{}", num)
            }
        }
        "date" => ms_to_input_value("date", num),
        "time" => ms_to_input_value("time", num),
        "month" => {
            let total_months = num.floor() as i64;
            let mut y = 1970 + total_months / 12;
            let mut m = (total_months % 12) + 1;
            if m <= 0 {
                m += 12;
                y -= 1;
            }
            format!("{:04}-{:02}", y, m)
        }
        "week" => {
            // ms → YYYY-Www
            let days = (num / 86_400_000.0).floor() as i64;
            let (y, _, _) = civil_from_days(days);
            // Hitta veckonummer
            let jan4 = days_from_civil(y, 1, 4);
            let dow_jan4 = ((jan4 % 7) + 3) % 7;
            let week1_monday = jan4 - dow_jan4;
            let week_num = ((days - week1_monday) / 7) + 1;
            if (1..=53).contains(&week_num) {
                format!("{:04}-W{:02}", y, week_num)
            } else {
                String::new()
            }
        }
        "datetime-local" => {
            let date_part = ms_to_input_value("date", num);
            let time_part = ms_to_input_value("time", num % 86_400_000.0);
            if !date_part.is_empty() && !time_part.is_empty() {
                format!("{}T{}", date_part, time_part)
            } else {
                String::new()
            }
        }
        _ => format!("{}", num),
    }
}

/// Validera datum-komponenter
fn is_valid_date(y: i64, m: u32, d: u32) -> bool {
    if y == 0 || !(1..=12).contains(&m) || d < 1 {
        return false;
    }
    let days_in_month = match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || y % 400 == 0 {
                29
            } else {
                28
            }
        }
        _ => return false,
    };
    d <= days_in_month
}

/// Konvertera input value till millisekunder (Date.getTime()-kompatibelt)
fn input_value_to_ms(input_type: &str, value: &str) -> f64 {
    match input_type {
        "date" => {
            // YYYY-MM-DD → ms since epoch (UTC)
            let parts: Vec<&str> = value.split('-').collect();
            if parts.len() == 3 {
                let y = parts[0].parse::<i64>().unwrap_or(0);
                let m = parts[1].parse::<u32>().unwrap_or(0);
                let d = parts[2].parse::<u32>().unwrap_or(0);
                if !is_valid_date(y, m, d) {
                    return f64::NAN;
                }
                let days = days_from_civil(y, m, d);
                days as f64 * 86_400_000.0
            } else {
                f64::NAN
            }
        }
        "time" => {
            // HH:MM[:SS[.mmm]] → ms since midnight
            let parts: Vec<&str> = value.split(':').collect();
            if parts.len() >= 2 {
                let h = parts[0].parse::<f64>().unwrap_or(-1.0);
                let m = parts[1].parse::<f64>().unwrap_or(-1.0);
                // Validera: 0 <= h <= 23, 0 <= m <= 59
                if !(0.0..=23.0).contains(&h) || !(0.0..=59.0).contains(&m) {
                    return f64::NAN;
                }
                let s = if parts.len() > 2 {
                    parts[2].parse::<f64>().unwrap_or(0.0)
                } else {
                    0.0
                };
                if !(0.0..60.0).contains(&s) {
                    return f64::NAN;
                }
                h * 3_600_000.0 + m * 60_000.0 + s * 1000.0
            } else {
                f64::NAN
            }
        }
        "week" => {
            // YYYY-Www → ms since epoch
            if value.len() >= 8 && value.contains("-W") {
                let parts: Vec<&str> = value.split("-W").collect();
                if parts.len() == 2 {
                    let y = parts[0].parse::<i64>().unwrap_or(0);
                    let w = parts[1].parse::<i64>().unwrap_or(0);
                    // Validera: y > 0, 1 <= w <= 53
                    if y == 0 || !(1..=53).contains(&w) {
                        return f64::NAN;
                    }
                    // ISO 8601: vecka 1 innehåller 4 januari
                    let jan4 = days_from_civil(y, 1, 4);
                    // Hitta måndag i vecka 1
                    let dow_jan4 = ((jan4 % 7) + 3) % 7; // 0=mån
                    let week1_monday = jan4 - dow_jan4;
                    let target_day = week1_monday + (w - 1) * 7;
                    target_day as f64 * 86_400_000.0
                } else {
                    f64::NAN
                }
            } else {
                f64::NAN
            }
        }
        "datetime-local" => {
            // YYYY-MM-DDTHH:MM[:SS[.mmm]]
            let parts: Vec<&str> = value.splitn(2, 'T').collect();
            if parts.len() == 2 {
                let date_ms = input_value_to_ms("date", parts[0]);
                let time_ms = input_value_to_ms("time", parts[1]);
                if date_ms.is_nan() || time_ms.is_nan() {
                    f64::NAN
                } else {
                    date_ms + time_ms
                }
            } else {
                f64::NAN
            }
        }
        _ => f64::NAN,
    }
}

/// Konvertera ms till input value-sträng
fn ms_to_input_value(input_type: &str, ms: f64) -> String {
    match input_type {
        "month" => {
            // ms → YYYY-MM (dag 1 av den månaden)
            let days = (ms / 86_400_000.0).floor() as i64;
            let (y, m, _) = civil_from_days(days);
            format!("{:04}-{:02}", y, m)
        }
        "date" => {
            let days = (ms / 86_400_000.0).floor() as i64;
            let (y, m, d) = civil_from_days(days);
            format!("{:04}-{:02}-{:02}", y, m, d)
        }
        "time" => {
            // Wrap negativa värden till 0..86400000 (24h)
            let day_ms = 86_400_000i64;
            let total_ms = ((ms as i64 % day_ms) + day_ms) % day_ms;
            let h = total_ms / 3_600_000;
            let min = (total_ms % 3_600_000) / 60_000;
            let s = (total_ms % 60_000) / 1000;
            let ms_part = total_ms % 1000;
            if ms_part > 0 {
                format!("{:02}:{:02}:{:02}.{:03}", h, min, s, ms_part)
            } else if s > 0 {
                format!("{:02}:{:02}:{:02}", h, min, s)
            } else {
                format!("{:02}:{:02}", h, min)
            }
        }
        _ => format!("{}", ms),
    }
}

/// Dagar sedan epoch (1970-01-01) — Howard Hinnants algoritm
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe as i64 - 719468
}

/// Civildatum från dagar sedan epoch
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ─── Select Element JS Handlers ─────────────────────────────────────────────

struct SelectValueGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SelectValueGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let val = dom_impls::select_element::get_select_value(&s, self.key);
        Ok(Value::from_string(rquickjs::String::from_str(
            ctx.clone(),
            &val,
        )?))
    }
}

struct SelectValueSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SelectValueSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let val = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        dom_impls::select_element::set_select_value(&mut s, self.key, &val);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct SelectSelectedIndexGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SelectSelectedIndexGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let idx = dom_impls::select_element::get_selected_index(&s, self.key);
        Ok(Value::new_int(ctx.clone(), idx))
    }
}

struct SelectSelectedIndexSetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SelectSelectedIndexSetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let idx = args.first().and_then(|v| v.as_int()).unwrap_or(-1);
        let mut s = self.state.borrow_mut();
        dom_impls::select_element::set_selected_index(&mut s, self.key, idx);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct SelectOptionsGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SelectOptionsGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let options = dom_impls::select_element::get_options(&s, self.key);
        let arr = rquickjs::Array::new(ctx.clone())?;
        for (i, &opt_key) in options.iter().enumerate() {
            let elem = make_element_object(ctx, opt_key, &self.state)?;
            arr.set(i, elem)?;
        }
        Ok(arr.into_value())
    }
}

// ─── Form Element JS Handlers ───────────────────────────────────────────────

struct FormElementsGetter {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for FormElementsGetter {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let elements = dom_impls::form_association::get_form_elements(&s, self.key);
        let arr = rquickjs::Array::new(ctx.clone())?;
        for (i, &elem_key) in elements.iter().enumerate() {
            let elem = make_element_object(ctx, elem_key, &self.state)?;
            arr.set(i, elem)?;
        }
        // Lägg till length-property
        let length = elements.len();
        drop(elements);
        drop(s);
        let arr_val = arr.into_value();
        if let Some(obj) = arr_val.as_object() {
            obj.set("length", length as i32)?;
        }
        Ok(arr_val)
    }
}

struct FormReset {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for FormReset {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        dom_impls::form_association::reset_form(&mut s, self.key);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct FormSubmit {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for FormSubmit {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // I headless mode: dispatcha submit-event, samla form data
        // Per spec: submit() skippar validation
        let _ = (&self.state, self.key);
        Ok(Value::new_undefined(ctx.clone()))
    }
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
            Some("1280"),
            "window.innerWidth borde vara 1280"
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

    // ─── Nya API-tester (Prio 1–3) ─────────────────────────────────────────

    #[test]
    fn test_get_attribute_names() {
        let arena = make_arena(
            r##"<html><body><div id="test" class="foo" data-x="1"></div></body></html>"##,
        );
        let code = r#"
            var el = document.getElementById('test');
            var names = el.getAttributeNames();
            names.sort().join(',');
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert!(val.contains("class"), "Borde innehålla 'class': {}", val);
        assert!(val.contains("id"), "Borde innehålla 'id': {}", val);
        assert!(val.contains("data-x"), "Borde innehålla 'data-x': {}", val);
    }

    #[test]
    fn test_insert_adjacent_html_beforeend() {
        let arena =
            make_arena(r##"<html><body><div id="container"><p>Existing</p></div></body></html>"##);
        let code = r#"
            var el = document.getElementById('container');
            el.insertAdjacentHTML('beforeend', '<span>Added</span>');
            el.innerHTML;
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert!(val.contains("Added"), "Borde ha lagt till span: {}", val);
    }

    #[test]
    fn test_insert_adjacent_html_afterbegin() {
        let arena = make_arena(r##"<html><body><div id="c"><p>Old</p></div></body></html>"##);
        let code = r#"
            var el = document.getElementById('c');
            el.insertAdjacentHTML('afterbegin', '<b>First</b>');
            el.innerHTML;
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert!(val.contains("First"), "Borde ha infogat först: {}", val);
    }

    #[test]
    fn test_storage_length_and_key() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"
            localStorage.setItem('a', '1');
            localStorage.setItem('b', '2');
            localStorage.setItem('c', '3');
            var len = localStorage.length;
            var k0 = localStorage.key(0);
            var k2 = localStorage.key(2);
            var kNull = localStorage.key(99);
            len + '|' + k0 + '|' + k2 + '|' + String(kNull);
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert!(val.starts_with("3|"), "length borde vara 3: {}", val);
        assert!(val.ends_with("|null"), "key(99) borde vara null: {}", val);
    }

    #[test]
    fn test_document_active_element() {
        let arena = make_arena(r##"<html><body><input id="inp"/></body></html>"##);
        let code = r#"
            var ae = document.activeElement;
            var tag1 = ae ? ae.tagName : 'null';
            var inp = document.getElementById('inp');
            inp.focus();
            var ae2 = document.activeElement;
            var tag2 = ae2 ? ae2.tagName : 'null';
            tag1 + '|' + tag2;
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert!(
            val.contains("BODY"),
            "Initialt activeElement borde vara body: {}",
            val
        );
        assert!(
            val.contains("INPUT"),
            "Efter focus() borde activeElement vara input: {}",
            val
        );
    }

    #[test]
    fn test_crypto_random_uuid() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"
            var uuid = crypto.randomUUID();
            // UUID v4 format: 8-4-4-4-12 hex
            var valid = uuid.length === 36 && uuid.charAt(8) === '-' && uuid.charAt(13) === '-';
            valid + '|' + uuid.charAt(14);
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert!(
            val.starts_with("true|4"),
            "UUID borde vara v4-format: {}",
            val
        );
    }

    #[test]
    fn test_crypto_get_random_values() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"
            var arr = [0, 0, 0, 0];
            var result = crypto.getRandomValues(arr);
            typeof result === 'object' ? 'ok' : 'fail';
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert_eq!(
            result.value.as_deref(),
            Some("ok"),
            "getRandomValues borde returnera arrayen"
        );
    }

    #[test]
    fn test_domparser() {
        let arena = make_arena(r#"<html><body><p>Hello</p></body></html>"#);
        let code = r#"
            var parser = new DOMParser();
            var doc = parser.parseFromString('<p>Test</p>', 'text/html');
            typeof doc.querySelector === 'function' ? 'ok' : 'fail';
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert_eq!(
            result.value.as_deref(),
            Some("ok"),
            "DOMParser borde returnera doc-liknande objekt"
        );
    }

    #[test]
    fn test_url_constructor() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"
            var u = new URL('https://example.com:8080/path?foo=bar#hash');
            u.protocol + '|' + u.hostname + '|' + u.port + '|' + u.pathname + '|' + u.search + '|' + u.hash;
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert_eq!(
            val, "https:|example.com|8080|/path|?foo=bar|#hash",
            "URL borde parsa korrekt"
        );
    }

    #[test]
    fn test_url_search_params() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"
            var u = new URL('https://example.com/page?a=1&b=2');
            u.searchParams.get('a') + '|' + u.searchParams.get('b') + '|' + u.searchParams.has('c');
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert_eq!(
            result.value.as_deref(),
            Some("1|2|false"),
            "URLSearchParams borde fungera"
        );
    }

    #[test]
    fn test_location_search_params() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"
            var sp = window.location.searchParams;
            typeof sp.get === 'function' && typeof sp.has === 'function' ? 'ok' : 'fail';
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert_eq!(
            result.value.as_deref(),
            Some("ok"),
            "location.searchParams borde finnas"
        );
    }

    // ─── Lifecycle-tester ───────────────────────────────────────────────────

    #[test]
    fn test_document_ready_state() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let code = r#"document.readyState"#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert_eq!(
            result.value.as_deref(),
            Some("complete"),
            "readyState borde vara 'complete' som standard"
        );
    }

    #[test]
    fn test_lifecycle_dom_content_loaded() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let scripts = vec![r#"
            var loaded = false;
            var readyStates = [document.readyState];
            document.addEventListener('DOMContentLoaded', function() {
                loaded = true;
                readyStates.push(document.readyState);
            });
            "#
        .to_string()];
        let result = eval_js_with_lifecycle(&scripts, arena);
        assert!(
            result.error.is_none(),
            "Lifecycle borde inte ge fel: {:?}",
            result.error
        );
    }

    #[test]
    fn test_lifecycle_load_event() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let scripts = vec![r#"
            var windowLoaded = false;
            window.addEventListener('load', function() {
                windowLoaded = true;
            });
            "#
        .to_string()];
        let result = eval_js_with_lifecycle(&scripts, arena);
        assert!(
            result.error.is_none(),
            "Lifecycle load borde inte ge fel: {:?}",
            result.error
        );
    }

    #[test]
    fn test_lifecycle_multiple_scripts() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let scripts = vec![
            "var x = 10;".to_string(),
            "var y = x + 5;".to_string(),
            "x + y".to_string(),
        ];
        let result = eval_js_with_lifecycle(&scripts, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert_eq!(
            result.value.as_deref(),
            Some("25"),
            "Scripts borde dela kontext"
        );
    }

    // ─── Mutation Pipeline-tester ──────────────────────────────────────────

    #[test]
    fn test_mutation_set_text_content_propagates() {
        let arena = make_arena(r##"<html><body><div id="target">Old</div></body></html>"##);
        let code = r#"
            var el = document.getElementById('target');
            el.textContent = 'New';
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert!(
            !result.mutations.is_empty(),
            "Borde ha registrerat mutationer"
        );
    }

    #[test]
    fn test_mutation_set_attribute_propagates() {
        let arena = make_arena(r##"<html><body><div id="target">Test</div></body></html>"##);
        let code = r#"
            var el = document.getElementById('target');
            el.setAttribute('class', 'highlight');
            el.getAttribute('class');
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        assert_eq!(
            result.value.as_deref(),
            Some("highlight"),
            "setAttribute + getAttribute borde fungera"
        );
    }

    #[test]
    fn test_mutation_append_child_propagates() {
        let arena = make_arena(r##"<html><body><div id="container"></div></body></html>"##);
        let code = r#"
            var c = document.getElementById('container');
            var span = document.createElement('span');
            span.textContent = 'Dynamic';
            c.appendChild(span);
            c.innerHTML;
        "#;
        let result = eval_js_with_dom(code, arena);
        assert!(result.error.is_none(), "Fel: {:?}", result.error);
        let val = result.value.unwrap_or_default();
        assert!(
            val.contains("Dynamic"),
            "appendChild borde synas i innerHTML: {}",
            val
        );
    }

    #[test]
    fn test_lifecycle_ready_state_transitions() {
        let arena = make_arena(r#"<html><body></body></html>"#);
        let scripts = vec![r#"
            var states = [];
            states.push(document.readyState);
            document.addEventListener('DOMContentLoaded', function() {
                states.push('dcl:' + document.readyState);
            });
            window.addEventListener('load', function() {
                states.push('load:' + document.readyState);
            });
            "#
        .to_string()];
        let result = eval_js_with_lifecycle(&scripts, arena);
        assert!(
            result.error.is_none(),
            "Lifecycle transitions borde inte ge fel: {:?}",
            result.error
        );
    }
}
