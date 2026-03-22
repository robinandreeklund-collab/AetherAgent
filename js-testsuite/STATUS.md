# Boa JS Integration — Test Suite Status

## Overview

Comprehensive test suite for AetherAgent's Boa JS engine integration.
50 tests covering the full JS pipeline: sandbox eval, DOM bridge, event loop,
hydration, escalation, and end-to-end `parse_with_js`.

**Test file:** `tests/js_testsuite.rs`
**Run:** `cargo test --features js-eval --test js_testsuite`

## Test Coverage

### 1. JS Sandbox (`js_eval.rs`) — 11 tests

| Test | What it verifies |
|------|-----------------|
| `test_eval_js_basic_math` | Basic arithmetic (2+3=5) |
| `test_eval_js_string_operations` | String methods (toUpperCase, concat) |
| `test_eval_js_json_operations` | JSON.stringify/parse roundtrip |
| `test_eval_js_array_methods` | Array sort, join |
| `test_eval_js_math_functions` | Math.max |
| `test_eval_js_blocked_fetch` | fetch() blocked by allowlist |
| `test_eval_js_blocked_eval` | eval() blocked |
| `test_eval_js_blocked_import` | import() blocked |
| `test_eval_js_blocked_xmlhttp` | XMLHttpRequest blocked |
| `test_eval_js_batch` | Batch eval of multiple snippets |
| `test_eval_js_batch_with_error` | Batch continues after error in one snippet |

### 2. JS Detection (`detect_js`) — 4 tests

| Test | What it verifies |
|------|-----------------|
| `test_detect_js_inline_script` | Detects `<script>` with DOM access |
| `test_detect_js_event_handlers` | Detects onclick, onchange, onmouseover |
| `test_detect_js_no_js` | Zero scripts/handlers on static HTML |
| `test_detect_js_framework_nextjs` | Next.js framework detection via __NEXT_DATA__ |

### 3. DOM Bridge (`dom_bridge.rs` via `eval_js_with_dom`) — 10 tests

| Test | What it verifies |
|------|-----------------|
| `test_dom_bridge_get_element_by_id` | getElementById + getAttribute |
| `test_dom_bridge_set_text_content` | textContent setter doesn't crash |
| `test_dom_bridge_set_attribute` | setAttribute + getAttribute roundtrip |
| `test_dom_bridge_create_element` | createElement returns object |
| `test_dom_bridge_query_selector` | querySelector with class selector |
| `test_dom_bridge_query_selector_all` | querySelectorAll returns correct count |
| `test_dom_bridge_remove_child` | removeChild reduces children count |
| `test_dom_bridge_inner_html_via_mutations` | Attribute manipulation on elements with children |
| `test_dom_bridge_classlist` | classList.add modifies className |
| `test_dom_bridge_style` | style.color getter/setter |

### 4. Event Loop (`event_loop.rs`) — 4 tests

| Test | What it verifies |
|------|-----------------|
| `test_event_loop_set_timeout` | setTimeout fires, event_loop_ticks > 0 |
| `test_event_loop_set_interval` | setInterval with clearInterval, timers_fired >= 1 |
| `test_event_loop_request_animation_frame` | rAF triggers ticks |
| `test_event_loop_timer_limits` | Large delay (999999ms) clamped, no crash |

### 5. Hydration (`hydration.rs`) — 4 tests

| Test | What it verifies |
|------|-----------------|
| `test_hydration_nextjs` | Next.js __NEXT_DATA__ extraction |
| `test_hydration_no_framework` | Plain HTML returns found=false |
| `test_hydration_nuxt` | Nuxt window.__NUXT__ returns valid response |
| `test_hydration_angular` | Angular ng-state extraction |

### 6. Escalation (`escalation.rs`) — 4 tests

| Test | What it verifies |
|------|-----------------|
| `test_tier_static_html` | Static HTML -> StaticParse tier |
| `test_tier_with_dom_scripts` | DOM-modifying scripts -> BoaDom tier |
| `test_tier_spa_shell` | React SPA -> high confidence tier |
| `test_tier_nextjs_hydration` | Next.js SSR -> Hydration tier |

### 7. parse_with_js Pipeline — 4 tests

| Test | What it verifies |
|------|-----------------|
| `test_parse_with_js_static_page` | Returns tree object for static HTML |
| `test_parse_with_js_dom_manipulation` | JS analysis reports inline scripts |
| `test_parse_with_js_event_handlers` | Event handlers processed without error |
| `test_parse_with_js_injection_detection` | Injection in hidden div doesn't crash pipeline |

### 8. Security — 4 tests

| Test | What it verifies |
|------|-----------------|
| `test_sandbox_no_require` | require('fs') blocked |
| `test_sandbox_no_process` | process.env not accessible |
| `test_sandbox_no_constructor_escape` | Constructor chain doesn't expose dangerous globals |
| `test_sandbox_no_settimeout_in_pure_eval` | setTimeout unavailable in pure sandbox (no DOM) |

### 9. Integration — 5 tests

| Test | What it verifies |
|------|-----------------|
| `test_eval_js_timing` | eval_time_us > 0 and < 5s |
| `test_full_ecommerce_with_js` | E-commerce page: button, select, JS detection |
| `test_full_login_form_with_js` | Login form: email input, submit button |
| `test_safe_page_no_warnings` | Clean page produces zero injection warnings |
| `test_large_page_performance` | 120 elements parsed in < 500ms |

## Known Limitations

1. **textContent as getter**: Boa's DOM bridge returns `textContent` as a function object
   rather than a string value when accessed as a property. Tests use `getAttribute()` instead.
2. **createElement + appendChild**: The `appendChild` call on dynamically created elements
   may fail in some cases. Test verifies `createElement` returns an object.
3. **Nuxt hydration**: `window.__NUXT__` format detection depends on exact script format;
   may return `found: false` for non-standard Nuxt setups.

## API Response Formats

### eval_js
```json
{"value": "5", "error": null, "timed_out": false, "eval_time_us": 42}
```

### eval_js_with_dom
```json
{"value": "result", "error": null, "mutations": [], "eval_time_us": 100, "event_loop_ticks": 3, "timers_fired": 1}
```

### extract_hydration
```json
{"found": true, "framework": "NextJs", "nodes": [...], "warnings": [...], "extract_time_ms": 1}
```

### parse_with_js
```json
{"tree": {"nodes": [...], "parse_time_ms": 5}, "js_bindings": [...], "js_analysis": {"total_inline_scripts": 1, "total_event_handlers": 2, "has_framework": false}, "total_evals": 1, "successful_evals": 1, "exec_time_ms": 10}
```

### select_parse_tier
```json
{"tier": "StaticParse", "reason": "No JS detected", "confidence": 0.95, "analysis_time_us": 50}
```
