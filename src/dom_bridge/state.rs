// Delad state mellan JS-callbacks i DOM Bridge

use rquickjs::{Function, Persistent};

use std::cell::RefCell;
use std::rc::Rc;

use crate::arena_dom::ArenaDom;

use super::element_state::ElementStateStore;

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
    /// URLs som JS anropade via fetch() — för Rust-side interception (BUGG J)
    #[cfg_attr(not(feature = "js-eval"), allow(dead_code))]
    pub fetched_urls: Vec<String>,
}

/// En mutation som JS-koden utförde på DOM:en — Cow undviker allokering för statiska strängar
pub type DomMutation = std::borrow::Cow<'static, str>;

/// Resultat med modifierad ArenaDom — för render_with_js-pipeline
#[cfg(feature = "blitz")]
#[allow(dead_code)]
pub struct DomEvalWithArena {
    pub result: DomEvalResult,
    pub arena: ArenaDom,
}

/// En registrerad event listener på ett DOM-element
#[allow(dead_code)]
pub(crate) struct EventListener {
    pub(super) event_type: String,
    pub(super) callback: Persistent<Function<'static>>,
    pub(super) capture: bool,
    pub(super) passive: Option<bool>,
    pub(super) once: bool,
    /// Unik ID för callback-identifiering (för removeEventListener)
    pub(super) callback_id: u64,
}

#[allow(dead_code)]
pub(crate) struct BridgeState {
    pub(super) arena: ArenaDom,
    pub(super) mutations: Vec<DomMutation>,
    /// Event listeners per nod (NodeKey ffi-index → listeners)
    pub(super) event_listeners: std::collections::HashMap<u64, Vec<EventListener>>,
    /// Vilken nod har fokus (NodeKey ffi-index)
    pub(super) focused_element: Option<u64>,
    /// Scroll-positioner per nod (NodeKey ffi-index → (scrollTop, scrollLeft))
    pub(super) scroll_positions: std::collections::HashMap<u64, (f64, f64)>,
    /// CSS Cascade Engine — lazy-initialiserad vid första getComputedStyle()
    pub(super) css_context: Option<crate::css_cascade::CssContext>,
    /// Blitz Stylo computed styles cache — DFS-mappade från Blitz DOM
    /// Key: NodeKey ffi-bits, Value: CSS properties
    #[cfg(feature = "blitz")]
    pub(super) blitz_styles:
        Option<std::collections::HashMap<u64, std::collections::HashMap<String, String>>>,
    /// In-memory localStorage (sandboxad, ingen persistens)
    pub(super) local_storage: std::collections::HashMap<String, String>,
    /// In-memory sessionStorage (sandboxad, ingen persistens)
    pub(super) session_storage: std::collections::HashMap<String, String>,
    /// Fångade console-meddelanden
    pub(super) console_output: Vec<String>,
    /// Per-element mutable state (value, checked, validity, etc.)
    /// Key: NodeKey ffi-bits (u64), Value: ElementState
    pub(super) element_state: ElementStateStore,
    /// document.readyState — "loading", "interactive" eller "complete"
    pub(super) ready_state: String,
    /// Original HTML — behövs för Blitz Stylo lazy-init
    #[cfg(feature = "blitz")]
    pub(super) original_html: Option<String>,
    /// Mutation counter — ökas vid DOM-mutationer, invaliderar Blitz cache
    #[cfg(feature = "blitz")]
    pub(super) blitz_style_generation: u64,
    /// Generation vid senaste Blitz-cache-build
    #[cfg(feature = "blitz")]
    pub(super) blitz_cache_generation: u64,
    /// Nästa callback_id för event listeners
    pub(super) next_callback_id: u64,
    /// History API: stack av (url, state_json) — för pushState/replaceState/back/forward
    pub(super) history_stack: Vec<(String, Option<String>)>,
    /// History API: aktuell index i history_stack
    pub(super) history_index: usize,
    /// Aktuell URL — uppdateras av pushState/replaceState och location-setters
    pub(super) current_url: String,
    /// Pre-populerade fetch-responses: URL → (status, content_type, body)
    /// Sätts av Rust innan JS-evaluering för att göra fetch() synkront tillgängligt
    pub(crate) fetch_responses: std::collections::HashMap<String, FetchResponse>,
    /// Pending fetch-requests som JS vill göra men inte har svar för
    pub(crate) pending_fetches: Vec<PendingFetch>,
    /// Pre-populerade WebSocket-meddelanden: URL → meddelanden
    pub(crate) websocket_messages: std::collections::HashMap<String, WebSocketMessages>,
    /// Registrerade WebSocket-URLer som JS öppnade
    pub(crate) websocket_urls: Vec<String>,
    /// Cookies att exponera via document.cookie (key=value par)
    pub(crate) cookies: String,
}

/// Pre-populerat fetch-response för JS-sandlådan
#[derive(Debug, Clone)]
pub struct FetchResponse {
    pub status: u16,
    pub content_type: String,
    pub body: String,
    pub headers: std::collections::HashMap<String, String>,
}

/// En pending fetch-request som JS vill göra — fält läses av extern orkestreringslogik
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) struct PendingFetch {
    pub url: String,
    pub method: String,
    pub headers: std::collections::HashMap<String, String>,
    pub body: Option<String>,
}

/// Pre-populerade WebSocket-meddelanden för JS-sandlådan
#[derive(Debug, Clone, Default)]
pub struct WebSocketMessages {
    /// Meddelanden att leverera till JS i ordning
    pub messages: Vec<String>,
}

pub(crate) type SharedState = Rc<RefCell<BridgeState>>;
