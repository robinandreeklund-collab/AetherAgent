// Window + Console + Storage + DOMException + getComputedStyle + matchMedia

use std::rc::Rc;

use rquickjs::{object::Accessor, Ctx, Function, Object, Value};

use crate::event_loop::{JsFn, JsHandler};

use super::events::{self, AddEventListenerHandler, RemoveEventListenerHandler};
use super::state::SharedState;
use super::style::kebab_to_camel;
use super::utils::{get_tag_style_defaults, parse_inline_styles, parse_media_query_matches};
#[cfg(feature = "blitz")]
use super::{build_blitz_computed_styles, map_blitz_styles_to_arena};
use super::{
    extract_node_key, node_key_to_f64, CheckXmlWellFormedHandler, GetSelectionFromDoc, NoOpHandler,
    XmlSerializeNodeHandler,
};

/// Window.dispatchEvent — dispatchar event med WINDOW_EVENT_KEY som target
/// så att window-registrerade listeners hittas vid AT_TARGET.
struct WindowDispatchEvent {
    state: SharedState,
}
impl JsHandler for WindowDispatchEvent {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Validera null/undefined argument
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
        // Kör listeners registrerade under WINDOW_EVENT_KEY direkt (AT_TARGET)
        let event_type = first
            .and_then(|v| v.as_object())
            .and_then(|obj| obj.get::<_, String>("type").ok())
            .unwrap_or_default();
        let event_val = first.cloned().unwrap_or(Value::new_undefined(ctx.clone()));
        // Sätt target = window
        if let Some(ev) = event_val.as_object() {
            let win_val: Value = ctx
                .eval("window")
                .unwrap_or(Value::new_undefined(ctx.clone()));
            let _ = ev.set("target", win_val.clone());
            let _ = ev.set("currentTarget", win_val);
            let _ = ev.set("eventPhase", 2i32); // AT_TARGET
        }
        let mut default_prevented = false;
        // Kör matchande window-listeners (clone Persistent för att undvika borrow-issue)
        let callbacks: Vec<_> = {
            let s = self.state.borrow();
            s.event_listeners
                .get(&events::WINDOW_EVENT_KEY)
                .map(|listeners| {
                    listeners
                        .iter()
                        .filter(|l| l.event_type == event_type)
                        .map(|l| l.callback.clone())
                        .collect()
                })
                .unwrap_or_default()
        };
        for cb in callbacks {
            if let Ok(func) = cb.restore(ctx) {
                let _ = func.call::<_, Value>((event_val.clone(),));
            }
        }
        // Kolla defaultPrevented
        if let Some(ev) = event_val.as_object() {
            default_prevented = ev.get::<_, bool>("defaultPrevented").unwrap_or(false);
            let _ = ev.set("eventPhase", 0i32);
            let _ = ev.set("currentTarget", Value::new_null(ctx.clone()));
        }
        Ok(Value::new_bool(ctx.clone(), !default_prevented))
    }
}

pub(super) struct GetComputedStyleHandler {
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
pub(super) struct ComputedStyleGetProperty {
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

pub(super) struct MatchMediaHandler;
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
pub(super) fn register_dom_exception(ctx: &Ctx<'_>) -> rquickjs::Result<()> {
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

pub(super) fn register_window<'js>(ctx: &Ctx<'js>, state: SharedState) -> rquickjs::Result<()> {
    register_window_with_viewport(ctx, state, 1280, 900)
}

/// Register window-objekt med dynamiska viewport-dimensioner
pub(super) fn register_window_with_viewport<'js>(
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

    // __xmlSerializeNode — native XML-serialisering via Rust
    win.set(
        "__xmlSerializeNode",
        Function::new(
            ctx.clone(),
            JsFn(XmlSerializeNodeHandler {
                state: Rc::clone(&state),
            }),
        )?,
    )?;

    // __checkXmlWellFormed — kontrollera XML well-formedness, returnerar null eller felmeddelande
    // Registrera på BÅDE window OCH globals (globalThis) så DOMParser kan hitta den
    win.set(
        "__checkXmlWellFormed",
        Function::new(ctx.clone(), JsFn(CheckXmlWellFormedHandler))?,
    )?;
    ctx.globals().set(
        "__checkXmlWellFormed",
        Function::new(ctx.clone(), JsFn(CheckXmlWellFormedHandler))?,
    )?;

    // addEventListener / removeEventListener / dispatchEvent på window
    {
        // Window-listeners lagras med WINDOW_EVENT_KEY, separat från document
        let doc_key = state.borrow().arena.document;
        win.set(
            "addEventListener",
            Function::new(
                ctx.clone(),
                JsFn(AddEventListenerHandler {
                    state: Rc::clone(&state),
                    key: doc_key,
                    override_key: Some(events::WINDOW_EVENT_KEY),
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
                    override_key: Some(events::WINDOW_EVENT_KEY),
                }),
            )?,
        )?;
        win.set(
            "dispatchEvent",
            Function::new(
                ctx.clone(),
                JsFn(WindowDispatchEvent {
                    state: Rc::clone(&state),
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
            globalThis.__xmlSerializeNode = globalThis.window.__xmlSerializeNode;
            // Synka requestIdleCallback/cancelIdleCallback till window
            if (globalThis.requestIdleCallback) {
                globalThis.window.requestIdleCallback = globalThis.requestIdleCallback;
                globalThis.window.cancelIdleCallback = globalThis.cancelIdleCallback;
            }
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
            this._returnValue = true;
            Object.defineProperty(this, 'returnValue', {
                get: function() { return this._returnValue; },
                set: function(v) {
                    if (!v) {
                        // returnValue=false: sätt canceled flag bara om cancelable
                        if (this.cancelable && !this.__passive) {
                            this.defaultPrevented = true;
                            this._returnValue = false;
                        }
                        // Om ej cancelable: ignorera (returnValue förblir true)
                    } else {
                        // returnValue=true: har ingen effekt om redan canceled
                        if (!this.defaultPrevented) {
                            this._returnValue = true;
                        }
                    }
                },
                configurable: true, enumerable: true
            });
            this.timeStamp = (typeof performance !== 'undefined' && performance.now) ? performance.now() : Date.now();
            this.isTrusted = false;
            this._stopPropagationFlag = false;
            this._stopImmediatePropagationFlag = false;
            this._dispatching = false;
            this._canceledFlag = false;
            // cancelBubble: getter/setter per spec — set(false) = no-op, set(true) = stopPropagation
            Object.defineProperty(this, 'cancelBubble', {
                get: function() { return this._stopPropagationFlag; },
                set: function(v) { if (v) this._stopPropagationFlag = true; },
                configurable: true, enumerable: true
            });
            this._composedPath = [];
            this.composedPath = function() { return this._dispatching ? this._composedPath.slice() : []; };
            this.stopPropagation = function() { this._stopPropagationFlag = true; };
            this.stopImmediatePropagation = function() { this._stopPropagationFlag = true; this._stopImmediatePropagationFlag = true; };
            this.preventDefault = function() { if (this.cancelable && !this.__passive) { this.defaultPrevented = true; this._returnValue = false; } };
            this.initEvent = function(type, bubbles, cancelable) { if (this._dispatching) return; this.type = type; this.bubbles = !!bubbles; this.cancelable = !!cancelable; this.defaultPrevented = false; this._returnValue = true; this._canceledFlag = false; this._stopPropagationFlag = false; this._stopImmediatePropagationFlag = false; this.target = null; this.srcElement = null; this.currentTarget = null; this.eventPhase = 0; };
        };
        Event.NONE = 0; Event.CAPTURING_PHASE = 1; Event.AT_TARGET = 2; Event.BUBBLING_PHASE = 3;
        Event.prototype.NONE = 0; Event.prototype.CAPTURING_PHASE = 1; Event.prototype.AT_TARGET = 2; Event.prototype.BUBBLING_PHASE = 3;
        globalThis.CustomEvent = function CustomEvent(type, opts) {
            Event.call(this, type, opts);
            this.detail = (opts && opts.detail) || null;
        };
        CustomEvent.prototype = Object.create(Event.prototype);
        CustomEvent.prototype.constructor = CustomEvent;
        CustomEvent.prototype.initCustomEvent = function(type, bubbles, cancelable, detail) { if (arguments.length < 1) throw new TypeError("Failed to execute 'initCustomEvent': 1 argument required, but only 0 present."); if (this._dispatching) { return; } this.initEvent(type, bubbles, cancelable); this.detail = detail !== undefined ? detail : null; };

        // ─── DOM Type Hierarchy (native, migrerad från polyfills.js) ─────────
        // EventTarget → Node → Element/CharacterData → HTMLElement/Text/Comment
        (function() {
            function EventTargetBase() {}
            function NodeBase() {}
            NodeBase.prototype = Object.create(EventTargetBase.prototype);
            NodeBase.prototype.constructor = NodeBase;
            function CharacterDataBase() {}
            CharacterDataBase.prototype = Object.create(NodeBase.prototype);
            CharacterDataBase.prototype.constructor = CharacterDataBase;
            function ElementBase() {}
            ElementBase.prototype = Object.create(NodeBase.prototype);
            ElementBase.prototype.constructor = ElementBase;
            function HTMLElementBase() {}
            HTMLElementBase.prototype = Object.create(ElementBase.prototype);
            HTMLElementBase.prototype.constructor = HTMLElementBase;

            // Node-konstanter
            var nc = {ELEMENT_NODE:1,ATTRIBUTE_NODE:2,TEXT_NODE:3,CDATA_SECTION_NODE:4,
                ENTITY_REFERENCE_NODE:5,ENTITY_NODE:6,
                PROCESSING_INSTRUCTION_NODE:7,COMMENT_NODE:8,DOCUMENT_NODE:9,
                DOCUMENT_TYPE_NODE:10,DOCUMENT_FRAGMENT_NODE:11,NOTATION_NODE:12,
                DOCUMENT_POSITION_DISCONNECTED:1,DOCUMENT_POSITION_PRECEDING:2,
                DOCUMENT_POSITION_FOLLOWING:4,DOCUMENT_POSITION_CONTAINS:8,
                DOCUMENT_POSITION_CONTAINED_BY:16,DOCUMENT_POSITION_IMPLEMENTATION_SPECIFIC:32};
            for (var k in nc) { NodeBase[k] = nc[k]; NodeBase.prototype[k] = nc[k]; }

            if (!globalThis.EventTarget) globalThis.EventTarget = EventTargetBase;
            globalThis.Node = NodeBase;
            if (!globalThis.Element) globalThis.Element = ElementBase;
            if (!globalThis.CharacterData) globalThis.CharacterData = CharacterDataBase;
            if (!globalThis.HTMLElement) globalThis.HTMLElement = HTMLElementBase;

            // Icke-HTML-typer med korrekt prototypkedja
            var nonHtml = {
                'Text': CharacterDataBase, 'Comment': CharacterDataBase,
                'DocumentFragment': NodeBase, 'Document': NodeBase,
                'DocumentType': NodeBase, 'ProcessingInstruction': CharacterDataBase,
                'CDATASection': CharacterDataBase, 'Attr': NodeBase, 'XMLDocument': NodeBase
            };
            for (var name in nonHtml) {
                var existing = globalThis[name];
                if (!existing || typeof existing !== 'function') {
                    var C = function() {}; C.prototype = Object.create(nonHtml[name].prototype);
                    C.prototype.constructor = C; globalThis[name] = C;
                } else {
                    var parent = nonHtml[name].prototype;
                    if (!parent.isPrototypeOf(existing.prototype)) {
                        var np = Object.create(parent);
                        var props = Object.getOwnPropertyNames(existing.prototype);
                        for (var i = 0; i < props.length; i++) {
                            if (props[i] !== '__proto__') {
                                try { var d = Object.getOwnPropertyDescriptor(existing.prototype, props[i]);
                                    if (d) Object.defineProperty(np, props[i], d); } catch(e) {}
                            }
                        }
                        np.constructor = existing; existing.prototype = np;
                    }
                }
            }
        })();

        // ─── Event Subclass Constructors (native, spec-compliant per W3C UIEvents) ─
        // Hierarki: Event → UIEvent → MouseEvent/KeyboardEvent/FocusEvent/InputEvent
        //                   UIEvent → MouseEvent → WheelEvent → PointerEvent
        (function() {
            // ── getModifierState helper (shared by Mouse/Keyboard) ──
            function _getModifierState(key) {
                switch(key) {
                    case 'Control': return !!this.ctrlKey;
                    case 'Shift': return !!this.shiftKey;
                    case 'Alt': return !!this.altKey;
                    case 'Meta': return !!this.metaKey;
                    case 'AltGraph': return !!this.modifierAltGraph;
                    case 'CapsLock': return !!this.modifierCapsLock;
                    case 'Fn': return !!this.modifierFn;
                    case 'FnLock': return !!this.modifierFnLock;
                    case 'Hyper': return !!this.modifierHyper;
                    case 'NumLock': return !!this.modifierNumLock;
                    case 'ScrollLock': return !!this.modifierScrollLock;
                    case 'Super': return !!this.modifierSuper;
                    case 'Symbol': return !!this.modifierSymbol;
                    case 'SymbolLock': return !!this.modifierSymbolLock;
                    default: return false;
                }
            }

            // ── UIEvent ──
            if (!globalThis.UIEvent) {
                globalThis.UIEvent = function UIEvent(type) {
                    var opts = arguments[1] || {};
                    Event.call(this, type, opts);
                    this.view = opts.view !== undefined ? opts.view : null;
                    this.detail = opts.detail !== undefined ? opts.detail : 0;
                };
                UIEvent.prototype = Object.create(Event.prototype);
                UIEvent.prototype.constructor = UIEvent;
            }
            UIEvent.prototype.initUIEvent = function(type, bubbles, cancelable, view, detail) {
                if (arguments.length < 1) throw new TypeError("Failed to execute 'initUIEvent': 1 argument required, but only 0 present.");
                if (this._dispatching) return;
                this.initEvent(type, !!bubbles, !!cancelable);
                this.view = view !== undefined ? view : null;
                this.detail = detail !== undefined ? detail : 0;
            };
            // pseudoTarget: new spec property (getter returning null)
            Object.defineProperty(UIEvent.prototype, 'pseudoTarget', {
                get: function() { return null; },
                configurable: true,
                enumerable: true
            });

            // ── MouseEvent ──
            if (!globalThis.MouseEvent) {
                globalThis.MouseEvent = function MouseEvent(type) {
                    var opts = arguments[1] || {};
                    UIEvent.call(this, type, opts);
                    var o = opts;
                    this.screenX = o.screenX || 0; this.screenY = o.screenY || 0;
                    this.clientX = o.clientX || 0; this.clientY = o.clientY || 0;
                    this.pageX = o.pageX !== undefined ? o.pageX : this.clientX;
                    this.pageY = o.pageY !== undefined ? o.pageY : this.clientY;
                    this.offsetX = o.offsetX || 0; this.offsetY = o.offsetY || 0;
                    this.movementX = o.movementX || 0; this.movementY = o.movementY || 0;
                    this.button = o.button !== undefined ? o.button : 0;
                    this.buttons = o.buttons || 0;
                    this.relatedTarget = o.relatedTarget || null;
                    this.ctrlKey = !!o.ctrlKey; this.shiftKey = !!o.shiftKey;
                    this.altKey = !!o.altKey; this.metaKey = !!o.metaKey;
                    this.modifierAltGraph = !!o.modifierAltGraph;
                    this.modifierCapsLock = !!o.modifierCapsLock;
                    this.modifierFn = !!o.modifierFn;
                    this.modifierFnLock = !!o.modifierFnLock;
                    this.modifierHyper = !!o.modifierHyper;
                    this.modifierNumLock = !!o.modifierNumLock;
                    this.modifierScrollLock = !!o.modifierScrollLock;
                    this.modifierSuper = !!o.modifierSuper;
                    this.modifierSymbol = !!o.modifierSymbol;
                    this.modifierSymbolLock = !!o.modifierSymbolLock;
                };
                MouseEvent.prototype = Object.create(UIEvent.prototype);
                MouseEvent.prototype.constructor = MouseEvent;
            }
            // x/y are aliases for clientX/clientY per spec
            Object.defineProperty(MouseEvent.prototype, 'x', { get: function() { return this.clientX; }, configurable: true });
            Object.defineProperty(MouseEvent.prototype, 'y', { get: function() { return this.clientY; }, configurable: true });
            Object.defineProperty(MouseEvent.prototype, 'layerX', { get: function() { return this.clientX; }, configurable: true });
            Object.defineProperty(MouseEvent.prototype, 'layerY', { get: function() { return this.clientY; }, configurable: true });
            MouseEvent.prototype.getModifierState = _getModifierState;
            MouseEvent.prototype.initMouseEvent = function(type, bubbles, cancelable, view, detail,
                    screenX, screenY, clientX, clientY, ctrlKey, altKey, shiftKey, metaKey, button, relatedTarget) {
                if (arguments.length < 1) throw new TypeError("Failed to execute 'initMouseEvent': 1 argument required, but only 0 present.");
                if (this._dispatching) return;
                this.initUIEvent(type, bubbles, cancelable, view, detail);
                this.screenX = screenX || 0; this.screenY = screenY || 0;
                this.clientX = clientX || 0; this.clientY = clientY || 0;
                this.ctrlKey = !!ctrlKey; this.altKey = !!altKey;
                this.shiftKey = !!shiftKey; this.metaKey = !!metaKey;
                this.button = button !== undefined ? button : 0;
                this.relatedTarget = relatedTarget || null;
            };

            // ── KeyboardEvent ──
            if (!globalThis.KeyboardEvent) {
                globalThis.KeyboardEvent = function KeyboardEvent(type) {
                    var opts = arguments[1] || {};
                    UIEvent.call(this, type, opts);
                    var o = opts;
                    this.key = o.key !== undefined ? String(o.key) : '';
                    this.code = o.code !== undefined ? String(o.code) : '';
                    this.location = o.location || 0;
                    this.repeat = !!o.repeat;
                    this.isComposing = !!o.isComposing;
                    this.ctrlKey = !!o.ctrlKey; this.shiftKey = !!o.shiftKey;
                    this.altKey = !!o.altKey; this.metaKey = !!o.metaKey;
                    this.modifierAltGraph = !!o.modifierAltGraph;
                    this.modifierCapsLock = !!o.modifierCapsLock;
                    this.modifierFn = !!o.modifierFn;
                    this.modifierFnLock = !!o.modifierFnLock;
                    this.modifierHyper = !!o.modifierHyper;
                    this.modifierNumLock = !!o.modifierNumLock;
                    this.modifierScrollLock = !!o.modifierScrollLock;
                    this.modifierSuper = !!o.modifierSuper;
                    this.modifierSymbol = !!o.modifierSymbol;
                    this.modifierSymbolLock = !!o.modifierSymbolLock;
                    this.charCode = o.charCode || 0;
                    this.keyCode = o.keyCode || 0;
                    this.which = o.which || 0;
                };
                KeyboardEvent.prototype = Object.create(UIEvent.prototype);
                KeyboardEvent.prototype.constructor = KeyboardEvent;
            }
            KeyboardEvent.prototype.getModifierState = _getModifierState;
            KeyboardEvent.prototype.initKeyboardEvent = function(type, bubbles, cancelable, view,
                    key, location, ctrlKey, altKey, shiftKey, metaKey) {
                if (arguments.length < 1) throw new TypeError("Failed to execute 'initKeyboardEvent': 1 argument required, but only 0 present.");
                if (this._dispatching) return;
                this.initUIEvent(type, bubbles, cancelable, view, 0);
                this.key = key !== undefined ? String(key) : '';
                this.location = location || 0;
                this.ctrlKey = !!ctrlKey; this.altKey = !!altKey;
                this.shiftKey = !!shiftKey; this.metaKey = !!metaKey;
            };
            KeyboardEvent.DOM_KEY_LOCATION_STANDARD = 0;
            KeyboardEvent.DOM_KEY_LOCATION_LEFT = 1;
            KeyboardEvent.DOM_KEY_LOCATION_RIGHT = 2;
            KeyboardEvent.DOM_KEY_LOCATION_NUMPAD = 3;
            KeyboardEvent.prototype.DOM_KEY_LOCATION_STANDARD = 0;
            KeyboardEvent.prototype.DOM_KEY_LOCATION_LEFT = 1;
            KeyboardEvent.prototype.DOM_KEY_LOCATION_RIGHT = 2;
            KeyboardEvent.prototype.DOM_KEY_LOCATION_NUMPAD = 3;

            // ── FocusEvent ──
            if (!globalThis.FocusEvent) {
                globalThis.FocusEvent = function FocusEvent(type) {
                    var opts = arguments[1] || {};
                    UIEvent.call(this, type, opts);
                    this.relatedTarget = opts.relatedTarget || null;
                };
                FocusEvent.prototype = Object.create(UIEvent.prototype);
                FocusEvent.prototype.constructor = FocusEvent;
            }

            // ── InputEvent ──
            if (!globalThis.InputEvent) {
                globalThis.InputEvent = function InputEvent(type) {
                    var opts = arguments[1] || {};
                    UIEvent.call(this, type, opts);
                    var o = opts;
                    this.data = o.data !== undefined ? o.data : null;
                    this.inputType = o.inputType !== undefined ? String(o.inputType) : '';
                    this.isComposing = !!o.isComposing;
                    this.dataTransfer = o.dataTransfer || null;
                };
                InputEvent.prototype = Object.create(UIEvent.prototype);
                InputEvent.prototype.constructor = InputEvent;
            }

            // ── WheelEvent ──
            if (!globalThis.WheelEvent) {
                globalThis.WheelEvent = function WheelEvent(type) {
                    var opts = arguments[1] || {};
                    MouseEvent.call(this, type, opts);
                    var o = opts;
                    this.deltaX = o.deltaX || 0;
                    this.deltaY = o.deltaY || 0;
                    this.deltaZ = o.deltaZ || 0;
                    this.deltaMode = o.deltaMode || 0;
                };
                WheelEvent.prototype = Object.create(MouseEvent.prototype);
                WheelEvent.prototype.constructor = WheelEvent;
            }
            WheelEvent.DOM_DELTA_PIXEL = 0;
            WheelEvent.DOM_DELTA_LINE = 1;
            WheelEvent.DOM_DELTA_PAGE = 2;
            WheelEvent.prototype.DOM_DELTA_PIXEL = 0;
            WheelEvent.prototype.DOM_DELTA_LINE = 1;
            WheelEvent.prototype.DOM_DELTA_PAGE = 2;
            // NOTE: initWheelEvent/initWebKitWheelEvent intentionally NOT on prototype (spec removed them)

            // ── PointerEvent ──
            if (!globalThis.PointerEvent) {
                globalThis.PointerEvent = function PointerEvent(type) {
                    var opts = arguments[1] || {};
                    MouseEvent.call(this, type, opts);
                    var o = opts;
                    this.pointerId = o.pointerId || 0;
                    this.width = o.width !== undefined ? o.width : 1;
                    this.height = o.height !== undefined ? o.height : 1;
                    this.pressure = o.pressure || 0;
                    this.tangentialPressure = o.tangentialPressure || 0;
                    this.tiltX = o.tiltX || 0; this.tiltY = o.tiltY || 0;
                    this.twist = o.twist || 0;
                    this.altitudeAngle = o.altitudeAngle || 0;
                    this.azimuthAngle = o.azimuthAngle || 0;
                    this.pointerType = o.pointerType !== undefined ? String(o.pointerType) : '';
                    this.isPrimary = !!o.isPrimary;
                };
                PointerEvent.prototype = Object.create(MouseEvent.prototype);
                PointerEvent.prototype.constructor = PointerEvent;
            }
            PointerEvent.prototype.getCoalescedEvents = function() { return []; };
            PointerEvent.prototype.getPredictedEvents = function() { return []; };

            // ── CompositionEvent ──
            if (!globalThis.CompositionEvent) {
                globalThis.CompositionEvent = function CompositionEvent(type) {
                    var opts = arguments[1] || {};
                    UIEvent.call(this, type, opts);
                    this.data = opts.data !== undefined ? String(opts.data) : '';
                };
                CompositionEvent.prototype = Object.create(UIEvent.prototype);
                CompositionEvent.prototype.constructor = CompositionEvent;
            }
            CompositionEvent.prototype.initCompositionEvent = function(type, bubbles, cancelable, view, data) {
                if (arguments.length < 1) throw new TypeError("Failed to execute 'initCompositionEvent': 1 argument required, but only 0 present.");
                if (this._dispatching) return;
                this.initUIEvent(type, bubbles, cancelable, view, 0);
                this.data = data !== undefined ? String(data) : '';
            };

            // ── TextEvent (legacy, created only via createEvent) ──
            if (!globalThis.TextEvent) {
                globalThis.TextEvent = function TextEvent() {
                    throw new TypeError("Illegal constructor");
                };
                TextEvent.prototype = Object.create(UIEvent.prototype);
                TextEvent.prototype.constructor = TextEvent;
                TextEvent.prototype.data = '';
                TextEvent.prototype.initTextEvent = function(type, bubbles, cancelable, view, data) {
                    if (arguments.length < 1) throw new TypeError("Failed to execute 'initTextEvent': 1 argument required, but only 0 present.");
                    if (this._dispatching) return;
                    this.initUIEvent(type, bubbles, cancelable, view, 0);
                    this.data = data !== undefined ? String(data) : 'undefined';
                };
            }
        })();

        // ─── Touch API (W3C Touch Events) ───────────────────────────────────
        (function() {
            globalThis.Touch = function Touch(opts) {
                if (!opts || opts.identifier === undefined || !opts.target) {
                    throw new TypeError("Failed to construct 'Touch': required member identifier/target is not provided.");
                }
                this.identifier = opts.identifier;
                this.target = opts.target;
                this.screenX = opts.screenX || 0;
                this.screenY = opts.screenY || 0;
                this.clientX = opts.clientX || 0;
                this.clientY = opts.clientY || 0;
                this.pageX = opts.pageX !== undefined ? opts.pageX : this.clientX;
                this.pageY = opts.pageY !== undefined ? opts.pageY : this.clientY;
                this.radiusX = opts.radiusX || 0;
                this.radiusY = opts.radiusY || 0;
                this.rotationAngle = opts.rotationAngle || 0;
                this.force = opts.force || 0;
                this.altitudeAngle = opts.altitudeAngle !== undefined ? opts.altitudeAngle : (Math.PI / 2);
                this.azimuthAngle = opts.azimuthAngle || 0;
                this.touchType = opts.touchType || 'direct';
            };
            // NOTE: webkitRadiusX/Y/webkitRotationAngle/webkitForce are instance properties only (not on prototype)
            // The WPT tests verify they are NOT on the prototype

            globalThis.TouchList = function TouchList(touches) {
                var list = touches || [];
                this.length = list.length;
                for (var i = 0; i < list.length; i++) this[i] = list[i];
            };
            TouchList.prototype.item = function(i) { return this[i] || null; };
            // NOTE: identifiedTouch intentionally NOT on prototype (removed from spec, WPT tests its absence)

            globalThis.TouchEvent = function TouchEvent(type, opts) {
                UIEvent.call(this, type, opts || {});
                var o = opts || {};
                this.touches = o.touches || new TouchList();
                this.targetTouches = o.targetTouches || new TouchList();
                this.changedTouches = o.changedTouches || new TouchList();
                this.ctrlKey = !!o.ctrlKey;
                this.shiftKey = !!o.shiftKey;
                this.altKey = !!o.altKey;
                this.metaKey = !!o.metaKey;
            };
            TouchEvent.prototype = Object.create(UIEvent.prototype);
            TouchEvent.prototype.constructor = TouchEvent;

            // NOTE: ontouchstart/end/move/cancel are NOT registered on prototypes
            // WPT tests verify their absence from GlobalEventHandlers when not in a touch context
        })();

        // ─── CSS.supports() ─────────────────────────────────────────────────
        (function() {
            var CSS = globalThis.CSS || {};
            CSS.supports = function supports(prop, val) {
                if (arguments.length === 1) {
                    // CSS.supports("display: flex") syntax
                    var str = String(prop).trim();
                    // @supports condition — basic parsing
                    if (str.indexOf(':') === -1) return false;
                    var parts = str.split(':');
                    prop = parts[0].trim();
                    val = parts.slice(1).join(':').trim();
                }
                prop = String(prop).trim();
                val = String(val).trim();
                // Known CSS properties (extensive list)
                var known = [
                    'display','color','background','background-color','margin','padding',
                    'border','width','height','font-size','font-family','font-weight',
                    'text-align','text-decoration','position','top','left','right','bottom',
                    'float','clear','overflow','z-index','opacity','visibility',
                    'flex','flex-direction','flex-wrap','justify-content','align-items',
                    'align-content','align-self','order','flex-grow','flex-shrink','flex-basis',
                    'grid','grid-template-columns','grid-template-rows','grid-gap','gap',
                    'transform','transition','animation','box-shadow','border-radius',
                    'outline','cursor','pointer-events','user-select','content',
                    'min-width','max-width','min-height','max-height',
                    'line-height','letter-spacing','word-spacing','white-space',
                    'text-transform','text-indent','vertical-align',
                    'list-style','list-style-type','table-layout','border-collapse',
                    'background-image','background-size','background-position','background-repeat',
                    'box-sizing','resize','appearance','filter','backdrop-filter',
                    'clip-path','mask','object-fit','object-position','scroll-behavior',
                    'accent-color','aspect-ratio','container-type','container-name',
                    'inset','isolation','mix-blend-mode','will-change','writing-mode',
                    'column-count','column-gap','column-width','column-span',
                    'text-overflow','word-break','overflow-wrap','hyphens',
                    'font-style','font-variant','text-shadow','direction',
                    'unicode-bidi','all','contain','touch-action',
                    'overscroll-behavior','scroll-snap-type','scroll-snap-align',
                    'rotate','scale','translate','offset-path',
                    'color-scheme','forced-color-adjust','print-color-adjust',
                    'background-blend-mode'
                ];
                if (known.indexOf(prop) === -1) return false;
                // Two-argument form: validate value isn't obviously invalid
                if (arguments.length >= 2) {
                    val = String(val).trim();
                    if (!val) return false;
                    // Reject bare numbers for properties that need units (quirky length test)
                    if (prop !== 'opacity' && prop !== 'z-index' && prop !== 'order' &&
                        prop !== 'flex-grow' && prop !== 'flex-shrink' && prop !== 'line-height' &&
                        prop !== 'font-weight' && prop !== 'column-count') {
                        // Bare number without units is quirky length — not valid in CSS.supports
                        if (/^\d+$/.test(val) && val !== '0') return false;
                    }
                    // Reject obviously invalid color values (quirky color test)
                    if (prop === 'color' || prop === 'background-color') {
                        // Valid: keywords, #hex, rgb(), hsl(), transparent, inherit, etc.
                        var colorValid = /^(#[0-9a-fA-F]{3,8}|rgba?\s*\(|hsla?\s*\(|transparent|inherit|initial|unset|currentColor|revert|[a-zA-Z]+)$/i;
                        if (!colorValid.test(val)) return false;
                    }
                }
                return true;
            };
            CSS.escape = function(str) {
                return String(str).replace(/([^\w-])/g, '\\$1');
            };
            globalThis.CSS = CSS;
        })();

        // ─── document.hidden / visibilityState ──────────────────────────────
        if (typeof document !== 'undefined') {
            Object.defineProperty(document, 'hidden', {
                value: false, configurable: true, enumerable: true
            });
            Object.defineProperty(document, 'visibilityState', {
                value: 'visible', configurable: true, enumerable: true
            });
            // onvisibilitychange
            Object.defineProperty(document, 'onvisibilitychange', {
                get: function() { return this._onvisibilitychange || null; },
                set: function(v) { this._onvisibilitychange = v; },
                configurable: true, enumerable: true
            });
            // document.onmessageerror
            Object.defineProperty(document, 'onmessageerror', {
                get: function() { return this._onmessageerror || null; },
                set: function(v) { this._onmessageerror = v; },
                configurable: true, enumerable: true
            });
        }

        // ─── BroadcastChannel ───────────────────────────────────────────────
        (function() {
            var channels = {};
            globalThis.BroadcastChannel = function BroadcastChannel(name) {
                this.name = String(name);
                this.onmessage = null;
                this.onmessageerror = null;
                this._closed = false;
                if (!channels[this.name]) channels[this.name] = [];
                channels[this.name].push(this);
            };
            BroadcastChannel.prototype.postMessage = function(message) {
                if (this._closed) throw new DOMException('BroadcastChannel is closed', 'InvalidStateError');
                var name = this.name;
                var sender = this;
                // Deliver asynchronously to other channels with same name
                var peers = channels[name] || [];
                for (var i = 0; i < peers.length; i++) {
                    var peer = peers[i];
                    if (peer !== sender && !peer._closed && typeof peer.onmessage === 'function') {
                        (function(p, msg) {
                            var ev = new Event('message');
                            ev.data = msg;
                            ev.origin = '';
                            ev.source = null;
                            p.onmessage(ev);
                        })(peer, message);
                    }
                }
            };
            BroadcastChannel.prototype.close = function() {
                this._closed = true;
                var list = channels[this.name];
                if (list) {
                    var idx = list.indexOf(this);
                    if (idx !== -1) list.splice(idx, 1);
                }
            };
            BroadcastChannel.prototype.addEventListener = function(type, fn) {
                if (type === 'message') this.onmessage = fn;
                if (type === 'messageerror') this.onmessageerror = fn;
            };
            BroadcastChannel.prototype.removeEventListener = function(type) {
                if (type === 'message') this.onmessage = null;
                if (type === 'messageerror') this.onmessageerror = null;
            };
            BroadcastChannel.prototype.dispatchEvent = function(ev) {
                if (ev.type === 'message' && this.onmessage) this.onmessage(ev);
                if (ev.type === 'messageerror' && this.onmessageerror) this.onmessageerror(ev);
            };
        })();

        // ─── XPath API (document.evaluate, XPathResult, XPathEvaluator) ─────
        (function() {
            // XPathResult constants
            var XPR = {
                ANY_TYPE: 0,
                NUMBER_TYPE: 1,
                STRING_TYPE: 2,
                BOOLEAN_TYPE: 3,
                UNORDERED_NODE_ITERATOR_TYPE: 4,
                ORDERED_NODE_ITERATOR_TYPE: 5,
                UNORDERED_NODE_SNAPSHOT_TYPE: 6,
                ORDERED_NODE_SNAPSHOT_TYPE: 7,
                ANY_UNORDERED_NODE_TYPE: 8,
                FIRST_ORDERED_NODE_TYPE: 9
            };

            globalThis.XPathResult = function XPathResult() {};
            for (var c in XPR) { XPathResult[c] = XPR[c]; XPathResult.prototype[c] = XPR[c]; }

            // Split XPath function arguments respecting quotes and nested parens
            function splitXPathArgs(argsStr) {
                var result = [];
                var depth = 0;
                var current = '';
                var inQuote = false;
                var quoteChar = '';
                for (var i = 0; i < argsStr.length; i++) {
                    var ch = argsStr[i];
                    if (inQuote) {
                        current += ch;
                        if (ch === quoteChar) inQuote = false;
                    } else if (ch === '"' || ch === "'") {
                        inQuote = true;
                        quoteChar = ch;
                        current += ch;
                    } else if (ch === '(') {
                        depth++;
                        current += ch;
                    } else if (ch === ')') {
                        depth--;
                        current += ch;
                    } else if (ch === ',' && depth === 0) {
                        result.push(current);
                        current = '';
                    } else {
                        current += ch;
                    }
                }
                if (current) result.push(current);
                return result;
            }

            // Evaluate an XPath expression and return its string value
            function xpathStringValue(expr, contextNode, resolver) {
                expr = expr.trim();
                // String literal
                if ((expr.charAt(0) === '"' && expr.charAt(expr.length - 1) === '"') ||
                    (expr.charAt(0) === "'" && expr.charAt(expr.length - 1) === "'")) {
                    return expr.substring(1, expr.length - 1);
                }
                // Number literal
                if (/^-?\d+(\.\d+)?$/.test(expr)) return expr;
                // Context node
                if (expr === '.' || expr === 'self::node()') {
                    return textOf(contextNode);
                }
                // Evaluate as full XPath (ANY_TYPE to preserve numbers)
                var r = evaluateXPath(expr, contextNode, resolver, 0);
                if (r.resultType === 1) return String(r.numberValue);
                if (r.resultType === 2) return r.stringValue || '';
                if (r.resultType === 3) return String(r.booleanValue);
                // Nodeset: return text content of first node
                if (r._nodes && r._nodes.length > 0) return textOf(r._nodes[0]);
                return r.stringValue || '';
            }

            // Helper: textOf — get string value of node (defined before evaluateXPath)
            function textOf(node) {
                if (!node) return '';
                if (node.nodeType === 3 || node.nodeType === 8) return node.data || node.textContent || '';
                return node.textContent || '';
            }

            // Resolve namespace prefix via resolver
            function resolveNS(resolver, prefix) {
                if (!resolver) return null;
                if (typeof resolver === 'function') return resolver(prefix);
                if (resolver.lookupNamespaceURI) return resolver.lookupNamespaceURI(prefix);
                return null;
            }

            // Simple XPath evaluator — handles common patterns
            function evaluateXPath(expression, contextNode, resolver, resultType) {
                var expr = expression.trim();
                var nodes = [];

                // Handle namespace prefixes: call resolver for any prefix:localname
                if (resolver && /[a-zA-Z_][\w-]*:/.test(expr)) {
                    var nsMatch = expr.match(/([a-zA-Z_][\w-]*):([a-zA-Z_][\w-]*)/);
                    if (nsMatch) {
                        var prefix = nsMatch[1];
                        // Ignore axis names (child::, descendant::, etc.)
                        var axisNames = ['child','descendant','descendant-or-self','parent','ancestor',
                            'ancestor-or-self','following','following-sibling','preceding','preceding-sibling',
                            'self','attribute','namespace'];
                        if (axisNames.indexOf(prefix) === -1) {
                            resolveNS(resolver, prefix); // Call resolver per spec
                        }
                    }
                }
                var numberVal = NaN;
                var stringVal = '';
                var boolVal = false;

                // Helper: get all descendants
                function getDescendants(node, includeRoot) {
                    var result = [];
                    if (includeRoot) result.push(node);
                    if (node.childNodes) {
                        for (var i = 0; i < node.childNodes.length; i++) {
                            var desc = getDescendants(node.childNodes[i], true);
                            for (var j = 0; j < desc.length; j++) result.push(desc[j]);
                        }
                    }
                    return result;
                }

                // Find top-level operator position (not inside parens/quotes)
                function findTopLevelOp(s, ops) {
                    var depth = 0;
                    var inQ = false;
                    var qC = '';
                    for (var i = 0; i < s.length; i++) {
                        var c = s[i];
                        if (inQ) { if (c === qC) inQ = false; continue; }
                        if (c === '"' || c === "'") { inQ = true; qC = c; continue; }
                        if (c === '(') { depth++; continue; }
                        if (c === ')') { depth--; continue; }
                        if (depth > 0) continue;
                        for (var oi = 0; oi < ops.length; oi++) {
                            var op = ops[oi];
                            if (s.substring(i, i + op.length) === op) {
                                // For word operators (or/and), check word boundaries
                                if (/^[a-z]+$/.test(op)) {
                                    var before = i > 0 ? s[i-1] : ' ';
                                    var after = i + op.length < s.length ? s[i + op.length] : ' ';
                                    if (/\s/.test(before) && /\s/.test(after)) {
                                        return { pos: i, op: op };
                                    }
                                } else {
                                    // Symbol ops: require space context to avoid matching inside paths
                                    if (i > 0 && /\s/.test(s[i-1])) {
                                        return { pos: i, op: op };
                                    }
                                }
                            }
                        }
                    }
                    return null;
                }

                // Helper to coerce XPathResult to boolean
                function xpathBool(r) {
                    if (r.resultType === 3) return r.booleanValue;
                    if (r.resultType === 1) return !isNaN(r.numberValue) && r.numberValue !== 0;
                    if (r.resultType === 2) return r.stringValue !== '';
                    return r._nodes && r._nodes.length > 0;
                }

                // Parse and evaluate expression
                var _opHandled = false;
                try {
                    // Check for top-level boolean/comparison operators FIRST
                    var topOr = findTopLevelOp(expr, ['or']);
                    if (topOr) {
                        _opHandled = true;
                        var orLeft = expr.substring(0, topOr.pos).trim();
                        var orRight = expr.substring(topOr.pos + topOr.op.length).trim();
                        var orLR = evaluateXPath(orLeft, contextNode, resolver, 0);
                        if (xpathBool(orLR)) { boolVal = true; }
                        else { boolVal = xpathBool(evaluateXPath(orRight, contextNode, resolver, 0)); }
                    }
                    if (!_opHandled) {
                    var topAnd = findTopLevelOp(expr, ['and']);
                    if (topAnd) {
                        _opHandled = true;
                        var andLeft = expr.substring(0, topAnd.pos).trim();
                        var andRight = expr.substring(topAnd.pos + topAnd.op.length).trim();
                        var andLR = evaluateXPath(andLeft, contextNode, resolver, 0);
                        if (!xpathBool(andLR)) { boolVal = false; }
                        else { boolVal = xpathBool(evaluateXPath(andRight, contextNode, resolver, 0)); }
                    }}
                    if (!_opHandled) {
                    var topCmp = findTopLevelOp(expr, ['!=', '<=', '>=', '=', '<', '>']);
                    if (topCmp) {
                        _opHandled = true;
                        var cmpLeft = expr.substring(0, topCmp.pos).trim();
                        var cmpRight = expr.substring(topCmp.pos + topCmp.op.length).trim();
                        var cmpLV = xpathStringValue(cmpLeft, contextNode, resolver);
                        var cmpRV = xpathStringValue(cmpRight, contextNode, resolver);
                        var cmpLN = parseFloat(cmpLV), cmpRN = parseFloat(cmpRV);
                        var cmpUseNum = !isNaN(cmpLN) && !isNaN(cmpRN);
                        switch (topCmp.op) {
                            case '=': boolVal = cmpUseNum ? cmpLN === cmpRN : cmpLV === cmpRV; break;
                            case '!=': boolVal = cmpUseNum ? cmpLN !== cmpRN : cmpLV !== cmpRV; break;
                            case '<': boolVal = cmpUseNum ? cmpLN < cmpRN : cmpLV < cmpRV; break;
                            case '>': boolVal = cmpUseNum ? cmpLN > cmpRN : cmpLV > cmpRV; break;
                            case '<=': boolVal = cmpUseNum ? cmpLN <= cmpRN : cmpLV <= cmpRV; break;
                            case '>=': boolVal = cmpUseNum ? cmpLN >= cmpRN : cmpLV >= cmpRV; break;
                        }
                    }}
                    // Arithmetic operators: +, -, *, div, mod
                    if (!_opHandled) {
                    var topArith = findTopLevelOp(expr, ['+', '-', 'div', 'mod', '*']);
                    if (topArith) {
                        _opHandled = true;
                        var arLeft = expr.substring(0, topArith.pos).trim();
                        var arRight = expr.substring(topArith.pos + topArith.op.length).trim();
                        var arLV = parseFloat(xpathStringValue(arLeft, contextNode, resolver));
                        var arRV = parseFloat(xpathStringValue(arRight, contextNode, resolver));
                        if (isNaN(arLV)) arLV = 0;
                        if (isNaN(arRV)) arRV = 0;
                        switch (topArith.op) {
                            case '+': numberVal = arLV + arRV; break;
                            case '-': numberVal = arLV - arRV; break;
                            case '*': numberVal = arLV * arRV; break;
                            case 'div': numberVal = arRV !== 0 ? arLV / arRV : NaN; break;
                            case 'mod': numberVal = arRV !== 0 ? arLV % arRV : NaN; break;
                        }
                    }}

                    if (!_opHandled) {

                    // id() function
                    var idMatch = expr.match(/^id\s*\(\s*"([^"]*)"\s*\)$/);
                    if (idMatch) {
                        var ids = idMatch[1].trim().split(/\s+/);
                        var doc = contextNode.ownerDocument || contextNode;
                        // Recursive id search (handles DOMParser-created docs where
                        // getElementById might not work for all descendants)
                        function findById(node, targetId) {
                            var found = [];
                            if (node.nodeType === 1 && node.getAttribute && node.getAttribute('id') === targetId) {
                                found.push(node);
                            }
                            if (node.childNodes) {
                                for (var ci = 0; ci < node.childNodes.length; ci++) {
                                    var sub = findById(node.childNodes[ci], targetId);
                                    for (var si = 0; si < sub.length; si++) found.push(sub[si]);
                                }
                            }
                            return found;
                        }
                        for (var ii = 0; ii < ids.length; ii++) {
                            if (ids[ii]) {
                                // Try getElementById first
                                var el = doc.getElementById ? doc.getElementById(ids[ii]) : null;
                                if (el) {
                                    nodes.push(el);
                                } else {
                                    // Fallback: recursive search from documentElement
                                    var root = doc.documentElement || doc;
                                    var found = findById(root, ids[ii]);
                                    for (var fi = 0; fi < found.length; fi++) nodes.push(found[fi]);
                                }
                            }
                        }
                    }
                    // self::node()
                    else if (expr === '.' || expr === 'self::node()') {
                        nodes = [contextNode];
                    }
                    // .. or parent::node()
                    else if (expr === '..' || expr === 'parent::node()') {
                        if (contextNode.parentNode) nodes = [contextNode.parentNode];
                    }
                    // ./tagname or ./tagname[N] — child elements relative to context
                    else if (/^\.\/([a-zA-Z_][\w-]*)(\[\d+\])?$/.test(expr)) {
                        var relMatch = expr.match(/^\.\/([a-zA-Z_][\w-]*)(?:\[(\d+)\])?$/);
                        var relTag = relMatch[1];
                        var relIdx = relMatch[2] ? parseInt(relMatch[2]) - 1 : -1;
                        var relFound = [];
                        if (contextNode.childNodes) {
                            for (var rti = 0; rti < contextNode.childNodes.length; rti++) {
                                var rn = contextNode.childNodes[rti];
                                if (rn.nodeType === 1 && rn.tagName &&
                                    rn.tagName.toLowerCase() === relTag.toLowerCase()) {
                                    relFound.push(rn);
                                }
                            }
                        }
                        if (relIdx >= 0) {
                            if (relIdx < relFound.length) nodes.push(relFound[relIdx]);
                        } else {
                            nodes = relFound;
                        }
                    }
                    // (expr)[N] — positional predicate filter
                    else if (/^\(.*\)\[\d+\]$/.test(expr)) {
                        var predMatch = expr.match(/^\((.*)\)\[(\d+)\]$/);
                        if (predMatch) {
                            var innerExpr = predMatch[1];
                            var predIdx = parseInt(predMatch[2]) - 1; // XPath 1-based
                            var innerResult = evaluateXPath(innerExpr, contextNode, resolver, 7);
                            if (innerResult.snapshotLength > predIdx) {
                                nodes.push(innerResult.snapshotItem(predIdx));
                            }
                        }
                    }
                    // Simple tag name: child::tagname or just tagname
                    else if (/^(child::)?[a-zA-Z_][\w-]*$/.test(expr)) {
                        var tag = expr.replace(/^child::/, '');
                        if (contextNode.childNodes) {
                            for (var ci = 0; ci < contextNode.childNodes.length; ci++) {
                                var cn = contextNode.childNodes[ci];
                                if (cn.nodeType === 1 && cn.tagName &&
                                    cn.tagName.toLowerCase() === tag.toLowerCase()) {
                                    nodes.push(cn);
                                }
                            }
                        }
                    }
                    // //tagname — all descendants of context (or document)
                    else if (/^\/\/([a-zA-Z_][\w-]*)$/.test(expr)) {
                        var dtag = expr.substring(2).toLowerCase();
                        var allDesc = getDescendants(contextNode, false);
                        for (var di = 0; di < allDesc.length; di++) {
                            if (allDesc[di].nodeType === 1 && allDesc[di].tagName &&
                                allDesc[di].tagName.toLowerCase() === dtag) {
                                nodes.push(allDesc[di]);
                            }
                        }
                    }
                    // .//tagname — all descendants of context
                    else if (/^\.\/\/([a-zA-Z_][\w-]*)$/.test(expr)) {
                        var ddtag = expr.substring(3).toLowerCase();
                        var descNodes = getDescendants(contextNode, false);
                        for (var ddi = 0; ddi < descNodes.length; ddi++) {
                            if (descNodes[ddi].nodeType === 1 && descNodes[ddi].tagName &&
                                descNodes[ddi].tagName.toLowerCase() === ddtag) {
                                nodes.push(descNodes[ddi]);
                            }
                        }
                    }
                    // descendant::node() or descendant-or-self::node()
                    else if (expr === 'descendant::node()' || expr === 'descendant-or-self::node()') {
                        var inclSelf = expr.indexOf('-or-self') !== -1;
                        nodes = getDescendants(contextNode, inclSelf);
                    }
                    // child::node() or node()
                    else if (expr === 'child::node()' || expr === 'node()') {
                        if (contextNode.childNodes) {
                            for (var ni = 0; ni < contextNode.childNodes.length; ni++) {
                                nodes.push(contextNode.childNodes[ni]);
                            }
                        }
                    }
                    // child::text() or text()
                    else if (expr === 'text()' || expr === 'child::text()') {
                        if (contextNode.childNodes) {
                            for (var ti2 = 0; ti2 < contextNode.childNodes.length; ti2++) {
                                if (contextNode.childNodes[ti2].nodeType === 3) nodes.push(contextNode.childNodes[ti2]);
                            }
                        }
                    }
                    // child::comment()
                    else if (expr === 'comment()' || expr === 'child::comment()') {
                        if (contextNode.childNodes) {
                            for (var cmi = 0; cmi < contextNode.childNodes.length; cmi++) {
                                if (contextNode.childNodes[cmi].nodeType === 8) nodes.push(contextNode.childNodes[cmi]);
                            }
                        }
                    }
                    // child::processing-instruction()
                    else if (expr === 'processing-instruction()' || expr === 'child::processing-instruction()') {
                        if (contextNode.childNodes) {
                            for (var pi = 0; pi < contextNode.childNodes.length; pi++) {
                                if (contextNode.childNodes[pi].nodeType === 7) nodes.push(contextNode.childNodes[pi]);
                            }
                        }
                    }
                    // * (all element children)
                    else if (expr === '*' || expr === 'child::*') {
                        if (contextNode.childNodes) {
                            for (var si = 0; si < contextNode.childNodes.length; si++) {
                                if (contextNode.childNodes[si].nodeType === 1) nodes.push(contextNode.childNodes[si]);
                            }
                        }
                    }
                    // attribute::* or @*
                    else if (expr === 'attribute::*' || expr === '@*') {
                        if (contextNode.attributes) {
                            for (var ai = 0; ai < contextNode.attributes.length; ai++) {
                                nodes.push(contextNode.attributes[ai]);
                            }
                        }
                    }
                    // @attrname
                    else if (/^@([a-zA-Z_][\w-]*)$/.test(expr)) {
                        var attrName = expr.substring(1);
                        if (contextNode.getAttributeNode) {
                            var attr = contextNode.getAttributeNode(attrName);
                            if (attr) nodes.push(attr);
                        }
                    }
                    // count() function
                    else if (/^count\s*\(/.test(expr)) {
                        var inner = expr.match(/^count\s*\(\s*(.*)\s*\)$/);
                        if (inner) {
                            var innerResult = evaluateXPath(inner[1], contextNode, resolver, 7);
                            numberVal = innerResult.snapshotLength;
                        }
                    }
                    // string() function
                    else if (/^string\s*\(/.test(expr)) {
                        var sInner = expr.match(/^string\s*\(\s*(.*)\s*\)$/);
                        if (sInner && sInner[1]) {
                            var sResult = evaluateXPath(sInner[1], contextNode, resolver, 9);
                            stringVal = sResult.singleNodeValue ? textOf(sResult.singleNodeValue) : '';
                        } else {
                            stringVal = textOf(contextNode);
                        }
                    }
                    // concat() function
                    else if (/^concat\s*\(/.test(expr)) {
                        // Parsa concat-argument med korrekt XPath-semantik
                        var concatBody = expr.match(/^concat\s*\((.*)\)$/);
                        if (concatBody) {
                            var cParts = splitXPathArgs(concatBody[1]);
                            var cResult = '';
                            for (var cpi = 0; cpi < cParts.length; cpi++) {
                                cResult += xpathStringValue(cParts[cpi].trim(), contextNode, resolver);
                            }
                            stringVal = cResult;
                        }
                    }
                    // contains(str, substr) function
                    else if (/^contains\s*\(/.test(expr)) {
                        var contBody = expr.match(/^contains\s*\((.*)\)$/);
                        if (contBody) {
                            var contArgs = splitXPathArgs(contBody[1]);
                            if (contArgs.length >= 2) {
                                var contStr = xpathStringValue(contArgs[0].trim(), contextNode, resolver);
                                var contSub = xpathStringValue(contArgs[1].trim(), contextNode, resolver);
                                boolVal = contStr.indexOf(contSub) !== -1;
                            }
                        }
                    }
                    // starts-with(str, prefix)
                    else if (/^starts-with\s*\(/.test(expr)) {
                        var swBody = expr.match(/^starts-with\s*\((.*)\)$/);
                        if (swBody) {
                            var swArgs = splitXPathArgs(swBody[1]);
                            if (swArgs.length >= 2) {
                                var swStr = xpathStringValue(swArgs[0].trim(), contextNode, resolver);
                                var swPre = xpathStringValue(swArgs[1].trim(), contextNode, resolver);
                                boolVal = swStr.indexOf(swPre) === 0;
                            }
                        }
                    }
                    // normalize-space()
                    else if (/^normalize-space\s*\(/.test(expr)) {
                        var nsArg = expr.match(/^normalize-space\s*\(\s*\)$/);
                        if (nsArg) {
                            stringVal = textOf(contextNode).replace(/[\x09\x0A\x0D\x20]+/g, ' ').replace(/^ | $/g, '');
                        } else {
                            var nsArg2 = expr.match(/^normalize-space\s*\(\s*"([^"]*)"\s*\)$/);
                            if (nsArg2) {
                                stringVal = nsArg2[1].replace(/[\x09\x0A\x0D\x20]+/g, ' ').replace(/^ | $/g, '');
                            } else {
                                stringVal = textOf(contextNode).replace(/[\x09\x0A\x0D\x20]+/g, ' ').replace(/^ | $/g, '');
                            }
                        }
                    }
                    // substring-before(str, delim)
                    else if (/^substring-before\s*\(/.test(expr)) {
                        var sbBody = expr.match(/^substring-before\s*\((.*)\)$/);
                        if (sbBody) {
                            var sbParts = splitXPathArgs(sbBody[1]);
                            if (sbParts.length >= 2) {
                                var sbStr = xpathStringValue(sbParts[0].trim(), contextNode, resolver);
                                var sbDelim = xpathStringValue(sbParts[1].trim(), contextNode, resolver);
                                var sbIdx = sbStr.indexOf(sbDelim);
                                stringVal = sbIdx >= 0 ? sbStr.substring(0, sbIdx) : '';
                            }
                        }
                    }
                    // substring-after(str, delim)
                    else if (/^substring-after\s*\(/.test(expr)) {
                        var saBody = expr.match(/^substring-after\s*\((.*)\)$/);
                        if (saBody) {
                            var saParts = splitXPathArgs(saBody[1]);
                            if (saParts.length >= 2) {
                                var saStr = xpathStringValue(saParts[0].trim(), contextNode, resolver);
                                var saDelim = xpathStringValue(saParts[1].trim(), contextNode, resolver);
                                var saIdx = saStr.indexOf(saDelim);
                                stringVal = saIdx >= 0 ? saStr.substring(saIdx + saDelim.length) : '';
                            }
                        }
                    }
                    // substring(str, start, length?)
                    else if (/^substring\s*\(/.test(expr)) {
                        var subBody = expr.match(/^substring\s*\((.*)\)$/);
                        if (subBody) {
                            var subParts = splitXPathArgs(subBody[1]);
                            if (subParts.length >= 2) {
                                var subStr = xpathStringValue(subParts[0].trim(), contextNode, resolver);
                                var subStart = Math.round(parseFloat(xpathStringValue(subParts[1].trim(), contextNode, resolver))) - 1;
                                if (subParts.length >= 3) {
                                    var subLen = Math.round(parseFloat(xpathStringValue(subParts[2].trim(), contextNode, resolver)));
                                    stringVal = subStr.substring(Math.max(0, subStart), subStart + subLen);
                                } else {
                                    stringVal = subStr.substring(Math.max(0, subStart));
                                }
                            }
                        }
                    }
                    // string-length(str?)
                    else if (/^string-length\s*\(/.test(expr)) {
                        var slArg = expr.match(/^string-length\s*\(\s*"([^"]*)"\s*\)$/);
                        if (slArg) {
                            numberVal = slArg[1].length;
                        } else if (/^string-length\s*\(\s*\)$/.test(expr)) {
                            numberVal = textOf(contextNode).length;
                        }
                    }
                    // translate(str, from, to)
                    else if (/^translate\s*\(/.test(expr)) {
                        var trBody = expr.match(/^translate\s*\((.*)\)$/);
                        if (trBody) {
                            var trParts = splitXPathArgs(trBody[1]);
                            if (trParts.length >= 3) {
                                var trStr = xpathStringValue(trParts[0].trim(), contextNode, resolver);
                                var trFrom = xpathStringValue(trParts[1].trim(), contextNode, resolver);
                                var trTo = xpathStringValue(trParts[2].trim(), contextNode, resolver);
                                var tResult = '';
                                for (var ti = 0; ti < trStr.length; ti++) {
                                    var ch = trStr[ti];
                                    var fromIdx = trFrom.indexOf(ch);
                                    if (fromIdx === -1) tResult += ch;
                                    else if (fromIdx < trTo.length) tResult += trTo[fromIdx];
                                }
                                stringVal = tResult;
                            }
                        }
                    }
                    // not() function
                    else if (/^not\s*\(/.test(expr)) {
                        var notInner = expr.match(/^not\s*\(\s*(.*)\s*\)$/);
                        if (notInner) {
                            var notResult = evaluateXPath(notInner[1], contextNode, resolver, 3);
                            boolVal = !notResult.booleanValue;
                        }
                    }
                    // true()/false()
                    } // close: if (!_opHandled)
                    if (!_opHandled && expr === 'true()') { boolVal = true; }
                    else if (!_opHandled && expr === 'false()') { boolVal = false; }
                    // Number literal
                    else if (!_opHandled && /^-?\d+(\.\d+)?$/.test(expr)) {
                        numberVal = parseFloat(expr);
                    }
                    // String literal
                    else if (!_opHandled && /^["'].*["']$/.test(expr)) {
                        stringVal = expr.substring(1, expr.length - 1);
                    }
                    // lang() function
                    else if (!_opHandled && /^lang\s*\(/.test(expr)) {
                        var langArg = expr.match(/^lang\s*\(\s*"([^"]*)"\s*\)$/);
                        if (langArg) {
                            var langVal = langArg[1].toLowerCase();
                            var node = contextNode;
                            while (node) {
                                var xmlLang = (node.getAttribute && (node.getAttribute('xml:lang') || node.getAttribute('lang'))) || '';
                                if (xmlLang) {
                                    boolVal = xmlLang.toLowerCase() === langVal ||
                                              xmlLang.toLowerCase().indexOf(langVal + '-') === 0;
                                    break;
                                }
                                node = node.parentNode;
                            }
                        }
                    }
                } catch(e) {
                    // Fallback: empty result
                }

                // Determine result type
                if (resultType === 0) { // ANY_TYPE
                    if (nodes.length > 0) resultType = 4;
                    else if (!isNaN(numberVal)) resultType = 1;
                    else if (stringVal) resultType = 2;
                    else resultType = 3;
                }

                var result = new XPathResult();
                result.resultType = resultType;
                result._nodes = nodes;
                result._index = 0;

                if (resultType === 1) {
                    result.numberValue = isNaN(numberVal) ? nodes.length : numberVal;
                } else if (resultType === 2) {
                    result.stringValue = stringVal || (nodes.length > 0 ? textOf(nodes[0]) : '');
                } else if (resultType === 3) {
                    result.booleanValue = boolVal || nodes.length > 0;
                } else if (resultType === 4 || resultType === 5) {
                    result.invalidIteratorState = false;
                    result.iterateNext = function() {
                        if (this.invalidIteratorState) {
                            throw new DOMException('Document mutated since XPathResult created', 'InvalidStateError');
                        }
                        if (this._index < this._nodes.length) return this._nodes[this._index++];
                        return null;
                    };
                    // Observe DOM mutations — set invalidIteratorState on change
                    if (typeof MutationObserver !== 'undefined') {
                        var _iterResult = result;
                        var _iterObs = new MutationObserver(function() {
                            _iterResult.invalidIteratorState = true;
                            _iterObs.disconnect();
                        });
                        var _root = contextNode.ownerDocument || contextNode;
                        if (_root.documentElement) {
                            _iterObs.observe(_root.documentElement, { childList: true, subtree: true, attributes: true });
                        }
                    }
                } else if (resultType === 6 || resultType === 7) {
                    result.snapshotLength = nodes.length;
                    result.snapshotItem = function(i) { return this._nodes[i] || null; };
                    result.invalidIteratorState = false;
                } else if (resultType === 8 || resultType === 9) {
                    result.singleNodeValue = nodes.length > 0 ? nodes[0] : null;
                    result.invalidIteratorState = false;
                }

                return result;
            }

            // XPathEvaluator constructor
            globalThis.XPathEvaluator = function XPathEvaluator() {};
            XPathEvaluator.prototype.evaluate = function(expr, ctx, resolver, type) {
                return evaluateXPath(expr, ctx, resolver, type || 0);
            };
            XPathEvaluator.prototype.createExpression = function(expr, resolver) {
                return {
                    evaluate: function(ctx, type) {
                        return evaluateXPath(expr, ctx, resolver, type || 0);
                    }
                };
            };
            XPathEvaluator.prototype.createNSResolver = function(node) {
                return { lookupNamespaceURI: function() { return null; } };
            };

            // Shared XPath methods (added to any document-like object)
            var _xpathEvaluate = function(expr, ctx, resolver, type) {
                return evaluateXPath(expr, ctx || this, resolver, type || 0);
            };
            var _xpathCreateExpr = function(expr, resolver) {
                return new XPathEvaluator().createExpression(expr, resolver);
            };
            var _xpathCreateNSRes = function(node) {
                return new XPathEvaluator().createNSResolver(node);
            };

            // Set on Document.prototype for proper docs
            if (typeof Document !== 'undefined') {
                Document.prototype.evaluate = _xpathEvaluate;
                Document.prototype.createExpression = _xpathCreateExpr;
                Document.prototype.createNSResolver = _xpathCreateNSRes;
            }
            // Set directly on current document
            if (typeof document !== 'undefined') {
                document.evaluate = _xpathEvaluate;
                document.createExpression = _xpathCreateExpr;
                document.createNSResolver = _xpathCreateNSRes;
            }
            // Store XPath methods globally so they can be applied to docs created later
            globalThis.__xpathEvaluate = _xpathEvaluate;
            globalThis.__xpathCreateExpr = _xpathCreateExpr;
            globalThis.__xpathCreateNSRes = _xpathCreateNSRes;
        })();

        // ─── onmessageerror on body/window ──────────────────────────────────
        if (typeof window !== 'undefined') {
            Object.defineProperty(window, 'onmessageerror', {
                get: function() { return this._onmessageerror || null; },
                set: function(v) { this._onmessageerror = v; },
                configurable: true, enumerable: true
            });
        }
        if (typeof document !== 'undefined' && document.body) {
            Object.defineProperty(document.body, 'onmessageerror', {
                get: function() { return this._onmessageerror || null; },
                set: function(v) { this._onmessageerror = v; },
                configurable: true, enumerable: true
            });
        }

        // ─── document.execCommand (basic editing commands) ────────────────────
        if (typeof document !== 'undefined' && !document.execCommand) {
            var _supportedCmds = ['insertText','insertOrderedList','insertUnorderedList',
                'insertLineBreak','insertParagraph','insertHorizontalRule','bold','italic',
                'underline','strikethrough','delete','forwardDelete','undo','redo',
                'selectAll','formatBlock','indent','outdent','createLink','unlink',
                'insertImage','copy','cut','paste','removeFormat'];
            document.execCommand = function(cmd, showUI, value) {
                try {
                    // Hitta aktiv contenteditable eller fokuselement
                    var target = document.activeElement || document.body;
                    if (!target) return false;
                    var inputType = '';
                    var data = null;
                    switch (cmd) {
                        case 'insertText':
                            inputType = 'insertText';
                            data = value;
                            if (target.textContent !== undefined && value) {
                                target.textContent += value;
                            }
                            break;
                        case 'insertOrderedList':
                            inputType = 'insertOrderedList';
                            var ol = document.createElement('ol');
                            var li = document.createElement('li');
                            li.textContent = target.textContent || '';
                            ol.appendChild(li);
                            target.textContent = '';
                            target.appendChild(ol);
                            break;
                        case 'insertUnorderedList':
                            inputType = 'insertUnorderedList';
                            var ul = document.createElement('ul');
                            var li2 = document.createElement('li');
                            li2.textContent = target.textContent || '';
                            ul.appendChild(li2);
                            target.textContent = '';
                            target.appendChild(ul);
                            break;
                        case 'insertLineBreak':
                            inputType = 'insertLineBreak';
                            target.appendChild(document.createElement('br'));
                            break;
                        case 'insertParagraph':
                            inputType = 'insertParagraph';
                            var p = document.createElement('p');
                            target.appendChild(p);
                            break;
                        case 'insertHorizontalRule':
                            inputType = 'insertHorizontalRule';
                            target.appendChild(document.createElement('hr'));
                            break;
                        case 'bold': inputType = 'formatBold'; break;
                        case 'italic': inputType = 'formatItalic'; break;
                        case 'underline': inputType = 'formatUnderline'; break;
                        case 'strikethrough': inputType = 'formatStrikeThrough'; break;
                        case 'delete': inputType = 'deleteContentBackward'; break;
                        case 'forwardDelete': inputType = 'deleteContentForward'; break;
                        default: inputType = cmd; break;
                    }
                    // Fire beforeinput (cancelable)
                    if (typeof InputEvent !== 'undefined') {
                        var beforeEvt = new InputEvent('beforeinput', {
                            bubbles: true, cancelable: true,
                            inputType: inputType, data: data
                        });
                        target.dispatchEvent(beforeEvt);
                        if (beforeEvt.defaultPrevented) return false;
                        // Fire input (not cancelable)
                        var inputEvt = new InputEvent('input', {
                            bubbles: true, cancelable: false,
                            inputType: inputType, data: data
                        });
                        target.dispatchEvent(inputEvt);
                    }
                    return true;
                } catch(e) { return false; }
            };
            document.queryCommandEnabled = function(cmd) { return _supportedCmds.indexOf(cmd) !== -1; };
            document.queryCommandSupported = function(cmd) { return _supportedCmds.indexOf(cmd) !== -1; };
            document.queryCommandState = function(cmd) { return false; };
            document.queryCommandValue = function(cmd) { return ''; };
        }

        // ─── MessagePort / MessageChannel ───────────────────────────────────
        (function() {
            globalThis.MessagePort = function MessagePort() {
                this.onmessage = null;
                this.onmessageerror = null;
                this._otherPort = null;
                this._closed = false;
                this._started = false;
                this._queue = [];
            };
            MessagePort.prototype.postMessage = function(message) {
                if (this._closed) return;
                var other = this._otherPort;
                if (!other || other._closed) return;
                var ev = new Event('message');
                ev.data = message;
                ev.origin = '';
                ev.source = null;
                ev.ports = [];
                if (other._started && typeof other.onmessage === 'function') {
                    other.onmessage(ev);
                } else {
                    other._queue.push(ev);
                }
            };
            MessagePort.prototype.start = function() {
                this._started = true;
                // Deliver queued messages
                while (this._queue.length > 0 && this.onmessage) {
                    var ev = this._queue.shift();
                    this.onmessage(ev);
                }
            };
            MessagePort.prototype.close = function() { this._closed = true; };
            MessagePort.prototype.addEventListener = function(type, fn) {
                if (type === 'message') { this.onmessage = fn; this.start(); }
                if (type === 'messageerror') this.onmessageerror = fn;
            };
            MessagePort.prototype.removeEventListener = function(type) {
                if (type === 'message') this.onmessage = null;
                if (type === 'messageerror') this.onmessageerror = null;
            };
            MessagePort.prototype.dispatchEvent = function(ev) {
                if (ev.type === 'message' && this.onmessage) this.onmessage(ev);
                return true;
            };

            globalThis.MessageChannel = function MessageChannel() {
                this.port1 = new MessagePort();
                this.port2 = new MessagePort();
                this.port1._otherPort = this.port2;
                this.port2._otherPort = this.port1;
            };

            // window.postMessage
            if (typeof window !== 'undefined' && !window.postMessage) {
                window.postMessage = function(message, targetOrigin, transfer) {
                    var ev = new Event('message');
                    ev.data = message;
                    ev.origin = (typeof location !== 'undefined' && location.origin) || 'https://example.com';
                    ev.source = window;
                    ev.ports = transfer || [];
                    if (typeof window.onmessage === 'function') {
                        window.onmessage(ev);
                    }
                };
            }
            // NOTE: postMessage is intentionally NOT set on globalThis
            // WPT tests check that global postMessage is NOT defined in non-worker contexts
        })();

        // ─── Element.checkVisibility() ──────────────────────────────────────
        if (typeof Element !== 'undefined') {
            Element.prototype.checkVisibility = function(opts) {
                opts = opts || {};
                // Check display:none
                if (typeof getComputedStyle === 'function') {
                    try {
                        var style = getComputedStyle(this);
                        if (style && style.display === 'none') return false;
                        if (opts.checkVisibilityCSS || opts.visibilityProperty) {
                            if (style && style.visibility === 'hidden') return false;
                        }
                        if (opts.checkOpacity || opts.opacityProperty) {
                            if (style && parseFloat(style.opacity) === 0) return false;
                        }
                    } catch(e) {}
                }
                // Check inert
                var node = this;
                while (node) {
                    if (node.inert || (node.getAttribute && node.getAttribute('inert') !== null)) return false;
                    if (node.getAttribute && node.getAttribute('hidden') !== null) return false;
                    node = node.parentNode;
                }
                return true;
            };
        }

        // ─── trustedTypes / TrustedTypePolicyFactory ────────────────────────
        (function() {
            function TrustedHTML(value) { this._value = value; }
            TrustedHTML.prototype.toString = function() { return this._value; };
            TrustedHTML.prototype.toJSON = function() { return this._value; };

            function TrustedScript(value) { this._value = value; }
            TrustedScript.prototype.toString = function() { return this._value; };
            TrustedScript.prototype.toJSON = function() { return this._value; };

            function TrustedScriptURL(value) { this._value = value; }
            TrustedScriptURL.prototype.toString = function() { return this._value; };
            TrustedScriptURL.prototype.toJSON = function() { return this._value; };

            function TrustedTypePolicy(name, rules) {
                this.name = name;
                this._rules = rules || {};
            }
            TrustedTypePolicy.prototype.createHTML = function(input) {
                var transform = this._rules.createHTML;
                var result = transform ? transform(input) : input;
                return new TrustedHTML(result);
            };
            TrustedTypePolicy.prototype.createScript = function(input) {
                var transform = this._rules.createScript;
                var result = transform ? transform(input) : input;
                return new TrustedScript(result);
            };
            TrustedTypePolicy.prototype.createScriptURL = function(input) {
                var transform = this._rules.createScriptURL;
                var result = transform ? transform(input) : input;
                return new TrustedScriptURL(result);
            };

            function TrustedTypePolicyFactory() {
                this._policies = [];
                this.defaultPolicy = null;
                this.emptyHTML = new TrustedHTML('');
                this.emptyScript = new TrustedScript('');
            }
            TrustedTypePolicyFactory.prototype.createPolicy = function(name, rules) {
                var policy = new TrustedTypePolicy(name, rules);
                this._policies.push(policy);
                if (name === 'default') this.defaultPolicy = policy;
                return policy;
            };
            TrustedTypePolicyFactory.prototype.isHTML = function(value) {
                return value instanceof TrustedHTML;
            };
            TrustedTypePolicyFactory.prototype.isScript = function(value) {
                return value instanceof TrustedScript;
            };
            TrustedTypePolicyFactory.prototype.isScriptURL = function(value) {
                return value instanceof TrustedScriptURL;
            };
            TrustedTypePolicyFactory.prototype.getAttributeType = function(tagName, attribute, elementNs, attrNs) {
                tagName = (tagName || '').toLowerCase();
                attribute = (attribute || '').toLowerCase();
                // script src → TrustedScriptURL, script text → TrustedScript
                if (tagName === 'script' && attribute === 'src') return 'TrustedScriptURL';
                if (tagName === 'script' && (attribute === 'text' || attribute === 'textcontent' || attribute === 'innertext')) return 'TrustedScript';
                // iframe srcdoc → TrustedHTML
                if (tagName === 'iframe' && attribute === 'srcdoc') return 'TrustedHTML';
                // Various href attributes on specific elements
                if ((attribute === 'href' || attribute === 'xlink:href') && (tagName === 'script' || tagName === 'embed' || tagName === 'object')) return 'TrustedScriptURL';
                if (attribute === 'src' && (tagName === 'embed' || tagName === 'object' || tagName === 'frame' || tagName === 'iframe')) return 'TrustedScriptURL';
                if (attribute === 'data' && tagName === 'object') return 'TrustedScriptURL';
                if (attribute === 'codebase' && (tagName === 'object' || tagName === 'applet')) return 'TrustedScriptURL';
                // SVG specific
                if (attrNs === 'http://www.w3.org/1999/xlink' && attribute === 'href' && tagName === 'script') return 'TrustedScriptURL';
                return null;
            };
            TrustedTypePolicyFactory.prototype.getPropertyType = function(tagName, property) {
                tagName = (tagName || '').toLowerCase();
                property = (property || '');
                if (property === 'innerHTML' || property === 'outerHTML') return 'TrustedHTML';
                if (tagName === 'script' && (property === 'src' || property === 'href')) return 'TrustedScriptURL';
                if (tagName === 'script' && (property === 'text' || property === 'textContent' || property === 'innerText')) return 'TrustedScript';
                if (tagName === 'iframe' && property === 'srcdoc') return 'TrustedHTML';
                return null;
            };

            // fromLiteral — tagged template literal factory (Trusted Types spec)
            function makeFromLiteral(Cls) {
                return function fromLiteral(templateObj) {
                    // Tagged template: fromLiteral`abc` → templateObj is TemplateStringsArray
                    if (!templateObj || !templateObj.raw || !Array.isArray(templateObj.raw)) {
                        throw new TypeError('fromLiteral requires a template literal');
                    }
                    // Reject interpolations: template must have exactly 1 part
                    if (templateObj.length !== 1) {
                        throw new TypeError('fromLiteral does not allow interpolation');
                    }
                    return new Cls(templateObj[0]);
                };
            }
            TrustedHTML.fromLiteral = makeFromLiteral(TrustedHTML);
            TrustedScript.fromLiteral = makeFromLiteral(TrustedScript);
            TrustedScriptURL.fromLiteral = makeFromLiteral(TrustedScriptURL);

            globalThis.TrustedHTML = TrustedHTML;
            globalThis.TrustedScript = TrustedScript;
            globalThis.TrustedScriptURL = TrustedScriptURL;
            globalThis.TrustedTypePolicy = TrustedTypePolicy;
            globalThis.TrustedTypePolicyFactory = TrustedTypePolicyFactory;
            globalThis.trustedTypes = new TrustedTypePolicyFactory();
            if (typeof window !== 'undefined') window.trustedTypes = globalThis.trustedTypes;
        })();

        // ─── SVG DOM stubs ──────────────────────────────────────────────────
        (function() {
            if (!globalThis.SVGElement) {
                globalThis.SVGElement = function SVGElement() {};
                SVGElement.prototype = Object.create(Element.prototype);
                SVGElement.prototype.constructor = SVGElement;
            }
            // SVG geometry/graphics interfaces
            var svgTypes = ['SVGGraphicsElement','SVGGeometryElement','SVGPathElement',
                'SVGRectElement','SVGCircleElement','SVGEllipseElement','SVGLineElement',
                'SVGPolylineElement','SVGPolygonElement','SVGTextElement','SVGTextContentElement',
                'SVGSVGElement','SVGGElement','SVGDefsElement','SVGUseElement','SVGImageElement',
                'SVGClipPathElement','SVGMaskElement','SVGPatternElement','SVGMarkerElement',
                'SVGLinearGradientElement','SVGRadialGradientElement','SVGStopElement',
                'SVGForeignObjectElement','SVGSymbolElement','SVGTitleElement','SVGDescElement',
                'SVGMetadataElement','SVGSwitchElement','SVGStyleElement','SVGScriptElement',
                'SVGAElement','SVGTextPathElement','SVGTSpanElement',
                'SVGAnimatedLength','SVGAnimatedString','SVGAnimatedNumber',
                'SVGAnimatedRect','SVGAnimatedBoolean','SVGAnimatedEnumeration',
                'SVGAnimatedInteger','SVGAnimatedAngle','SVGAnimatedTransformList',
                'SVGAnimatedNumberList','SVGAnimatedLengthList','SVGAnimatedPreserveAspectRatio',
                'SVGLength','SVGLengthList','SVGNumber','SVGNumberList','SVGPoint','SVGPointList',
                'SVGMatrix','SVGRect','SVGTransform','SVGTransformList','SVGPreserveAspectRatio',
                'SVGStringList','SVGAngle'];
            for (var sti = 0; sti < svgTypes.length; sti++) {
                if (!globalThis[svgTypes[sti]]) {
                    globalThis[svgTypes[sti]] = function() {};
                    // Geometry types inherit from SVGElement
                    if (svgTypes[sti].indexOf('Element') !== -1) {
                        globalThis[svgTypes[sti]].prototype = Object.create(SVGElement.prototype);
                    }
                }
            }
            // SVGPathElement specifics
            if (SVGPathElement) {
                SVGPathElement.prototype.getTotalLength = function() { return 0; };
                SVGPathElement.prototype.getPointAtLength = function() { return { x: 0, y: 0 }; };
            }
            // SVGSVGElement specifics
            if (SVGSVGElement) {
                SVGSVGElement.prototype.createSVGPoint = function() { return { x: 0, y: 0, matrixTransform: function() { return { x: 0, y: 0 }; } }; };
                SVGSVGElement.prototype.createSVGRect = function() { return { x: 0, y: 0, width: 0, height: 0 }; };
                SVGSVGElement.prototype.createSVGMatrix = function() { return { a:1,b:0,c:0,d:1,e:0,f:0 }; };
                SVGSVGElement.prototype.createSVGTransform = function() { return { type: 0, matrix: { a:1,b:0,c:0,d:1,e:0,f:0 } }; };
            }
            // SVGGeometryElement.getBBox
            if (typeof SVGGeometryElement !== 'undefined') {
                SVGGeometryElement.prototype.getBBox = function() { return { x: 0, y: 0, width: 0, height: 0 }; };
                SVGGeometryElement.prototype.isPointInFill = function() { return false; };
                SVGGeometryElement.prototype.isPointInStroke = function() { return false; };
                SVGGeometryElement.prototype.getTotalLength = function() { return 0; };
                SVGGeometryElement.prototype.getPointAtLength = function() { return { x: 0, y: 0 }; };
            }
            if (typeof SVGGraphicsElement !== 'undefined') {
                SVGGraphicsElement.prototype.getBBox = function() { return { x: 0, y: 0, width: 0, height: 0 }; };
                SVGGraphicsElement.prototype.getCTM = function() { return { a:1,b:0,c:0,d:1,e:0,f:0 }; };
                SVGGraphicsElement.prototype.getScreenCTM = function() { return { a:1,b:0,c:0,d:1,e:0,f:0 }; };
            }
        })();

        // ─── window.onload / DOMContentLoaded support ───────────────────────
        if (typeof window !== 'undefined') {
            Object.defineProperty(window, 'onload', {
                get: function() { return this._onload || null; },
                set: function(v) { this._onload = v; },
                configurable: true, enumerable: true
            });
        }

        // ─── File API: File, FileList, FileReader ────────────────────────────
        (function() {
            // Blob (basic)
            if (!globalThis.Blob) {
                globalThis.Blob = function Blob(parts, opts) {
                    var content = '';
                    if (parts) {
                        for (var i = 0; i < parts.length; i++) {
                            content += String(parts[i]);
                        }
                    }
                    this.size = content.length;
                    this.type = (opts && opts.type) || '';
                    this._content = content;
                };
                Blob.prototype.text = function() { return Promise.resolve(this._content); };
                Blob.prototype.arrayBuffer = function() { return Promise.resolve(new ArrayBuffer(0)); };
                Blob.prototype.slice = function(start, end, type) {
                    var s = this._content.substring(start || 0, end);
                    return new Blob([s], { type: type || this.type });
                };
                Blob.prototype.stream = function() { return null; };
            }

            globalThis.File = function File(bits, name, opts) {
                Blob.call(this, bits, opts);
                this.name = name || '';
                this.lastModified = (opts && opts.lastModified) || Date.now();
                this.webkitRelativePath = '';
            };
            File.prototype = Object.create(Blob.prototype);
            File.prototype.constructor = File;

            globalThis.FileList = function FileList() {
                this.length = 0;
            };
            FileList.prototype.item = function(i) { return this[i] || null; };

            globalThis.FileReader = function FileReader() {
                this.readyState = 0; // EMPTY
                this.result = null;
                this.error = null;
                this.onload = null;
                this.onerror = null;
                this.onloadstart = null;
                this.onloadend = null;
                this.onprogress = null;
                this.onabort = null;
            };
            FileReader.EMPTY = 0;
            FileReader.LOADING = 1;
            FileReader.DONE = 2;
            FileReader.prototype.EMPTY = 0;
            FileReader.prototype.LOADING = 1;
            FileReader.prototype.DONE = 2;
            FileReader.prototype.readAsText = function(blob) {
                var self = this;
                self.readyState = 1;
                self.result = blob._content || '';
                self.readyState = 2;
                if (self.onload) self.onload({ target: self });
                if (self.onloadend) self.onloadend({ target: self });
            };
            FileReader.prototype.readAsDataURL = function(blob) {
                var self = this;
                self.readyState = 1;
                self.result = 'data:' + (blob.type || '') + ';base64,';
                self.readyState = 2;
                if (self.onload) self.onload({ target: self });
                if (self.onloadend) self.onloadend({ target: self });
            };
            FileReader.prototype.readAsArrayBuffer = function(blob) {
                var self = this;
                self.readyState = 1;
                self.result = new ArrayBuffer(0);
                self.readyState = 2;
                if (self.onload) self.onload({ target: self });
                if (self.onloadend) self.onloadend({ target: self });
            };
            FileReader.prototype.readAsBinaryString = function(blob) {
                this.readAsText(blob);
            };
            FileReader.prototype.abort = function() {
                this.readyState = 2;
                if (this.onabort) this.onabort({ target: this });
            };
            FileReader.prototype.addEventListener = function(type, fn) {
                this['on' + type] = fn;
            };
            FileReader.prototype.removeEventListener = function(type) {
                this['on' + type] = null;
            };

            // URL.createObjectURL / revokeObjectURL
            if (typeof URL !== 'undefined') {
                if (!URL.createObjectURL) {
                    URL.createObjectURL = function(blob) { return 'blob:null/' + Math.random().toString(36).substring(2); };
                }
                if (!URL.revokeObjectURL) {
                    URL.revokeObjectURL = function() {};
                }
            }

            // Expose FileList on window for "window has a FileList property" test
            if (typeof window !== 'undefined') {
                window.FileList = FileList;
                window.File = File;
                window.FileReader = FileReader;
                window.Blob = Blob;
            }
        })();

        // ─── document.write / document.writeln stubs ────────────────────────
        if (typeof document !== 'undefined') {
            if (!document.write) {
                document.write = function() {
                    var parts = [];
                    for (var i = 0; i < arguments.length; i++) {
                        var arg = arguments[i];
                        parts.push(arg && typeof arg === 'object' && arg.toString ? arg.toString() : String(arg));
                    }
                    var str = parts.join('');
                    if (document.body) {
                        document.body.innerHTML += str;
                    }
                };
            }
            if (!document.writeln) {
                document.writeln = function() {
                    var parts = [];
                    for (var i = 0; i < arguments.length; i++) {
                        var arg = arguments[i];
                        parts.push(arg && typeof arg === 'object' && arg.toString ? arg.toString() : String(arg));
                    }
                    var str = parts.join('');
                    if (document.body) {
                        document.body.innerHTML += str + '\n';
                    }
                };
            }
        }

        // ─── EditContext API stub (Input Editing) ─────────────────────────
        (function() {
            globalThis.EditContext = function EditContext(opts) {
                opts = opts || {};
                this.text = opts.text || '';
                this.selectionStart = opts.selectionStart || 0;
                this.selectionEnd = opts.selectionEnd || 0;
                this._elements = [];
                this.oncharacterboundsupdate = null;
                this.oncompositionstart = null;
                this.oncompositionend = null;
                this.ontextupdate = null;
                this.ontextformatupdate = null;
            };
            EditContext.prototype.updateText = function(start, end, newText) {
                this.text = this.text.substring(0, start) + newText + this.text.substring(end);
            };
            EditContext.prototype.updateSelection = function(start, end) {
                this.selectionStart = start;
                this.selectionEnd = end;
            };
            EditContext.prototype.updateControlBounds = function() {};
            EditContext.prototype.updateSelectionBounds = function() {};
            EditContext.prototype.updateCharacterBounds = function(start, bounds) {
                this._characterBounds = bounds || [];
                this._characterBoundsRangeStart = start || 0;
            };
            EditContext.prototype.characterBounds = function() {
                return this._characterBounds || [];
            };
            Object.defineProperty(EditContext.prototype, 'characterBoundsRangeStart', {
                get: function() { return this._characterBoundsRangeStart || 0; },
                configurable: true
            });
            EditContext.prototype.attachedElements = function() { return this._elements.slice(); };
            EditContext.prototype.addEventListener = function(type, fn) {
                this['on' + type] = fn;
            };
            EditContext.prototype.removeEventListener = function() {};

            globalThis.TextFormat = function TextFormat(opts) {
                opts = opts || {};
                this.rangeStart = opts.rangeStart || 0;
                this.rangeEnd = opts.rangeEnd || 0;
                this.underlineStyle = opts.underlineStyle || 'none';
                this.underlineThickness = opts.underlineThickness || 'none';
            };

            // HTMLElement.editContext property
            if (typeof HTMLElement !== 'undefined') {
                Object.defineProperty(HTMLElement.prototype, 'editContext', {
                    get: function() { return this._editContext || null; },
                    set: function(v) {
                        if (v !== null && !(v instanceof EditContext)) {
                            throw new TypeError("Failed to set 'editContext': The provided value is not of type 'EditContext'.");
                        }
                        if (v && v._elements.indexOf(this) === -1) v._elements.push(this);
                        this._editContext = v;
                    },
                    configurable: true, enumerable: true
                });
            }
        })();

        // ─── Element.inert property (getter/setter) ─────────────────────────
        if (typeof HTMLElement !== 'undefined') {
            Object.defineProperty(HTMLElement.prototype, 'inert', {
                get: function() { return this.hasAttribute('inert'); },
                set: function(v) {
                    if (v) this.setAttribute('inert', '');
                    else this.removeAttribute('inert');
                },
                configurable: true, enumerable: true
            });
        }

        // ─── Element.insertAdjacentHTML stub ────────────────────────────────
        if (typeof Element !== 'undefined' && !Element.prototype.insertAdjacentHTML) {
            Element.prototype.insertAdjacentHTML = function(position, text) {
                // Konvertera TrustedHTML
                if (text && typeof text === 'object' && text.toString) text = text.toString();
                var frag = document.createElement('div');
                frag.innerHTML = String(text);
                switch (String(position).toLowerCase()) {
                    case 'beforebegin':
                        if (this.parentNode) {
                            while (frag.firstChild) this.parentNode.insertBefore(frag.firstChild, this);
                        }
                        break;
                    case 'afterbegin':
                        if (frag.lastChild) {
                            var first = this.firstChild;
                            while (frag.firstChild) this.insertBefore(frag.firstChild, first);
                        }
                        break;
                    case 'beforeend':
                        while (frag.firstChild) this.appendChild(frag.firstChild);
                        break;
                    case 'afterend':
                        if (this.parentNode) {
                            var next = this.nextSibling;
                            while (frag.firstChild) this.parentNode.insertBefore(frag.firstChild, next);
                        }
                        break;
                }
            };
        }

        // ─── NodeFilter konstanter (migrerad från polyfills.js) ──────────────
        if (!globalThis.NodeFilter) globalThis.NodeFilter = {};
        NodeFilter.FILTER_ACCEPT = 1;
        NodeFilter.FILTER_REJECT = 2;
        NodeFilter.FILTER_SKIP = 3;
        NodeFilter.SHOW_ALL = 0xFFFFFFFF;
        NodeFilter.SHOW_ELEMENT = 0x1;
        NodeFilter.SHOW_ATTRIBUTE = 0x2;
        NodeFilter.SHOW_TEXT = 0x4;
        NodeFilter.SHOW_CDATA_SECTION = 0x8;
        NodeFilter.SHOW_ENTITY_REFERENCE = 0x10;
        NodeFilter.SHOW_ENTITY = 0x20;
        NodeFilter.SHOW_PROCESSING_INSTRUCTION = 0x40;
        NodeFilter.SHOW_COMMENT = 0x80;
        NodeFilter.SHOW_DOCUMENT = 0x100;
        NodeFilter.SHOW_DOCUMENT_TYPE = 0x200;
        NodeFilter.SHOW_DOCUMENT_FRAGMENT = 0x400;
        NodeFilter.SHOW_NOTATION = 0x800;

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
        Range.prototype.createContextualFragment = function(html) {
            // Spec: parse html as fragment, return DocumentFragment
            var frag = document.createDocumentFragment();
            var temp = document.createElement('div');
            temp.innerHTML = html;
            while (temp.firstChild) { frag.appendChild(temp.firstChild); }
            return frag;
        };
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
                var isXml = type && type !== 'text/html';
                // Skapa en ny document via createHTMLDocument eller fallback
                var newDoc;
                if (document.implementation && document.implementation.createHTMLDocument) {
                    newDoc = document.implementation.createHTMLDocument('');
                } else {
                    // Fallback: skapa ett enkelt doc-liknande objekt
                    newDoc = { nodeType: 9, nodeName: '#document',
                        querySelector: document.querySelector ? document.querySelector.bind(document) : function(){return null;},
                        querySelectorAll: document.querySelectorAll ? document.querySelectorAll.bind(document) : function(){return [];},
                        createElement: document.createElement.bind(document),
                        createTextNode: document.createTextNode.bind(document),
                        getElementById: function(id){return null;},
                        documentElement: document.createElement('html'),
                        body: document.createElement('body'),
                        head: document.createElement('head')
                    };
                    newDoc.documentElement.appendChild(newDoc.head);
                    newDoc.documentElement.appendChild(newDoc.body);
                }
                // XML well-formedness check via Rust
                if (isXml && str) {
                    var xmlErr = null;
                    if (typeof __checkXmlWellFormed === 'function') {
                        xmlErr = __checkXmlWellFormed(str);
                    }
                    if (xmlErr) {
                        // Parsererror: sätt som enda barn till documentElement
                        if (newDoc.documentElement) {
                            newDoc.documentElement.innerHTML = '';
                            var pe = newDoc.createElement('parsererror');
                            pe.namespaceURI = 'http://www.mozilla.org/newlayout/xml/parsererror.xml';
                            pe.textContent = 'XML Parsing Error: ' + xmlErr;
                            newDoc.documentElement.appendChild(pe);
                        }
                    } else if (str && newDoc.documentElement) {
                        newDoc.documentElement.innerHTML = str;
                    }
                } else if (str && newDoc.documentElement) {
                    newDoc.documentElement.innerHTML = str;
                }
                // Spec-krävda properties
                newDoc.contentType = type || 'text/html';
                newDoc.compatMode = (str && str.indexOf('<!DOCTYPE') !== -1) ? 'CSS1Compat' : 'BackCompat';
                newDoc.location = null;
                newDoc.URL = 'about:blank';
                newDoc.documentURI = 'about:blank';
                newDoc.nodeType = 9;
                // Bygg styleSheets från <style>-taggar i det parsade dokumentet
                var sheets = [];
                if (newDoc.getElementsByTagName) {
                    var styles = newDoc.getElementsByTagName('style');
                    if (styles && styles.length) {
                        for (var si = 0; si < styles.length; si++) {
                            var sheet = new CSSStyleSheet();
                            sheet.ownerNode = styles[si];
                            var text = styles[si].textContent || '';
                            text.replace(/\/\*[\s\S]*?\*\//g, '').split('}').forEach(function(block) {
                                block = block.trim();
                                if (block && block.indexOf('{') !== -1) {
                                    sheet.cssRules.push(new CSSRule(block + '}'));
                                }
                            });
                            sheets.push(sheet);
                        }
                    }
                }
                newDoc.styleSheets = sheets;
                // DOMParser spec: returnerar Document (INTE XMLDocument)
                if (globalThis.Document && globalThis.Document.prototype) {
                    try { Object.setPrototypeOf(newDoc, globalThis.Document.prototype); } catch(e) {}
                }
                // Ensure XPath methods are available on parsed docs
                if (typeof XPathEvaluator !== 'undefined' && !newDoc.evaluate) {
                    var _eval = new XPathEvaluator();
                    newDoc.evaluate = function(expr, ctx, res, type) { return _eval.evaluate(expr, ctx || newDoc, res, type); };
                    newDoc.createExpression = function(expr, res) { return _eval.createExpression(expr, res); };
                    newDoc.createNSResolver = function(n) { return _eval.createNSResolver(n); };
                }
                return newDoc;
            };
        };
        globalThis.XMLSerializer = function XMLSerializer() {
            this.serializeToString = function(node) {
                // Försök native XML-serialisering via Rust
                if (node && node.__nodeKey__ !== undefined && typeof __xmlSerializeNode === 'function') {
                    return __xmlSerializeNode(node.__nodeKey__);
                }
                // Fallback för DocumentFragment och liknande
                if (node && node.nodeType === 11) {
                    var result = '';
                    var kids = node.childNodes || [];
                    for (var i = 0; i < kids.length; i++) {
                        if (kids[i] && kids[i].__nodeKey__ !== undefined && typeof __xmlSerializeNode === 'function') {
                            result += __xmlSerializeNode(kids[i].__nodeKey__);
                        } else if (kids[i] && kids[i].outerHTML !== undefined) {
                            result += kids[i].outerHTML;
                        }
                    }
                    return result;
                }
                if (node && node.nodeType === 9 && node.documentElement) {
                    if (typeof __xmlSerializeNode === 'function' && node.documentElement.__nodeKey__ !== undefined) {
                        return __xmlSerializeNode(node.documentElement.__nodeKey__);
                    }
                    return node.documentElement.outerHTML || '';
                }
                if (node && node.outerHTML !== undefined) return node.outerHTML;
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

pub(super) struct ConsoleLogHandler {
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

pub(super) fn register_console<'js>(ctx: &Ctx<'js>, state: SharedState) -> rquickjs::Result<()> {
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

pub(super) struct StorageGetItem {
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

pub(super) struct StorageSetItem {
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

pub(super) struct StorageRemoveItem {
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

pub(super) struct StorageClear {
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

pub(super) struct StorageLength {
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

pub(super) struct StorageKey {
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

pub(super) fn register_storage<'js>(
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
