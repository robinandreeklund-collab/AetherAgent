# WPT Dashboard — AetherAgent

> Komplett Web Platform Tests resultat per svit och subkategori.
> Baseline-datum: 2026-03-25 | Senast uppdaterad: 2026-03-28 (Session 3: +350 WPT pass, native Rust DOM)
>
> **Referens:** Se [wpt-testing-strategy.md](wpt-testing-strategy.md) för strategi
> och [wpt-workflow-guide.md](wpt-workflow-guide.md) för arbetsflöde.

---

## Sammanfattning (2026-03-28)

| Tier | Sviter | Cases | Passed | Rate |
|------|--------|-------|--------|------|
| **Tier 1** (Core DOM) | dom/nodes, events, ranges, traversal, collections, lists | ~30,100 | ~17,700+ | ~59% |
| **Tier 2** (Parsing & Serialization) | domparsing, html/syntax | ~794 | ~377 | ~47% |
| **Tier 3** (CSS) | css/selectors, css-values, css-cascade, cssom, css-display, css-color, css-flexbox | ~6,296 | ~2,045 | ~32% |
| **Tier 4** (HTML) | html/semantics/forms, html/webappapis/timers | ~2,529 | ~1,266 | ~50% |
| **Tier 5** (Events & Interaction) | uievents, pointerevents, focus, selection | ~29,729 | ~7,450 | ~25% |
| **Tier 6** (JS & Standards) | ecmascript, webidl, quirks | ~190 | ~97 | ~51% |
| **Tier 7** (Övriga) | FileAPI, trusted-types, svg, xhr, encoding, inert, domxpath, webstorage, url, wai-aria, accname, html-aam, core-aam | ~3,683 | ~239 | ~6% |

---

## Alla sviter — Fullständig status (2026-03-28)

### Tier 1 — Core DOM (kör varje PR)

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **dom/nodes** | 6,029 | 6,676 | **90.3%** | ↑ från 84.9% |
| **dom/events** | 271 | 322 | **84.2%** | ↑ från 67.0% |
| **dom/ranges** | 8,153 | 10,995 | **74.2%** | ↑ från ~67.7% |
| **dom/traversal** | 1,534 | 1,591 | **96.4%** | ↑ från 91.5% |
| **dom/collections** | 30 | 48 | **62.5%** | ↑ från 56.2% |
| **dom/lists** | 181 | 189 | **95.8%** | ─ stabil |

### Tier 2 — Parsing & Serialization

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **domparsing** | 137 | 233 | **58.8%** | ↑ från 18.8% |
| **html/syntax** | 240 | 561 | **42.8%** | ↑ från 20.0% |

### Tier 3 — CSS

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **css/selectors** | 1,703 | 3,457 | **49.3%** | ↑ från 12.0% baseline |
| **css/css-values** | 164 | 1,526 | **10.7%** | ny |
| **css/cssom** | 112 | 656 | **17.1%** | ↑ från 14.3% |
| **css/css-display** | 16 | 44 | **36.4%** | ny |
| **css/css-color** | 12 | 87 | **13.8%** | ny |
| **css/css-cascade** | 27 | 402 | **6.7%** | ny |
| **css/css-flexbox** | 11 | 124 | **8.9%** | ny |

### Tier 4 — HTML

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **html/semantics/forms** | 1,265 | 2,528 | **50.0%** | ny |
| **html/webappapis/timers** | 1 | 1 | **100%** | ny |

### Tier 5 — Events & Interaction

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **selection** | 7,239 | 29,383 | **24.6%** | ny |
| **pointerevents** | 192 | 320 | **60.0%** | ny |
| **uievents** | 19 | 25 | **76.0%** | ny |
| **focus** | 0 | 1 | **0.0%** | ny |

### Tier 6 — JS & Standards

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **ecmascript** | 18 | 21 | **85.7%** | ny |
| **quirks** | 58 | 75 | **77.3%** | ny |
| **webidl** | 21 | 94 | **22.3%** | ny |

### Tier 7 — Övriga

| Suite | Passed | Total | Rate | Trend |
|-------|--------|-------|------|-------|
| **svg** | 58 | 887 | **6.5%** | ny |
| **FileAPI** | 10 | 23 | **43.5%** | ny |
| **xhr** | 31 | 461 | **6.7%** | ny |
| **encoding** | 6 | 331 | **1.8%** | ny |
| **trusted-types** | 26 | 259 | **10.0%** | ny |
| **domxpath** | 2 | 105 | **1.9%** | ny |
| **inert** | 5 | 61 | **8.2%** | ny |
| **webstorage** | 0 | 7 | **0.0%** | ny |
| **url** | 0 | 1 | **0.0%** | ny |
| **wai-aria** | 0 | 0 | **N/A** | test setup issues |
| **accname** | 0 | 0 | **N/A** | test setup issues |
| **html-aam** | 0 | 0 | **N/A** | test setup issues |
| **core-aam** | 0 | 0 | **N/A** | test setup issues |

---

## Toppresterande (>75%)

| Suite | Rate |
|-------|------|
| html/webappapis/timers | **100%** |
| dom/traversal | **96.4%** |
| dom/lists | **95.8%** |
| dom/nodes | **90.3%** |
| ecmascript | **85.7%** |
| dom/events | **84.2%** |
| quirks | **77.3%** |
| uievents | **76.0%** |

---

## Historik — Sessionsloggar

### Session 3 (2026-03-28) — Tier A+B DOM Integrations (+350 WPT, 21 commits)

| Suite | Före | Efter | Delta | Nyckelförbättringar |
|-------|------|-------|-------|---------------------|
| dom/nodes | 5,810 (87.0%) | **6,029 (90.3%)** | **+219** | getElementsByTagName namespace, splitText, nodeValue, createEvent, classList, compareDocumentPosition 100% |
| dom/events | 241 (74.8%) | **271 (84.2%)** | **+30** | WindowDispatchEvent, handleEvent objects, returnValue, capture options, click activation |
| dom/traversal | 1,490 (94.1%) | **1,534 (96.4%)** | **+44** | createHTMLDocument doctype, TreeWalker readonly, NodeFilter constants |
| domparsing | 89 (38.2%) | **137 (58.8%)** | **+48** | innerHTML fragment parsing (parse_html_fragment), DOMParser XML well-formedness, parsererror |
| html/semantics/forms | 1,258 (49.7%) | **1,265 (50.1%)** | **+7** | color named colors, click activation |
| css/selectors | 1,702 (49.2%) | **1,703 (49.3%)** | **+1** | getElementsByTagName case fix |
| uievents | 18 | **19** | **+1** | NodeFilter constants |
| **Totalt** | | | **~+350** | |

**Alla implementationer native Rust i produktionspipelinen:**
- `parse_html_fragment()` i parser.rs — innerHTML utan html/head/body wrapper
- `check_xml_well_formed()` — Rust XML validator för DOMParser
- `getElementsByTagName` — spec-korrekt namespace, case-sensitivity, context exclusion
- `Text.splitText()` — UTF-16-medveten split
- `WindowDispatchEvent` — korrekt window event dispatch med WINDOW_EVENT_KEY
- `AttrGetter/AttrSetter` — native id/className/namespaceURI accessors
- `NodeValueSetter` — null→"" för Text/Comment/PI
- `is_valid_xml_name()` — XML Name-produktion validator
- `sanitize_color()` — 148 namngivna CSS-färger + #rgb expansion

**Polyfill → Rust migrationer:**
- id/className → native AttrGetter/AttrSetter
- namespaceURI/prefix → native i make_element_object
- nodeValue → native NullGetter + NodeValueSetter
- Document.textContent → native accessor i register_document
- splitText → native i chardata.rs
- innerHTML → parse_html_fragment (Rust html5ever)

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
| dom/nodes | 90.3% | 95% | 🔄 |
| dom/events | 84.2% | 90% | 🔄 |
| dom/traversal | 96.4% | 98% | 🔄 |
| dom/ranges | 74.2% | 80% | 🔄 |
| domparsing | 58.8% | 70% | 🔄 |
| css/selectors | 49.3% | 60% | 🔄 |
| dom/lists | 95.8% | 98% | 🔄 |
| dom/collections | 62.5% | 75% | 🔄 |
