# WPT Dashboard — AetherAgent

> Komplett Web Platform Tests resultat per svit och subkategori.
> Baseline-datum: 2026-03-25
>
> **Referens:** Se [wpt-testing-strategy.md](wpt-testing-strategy.md) för strategi
> och [wpt-workflow-guide.md](wpt-workflow-guide.md) för arbetsflöde.

---

## Sammanfattning

| Tier | Sviter | Cases | Passed | Rate |
|------|--------|-------|--------|------|
| **Tier 1** (Kärna) | dom/nodes, events, ranges, traversal, collections | ~20,000+ | ~12,800+ | ~64% |
| **Tier 2** (Stödjande) | dom/abort, lists, domparsing, html/syntax, html/dom, css/selectors | ~2,300+ | ~360+ | ~16% |
| **Tier 3** (Utökad) | encoding, webstorage, xhr, css/cssom, hr-time, console, url | ~1,300+ | ~72 | ~5% |
| **Total alla sviter** | | **~23,600+** | **~13,200+** | — |

---

## Tier 1 — Kärna (kör varje PR)

### dom/nodes/ — Core DOM

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 286 | 6,624 | 4,946 | 74.7% | Baseline |
| **2026-03-25** | **286** | **6,624** | **5,017** | **75.7%** | +71 pass: Event fix, classList, Text/Comment constructors |

**Toppresterare:**
- CharacterData: ~100%
- ChildNode (before/after/replaceWith): 100%
- Node-cloneNode: 93%
- Document-createEvent: 94%
- querySelector-escapes: 91%

**Största failures:**
- Node-insertBefore (skipped — hänger)
- Namespace-relaterade tester
- ProcessingInstruction-stöd

**Mål Q2 2026:** 90%

---

### dom/events/ — Event System

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 160 | 312 | 100 | 32.1% | Baseline |
| 2026-03-25 | 160 | 311 | 109 | 35.0% | +9 pass: Event constants, cancelBubble, initEvent |
| 2026-03-25 | 160 | 311 | 140 | 45.0% | +31 pass: Event subclasses, cancelBubble spec fix |
| **2026-03-25** | **160** | **311** | **195** | **62.7%** | +55 pass: passive preventDefault, CompositionEvent.data |

**Implementerat:**
- addEventListener med options (capture, passive) ✅
- dispatchEvent med bubbling ✅
- stopPropagation/stopImmediatePropagation ✅
- Event/CustomEvent constructors ✅

**Saknas:**
- Event subclasses (MouseEvent, KeyboardEvent med properties)
- Event phases (capture → target → bubble) edge cases
- scroll events, passive-by-default tester

**Mål Q2 2026:** 90%

---

### dom/ranges/ — Range API

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | ~55 | ~11,000+ | ~7,700+ | ~70% | Baseline (polyfill, inkl. slow tester) |
| 2026-03-25 | 55 | 645 | 315 | 48.8% | Native Range, 6 slow skippade |
| 2026-03-25 | 55 | 11,373 | 6,958 | 61.2% | Rust compare_boundary_points, 4 filer re-enabled |
| 2026-03-25 | 55 | 10,762 | 7,182 | 66.7% | WrongDocumentError, nativeChildIndex, toString |
| **2026-03-25** | **55** | **~10,800** | **~7,400+** | **~69%** | ownerDocument lazy getter, getSelection, Range mutations scaffolding |

**Native:** Range i `dom_bridge.rs`, `__nativeCompareBoundary` + `__nativeChildIndex` i Rust.

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
| **2026-03-25** | **18** | **1,584** | **516** | **32.6%** | Baseline |

**Implementerat:**
- TreeWalker: nextNode, previousNode, parentNode, firstChild ✅
- NodeIterator: nextNode, previousNode ✅
- whatToShow filter ✅

**Saknas:**
- ProcessingInstruction-stöd (common.js testNodes)
- NodeIterator: referenceNode tracking efter DOM-mutation
- Avancerade filter-callbacks

**Mål Q2 2026:** 60%

---

### dom/collections/ — HTMLCollection & NodeList

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **9** | **48** | **6** | **12.5%** | Baseline |

**Saknas:**
- Live HTMLCollection (getElementsBy* returnerar statisk array)
- Named property access (collection["id"])
- NodeList iteration

**Mål Q2 2026:** 50%

---

## Tier 2 — Stödjande (kör vid relevanta ändringar)

### dom/lists/ — DOMTokenList

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| 2026-03-25 | 5 | 189 | 175 | 92.6% | Baseline |
| **2026-03-25** | **5** | **189** | **179** | **94.7%** | +4 pass: classList raw value, unique tokens |

Nästan komplett tack vare native classList-implementation.

---

### dom/abort/ — AbortController

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **5** | **2** | **0** | **0.0%** | Baseline |

AbortController/AbortSignal saknas helt. Låg prioritet.

---

### domparsing/ — DOMParser & Serialization

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **53** | **453** | **25** | **5.5%** | Baseline |

**Implementerat:**
- DOMParser.parseFromString (basic) ✅
- innerHTML getter/setter ✅

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
| **2026-03-25** | **536** | **340** | **68** | **20.0%** | +42 pass: ownerDocument fix |

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
| **2026-03-25** | **636** | **761** | **91** | **12.0%** | Baseline |

**Implementerat (native Rust):**
- ID, class, tag selectors ✅
- Attribute selectors ([attr], [attr="val"], [attr~="val"]) ✅
- Child (>) / descendant ( ) combinators ✅
- :first-child, :last-child, :nth-child, :nth-of-type ✅
- :root, :empty, :checked, :not() ✅

**Saknas:**
- Adjacent sibling (+) / General sibling (~)
- :has(), :is(), :where()
- :nth-last-child, :nth-last-of-type
- :only-child, :only-of-type
- Attribute selectors: ^=, $=, *=

**Mål Q2 2026:** 40%

---

### css/cssom/ — CSS Object Model ⭐ NY

| Datum | Filer | Cases | Passed | Rate | Kommentar |
|-------|-------|-------|--------|------|-----------|
| **2026-03-25** | **210** | **531** | **43** | **8.1%** | Baseline |

**Implementerat:**
- style.setProperty/getPropertyValue/removeProperty ✅
- getComputedStyle (basic) ✅

**Saknas:**
- CSSStyleSheet API
- CSSStyleDeclaration fullständig
- window.getComputedStyle med cascading

**Mål Q2 2026:** 20%

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

## Tier 4 — Framtida

### custom-elements/

| Datum | Filer | Cases | Passed | Rate |
|-------|-------|-------|--------|------|
| 2026-03-25 | 172 | 930 | 18 | 1.9% |

### shadow-dom/

| Datum | Filer | Cases | Passed | Rate |
|-------|-------|-------|--------|------|
| 2026-03-25 | 295 | 1,393 | 24 | 1.7% |

### html/semantics/

| Datum | Filer | Cases | Passed | Rate |
|-------|-------|-------|--------|------|
| 2026-03-25 | 2,803 | TBD | TBD | TBD |

---

## Historik — Övergripande

| Datum | dom/ total | Alla sviter | Kommentar |
|-------|-----------|-------------|-----------|
| 2026-03-24 | 1,382/2,004 (69.0%) | — | Första baseline (5s timeout) |
| 2026-03-25 | 13,383/19,938 (67.1%) | ~13,600/23,649 (57.4%) | 30s timeout, 10x fler tester |
| 2026-03-25 | — | Se ovan per svit | Ny detaljerad baseline med alla sviter |

---

## Köra tester

```bash
# Setup
./wpt/setup.sh

# Tier 1 (obligatoriskt per PR)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/events/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/ranges/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/traversal/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/collections/

# Tier 2 (vid relevanta ändringar)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/lists/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/domparsing/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/html/syntax/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/css/selectors/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/css/cssom/

# Tier 3 (milstolpar)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/encoding/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/webstorage/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/xhr/

# Verbose (debug)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose
```

---

## Implementation Status

Se detaljerad API-täckning:
- [dom-implementation-status.md](dom-implementation-status.md) — Native vs Polyfill per API
- [dom-api-coverage.md](dom-api-coverage.md) — Full referens, 70+ metoder
- [wpt-testing-strategy.md](wpt-testing-strategy.md) — Strategi och prioritering
- [wpt-workflow-guide.md](wpt-workflow-guide.md) — Arbetsflöde steg-för-steg
