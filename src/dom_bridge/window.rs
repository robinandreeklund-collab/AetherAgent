// Window + Console + Storage + DOMException + getComputedStyle + matchMedia

use std::rc::Rc;

use rquickjs::{object::Accessor, Ctx, Function, Object, Value};

use crate::event_loop::{JsFn, JsHandler};

use super::events::{AddEventListenerHandler, DispatchEventHandler, RemoveEventListenerHandler};
use super::state::SharedState;
use super::style::kebab_to_camel;
use super::utils::{get_tag_style_defaults, parse_inline_styles, parse_media_query_matches};
#[cfg(feature = "blitz")]
use super::{build_blitz_computed_styles, map_blitz_styles_to_arena};
use super::{extract_node_key, node_key_to_f64, GetSelectionFromDoc, NoOpHandler};

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
                PROCESSING_INSTRUCTION_NODE:7,COMMENT_NODE:8,DOCUMENT_NODE:9,
                DOCUMENT_TYPE_NODE:10,DOCUMENT_FRAGMENT_NODE:11,
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

        // ─── Event Subclass Constructors (native, migrerad från polyfills.js) ─
        // UIEvent → MouseEvent/KeyboardEvent/FocusEvent/InputEvent/WheelEvent/PointerEvent
        (function() {
            if (!globalThis.UIEvent) {
                globalThis.UIEvent = function UIEvent(type, opts) {
                    Event.call(this, type, opts);
                    this.view = (opts && opts.view) || null;
                    this.detail = (opts && opts.detail !== undefined) ? opts.detail : 0;
                };
                UIEvent.prototype = Object.create(Event.prototype);
                UIEvent.prototype.constructor = UIEvent;
                UIEvent.prototype.initUIEvent = function(t,b,c,v,d) { this.initEvent(t,b,c); this.view=v||null; this.detail=d||0; };
            }
            if (!globalThis.MouseEvent) {
                globalThis.MouseEvent = function MouseEvent(type, opts) {
                    UIEvent.call(this, type, opts);
                    var o = opts || {};
                    this.screenX=o.screenX||0; this.screenY=o.screenY||0;
                    this.clientX=o.clientX||0; this.clientY=o.clientY||0;
                    this.pageX=o.pageX||0; this.pageY=o.pageY||0;
                    this.offsetX=o.offsetX||0; this.offsetY=o.offsetY||0;
                    this.movementX=o.movementX||0; this.movementY=o.movementY||0;
                    this.button=o.button||0; this.buttons=o.buttons||0;
                    this.relatedTarget=o.relatedTarget||null;
                    this.ctrlKey=!!o.ctrlKey; this.shiftKey=!!o.shiftKey;
                    this.altKey=!!o.altKey; this.metaKey=!!o.metaKey;
                };
                MouseEvent.prototype = Object.create(UIEvent.prototype);
                MouseEvent.prototype.constructor = MouseEvent;
                MouseEvent.prototype.initMouseEvent = function(t,b,c,v,d,sx,sy,cx,cy,ctrl,alt,shift,meta,btn,rt) {
                    this.initUIEvent(t,b,c,v,d); this.screenX=sx||0; this.screenY=sy||0; this.clientX=cx||0; this.clientY=cy||0;
                    this.ctrlKey=!!ctrl; this.altKey=!!alt; this.shiftKey=!!shift; this.metaKey=!!meta; this.button=btn||0; this.relatedTarget=rt||null;
                };
                MouseEvent.prototype.getModifierState = function(key) {
                    if(key==='Control')return this.ctrlKey; if(key==='Shift')return this.shiftKey;
                    if(key==='Alt')return this.altKey; if(key==='Meta')return this.metaKey; return false;
                };
            }
            if (!globalThis.KeyboardEvent) {
                globalThis.KeyboardEvent = function KeyboardEvent(type, opts) {
                    UIEvent.call(this, type, opts); var o = opts || {};
                    this.key=o.key||''; this.code=o.code||''; this.location=o.location||0;
                    this.repeat=!!o.repeat; this.isComposing=!!o.isComposing;
                    this.ctrlKey=!!o.ctrlKey; this.shiftKey=!!o.shiftKey;
                    this.altKey=!!o.altKey; this.metaKey=!!o.metaKey;
                    this.charCode=o.charCode||0; this.keyCode=o.keyCode||0; this.which=o.which||0;
                };
                KeyboardEvent.prototype = Object.create(UIEvent.prototype);
                KeyboardEvent.prototype.constructor = KeyboardEvent;
                KeyboardEvent.prototype.getModifierState = MouseEvent.prototype.getModifierState;
                KeyboardEvent.DOM_KEY_LOCATION_STANDARD=0; KeyboardEvent.DOM_KEY_LOCATION_LEFT=1;
                KeyboardEvent.DOM_KEY_LOCATION_RIGHT=2; KeyboardEvent.DOM_KEY_LOCATION_NUMPAD=3;
            }
            if (!globalThis.FocusEvent) {
                globalThis.FocusEvent = function FocusEvent(type, opts) {
                    UIEvent.call(this, type, opts);
                    this.relatedTarget = (opts && opts.relatedTarget) || null;
                };
                FocusEvent.prototype = Object.create(UIEvent.prototype);
                FocusEvent.prototype.constructor = FocusEvent;
            }
            if (!globalThis.InputEvent) {
                globalThis.InputEvent = function InputEvent(type, opts) {
                    UIEvent.call(this, type, opts); var o = opts || {};
                    this.data=o.data!==undefined?o.data:null; this.inputType=o.inputType||'';
                    this.isComposing=!!o.isComposing; this.dataTransfer=o.dataTransfer||null;
                };
                InputEvent.prototype = Object.create(UIEvent.prototype);
                InputEvent.prototype.constructor = InputEvent;
            }
            if (!globalThis.WheelEvent) {
                globalThis.WheelEvent = function WheelEvent(type, opts) {
                    MouseEvent.call(this, type, opts); var o = opts || {};
                    this.deltaX=o.deltaX||0; this.deltaY=o.deltaY||0; this.deltaZ=o.deltaZ||0;
                    this.deltaMode=o.deltaMode||0;
                };
                WheelEvent.prototype = Object.create(MouseEvent.prototype);
                WheelEvent.prototype.constructor = WheelEvent;
                WheelEvent.DOM_DELTA_PIXEL=0; WheelEvent.DOM_DELTA_LINE=1; WheelEvent.DOM_DELTA_PAGE=2;
            }
            if (!globalThis.PointerEvent) {
                globalThis.PointerEvent = function PointerEvent(type, opts) {
                    MouseEvent.call(this, type, opts); var o = opts || {};
                    this.pointerId=o.pointerId||0; this.width=o.width||1; this.height=o.height||1;
                    this.pressure=o.pressure||0; this.tangentialPressure=o.tangentialPressure||0;
                    this.tiltX=o.tiltX||0; this.tiltY=o.tiltY||0; this.twist=o.twist||0;
                    this.pointerType=o.pointerType||''; this.isPrimary=!!o.isPrimary;
                };
                PointerEvent.prototype = Object.create(MouseEvent.prototype);
                PointerEvent.prototype.constructor = PointerEvent;
            }
            if (!globalThis.CompositionEvent) {
                globalThis.CompositionEvent = function CompositionEvent(type, opts) {
                    UIEvent.call(this, type, opts);
                    this.data = (opts && opts.data !== undefined) ? opts.data : '';
                };
                CompositionEvent.prototype = Object.create(UIEvent.prototype);
                CompositionEvent.prototype.constructor = CompositionEvent;
            }
        })();

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
