# DOM Implementation Status

> Ärlig inventering: vad är Rust-native vs JS-polyfill.

## Princip

- **Native (Rust)** = Implementerad i `dom_bridge.rs`, läser/skriver ArenaDom direkt. Riktig implementation.
- **Polyfill (JS)** = Shim i `wpt/polyfills.js`. Fungerar för WPT-validering men är **inte** riktig implementation. Mål: migrera till Rust.
- **Stub** = Returnerar hårdkodat värde. Ingen riktig logik.

## WPT Score

| Suite | Cases | Passed | Rate | Datum |
|-------|-------|--------|------|-------|
| dom/nodes | 6,624 | 4,946 | 74.7% | 2026-03-25 |
| dom/events | 312 | 100 | 32.1% | 2026-03-25 |
| dom/ranges | ~11,000+ | ~7,700+ | ~70% | 2026-03-25 |
| dom/traversal | 1,584 | 516 | 32.6% | 2026-03-25 |
| dom/collections | 48 | 6 | 12.5% | 2026-03-25 |
| dom/lists | 189 | 175 | 92.6% | 2026-03-25 |
| dom/abort | 2 | 0 | 0.0% | 2026-03-25 |
| css/selectors | 761 | 91 | 12.0% | 2026-03-25 |
| css/cssom | 531 | 43 | 8.1% | 2026-03-25 |
| domparsing | 453 | 25 | 5.5% | 2026-03-25 |
| html/syntax | 204 | 26 | 12.7% | 2026-03-25 |
| encoding | 331 | 1 | 0.3% | 2026-03-25 |
| xhr | 430 | 28 | 6.5% | 2026-03-25 |
| webstorage | 7 | 0 | 0.0% | 2026-03-25 |

**OBS:** En del av pass-raten beror fortfarande på polyfills (främst Range API).
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
| `getElementsByClassName(cls)` | **Native** | Rust — rekursiv sökning |
| `getElementsByTagName(tag)` | **Native** | Rust — rekursiv sökning |
| `createElementNS(ns, qname)` | Polyfill | JS — delegerar till createElement |
| `getElementsByTagNameNS(ns, tag)` | Polyfill | JS — ignorerar namespace |
| `createEvent(type)` | Polyfill | JS — skapar Event-objekt med rätt typ |
| `createAttribute(name)` | Polyfill | JS — skapar Attr-objekt med nodeType=2 |
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
| `outerHTML` (get) | Serialiserar element + barn |
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
| `addEventListener(type, fn, options)` | Stöd för options-objekt {capture, passive} (2026-03-25) |
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

## Range API — Polyfill (JS, hög prioritet för Rust-migration)

| Method | Status | Kommentar |
|--------|--------|----------|
| `document.createRange()` | Polyfill | Skapar AetherRange i JS |
| `setStart/setEnd(node, offset)` | Polyfill | Boundary-sättning med offset-validering |
| `setStartBefore/After(node)` | Polyfill | Via parentNode-index |
| `setEndBefore/After(node)` | Polyfill | Via parentNode-index |
| `collapse(toStart)` | Polyfill | Booleansk collapse |
| `selectNode/selectNodeContents` | Polyfill | Väljer nod/barn |
| `compareBoundaryPoints(how, range)` | Polyfill | NotSupportedError för how>3 (2026-03-25) |
| `comparePoint(node, offset)` | Polyfill | IndexSizeError-validering (2026-03-25) |
| `isPointInRange(node, offset)` | Polyfill | Wrapper kring comparePoint |
| `intersectsNode(node)` | Polyfill | Root-jämförelse |
| `cloneRange()` | Polyfill | Kopierar state |
| `toString()` | Polyfill | Textextraktion |
| `deleteContents/extractContents` | Polyfill | Stubs (no-op) |
| `getBoundingClientRect()` | Polyfill | Returnerar nollor |
| `detach()` | Polyfill | No-op per spec |

**~4,000 WPT-tester berör Range API. Rust-migration är högsta prioritet.**

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
| `addEventListener(type, fn, options)` | **Native** — stöd för {capture, passive} (2026-03-25) |
| `removeEventListener(type, fn)` | **Native** |
| `dispatchEvent` med bubbling + passive | **Native** (2026-03-25) |
| `Event` / `CustomEvent` konstruktorer | **Native** (Rust) |
| `MutationObserver` constructor | **Native** — new-stöd via JS klass-wrapper (2026-03-25) |
| Passive-by-default (touchstart, wheel) | **Native** (2026-03-25) |
| `MouseEvent`, `KeyboardEvent` etc. | Polyfill (tomma konstruktorer) |
| `document.createEvent(type)` | Polyfill |
| `event.initEvent()` | Polyfill |
| `stopPropagation` / `stopImmediatePropagation` | Native |

## DOMException — Polyfill

| Feature | Status |
|---------|--------|
| `DOMException(message, name)` constructor | Polyfill |
| `throw_dom_exception()` Rust-helper | **Native** (2026-03-25) — skapar DOMException via JS eval |
| DOMException.INDEX_SIZE_ERR etc. | Polyfill (statiska koder) |
| `validate_token()` → SyntaxError/InvalidCharacterError | **Native** (2026-03-25) |

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
| `:has()`, `:is()`, `:where()` | Saknas |
| `+` (adjacent sibling), `~` (general sibling) | Saknas |

---

## WPT-sviter per implementation

> Varje implementation mappar till en eller flera WPT-sviter.
> Se [wpt-dashboard.md](wpt-dashboard.md) för aktuella scores.

| Implementation | WPT-svit | Baseline | Mål |
|---------------|----------|----------|-----|
| DOM Core (createElement, appendChild, etc.) | `dom/nodes/` | 74.7% | 90% |
| Event System (addEventListener, dispatch) | `dom/events/` | 32.1% | 55% |
| Range API (polyfill) | `dom/ranges/` | ~70% | 85% |
| TreeWalker/NodeIterator | `dom/traversal/` | 32.6% | 60% |
| HTMLCollection/NodeList | `dom/collections/` | 12.5% | 50% |
| DOMTokenList (classList) | `dom/lists/` | 92.6% | 95% |
| AbortController | `dom/abort/` | 0.0% | 50% |
| DOMParser/innerHTML | `domparsing/` | 5.5% | 30% |
| HTML5 Parsing | `html/syntax/` | 12.7% | 25% |
| CSS Selectors | `css/selectors/` | 12.0% | 40% |
| CSSOM (style, getComputedStyle) | `css/cssom/` | 8.1% | 20% |
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

### Fas 3 — Nästa (Hög prioritet)
13. **Range API → Rust** — ~11,000 WPT-testcases, störst impact. Migrera från `wpt/polyfills.js` till `dom_bridge.rs`
14. **DOMException constructor → Rust** — renare error-hantering, påverkar alla sviter
15. **Event subclasses → Rust** — MouseEvent, KeyboardEvent med properties. `dom/events/` 32% → 55%
16. `prepend()` / `append()` / `replaceChildren()` — ParentNode convenience. `dom/nodes/` +50-100 pass

### Fas 4 — Medium prioritet
17. CSS selectors: `+`, `~` — Adjacent/general sibling combinators. `css/selectors/` 12% → 30%
18. CSS selectors: `:has()`, `:is()`, `:where()` — Modern pseudo-classes. `css/selectors/` → 40%
19. Namespace-metoder (setAttributeNS etc.) — `dom/nodes/` +100 pass
20. `element.attributes` som riktig NamedNodeMap — `dom/collections/` impact
21. TreeWalker/NodeIterator filter-förbättringar — `dom/traversal/` 33% → 60%
22. TextEncoder/TextDecoder — `encoding/` 0% → 40%

### Fas 5 — Lägre prioritet
23. AbortController/AbortSignal — `dom/abort/`
24. Live HTMLCollection — `dom/collections/`
25. DOMParser fullständig — `domparsing/` 6% → 30%
26. XMLSerializer — `domparsing/`
27. document.implementation fullständig — `dom/nodes/`
