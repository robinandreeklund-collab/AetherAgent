# DOM Implementation Status

> Ärlig inventering: vad är Rust-native vs JS-polyfill.

## Princip

- **Native (Rust)** = Implementerad i `dom_bridge/` (modulkatalog), läser/skriver ArenaDom direkt. Riktig implementation.
- **Polyfill (JS)** = Shim i `wpt/polyfills.js`. Fungerar för WPT-validering men är **inte** riktig implementation. Mål: migrera till Rust.
- **Stub** = Returnerar hårdkodat värde. Ingen riktig logik.

## WPT Score

| Suite | Cases | Passed | Rate | Datum |
|-------|-------|--------|------|-------|
| dom/nodes | 6,676 | 5,666 | 84.9% | 2026-03-26 |
| dom/events | 318 | 213 | 67.0% | 2026-03-26 |
| dom/ranges | ~10,943 | ~7,404 | ~67.7% | 2026-03-26 |
| dom/traversal | 1,584 | 1,449 | 91.5% | 2026-03-26 |
| dom/collections | 48 | 27 | 56.2% | 2026-03-26 |
| dom/lists | 189 | 181 | 95.8% | 2026-03-26 |
| dom/abort | 2 | 0 | 0.0% | 2026-03-25 |
| css/selectors | 3,457 | 1,840 | 53.2% | 2026-03-26 |
| css/cssom | 531 | 76 | 14.3% | 2026-03-26 |
| domparsing | 453 | 85 | 18.8% | 2026-03-26 |
| html/syntax | 340 | 68 | 20.0% | 2026-03-26 |
| encoding | 331 | 1 | 0.3% | 2026-03-25 |
| xhr | 430 | 28 | 6.5% | 2026-03-25 |
| webstorage | 7 | 0 | 0.0% | 2026-03-25 |

> Alla scores bekräftade med `--features js-eval,blitz,fetch` (LightningCSS + css-inline).
> Range API native i Rust. Polyfill borttagen.
Se [wpt-dashboard.md](wpt-dashboard.md) för fullständig detaljerad breakdown.

---

## Document Methods

| Method | Status | Impl |
|--------|--------|------|
| `getElementById(id)` | **Native** | Rust — rekursiv sökning i ArenaDom |
| `querySelector(sel)` | **Native** | Rust — full CSS selector matching |
| `querySelectorAll(sel)` | **Native** | Rust — returnerar alla matchande noder |
| `createElement(tag)` | **Native** | Rust — skapar ny nod i ArenaDom |
| `createTextNode(text)` | **Native** | Rust — skapar Text-nod |
| `createComment(text)` | **Native** | Rust — skapar Comment-nod |
| `createDocumentFragment()` | **Native** | Rust — skapar fragment |
| `getElementsByClassName(cls)` | **Native** | Rust — rekursiv sökning, live Proxy HTMLCollection (2026-03-26) |
| `getElementsByTagName(tag)` | **Native** | Rust — rekursiv sökning med "*" wildcard, live Proxy HTMLCollection (2026-03-26) |
| `createElementNS(ns, qname)` | **Native** | Rust — full namespace validation (xml/xmlns constraints, qualified name parsing) (2026-03-26) |
| `getElementsByTagNameNS(ns, tag)` | Polyfill | JS — ignorerar namespace |
| `createEvent(type)` | Polyfill | JS — skapar Event-objekt med rätt typ |
| `createAttribute(name)` | **Native** | Rust — name validation, DOMString conversion (2026-03-26) |
| `document.implementation` | Polyfill | JS — createDocument/createHTMLDocument/createDocumentType |
| `document.title` | Polyfill | JS — getter/setter via `<title>` element |
| `document.URL` | Polyfill | JS — alias för location.href |
| `document.doctype` | **Native** | Rust — exponerar Doctype-nod från ArenaDom (2026-03-25) |
| `document.childNodes` | **Native** | Rust — returnerar alla document-barn inkl. doctype (2026-03-25) |
| `document.firstChild/lastChild` | **Native** | Rust — (2026-03-25) |
| `document.compareDocumentPosition()` | **Native** | Rust — (2026-03-25) |
| `document.contains()` | **Native** | Rust — (2026-03-25) |
| `document.lookupNamespaceURI()` | **Native** | Rust — (2026-03-25) |
| `document.isSameNode/isEqualNode` | **Native** | Rust — (2026-03-25) |

## Element Methods — Native (Rust)

| Method | Detaljer |
|--------|---------|
| `getAttribute(name)` | Läser attribut, lowercasar namn |
| `setAttribute(name, value)` | Skriver attribut till ArenaDom |
| `removeAttribute(name)` | Tar bort attribut |
| `hasAttribute(name)` | Kontrollerar existens |
| `getAttributeNames()` | Returnerar alla attributnamn |
| `textContent` (get/set) | Accessor — rekursiv text extraction/replacement |
| `innerHTML` (get/set) | Accessor — serialiserar/parsar HTML |
| `outerHTML` (get/set) | Serialiserar element + barn; setter parser-baserad med DOMException för root (2026-03-26) |
| `insertAdjacentHTML(pos, html)` | Parsar och infogar HTML |
| `appendChild(child)` | Flyttar nod i ArenaDom |
| `removeChild(child)` | Tar bort, kastar NotFoundError |
| `insertBefore(new, ref)` | Index-baserad insertion |
| `replaceChild(new, old)` | Ersätter nod, kastar TypeError/NotFoundError |
| `cloneNode(deep)` | Rekursiv djup kopia i ArenaDom |
| `parentNode` / `childNodes` / `children` | Traversering via ArenaDom |
| `firstChild` / `lastChild` / `nextSibling` / `previousSibling` | Navigering |
| `firstElementChild` / `lastElementChild` / `nextElementSibling` | Element-filtrering |
| `closest(selector)` | Traverserar ancestors med CSS matching |
| `matches(selector)` | Testar CSS selector-matchning |
| `contains(other)` | Rekursiv descendant-check |
| `isConnected` | Parent-chain till document |
| `getRootNode()` | Traverserar till rot |
| `ownerDocument` | Sätts på alla parsade DOM-noder, pekar på document (2026-03-25) |
| `nodeValue` | null för Element/Document/Doctype, data för Text/Comment (2026-03-25) |
| `classList.add/remove/toggle/contains/replace` | Modifierar class-attribut |
| `classList.item/forEach/entries/keys/values` | DOMTokenList-iteration (2026-03-25) |
| `classList.length/value` | Live getters via defineProperty (2026-03-25) |
| `classList` (property) | Read-only getter, assignment = no-op per spec (2026-03-25) |
| `compareDocumentPosition(other)` | Returnerar DOCUMENT_POSITION_* bitmask |
| `isSameNode(other)` | Jämför NodeKey |
| `isEqualNode(other)` | Djup jämförelse |
| `lookupNamespaceURI(prefix)` | Parent-chain traversal |
| `toggleAttribute(name, force)` | DOMException vid ogiltig name (2026-03-25) |
| `style.setProperty/getPropertyValue/removeProperty` | Inline style manipulation |
| `addEventListener(type, fn, options)` | Stöd för options-objekt {capture, passive, once} (2026-03-26) |
| `removeEventListener(type, fn)` | Event-system |
| `dispatchEvent(event)` | Med passive-stöd och !defaultPrevented returnvärde (2026-03-25) |
| `focus()` / `blur()` / `click()` | Focus-tracking + event dispatch |
| `getBoundingClientRect()` | Estimerad rect från tag+style |

## Element Methods — Polyfill (JS, behöver Rust-migration)

| Method | Prioritet | Kommentar |
|--------|-----------|----------|
| ~~`remove()`~~ | ~~Hög~~ | **Migrerad till Rust** (2026-03-24) |
| ~~`before()` / `after()`~~ | ~~Hög~~ | **Migrerad till Rust** (2026-03-24) |
| ~~`replaceWith()`~~ | ~~Hög~~ | **Migrerad till Rust** (2026-03-24) |
| `prepend()` / `append()` / `replaceChildren()` | Hög | ParentNode convenience |
| ~~`toggleAttribute(name, force)`~~ | ~~Medium~~ | **Migrerad till Rust** (2026-03-24) |
| `insertAdjacentElement(pos, el)` | Medium | Wrapper kring insertBefore/appendChild |
| `insertAdjacentText(pos, text)` | Medium | Wrapper kring createTextNode + insert |
| `setAttributeNS(ns, qname, val)` | Medium | Ignorerar namespace nu — behöver riktig NS-stöd |
| `getAttributeNS(ns, local)` | Medium | Samma |
| `hasAttributeNS(ns, local)` | Medium | Samma |
| `removeAttributeNS(ns, local)` | Medium | Samma |
| `getAttributeNode(name)` | Låg | Returnerar Attr-liknande objekt |
| `moveBefore(node, child)` | Låg | Ny spec — delegerar till insertBefore |

## Range API — Native (migrerad 2026-03-25)

| Method | Status | Kommentar |
|--------|--------|----------|
| `document.createRange()` | **Native** | Skapar Range via globalThis.Range |
| `setStart/setEnd(node, offset)` | **Native** | IndexSizeError-validering |
| `setStartBefore/After(node)` | **Native** | Via __nativeChildIndex (Rust) |
| `setEndBefore/After(node)` | **Native** | Via __nativeChildIndex (Rust) |
| `collapse(toStart)` | **Native** | 96.8% WPT pass |
| `selectNode/selectNodeContents` | **Native** | 100% WPT pass |
| `compareBoundaryPoints(how, range)` | **Native** | WrongDocumentError, NotSupportedError |
| `comparePoint(node, offset)` | **Native** | 88.6% WPT pass |
| `isPointInRange(node, offset)` | **Native** | Wrapper kring comparePoint |
| `intersectsNode(node)` | **Native** | __nativeChildIndex optimization |
| `cloneRange()` | **Native** | 90.3% WPT pass |
| `toString()` | **Native** | Multi-nod stöd (DOM tree walk) |
| `_compareBoundary()` | **Native** | __nativeCompareBoundary → Rust ArenaDom |
| `deleteContents/extractContents` | Stub | No-op |
| `getBoundingClientRect()` | Stub | Returnerar nollor |
| `detach()` | **Native** | No-op per spec |

**Migrerad från polyfill → dom_bridge/mod.rs (2026-03-25).** Boundary-jämförelse i ren Rust.
WPT dom/ranges: ~69%. Kvarvarande: foreignDoc/detached ranges, mutation tracking.

## NodeType — Native (Rust)

| NodeType | Värde | Status |
|----------|-------|--------|
| Element | 1 | **Native** |
| Text | 3 | **Native** |
| Comment | 8 | **Native** |
| Document | 9 | **Native** |
| Doctype | 10 | **Native** (2026-03-25) |
| DocumentFragment | 11 | **Native** |

## CharacterData — Native (Rust, migrerad Fas 17)

| Method | Status |
|--------|--------|
| `.data` (get/set) | **Native** — UTF-16 code unit aware |
| `.length` | **Native** |
| `.substringData(offset, count)` | **Native** |
| `.appendData(data)` | **Native** |
| `.insertData(offset, data)` | **Native** |
| `.deleteData(offset, count)` | **Native** |
| `.replaceData(offset, count, data)` | **Native** |

## Event-system

| Del | Status |
|-----|--------|
| `addEventListener(type, fn, options)` | **Native** — stöd för {capture, passive, once} (2026-03-26) |
| `removeEventListener(type, fn)` | **Native** |
| `dispatchEvent` med bubbling + passive | **Native** (2026-03-25) |
| `Event` / `CustomEvent` konstruktorer | **Native** (Rust) |
| `MutationObserver` constructor | **Native** — new-stöd via JS klass-wrapper (2026-03-25) |
| Passive-by-default (touchstart, wheel) | **Native** (2026-03-25) |
| `UIEvent`, `MouseEvent`, `KeyboardEvent`, `FocusEvent`, `InputEvent` | Polyfill (spec-properties: coordinates, modifier keys, getModifierState) |
| `WheelEvent`, `PointerEvent` | Polyfill (deltaX/Y/Z, pointerId, pressure) |
| `document.createEvent(type)` | Polyfill |
| `event.initEvent()` | **Native** (2026-03-25) — med state reset |
| `Event.NONE/CAPTURING_PHASE/AT_TARGET/BUBBLING_PHASE` | **Native** (2026-03-25) |
| `cancelBubble` | **Native** (2026-03-25) |
| `stopPropagation` / `stopImmediatePropagation` | **Native** (2026-03-25) — sätter flaggor |

## DOMException — Native (migrerad 2026-03-25)

| Feature | Status |
|---------|--------|
| `DOMException(message, name)` constructor | **Native** — register_dom_exception() i dom_bridge.rs |
| Alla error-koder (INDEX_SIZE_ERR etc.) | **Native** — 25 koder på constructor + prototype |
| `throw_dom_exception()` Rust-helper | **Native** — skapar DOMException via JS eval |
| `validate_token()` → SyntaxError/InvalidCharacterError | **Native** |
| `Error.prototype` kedja | **Native** — DOMException ärver Error |

## CSS Selector Parser — Native (Rust)

| Feature | Status |
|---------|--------|
| ID (`#id`) | Native |
| Class (`.cls`) | Native |
| Tag (`div`) | Native |
| Attribute (`[attr]`, `[attr="val"]`, `[attr~="val"]`) | Native |
| Child (`>`) / Descendant (` `) | Native |
| `:first-child`, `:last-child`, `:nth-child`, `:nth-of-type` | Native |
| `:root`, `:empty`, `:checked`, `:disabled`, `:enabled`, `:focus` | Native |
| `:not(sel)` | Native |
| Comma-separated | Native |
| CSS escape (`\30 foo`) | Native |
| `:has()`, `:is()`, `:where()` | **Native** (2026-03-25) |
| `+` (adjacent sibling), `~` (general sibling) | **Native** |
| `:nth-last-child`, `:nth-last-of-type` | **Native** (2026-03-25) |
| `:only-of-type` | **Native** |

---

## WPT-sviter per implementation

> Varje implementation mappar till en eller flera WPT-sviter.
> Se [wpt-dashboard.md](wpt-dashboard.md) för aktuella scores.

| Implementation | WPT-svit | Score | Mål |
|---------------|----------|-------|-----|
| DOM Core (createElement, appendChild, etc.) | `dom/nodes/` | 84.9% | 90% |
| Event System (addEventListener, dispatch) | `dom/events/` | 67.0% | 90% |
| Range API (native Rust) | `dom/ranges/` | ~67.7% | 80% |
| TreeWalker/NodeIterator | `dom/traversal/` | 91.5% | 95% |
| HTMLCollection/NodeList (live Proxy) | `dom/collections/` | 56.2% | 70% |
| DOMTokenList (classList) | `dom/lists/` | 95.8% | 98% |
| AbortController | `dom/abort/` | 0.0% | 50% |
| DOMParser/innerHTML/outerHTML | `domparsing/` | 18.8% | 30% |
| HTML5 Parsing | `html/syntax/` | 20.0% | 30% |
| CSS Selectors | `css/selectors/` | 53.2% | 65% |
| CSSOM (style, getComputedStyle) | `css/cssom/` | 14.3% | 25% |
| TextEncoder/TextDecoder | `encoding/` | 0.3% | 40% |
| localStorage/sessionStorage | `webstorage/` | 0.0% | 60% |
| XMLHttpRequest | `xhr/` | 6.5% | 20% |
| performance.now() | `hr-time/` | 0.0% | 50% |
| Custom Elements | `custom-elements/` | 1.9% | 10% |
| Shadow DOM | `shadow-dom/` | 1.7% | 10% |

---

## Migrationsplan: Polyfill → Rust

### Fas 1 — KLAR (2026-03-24)
1. ~~`element.remove()`~~ ✅
2. ~~`before()` / `after()` / `replaceWith()`~~ ✅
3. ~~`toggleAttribute()`~~ ✅
4. ~~CharacterData `.data`, `.substringData()`, etc.~~ ✅

### Fas 2 — KLAR (2026-03-25)
5. ~~`ownerDocument` på alla parsade noder~~ ✅
6. ~~`document.doctype` + NodeType::Doctype~~ ✅
7. ~~`document.childNodes/firstChild/lastChild`~~ ✅
8. ~~`document.compareDocumentPosition/contains/lookupNamespaceURI`~~ ✅
9. ~~`classList` live-update + DOMTokenList-metoder~~ ✅
10. ~~`addEventListener` options-objekt (passive)~~ ✅
11. ~~`nodeValue` per nodeType~~ ✅
12. ~~`MutationObserver` constructor~~ ✅

### Fas 3 — KLAR (2026-03-25)
13. ~~**Range API → Rust**~~ ✅ — Migrerad till dom_bridge.rs med Rust-native boundary comparison
14. ~~**DOMException constructor → Rust**~~ ✅ — register_dom_exception() med alla 25 koder
15. ~~**Event subclasses**~~ ✅ — UIEvent, MouseEvent, KeyboardEvent, FocusEvent, InputEvent, WheelEvent, PointerEvent med spec-properties
16. ~~`prepend()` / `append()` / `replaceChildren()`~~ ✅ — Redan native sedan Fas 17

### Fas 4 — KLAR (2026-03-26)
17. ~~CSS selectors: `+`, `~`~~ ✅ — Redan native
18. ~~CSS selectors: `:has()`, `:is()`, `:where()`, `:nth-last-child/type`~~ ✅ — Native (2026-03-25). `css/selectors/` 12% → 53.2%
19. ~~Namespace-metoder (createElementNS)~~ ✅ — Native med full namespace validation (2026-03-26)
20. ~~`element.attributes` som riktig NamedNodeMap~~ ✅ — Proxy-baserad med Object.getOwnPropertyNames (2026-03-26)
21. ~~TreeWalker/NodeIterator filter-förbättringar~~ ✅ — `dom/traversal/` 33% → 91.5%
22. TextEncoder/TextDecoder — `encoding/` 0% → 40%

### Fas 5 — KLAR (2026-03-26)
23. ~~Live HTMLCollection~~ ✅ — Proxy-baserad, getElementsByTagName/ClassName, item(), namedItem(), Symbol.iterator (2026-03-26)
24. ~~createAttribute (native)~~ ✅ — Name validation, DOMString conversion (2026-03-26)
25. ~~outerHTML setter~~ ✅ — Parser-baserad med DOMException för root element (2026-03-26)
26. ~~addEventListener({once: true})~~ ✅ — {once: true} support (2026-03-26)
27. ~~document.implementation stub~~ ✅ — hasFeature för pre-polyfill availability (2026-03-26)

### Fas 6 — Lägre prioritet
28. AbortController/AbortSignal — `dom/abort/`
29. DOMParser fullständig — `domparsing/` 18.8% → 30%
30. XMLSerializer — `domparsing/`
31. setAttributeNS/getAttributeNS riktig NS-stöd — `dom/nodes/` impact
32. TextEncoder/TextDecoder — `encoding/` 0% → 40%
