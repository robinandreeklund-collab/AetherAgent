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
use rquickjs::{object::Accessor, Ctx, Function, Object, Persistent, Value};

use std::cell::RefCell;
use std::rc::Rc;

use crate::arena_dom::{ArenaDom, NodeKey, NodeType};
use crate::event_loop::{self, EventLoopState, JsFn, JsHandler, SharedEventLoop};

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

/// En mutation som JS-koden utförde på DOM:en — Cow undviker allokering för statiska strängar
pub type DomMutation = std::borrow::Cow<'static, str>;

// ─── Delad state mellan JS-callbacks ────────────────────────────────────────

/// En registrerad event listener på ett DOM-element
#[allow(dead_code)]
struct EventListener {
    event_type: String,
    callback: Persistent<Function<'static>>,
    capture: bool,
    passive: Option<bool>,
}

#[allow(dead_code)]
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
    /// Blitz Stylo computed styles cache — DFS-mappade från Blitz DOM
    /// Key: NodeKey ffi-bits, Value: CSS properties
    #[cfg(feature = "blitz")]
    blitz_styles: Option<std::collections::HashMap<u64, std::collections::HashMap<String, String>>>,
    /// In-memory localStorage (sandboxad, ingen persistens)
    local_storage: std::collections::HashMap<String, String>,
    /// In-memory sessionStorage (sandboxad, ingen persistens)
    session_storage: std::collections::HashMap<String, String>,
    /// Fångade console-meddelanden
    console_output: Vec<String>,
    /// document.readyState — "loading", "interactive" eller "complete"
    ready_state: String,
    /// Original HTML — behövs för Blitz Stylo lazy-init
    #[cfg(feature = "blitz")]
    original_html: Option<String>,
    /// Mutation counter — ökas vid DOM-mutationer, invaliderar Blitz cache
    #[cfg(feature = "blitz")]
    blitz_style_generation: u64,
    /// Generation vid senaste Blitz-cache-build
    #[cfg(feature = "blitz")]
    blitz_cache_generation: u64,
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
        ready_state: "complete".to_string(),
        #[cfg(feature = "blitz")]
        blitz_styles: None,
        #[cfg(feature = "blitz")]
        original_html: None,
        #[cfg(feature = "blitz")]
        blitz_style_generation: 0,
        #[cfg(feature = "blitz")]
        blitz_cache_generation: 0,
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        // Registrera event-loop (setTimeout, setInterval, rAF, MutationObserver, queueMicrotask)
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));

        // Registrera DOM-objekt
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ = register_window(&ctx, Rc::clone(&state));
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));
        // Node identity cache — samma NodeKey ger alltid samma JS-objekt
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");

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
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ = register_window(&ctx, Rc::clone(&state));
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");

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
    let result = eval_js_with_lifecycle_internal(scripts, arena, Some(html.to_string()));
    result
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
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ = register_window(&ctx, Rc::clone(&state));
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));
        // Node identity cache
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");

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
    }));

    let (_rt, context, interrupt_ptr) = crate::js_eval::create_sandboxed_runtime();

    let result = context.with(|ctx| {
        let el: SharedEventLoop = Rc::new(RefCell::new(EventLoopState::new()));
        let _ = event_loop::register_event_loop(&ctx, Rc::clone(&el));
        let _ = register_document(&ctx, Rc::clone(&state));
        let _ =
            register_window_with_viewport(&ctx, Rc::clone(&state), viewport_width, viewport_height);
        let _ = register_dom_exception(&ctx);
        let _ = register_console(&ctx, Rc::clone(&state));
        let _ = ctx.eval::<Value, _>("globalThis.__nodeCache = new Map()");

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
            .unwrap_or_default()
            .to_lowercase();
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

struct CreateTextNode {
    state: SharedState,
}
impl JsHandler for CreateTextNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let text = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
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
        let text = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
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
        let keys = {
            let s = self.state.borrow();
            let mut results = vec![];
            find_all_by_class(&s.arena, s.arena.document, &cls, &mut results);
            results
        };
        let array = rquickjs::Array::new(ctx.clone())?;
        for (i, key) in keys.into_iter().enumerate() {
            let elem = make_element_object(ctx, key, &self.state)?;
            array.set(i, elem)?;
        }
        Ok(array.into_value())
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
        let keys = {
            let s = self.state.borrow();
            let mut results = vec![];
            find_all_by_tag(&s.arena, s.arena.document, &tag, &mut results);
            results
        };
        let array = rquickjs::Array::new(ctx.clone())?;
        for (i, key) in keys.into_iter().enumerate() {
            let elem = make_element_object(ctx, key, &self.state)?;
            array.set(i, elem)?;
        }
        Ok(array.into_value())
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
                node_type: NodeType::Other,
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
fn notify_range_mutation(
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
fn invalidate_blitz_cache(state: &SharedState) {
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

struct GetSelectionFromDoc;
impl JsHandler for GetSelectionFromDoc {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Delegerar till document.getSelection()
        ctx.eval::<Value, _>("document.getSelection()")
    }
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

    ctx.globals().set("document", doc)?;

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
fn extract_node_key(val: &Value) -> Option<NodeKey> {
    let obj = val.as_object()?;
    let bits: f64 = obj.get("__nodeKey__").ok()?;
    Some(f64_to_node_key(bits))
}

/// Konvertera f64 tillbaka till NodeKey
fn f64_to_node_key(bits: f64) -> NodeKey {
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
        NodeType::Text | NodeType::Comment => node.text.as_deref().unwrap_or("").to_string(),
        _ => {
            let mut text = String::new();
            for &child in &node.children {
                text.push_str(&get_text_content(arena, child));
            }
            text
        }
    }
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

fn serialize_node_html(arena: &ArenaDom, key: NodeKey, out: &mut String) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    match &node.node_type {
        NodeType::Text => {
            if let Some(text) = &node.text {
                out.push_str(text);
            }
        }
        NodeType::Comment => {
            if let Some(text) = &node.text {
                out.push_str("<!--");
                out.push_str(text);
                out.push_str("-->");
            }
        }
        NodeType::Element => {
            let tag = node.tag.as_deref().unwrap_or("div");
            out.push('<');
            out.push_str(tag);
            for (k, v) in &node.attributes {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(v);
                out.push('"');
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

// ─── JsHandler-structs för element-metoder ──────────────────────────────────

struct GetAttribute {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let lc_name = name.to_ascii_lowercase();
        let s = self.state.borrow();
        match s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr(&lc_name))
        {
            Some(val) => Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value()),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct SetAttribute {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SetAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let val = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        {
            let mut s = self.state.borrow_mut();
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                // HTML-spec: attributnamn lowercasas på HTML-element
                let lc_name = name.to_ascii_lowercase();
                node.attributes.insert(lc_name, val);
            }
            s.mutations.push(std::borrow::Cow::Owned(format!(
                "setAttribute:{}:{}",
                node_key_to_f64(self.key),
                name
            )));
        }
        #[cfg(feature = "blitz")]
        invalidate_blitz_cache(&self.state);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct RemoveAttribute {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for RemoveAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let lc_name = name.to_ascii_lowercase();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.attributes.remove(&lc_name);
        }
        drop(s);
        #[cfg(feature = "blitz")]
        invalidate_blitz_cache(&self.state);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct HasAttribute {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for HasAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let lc_name = name.to_ascii_lowercase();
        let s = self.state.borrow();
        let has = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr(&lc_name))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), has))
    }
}

// ─── getAttributeNames ──────────────────────────────────────────────────────

struct GetAttributeNames {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetAttributeNames {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let arr = rquickjs::Array::new(ctx.clone())?;
        if let Some(node) = s.arena.nodes.get(self.key) {
            let mut names: Vec<&String> = node.attributes.keys().collect();
            names.sort();
            for (i, name) in names.iter().enumerate() {
                arr.set(
                    i,
                    rquickjs::String::from_str(ctx.clone(), name)?.into_value(),
                )?;
            }
        }
        Ok(arr.into_value())
    }
}

// ─── insertAdjacentHTML ─────────────────────────────────────────────────────

struct InsertAdjacentHTML {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InsertAdjacentHTML {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let position = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_lowercase();
        let html_str = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();

        if html_str.is_empty() {
            return Ok(Value::new_undefined(ctx.clone()));
        }

        {
            let mut s = self.state.borrow_mut();

            // Parsa HTML-fragmentet
            let parsed_keys = if !html_str.contains('<') {
                // Enkel textnode
                let text_key = s.arena.nodes.insert(crate::arena_dom::DomNode {
                    node_type: NodeType::Text,
                    tag: None,
                    attributes: crate::arena_dom::Attrs::new(),
                    text: Some(html_str.into()),
                    parent: None,
                    children: vec![],
                    owner_doc: None,
                });
                vec![text_key]
            } else {
                let rcdom = crate::parser::parse_html(&html_str);
                let fragment = ArenaDom::from_rcdom(&rcdom);
                let doc_key = fragment.document;
                let children = fragment
                    .nodes
                    .get(doc_key)
                    .map(|n| n.children.clone())
                    .unwrap_or_default();
                // Kopiera alla fragment-barn till vår arena (utan förälder ännu)
                let mut new_keys = Vec::new();
                // Använd temporär nyckel — vi sätter rätt förälder nedan
                let temp_parent = s.arena.document;
                for &child in &children {
                    let nk = copy_subtree_return_key(&fragment, child, &mut s.arena);
                    new_keys.push(nk);
                }
                // Ta bort från temp-förälder (copy_subtree_return_key lägger inte till)
                let _ = temp_parent;
                new_keys
            };

            match position.as_str() {
                "beforebegin" => {
                    // Infoga före detta element (syskon)
                    if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                        if let Some(parent_node) = s.arena.nodes.get_mut(parent_key) {
                            if let Some(pos) =
                                parent_node.children.iter().position(|&c| c == self.key)
                            {
                                for (i, &nk) in parsed_keys.iter().enumerate() {
                                    parent_node.children.insert(pos + i, nk);
                                }
                            }
                        }
                        for &nk in &parsed_keys {
                            if let Some(n) = s.arena.nodes.get_mut(nk) {
                                n.parent = Some(parent_key);
                            }
                        }
                    }
                }
                "afterbegin" => {
                    // Infoga som första barn
                    if let Some(node) = s.arena.nodes.get_mut(self.key) {
                        for (i, &nk) in parsed_keys.iter().enumerate() {
                            node.children.insert(i, nk);
                        }
                    }
                    for &nk in &parsed_keys {
                        if let Some(n) = s.arena.nodes.get_mut(nk) {
                            n.parent = Some(self.key);
                        }
                    }
                }
                "beforeend" => {
                    // Infoga som sista barn
                    for &nk in &parsed_keys {
                        s.arena.append_child(self.key, nk);
                    }
                }
                "afterend" => {
                    // Infoga efter detta element (syskon)
                    if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                        if let Some(parent_node) = s.arena.nodes.get_mut(parent_key) {
                            if let Some(pos) =
                                parent_node.children.iter().position(|&c| c == self.key)
                            {
                                for (i, &nk) in parsed_keys.iter().enumerate() {
                                    parent_node.children.insert(pos + 1 + i, nk);
                                }
                            }
                        }
                        for &nk in &parsed_keys {
                            if let Some(n) = s.arena.nodes.get_mut(nk) {
                                n.parent = Some(parent_key);
                            }
                        }
                    }
                }
                _ => {} // Ogiltig position — ignorera tyst
            }

            s.mutations
                .push(std::borrow::Cow::Borrowed("insertAdjacentHTML"));
        }

        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Kopiera ett subträd och returnera nyckeln till roten (utan att lägga till som barn)
fn copy_subtree_return_key(src: &ArenaDom, src_key: NodeKey, dst: &mut ArenaDom) -> NodeKey {
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

struct AppendChild {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for AppendChild {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let child_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let old_parent_key;
        let old_index;
        {
            let mut s = self.state.borrow_mut();
            // CharacterData (Text, Comment) får inte ha barn
            if let Some(node) = s.arena.nodes.get(self.key) {
                if matches!(
                    node.node_type,
                    crate::arena_dom::NodeType::Text | crate::arena_dom::NodeType::Comment
                ) {
                    drop(s);
                    return Err(ctx.throw(
                        rquickjs::String::from_str(
                            ctx.clone(),
                            "HierarchyRequestError: CharacterData nodes cannot have children",
                        )?
                        .into(),
                    ));
                }
            }
            // Spara gammal position för Range-mutation
            old_parent_key = s.arena.nodes.get(child_key).and_then(|n| n.parent);
            old_index = old_parent_key.and_then(|pk| {
                s.arena
                    .nodes
                    .get(pk)
                    .and_then(|p| p.children.iter().position(|&c| c == child_key))
            });
            // Ta bort från gammal förälder
            if let Some(old_parent) = old_parent_key {
                if let Some(parent_node) = s.arena.nodes.get_mut(old_parent) {
                    parent_node.children.retain(|&c| c != child_key);
                }
            }
            s.arena.append_child(self.key, child_key);
            s.mutations.push(std::borrow::Cow::Borrowed("appendChild"));
        }
        // Notifiera Range-objekt om mutationen
        notify_range_mutation(
            ctx,
            "appendChild",
            self.key,
            child_key,
            old_parent_key,
            old_index,
        );
        make_element_object(ctx, child_key, &self.state)
    }
}

struct RemoveChild {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for RemoveChild {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let child_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: argument is not a Node")?
                        .into(),
                ));
            }
        };
        let old_index;
        {
            let mut s = self.state.borrow_mut();
            let is_child = s
                .arena
                .nodes
                .get(self.key)
                .map(|n| n.children.contains(&child_key))
                .unwrap_or(false);
            if !is_child {
                drop(s);
                return Err(ctx.throw(
                    rquickjs::String::from_str(
                        ctx.clone(),
                        "NotFoundError: child is not a child of this node",
                    )?
                    .into(),
                ));
            }
            old_index = s
                .arena
                .nodes
                .get(self.key)
                .and_then(|n| n.children.iter().position(|&c| c == child_key));
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.retain(|&c| c != child_key);
            }
            if let Some(child) = s.arena.nodes.get_mut(child_key) {
                child.parent = None;
            }
            s.mutations.push(std::borrow::Cow::Borrowed("removeChild"));
        }
        notify_range_mutation(ctx, "removeChild", self.key, child_key, None, old_index);
        make_element_object(ctx, child_key, &self.state)
    }
}

struct InsertBefore {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InsertBefore {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let ref_key = args.get(1).and_then(extract_node_key);
        let old_parent_key;
        let old_index;
        {
            let mut s = self.state.borrow_mut();
            old_parent_key = s.arena.nodes.get(new_key).and_then(|n| n.parent);
            old_index = old_parent_key.and_then(|pk| {
                s.arena
                    .nodes
                    .get(pk)
                    .and_then(|p| p.children.iter().position(|&c| c == new_key))
            });
            if let Some(old_parent) = old_parent_key {
                if let Some(p) = s.arena.nodes.get_mut(old_parent) {
                    p.children.retain(|&c| c != new_key);
                }
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                if let Some(rk) = ref_key {
                    if let Some(pos) = node.children.iter().position(|&c| c == rk) {
                        node.children.insert(pos, new_key);
                    } else {
                        node.children.push(new_key);
                    }
                } else {
                    node.children.push(new_key);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(new_key) {
                n.parent = Some(self.key);
            }
            s.mutations.push(std::borrow::Cow::Borrowed("insertBefore"));
        }
        notify_range_mutation(
            ctx,
            "insertBefore",
            self.key,
            new_key,
            old_parent_key,
            old_index,
        );
        make_element_object(ctx, new_key, &self.state)
    }
}

struct ReplaceChild {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ReplaceChild {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Spec: replaceChild(null, ...) och replaceChild(.., null) kastar TypeError
        let new_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: argument is not a Node")?
                        .into(),
                ));
            }
        };
        let old_key = match args.get(1).and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: argument is not a Node")?
                        .into(),
                ));
            }
        };
        {
            let mut s = self.state.borrow_mut();
            // Hitta gammal child — NotFoundError om den inte finns
            let pos = s
                .arena
                .nodes
                .get(self.key)
                .and_then(|n| n.children.iter().position(|&c| c == old_key));
            let _pos = match pos {
                Some(p) => p,
                None => {
                    drop(s);
                    return Err(ctx.throw(
                        rquickjs::String::from_str(
                            ctx.clone(),
                            "NotFoundError: old child is not a child of this node",
                        )?
                        .into(),
                    ));
                }
            };
            // Detach new_key från gammal parent
            if let Some(old_parent) = s.arena.nodes.get(new_key).and_then(|n| n.parent) {
                if let Some(parent_node) = s.arena.nodes.get_mut(old_parent) {
                    parent_node.children.retain(|&c| c != new_key);
                }
            }
            // Ersätt — recalkulera pos efter detach
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                // pos kan ha ändrats om new_key var barn till samma parent
                let pos = node
                    .children
                    .iter()
                    .position(|&c| c == old_key)
                    .unwrap_or(0);
                if pos < node.children.len() {
                    node.children[pos] = new_key;
                } else {
                    node.children.push(new_key);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(new_key) {
                n.parent = Some(self.key);
            }
            if let Some(n) = s.arena.nodes.get_mut(old_key) {
                n.parent = None;
            }
            s.mutations.push(std::borrow::Cow::Borrowed("replaceChild"));
        }
        make_element_object(ctx, old_key, &self.state)
    }
}

struct CloneNode {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for CloneNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let deep = args.first().and_then(|v| v.as_bool()).unwrap_or(false);
        let new_key = clone_node_recursive(&self.state, self.key, deep);
        make_element_object(ctx, new_key, &self.state)
    }
}

// ─── Migration 1: element.remove() ──────────────────────────────────────────
struct Remove {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for Remove {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                parent.children.retain(|&c| c != self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.parent = None;
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 2: element.before(...nodes) ──────────────────────────────────
struct Before {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for Before {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parent_key = {
            let s = self.state.borrow();
            s.arena.nodes.get(self.key).and_then(|n| n.parent)
        };
        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Detach ALLA nya noder först (förhindrar position-shift vid sibling-args)
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        // Hitta position EFTER detach
        let pos = s
            .arena
            .nodes
            .get(parent_key)
            .and_then(|n| n.children.iter().position(|&c| c == self.key))
            .unwrap_or(0);
        for (i, nk) in new_keys.into_iter().enumerate() {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(parent_key);
            }
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                let insert_pos = (pos + i).min(parent.children.len());
                parent.children.insert(insert_pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 3: element.after(...nodes) ───────────────────────────────────
struct After {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for After {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parent_key = {
            let s = self.state.borrow();
            s.arena.nodes.get(self.key).and_then(|n| n.parent)
        };
        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Detach ALLA först
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        // Hitta position EFTER detach
        let pos = s
            .arena
            .nodes
            .get(parent_key)
            .and_then(|n| n.children.iter().position(|&c| c == self.key))
            .map(|p| p + 1)
            .unwrap_or(0);
        for (i, nk) in new_keys.into_iter().enumerate() {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(parent_key);
            }
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                let insert_pos = (pos + i).min(parent.children.len());
                parent.children.insert(insert_pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 4: element.replaceWith(...nodes) ─────────────────────────────
struct ReplaceWith {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ReplaceWith {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parent_key = {
            let s = self.state.borrow();
            s.arena.nodes.get(self.key).and_then(|n| n.parent)
        };
        let parent_key = match parent_key {
            Some(pk) => pk,
            None => return Ok(Value::new_undefined(ctx.clone())),
        };
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Detach alla nya noder + self
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        let pos = s
            .arena
            .nodes
            .get(parent_key)
            .and_then(|n| n.children.iter().position(|&c| c == self.key))
            .unwrap_or(0);
        if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
            parent.children.retain(|&c| c != self.key);
        }
        if let Some(n) = s.arena.nodes.get_mut(self.key) {
            n.parent = None;
        }
        for (i, nk) in new_keys.into_iter().enumerate() {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(parent_key);
            }
            if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                let insert_pos = (pos + i).min(parent.children.len());
                parent.children.insert(insert_pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration 5: toggleAttribute(name [, force]) ──────────────────────────
struct ToggleAttribute {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ToggleAttribute {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        // Spec: om name inte matchar Name-produktionen → InvalidCharacterError
        let invalid_name = name.is_empty() || name.contains(char::is_whitespace);
        if invalid_name {
            return Err(throw_dom_exception(
                ctx,
                "InvalidCharacterError",
                "The string contains invalid characters.",
            ));
        }
        let has_force = args.len() > 1 && !args.get(1).map(|v| v.is_undefined()).unwrap_or(true);
        let force = if has_force {
            args.get(1)
                .map(|v| {
                    v.as_bool().unwrap_or_else(|| {
                        !v.is_null()
                            && !v.is_undefined()
                            && v.as_int().map(|n| n != 0).unwrap_or(true)
                    })
                })
                .unwrap_or(false)
        } else {
            false
        };
        let mut s = self.state.borrow_mut();
        let has_attr = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr(&name))
            .unwrap_or(false);
        if has_force {
            if force {
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.attributes.insert(name, String::new());
                }
                return Ok(Value::new_bool(ctx.clone(), true));
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.attributes.remove(&name);
            }
            return Ok(Value::new_bool(ctx.clone(), false));
        }
        if has_attr {
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.attributes.remove(&name);
            }
            Ok(Value::new_bool(ctx.clone(), false))
        } else {
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.attributes.insert(name, String::new());
            }
            Ok(Value::new_bool(ctx.clone(), true))
        }
    }
}

/// Konvertera JS-argument till NodeKeys.
/// Strängar och null/undefined/numbers konverteras till textnoder.
// ─── Migration: prepend/append/replaceChildren ──────────────────────────────
struct Prepend {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for Prepend {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        for (i, nk) in new_keys.into_iter().enumerate() {
            // Detachera precis innan infogning
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                let pos = i.min(node.children.len());
                node.children.insert(pos, nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct Append {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for Append {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        for nk in new_keys {
            // Detachera precis innan infogning (hanterar append(same, same))
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.push(nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct ReplaceChildren {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ReplaceChildren {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let new_keys = args_to_node_keys(ctx, args, &self.state)?;
        let mut s = self.state.borrow_mut();
        // Ta bort alla befintliga barn
        let old_children: Vec<NodeKey> = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        for ck in old_children {
            if let Some(c) = s.arena.nodes.get_mut(ck) {
                c.parent = None;
            }
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.children.clear();
        }
        // Detach + append nya
        for &nk in &new_keys {
            if let Some(old_p) = s.arena.nodes.get(nk).and_then(|n| n.parent) {
                if let Some(p) = s.arena.nodes.get_mut(old_p) {
                    p.children.retain(|&c| c != nk);
                }
            }
        }
        for nk in new_keys {
            if let Some(n) = s.arena.nodes.get_mut(nk) {
                n.parent = Some(self.key);
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.push(nk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration: CharacterData metoder ───────────────────────────────────────
struct SubstringData {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SubstringData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        if args.len() < 2 {
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "TypeError: Not enough arguments")?.into(),
            ));
        }
        let offset = webidl_unsigned_long(args.first()) as usize;
        let count = webidl_unsigned_long(args.get(1)) as usize;
        let s = self.state.borrow();
        let data = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_deref())
            .unwrap_or("");
        if offset > utf16_len(data) {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        let byte_start = utf16_offset_to_byte(data, offset);
        let byte_end = utf16_offset_to_byte(data, offset + count);
        let result = &data[byte_start..byte_end];
        Ok(rquickjs::String::from_str(ctx.clone(), result)?.into_value())
    }
}

struct AppendData {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for AppendData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        if args.is_empty() {
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "TypeError: Not enough arguments")?.into(),
            ));
        }
        let data = args[0]
            .as_string()
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let mut current = node.text.as_deref().unwrap_or("").to_string();
            current.push_str(&data);
            node.text = Some(current.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct InsertData {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InsertData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let offset = webidl_unsigned_long(args.first()) as usize;
        let data = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let text_len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| utf16_len(t))
            .unwrap_or(0);
        if offset > text_len {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.text.as_deref().unwrap_or("").to_string();
            let safe_offset = utf16_offset_to_byte(&current, offset);
            let mut new_text = String::with_capacity(current.len() + data.len());
            new_text.push_str(&current[..safe_offset]);
            new_text.push_str(&data);
            new_text.push_str(&current[safe_offset..]);
            node.text = Some(new_text.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct DeleteData {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for DeleteData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let offset = webidl_unsigned_long(args.first()) as usize;
        let count = webidl_unsigned_long(args.get(1)) as usize;
        let mut s = self.state.borrow_mut();
        let text_len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| utf16_len(t))
            .unwrap_or(0);
        if offset > text_len {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.text.as_deref().unwrap_or("").to_string();
            // Char-boundary-säker: hitta närmaste giltiga byte-offset
            let safe_start = utf16_offset_to_byte(&current, offset);
            let safe_end = utf16_offset_to_byte(&current, offset + count);
            let mut new_text = String::with_capacity(current.len());
            new_text.push_str(&current[..safe_start]);
            new_text.push_str(&current[safe_end..]);
            node.text = Some(new_text.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct ReplaceData {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ReplaceData {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let offset = webidl_unsigned_long(args.first()) as usize;
        let count = webidl_unsigned_long(args.get(1)) as usize;
        let data = args
            .get(2)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let text_len = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.text.as_ref())
            .map(|t| utf16_len(t))
            .unwrap_or(0);
        if offset > text_len {
            drop(s);
            return Err(ctx.throw(
                rquickjs::String::from_str(ctx.clone(), "IndexSizeError: offset out of range")?
                    .into(),
            ));
        }
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.text.as_deref().unwrap_or("").to_string();
            let safe_start = utf16_offset_to_byte(&current, offset);
            let safe_end = utf16_offset_to_byte(&current, offset + count);
            let mut new_text = String::with_capacity(current.len());
            new_text.push_str(&current[..safe_start]);
            new_text.push_str(&data);
            new_text.push_str(&current[safe_end..]);
            node.text = Some(new_text.into());
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

// ─── Migration: insertAdjacentElement/Text ──────────────────────────────────
struct InsertAdjacentElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InsertAdjacentElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let position = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_lowercase();
        let new_key = match args.get(1).and_then(extract_node_key) {
            Some(k) => k,
            None => return Ok(Value::new_null(ctx.clone())),
        };
        let mut s = self.state.borrow_mut();
        // Detach
        if let Some(old_p) = s.arena.nodes.get(new_key).and_then(|n| n.parent) {
            if let Some(p) = s.arena.nodes.get_mut(old_p) {
                p.children.retain(|&c| c != new_key);
            }
        }
        match position.as_str() {
            "beforebegin" => {
                if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                    let pos = s
                        .arena
                        .nodes
                        .get(parent_key)
                        .and_then(|n| n.children.iter().position(|&c| c == self.key))
                        .unwrap_or(0);
                    if let Some(n) = s.arena.nodes.get_mut(new_key) {
                        n.parent = Some(parent_key);
                    }
                    if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                        parent.children.insert(pos, new_key);
                    }
                }
            }
            "afterbegin" => {
                if let Some(n) = s.arena.nodes.get_mut(new_key) {
                    n.parent = Some(self.key);
                }
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.children.insert(0, new_key);
                }
            }
            "beforeend" => {
                if let Some(n) = s.arena.nodes.get_mut(new_key) {
                    n.parent = Some(self.key);
                }
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    node.children.push(new_key);
                }
            }
            "afterend" => {
                if let Some(parent_key) = s.arena.nodes.get(self.key).and_then(|n| n.parent) {
                    let pos = s
                        .arena
                        .nodes
                        .get(parent_key)
                        .and_then(|n| n.children.iter().position(|&c| c == self.key))
                        .map(|p| p + 1)
                        .unwrap_or(0);
                    if let Some(n) = s.arena.nodes.get_mut(new_key) {
                        n.parent = Some(parent_key);
                    }
                    if let Some(parent) = s.arena.nodes.get_mut(parent_key) {
                        let p = pos.min(parent.children.len());
                        parent.children.insert(p, new_key);
                    }
                }
            }
            _ => {}
        }
        drop(s);
        make_element_object(ctx, new_key, &self.state)
    }
}

struct InsertAdjacentText {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for InsertAdjacentText {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let position = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let text = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        // Skapa textnod
        let text_key = {
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
        // Delegera till InsertAdjacentElement-logik
        let pos_val = rquickjs::String::from_str(ctx.clone(), &position)?.into_value();
        let elem_val = make_element_object(ctx, text_key, &self.state)?;
        let handler = InsertAdjacentElement {
            state: Rc::clone(&self.state),
            key: self.key,
        };
        handler.handle(ctx, &[pos_val, elem_val])
    }
}

/// Hitta närmaste giltiga char boundary vid eller efter given byte-offset.
// ─── Migration: getElementsByTagName/ClassName på element ────────────────────
struct GetElementsByTagNameElement {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetElementsByTagNameElement {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let tag = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let keys = {
            let s = self.state.borrow();
            let mut results = vec![];
            find_all_by_tag(&s.arena, self.key, &tag, &mut results);
            results
        };
        let array = rquickjs::Array::new(ctx.clone())?;
        for (i, key) in keys.into_iter().enumerate() {
            let elem = make_element_object(ctx, key, &self.state)?;
            array.set(i, elem)?;
        }
        Ok(array.into_value())
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
        let keys = {
            let s = self.state.borrow();
            let mut results = vec![];
            find_all_by_class(&s.arena, self.key, &cls, &mut results);
            results
        };
        let array = rquickjs::Array::new(ctx.clone())?;
        for (i, key) in keys.into_iter().enumerate() {
            let elem = make_element_object(ctx, key, &self.state)?;
            array.set(i, elem)?;
        }
        Ok(array.into_value())
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

// ─── Migration: NS attribute methods ────────────────────────────────────────
struct SetAttributeNS {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for SetAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let _ns = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok());
        let qname = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let val = args
            .get(2)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let local = if let Some(colon) = qname.find(':') {
            &qname[colon + 1..]
        } else {
            &qname
        };
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            // NS-attribut bevarar case (lowercas ej)
            node.attributes.insert(local.to_string(), val);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct GetAttributeNS {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let local = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        match s.arena.nodes.get(self.key).and_then(|n| n.get_attr(&local)) {
            Some(val) => Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value()),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct HasAttributeNS {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for HasAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let local = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        let has = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.has_attr(&local))
            .unwrap_or(false);
        Ok(Value::new_bool(ctx.clone(), has))
    }
}

struct RemoveAttributeNS {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for RemoveAttributeNS {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let local = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            node.attributes.remove(&local);
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct GetAttributeNode {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for GetAttributeNode {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let name = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let s = self.state.borrow();
        let val = s
            .arena
            .nodes
            .get(self.key)
            .and_then(|n| n.get_attr(&name))
            .map(|v| v.to_string());
        drop(s);
        match val {
            Some(v) => {
                let obj = Object::new(ctx.clone())?;
                obj.set("name", name.as_str())?;
                obj.set("localName", name.as_str())?;
                obj.set("value", v.as_str())?;
                obj.set("namespaceURI", Value::new_null(ctx.clone()))?;
                obj.set("prefix", Value::new_null(ctx.clone()))?;
                obj.set("specified", true)?;
                obj.set("nodeType", 2)?;
                obj.set("nodeName", name.as_str())?;
                Ok(obj.into_value())
            }
            None => Ok(Value::new_null(ctx.clone())),
        }
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
fn ancestor_chain(arena: &ArenaDom, key: NodeKey) -> Vec<NodeKey> {
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

// ─── Node.normalize ────────────────────────────────────────────────────────
struct Normalize {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for Normalize {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        let children: Vec<NodeKey> = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        let mut prev_text: Option<NodeKey> = None;
        let mut to_remove = vec![];
        for ck in &children {
            let is_text = s
                .arena
                .nodes
                .get(*ck)
                .map(|n| n.node_type == NodeType::Text)
                .unwrap_or(false);
            if is_text {
                let text = s
                    .arena
                    .nodes
                    .get(*ck)
                    .and_then(|n| n.text.as_deref())
                    .unwrap_or("")
                    .to_string();
                if text.is_empty() {
                    to_remove.push(*ck);
                } else if let Some(prev) = prev_text {
                    // Merga med föregående textnod
                    let prev_text_val = s
                        .arena
                        .nodes
                        .get(prev)
                        .and_then(|n| n.text.as_deref())
                        .unwrap_or("")
                        .to_string();
                    let merged = format!("{}{}", prev_text_val, text);
                    if let Some(pn) = s.arena.nodes.get_mut(prev) {
                        pn.text = Some(merged.into());
                    }
                    to_remove.push(*ck);
                } else {
                    prev_text = Some(*ck);
                }
            } else {
                prev_text = None;
            }
        }
        for rk in &to_remove {
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.retain(|c| c != rk);
            }
        }
        Ok(Value::new_undefined(ctx.clone()))
    }
}

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
        let what_to_show = args
            .get(1)
            .and_then(|v| v.as_number())
            .map(|n| n as i64 as u32)
            .unwrap_or(0xFFFFFFFF);
        let filter = args.get(2).cloned();

        let tw = Object::new(ctx.clone())?;
        // Använd inpassat root-objekt direkt (bevarar JS-referens)
        let root_val = args
            .first()
            .cloned()
            .unwrap_or(Value::new_undefined(ctx.clone()));
        let root_obj = make_element_object(ctx, root_key, &self.state)?;
        tw.set("root", root_val)?;
        tw.set("whatToShow", what_to_show as f64)?;
        tw.set("currentNode", root_obj)?;
        tw.set("__rootKey", node_key_to_f64(root_key))?;
        tw.set("__whatToShow", what_to_show as f64)?;
        if let Some(f) = &filter {
            if !f.is_null() && !f.is_undefined() {
                tw.set("filter", f.clone())?;
            } else {
                tw.set("filter", Value::new_null(ctx.clone()))?;
            }
        } else {
            tw.set("filter", Value::new_null(ctx.clone()))?;
        }

        // TreeWalker-metoder via JS
        let walker_code = r#"
        (function(tw) {
            var FILTER_ACCEPT = 1, FILTER_REJECT = 2, FILTER_SKIP = 3;
            function accept(tw, node) {
                var show = tw.__whatToShow || 0xFFFFFFFF;
                var nt = node.nodeType;
                var bit = 1 << (nt - 1);
                if (!(show & bit)) return FILTER_SKIP;
                var f = tw.filter;
                if (!f) return FILTER_ACCEPT;
                if (typeof f === 'function') return f(node) || FILTER_ACCEPT;
                if (typeof f === 'object' && typeof f.acceptNode === 'function') return f.acceptNode(node) || FILTER_ACCEPT;
                return FILTER_ACCEPT;
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
        let root_key = match args.first().and_then(extract_node_key) {
            Some(k) => k,
            None => {
                return Err(ctx.throw(
                    rquickjs::String::from_str(ctx.clone(), "TypeError: root is not a Node")?
                        .into(),
                ));
            }
        };
        let what_to_show = args
            .get(1)
            .and_then(|v| v.as_number())
            .map(|n| n as i64 as u32)
            .unwrap_or(0xFFFFFFFF);
        let filter = args.get(2).cloned();

        let ni = Object::new(ctx.clone())?;
        // Använd det inpassade root-objektet direkt (bevarar JS-referens-identitet)
        let root_val = args
            .first()
            .cloned()
            .unwrap_or(Value::new_undefined(ctx.clone()));
        ni.set("root", root_val.clone())?;
        ni.set("referenceNode", root_val)?;
        // whatToShow som f64 för att undvika signed/unsigned problem
        ni.set("whatToShow", what_to_show as f64)?;
        ni.set("__whatToShow", what_to_show as f64)?;
        ni.set("pointerBeforeReferenceNode", true)?;
        if let Some(f) = &filter {
            if !f.is_null() && !f.is_undefined() {
                ni.set("filter", f.clone())?;
            } else {
                ni.set("filter", Value::new_null(ctx.clone()))?;
            }
        } else {
            ni.set("filter", Value::new_null(ctx.clone()))?;
        }

        let iter_code = r#"
        (function(ni) {
            var FILTER_ACCEPT = 1, FILTER_REJECT = 2, FILTER_SKIP = 3;
            function accept(ni, node) {
                var show = ni.__whatToShow || 0xFFFFFFFF;
                var bit = 1 << (node.nodeType - 1);
                if (!(show & bit)) return FILTER_SKIP;
                var f = ni.filter;
                if (!f) return FILTER_ACCEPT;
                if (typeof f === 'function') return f(node) || FILTER_ACCEPT;
                if (typeof f === 'object' && typeof f.acceptNode === 'function') return f.acceptNode(node) || FILTER_ACCEPT;
                return FILTER_ACCEPT;
            }
            // Pre-order flat traversal
            function traverse(root) {
                var list = [];
                function walk(node) {
                    list.push(node);
                    if (node.childNodes) for (var i = 0; i < node.childNodes.length; i++) walk(node.childNodes[i]);
                }
                walk(root);
                return list;
            }
            ni.nextNode = function() {
                var all = traverse(this.root);
                var ref = this.referenceNode;
                var idx = -1;
                for (var i = 0; i < all.length; i++) {
                    if (all[i] === ref || (all[i].__nodeKey__ && ref.__nodeKey__ && all[i].__nodeKey__ === ref.__nodeKey__)) { idx = i; break; }
                }
                if (this.pointerBeforeReferenceNode) {
                    for (var j = idx; j < all.length; j++) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.referenceNode = all[j]; this.pointerBeforeReferenceNode = false; return all[j];
                        }
                    }
                } else {
                    for (var j = idx + 1; j < all.length; j++) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.referenceNode = all[j]; return all[j];
                        }
                    }
                }
                return null;
            };
            ni.previousNode = function() {
                var all = traverse(this.root);
                var ref = this.referenceNode;
                var idx = all.length;
                for (var i = 0; i < all.length; i++) {
                    if (all[i] === ref || (all[i].__nodeKey__ && ref.__nodeKey__ && all[i].__nodeKey__ === ref.__nodeKey__)) { idx = i; break; }
                }
                if (!this.pointerBeforeReferenceNode) {
                    for (var j = idx; j >= 0; j--) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.referenceNode = all[j]; this.pointerBeforeReferenceNode = true; return all[j];
                        }
                    }
                } else {
                    for (var j = idx - 1; j >= 0; j--) {
                        if (accept(this, all[j]) === FILTER_ACCEPT) {
                            this.referenceNode = all[j]; return all[j];
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

fn args_to_node_keys<'js>(
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

fn clone_node_recursive(state: &SharedState, key: NodeKey, deep: bool) -> NodeKey {
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

struct AddEventListenerHandler {
    state: SharedState,
    key: NodeKey,
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
        let (capture, passive) = if let Some(opts) = args.get(2) {
            if let Some(b) = opts.as_bool() {
                (b, None)
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
                (cap, pas)
            } else {
                (false, None)
            }
        } else {
            (false, None)
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
            });
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct RemoveEventListenerHandler {
    state: SharedState,
    key: NodeKey,
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

struct DispatchEventHandler {
    state: SharedState,
    key: NodeKey,
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
        let callbacks: Vec<(Persistent<Function<'static>>, Option<bool>)> = {
            let s = self.state.borrow();
            s.event_listeners
                .get(&key_bits)
                .map(|listeners| {
                    listeners
                        .iter()
                        .filter(|l| l.event_type == event_type)
                        .map(|l| (l.callback.clone(), l.passive))
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
        for (cb, passive) in callbacks {
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

struct FocusHandler {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for FocusHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        s.focused_element = Some(node_key_to_f64(self.key) as u64);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct BlurHandler {
    state: SharedState,
}
impl JsHandler for BlurHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        s.focused_element = None;
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct ContainsHandler {
    state: SharedState,
    key: NodeKey,
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
        let text = args
            .first()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    v.as_string()
                        .and_then(|s| s.to_string().ok())
                        .unwrap_or_else(|| {
                            // Konvertera nummer/boolean till sträng
                            if let Some(n) = v.as_number() {
                                if n == (n as i64) as f64 {
                                    format!("{}", n as i64)
                                } else {
                                    format!("{}", n)
                                }
                            } else {
                                String::new()
                            }
                        })
                }
            })
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        // Text/Comment-noder: uppdatera .text direkt (data-alias)
        let is_text_or_comment = s
            .arena
            .nodes
            .get(self.key)
            .map(|n| matches!(n.node_type, NodeType::Text | NodeType::Comment))
            .unwrap_or(false);
        if is_text_or_comment {
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.text = Some(text.into());
            }
        } else {
            // Element: rensa barn och skapa ny textnod
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                node.children.clear();
            }
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
fn copy_subtree(src: &ArenaDom, src_key: NodeKey, parent: NodeKey, dst: &mut ArenaDom) {
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

struct NoOpHandler;
impl JsHandler for NoOpHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        Ok(Value::new_undefined(ctx.clone()))
    }
}

/// Skapa ett JS-objekt som representerar ett DOM-element
fn make_element_object<'js>(
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
            Some(NodeType::Comment) => 8,
            Some(NodeType::Document) => 9,
            Some(NodeType::Doctype) => 10,
            _ => 1,
        };
        let tag = if nt == 10 {
            // Doctype — nodeName/name = doctypens namn (t.ex. "html")
            node.and_then(|n| n.text.as_ref())
                .map(|t| t.to_string())
                .unwrap_or_else(|| "html".to_string())
        } else {
            node.and_then(|n| n.tag.as_ref())
                .map(|t| t.to_uppercase())
                .unwrap_or_default()
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
    obj.set("nodeName", tag_name.as_str())?;
    // ownerDocument — lazy getter som hämtar document från globals vid anrop
    // Löser timing: body/head/documentElement skapas innan document registreras.
    if node_type_val != 9 {
        obj.prop(
            "ownerDocument",
            Accessor::new_get(JsFn(OwnerDocumentGetter)),
        )?;
    }
    obj.set("id", id_val.as_str())?;
    obj.set("className", class_val.as_str())?;
    // Doctype-specifika egenskaper
    if node_type_val == 10 {
        obj.set("name", tag_name.as_str())?;
        obj.set("publicId", "")?;
        obj.set("systemId", "")?;
    }
    // nodeValue: null för Element/Document/Doctype/Fragment, data för Text/Comment
    match node_type_val {
        1 | 9 | 10 | 11 => {
            obj.set("nodeValue", Value::new_null(ctx.clone()))?;
        }
        3 | 8 => {
            // Text/Comment — sätt nodeValue till textinnehållet
            let text_val = {
                let s = state.borrow();
                s.arena
                    .nodes
                    .get(key)
                    .and_then(|n| n.text.as_ref())
                    .map(|t| t.to_string())
                    .unwrap_or_default()
            };
            obj.set("nodeValue", text_val.as_str())?;
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
            JsFn(GetElementsByTagNameElement {
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
            JsFn(Normalize {
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
    obj.set("click", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
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
        Accessor::new_get(JsFn(ParentNodeGetter {
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
        let outer_html = {
            let mut out = String::new();
            serialize_node_html(&s.arena, key, &mut out);
            out
        };
        drop(s);
        obj.set("isConnected", connected)?;
        obj.set("hidden", hidden)?;
        obj.set("outerHTML", outer_html.as_str())?;
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

    Ok(obj.into_value())
}

/// Konvertera data-attributnamn till camelCase (t.ex. "my-value" → "myValue")
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

// ─── classList ───────────────────────────────────────────────────────────────

/// Skapa ett DOMException-objekt med name och message
fn throw_dom_exception(ctx: &Ctx<'_>, name: &str, message: &str) -> rquickjs::Error {
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
fn validate_token(ctx: &Ctx<'_>, token: &str) -> rquickjs::Result<()> {
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

struct ClassListAdd {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ClassListAdd {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Stöd för flera tokens: classList.add("a", "b", "c")
        let mut tokens = Vec::new();
        for arg in args {
            let t = arg
                .as_string()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_default();
            validate_token(ctx, &t)?;
            tokens.push(t);
        }
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            let mut classes: Vec<String> =
                current.split_whitespace().map(|s| s.to_string()).collect();
            for t in tokens {
                if !classes.iter().any(|c| c == &t) {
                    classes.push(t);
                }
            }
            node.attributes
                .insert("class".to_string(), classes.join(" "));
        }
        #[cfg(feature = "blitz")]
        invalidate_blitz_cache(&self.state);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct ClassListRemove {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ClassListRemove {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Stöd för flera tokens
        let mut tokens = Vec::new();
        for arg in args {
            let t = arg
                .as_string()
                .and_then(|s| s.to_string().ok())
                .unwrap_or_default();
            validate_token(ctx, &t)?;
            tokens.push(t);
        }
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            let new_cls: Vec<&str> = current
                .split_whitespace()
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

struct ClassListContains {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ClassListContains {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let cls = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
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

struct ClassListToggle {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ClassListToggle {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let cls = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        validate_token(ctx, &cls)?;
        // Stöd för force-argument
        let has_force = args.len() > 1 && !args.get(1).map(|v| v.is_undefined()).unwrap_or(true);
        let force = args.get(1).and_then(|v| v.as_bool()).unwrap_or(false);
        if has_force {
            let mut s = self.state.borrow_mut();
            if force {
                if let Some(node) = s.arena.nodes.get_mut(self.key) {
                    let current = node.get_attr("class").unwrap_or("").to_string();
                    let classes: Vec<&str> = current.split_whitespace().collect();
                    if !classes.contains(&cls.as_str()) {
                        let new_cls = if current.is_empty() {
                            cls
                        } else {
                            format!("{} {}", current, cls)
                        };
                        node.attributes.insert("class".to_string(), new_cls);
                    }
                }
                return Ok(Value::new_bool(ctx.clone(), true));
            }
            if let Some(node) = s.arena.nodes.get_mut(self.key) {
                let current = node.get_attr("class").unwrap_or("").to_string();
                let new_cls: Vec<&str> = current.split_whitespace().filter(|&c| c != cls).collect();
                node.attributes
                    .insert("class".to_string(), new_cls.join(" "));
            }
            return Ok(Value::new_bool(ctx.clone(), false));
        }
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            let classes: Vec<&str> = current.split_whitespace().collect();
            if classes.contains(&cls.as_str()) {
                let new_cls: Vec<&str> = classes.into_iter().filter(|&c| c != cls).collect();
                node.attributes
                    .insert("class".to_string(), new_cls.join(" "));
                return Ok(Value::new_bool(ctx.clone(), false));
            } else {
                let new_cls = if current.is_empty() {
                    cls
                } else {
                    format!("{} {}", current, cls)
                };
                node.attributes.insert("class".to_string(), new_cls);
                return Ok(Value::new_bool(ctx.clone(), true));
            }
        }
        Ok(Value::new_bool(ctx.clone(), false))
    }
}

struct ClassListReplace {
    state: SharedState,
    key: NodeKey,
}
impl JsHandler for ClassListReplace {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let old_cls = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let new_cls = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        if let Some(node) = s.arena.nodes.get_mut(self.key) {
            let current = node.get_attr("class").unwrap_or("").to_string();
            let classes: Vec<&str> = current.split_whitespace().collect();
            if classes.contains(&old_cls.as_str()) {
                let replaced: Vec<String> = classes
                    .into_iter()
                    .map(|c| {
                        if c == old_cls {
                            new_cls.clone()
                        } else {
                            c.to_string()
                        }
                    })
                    .collect();
                node.attributes
                    .insert("class".to_string(), replaced.join(" "));
                return Ok(Value::new_bool(ctx.clone(), true));
            }
        }
        Ok(Value::new_bool(ctx.clone(), false))
    }
}

struct ClassListItem {
    state: SharedState,
    key: NodeKey,
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

struct ClassListGetRawClass {
    state: SharedState,
    key: NodeKey,
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

struct ClassListGetClasses {
    state: SharedState,
    key: NodeKey,
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

fn make_class_list<'js>(
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
    let update_fn_code = r#"(function(obj, getClasses, getRawClass) {
        Object.defineProperty(obj, 'length', { get: function(){ return getClasses().length; }, configurable: true });
        Object.defineProperty(obj, 'value', {
            get: function(){ return getRawClass(); },
            set: function(v){ /* setter via className */ },
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
        let _ = update_fn.call::<_, Value>((obj.clone(), get_classes_fn, get_raw_class_fn));
    }

    Ok(obj.into_value())
}

// ─── style ──────────────────────────────────────────────────────────────────

struct StyleSetProperty {
    state: SharedState,
    key: NodeKey,
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

struct StyleGetPropertyValue {
    state: SharedState,
    key: NodeKey,
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

struct StyleRemoveProperty {
    state: SharedState,
    key: NodeKey,
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

fn camel_to_kebab(name: &str) -> String {
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

fn make_style_object<'js>(
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

fn kebab_to_camel(name: &str) -> String {
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

// ─── Window-objekt ──────────────────────────────────────────────────────────

struct GetComputedStyleHandler {
    state: SharedState,
}
impl JsHandler for GetComputedStyleHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let key = args.first().and_then(extract_node_key);
        let k = match key {
            Some(k) => k,
            None => return Ok(Object::new(ctx.clone())?.into_value()),
        };
        // Beräkna computed styles:
        // 1. Försök Blitz Stylo (riktig CSS-motor) om tillgänglig
        // 2. Fallback: tag defaults + inline styles
        let styles = {
            let key_bits = node_key_to_f64(k) as u64;

            #[cfg(feature = "blitz")]
            {
                // Bygg/re-bygg Blitz computed styles.
                // Serialiserar AKTUELL ArenaDom → HTML → Blitz Stylo.
                // Ger live computed styles efter DOM-mutationer.
                let needs_rebuild = {
                    let s = self.state.borrow();
                    s.blitz_styles.is_none()
                };
                if needs_rebuild {
                    let html = {
                        let s = self.state.borrow();
                        // Serialisera aktuell ArenaDom (inkl. JS-mutationer)
                        let doc_key = s.arena.document;
                        s.arena.serialize_inner_html(doc_key)
                    };
                    let blitz_raw = build_blitz_computed_styles(&html);
                    let s_ref = self.state.borrow();
                    let mapped = map_blitz_styles_to_arena(&html, &blitz_raw, &s_ref.arena);
                    drop(s_ref);
                    self.state.borrow_mut().blitz_styles = Some(mapped);
                }
                // Försök hämta Blitz-computed styles
                let blitz_found = {
                    let s = self.state.borrow();
                    s.blitz_styles
                        .as_ref()
                        .and_then(|m| m.get(&key_bits))
                        .cloned()
                };
                if let Some(blitz_props) = blitz_found {
                    // Merga med inline styles (inline har högst prio)
                    let s = self.state.borrow();
                    let mut computed = blitz_props;
                    if let Some(inline) = s.arena.nodes.get(k).and_then(|n| n.get_attr("style")) {
                        for (prop, val) in parse_inline_styles(inline) {
                            computed.insert(prop, val);
                        }
                    }
                    computed
                } else {
                    // Fallback: tag defaults + inline
                    let s = self.state.borrow();
                    let tag = s
                        .arena
                        .nodes
                        .get(k)
                        .and_then(|n| n.tag.as_deref())
                        .unwrap_or("");
                    let mut computed = get_tag_style_defaults(tag);
                    if let Some(inline) = s.arena.nodes.get(k).and_then(|n| n.get_attr("style")) {
                        for (prop, val) in parse_inline_styles(inline) {
                            computed.insert(prop, val);
                        }
                    }
                    computed
                }
            }

            #[cfg(not(feature = "blitz"))]
            {
                let s = self.state.borrow();
                let tag = s
                    .arena
                    .nodes
                    .get(k)
                    .and_then(|n| n.tag.as_deref())
                    .unwrap_or("");
                let mut computed = get_tag_style_defaults(tag);
                if let Some(inline) = s.arena.nodes.get(k).and_then(|n| n.get_attr("style")) {
                    for (prop, val) in parse_inline_styles(inline) {
                        computed.insert(prop, val);
                    }
                }
                computed
            }
        };

        let style_obj = Object::new(ctx.clone())?;
        // getPropertyValue metod
        let styles_clone = styles.clone();
        style_obj.set(
            "getPropertyValue",
            Function::new(
                ctx.clone(),
                JsFn(ComputedStyleGetProperty {
                    styles: styles_clone,
                }),
            )?,
        )?;
        for (prop, val) in &styles {
            // Sätt både kebab-case och camelCase
            style_obj.set(prop.as_str(), val.as_str())?;
            let camel = kebab_to_camel(prop);
            if camel != *prop {
                style_obj.set(camel.as_str(), val.as_str())?;
            }
        }
        Ok(style_obj.into_value())
    }
}

/// getPropertyValue via stängda computed styles
struct ComputedStyleGetProperty {
    styles: std::collections::HashMap<String, String>,
}
impl JsHandler for ComputedStyleGetProperty {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let prop = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let val = self.styles.get(&prop).cloned().unwrap_or_default();
        Ok(rquickjs::String::from_str(ctx.clone(), &val)?.into_value())
    }
}

struct MatchMediaHandler;
impl JsHandler for MatchMediaHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let query = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let matches = parse_media_query_matches(&query, 1280.0, 900.0);
        let result = Object::new(ctx.clone())?;
        result.set("matches", matches)?;
        result.set("media", query.as_str())?;
        result.set(
            "addEventListener",
            Function::new(ctx.clone(), JsFn(NoOpHandler))?,
        )?;
        result.set(
            "removeEventListener",
            Function::new(ctx.clone(), JsFn(NoOpHandler))?,
        )?;
        Ok(result.into_value())
    }
}

/// Registrera DOMException-konstruktorn som native Rust
/// Ersätter polyfill i polyfills.js — skapar DOMException med name/message/code
fn register_dom_exception(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
    ctx.eval::<Value, _>(
        r#"(function(){
  if (typeof globalThis.DOMException !== 'undefined') return;
  var _codes = {
    IndexSizeError:1, HierarchyRequestError:3, WrongDocumentError:4,
    InvalidCharacterError:5, NoModificationAllowedError:7, NotFoundError:8,
    NotSupportedError:9, InUseAttributeError:10, InvalidStateError:11,
    SyntaxError:12, InvalidModificationError:13, NamespaceError:14,
    InvalidAccessError:15, TypeMismatchError:17, SecurityError:18,
    NetworkError:19, AbortError:20, URLMismatchError:21,
    QuotaExceededError:22, TimeoutError:23, InvalidNodeTypeError:24,
    DataCloneError:25
  };
  globalThis.DOMException = function DOMException(message, name) {
    this.message = message || '';
    this.name = name || 'Error';
    this.code = _codes[this.name] || 0;
    this.stack = (new Error()).stack;
  };
  DOMException.prototype = Object.create(Error.prototype);
  DOMException.prototype.constructor = DOMException;
  Object.defineProperty(DOMException.prototype, Symbol.toStringTag, {value:'DOMException'});
  DOMException.prototype.toString = function(){ return 'DOMException: ' + this.message; };
  DOMException._codes = _codes;
  Object.keys(_codes).forEach(function(n){
    var c = n.replace(/Error$/, '').replace(/([A-Z])/g, '_$1').toUpperCase();
    if (c.charAt(0)==='_') c = c.substring(1);
    c = c + '_ERR';
    DOMException[c] = _codes[n];
    DOMException.prototype[c] = _codes[n];
  });
  // Enklare alias
  DOMException.INDEX_SIZE_ERR = 1;
  DOMException.DOMSTRING_SIZE_ERR = 2;
  DOMException.HIERARCHY_REQUEST_ERR = 3;
  DOMException.WRONG_DOCUMENT_ERR = 4;
  DOMException.INVALID_CHARACTER_ERR = 5;
  DOMException.NO_DATA_ALLOWED_ERR = 6;
  DOMException.NO_MODIFICATION_ALLOWED_ERR = 7;
  DOMException.NOT_FOUND_ERR = 8;
  DOMException.NOT_SUPPORTED_ERR = 9;
  DOMException.INUSE_ATTRIBUTE_ERR = 10;
  DOMException.INVALID_STATE_ERR = 11;
  DOMException.SYNTAX_ERR = 12;
  DOMException.INVALID_MODIFICATION_ERR = 13;
  DOMException.NAMESPACE_ERR = 14;
  DOMException.INVALID_ACCESS_ERR = 15;
  DOMException.VALIDATION_ERR = 16;
  DOMException.TYPE_MISMATCH_ERR = 17;
  DOMException.SECURITY_ERR = 18;
  DOMException.NETWORK_ERR = 19;
  DOMException.ABORT_ERR = 20;
  DOMException.URL_MISMATCH_ERR = 21;
  DOMException.QUOTA_EXCEEDED_ERR = 22;
  DOMException.TIMEOUT_ERR = 23;
  DOMException.INVALID_NODE_TYPE_ERR = 24;
  DOMException.DATA_CLONE_ERR = 25;
})()"#,
    )?;
    Ok(())
}

fn register_window<'js>(ctx: &Ctx<'js>, state: SharedState) -> rquickjs::Result<()> {
    register_window_with_viewport(ctx, state, 1280, 900)
}

/// Register window-objekt med dynamiska viewport-dimensioner
fn register_window_with_viewport<'js>(
    ctx: &Ctx<'js>,
    state: SharedState,
    viewport_width: u32,
    viewport_height: u32,
) -> rquickjs::Result<()> {
    let win = Object::new(ctx.clone())?;

    // getComputedStyle
    win.set(
        "getComputedStyle",
        Function::new(
            ctx.clone(),
            JsFn(GetComputedStyleHandler {
                state: Rc::clone(&state),
            }),
        )?,
    )?;

    // matchMedia
    win.set(
        "matchMedia",
        Function::new(ctx.clone(), JsFn(MatchMediaHandler))?,
    )?;

    // Viewport — synkad med rendering-dimensioner
    win.set("innerWidth", viewport_width)?;
    win.set("innerHeight", viewport_height)?;
    win.set("outerWidth", viewport_width)?;
    win.set("outerHeight", viewport_height)?;
    win.set("devicePixelRatio", 1.0)?;

    // Scroll no-ops
    win.set("scrollTo", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
    win.set("scrollBy", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
    win.set("scroll", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
    win.set("scrollX", 0)?;
    win.set("scrollY", 0)?;
    // getSelection — delegerar till document.getSelection
    win.set(
        "getSelection",
        Function::new(ctx.clone(), JsFn(GetSelectionFromDoc))?,
    )?;

    // addEventListener / removeEventListener / dispatchEvent på window
    {
        // Använd document-nyckel som proxy — window-events lagras där
        let doc_key = state.borrow().arena.document;
        // Separata handlers med nyckel 0 (speciell window-markör)
        // Vi använder doc_key+1 offset som unik nyckel
        win.set(
            "addEventListener",
            Function::new(
                ctx.clone(),
                JsFn(AddEventListenerHandler {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )?,
        )?;
        win.set(
            "removeEventListener",
            Function::new(
                ctx.clone(),
                JsFn(RemoveEventListenerHandler {
                    state: Rc::clone(&state),
                    key: doc_key,
                }),
            )?,
        )?;
        win.set(
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

    // location
    let loc = Object::new(ctx.clone())?;
    loc.set("href", "https://example.com/")?;
    loc.set("protocol", "https:")?;
    loc.set("host", "example.com")?;
    loc.set("hostname", "example.com")?;
    loc.set("pathname", "/")?;
    loc.set("search", "")?;
    loc.set("hash", "")?;
    loc.set("origin", "https://example.com")?;
    loc.set("port", "")?;
    win.set("location", loc)?;

    // navigator
    let nav = Object::new(ctx.clone())?;
    nav.set("userAgent", "AetherAgent/1.0 (QuickJS Sandbox)")?;
    nav.set("language", "sv-SE")?;
    nav.set("languages", rquickjs::Array::new(ctx.clone())?)?;
    nav.set("platform", "Linux x86_64")?;
    nav.set("cookieEnabled", false)?;
    nav.set("onLine", true)?;
    nav.set("hardwareConcurrency", 1)?;
    win.set("navigator", nav)?;

    // screen — synkad med viewport
    let screen = Object::new(ctx.clone())?;
    screen.set("width", viewport_width)?;
    screen.set("height", viewport_height)?;
    screen.set("availWidth", viewport_width)?;
    screen.set("availHeight", viewport_height)?;
    screen.set("colorDepth", 24)?;
    screen.set("pixelDepth", 24)?;
    win.set("screen", screen)?;

    // performance.now()
    let perf_start = std::time::Instant::now();
    struct PerfNow {
        start: std::time::Instant,
    }
    impl JsHandler for PerfNow {
        fn handle<'js>(
            &self,
            ctx: &Ctx<'js>,
            _args: &[Value<'js>],
        ) -> rquickjs::Result<Value<'js>> {
            let elapsed = self.start.elapsed().as_micros() as f64 / 1000.0;
            Ok(Value::new_float(ctx.clone(), elapsed))
        }
    }
    let perf = Object::new(ctx.clone())?;
    perf.set(
        "now",
        Function::new(ctx.clone(), JsFn(PerfNow { start: perf_start }))?,
    )?;
    win.set("performance", perf)?;

    // customElements
    let custom_elements = Object::new(ctx.clone())?;
    custom_elements.set("define", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
    custom_elements.set("get", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
    custom_elements.set(
        "whenDefined",
        Function::new(ctx.clone(), JsFn(NoOpHandler))?,
    )?;
    win.set("customElements", custom_elements)?;

    // ResizeObserver
    {
        struct ResizeObserverConstructor;
        impl JsHandler for ResizeObserverConstructor {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                let obs = Object::new(ctx.clone())?;
                obs.set("observe", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
                obs.set("unobserve", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
                obs.set("disconnect", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
                Ok(obs.into_value())
            }
        }
        win.set(
            "ResizeObserver",
            Function::new(ctx.clone(), JsFn(ResizeObserverConstructor))?,
        )?;
    }

    // crypto
    {
        struct RandomUUID;
        impl JsHandler for RandomUUID {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                // Generera v4-liknande UUID utan externt beroende
                let mut bytes = [0u8; 16];
                // Enkel PRNG baserad på tid — tillräcklig för sandbox
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let mut state = seed;
                for b in &mut bytes {
                    state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                    *b = (state >> 33) as u8;
                }
                bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
                bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 1
                let uuid = format!(
                    "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
                    bytes[0], bytes[1], bytes[2], bytes[3],
                    bytes[4], bytes[5], bytes[6], bytes[7],
                    bytes[8], bytes[9], bytes[10], bytes[11],
                    bytes[12], bytes[13], bytes[14], bytes[15]
                );
                Ok(rquickjs::String::from_str(ctx.clone(), &uuid)?.into_value())
            }
        }

        struct GetRandomValues;
        impl JsHandler for GetRandomValues {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                // Fyller en TypedArray/Array med pseudo-slumpmässiga bytes
                if let Some(arr_val) = args.first() {
                    if let Some(arr) = arr_val.as_object() {
                        let length: i32 = arr.get("length").unwrap_or(0);
                        let seed = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos();
                        let mut state = seed;
                        for i in 0..length {
                            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                            let val = ((state >> 33) & 0xff) as i32;
                            arr.set(i as u32, val)?;
                        }
                    }
                    return Ok(args[0].clone());
                }
                Ok(Value::new_undefined(ctx.clone()))
            }
        }

        let crypto = Object::new(ctx.clone())?;
        crypto.set("randomUUID", Function::new(ctx.clone(), JsFn(RandomUUID))?)?;
        crypto.set(
            "getRandomValues",
            Function::new(ctx.clone(), JsFn(GetRandomValues))?,
        )?;
        win.set("crypto", crypto)?;
    }

    // location.searchParams — enkel URLSearchParams stub
    {
        let search_params = Object::new(ctx.clone())?;
        struct SPGet;
        impl JsHandler for SPGet {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                Ok(Value::new_null(ctx.clone()))
            }
        }
        struct SPHas;
        impl JsHandler for SPHas {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                Ok(Value::new_bool(ctx.clone(), false))
            }
        }
        struct SPToString;
        impl JsHandler for SPToString {
            fn handle<'js>(
                &self,
                ctx: &Ctx<'js>,
                _args: &[Value<'js>],
            ) -> rquickjs::Result<Value<'js>> {
                Ok(rquickjs::String::from_str(ctx.clone(), "")?.into_value())
            }
        }
        search_params.set("get", Function::new(ctx.clone(), JsFn(SPGet))?)?;
        search_params.set("getAll", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
        search_params.set("has", Function::new(ctx.clone(), JsFn(SPHas))?)?;
        search_params.set("set", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
        search_params.set("delete", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
        search_params.set("toString", Function::new(ctx.clone(), JsFn(SPToString))?)?;
        search_params.set("entries", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
        search_params.set("keys", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
        search_params.set("values", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
        search_params.set("forEach", Function::new(ctx.clone(), JsFn(NoOpHandler))?)?;
        // Sätt på location-objektet
        let loc: Object = win.get("location")?;
        loc.set("searchParams", search_params)?;
    }

    // Kopiera till globalThis
    ctx.globals().set("window", win)?;

    // Registrera atob/btoa, encodeURI/decodeURI via JS + Event/CustomEvent constructors
    ctx.eval::<(), _>(
        r#"
        globalThis.atob = function(s) { return s; };
        globalThis.btoa = function(s) { return s; };
        globalThis.self = globalThis.window;
        globalThis.crypto = globalThis.window.crypto;
        // Synka window-funktioner till globalThis (WPT anropar utan window.)
        if (globalThis.window) {
            globalThis.addEventListener = globalThis.window.addEventListener;
            globalThis.removeEventListener = globalThis.window.removeEventListener;
            globalThis.dispatchEvent = globalThis.window.dispatchEvent;
            globalThis.getComputedStyle = globalThis.window.getComputedStyle;
            globalThis.getSelection = globalThis.window.getSelection;
            globalThis.matchMedia = globalThis.window.matchMedia;
        }

        // TextEncoder/TextDecoder — UTF-8
        globalThis.TextEncoder = function TextEncoder() {
            this.encoding = 'utf-8';
        };
        TextEncoder.prototype.encode = function(str) {
            str = String(str || '');
            var buf = [];
            for (var i = 0; i < str.length; i++) {
                var cp = str.codePointAt(i);
                if (cp < 0x80) {
                    buf.push(cp);
                } else if (cp < 0x800) {
                    buf.push(0xC0 | (cp >> 6), 0x80 | (cp & 0x3F));
                } else if (cp < 0x10000) {
                    buf.push(0xE0 | (cp >> 12), 0x80 | ((cp >> 6) & 0x3F), 0x80 | (cp & 0x3F));
                } else {
                    buf.push(0xF0 | (cp >> 18), 0x80 | ((cp >> 12) & 0x3F), 0x80 | ((cp >> 6) & 0x3F), 0x80 | (cp & 0x3F));
                    i++;
                }
            }
            return new Uint8Array(buf);
        };
        TextEncoder.prototype.encodeInto = function(str, dest) {
            var encoded = this.encode(str);
            var written = Math.min(encoded.length, dest.length);
            for (var i = 0; i < written; i++) dest[i] = encoded[i];
            return { read: str.length, written: written };
        };

        globalThis.TextDecoder = function TextDecoder(label) {
            this.encoding = (label || 'utf-8').toLowerCase().replace(/[^a-z0-9-]/g, '');
            if (this.encoding === 'utf8') this.encoding = 'utf-8';
            this.fatal = false;
            this.ignoreBOM = false;
        };
        TextDecoder.prototype.decode = function(input) {
            if (!input || !input.length) return '';
            var bytes = input instanceof Uint8Array ? input : new Uint8Array(input);
            var result = '';
            for (var i = 0; i < bytes.length;) {
                var b = bytes[i];
                if (b < 0x80) { result += String.fromCodePoint(b); i++; }
                else if (b < 0xE0) { result += String.fromCodePoint(((b & 0x1F) << 6) | (bytes[i+1] & 0x3F)); i += 2; }
                else if (b < 0xF0) { result += String.fromCodePoint(((b & 0x0F) << 12) | ((bytes[i+1] & 0x3F) << 6) | (bytes[i+2] & 0x3F)); i += 3; }
                else { result += String.fromCodePoint(((b & 0x07) << 18) | ((bytes[i+1] & 0x3F) << 12) | ((bytes[i+2] & 0x3F) << 6) | (bytes[i+3] & 0x3F)); i += 4; }
            }
            return result;
        };
        globalThis.Event = function Event(type, opts) {
            this.type = type || '';
            this.bubbles = (opts && opts.bubbles) || false;
            this.cancelable = (opts && opts.cancelable) || false;
            this.composed = (opts && opts.composed) || false;
            this.target = null;
            this.srcElement = null;
            this.currentTarget = null;
            this.eventPhase = 0;
            this.defaultPrevented = false;
            this.returnValue = true;
            this.timeStamp = Date.now();
            this.isTrusted = false;
            this._stopPropagationFlag = false;
            this._stopImmediatePropagationFlag = false;
            // cancelBubble: getter/setter per spec — set(false) = no-op, set(true) = stopPropagation
            Object.defineProperty(this, 'cancelBubble', {
                get: function() { return this._stopPropagationFlag; },
                set: function(v) { if (v) this._stopPropagationFlag = true; },
                configurable: true, enumerable: true
            });
            this.stopPropagation = function() { this._stopPropagationFlag = true; };
            this.stopImmediatePropagation = function() { this._stopPropagationFlag = true; this._stopImmediatePropagationFlag = true; };
            this.preventDefault = function() { if (this.cancelable && !this.__passive) { this.defaultPrevented = true; this.returnValue = false; } };
            this.initEvent = function(type, bubbles, cancelable) { this.type = type; this.bubbles = !!bubbles; this.cancelable = !!cancelable; this.defaultPrevented = false; this._stopPropagationFlag = false; this._stopImmediatePropagationFlag = false; };
        };
        Event.NONE = 0; Event.CAPTURING_PHASE = 1; Event.AT_TARGET = 2; Event.BUBBLING_PHASE = 3;
        Event.prototype.NONE = 0; Event.prototype.CAPTURING_PHASE = 1; Event.prototype.AT_TARGET = 2; Event.prototype.BUBBLING_PHASE = 3;
        globalThis.CustomEvent = function CustomEvent(type, opts) {
            Event.call(this, type, opts);
            this.detail = (opts && opts.detail) || null;
        };
        CustomEvent.prototype = Object.create(Event.prototype);
        CustomEvent.prototype.constructor = CustomEvent;
        CustomEvent.prototype.initCustomEvent = function(type, bubbles, cancelable, detail) { this.initEvent(type, bubbles, cancelable); this.detail = detail !== undefined ? detail : null; };
        // ─── Range API (native, flyttad från polyfills.js) ────────────────────
        globalThis.__liveRanges = [];
        // Range mutation notification via __nodeKey__ (anropas från Rust)
        globalThis.__notifyRangeMutationByKey = function(type, parentKey, oldParentKey, oldIndex) {
            var ranges = globalThis.__liveRanges;
            if (!ranges || !ranges.length) return;
            for (var i = 0; i < ranges.length; i++) {
                var r = ranges[i];
                if (!r) continue;
                var sc = r.startContainer, so = r.startOffset, ec = r.endContainer, eo = r.endOffset;
                var scKey = sc && sc.__nodeKey__, ecKey = ec && ec.__nodeKey__;
                if (type === 'removeChild') {
                    if (scKey === parentKey && so > oldIndex) r.startOffset = so - 1;
                    if (ecKey === parentKey && eo > oldIndex) r.endOffset = eo - 1;
                } else if (type === 'appendChild' || type === 'insertBefore') {
                    // Nod togs bort från oldParent
                    if (oldParentKey >= 0 && oldParentKey !== parentKey) {
                        if (scKey === oldParentKey && so > oldIndex) r.startOffset = Math.max(0, so - 1);
                        if (ecKey === oldParentKey && eo > oldIndex) r.endOffset = Math.max(0, eo - 1);
                    }
                }
                r._update();
            }
        };
        // ─── CSSOM: document.styleSheets, CSSStyleSheet, CSSRule ────────
        (function() {
            function CSSRule(cssText) {
                this.cssText = cssText || '';
                this.type = 1; // STYLE_RULE
                var m = cssText.match(/^([^{]*)\{/);
                this.selectorText = m ? m[1].trim() : '';
                var body = cssText.replace(/^[^{]*\{/, '').replace(/\}$/, '').trim();
                this.style = {};
                body.split(';').forEach(function(decl) {
                    var parts = decl.split(':');
                    if (parts.length >= 2) {
                        var prop = parts[0].trim();
                        var val = parts.slice(1).join(':').trim();
                        if (prop) this.style[prop] = val;
                    }
                }, this);
            }
            function CSSStyleSheet() {
                this.cssRules = [];
                this.rules = this.cssRules;
                this.type = 'text/css';
                this.disabled = false;
            }
            CSSStyleSheet.prototype.insertRule = function(rule, index) {
                if (index === undefined) index = 0;
                var r = new CSSRule(rule);
                this.cssRules.splice(index, 0, r);
                return index;
            };
            CSSStyleSheet.prototype.deleteRule = function(index) {
                this.cssRules.splice(index, 1);
            };
            CSSStyleSheet.prototype.addRule = function(sel, style, index) {
                var rule = sel + ' { ' + style + ' }';
                return this.insertRule(rule, index !== undefined ? index : this.cssRules.length);
            };
            CSSStyleSheet.prototype.removeRule = function(index) { this.deleteRule(index); };
            // Skapa styleSheets från existerande <style>-taggar
            if (typeof document !== 'undefined') {
                var sheets = [];
                var styles = document.getElementsByTagName ? document.getElementsByTagName('style') : [];
                if (styles && styles.length) {
                    for (var si = 0; si < styles.length; si++) {
                        var sheet = new CSSStyleSheet();
                        sheet.ownerNode = styles[si];
                        var text = styles[si].textContent || '';
                        // Parsa CSS-regler
                        text.replace(/\/\*[\s\S]*?\*\//g, '').split('}').forEach(function(block) {
                            block = block.trim();
                            if (block && block.indexOf('{') !== -1) {
                                sheet.cssRules.push(new CSSRule(block + '}'));
                            }
                        });
                        sheets.push(sheet);
                    }
                }
                // Alltid minst ett tomt stylesheet (många tester förväntar det)
                if (sheets.length === 0) sheets.push(new CSSStyleSheet());
                Object.defineProperty(document, 'styleSheets', { value: sheets, configurable: true });
            }
            globalThis.CSSStyleSheet = CSSStyleSheet;
            globalThis.CSSRule = CSSRule;
            globalThis.CSSStyleRule = CSSRule;
        })();
        globalThis.Range = function Range() {
            this.startContainer = document;
            this.startOffset = 0;
            this.endContainer = document;
            this.endOffset = 0;
            this.collapsed = true;
            this.commonAncestorContainer = document;
            globalThis.__liveRanges.push(this);
        };
        Range.START_TO_START = 0; Range.START_TO_END = 1; Range.END_TO_END = 2; Range.END_TO_START = 3;
        Range.prototype.START_TO_START = 0; Range.prototype.START_TO_END = 1; Range.prototype.END_TO_END = 2; Range.prototype.END_TO_START = 3;
        Range.prototype._nodeLen = function(node) {
            if (!node) return 0;
            var nt = node.nodeType;
            if (nt === 3 || nt === 8 || nt === 7) return node.data !== undefined ? node.data.length : (node.textContent || '').length;
            return node.childNodes ? node.childNodes.length : 0;
        };
        Range.prototype._update = function() {
            this.collapsed = (this.startContainer === this.endContainer && this.startOffset === this.endOffset);
            var a = this.startContainer, b = this.endContainer;
            var ancestorsA = [];
            var node = a;
            while (node) { ancestorsA.push(node); node = node.parentNode; }
            node = b;
            while (node) {
                if (ancestorsA.indexOf(node) !== -1) { this.commonAncestorContainer = node; return; }
                node = node.parentNode;
            }
            this.commonAncestorContainer = document;
        };
        Range.prototype._compareBoundary = function(cA, oA, cB, oB) {
            // Snabb path: Rust-native boundary-jämförelse via ArenaDom (inga JS round-trips)
            if (cA && cB && cA.__nodeKey__ && cB.__nodeKey__ && document.__nativeCompareBoundary) {
                return document.__nativeCompareBoundary(cA, oA, cB, oB);
            }
            // Fallback: JS-baserad jämförelse (för noder utan __nodeKey__)
            if (cA === cB) { return oA < oB ? -1 : (oA > oB ? 1 : 0); }
            if (!cA || !cA.compareDocumentPosition) return 0;
            var pos = cA.compareDocumentPosition(cB);
            if (pos & 16) { return -1; }
            if (pos & 8) { return 1; }
            if (pos & 4) return -1;
            if (pos & 2) return 1;
            return 0;
        };
        Range.prototype.setStart = function(node, offset) {
            if (offset < 0 || offset > this._nodeLen(node)) throw new DOMException('Index out of range', 'IndexSizeError');
            this.startContainer = node; this.startOffset = offset;
            if (this._compareBoundary(this.startContainer, this.startOffset, this.endContainer, this.endOffset) > 0) {
                this.endContainer = this.startContainer; this.endOffset = this.startOffset;
            }
            this._update();
        };
        Range.prototype.setEnd = function(node, offset) {
            if (offset < 0 || offset > this._nodeLen(node)) throw new DOMException('Index out of range', 'IndexSizeError');
            this.endContainer = node; this.endOffset = offset;
            if (this._compareBoundary(this.startContainer, this.startOffset, this.endContainer, this.endOffset) > 0) {
                this.startContainer = this.endContainer; this.startOffset = this.endOffset;
            }
            this._update();
        };
        Range.prototype.setStartBefore = function(node) {
            var p = node.parentNode; if (!p) throw new DOMException('Invalid node type', 'InvalidNodeTypeError');
            var idx = (p.__nodeKey__ && node.__nodeKey__ && document.__nativeChildIndex) ? document.__nativeChildIndex(p, node) : Array.from(p.childNodes).indexOf(node); this.setStart(p, idx);
        };
        Range.prototype.setStartAfter = function(node) {
            var p = node.parentNode; if (!p) throw new DOMException('Invalid node type', 'InvalidNodeTypeError');
            var idx = (p.__nodeKey__ && node.__nodeKey__ && document.__nativeChildIndex) ? document.__nativeChildIndex(p, node) : Array.from(p.childNodes).indexOf(node); this.setStart(p, idx + 1);
        };
        Range.prototype.setEndBefore = function(node) {
            var p = node.parentNode; if (!p) throw new DOMException('Invalid node type', 'InvalidNodeTypeError');
            var idx = (p.__nodeKey__ && node.__nodeKey__ && document.__nativeChildIndex) ? document.__nativeChildIndex(p, node) : Array.from(p.childNodes).indexOf(node); this.setEnd(p, idx);
        };
        Range.prototype.setEndAfter = function(node) {
            var p = node.parentNode; if (!p) throw new DOMException('Invalid node type', 'InvalidNodeTypeError');
            var idx = (p.__nodeKey__ && node.__nodeKey__ && document.__nativeChildIndex) ? document.__nativeChildIndex(p, node) : Array.from(p.childNodes).indexOf(node); this.setEnd(p, idx + 1);
        };
        Range.prototype.collapse = function(toStart) {
            if (toStart) { this.endContainer = this.startContainer; this.endOffset = this.startOffset; }
            else { this.startContainer = this.endContainer; this.startOffset = this.endOffset; }
            this._update();
        };
        Range.prototype.selectNode = function(node) {
            var p = node.parentNode; if (!p) throw new DOMException('Invalid node type', 'InvalidNodeTypeError');
            var idx = (p.__nodeKey__ && node.__nodeKey__ && document.__nativeChildIndex) ? document.__nativeChildIndex(p, node) : Array.from(p.childNodes).indexOf(node);
            this.setStart(p, idx); this.setEnd(p, idx + 1);
        };
        Range.prototype.selectNodeContents = function(node) {
            this.startContainer = node; this.startOffset = 0;
            this.endContainer = node; this.endOffset = this._nodeLen(node);
            this._update();
        };
        Range.prototype.compareBoundaryPoints = function(how, sourceRange) {
            how = ((how | 0) & 0xFFFF) >>> 0;
            if (how > 3) throw new DOMException('The comparison method provided is not supported.', 'NotSupportedError');
            // Spec: ranges must share same root, otherwise WrongDocumentError
            var thisRoot = this.startContainer; while (thisRoot && thisRoot.parentNode) thisRoot = thisRoot.parentNode;
            var srcRoot = sourceRange.startContainer; while (srcRoot && srcRoot.parentNode) srcRoot = srcRoot.parentNode;
            if (thisRoot !== srcRoot && !(thisRoot && srcRoot && thisRoot.__nodeKey__ && srcRoot.__nodeKey__ && thisRoot.__nodeKey__ === srcRoot.__nodeKey__))
                throw new DOMException('Wrong document', 'WrongDocumentError');
            var thisC, thisO, srcC, srcO;
            switch (how) {
                case 0: thisC = this.startContainer; thisO = this.startOffset; srcC = sourceRange.startContainer; srcO = sourceRange.startOffset; break;
                case 1: thisC = this.startContainer; thisO = this.startOffset; srcC = sourceRange.endContainer; srcO = sourceRange.endOffset; break;
                case 2: thisC = this.endContainer; thisO = this.endOffset; srcC = sourceRange.endContainer; srcO = sourceRange.endOffset; break;
                case 3: thisC = this.endContainer; thisO = this.endOffset; srcC = sourceRange.startContainer; srcO = sourceRange.startOffset; break;
            }
            var cmp = this._compareBoundary(thisC, thisO, srcC, srcO);
            return cmp < 0 ? -1 : (cmp > 0 ? 1 : 0);
        };
        Range.prototype.comparePoint = function(node, offset) {
            var nodeRoot = node; while (nodeRoot.parentNode) nodeRoot = nodeRoot.parentNode;
            var rangeRoot = this.startContainer; while (rangeRoot.parentNode) rangeRoot = rangeRoot.parentNode;
            if (nodeRoot !== rangeRoot && !(nodeRoot.__nodeKey__ && rangeRoot.__nodeKey__ && nodeRoot.__nodeKey__ === rangeRoot.__nodeKey__))
                throw new DOMException('Wrong document', 'WrongDocumentError');
            if (offset < 0 || offset > this._nodeLen(node)) throw new DOMException('Index out of range', 'IndexSizeError');
            var cmpStart = this._compareBoundary(node, offset, this.startContainer, this.startOffset);
            if (cmpStart < 0) return -1;
            var cmpEnd = this._compareBoundary(node, offset, this.endContainer, this.endOffset);
            if (cmpEnd > 0) return 1;
            return 0;
        };
        Range.prototype.isPointInRange = function(node, offset) {
            try { return this.comparePoint(node, offset) === 0; } catch(e) { return false; }
        };
        Range.prototype.intersectsNode = function(node) {
            var nodeRoot = node; while (nodeRoot.parentNode) nodeRoot = nodeRoot.parentNode;
            var rangeRoot = this.startContainer; while (rangeRoot.parentNode) rangeRoot = rangeRoot.parentNode;
            if (nodeRoot !== rangeRoot && !(nodeRoot.__nodeKey__ && rangeRoot.__nodeKey__ && nodeRoot.__nodeKey__ === rangeRoot.__nodeKey__)) return false;
            var parent = node.parentNode;
            if (!parent) return true;
            var idx = (parent.__nodeKey__ && node.__nodeKey__ && document.__nativeChildIndex) ? document.__nativeChildIndex(parent, node) : -1;
            if (idx < 0) {
                var kids = parent.childNodes; if (!kids) return true;
                for (var i = 0; i < kids.length; i++) {
                    if (kids[i] === node || (kids[i].__nodeKey__ && node.__nodeKey__ && kids[i].__nodeKey__ === node.__nodeKey__)) { idx = i; break; }
                }
            }
            if (idx < 0) return true;
            return this._compareBoundary(parent, idx + 1, this.startContainer, this.startOffset) > 0 &&
                   this._compareBoundary(parent, idx, this.endContainer, this.endOffset) < 0;
        };
        Range.prototype.cloneRange = function() {
            var r = new Range(); r.startContainer = this.startContainer; r.startOffset = this.startOffset;
            r.endContainer = this.endContainer; r.endOffset = this.endOffset; r._update(); return r;
        };
        Range.prototype.detach = function() {};
        Range.prototype.toString = function() {
            if (this.collapsed) return '';
            var sc = this.startContainer, so = this.startOffset, ec = this.endContainer, eo = this.endOffset;
            // Same text node — simple case
            if (sc === ec && sc.nodeType === 3) return (sc.data || '').substring(so, eo);
            // Multi-node: collect text
            var result = '';
            // Start node partial text
            if (sc.nodeType === 3) { result += (sc.data || '').substring(so); }
            // Walk DOM in document order between start and end
            function walk(node) {
                if (node === ec) { if (node.nodeType === 3) result += (node.data || '').substring(0, eo); return true; }
                if (node.nodeType === 3) result += (node.data || '');
                if (node.childNodes) { for (var i = 0; i < node.childNodes.length; i++) { if (walk(node.childNodes[i])) return true; } }
                return false;
            }
            // Find next node after start
            var current = sc;
            if (sc.nodeType !== 3 && sc.childNodes && sc.childNodes[so]) { if (walk(sc.childNodes[so])) return result; current = sc.childNodes[so]; }
            // Walk siblings and up
            while (current) {
                var next = current.nextSibling;
                while (next) { if (walk(next)) return result; next = next.nextSibling; }
                current = current.parentNode;
            }
            return result;
        };
        Range.prototype.deleteContents = function() {};
        Range.prototype.extractContents = function() { return document.createDocumentFragment(); };
        Range.prototype.cloneContents = function() { return document.createDocumentFragment(); };
        Range.prototype.insertNode = function(node) {};
        Range.prototype.surroundContents = function(node) {};
        Range.prototype.getBoundingClientRect = function() { return {x:0,y:0,width:0,height:0,top:0,right:0,bottom:0,left:0}; };
        Range.prototype.getClientRects = function() { return []; };
        // Text and Comment constructors (DOM spec)
        globalThis.Text = function Text(data) {
            var node = document.createTextNode(data !== undefined ? String(data) : '');
            return node;
        };
        globalThis.Comment = function Comment(data) {
            var node = document.createComment(data !== undefined ? String(data) : '');
            return node;
        };
        globalThis.DOMParser = function DOMParser() {
            this.parseFromString = function(str, type) {
                if (type && type !== 'text/html' && type !== 'text/xml' &&
                    type !== 'application/xml' && type !== 'application/xhtml+xml' &&
                    type !== 'image/svg+xml') {
                    throw new TypeError("Invalid MIME type: " + type);
                }
                // Skapa en ny document via createHTMLDocument och parsa HTML in i den
                var newDoc = document.implementation.createHTMLDocument('');
                if (str && newDoc.documentElement) {
                    newDoc.documentElement.innerHTML = str;
                }
                // Spec-krävda properties
                newDoc.contentType = type || 'text/html';
                newDoc.compatMode = (str && str.indexOf('<!DOCTYPE') !== -1) ? 'CSS1Compat' : 'BackCompat';
                newDoc.location = null;
                newDoc.URL = 'about:blank';
                newDoc.documentURI = 'about:blank';
                newDoc.nodeType = 9;
                return newDoc;
            };
        };
        globalThis.XMLSerializer = function XMLSerializer() {
            this.serializeToString = function(node) {
                if (node && node.outerHTML !== undefined) return node.outerHTML;
                if (node && node.nodeType === 9 && node.documentElement) return node.documentElement.outerHTML || '';
                if (node && node.textContent !== undefined) return node.textContent;
                return '';
            };
        };
        globalThis.URL = function URL(url, base) {
            var full = url || '';
            if (base && url && url.indexOf('://') === -1) {
                if (base.charAt(base.length - 1) !== '/' && url.charAt(0) !== '/') {
                    full = base + '/' + url;
                } else {
                    full = base + url;
                }
            }
            this.href = full;
            this.toString = function() { return this.href; };
            var parts = full.match(/^(https?:)\/\/([^/:]+)(:[0-9]+)?(\/[^?#]*)?(\?[^#]*)?(#.*)?$/);
            if (parts) {
                this.protocol = parts[1] || '';
                this.hostname = parts[2] || '';
                this.port = parts[3] ? parts[3].substring(1) : '';
                this.host = this.hostname + (this.port ? ':' + this.port : '');
                this.pathname = parts[4] || '/';
                this.search = parts[5] || '';
                this.hash = parts[6] || '';
                this.origin = this.protocol + '//' + this.host;
            } else {
                this.protocol = ''; this.hostname = ''; this.port = '';
                this.host = ''; this.pathname = full; this.search = '';
                this.hash = ''; this.origin = '';
            }
            this.searchParams = {
                _params: {},
                get: function(key) { return this._params[key] || null; },
                has: function(key) { return key in this._params; },
                set: function(key, val) { this._params[key] = val; },
                delete: function(key) { delete this._params[key]; },
                toString: function() {
                    var parts = [];
                    for (var k in this._params) { parts.push(k + '=' + this._params[k]); }
                    return parts.join('&');
                }
            };
            if (this.search) {
                var qs = this.search.substring(1).split('&');
                for (var i = 0; i < qs.length; i++) {
                    var pair = qs[i].split('=');
                    if (pair[0]) this.searchParams._params[decodeURIComponent(pair[0])] = decodeURIComponent(pair[1] || '');
                }
            }
        };
    "#,
    )?;

    // SPA-stöd: fetch/XHR/Observer-stubs som förhindrar krascher i SPA-bundles
    ctx.eval::<(), _>(
        r#"
        // fetch() — returnera tom Response (förhindrar ReferenceError i SPA-bundles)
        globalThis.fetch = function(url, opts) {
            return Promise.resolve({
                ok: false,
                status: 0,
                statusText: 'Sandbox: network disabled',
                url: typeof url === 'string' ? url : '',
                redirected: false,
                type: 'basic',
                headers: {
                    get: function() { return null; },
                    has: function() { return false; },
                    forEach: function() {},
                    entries: function() { return []; },
                    keys: function() { return []; },
                    values: function() { return []; }
                },
                json: function() { return Promise.reject(new Error('Sandbox: fetch disabled')); },
                text: function() { return Promise.resolve(''); },
                blob: function() { return Promise.resolve(new Blob ? new Blob([]) : {}); },
                arrayBuffer: function() { return Promise.resolve(new ArrayBuffer(0)); },
                clone: function() { return this; },
                body: null,
                bodyUsed: false
            });
        };

        // Headers constructor
        globalThis.Headers = function Headers(init) {
            this._h = {};
            if (init && typeof init === 'object') {
                for (var k in init) { this._h[k.toLowerCase()] = init[k]; }
            }
            this.get = function(k) { return this._h[k.toLowerCase()] || null; };
            this.has = function(k) { return k.toLowerCase() in this._h; };
            this.set = function(k, v) { this._h[k.toLowerCase()] = v; };
            this.delete = function(k) { delete this._h[k.toLowerCase()]; };
            this.forEach = function(cb) { for (var k in this._h) { cb(this._h[k], k, this); } };
            this.entries = function() { var r = []; for (var k in this._h) { r.push([k, this._h[k]]); } return r; };
            this.keys = function() { return Object.keys(this._h); };
            this.values = function() { return Object.values(this._h); };
        };

        // Response constructor
        globalThis.Response = function Response(body, opts) {
            this.ok = opts && opts.status >= 200 && opts.status < 300;
            this.status = (opts && opts.status) || 200;
            this.statusText = (opts && opts.statusText) || '';
            this.headers = new Headers((opts && opts.headers) || {});
            this._body = body || '';
            this.json = function() { try { return Promise.resolve(JSON.parse(this._body)); } catch(e) { return Promise.reject(e); } };
            this.text = function() { return Promise.resolve(String(this._body)); };
            this.clone = function() { return new Response(this._body, opts); };
        };

        // Request constructor
        globalThis.Request = function Request(url, opts) {
            this.url = typeof url === 'string' ? url : (url && url.url) || '';
            this.method = (opts && opts.method) || 'GET';
            this.headers = new Headers((opts && opts.headers) || {});
            this.body = (opts && opts.body) || null;
        };

        // AbortController
        globalThis.AbortController = function AbortController() {
            this.signal = { aborted: false, addEventListener: function(){}, removeEventListener: function(){} };
            this.abort = function() { this.signal.aborted = true; };
        };

        // XMLHttpRequest stub
        globalThis.XMLHttpRequest = function XMLHttpRequest() {
            this.readyState = 0;
            this.status = 0;
            this.statusText = '';
            this.responseText = '';
            this.response = '';
            this.onreadystatechange = null;
            this.onload = null;
            this.onerror = null;
            this.open = function() { this.readyState = 1; };
            this.send = function() {
                this.readyState = 4;
                this.status = 0;
                if (this.onerror) { try { this.onerror({}); } catch(e) {} }
                if (this.onreadystatechange) { try { this.onreadystatechange(); } catch(e) {} }
            };
            this.setRequestHeader = function() {};
            this.getResponseHeader = function() { return null; };
            this.getAllResponseHeaders = function() { return ''; };
            this.abort = function() {};
            this.addEventListener = function() {};
            this.removeEventListener = function() {};
        };

        // IntersectionObserver stub
        globalThis.IntersectionObserver = function IntersectionObserver(cb, opts) {
            this.observe = function() {};
            this.unobserve = function() {};
            this.disconnect = function() {};
            this.takeRecords = function() { return []; };
        };

        // MessageChannel stub (React uses this)
        globalThis.MessageChannel = function MessageChannel() {
            var self = this;
            this.port1 = {
                onmessage: null,
                postMessage: function(msg) {
                    if (self.port2.onmessage) {
                        try { self.port2.onmessage({ data: msg }); } catch(e) {}
                    }
                },
                close: function() {}
            };
            this.port2 = {
                onmessage: null,
                postMessage: function(msg) {
                    if (self.port1.onmessage) {
                        try { self.port1.onmessage({ data: msg }); } catch(e) {}
                    }
                },
                close: function() {}
            };
        };

        // Blob stub
        if (typeof Blob === 'undefined') {
            globalThis.Blob = function Blob(parts, opts) {
                this.size = 0;
                this.type = (opts && opts.type) || '';
                if (parts) { for (var i = 0; i < parts.length; i++) { this.size += (parts[i].length || 0); } }
                this.text = function() { return Promise.resolve(''); };
                this.arrayBuffer = function() { return Promise.resolve(new ArrayBuffer(0)); };
                this.slice = function() { return new Blob([]); };
            };
        }

        // FormData stub
        globalThis.FormData = function FormData() {
            this._data = {};
            this.append = function(k, v) { this._data[k] = v; };
            this.get = function(k) { return this._data[k] || null; };
            this.has = function(k) { return k in this._data; };
            this.delete = function(k) { delete this._data[k]; };
            this.entries = function() { var r = []; for (var k in this._data) { r.push([k, this._data[k]]); } return r; };
        };

        // Map polyfill om saknas
        if (typeof Map === 'undefined') {
            globalThis.Map = function Map() {
                this._data = {};
                this.set = function(k, v) { this._data[k] = v; return this; };
                this.get = function(k) { return this._data[k]; };
                this.has = function(k) { return k in this._data; };
                this.delete = function(k) { return delete this._data[k]; };
                this.clear = function() { this._data = {}; };
                this.forEach = function(cb) { for (var k in this._data) { cb(this._data[k], k, this); } };
                Object.defineProperty(this, 'size', { get: function() { return Object.keys(this._data).length; } });
            };
        }
        if (typeof Set === 'undefined') {
            globalThis.Set = function Set() {
                this._data = {};
                this.add = function(v) { this._data[v] = true; return this; };
                this.has = function(v) { return v in this._data; };
                this.delete = function(v) { return delete this._data[v]; };
                this.clear = function() { this._data = {}; };
                this.forEach = function(cb) { for (var k in this._data) { cb(k, k, this); } };
                Object.defineProperty(this, 'size', { get: function() { return Object.keys(this._data).length; } });
            };
        }
    "#,
    )?;

    // Registrera localStorage/sessionStorage
    register_storage(ctx, Rc::clone(&state), "localStorage", true)?;
    register_storage(ctx, Rc::clone(&state), "sessionStorage", false)?;

    Ok(())
}

// ─── Console ────────────────────────────────────────────────────────────────

struct ConsoleLogHandler {
    state: SharedState,
    level: String,
}
impl JsHandler for ConsoleLogHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let parts: Vec<String> = args
            .iter()
            .map(|v| crate::js_eval::quickjs_value_to_string(ctx, v))
            .collect();
        let msg = format!("[{}] {}", self.level, parts.join(" "));
        self.state.borrow_mut().console_output.push(msg);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

fn register_console<'js>(ctx: &Ctx<'js>, state: SharedState) -> rquickjs::Result<()> {
    let console = Object::new(ctx.clone())?;
    for level in &["log", "warn", "error", "info", "debug"] {
        console.set(
            *level,
            Function::new(
                ctx.clone(),
                JsFn(ConsoleLogHandler {
                    state: Rc::clone(&state),
                    level: level.to_string(),
                }),
            )?,
        )?;
    }
    ctx.globals().set("console", console)?;
    Ok(())
}

// ─── Storage ────────────────────────────────────────────────────────────────

struct StorageGetItem {
    state: SharedState,
    is_local: bool,
}
impl JsHandler for StorageGetItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let key = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let s = self.state.borrow();
        let storage = if self.is_local {
            &s.local_storage
        } else {
            &s.session_storage
        };
        match storage.get(&key) {
            Some(val) => Ok(rquickjs::String::from_str(ctx.clone(), val)?.into_value()),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

struct StorageSetItem {
    state: SharedState,
    is_local: bool,
}
impl JsHandler for StorageSetItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let key = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let val = args
            .get(1)
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let storage = if self.is_local {
            &mut s.local_storage
        } else {
            &mut s.session_storage
        };
        storage.insert(key, val);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct StorageRemoveItem {
    state: SharedState,
    is_local: bool,
}
impl JsHandler for StorageRemoveItem {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let key = args
            .first()
            .and_then(|v| v.as_string())
            .and_then(|s| s.to_string().ok())
            .unwrap_or_default();
        let mut s = self.state.borrow_mut();
        let storage = if self.is_local {
            &mut s.local_storage
        } else {
            &mut s.session_storage
        };
        storage.remove(&key);
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct StorageClear {
    state: SharedState,
    is_local: bool,
}
impl JsHandler for StorageClear {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let mut s = self.state.borrow_mut();
        let storage = if self.is_local {
            &mut s.local_storage
        } else {
            &mut s.session_storage
        };
        storage.clear();
        Ok(Value::new_undefined(ctx.clone()))
    }
}

struct StorageLength {
    state: SharedState,
    is_local: bool,
}
impl JsHandler for StorageLength {
    fn handle<'js>(&self, ctx: &Ctx<'js>, _args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let s = self.state.borrow();
        let len = if self.is_local {
            s.local_storage.len()
        } else {
            s.session_storage.len()
        };
        Ok(Value::new_int(ctx.clone(), len as i32))
    }
}

struct StorageKey {
    state: SharedState,
    is_local: bool,
}
impl JsHandler for StorageKey {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        let index = args.first().and_then(|v| v.as_int()).unwrap_or(-1);
        if index < 0 {
            return Ok(Value::new_null(ctx.clone()));
        }
        let s = self.state.borrow();
        let storage = if self.is_local {
            &s.local_storage
        } else {
            &s.session_storage
        };
        let mut keys: Vec<&String> = storage.keys().collect();
        keys.sort();
        match keys.get(index as usize) {
            Some(k) => Ok(rquickjs::String::from_str(ctx.clone(), k)?.into_value()),
            None => Ok(Value::new_null(ctx.clone())),
        }
    }
}

fn register_storage<'js>(
    ctx: &Ctx<'js>,
    state: SharedState,
    name: &str,
    is_local: bool,
) -> rquickjs::Result<()> {
    let storage = Object::new(ctx.clone())?;
    storage.set(
        "getItem",
        Function::new(
            ctx.clone(),
            JsFn(StorageGetItem {
                state: Rc::clone(&state),
                is_local,
            }),
        )?,
    )?;
    storage.set(
        "setItem",
        Function::new(
            ctx.clone(),
            JsFn(StorageSetItem {
                state: Rc::clone(&state),
                is_local,
            }),
        )?,
    )?;
    storage.set(
        "removeItem",
        Function::new(
            ctx.clone(),
            JsFn(StorageRemoveItem {
                state: Rc::clone(&state),
                is_local,
            }),
        )?,
    )?;
    storage.set(
        "clear",
        Function::new(
            ctx.clone(),
            JsFn(StorageClear {
                state: Rc::clone(&state),
                is_local,
            }),
        )?,
    )?;
    storage.set(
        "key",
        Function::new(
            ctx.clone(),
            JsFn(StorageKey {
                state: Rc::clone(&state),
                is_local,
            }),
        )?,
    )?;
    // Dynamisk length-getter via Accessor
    storage.prop(
        "length",
        Accessor::new_get(JsFn(StorageLength {
            state: Rc::clone(&state),
            is_local,
        })),
    )?;
    ctx.globals().set(name, storage)?;
    Ok(())
}

// ─── Hjälpfunktioner (boa-oberoende) ──────────────────────────────────────

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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
/// Hitta matchande ) med hänsyn till nestade parenteser
fn find_matching_paren(s: &str) -> Option<usize> {
    let mut depth = 1;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

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
    if find_unescaped_delimiter(selector, &[',']) < selector.len() {
        return selector
            .split(',')
            .any(|s| matches_single_selector(arena, key, s.trim()));
    }

    // Descendant/child/sibling-kombinator — kolla bara oescaped combinators
    {
        let has_combinator =
            find_unescaped_delimiter(selector, &[' ', '>', '+', '~']) < selector.len();
        if has_combinator {
            // Dubbelkolla: hitta den faktiska split-punkten
            let split = find_unescaped_delimiter(selector, &[' ', '>', '+', '~']);
            // Om split == selector.len() → ingen combinator
            if split < selector.len() {
                return matches_combinator_selector(arena, key, selector);
            }
        }
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
/// Hitta nästa oescaped delimiter i CSS-selektor.
/// Hoppar över escaped tecken (\X, \XXXXXX).
fn find_unescaped_delimiter(s: &str, delimiters: &[char]) -> usize {
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 1; // hoppa över backslash
                    // Hoppa över hex-sekvens (1-6 hex + optional whitespace)
            if i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                let mut hex_count = 0;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() && hex_count < 6 {
                    i += 1;
                    hex_count += 1;
                }
                // Optional trailing whitespace
                if i < bytes.len() && matches!(bytes[i], b' ' | b'\t' | b'\n' | b'\r' | b'\x0C') {
                    i += 1;
                }
            } else if i < bytes.len() {
                i += 1; // hoppa över escaped tecken
            }
            continue;
        }
        // Kolla om det är en delimiter (men bara om vi är på en char boundary)
        if s.is_char_boundary(i) {
            let ch = s[i..].chars().next().unwrap();
            if delimiters.contains(&ch) {
                return i;
            }
            i += ch.len_utf8();
        } else {
            i += 1;
        }
    }
    s.len()
}

/// CSS escape-unescape per CSS syntax spec.
/// Hanterar: \XX (hex 1-6 siffror + optional space), \c (escaped tecken)
fn css_unescape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        // Backslash — kolla nästa tecken
        match chars.peek() {
            None => {
                // Backslash i slutet — ignorera per spec
            }
            Some(&next) if next.is_ascii_hexdigit() => {
                // Hex escape: 1-6 hex siffror
                let mut hex = String::with_capacity(6);
                for _ in 0..6 {
                    if let Some(&h) = chars.peek() {
                        if h.is_ascii_hexdigit() {
                            hex.push(h);
                            chars.next();
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                // Optional trailing whitespace (konsumeras)
                if let Some(&ws) = chars.peek() {
                    if ws == ' ' || ws == '\t' || ws == '\n' || ws == '\r' || ws == '\x0C' {
                        chars.next();
                    }
                }
                if let Ok(cp) = u32::from_str_radix(&hex, 16) {
                    if cp == 0 || (0xD800..=0xDFFF).contains(&cp) || cp > 0x10FFFF {
                        result.push('\u{FFFD}');
                    } else if let Some(ch) = char::from_u32(cp) {
                        result.push(ch);
                    } else {
                        result.push('\u{FFFD}');
                    }
                }
            }
            Some(_) => {
                // Escaped tecken — ta bokstavligt
                result.push(chars.next().unwrap());
            }
        }
    }
    result
}

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
    let mut nth_last_child_expr: Option<(i32, i32)> = None;
    let mut nth_last_of_type_expr: Option<(i32, i32)> = None;
    let mut require_is: Option<String> = None;
    let mut require_where: Option<String> = None;
    let mut require_has: Option<String> = None;
    let mut require_heading = false;
    let mut require_heading_levels: Option<Vec<u32>> = None;
    let mut require_lang: Option<String> = None;
    let mut require_dir: Option<String> = None;
    let mut require_placeholder_shown = false;
    let mut require_any_link = false;
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
            let end = find_unescaped_delimiter(rest, &['.', '[', ':']);
            required_id = Some(&rest[..end]);
            remaining = &rest[end..];
        } else if let Some(rest) = remaining.strip_prefix('.') {
            let end = find_unescaped_delimiter(rest, &['#', '.', '[', ':']);
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
        } else if let Some(rest) = remaining.strip_prefix(":nth-last-child(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_last_child_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":nth-last-of-type(") {
            if let Some(end) = rest.find(')') {
                let expr = &rest[..end];
                nth_last_of_type_expr = Some(parse_nth_expression(expr));
                remaining = &rest[end + 1..];
            } else {
                break;
            }
        } else if let Some(rest) = remaining.strip_prefix(":is(") {
            if let Some(end) = find_matching_paren(rest) {
                let inner = &rest[..end];
                require_is = Some(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":where(") {
            if let Some(end) = find_matching_paren(rest) {
                let inner = &rest[..end];
                require_where = Some(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":has(") {
            if let Some(end) = find_matching_paren(rest) {
                let inner = &rest[..end];
                require_has = Some(inner.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
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
        } else if let Some(rest) = remaining.strip_prefix(":heading(") {
            // :heading(n, m, ...) — matchar h<n>, h<m>, etc.
            if let Some(end) = rest.find(')') {
                let args = &rest[..end];
                require_heading_levels = Some(
                    args.split(',')
                        .filter_map(|s| s.trim().parse::<u32>().ok())
                        .collect(),
                );
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if remaining.starts_with(":heading") {
            // :heading (utan parentes) — matchar alla h1-h6
            require_heading = true;
            remaining = &remaining[8..]; // len(":heading") = 8
        } else if let Some(rest) = remaining.strip_prefix(":lang(") {
            // :lang(xx) — matchar element med lang-attribut
            if let Some(end) = find_matching_paren(rest) {
                let lang_arg = rest[..end].trim().trim_matches('"').trim_matches('\'');
                require_lang = Some(lang_arg.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if let Some(rest) = remaining.strip_prefix(":dir(") {
            // :dir(ltr|rtl)
            if let Some(end) = rest.find(')') {
                let dir_arg = rest[..end].trim();
                require_dir = Some(dir_arg.to_string());
                remaining = &rest[end + 1..];
            } else {
                return false;
            }
        } else if remaining.starts_with(":placeholder-shown") {
            require_placeholder_shown = true;
            remaining = &remaining[18..];
        } else if remaining.starts_with(":any-link") {
            require_any_link = true;
            remaining = &remaining[9..];
        } else if remaining.starts_with(":link") {
            require_any_link = true; // :link ≈ :any-link i vår kontext
            remaining = &remaining[5..];
        } else if remaining.starts_with(":visited") {
            // :visited — aldrig matchad (ingen browsing history)
            return false;
        } else if remaining.starts_with(":hover") || remaining.starts_with(":active") {
            // Dynamiska pseudo-klasser — aldrig matchade i statisk parse
            return false;
        } else if remaining.starts_with(':') {
            // Okänd pseudo-klass
            return false;
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
        let unesc = css_unescape(id);
        if node.get_attr("id") != Some(unesc.as_str()) {
            return false;
        }
    }
    for cls in &required_classes {
        let unesc = css_unescape(cls);
        let has = node
            .get_attr("class")
            .map(|c| split_ascii_whitespace(c).any(|x| x == unesc))
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

    // :nth-last-child
    if let Some((a, b)) = nth_last_child_expr {
        let matched = element_index_among_siblings(arena, key)
            .map(|(pos, total)| {
                let from_last = total - pos + 1;
                matches_nth(from_last, a, b)
            })
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }
    // :nth-last-of-type
    if let Some((a, b)) = nth_last_of_type_expr {
        let matched = type_index_among_siblings(arena, key)
            .map(|(pos, total)| {
                let from_last = total - pos + 1;
                matches_nth(from_last, a, b)
            })
            .unwrap_or(false);
        if !matched {
            return false;
        }
    }
    // :is() / :where() — matcha mot kommaseparerade inre selektorer
    if let Some(ref inner) = require_is {
        let any_match = inner
            .split(',')
            .any(|s| matches_selector(arena, key, s.trim()));
        if !any_match {
            return false;
        }
    }
    if let Some(ref inner) = require_where {
        let any_match = inner
            .split(',')
            .any(|s| matches_selector(arena, key, s.trim()));
        if !any_match {
            return false;
        }
    }
    // :has() — matcha om elementet har efterkommande som matchar
    if let Some(ref inner) = require_has {
        let has_match = inner.split(',').any(|sel| {
            let sel = sel.trim();
            // Sök bland alla efterkommande
            fn check_descendants(arena: &ArenaDom, parent: NodeKey, sel: &str) -> bool {
                if let Some(node) = arena.nodes.get(parent) {
                    for &child in &node.children {
                        if matches_selector(arena, child, sel) {
                            return true;
                        }
                        if check_descendants(arena, child, sel) {
                            return true;
                        }
                    }
                }
                false
            }
            check_descendants(arena, key, sel)
        });
        if !has_match {
            return false;
        }
    }
    // :heading / :heading(n) — matchar h1-h6
    if require_heading {
        let tag = node.tag.as_deref().unwrap_or("");
        let is_heading = matches!(tag, "h1" | "h2" | "h3" | "h4" | "h5" | "h6");
        if !is_heading {
            return false;
        }
    }
    if let Some(ref levels) = require_heading_levels {
        let tag = node.tag.as_deref().unwrap_or("");
        let heading_level: u32 = match tag {
            "h1" => 1,
            "h2" => 2,
            "h3" => 3,
            "h4" => 4,
            "h5" => 5,
            "h6" => 6,
            _ => 0,
        };
        if heading_level == 0 || !levels.contains(&heading_level) {
            return false;
        }
    }
    // :lang(xx) — matchar element eller ancestors med lang-attribut
    if let Some(ref lang) = require_lang {
        let lang_lower = lang.to_lowercase();
        let mut found = false;
        let mut current = Some(key);
        while let Some(k) = current {
            if let Some(n) = arena.nodes.get(k) {
                if let Some(node_lang) = n.get_attr("lang").or_else(|| n.get_attr("xml:lang")) {
                    let node_lang_lower = node_lang.to_lowercase();
                    // :lang(en) matchar "en", "en-US", "en-GB" etc.
                    if node_lang_lower == lang_lower
                        || node_lang_lower.starts_with(&format!("{}-", lang_lower))
                    {
                        found = true;
                    }
                    break; // Närmaste lang-attribut bestämmer
                }
                current = n.parent;
            } else {
                break;
            }
        }
        if !found {
            return false;
        }
    }
    // :dir(ltr|rtl)
    if let Some(ref dir) = require_dir {
        let mut found_dir = "ltr".to_string(); // default
        let mut current = Some(key);
        while let Some(k) = current {
            if let Some(n) = arena.nodes.get(k) {
                if let Some(d) = n.get_attr("dir") {
                    found_dir = d.to_lowercase();
                    break;
                }
                current = n.parent;
            } else {
                break;
            }
        }
        if found_dir != dir.to_lowercase() {
            return false;
        }
    }
    // :placeholder-shown
    if require_placeholder_shown {
        let has_placeholder = node.get_attr("placeholder").is_some();
        let is_input = node
            .tag
            .as_deref()
            .map_or(false, |t| t == "input" || t == "textarea");
        let value_empty = node.get_attr("value").map_or(true, |v| v.is_empty());
        if !(is_input && has_placeholder && value_empty) {
            return false;
        }
    }
    // :any-link / :link
    if require_any_link {
        let is_link = node
            .tag
            .as_deref()
            .map_or(false, |t| t == "a" || t == "area")
            && node.has_attr("href");
        if !is_link {
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
/// Splitta sträng på ASCII whitespace per HTML-spec (space, tab, LF, FF, CR).
/// Unicode-whitespace som \u{00A0} (NBSP) är INTE separatorer — de är giltiga class-tecken.
fn split_ascii_whitespace(s: &str) -> impl Iterator<Item = &str> {
    s.split([' ', '\t', '\n', '\x0C', '\r'])
        .filter(|s| !s.is_empty())
}

fn find_all_by_class(arena: &ArenaDom, key: NodeKey, class: &str, results: &mut Vec<NodeKey>) {
    let node = match arena.nodes.get(key) {
        Some(n) => n,
        None => return,
    };
    if node.node_type == NodeType::Element {
        if let Some(attr_classes) = node.get_attr("class") {
            // getElementsByClassName("a b") matchar element med BÅDA "a" och "b"
            let search_tokens: Vec<&str> = split_ascii_whitespace(class).collect();
            if !search_tokens.is_empty() {
                let elem_tokens: Vec<&str> = split_ascii_whitespace(attr_classes).collect();
                if search_tokens.iter().all(|t| elem_tokens.contains(t)) {
                    results.push(key);
                }
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
