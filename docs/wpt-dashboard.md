# WPT Dashboard — AetherAgent

> Komplett Web Platform Tests resultat per svit och subkategori.
> Baseline-datum: 2026-03-25 | Senast uppdaterad: 2026-03-27 (codegen v3)
>
> **Referens:** Se [wpt-testing-strategy.md](wpt-testing-strategy.md) för strategi
> och [wpt-workflow-guide.md](wpt-workflow-guide.md) för arbetsflöde.

---

## Sammanfattning (2026-03-26)

| Tier | Sviter | Cases | Passed | Rate |
|------|--------|-------|--------|------|
| **Tier 1** (Core DOM) | dom/nodes, events, ranges, traversal, collections, lists, selection | ~50,000+ | ~22,500+ | ~45% |
| **Tier 2** (Events & Interaction) | pointerevents, uievents, touch-events, input-events, focus, editing | ~700+ | ~160+ | ~23% |
| **Tier 3** (CSS) | css/selectors, css-values, css-cascade, cssom, css-display, css-color, css-flexbox | ~6,100+ | ~2,140+ | ~35% |
| **Tier 4** (HTML) | html/semantics, html/syntax, html/infrastructure, html/webappapis, domparsing | ~5,800+ | ~1,650+ | ~28% |
| **Tier 5** (JS & Standards) | ecmascript, webidl, quirks | ~190 | ~96 | ~51% |
| **Tier 6** (Övriga) | FileAPI, trusted-types, svg, xhr, fetch, encoding, webmessaging, inert, domxpath, shadow-dom, custom-elements, editing, user-timing, mathml, webstorage, hr-time, url, console, streams, compression | ~4,100+ | ~200+ | ~5% |
| **Totalt** | **~45 sviter** | **~67,000+** | **~26,000+** | — |

---

## Tier 1 — Kärna (kör varje PR)

### dom/nodes/ — Core DOM

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 286 | 6,624 | 5,004 | 75.5% | blitz-bekräftad baseline |
| 2026-03-26 | 286 | 6,659 | 5,089 | 76.4% | +hasChildNodes, HierarchyRequestError, unskip insertBefore |
| 2026-03-26 | 286 | 6,676 | 5,243 | 78.5% | +native createDocumentType/PI, nodeName per spec |
| 2026-03-26 | 286 | 6,674 | 5,332 | 79.9% | +ownerDocument configurable, DOMString, TypeError |
| 2026-03-26 | 286 | 6,676 | 5,348 | 80.1% | +DOM prototype chain (Node→CharacterData→Comment) |
| 2026-03-26 | 286 | 6,676 | 5,666 | 84.9% | +live HTMLCollection, createElementNS, createAttribute |
| **2026-03-26** | **286** | **6,676** | **5,659** | **84.8%** | codegen v3 (setup fix korrigerar falska positiva) |

**Toppresterare:**
- CharacterData: ~100%
- ChildNode (before/after/replaceWith): 100%
- Node-cloneNode: 93%
- DOMImplementation-createDocumentType: 97.6%
- querySelector-escapes: 91%

**Största kvarvarande failures:**
- Node-textContent: 37/81 pass (44 fail — foreign doc issues)
- Node-removeChild: 4/28 pass (24 fail — leaf node TypeError)
- "no test suite completion": ~81 tester (async patterns)

**Mål Q2 2026:** 90%

---

### dom/events/ — Event System

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 160 | 310 | 208 | 67.1% | blitz-bekräftad baseline |
| 2026-03-26 | 160 | 318 | 213 | 67.0% | +click() dispatchar MouseEvent |
| 2026-03-26 | 160 | 318 | 213 | 67.0% | +event listener {once: true} stöd |
| **2026-03-26** | **161** | **318** | **221** | **69.5%** | codegen v3 (+on_event global) |

**Implementerat:**
- addEventListener med options (capture, passive, once) ✅
- dispatchEvent med bubbling ✅
- stopPropagation/stopImmediatePropagation ✅
- Event/CustomEvent constructors ✅
- click() dispatchar riktig MouseEvent ✅
- addEventListener({once: true}) ✅

**Saknas:**
- 105 tester: "no test suite completion" (async_test med iframe/DOMContentLoaded)
- window.event stöd
- Event.composedPath()

**Mål Q2 2026:** 90%

---

### dom/ranges/ — Range API

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 55 | ~10,800 | ~7,400+ | ~69% | Baseline |
| 2026-03-26 | 55 | 11,431 | 7,842 | 68.6% | Stabil (hasChildNodes hjälper common.js) |
| **2026-03-26** | **55** | **~11,400** | **~7,752** | **~68.0%** | Verifierad efter refaktorering |

**Native:** Range i `dom_bridge/mod.rs`, `__nativeCompareBoundary` + `__nativeChildIndex` i Rust.

**Mål Q2 2026:** 80%

---

### dom/traversal/ — TreeWalker & NodeIterator

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 18 | 1,584 | 619 | 39.1% | Baseline |
| 2026-03-26 | 18 | 1,584 | 1,412 | 89.1% | +hasChildNodes (blockerare), NodeIterator readonly/filter fix |
| **2026-03-26** | **18** | **1,584** | **1,449** | **91.5%** | +native createDocumentType, TreeWalker filter fix |

**Nyckelfixar (2026-03-26):**
- hasChildNodes() — saknad metod som blockerade ALL traversal via common.js
- NodeIterator: filter boolean→number konvertering, readonly properties
- Native createDocumentType — doctype-noder har nu __nodeKey__
- ProcessingInstruction nodeType=7 i arena

**Kvarvarande failures (135):**
- xmlDoctype foreignDoctype: ~48 (polyfill doctype utan __nodeKey__ för foreign docs)
- Recursive filter InvalidStateError: 2
- ProcessingInstruction edge cases

**Mål Q2 2026:** ~~60%~~ **91.5% — UPPNÅTT!** Nytt mål: 95%

---

### dom/collections/ — HTMLCollection & NodeList

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 9 | 48 | 6 | 12.5% | Baseline |
| 2026-03-26 | 9 | 48 | 27 | 56.2% | +live Proxy HTMLCollection, NamedNodeMap Proxy |
| **2026-03-26** | **9** | **48** | **28** | **58.3%** | codegen v3 (+1) |

**Implementerat:**
- Live HTMLCollection (Proxy-baserad, getElementsByTagName/ClassName) ✅
- NamedNodeMap Proxy (element.attributes) ✅
- item(), namedItem(), Symbol.iterator ✅

**Saknas:**
- Named property access edge cases
- NodeList iteration edge cases

**Mål Q2 2026:** ~~50%~~ **56.2% — UPPNATT!** Nytt mål: 70%

---

## Tier 2 — Stödjande (kör vid relevanta ändringar)

### dom/lists/ — DOMTokenList

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-26 | 5 | 189 | 180 | 95.2% | Stabil |
| **2026-03-26** | **5** | **189** | **181** | **95.8%** | +1 pass efter refaktorering |

---

### domparsing/ — DOMParser & Serialization

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 53 | 453 | 25 | 5.5% | Baseline |
| 2026-03-26 | 53 | 453 | 83 | 18.3% | +native createDocumentType, nodeName fix |
| 2026-03-26 | 53 | 453 | 85 | 18.8% | +outerHTML setter, createAttribute fix |
| **2026-03-26** | **53** | **457** | **85** | **18.6%** | codegen v3 WebIDL wrappers |

**Mål Q2 2026:** 30%

---

### css/selectors/ — CSS Selector Matching

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 636 | 761 | 91 | 12.0% | Baseline |
| **2026-03-26** | **636** | **3,457** | **1,840** | **53.2%** | +hasChildNodes fixade common.js → massiv förbättring |

**Mål Q2 2026:** ~~40%~~ **53.2% — UPPNÅTT!** Nytt mål: 65%

---

### css/cssom/ — CSS Object Model

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **210** | **531** | **76** | **14.3%** | Baseline (oförändrad) |

---

### html/syntax/ — HTML Parsing

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 536 | 340 | 68 | 20.0% | Baseline |
| 2026-03-26 | 536 | 298 | 85 | 28.5% | +17 pass |
| **2026-03-26** | **536** | **450** | **214** | **47.6%** | codegen v3: void elements, text escaping, doctype serialization |

---

## Historik — Övergripande

| Datum | dom/ total | Alla sviter | Kommentar |
|-------|-----------|-------------|-----------|
| 2026-03-24 | 1,382/2,004 (69.0%) | — | Första baseline (5s timeout) |
| 2026-03-25 | 13,383/19,938 (67.1%) | ~13,600/23,649 (57.4%) | 30s timeout, 10x fler tester |
| 2026-03-26 | 15,100+/20,800+ (~73%) | ~17,300+/26,700+ | Runda 1-4: +2979 nya pass |
| 2026-03-26 | ~14,759/~19,569 (~75.4%) | ~17,000+/~25,300+ | Runda 5: +live HTMLCollection, createElementNS, NamedNodeMap |
| **2026-03-26** | — | **26,000+/67,000+** | Utökning från 16→45 sviter, +nya baselines |

### Förbättringslogg 2026-03-26 (Runda 1-5)

| Runda | Nyckelfixar | dom/nodes | dom/traversal | css/selectors |
|-------|-------------|-----------|---------------|---------------|
| 1 | hasChildNodes, NodeIterator filter/readonly, HierarchyRequestError | +80 | +793 | +1749 |
| 2 | Native createDocumentType/PI, document props, nodeName | +154 | +37 | — |
| 3 | ownerDocument configurable, DOMString, TypeError | +89 | — | — |
| 4 | DOM prototype chain (Node→CharacterData→Comment) | +16 | — | — |
| 5 | live HTMLCollection, createElementNS, NamedNodeMap, outerHTML setter, event once | +318 | — | — |
| **Total** | | **+657** | **+830** | **+1749** |

#### Runda 5 detaljer
- dom/nodes: 5,348 → 5,666 (+318)
- dom/collections: 6 → 27 (+21)
- domparsing: 83 → 85 (+2)
- dom/lists: 180 → 181 (+1)

---

## Köra tester

```bash
# Setup
./wpt/setup.sh

# Tier 1 (obligatoriskt per PR)
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/events/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/ranges/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/traversal/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/collections/

# Tier 2 (vid relevanta ändringar)
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/lists/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/domparsing/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/html/syntax/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/css/selectors/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/css/cssom/
```

---

## Implementation Status

Se detaljerad API-täckning:
- [dom-implementation-status.md](dom-implementation-status.md) — Native vs Polyfill per API
- [dom-api-coverage.md](dom-api-coverage.md) — Full referens, 70+ metoder
- [wpt-testing-strategy.md](wpt-testing-strategy.md) — Strategi och prioritering
- [wpt-workflow-guide.md](wpt-workflow-guide.md) — Arbetsflöde steg-för-steg

---

## Sammanfattning (2026-03-26)

| Tier | Sviter | Cases | Passed | Rate |
|------|--------|-------|--------|------|
| **Tier 1** (Core DOM) | dom/nodes, events, ranges, traversal, collections, lists, selection | ~50,000+ | ~22,500+ | ~45% |
| **Tier 2** (Events & Interaction) | pointerevents, uievents, touch-events, input-events, focus, editing | ~700+ | ~160+ | ~23% |
| **Tier 3** (CSS) | css/selectors, css-values, css-cascade, cssom, css-display, css-color, css-flexbox | ~6,100+ | ~2,140+ | ~35% |
| **Tier 4** (HTML) | html/semantics, html/syntax, html/infrastructure, html/webappapis, domparsing | ~5,800+ | ~1,650+ | ~28% |
| **Tier 5** (JS & Standards) | ecmascript, webidl, quirks | ~190 | ~96 | ~51% |
| **Tier 6** (Övriga) | FileAPI, trusted-types, svg, xhr, fetch, encoding, webmessaging, inert, domxpath, shadow-dom, custom-elements, editing, user-timing, mathml, webstorage, hr-time, url, console, streams, compression | ~4,100+ | ~200+ | ~5% |
| **Totalt** | **~45 sviter** | **~67,000+** | **~26,000+** | — |

---

## Tier 1 — Kärna (kör varje PR)

### dom/nodes/ — Core DOM

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 286 | 6,624 | 4,946 | 74.7% | Baseline |
| 2026-03-25 | 286 | 6,624 | 5,017 | 75.7% | +71 pass: Event fix, classList, Text/Comment constructors |
| 2026-03-25 | 286 | 6,624 | 5,004 | 75.5% | blitz-bekräftad (css_compiler + LightningCSS) |
| **2026-03-26** | **286** | **6,676** | **5,666** | **84.9%** | Runda 1-5: +662 pass |

**Toppresterare:**
- CharacterData: ~100%
- ChildNode (before/after/replaceWith): 100%
- Node-cloneNode: 93%
- DOMImplementation-createDocumentType: 97.6%
- querySelector-escapes: 91%

**Största failures:**
- Node-textContent: foreign doc issues
- Node-removeChild: leaf node TypeError
- "no test suite completion": ~81 tester (async patterns)

**Mål Q2 2026:** 90%

---

### dom/events/ — Event System

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 160 | 312 | 100 | 32.1% | Baseline |
| 2026-03-25 | 160 | 311 | 109 | 35.0% | +9 pass: Event constants, cancelBubble, initEvent |
| 2026-03-25 | 160 | 311 | 140 | 45.0% | +31 pass: Event subclasses, cancelBubble spec fix |
| 2026-03-25 | 160 | 310 | 208 | 67.1% | blitz-bekräftad (eventPhase, global addEventListener) |
| **2026-03-26** | **160** | **318** | **213** | **67.0%** | +click(), addEventListener({once: true}) |

**Implementerat:**
- addEventListener med options (capture, passive, once) ✅
- dispatchEvent med bubbling ✅
- stopPropagation/stopImmediatePropagation ✅
- Event/CustomEvent constructors ✅
- click() dispatchar riktig MouseEvent ✅
- addEventListener({once: true}) ✅

**Saknas:**
- 105 tester: "no test suite completion" (async_test med iframe/DOMContentLoaded)
- window.event stöd
- Event.composedPath()

**Mål Q2 2026:** 90%

---

### dom/ranges/ — Range API

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | ~55 | ~11,000+ | ~7,700+ | ~70% | Baseline (polyfill, inkl. slow tester) |
| 2026-03-25 | 55 | 645 | 315 | 48.8% | Native Range, 6 slow skippade |
| 2026-03-25 | 55 | 11,373 | 6,958 | 61.2% | Rust compare_boundary_points, 4 filer re-enabled |
| 2026-03-25 | 55 | 10,762 | 7,182 | 66.7% | WrongDocumentError, nativeChildIndex, toString |
| 2026-03-25 | 55 | ~10,800 | ~7,400+ | ~69% | ownerDocument lazy getter, getSelection, Range mutations scaffolding |
| **2026-03-26** | **55** | **~11,400** | **~7,752** | **~68.0%** | Verifierad efter dom_bridge refaktorering |

**Native:** Range i `src/dom_bridge/mod.rs`, `__nativeCompareBoundary` + `__nativeChildIndex` i Rust.

**Skippade (1 kvar):** Range-intersectsNode.html (>60s)

**Kvarvarande failures (roadmap till 80%):**
- compareBoundaryPoints: ~570 fail (detached/foreignDoc ranges — multi-doc stöd)
- Range-set: ~900 fail (ownerDocument.createRange undefined på documentElement)
- Range-mutations: ~170 fail (Range boundary update vid DOM-mutationer)
- OpaqueRange tentative: ~100 fail (experimentell spec, ej prioriterad)

**Kända begränsningar:**
- ~~`document.documentElement.ownerDocument` = undefined~~ ✅ Fixad med lazy Accessor getter
- Range-set/collapse varierar ±100 pass pga 30s timeout-gräns

**Mål Q2 2026:** 80%

---

### dom/traversal/ — TreeWalker & NodeIterator

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 18 | 1,584 | 516 | 32.6% | Baseline |
| 2026-03-25 | 18 | 1,584 | 619 | 39.1% | blitz-bekräftad (whatToShow unsigned, root identity) |
| **2026-03-26** | **18** | **1,584** | **1,449** | **91.5%** | +hasChildNodes, NodeIterator fix, createDocumentType, TreeWalker filter |

**Implementerat:**
- TreeWalker: nextNode, previousNode, parentNode, firstChild ✅
- NodeIterator: nextNode, previousNode, filter boolean→number ✅
- whatToShow filter ✅
- hasChildNodes() — fixade common.js blockering ✅
- Native createDocumentType — doctype-noder med __nodeKey__ ✅
- ProcessingInstruction nodeType=7 i arena ✅

**Saknas:**
- xmlDoctype foreignDoctype: ~48 (polyfill doctype utan __nodeKey__ för foreign docs)
- Recursive filter InvalidStateError: 2
- ProcessingInstruction edge cases

**Mål Q2 2026:** ~~60%~~ **91.5% — UPPNATT!** Nytt mål: 95%

---

### dom/collections/ — HTMLCollection & NodeList

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 9 | 48 | 6 | 12.5% | Baseline |
| 2026-03-26 | 9 | 48 | 27 | 56.2% | +live Proxy HTMLCollection, NamedNodeMap Proxy |
| **2026-03-26** | **9** | **48** | **28** | **58.3%** | codegen v3 (+1) |

**Implementerat:**
- Live HTMLCollection (Proxy-baserad, getElementsByTagName/ClassName) ✅
- NamedNodeMap Proxy (element.attributes) ✅
- item(), namedItem(), Symbol.iterator ✅

**Saknas:**
- Named property access edge cases
- NodeList iteration edge cases

**Mål Q2 2026:** ~~50%~~ **56.2% — UPPNATT!** Nytt mål: 70%

---

### dom/lists/ — DOMTokenList

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 5 | 189 | 175 | 92.6% | Baseline |
| 2026-03-25 | 5 | 189 | 179 | 94.7% | +4 pass: classList raw value, unique tokens |
| **2026-03-26** | **5** | **189** | **181** | **95.8%** | +1 pass efter refaktorering |

Nästan komplett tack vare native classList-implementation.

---

### dom/abort/ — AbortController

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **5** | **2** | **0** | **0.0%** | Baseline |

AbortController/AbortSignal saknas helt. Låg prioritet.

---

### selection/ — Selection API

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 29,775 | 7,293 | 24.5% | Baseline |

---

### domparsing/ — DOMParser & Serialization

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 53 | 453 | 25 | 5.5% | Baseline |
| 2026-03-26 | 53 | 453 | 83 | 18.3% | +native createDocumentType, nodeName fix |
| 2026-03-26 | 53 | 453 | 85 | 18.8% | +outerHTML setter, createAttribute fix |
| **2026-03-26** | **53** | **457** | **85** | **18.6%** | codegen v3 WebIDL wrappers |

**Implementerat:**
- DOMParser.parseFromString (basic) ✅
- innerHTML getter/setter ✅
- outerHTML setter ✅
- createAttribute native ✅

**Saknas:**
- XMLSerializer
- Range.createContextualFragment
- Robust DOMParser med error handling

**Mål Q2 2026:** 30%

---

### html/syntax/ — HTML Parsing

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 536 | 204 | 26 | 12.7% | Baseline |
| 2026-03-25 | 536 | 340 | 68 | 20.0% | +42 pass: ownerDocument fix |
| **2026-03-26** | **536** | **298** | **85** | **28.5%** | +17 pass |

html5ever ger bra grundstöd men WPT kräver specifika parsing edge cases.

**Saknas:**
- template element parsing
- Foreign content (SVG/MathML)
- Encoding detection

**Mål Q2 2026:** 25%

---

### css/selectors/ — CSS Selector Matching ⭐ NY

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 636 | 761 | 91 | 12.0% | Baseline |
| **2026-03-26** | **636** | **3,457** | **1,840** | **53.2%** | +hasChildNodes fixade common.js → massiv förbättring |

**Implementerat (native Rust):**
- ID, class, tag selectors ✅
- Attribute selectors ([attr], [attr="val"], [attr~="val"], [attr^="val"], [attr$="val"], [attr*="val"]) ✅
- Child (>) / descendant ( ) combinators ✅
- Adjacent sibling (+) / General sibling (~) ✅
- :first-child, :last-child, :nth-child, :nth-of-type ✅
- :nth-last-child, :nth-last-of-type ✅
- :only-of-type ✅
- :root, :empty, :checked, :not() ✅
- :has(), :is(), :where() ✅

**Mål Q2 2026:** ~~40%~~ **53.2% — UPPNATT!** Nytt mål: 65%

---

### css/cssom/ — CSS Object Model ⭐ NY

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 210 | 531 | 43 | 8.1% | Baseline |
| **2026-03-25** | **210** | **531** | **76** | **14.3%** | +33 pass: CSS cascade engine connected |

**Implementerat:**
- style.setProperty/getPropertyValue/removeProperty ✅
- getComputedStyle (basic) ✅

**Saknas:**
- CSSStyleSheet API
- CSSStyleDeclaration fullständig
- window.getComputedStyle med cascading

**Mål Q2 2026:** 20%

---

### css/css-values/ — CSS Values

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 1,523 | 164 | 10.8% | Baseline |

---

### css/css-cascade/ — CSS Cascade

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 402 | 23 | 5.7% | Baseline |

---

### css/css-display/ — CSS Display

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 44 | 16 | 36.4% | Baseline |

---

### css/css-color/ — CSS Color

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 84 | 12 | 14.3% | Baseline |

---

### css/css-flexbox/ — CSS Flexbox

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 124 | 11 | 8.9% | Baseline |

---

### html/dom/ — HTML-specifika DOM-APIer

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **463** | **~2,000** | **—** | **—** | Stack overflow vid batch-körning |

Kräver fix i runner för stora sviter. Kör subkataloger separat.

---

## Tier 3 — Utökad (kör vid milstolpar)

### encoding/ — TextEncoder/TextDecoder

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **160** | **331** | **1** | **0.3%** | Baseline |

TextEncoder/TextDecoder saknas. Enkel implementation ger stor vinst.

**Mål Q2 2026:** 40%

---

### webstorage/ — localStorage/sessionStorage

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **44** | **7** | **0** | **0.0%** | Baseline |

In-memory storage finns men WPT kräver specifika beteenden (events, quota).

**Mål Q2 2026:** 60%

---

### xhr/ — XMLHttpRequest ⭐ NY

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **244** | **430** | **28** | **6.5%** | Baseline |

AetherAgent har XHR interception i `intercept.rs` men WPT kräver full XMLHttpRequest API.

**Mål Q2 2026:** 20%

---

### hr-time/ — High Resolution Timing

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **15** | **5** | **0** | **0.0%** | Baseline |

performance.now() finns men WPT kräver specifika precision-krav.

---

### console/ — Console API

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **6** | **0** | **0** | **—** | Inga testcases detekterade |

Console-testerna kräver troligen specifik testharness-integration.

---

### url/ — URL API

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **4** | **1** | **0** | **0.0%** | Baseline |

---

## Tier 2 — Events & Interaction (NY)

### pointerevents/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 322 | 133 | 41.3% | Baseline |

### uievents/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 24 | 9 | 37.5% | Baseline |

### touch-events/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 16 | 6 | 37.5% | Baseline |

### input-events/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 29 | 9 | 31.0% | Baseline |

### focus/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 1 | 0 | 0.0% | Baseline |

### editing/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 229 | 3 | 1.3% | Baseline |

---

## Tier 4 — HTML

### html/semantics/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 2,803 | TBD | TBD | TBD | Stack overflow vid batch-körning |
| 2026-03-26 | — | ~4,900 | 697 | 14.3% | Baseline |
| 2026-03-26 | 3,007 | 4,856 | 1,068 | 22.0% | +codegen v1/v1.5 (+311 properties) |
| **2026-03-26** | **3,007** | **4,897** | **1,484** | **30.3%** | +codegen v3: ValidityState, input dirty state, form association, select (+416) |

---

## Tier 5 — JS & Standards

### ecmascript/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 21 | 18 | 85.7% | Baseline |

### webidl/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 94 | 20 | 21.3% | Baseline |

### quirks/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 75 | 58 | 77.3% | Baseline |

---

## Tier 6 — Övriga

### FileAPI/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 23 | 10 | 43.5% | Baseline |

### trusted-types/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 238 | 25 | 10.5% | Baseline |

### svg/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 869 | 53 | 6.1% | Baseline |

### xhr/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 244 | 430 | 28 | 6.5% | Baseline |
| **2026-03-26** | — | 463 | 31 | 6.7% | Uppdaterad baseline |

### fetch/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 400 | 10 | 2.5% | Baseline |

### encoding/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 160 | 331 | 1 | 0.3% | Baseline |
| **2026-03-26** | — | 331 | 2 | 0.6% | +1 pass |

### webmessaging/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 70 | 10 | 14.3% | Baseline |

### inert/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 49 | 6 | 12.2% | Baseline |

### domxpath/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 105 | 2 | 1.9% | Baseline |

### shadow-dom/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 295 | 1,393 | 24 | 1.7% | Baseline |
| **2026-03-26** | — | ~100+ | ~15 | ~5% | Uppdaterad baseline |

### custom-elements/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 172 | 930 | 18 | 1.9% | Baseline |
| **2026-03-26** | — | 950 | 19 | 2.0% | +1 pass |

### user-timing/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 7 | 3 | 42.9% | Baseline |

### mathml/

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-26** | — | 143 | 2 | 1.4% | Baseline |

---

## Historik — Övergripande

| Datum | dom/ total | Alla sviter | Kommentar |
|-------|-----------|-------------|-----------|
| 2026-03-24 | 1,382/2,004 (69.0%) | — | Första baseline (5s timeout) |
| 2026-03-25 | 13,383/19,938 (67.1%) | ~13,600/23,649 (57.4%) | 30s timeout, 10x fler tester |
| 2026-03-25 | — | Se ovan per svit | Ny detaljerad baseline med alla sviter |
| 2026-03-26 | 15,100+/20,800+ (~73%) | ~17,300+/26,700+ | Runda 1-4: +2979 nya pass |
| 2026-03-26 | ~14,759/~19,569 (~75.4%) | ~17,000+/~25,300+ | Runda 5: +live HTMLCollection, createElementNS, NamedNodeMap |
| **2026-03-26** | — | **26,000+/67,000+** | Utökning från 16→45 sviter, +nya baselines |

---

## Köra tester

```bash
# Setup
./wpt/setup.sh

# Tier 1 (obligatoriskt per PR)
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/events/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/ranges/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/traversal/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/collections/

# Tier 2 (vid relevanta ändringar)
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/lists/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/domparsing/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/html/syntax/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/css/selectors/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/css/cssom/

# Tier 3 (milstolpar)
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/encoding/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/webstorage/
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/xhr/

# Verbose (debug)
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/ --verbose
```

---

## Implementation Status

Se detaljerad API-täckning:
- [dom-implementation-status.md](dom-implementation-status.md) — Native vs Polyfill per API
- [dom-api-coverage.md](dom-api-coverage.md) — Full referens, 70+ metoder
- [wpt-testing-strategy.md](wpt-testing-strategy.md) — Strategi och prioritering
- [wpt-workflow-guide.md](wpt-workflow-guide.md) — Arbetsflöde steg-för-steg
