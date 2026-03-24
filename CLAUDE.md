# AetherAgent – Project Standards

## Project Overview

AetherAgent is an LLM-native embeddable browser engine written in Rust, compiled to WebAssembly.
It provides a semantic perception layer for AI agents with built-in prompt injection protection.

## Code Standard

### Language & Style

- **Language**: All code is Rust (2021 edition), targeting `wasm32-unknown-unknown` and native.
- **Formatting**: `cargo fmt` must pass with zero diffs. Run before every commit.
- **Linting**: `cargo clippy -- -D warnings` must pass with zero warnings. No `#[allow]` suppression without documented justification.
- **No dead code**: Every function, field, import, and dependency must be used. Remove anything unused immediately.
- **No unused dependencies**: Every crate in `Cargo.toml` must be actively imported somewhere in `src/`. Audit before adding new dependencies.

### Safety & Correctness

- **No `.unwrap()` on fallible operations in non-test code.** Use `unwrap_or`, `unwrap_or_else`, `?`, or proper error handling.
- **No `partial_cmp().unwrap()`** — use `total_cmp()` for f32/f64 sorting to prevent NaN panics.
- **No global mutable state** (static mut, global AtomicU32 shared across instances). Use local state in structs.
- **UTF-8 safety**: All string operations must handle multi-byte characters. Never use byte offsets from one string on another. Always verify `is_char_boundary()` before `replace_range()`.
- **Thread safety**: All types used across threads must be `Send + Sync`. Prefer local counters over shared atomics.

### Architecture Rules

- **Module boundaries**: `lib.rs` is the only public API surface. Internal modules (`parser`, `semantic`, `trust`, `types`) are `mod` (private).
- **Trust by default**: All web content is `TrustLevel::Untrusted`. Never promote trust level without explicit justification.
- **Separation of concerns**:
  - `parser.rs` — HTML parsing, DOM traversal, attribute extraction, WCAG label chain
  - `semantic.rs` — Accessibility tree building, goal-relevance scoring
  - `trust.rs` — Prompt injection detection, sanitization, content wrapping
  - `types.rs` — Data structures and their inherent methods
  - `fetch.rs` — HTTP page fetching, cookies, redirects, robots.txt, SSRF protection
  - `firewall.rs` — Semantic Firewall: 3-level goal-aware request filtering (L1/L2/L3)
  - `lib.rs` — WASM API surface, orchestration, serialization
- **No feature creep**: Only implement what the current Fas (phase) requires. Do not add speculative abstractions, unused helpers, or future-proofing code.

### Naming

- Module-level constants: `UPPER_SNAKE_CASE`
- Functions and methods: `snake_case`
- Types and enums: `PascalCase`
- Comments and doc comments: Swedish for internal comments, English for public API doc comments (`///`).

### Dependencies

- Minimize dependency count. Every new crate must justify its inclusion.
- Pin to minor versions (e.g., `"1.0"` not `"*"`).
- `[dev-dependencies]` for test-only crates. Never put test crates in `[dependencies]`.

## Test Standard

### Golden Rule

**All changes must be tested end-to-end. No exceptions.**

Every PR, every commit, every fix — regardless of size — must pass the full test suite before merge:

```bash
cargo test              # ALL tests (unit + integration)
cargo clippy -- -D warnings
cargo fmt --check
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/  # WPT baseline
```

### Test Levels

#### Unit Tests (in each module's `#[cfg(test)] mod tests`)

Every public function in a module must have at least one unit test. Place tests at the bottom of the source file they test.

- `parser.rs` — Test HTML parsing, attribute extraction, label fallback chain, visibility detection
- `semantic.rs` — Test relevance scoring, tree building, structural node handling
- `trust.rs` — Test injection pattern detection, zero-width char detection, sanitization, wrapping
- `lib.rs` — Test WASM API functions return valid JSON, limits are respected

#### Integration Tests (`tests/integration_test.rs`)

End-to-end tests that exercise the full pipeline: HTML input -> parse -> semantic tree -> JSON output.

These must cover:

- **E-commerce scenarios** — Product pages with buttons, links, selects, price text
- **Form scenarios** — Login forms, registration, search forms
- **Injection scenarios** — Hidden injection text, zero-width characters, multi-pattern attacks
- **Safe content scenarios** — Normal pages must produce zero warnings
- **Performance scenarios** — Pages with 100+ elements must parse in <500ms
- **Top-N scenarios** — `parse_top_nodes` must respect the limit

#### When to Add Tests

- **New feature**: Unit test + integration test covering the feature end-to-end.
- **Bug fix**: Add a regression test that reproduces the bug BEFORE fixing it, then verify it passes after.
- **Refactor**: All existing tests must still pass. Add tests if the refactor changes behavior.

### Test Requirements

- **Recursive node search**: Integration tests must search the full tree (including children), not just top-level nodes. Use `find_node_recursive` or equivalent.
- **No hardcoded DOM traversal paths** (e.g., `children[0].children[1]`). DOM structure can change — use semantic search by role/label.
- **Assertions must have descriptive messages**: `assert!(x, "Borde hitta button")` — not bare `assert!(x)`.
- **Raw strings with `#`**: Use `r##"..."##` for HTML containing `href="#"` to avoid raw string delimiter conflicts.

### WPT Standard (Web Platform Tests)

**Every PR must include WPT results.** This is a mandatory quality gate.

WPT tests run the official, unmodified Web Platform Tests from https://github.com/web-platform-tests/wpt directly against AetherAgent's DOM implementation via QuickJS sandbox + DOM bridge.

**Important:** WPT pass rate includes both Rust-native and JS-polyfilled implementations. See `docs/dom-implementation-status.md` for the full breakdown. The goal is to migrate all polyfills to Rust-native implementations. When implementing a new DOM method:
1. Implement it in Rust (`dom_bridge.rs`) — not as a polyfill
2. Remove the corresponding polyfill from `wpt/polyfills.js`
3. Verify WPT score doesn't regress

#### Setup

```bash
./wpt/setup.sh          # Sparse-checkout av relevanta WPT-tester
```

#### Running WPT

```bash
# Kör alla dom/nodes tester
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/

# Kör med verbose output (visar varje testcase)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose

# Kör specifik fil
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/Document-getElementById.html

# JSON-output (för CI)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --json
```

#### PR Requirements

Every PR that touches DOM, JS eval, event loop, CSS, or parser code MUST include:

1. **WPT score before** (baseline)
2. **WPT score after** (med ändringar)
3. **Delta** (förbättring eller regression)

Format i PR-beskrivning:
```
## WPT Results
- dom/nodes: 90/1026 passed (8.8%) → XX/1026 passed (X.X%)
- Nya pass: [lista testnamn]
- Nya failures: [lista om regression]
```

**WPT score får aldrig gå ner utan dokumenterad motivering.**

#### Baseline (2026-03-24)

| Suite | Cases | Passed | Rate |
|-------|-------|--------|------|
| dom/ (total) | 2,004 | 1,225 | 61.1% |

#### Targeted Test Directories

| Directory | Relevance |
|-----------|-----------|
| `dom/nodes/` | Core DOM: getElementById, querySelector, appendChild, textContent |
| `dom/events/` | Event dispatch, bubbling, capture |
| `dom/traversal/` | TreeWalker, NodeIterator |
| `dom/collections/` | HTMLCollection, NodeList |
| `html/dom/` | HTML-specific DOM APIs |
| `html/syntax/` | HTML parsing edge cases |
| `html/webappapis/timers/` | setTimeout, setInterval |
| `selectors/` | CSS selector matching |
| `shadow-dom/` | Shadow DOM (not yet implemented) |
| `custom-elements/` | Custom elements (not yet implemented) |

### Bug Policy

**All bugs found during any PR — even if unrelated to the PR's scope — must be fixed immediately in the same PR.**

Do not defer bugs to separate issues. Do not merge known-broken code. The codebase must always be in a working state on every commit.

This includes:
- Test failures discovered while working on other features
- Clippy warnings introduced by dependency updates
- Formatting drift
- Dead code or unused imports
- Logic bugs discovered during code review

## Build & CI

### Local Development Loop

```bash
# Quick check (run before every commit)
cargo test && cargo clippy -- -D warnings && cargo fmt --check

# WASM build (run before release)
wasm-pack build --target web --release
```

### CI Pipeline (`.github/workflows/ci.yml`)

All jobs must pass before merge:

| Job | What it checks |
|-----|----------------|
| `test` | `cargo test` (all), `cargo clippy -- -D warnings`, `cargo fmt --check` |
| `wasm-build` | WASM compiles, binary size < 6MB |
| `python-test` | Python mock integration runs without errors |
| `security` | `cargo audit` finds no known vulnerabilities |

### Commit Messages

Format: `type: short description`

Types:
- `feat:` — New functionality
- `fix:` — Bug fix
- `refactor:` — Code improvement without behavior change
- `test:` — Test additions or fixes
- `docs:` — Documentation only
- `ci:` — CI/CD changes

## Current Phase

**Fas 1 (Complete)**: HTML parser, semantic layer, trust shield, WASM build, CI.

**Fas 2 (Complete)**: Intent API — `find_and_click`, `fill_form`, `extract_data`, workflow memory.

**Fas 3 (Complete)**: Runtime & integration — HTTP API server, Python + Node bindings, benchmarks, 20 real-site tests.

**Fas 4a (Complete)**: Semantic DOM Diffing — `diff_semantic_trees`, minimal delta between trees, 80–95% token savings.

**Fas 4b (Complete)**: JS Sandbox (QuickJS) — embedded QuickJS engine (via rquickjs) for evaluating inline scripts, event handlers, expressions. Sandboxed: no DOM/fetch/timers.

**Fas 4c (Complete)**: Selective Execution — `parse_with_js` pipeline: detect JS → extract DOM targets → evaluate in sandbox → apply to semantic tree. Handles `getElementById`/`querySelector` patterns.

**Fas 5 (Complete)**: Temporal Memory & Adversarial Modeling — time-series page state tracking, node volatility, adversarial pattern detection (escalating/gradual injection, suspicious volatility, structural manipulation), predictive state estimation.

**Fas 6 (Complete)**: Intent Compiler — goal decomposition via keyword-matched templates, topological sort with parallel group detection, action plan with sub-goals/dependencies, plan execution with recommended next action, prefetch suggestions.

**Fas 7 (Complete)**: HTTP Fetch Integration — reqwest-based page fetching with cookie jar, redirect following, gzip/brotli decompression, robots.txt compliance, SSRF protection. Combined endpoints: `fetch_parse`, `fetch_click`, `fetch_extract`, `fetch_plan`.

**Fas 8 (Complete)**: Semantic Firewall & Ethical Engine & MCP Server — three-level goal-aware request filtering (L1: URL pattern blocklist, L2: MIME/extension filter, L3: semantic relevance scoring), Google's `robotstxt` crate for RFC 9309 compliance, `governor` per-domain rate limiter (GCRA), Retry-After handling, MCP server via `rmcp` crate exposing all tools to Claude/Cursor/VS Code. Modules: `firewall.rs`, `bin/mcp_server.rs`. Endpoints: `/api/firewall/classify`, `/api/firewall/classify-batch`. Binary: `aether-mcp` (stdio MCP).

**Fas 9a (Complete)**: Causal Action Graph — `causal.rs`, action-consequence modeling, `find_safest_path` with semantic goal matching.

**Fas 9b (Complete)**: WebMCP Discovery — `webmcp.rs`, detect and extract `navigator.modelContext.registerTool()`, `<script type="application/mcp+json">`, `window.__webmcp__`, `window.mcpTools` registrations from web pages.

**Fas 9c (Complete)**: Multimodal Grounding — `grounding.rs`, ground semantic nodes to visual coordinates.

**Fas 9d (Complete)**: Cross-Agent Semantic Diffing — `collab.rs`, shared diff stores for multi-agent collaboration.

**Fas 10 (Complete)**: XHR Network Interception — `intercept.rs`, capture fetch()/XHR calls from JS sandbox, firewall-filtered fetching, response normalization to semantic nodes, XHR response caching.

**Fas 11 (Complete)**: YOLOv8 Vision — `vision.rs`, built-in ONNX Runtime inference for UI element detection (buttons, inputs, links, icons, text, images, checkboxes, selects, headings). MCP tools: `parse_screenshot`, `vision_parse`, `fetch_vision`.

**Fas 12 (Complete)**: TieredBackend & BUG-6 Fix — intelligent dual-tier screenshot rendering: Tier 1 (Blitz, pure Rust, ~10-50ms) with automatic escalation to Tier 2 (CDP/Chrome, feature-gated `cdp`) for JS-heavy pages. XHR-driven tier selection via `TierHint` (SPA detection, chart library detection). BUG-6 fixed: `find_safest_path` now uses 3-level semantic goal matching (direct similarity, word-level matching, context-word mapping for domain-specific synonyms like kontakt↔telefon/email). `compile_goal` enhanced with domain-specific templates (kontakt, analysera, nyheter, navigera). Modules: `vision_backend.rs`, updates to `causal.rs`, `compiler.rs`, `intercept.rs`. MCP tools: `tiered_screenshot`, `tier_stats`. HTTP endpoints: `/api/tiered-screenshot`, `/api/tier-stats`.

**Fas 13 (Complete)**: Session Management — `session.rs`, persistent browser sessions with cookie jars, page history, form state, workflow context. 11 HTTP endpoints for session CRUD.

**Fas 14 (Complete)**: Workflow Orchestration — `orchestrator.rs`, multi-page workflow engine with auto-navigation, rollback/retry, step tracking. 8 HTTP endpoints.

**Fas 15 (Complete)**: Streaming Parse — `streaming.rs`, `StreamingParser` with early-stopping at `max_nodes`, depth limiting, relevance filtering. WASM API: `parse_streaming`.

**Fas 16 (Complete)**: Goal-Driven Adaptive DOM Streaming — `stream_state.rs` (StreamState, DecisionLayer, Directive enum), `stream_engine.rs` (StreamEngine with relevance-ranked chunked emission). LLM-directed branch expansion via directives: `expand(node_id)`, `stop`, `next_branch`, `lower_threshold(value)`. 95–99% token savings on real-world pages (10 noder av 372 på SVT-liknande sida). MCP tools: `stream_parse`, `stream_parse_directive`. HTTP endpoints: `/api/stream-parse`, `/api/fetch/stream-parse`, `/api/directive`. WASM API: `stream_parse_adaptive`, `stream_parse_with_directives`.

**Fas 17 (Complete)**: JS Hardening — Arena DOM (`arena_dom.rs`), DOM Bridge (`dom_bridge.rs`), SSR Hydration (`hydration.rs`: 10 ramverk inkl. devalue-parser för Nuxt 3+/SvelteKit, RSC Flight Protocol, Qwik QRL), Progressive Escalation (`escalation.rs`: Tier 0-4), allowlist-säkerhet i `js_eval.rs`, persistent QuickJS Context i `eval_js_batch`.

**Fas 18 (Complete)**: Event Loop — `event_loop.rs`: microtask-kö (Promise.then, queueMicrotask via QuickJS inbyggda job-kö), setTimeout/setInterval (begränsade: max 100 timers, max 5000ms delay, virtuell klocka), requestAnimationFrame/cancelAnimationFrame (simulerad 16ms tick), MutationObserver (kopplad till ArenaDom med observe/disconnect). Säkerhetsbegränsningar: max 1000 ticks, max 50ms väggklocka. Integrerat i `dom_bridge.rs` — alla eval-anrop dränerar event-loopen automatiskt.

## Roadmap (ej påbörjad)

<!-- Fas 19: Produktionshärdning — de flesta punkterna redan adresserade:
     - .unwrap()-audit: GJORD (alla non-test unwrap har fallbacks)
     - MCP felmeddelanden: GJORD (tool-specifika, engelska, med parametertips)
     - WASM <4MB: REDAN UPPFYLLT (1.8MB)
     - Parse-profilering: REDAN SNABB (0-30ms)
     - stream_engine 1000+ noder: REDAN TESTAD
     - Timeout fetch-kedjan: REDAN 10s default
     - SSRF-audit: REDAN SOLID (validate_url blockerar privata IP)
     Kvar att överväga:
     - Pen-testa trust shield med nya injektionsvektorer (encoding-evasion, polyglot)
     - Rate limiter-stresstester under hög last
-->

**Fas 19 (Planned)**: Utökad Webbförståelse
- CSS-medveten parsing: `display:none`/`visibility:hidden`/`opacity:0`/`aria-hidden` filtrering (IMPLEMENTERAD). Framtida: computed styles, flexbox/grid-semantik.
- iframe-hantering: rekursiv parsing av iframe-innehåll, trust-nivå per iframe-origin, sandboxad JS per frame.
<!-- Shadow DOM: Stöd för open shadow roots + semantisk sammanslagning. LÅG PRIO — sällsynt i scraping-kontext. -->

<!-- Fas 20: Agent-protokoll & Ekosystem — AVVAKTA
     - A2A (Google Agent-to-Agent): Protokollet mognar fortfarande. Implementera agent card + task lifecycle när spec stabiliseras.
     - MCP 2.0: Vi har redan Streamable HTTP. Uppgradera bara vid breaking changes. OAuth 2.1 kan bli relevant.
     - Plugin-system: Prematur abstraktion. Bygg inte innan det behövs. Dynamisk parser-laddning, custom trust-regler, webhooks.
-->
