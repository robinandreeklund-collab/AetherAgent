# DOM API Coverage

> Full reference for AetherAgent's DOM API implementation.
> For a summary, see the [main README](../README.md).

## WPT Conformance (Web Platform Tests)

Real, unmodified tests from https://github.com/web-platform-tests/wpt run directly against AetherAgent's DOM via QuickJS sandbox.

| Suite | Cases | Passed | Rate | Date |
|-------|-------|--------|------|------|
| **dom/** (total) | 19,938 | 13,383 | **67.1%** | 2026-03-25 |
| dom/ (baseline) | 2,004 | 1,382 | 69.0% | 2026-03-24 |

Run WPT yourself:
```bash
./wpt/setup.sh
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose
```

---

## DOM Methods (70+)

Methods marked **Full** read/write the Arena DOM in Rust. Methods marked **Polyfill** use JS shims.

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
| `document.doctype` | Full | Doctype node from arena (NodeType::Doctype) |
| `document.childNodes` | Full | All document children incl. doctype |
| `document.firstChild/lastChild` | Full | Navigation |
| `document.activeElement` | Full | Returns focused element or body (default) |
| `document.readyState` | Full | "loading" → "interactive" → "complete" lifecycle |
| `document.createRange()` | Polyfill | Range with full boundary tracking (JS AetherRange) |
| `document.getSelection()` | Full | Selection with anchorNode, focusNode, etc. |
| `document.compareDocumentPosition()` | Full | Returns DOCUMENT_POSITION_* bitmask |
| `document.contains()` | Full | Recursive descendant check |
| `document.lookupNamespaceURI()` | Full | Parent-chain traversal |
| `document.isSameNode/isEqualNode` | Full | NodeKey comparison / deep equality |
| `document.title` | Polyfill | Getter/setter via `<title>` element |
| `document.URL` | Polyfill | Returns window.location.href |
| `document.createEvent()` | Polyfill | Basic Event factory with initEvent() |
| `document.createAttribute(name)` | Polyfill | Creates Attr object (nodeType=2) |
| `document.implementation` | Polyfill | createDocument/createHTMLDocument/createDocumentType |

### Element methods

| Method | Status | Details |
|--------|--------|---------|
| `getAttribute(name)` | Full | Reads from attributes, O(1) |
| `setAttribute(name, value)` | Full | Writes to arena, logs mutation |
| `removeAttribute(name)` | Full | Removes from arena, logs mutation |
| `hasAttribute(name)` | Full | Checks presence |
| `getAttributeNames()` | Full | Returns all attribute names |
| `toggleAttribute(name, force)` | Full | DOMException on invalid name |
| `textContent` (getter/setter) | Full | Recursive text extraction / replacement |
| `innerHTML` (getter/setter) | Full | Serializes/parses HTML |
| `outerHTML` (getter) | Full | Serializes element + children |
| `appendChild(child)` | Full | Moves node in arena, updates parent refs |
| `removeChild(child)` | Full | Removes from arena, clears parent ref |
| `insertBefore(new, ref)` | Full | Index-based insertion in children vec |
| `replaceChild(new, old)` | Full | Combined remove + insert, recalculates pos |
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
| `ownerDocument` | Full | Set on all parsed nodes, points to document |
| `nodeValue` | Full | null for Element/Document/Doctype, data for Text/Comment |
| `nodeType` | Full | 1=Element, 3=Text, 8=Comment, 9=Document, 10=Doctype |
| `compareDocumentPosition(other)` | Full | DOCUMENT_POSITION_* bitmask |
| `isSameNode(other)` / `isEqualNode(other)` | Full | NodeKey / deep equality |
| `lookupNamespaceURI(prefix)` | Full | Parent-chain traversal |
| `dataset` | Full | Reads `data-*` attributes, kebab→camelCase |
| `id` / `className` / `tagName` / `nodeName` | Full | Properties from arena |
| `hidden` | Full | Bound to `hidden` HTML attribute |
| `classList.add/remove/toggle/contains/replace` | Full | Token validation (SyntaxError/InvalidCharacterError) |
| `classList.item/forEach/entries/keys/values` | Full | DOMTokenList iteration, Symbol.toStringTag |
| `classList.length/value` | Full | Live getters via defineProperty |
| `classList` (property) | Full | Read-only getter, assignment = no-op |
| `addEventListener(type, fn, options)` | Full | Supports {capture, passive} options object |
| `removeEventListener(type, fn)` | Full | Removes matching listener |
| `dispatchEvent(event)` | Full | Passive support, returns !defaultPrevented |
| `focus()` / `blur()` | Full | Focus tracking in BridgeState |
| `click()` | Full | Dispatches click event |
| `scrollIntoView(options)` | Full | Updates scroll position |
| `getBoundingClientRect()` | Full | Estimated rect from tag+style |
| `getClientRects()` | Full | Array with estimated rect |
| `style.setProperty/getPropertyValue/removeProperty` | Full | Inline style manipulation |
| `style.cssText` | Full | Raw style attribute |
| `style.[property]` | Full | 21 CSS properties as camelCase |
| `shadowRoot` | Full | Declarative Shadow DOM |
| `attachShadow()` | Full | Creates shadow root |
| `remove()` | Full | Migrated to Rust (2026-03-24) |
| `before()` / `after()` | Full | Migrated to Rust (2026-03-24) |
| `replaceWith()` | Full | Migrated to Rust (2026-03-24) |
| `prepend()` / `append()` | Polyfill | ParentNode convenience methods |
| `replaceChildren()` | Polyfill | Clear + append |
| `insertAdjacentElement()` | Polyfill | Position-based element insertion |
| `insertAdjacentHTML()` | Full | Position-based HTML insertion |
| `insertAdjacentText()` | Polyfill | Position-based text insertion |

### CharacterData methods (Native Rust)

| Method | Status | Details |
|--------|--------|---------|
| `.data` (get/set) | Full | UTF-16 code unit aware |
| `.length` | Full | UTF-16 length |
| `.nodeValue` | Full | Alias for data |
| `.substringData(offset, count)` | Full | UTF-16 boundary safe |
| `.appendData(data)` | Full | Appends to text |
| `.insertData(offset, data)` | Full | UTF-16 offset insert |
| `.deleteData(offset, count)` | Full | UTF-16 range delete |
| `.replaceData(offset, count, data)` | Full | UTF-16 range replace |

### Range API (Polyfill — migration priority)

| Method | Status | Details |
|--------|--------|---------|
| `setStart/setEnd(node, offset)` | Polyfill | Boundary tracking |
| `setStartBefore/After(node)` | Polyfill | Parent-index based |
| `setEndBefore/After(node)` | Polyfill | Parent-index based |
| `collapse(toStart)` | Polyfill | Boolean collapse |
| `selectNode/selectNodeContents` | Polyfill | Node selection |
| `compareBoundaryPoints(how, range)` | Polyfill | NotSupportedError for how>3 |
| `comparePoint(node, offset)` | Polyfill | IndexSizeError validation |
| `isPointInRange(node, offset)` | Polyfill | Wrapper around comparePoint |
| `intersectsNode(node)` | Polyfill | Root comparison |
| `cloneRange()` | Polyfill | State copy |
| `toString()` | Polyfill | Text extraction |
| `START_TO_START/END_TO_END` | Polyfill | Constants |

### Window & global methods

| Method | Status | Details |
|--------|--------|---------|
| `window.innerWidth/innerHeight` | Stub | 1024/768 |
| `window.location.*` | Full | href, hostname, pathname, protocol, search, hash, origin, port, searchParams |
| `window.navigator.*` | Stub | userAgent="AetherAgent/0.1", language="en" |
| `getComputedStyle(el)` | Full | Merges inline styles + tag defaults |
| `matchMedia(query)` | Stub | Returns matches=true |
| `IntersectionObserver` | Full | Fires callback per element |
| `ResizeObserver` | Full | Fires callback with contentRect |
| `Event` constructor | Full | `new Event('click', {bubbles, cancelable, composed})` |
| `CustomEvent` constructor | Full | `new CustomEvent('x', {detail, ...})` |
| `MutationObserver` | Full | Constructor + observe/disconnect (2026-03-25) |
| `setTimeout/setInterval` | Full | Virtual clock, max 500 timers, 5s delay |
| `clearTimeout/clearInterval` | Full | Cancel by ID |
| `requestAnimationFrame` | Full | Simulated 16ms ticks |
| `queueMicrotask` | Full | QuickJS job queue |
| `Promise` | Full | Via QuickJS native Promise |
| `DOMParser` | Full | parseFromString() creates document |
| `URL/URLSearchParams` | Full | URL parsing and search params |
| `crypto.randomUUID/getRandomValues` | Full | Crypto API |
| `atob/btoa` | Full | Base64 encode/decode |
| `console.log/warn/error/info` | Stub | Captured in BridgeState |
| `performance.now()` | Full | Monotonic time |
| `DOMException` | Polyfill | Constructor with name/message/code |

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
| Child combinator | `div > span` | Full |
| Descendant combinator | `div span` | Full |
| `:first-child` / `:last-child` | Full |
| `:nth-child(n)` / `:nth-of-type(n)` | Full |
| `:root` / `:empty` / `:checked` / `:not()` | Full |
| Multiple selectors | `h1, h2, h3` | Full |
| `+` (adjacent sibling) | `h1 + p` | Not yet |
| `~` (general sibling) | `h1 ~ p` | Not yet |
| `:has()` / `:is()` / `:where()` | Not yet |

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

## Known Limitations

1. **Range API is polyfill-only**: ~4,000 WPT tests use Range. Migration to Rust is highest priority.
2. **NodeIterator/TreeWalker**: Rust handlers + JS implementation. Filter callbacks work for basic cases but `common.js` test nodes (ProcessingInstruction, foreignDoc) need more support.
3. **Event subclasses**: MouseEvent, KeyboardEvent etc. are empty constructors — missing default property initialization.
4. **No `Attr` node type**: `getAttributeNode()` / `setAttributeNode()` return JS objects, not spec Attr.
5. **No `NamedNodeMap`**: `element.attributes` returns array-like object, not live NamedNodeMap.
6. **Namespace methods**: setAttributeNS/getAttributeNS work but ignore namespace.
7. **CSS selectors**: Missing `+`, `~`, `:has()`, `:is()`, `:where()`.
