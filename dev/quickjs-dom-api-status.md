# QuickJS DOM API Status — Boa → QuickJS Migration

**Datum:** 2026-03-23
**Branch:** `claude/swap-boa-quickjs-cwKPo`
**Status:** 831 tester passerar, 0 failures

## Bakgrund

JS-motorn byttes från Boa (boa_engine) till QuickJS (rquickjs 0.11) för snabbare
evaluering och mindre binärstorlek. DOM-bryggan (dom_bridge.rs) skrevs om helt
med ett nytt `JsHandler`-mönster pga rquickjs livstidsinvarians i closures.

### Arkitekturmönster: JsHandler

rquickjs closures kan INTE returnera `Value<'js>` eller `Object<'js>` pga Rusts
livstidsinferens. Lösningen är trait-baserade handlers:

```rust
// Definierad i event_loop.rs
trait JsHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>>;
}
struct JsFn<H: JsHandler>(H);
impl<'js, H: JsHandler> IntoJsFunc<'js, ...> for JsFn<H> { ... }

// Användning:
struct GetElementById { state: SharedState }
impl JsHandler for GetElementById { ... }
Function::new(ctx.clone(), JsFn(GetElementById { state }))
```

## API-täckning

### Document — 12/14 (86%)

| API | Status | Kommentar |
|-----|--------|-----------|
| `getElementById(id)` | ✅ | |
| `querySelector(selector)` | ✅ | |
| `querySelectorAll(selector)` | ✅ | |
| `createElement(tag)` | ✅ | |
| `createTextNode(text)` | ✅ | |
| `createComment(text)` | ✅ | |
| `createDocumentFragment()` | ✅ | |
| `getElementsByClassName(cls)` | ✅ | |
| `getElementsByTagName(tag)` | ✅ | |
| `createRange()` | ✅ | Stubbad Range med grundläggande metoder |
| `getSelection()` | ✅ | Stubbad Selection |
| `document.body / head / documentElement` | ✅ | |
| `document.activeElement` | ❌ | Saknas — behöver fokus-tracking |
| `document.exitPointerLock()` | ❌ | Saknas — sällan använd |

### Element metoder — 16/18 (89%)

| API | Status | Kommentar |
|-----|--------|-----------|
| `getAttribute(name)` | ✅ | |
| `setAttribute(name, value)` | ✅ | |
| `removeAttribute(name)` | ✅ | |
| `hasAttribute(name)` | ✅ | |
| `appendChild(child)` | ✅ | |
| `removeChild(child)` | ✅ | |
| `insertBefore(new, ref)` | ✅ | |
| `replaceChild(new, old)` | ✅ | |
| `cloneNode(deep)` | ✅ | Rekursiv deep clone |
| `contains(other)` | ✅ | |
| `closest(selector)` | ✅ | |
| `matches(selector)` | ✅ | |
| `querySelector(sel)` | ✅ | Element-scope |
| `querySelectorAll(sel)` | ✅ | Element-scope |
| `getBoundingClientRect()` | ✅ | Estimerad layout |
| `getClientRects()` | ✅ | |
| `getAttributeNames()` | ❌ | Saknas |
| `insertAdjacentHTML(pos, html)` | ❌ | Saknas |

### Element properties — 19/19 (100%)

| API | Status | Kommentar |
|-----|--------|-----------|
| `id` | ✅ | |
| `className` | ✅ | |
| `tagName` / `nodeName` | ✅ | |
| `nodeType` | ✅ | |
| `textContent` | ✅ | Getter/setter via Accessor |
| `innerHTML` | ✅ | Getter/setter via Accessor, HTML-parsing |
| `outerHTML` | ✅ | Read-only |
| `classList` | ✅ | add, remove, toggle, contains, replace |
| `style` | ✅ | setProperty, getPropertyValue, removeProperty |
| `dataset` | ✅ | data-* attribut → camelCase |
| `hidden` | ✅ | |
| `isConnected` | ✅ | |
| `shadowRoot` | ✅ | Deklarativ Shadow DOM |
| `childElementCount` | ✅ | |
| `offsetTop/Left/Width/Height` | ✅ | Estimerad layout |
| `scrollTop/Left/Width/Height` | ✅ | |
| `clientWidth/Height` | ✅ | |
| `offsetParent` | ✅ | Lazy getter |

### Navigation — 12/12 (100%)

| API | Status | Kommentar |
|-----|--------|-----------|
| `parentNode` | ✅ | Lazy getter (undviker rekursion) |
| `parentElement` | ✅ | Lazy getter |
| `childNodes` | ✅ | Lazy getter |
| `children` | ✅ | Lazy getter (element only) |
| `firstChild` | ✅ | Lazy getter |
| `lastChild` | ✅ | Lazy getter |
| `firstElementChild` | ✅ | Lazy getter |
| `lastElementChild` | ✅ | Lazy getter |
| `nextSibling` | ✅ | Lazy getter |
| `previousSibling` | ✅ | Lazy getter |
| `nextElementSibling` | ✅ | Lazy getter |
| `previousElementSibling` | ✅ | Lazy getter |

### Event API — 3/3 (100%)

| API | Status |
|-----|--------|
| `addEventListener(type, cb, options)` | ✅ |
| `removeEventListener(type, cb)` | ✅ |
| `dispatchEvent(event)` | ✅ |

### classList — 5/5 (100%)

| API | Status |
|-----|--------|
| `add(cls)` | ✅ |
| `remove(cls)` | ✅ |
| `toggle(cls)` | ✅ |
| `contains(cls)` | ✅ |
| `replace(old, new)` | ✅ |

### style — 3/3 (100%)

| API | Status |
|-----|--------|
| `setProperty(prop, value)` | ✅ |
| `getPropertyValue(prop)` | ✅ |
| `removeProperty(prop)` | ✅ |

### Window — 14/18 (78%)

| API | Status | Kommentar |
|-----|--------|-----------|
| `innerWidth` / `innerHeight` | ✅ | 1280×900 |
| `outerWidth` / `outerHeight` | ✅ | |
| `scrollX` / `scrollY` | ✅ | |
| `devicePixelRatio` | ✅ | |
| `getComputedStyle(el)` | ✅ | Baserat på inline + tag defaults |
| `matchMedia(query)` | ✅ | |
| `scrollTo/scrollBy` | ✅ | No-op |
| `location.*` | ✅ | href, protocol, host, hostname, pathname, search, hash, origin |
| `navigator.*` | ✅ | userAgent, language, platform, cookieEnabled, onLine |
| `screen.*` | ✅ | width, height, colorDepth |
| `performance.now()` | ✅ | Elapsed µs |
| `customElements.define()` | ✅ | No-op stub |
| `location.searchParams` | ❌ | Saknas — URLSearchParams |
| `crypto.randomUUID()` | ❌ | Saknas |
| `crypto.getRandomValues()` | ❌ | Saknas |

### Globala konstruktorer

| API | Status | Kommentar |
|-----|--------|-----------|
| `Event(type, options)` | ✅ | JS-baserad stub |
| `CustomEvent(type, options)` | ✅ | JS-baserad stub med detail |
| `ResizeObserver(callback)` | ✅ | No-op observe/disconnect |
| `MutationObserver(callback)` | ✅ | Full implementation i event_loop.rs |
| `DOMParser` | ❌ | Saknas |
| `URL` | ❌ | Saknas |

### Console — 5/5 (100%)

| API | Status |
|-----|--------|
| `console.log` | ✅ |
| `console.warn` | ✅ |
| `console.error` | ✅ |
| `console.info` | ✅ |
| `console.debug` | ✅ |

### Storage (localStorage / sessionStorage) — 4/6 (67%)

| API | Status | Kommentar |
|-----|--------|-----------|
| `getItem(key)` | ✅ | |
| `setItem(key, value)` | ✅ | |
| `removeItem(key)` | ✅ | |
| `clear()` | ✅ | |
| `key(index)` | ❌ | Saknas |
| `length` | ❌ | Saknas (sätts vid skapande men uppdateras inte) |

### Event Loop (event_loop.rs) — 100%

| API | Status | Kommentar |
|-----|--------|-----------|
| `setTimeout(cb, delay)` | ✅ | Max 100 timers, max 5000ms delay |
| `setInterval(cb, delay)` | ✅ | |
| `clearTimeout(id)` | ✅ | |
| `clearInterval(id)` | ✅ | |
| `requestAnimationFrame(cb)` | ✅ | 16ms virtuell tick |
| `cancelAnimationFrame(id)` | ✅ | |
| `queueMicrotask(cb)` | ✅ | Via Promise.resolve().then() |
| `MutationObserver` | ✅ | observe, disconnect |

### Range API — 5/12 (42%)

| API | Status | Kommentar |
|-----|--------|-----------|
| `collapse()` | ✅ | No-op stub |
| `selectNode()` | ✅ | No-op stub |
| `selectNodeContents()` | ✅ | No-op stub |
| `cloneRange()` | ✅ | |
| `toString()` | ✅ | |
| `setStart/setEnd` | ❌ | Saknas |
| `setStartBefore/setEndAfter` | ❌ | Saknas |
| `deleteContents()` | ❌ | Saknas |
| `startContainer/endContainer` | ❌ | Saknas |
| `commonAncestorContainer` | ❌ | Saknas |

## Totalt

| Kategori | Implementerat | Totalt | Täckning |
|----------|---------------|--------|----------|
| Document | 12 | 14 | 86% |
| Element metoder | 16 | 18 | 89% |
| Element properties | 19 | 19 | 100% |
| Navigation | 12 | 12 | 100% |
| Events | 3 | 3 | 100% |
| classList | 5 | 5 | 100% |
| style | 3 | 3 | 100% |
| Window | 14 | 18 | 78% |
| Konstruktorer | 4 | 6 | 67% |
| Console | 5 | 5 | 100% |
| Storage | 4 | 6 | 67% |
| Event Loop | 8 | 8 | 100% |
| Range API | 5 | 12 | 42% |
| **TOTALT** | **110** | **129** | **85%** |

## Kända begränsningar

1. **QuickJS interrupt handler**: Fungerar inte under `ctx.with()` — loops avbryts
   inte. Testerna `test_infinite_loop_aborts` och `test_large_for_loop_aborts` är
   `#[ignore]`. Kräver raw FFI-lösning.

2. **Event loop RefCell**: `run_event_loop` använder raw FFI
   (`JS_IsJobPending`/`JS_ExecutePendingJob`) istället för `Runtime`-metoder
   för att undvika dubbelborrow med `Context::with()`.

3. **Persistent cleanup**: Alla `Persistent<Function>` måste rensas manuellt
   innan QuickJS-kontexten droppas, annars crashar GC. Görs via
   `state.event_listeners.clear()` + `el.clear_persistent()`.

4. **Navigation properties**: Implementerade som lazy getters (Accessor)
   istället för eager properties för att undvika oändlig rekursion vid
   objektskapande.

## Saknade API:er att prioritera

### Prio 1 (används ofta av frameworks)
- `document.activeElement`
- `storage.length` + `storage.key(index)`
- `insertAdjacentHTML(position, html)`

### Prio 2 (används ibland)
- `getAttributeNames()`
- `crypto.randomUUID()` / `crypto.getRandomValues()`
- `DOMParser` konstruktor
- `URL` konstruktor

### Prio 3 (sällan behövt i agent-kontext)
- Fullständig Range API (setStart, setEnd, etc.)
- Selection API properties
- `location.searchParams` (URLSearchParams)
