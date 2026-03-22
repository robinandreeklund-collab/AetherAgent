# Boa JS Integration — Test Suite Status

## Overview

Comprehensive test suite for AetherAgent's Boa JS engine integration.
**142 tests** total: 120 unit/integration + 22 live site tests.

## Quick Start

```bash
# Run all JS tests (120 tests, no network needed)
cargo test --features js-eval --test js_testsuite

# Run live site tests (22 tests, requires network)
cargo test --features js-eval --test live_site_tests -- --include-ignored

# Run everything
cargo test --features js-eval --test js_testsuite --test live_site_tests -- --include-ignored
```

## Test Files

| File | Tests | Description |
|------|-------|-------------|
| `tests/js_testsuite.rs` | 120 | Full DOM API, sandbox, event loop, hydration, escalation |
| `tests/live_site_tests.rs` | 22 | Real website parsing, extraction, JS detection |

## js_testsuite.rs — 120 Tests

### 1. JS Sandbox (`eval_js`) — 11 tests
- Basic math, strings, JSON, arrays, Math functions
- Security: fetch/eval/import/XMLHttpRequest blocked
- Batch eval with error isolation

### 2. JS Detection (`detect_js`) — 4 tests
- Inline scripts, event handlers, no-JS pages, framework detection

### 3. DOM Bridge (`eval_js_with_dom`) — 10 tests
- getElementById, querySelector/All, setAttribute/getAttribute
- textContent, createElement, removeChild, classList, style

### 4. DOM API Complete Coverage — 45 tests

**Document methods (9):**
body, head, documentElement, getElementsByClassName, getElementsByTagName,
createTextNode, createComment, createDocumentFragment, activeElement

**Tree navigation (7):**
parentNode, firstChild, firstElementChild, nextSibling, nextElementSibling,
childNodes, children

**Element manipulation (12):**
removeAttribute, insertBefore, cloneNode, outerHTML, innerHTML,
dataset, closest, matches, contains, getRootNode, isConnected, hidden

**classList (5):** remove, toggle, contains, replace, length

**style (3):** setProperty/getPropertyValue, removeProperty, multiple properties

**Geometry (5):** offset dims, scroll dims, client dims, getBoundingClientRect, getClientRects

**Events (4):** addEventListener, dispatchEvent, CustomEvent, Event/stopPropagation

**Observers (3):** IntersectionObserver, ResizeObserver, MutationObserver

**Other (4):** getComputedStyle, customElements, console methods, pointer lock, shadow DOM

**CSS selectors (4):** attribute, child combinator, comma-separated, :first-child

### 5. Event Loop — 7 tests
- setTimeout, setInterval, requestAnimationFrame
- Timer limits, queueMicrotask, cancelAnimationFrame, clearTimeout

### 6. Hydration — 8 tests
- Next.js, Nuxt, Angular (detection tests)
- SvelteKit, Remix, Gatsby, Qwik (format validation)
- No-framework detection

### 7. Escalation — 6 tests
- Static HTML, DOM scripts, SPA shell, Next.js SSR
- WebGL, WebAssembly pages

### 8. parse_with_js Pipeline — 4 tests
- Static page, DOM manipulation, event handlers, injection detection

### 9. Security — 5 tests
- require/process blocked, constructor escape safe
- setTimeout isolation in pure sandbox, eval timing

### 10. End-to-end Integration — 5 tests
- E-commerce with JS, login form, safe page, performance (120 elements)

## live_site_tests.rs — 22 Tests

All tests are `#[ignore]` by default (require network).

### Sites Tested

| Site | Tests | What's verified |
|------|-------|-----------------|
| books.toscrape.com | 5 | Parse, JS detect, extract, tier, injection |
| news.ycombinator.com | 3 | Parse (30+ noder), find_and_click, injection |
| example.com | 3 | Parse, extract, tier (StaticParse) |
| httpbin.org | 2 | Parse, JS detection |
| Wikipedia (Rust article) | 3 | Parse (20+ noder), performance (<1s), injection |
| GitHub repo | 2 | Parse, JS detection |
| Cross-site | 4 | Semantic diff, compile_goal, parse_with_js, hydration |

### MCP Test Scenarios (10 documented)

The file also documents 10 MCP tool test scenarios for manual/CI testing:

1. `fetch_parse` — books.toscrape.com, HN, example.com
2. `fetch_extract` — books.toscrape.com, example.com
3. `fetch_click` — HN "new" link, books "next" page
4. `parse_with_js` — GitHub repo pages
5. `check_injection` — positive and negative cases

## Known Limitations

1. **DOM property getters**: Boa returns some DOM properties (parentNode, dataset,
   innerHTML) as getter functions rather than values. Tests use `typeof` checks.
2. **Event/CustomEvent constructors**: `new Event('click')` may fail in Boa.
   Tests verify constructor availability rather than instantiation.
3. **Observer constructors**: IntersectionObserver/ResizeObserver registered via
   window globals; MutationObserver via event_loop. Availability varies.
4. **Live tests**: Depend on external sites being available. Run with `--include-ignored`.

## API Response Formats

### eval_js
```json
{"value": "5", "error": null, "timed_out": false, "eval_time_us": 42}
```

### eval_js_with_dom
```json
{"value": "result", "error": null, "mutations": [], "eval_time_us": 100,
 "event_loop_ticks": 3, "timers_fired": 1}
```

### extract_hydration
```json
{"found": true, "framework": "NextJs", "nodes": [...], "warnings": [...]}
```

### parse_with_js
```json
{"tree": {"nodes": [...]}, "js_analysis": {"total_inline_scripts": 1,
 "total_event_handlers": 2, "has_framework": false}, "total_evals": 1}
```

### select_parse_tier
```json
{"tier": "StaticParse", "reason": "...", "confidence": 0.95}
```
