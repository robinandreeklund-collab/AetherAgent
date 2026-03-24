# DOM API Coverage

> Full reference for AetherAgent's DOM API implementation.
> For a summary, see the [main README](../README.md).

## WPT Conformance (Web Platform Tests)

Real, unmodified tests from https://github.com/web-platform-tests/wpt run directly against AetherAgent's DOM via QuickJS sandbox.

| Suite | Cases | Passed | Rate | Date |
|-------|-------|--------|------|------|
| **dom/** (total) | 2,004 | 1,225 | **61.1%** | 2026-03-24 |

Run WPT yourself:
```bash
./wpt/setup.sh
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose
```

---

## DOM Methods (55+)

Methods marked **Full** read/write the Arena DOM. Methods marked **Stub** return realistic defaults without real behavior.

### Document methods

| Method | Status | Details |
|--------|--------|---------|
| `getElementById(id)` | Full | O(n) recursive search by `id` attribute |
| `querySelector(sel)` | Full | Full CSS selector matching (see below) |
| `querySelectorAll(sel)` | Full | Returns JsArray of all matches |
| `createElement(tag)` | Full | Inserts new Element node into arena |
| `createTextNode(text)` | Full | Inserts new Text node into arena |
| `createComment(text)` | Full | Inserts new Comment node (Vue support) |
| `createDocumentFragment()` | Full | Creates fragment node for batch operations |
| `getElementsByClassName(cls)` | Full | Recursive class search, returns JsArray |
| `getElementsByTagName(tag)` | Full | Recursive tag search, returns JsArray |
| `document.body` | Full | Resolved from arena at init |
| `document.head` | Full | Resolved from arena at init |
| `document.documentElement` | Full | Resolved from arena at init |
| `document.activeElement` | Full | Returns focused element or body (default) |
| `document.readyState` | Full | "loading" → "interactive" → "complete" lifecycle |
| `document.createRange()` | Full | Range with collapse, selectNode, setStart/End, cloneRange, getBoundingClientRect |
| `document.getSelection()` | Full | Selection with anchorNode, focusNode, removeAllRanges, addRange, collapse |
| `document.title` | Polyfill | Getter/setter via `<title>` element (WPT polyfill) |
| `document.URL` | Polyfill | Returns window.location.href |
| `document.location` | Polyfill | Alias for window.location |
| `document.createEvent()` | Polyfill | Basic Event factory with initEvent() |
| `document.exitPointerLock()` | Stub | No-op |

### Element methods

| Method | Status | Details |
|--------|--------|---------|
| `getAttribute(name)` | Full | Reads from attributes, O(1) |
| `setAttribute(name, value)` | Full | Writes to arena, logs mutation |
| `removeAttribute(name)` | Full | Removes from arena, logs mutation |
| `hasAttribute(name)` | Full | Checks presence |
| `getAttributeNames()` | Full | Returns all attribute names |
| `textContent` (getter/setter) | Full | Recursive text extraction / replacement |
| `innerHTML` (getter/setter) | Full | Serializes/parses HTML |
| `outerHTML` (getter) | Full | Serializes element + children |
| `appendChild(child)` | Full | Moves node in arena, updates parent refs |
| `removeChild(child)` | Full | Removes from arena, clears parent ref |
| `insertBefore(new, ref)` | Full | Index-based insertion in children vec |
| `replaceChild(new, old)` | Full | Combined remove + insert |
| `cloneNode(deep)` | Full | Recursive deep copy in arena |
| `parentNode` / `parentElement` | Full | Returns parent key from arena |
| `childNodes` | Full | Returns JsArray of all children |
| `children` | Full | Returns JsArray of element children only |
| `firstChild` / `lastChild` | Full | First/last child key |
| `firstElementChild` / `lastElementChild` | Full | First/last Element child |
| `nextSibling` / `previousSibling` | Full | Sibling navigation |
| `nextElementSibling` / `previousElementSibling` | Full | Element sibling navigation |
| `childElementCount` | Full | Count of element children |
| `closest(selector)` | Full | Traverses ancestors, matches CSS selector |
| `matches(selector)` | Full | Tests if element matches CSS selector |
| `contains(other)` | Full | Recursive descendant check |
| `isConnected` | Full | Traverses parent chain to document |
| `getRootNode()` | Full | Walks parent chain to root |
| `dataset` | Full | Reads `data-*` attributes, kebab→camelCase |
| `id` / `className` / `tagName` / `nodeName` | Full | Properties from arena |
| `nodeType` | Full | 1=Element, 3=Text, 8=Comment, 9=Document |
| `hidden` | Full | Bound to `hidden` HTML attribute |
| `remove()` | Polyfill | `parentNode.removeChild(this)` |
| `before()` / `after()` | Polyfill | ChildNode insertion methods |
| `replaceWith()` | Polyfill | ChildNode replacement |
| `prepend()` / `append()` | Polyfill | ParentNode convenience methods |
| `replaceChildren()` | Polyfill | Clear + append |
| `insertAdjacentElement()` | Polyfill | Position-based element insertion |
| `insertAdjacentHTML()` | Full | Position-based HTML insertion |
| `insertAdjacentText()` | Polyfill | Position-based text insertion |
| `ownerDocument` | Polyfill | Returns document (on created elements) |
| `classList.add/remove/toggle/contains/replace` | Full | Class manipulation |
| `classList.value()` / `classList.length()` | Full | Class string / count |
| `addEventListener(type, fn, capture)` | Full | Per-node event registration |
| `removeEventListener(type, fn)` | Full | Removes matching listener |
| `dispatchEvent(event)` | Full | Fires listeners + ancestor bubbling |
| `focus()` / `blur()` | Full | Focus tracking in BridgeState |
| `click()` | Full | Dispatches click event |
| `scrollIntoView(options)` | Full | Updates scroll position |
| `getBoundingClientRect()` | Full | Estimated rect from tag+style |
| `getClientRects()` | Full | Array with estimated rect |
| `style.setProperty/getPropertyValue/removeProperty` | Full | Inline style manipulation |
| `style.cssText` | Full | Raw style attribute |
| `style.[property]` | Full | 21 CSS properties as camelCase |
| `shadowRoot` | Full | Declarative Shadow DOM via `<template shadowrootmode>` |
| `attachShadow()` | Full | Creates shadow root |
| `offsetTop/Left/Width/Height` | Full | Estimated from tag+style |
| `scrollTop/Left/Width/Height` | Full | Tracked per node |
| `clientWidth/Height` | Full | Same as offset dimensions |
| `requestPointerLock()` | Stub | No-op |

### Window & global methods

| Method | Status | Details |
|--------|--------|---------|
| `window.innerWidth/innerHeight` | Stub | 1024/768 |
| `window.location.*` | Full | href, hostname, pathname, protocol, search, hash, origin, port, searchParams |
| `window.navigator.*` | Stub | userAgent="AetherAgent/0.1", language="en" |
| `getComputedStyle(el)` | Full | Merges inline styles + tag defaults, 15 CSS properties |
| `matchMedia(query)` | Stub | Returns matches=true for all queries |
| `IntersectionObserver` | Full | Fires callback per element on observe() |
| `ResizeObserver` | Full | Fires callback on observe() with contentRect |
| `Event` constructor | Full | `new Event('click', {bubbles, cancelable, composed})` |
| `CustomEvent` constructor | Full | `new CustomEvent('x', {detail, ...})` |
| `customElements.define/get/whenDefined` | Full | Web Components registry |
| `MutationObserver` | Full | observe/disconnect via event loop |
| `setTimeout/setInterval` | Full | Virtual clock, max 500 timers, 5s delay |
| `clearTimeout/clearInterval` | Full | Cancel by ID |
| `requestAnimationFrame/cancelAnimationFrame` | Full | Simulated 16ms ticks |
| `queueMicrotask` | Full | Delegates to QuickJS job queue |
| `Promise` | Full | Via QuickJS native Promise + job queue |
| `DOMParser` | Full | parseFromString() creates document |
| `URL/URLSearchParams` | Full | URL parsing and search param manipulation |
| `crypto.randomUUID/getRandomValues` | Full | Crypto API |
| `atob/btoa` | Full | Base64 encode/decode |
| `console.log/warn/error/info` | Stub | Captured in BridgeState |
| `performance.now()` | Full | Monotonic time |
| `scrollTo/scrollBy/scroll` | Full | Scroll position tracking |

### CSS Selector support

Used by `querySelector`, `querySelectorAll`, `closest`, `matches`:

| Selector | Example | Status |
|----------|---------|--------|
| ID | `#myid` | Full |
| Class | `.myclass` | Full |
| Tag | `div` | Full |
| Combined | `div.cls` | Full |
| Attribute presence | `[data-id]` | Full |
| Attribute value | `[type="text"]` | Full |
| Attribute word | `[class~="word"]` | Full |
| Attribute prefix | `[lang\|="en"]` | Full |
| Tag + attribute | `input[type="text"]` | Full |
| Child combinator | `div > span` | Full |
| Descendant combinator | `div span` | Full |
| Pseudo-class | `:first-child` | Full |
| Nth-child | `:nth-child(n)` | Full |
| Nth-of-type | `:nth-of-type(n)` | Full |
| Multiple selectors | `h1, h2, h3` | Full |
| Complex combination | `div.container > a.link` | Full |

### Global type constructors (WPT polyfill)

These are defined as constructor stubs for `instanceof` checks in WPT tests:

`HTMLElement`, `HTMLDivElement`, `HTMLSpanElement`, `HTMLAnchorElement`, `HTMLButtonElement`, `HTMLInputElement`, `HTMLFormElement`, `HTMLSelectElement`, `HTMLImageElement`, `HTMLTableElement`, `Text`, `Comment`, `DocumentFragment`, `Document`, `Element`, `CharacterData`, `Attr`, `NamedNodeMap`, `NodeList`, `HTMLCollection`, `DOMTokenList`, `Node`, `DOMException`, and 40+ more.

---

## Framework Coverage Estimates

| Framework / Scenario | Coverage | Notes |
|---------------------|----------|-------|
| **React SSR hydration** | ~95% | getElementById, textContent, classList, appendChild, addEventListener, Event |
| **Vue 3 mount + reactivity** | ~92% | querySelector, classList, createComment, setAttribute, addEventListener |
| **Svelte compiled output** | ~95% | Direct DOM manipulation + events + style.setProperty |
| **Angular Universal** | ~88% | querySelector, classList, setAttribute, events, getComputedStyle |
| **Vanilla JS / jQuery** | ~98% | All query + manipulation + event + style methods |
| **Next.js App Router** | ~88% | RSC Flight Protocol (Tier 0) + DOM bridge |
| **Nuxt 3 / SvelteKit** | ~90% | Devalue hydration (Tier 0) + DOM bridge |
| **Web Components (Lit, Stencil)** | ~82% | customElements.define, shadowRoot, isConnected |
| **Chart.js / D3** | ~30% | Requires SVG/Canvas — escalate to Tier 3/4 |
| **WebGL / Canvas apps** | ~5% | No Canvas API — must use Tier 4 (CDP) |

---

## Known Limitations (from WPT analysis)

1. **Element identity**: Each `getElementById`/`querySelector` call creates a new JS proxy object. WPT tests using `===` to compare DOM nodes will fail. This is a fundamental limitation of the proxy-based DOM bridge.
2. **Missing prototype chain**: Elements are plain JS objects, not `HTMLDivElement` instances. `instanceof` checks fail (stubs provided via polyfill for WPT).
3. **No `compareDocumentPosition()`**: Tree ordering comparison not yet implemented in Rust DOM bridge.
4. **No `TreeWalker` / `NodeIterator`**: DOM traversal interfaces not implemented.
5. **No `Attr` objects**: `getAttributeNode()` / `setAttributeNode()` not supported.
6. **No `NamedNodeMap`**: `element.attributes` returns plain object, not NamedNodeMap.
7. **Event phases**: Capture phase partially supported. `AT_TARGET` and `BUBBLING_PHASE` constants may be missing.
