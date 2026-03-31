# WPT Dashboard — AetherAgent

> Komplett Web Platform Tests resultat per svit och subkategori.
> Baseline-datum: 2026-03-25 | Senast uppdaterad: 2026-03-31 (Session 5: +142 WPT pass, Live CSSStyleDeclaration + qSA + MutationObserver + Namespace)
>
> **Referens:** Se [wpt-testing-strategy.md](wpt-testing-strategy.md) för strategi
> och [wpt-workflow-guide.md](wpt-workflow-guide.md) för arbetsflöde.

---

## Sammanfattning (2026-03-31)

| Tier | Sviter | Cases | Passed | Rate |
|------|--------|-------|--------|------|
| **Tier 1** (Core DOM) | dom/nodes, events, ranges, traversal, collections, lists | ~29,900 | ~18,200+ | ~61% |
| **Tier 2** (Parsing & Serialization) | domparsing, html/syntax | ~936 | ~401 | ~43% |
| **Tier 3** (CSS) | css/selectors, css-values, css-cascade, cssom, css-display, css-color, css-flexbox | ~6,322 | ~2,092 | ~33% |
| **Tier 4** (HTML) | html/semantics | ~4,922 | ~2,023 | ~41% |
| **Tier 5** (Events & Interaction) | uievents, pointerevents, focus, selection, input-events, touch-events | ~30,180 | ~7,842 | ~26% |
| **Tier 6** (JS & Standards) | ecmascript, webidl, quirks | ~190 | ~99 | ~52% |
| **Tier 7** (Övriga) | FileAPI, trusted-types, svg, xhr, encoding, inert, domxpath, webstorage, requestidlecallback, editing, webmessaging | ~3,831 | ~843 | ~22% |

---

## Alla sviter — Fullständig status (2026-03-31)

### Tier 1 — Core DOM (kör varje PR)

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **dom/nodes** | 6,094 | 6,673 | **91.3%** | ↑↑ från 90.4% |
| **dom/events** | 271 | 322 | **84.2%** | ─ stabil |
| **dom/ranges** | 4,360 | 5,788 | **75.3%** | ─ stabil |
| **dom/traversal** | 1,533 | 1,591 | **96.3%** | ─ stabil |
| **dom/collections** | 30 | 48 | **62.5%** | ─ stabil |
| **dom/lists** | 181 | 189 | **95.8%** | ─ stabil |

### Tier 2 — Parsing & Serialization

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **domparsing** | 161 | 375 | **42.9%** | ↑ från 39.1% |
| **html/syntax** | 241 | 561 | **43.0%** | ─ stabil |

### Tier 3 — CSS

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **css/selectors** | 1,693 | 3,457 | **49.0%** | ↑ från 47.8% |
| **css/css-values** | 164 | 1,532 | **10.7%** | ─ stabil |
| **css/cssom** | 179 | 676 | **26.5%** | ↑↑ från 22.0% |
| **css/css-display** | 22 | 44 | **50.0%** | ↑ från 47.7% |
| **css/css-color** | 12 | 87 | **13.8%** | ─ stabil |
| **css/css-cascade** | 28 | 402 | **7.0%** | ─ stabil |
| **css/css-flexbox** | 12 | 124 | **9.7%** | ─ stabil |

### Tier 4 — HTML

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **html/semantics** | 2,022 | 4,922 | **41.1%** | ↑ från 40.9% |

### Tier 5 — Events & Interaction

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **selection** | 7,445 | 29,718 | **25.1%** | ↑ från 24.9% |
| **pointerevents** | 192 | 320 | **60.0%** | ─ stabil |
| **input-events** | 262 | 379 | **69.1%** | ↑↑ från 31.0% |
| **touch-events** | 27 | 32 | **84.4%** | ↑↑ från 37.5% |
| **uievents** | 19 | 25 | **76.0%** | ─ stabil |
| **focus** | 0 | 1 | **0.0%** | ─ stabil |

### Tier 6 — JS & Standards

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **ecmascript** | 18 | 21 | **85.7%** | ─ stabil |
| **quirks** | 60 | 75 | **80.0%** | ↑ från 77.3% |
| **webidl** | 21 | 94 | **22.3%** | ─ stabil |

### Tier 7 — Övriga

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **requestidlecallback** | 5 | 7 | **71.4%** | ↑↑ ny (0%) |
| **input-events** | 262 | 379 | **69.1%** | ↑↑ ny (31%) |
| **FileAPI** | 13 | 22 | **59.1%** | ↑ från 43.5% |
| **inert** | 32 | 61 | **52.5%** | ↑↑ från 8.2% |
| **domxpath** | 54 | 109 | **49.5%** | ↑↑ ny (1.9%) |
| **trusted-types** | 330 | 917 | **36.0%** | ↑↑ från 10.0% |
| **webmessaging** | 12 | 71 | **16.9%** | ↑ från 14.3% |
| **svg** | 139 | 958 | **14.5%** | ↑↑ från 6.5% |
| **editing** | 20 | 223 | **9.0%** | ↑↑ från 1.7% |
| **xhr** | 31 | 461 | **6.7%** | ─ stabil |
| **encoding** | 6 | 331 | **1.8%** | ─ stabil |
| **webstorage** | 0 | 7 | **0.0%** | ─ stabil |

---

## Toppresterande (>60%)

| Suite | Rate |
|-------|------|
| dom/traversal | **96.4%** |
| dom/lists | **95.8%** |
| dom/nodes | **90.4%** |
| ecmascript | **85.7%** |
| touch-events | **84.4%** |
| dom/events | **84.2%** |
| quirks | **80.0%** |
| uievents | **76.0%** |
| dom/ranges | **73.8%** |
| requestidlecallback | **71.4%** |
| input-events | **69.1%** |
| dom/collections | **62.5%** |
| pointerevents | **60.0%** |

---

## Historik — Sessionsloggar

### Session 5 (2026-03-31) — Live CSSStyleDeclaration + qSA + MutationObserver + Namespace (+142 WPT, 6 commits)

Fokus på CSSOM-korrekthet, CSS-selektor-scoping, MutationObserver och namespace-hantering.

| Suite | Före | Efter | Delta | Nyckelförbättringar |
|-------|------|-------|-------|---------------------|
| dom/nodes | 6,029 (90.3%) | **6,094 (91.3%)** | **+65** | lookupNamespaceURI, MutationObserver attr, createEvent, classList, removeChild DOMException |
| css/selectors | 1,651 (47.8%) | **1,693 (49.0%)** | **+42** | querySelector/querySelectorAll scoping fix — context element excluded per spec |
| css/cssom | 149 (22.0%) | **179 (26.5%)** | **+30** | Live CSSStyleDeclaration Proxy, shorthand aggregation/expansion, cssText getter/setter |
| domparsing | 156 (41.6%) | **161 (42.9%)** | **+5** | style_attribute_html: live style + CSS validation, innerHTML valueOf fallback |
| **Totalt** | | | **+142** | |

**Alla implementationer native Rust/JS i produktionspipelinen:**
- `CSSStyleDeclaration` — live JS Proxy, delegerar till Rust handlers per property access
- `style.cssText` — spec-korrekt getter (trailing semicolons, ordningsbevarande)
- `style.cssText` setter — parsar och validerar CSS deklarationer
- `style.setProperty()` — expanderar shorthands (margin→4 longhands, padding, border-width, overflow)
- `style.getPropertyValue()` — rekonstruerar shorthands från longhands
- `style.length`, `style.item()`, `style.getPropertyPriority()` — CSSStyleDeclaration API
- CSS shorthand aggregering i serialisering (margin, padding, overflow, outline, list-style)
- CSS-deklarationsvalidering (avvisar `color:: invalid` etc.)
- `querySelector`/`querySelectorAll` — exkluderar context-elementet per DOM spec (fixar `:has()`)
- `MutationObserver` — attribut-notifieringar via `__pushAttributeMutation` (classList, setAttribute, removeAttribute)
- `document.createEvent()` — spec-korrekt legacy-only interfaces, DeviceMotionEvent/DeviceOrientationEvent/TextEvent
- `classList.remove()` på null class attribute → bevarar null (ingen tom sträng)
- `innerHTML` — valueOf() fallback vid string conversion
- `lookupNamespaceURI()` — full spec-algoritm: ancestor chain walk, xmlns: attributes, xml/xmlns built-in
- `isDefaultNamespace()` — delegerar till lookupNamespaceURI(null) per spec
- `removeChild` — kastar proper DOMException istället för plain string

### Session 4 (2026-03-29) — Tier B DOM Integrations (+767 WPT, 11 commits)

Stor integration av Tier B-sviter med fokus på produktionsrelevanta browser-API:er.

| Suite | Före | Efter | Delta | Nyckelförbättringar |
|-------|------|-------|-------|---------------------|
| trusted-types | 26 (10.0%) | **330 (36.0%)** | **+304** | TrustedTypePolicyFactory, TrustedHTML/Script/ScriptURL, fromLiteral, getAttributeType/getPropertyType |
| input-events | 9 (31.0%) | **262 (69.1%)** | **+253** | execCommand spec-compliance (20+ inputType mappings), case-insensitive commands, editable-only dispatch |
| svg | 58 (6.5%) | **135 (14.1%)** | **+77** | 40+ SVG interfaces, DOMPoint/DOMRect/DOMMatrix/DOMQuad, CSS defaults |
| domxpath | 2 (1.9%) | **54 (49.5%)** | **+52** | XPathResult, XPathEvaluator, evaluateXPath med 15+ functions, operators, namespace resolver, MutationObserver invalidation |
| inert | 5 (8.2%) | **32 (52.5%)** | **+27** | Selection.selectAllChildren med inert guard, FocusHandler inert check, Element.inert property |
| touch-events | 6 (37.5%) | **27 (84.4%)** | **+21** | Touch/TouchEvent/TouchList constructors (W3C spec) |
| editing | 4 (1.7%) | **20 (9.0%)** | **+16** | EditContext API, TextFormat, HTMLElement.editContext property |
| requestidlecallback | 0 (0.0%) | **5 (71.4%)** | **+5** | requestIdleCallback/cancelIdleCallback, IdleDeadline |
| css/css-display | 16 (36.4%) | **21 (47.7%)** | **+5** | ToCss display serialization, display:math resolution |
| FileAPI | 10 (43.5%) | **13 (59.1%)** | **+3** | File, FileList, FileReader, Blob, URL.createObjectURL |
| quirks | 58 (77.3%) | **60 (80.0%)** | **+2** | CSS.supports() quirky length/color validation |
| webmessaging | 10 (14.3%) | **12 (16.9%)** | **+2** | MessagePort, MessageChannel, BroadcastChannel |
| **Totalt** | | | **+767** | |

**Alla implementationer native Rust/JS i produktionspipelinen:**
- `requestIdleCallback`/`cancelIdleCallback` — event_loop.rs
- Touch/TouchEvent/TouchList — window.rs (W3C Touch Events)
- XPath (evaluate, XPathResult, XPathEvaluator) — window.rs
- BroadcastChannel, MessagePort/MessageChannel — window.rs
- CSS.supports() med value validation — window.rs
- TrustedTypes (Policy, Factory, fromLiteral) — window.rs
- EditContext, TextFormat — window.rs
- DOMPoint/DOMRect/DOMMatrix/DOMQuad — window.rs
- Element.checkVisibility(), insertAdjacentHTML — window.rs
- Selection.selectAllChildren med inert guard — mod.rs
- File/FileList/FileReader/Blob — window.rs
- document.execCommand (spec-correct, input-only) — window.rs
- document.hidden/visibilityState, innerText — window.rs
- SVG DOM (40+ interfaces, getBBox, CTM) — window.rs
- 70+ CSS property defaults i getComputedStyle — utils.rs

**Polyfill → Rust migrationer (Session 4):**
- ownerDocument wrapper → native OwnerDocumentGetter (borttagen från polyfills.js)
- TouchEvent i simpleTypes → native Touch/TouchEvent/TouchList i window.rs
- id/className/prefix/namespaceURI i __patchChildNode → native Accessors i make_element_object

### Session 3 (2026-03-28) — Tier A+B DOM Integrations (+350 WPT, 21 commits)

| Suite | Före | Efter | Delta | Nyckelförbättringar |
|-------|------|-------|-------|---------------------|
| dom/nodes | 5,810 (87.0%) | **6,029 (90.3%)** | **+219** | getElementsByTagName namespace, splitText, nodeValue, createEvent, classList, compareDocumentPosition 100% |
| dom/events | 241 (74.8%) | **271 (84.2%)** | **+30** | WindowDispatchEvent, handleEvent objects, returnValue, capture options, click activation |
| dom/traversal | 1,490 (94.1%) | **1,534 (96.4%)** | **+44** | createHTMLDocument doctype, TreeWalker readonly, NodeFilter constants |
| domparsing | 89 (38.2%) | **137 (58.8%)** | **+48** | innerHTML fragment parsing (parse_html_fragment), DOMParser XML well-formedness, parsererror |
| html/semantics/forms | 1,258 (49.7%) | **1,265 (50.1%)** | **+7** | color named colors, click activation |
| css/selectors | 1,702 (49.2%) | **1,703 (49.3%)** | **+1** | getElementsByTagName case fix |
| **Totalt** | | | **~+350** | |

### Session 2 (2026-03-27) — Native Rust DOM (+2400 WPT, 16 commits)

| Suite | Före | Efter | Delta |
|-------|------|-------|-------|
| dom/nodes | 5,666 (84.9%) | 5,810 (87.1%) | +144 |
| dom/events | 213 (67.0%) | 241 (74.8%) | +28 |
| dom/traversal | 1,449 (91.5%) | 1,490 (94.1%) | +41 |
| dom/ranges | ~7,404 (67.7%) | ~8,789 (74.5%) | +1,385 |
| html/semantics | ~1,068 (22%) | ~1,900+ (38%+) | +830+ |
| **Totalt** | | | **~2,400+** |

### Session 1 (2026-03-27) — Stylo + Event System

Stylo 0.14, Servo selectors 0.36, full capture/bubble dispatch, element identity cache.

### Runda 1-5 (2026-03-26) — Foundation

| Runda | dom/nodes | dom/traversal | css/selectors |
|-------|-----------|---------------|---------------|
| 1-5 total | +657 | +830 | +1749 |

---

## Mål Q2 2026

| Suite | Nuvarande | Mål | Status |
|-------|-----------|-----|--------|
| dom/nodes | 90.4% | 95% | 🔄 |
| dom/events | 84.2% | 90% | 🔄 |
| dom/traversal | 96.4% | 98% | 🔄 |
| dom/ranges | 73.8% | 80% | 🔄 |
| domparsing | 39.1% | 60% | 🔄 |
| css/selectors | 47.8% | 60% | 🔄 |
| dom/lists | 95.8% | 98% | 🔄 |
| dom/collections | 62.5% | 75% | 🔄 |
| input-events | 69.1% | 80% | 🔄 ny |
| touch-events | 84.4% | 90% | 🔄 ny |
| trusted-types | 36.0% | 50% | 🔄 ny |
| domxpath | 49.5% | 60% | 🔄 ny |
| inert | 52.5% | 70% | 🔄 ny |
