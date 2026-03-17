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

**Fas 4b (Complete)**: JS Sandbox (Boa) — embedded Boa JS engine for evaluating inline scripts, event handlers, expressions. Sandboxed: no DOM/fetch/timers.

**Fas 4c (Complete)**: Selective Execution — `parse_with_js` pipeline: detect JS → extract DOM targets → evaluate in sandbox → apply to semantic tree. Handles `getElementById`/`querySelector` patterns.

**Fas 5 (Complete)**: Temporal Memory & Adversarial Modeling — time-series page state tracking, node volatility, adversarial pattern detection (escalating/gradual injection, suspicious volatility, structural manipulation), predictive state estimation.

**Fas 6 (Complete)**: Intent Compiler — goal decomposition via keyword-matched templates, topological sort with parallel group detection, action plan with sub-goals/dependencies, plan execution with recommended next action, prefetch suggestions.

**Fas 7 (Complete)**: HTTP Fetch Integration — reqwest-based page fetching with cookie jar, redirect following, gzip/brotli decompression, robots.txt compliance, SSRF protection. Combined endpoints: `fetch_parse`, `fetch_click`, `fetch_extract`, `fetch_plan`.

**Fas 8 (Complete)**: Semantic Firewall & Ethical Engine & MCP Server — three-level goal-aware request filtering (L1: URL pattern blocklist, L2: MIME/extension filter, L3: semantic relevance scoring), Google's `robotstxt` crate for RFC 9309 compliance, `governor` per-domain rate limiter (GCRA), Retry-After handling, MCP server via `rmcp` crate exposing all tools to Claude/Cursor/VS Code. Modules: `firewall.rs`, `bin/mcp_server.rs`. Endpoints: `/api/firewall/classify`, `/api/firewall/classify-batch`. Binary: `aether-mcp` (stdio MCP).
