# Features Reference

> Complete feature documentation for AetherAgent.
> For a summary, see the [main README](../README.md).

AetherAgent contains **28 Rust modules**, **62 WASM-exported functions**, **65 HTTP endpoints**, and **30 MCP tools**.

---

## 1. Semantic Perception

**Module:** `parser.rs`, `semantic.rs`, `arena_dom.rs`, `types.rs`

Parses HTML into a structured accessibility tree with roles, labels, states, and goal-relevance scores. Uses `html5ever` for spec-compliant parsing, converted to an **Arena DOM** (`slotmap`-based, cache-friendly, ~5-10x faster traversal than `RcDom`).

| Function | What it does |
|----------|-------------|
| `parse_to_semantic_tree` | Full semantic tree with goal-relevance scoring |
| `parse_top_nodes` | Top-N most relevant nodes (token-efficient) |
| `parse_top_nodes_hybrid` | Top-N via BM25 → HDC → Embedding pipeline |
| `parse_top_nodes_with_config` | Top-N with configurable Stage 3 reranker |
| `parse_with_js` | Parse with automatic JS detection and evaluation |

---

## 1b. Hybrid Scoring Pipeline

**Module:** `scoring/pipeline.rs`, `scoring/tfidf.rs`, `scoring/hdc.rs`, `scoring/embed_score.rs`, `scoring/colbert_reranker.rs`

Three-stage neuro-symbolic retrieval pipeline for goal-directed DOM node ranking:

| Stage | Method | Latency | What it does |
|-------|--------|---------|-------------|
| 1. BM25 | Lexical retrieval | ~0.1ms | Keyword-match candidates (300 → 50-300) |
| 2. HDC | 4096-bit structural pruning | ~0.5ms | Subtree-level relevance (300 → 20-80 survivors) |
| 3. Neural | Embedding scoring | ~1-4s | Semantic precision on survivors |

**Stage 3 Reranker options** (configurable via `PipelineConfig.stage3_reranker`):

| Reranker | Model | How it works | Latency | Top-1 quality |
|----------|-------|-------------|---------|---------------|
| `MiniLM` (default) | all-MiniLM-L6-v2 (384-dim, FP32) | Mean-pooled bi-encoder cosine similarity | ~1.2s | 0.675 |
| `ColBert` | all-MiniLM-L6-v2 (384-dim, int8) | MaxSim late interaction — per-token matching | **~0.4s** | **0.950** |
| `Hybrid` | Both | Adaptive α blend (0.3–0.95 by node length) | **~0.4s** | 0.817 |

ColBERT is **2.8× faster** than the bi-encoder and produces **41% higher confidence scores**. It consistently ranks information-bearing nodes (facts, data, tables) above headings and navigation. Optimized via int8 quantization, batch ONNX encoding, reduced survivor cap (25-35), u8 MaxSim, and score caching.

Feature flags: `embeddings` (MiniLM), `colbert` (ColBERT + Hybrid, depends on `embeddings`).
Runtime: set `AETHER_COLBERT_MODEL` + `AETHER_COLBERT_VOCAB` env vars, or ColBERT falls back to the bi-encoder model in late-interaction mode.

---

## 2. Trust Shield — Prompt Injection Protection

**Module:** `trust.rs`

All web content is `TrustLevel::Untrusted` at the type level. 20+ injection patterns scanned at parse time (English + Swedish), including zero-width character attacks. Content wrapped in boundary markers before LLM delivery.

| Function | What it does |
|----------|-------------|
| `check_injection` | Scan text for prompt injection patterns |
| `wrap_untrusted` | Wrap content in `<UNTRUSTED_WEB_CONTENT>` markers |

**Detection patterns:** "ignore previous instructions", "you are now", "system prompt", invisible Unicode (zero-width space, joiners), role-play triggers, instruction override attempts.

---

## 3. Intent API

**Module:** `intent.rs`

Goal-oriented actions instead of raw coordinate clicks:

| Function | What it does |
|----------|-------------|
| `find_and_click` | Find best clickable element matching a label |
| `fill_form` | Map form fields to key/value pairs with selector hints |
| `extract_data` | Extract structured data by semantic keys (price, title, etc.) |

---

## 4. Semantic DOM Diffing

**Module:** `diff.rs`

Computes minimal deltas between two semantic trees. 70–99% token savings for multi-step agent flows.

| Function | What it does |
|----------|-------------|
| `diff_semantic_trees` | Compare two trees, return only changes |

```json
{
  "changes": [{"node_id": 42, "change_type": "Modified", "label": "1 i varukorg"}],
  "token_savings_ratio": 0.87,
  "summary": "2 changes (1 modified, 1 added), 87% token savings"
}
```

---

## 5. JavaScript Sandbox + DOM Bridge

**Module:** `js_eval.rs`, `js_bridge.rs`, `dom_bridge.rs`

Embedded **QuickJS** JS engine (via `rquickjs` 0.11) for safe snippet evaluation. Two modes:

1. **Expression sandbox** (`eval_js`) — no DOM, evaluates pure expressions
2. **DOM bridge** (`eval_js_with_dom`) — exposes `document`/`window` to QuickJS via Arena DOM handles

| Function | What it does |
|----------|-------------|
| `detect_js` | Scan HTML for scripts, handlers, framework markers |
| `eval_js` | Evaluate single JS expression in sandbox |
| `eval_js_batch` | Evaluate multiple expressions |
| `eval_js_with_dom` | Evaluate JS with full DOM API |

**DOM bridge methods:** `getElementById`, `querySelector`, `querySelectorAll`, `createElement`, `createTextNode`, `document.body/head/documentElement`, `window.innerWidth/innerHeight/location/navigator`, `console.log/warn/error`.

**Security model:** Allowlist-based — only known safe operations permitted. Deny-list catches 18 explicitly dangerous patterns.

**Event loop:** Full event loop with microtask queue (Promise.then, queueMicrotask), setTimeout/setInterval, requestAnimationFrame, MutationObserver. Safety: max 5000 ticks, 500ms wall time.

> Full DOM API reference: **[docs/dom-api-coverage.md](dom-api-coverage.md)**

---

## 6. Temporal Memory & Adversarial Modeling

**Module:** `temporal.rs`, `memory.rs`

Tracks page state across multiple visits. Detects adversarial patterns that single-snapshot analysis misses.

| Function | What it does |
|----------|-------------|
| `create_workflow_memory` | Create stateless workflow memory |
| `add_workflow_step` | Record action in memory |
| `create_temporal_memory` | Create time-series memory |
| `add_temporal_snapshot` | Record page state at timestamp |
| `analyze_temporal` | Detect adversarial patterns (risk score 0.0–1.0) |
| `predict_temporal` | Predict next page state |

**Adversarial detection types:** `EscalatingInjection`, `GradualInjection`, `SuspiciousVolatility`, `StructuralManipulation`.

---

## 7. Intent Compiler

**Module:** `compiler.rs`

Compiles complex goals into optimized action plans with dependency tracking and parallel execution groups.

| Function | What it does |
|----------|-------------|
| `compile_goal` | Decompose goal into sub-steps with dependencies |
| `execute_plan` | Execute plan against current page state |

**Supported templates:** `buy/purchase`, `login/sign in`, `search/find`, `register/sign up`, `extract/scrape`.

---

## 8. HTTP Fetch Integration

**Module:** `fetch.rs`

Built-in page fetching with cookie jar, redirect following, gzip/brotli decompression, robots.txt compliance, and SSRF protection.

| Function | What it does |
|----------|-------------|
| `fetch` | Fetch URL → HTML + metadata |
| `fetch/parse` | Fetch → semantic tree (one call) |
| `fetch/click` | Fetch → find clickable element |
| `fetch/extract` | Fetch → extract structured data |
| `fetch/plan` | Fetch → compile goal → execute plan |

**Security:** Blocks `localhost`, `127.0.0.1`, private IP ranges, non-HTTP schemes.

---

## 9. Semantic Firewall

**Module:** `firewall.rs`

Three-level goal-aware request filter:

| Level | What it checks | Speed |
|-------|---------------|-------|
| **L1** | URL pattern — 45+ tracking domains | <1 us |
| **L2** | File extension/MIME — non-semantic resources | <1 us |
| **L3** | Semantic relevance — scores URL against goal | ~1 ms |

---

## 10. Causal Action Graph

**Module:** `causal.rs`

Models page state transitions as a directed graph for reasoning about action consequences.

| Function | What it does |
|----------|-------------|
| `build_causal_graph` | Build graph from temporal history |
| `predict_action_outcome` | Predict probability, risk, and expected state changes |
| `find_safest_path` | BFS with risk-weighting to goal state |

---

## 11. WebMCP Discovery

**Module:** `webmcp.rs`

Detects W3C-incubated WebMCP tool registrations in HTML pages.

---

## 12. Multimodal Grounding

**Module:** `grounding.rs`

Combines semantic tree with bounding box coordinates from vision models. Enables Set-of-Mark integration.

---

## 13. Cross-Agent Collaboration

**Module:** `collab.rs`

Shared diff store for multiple agents working on the same site.

---

## 14. XHR Network Interception

**Module:** `intercept.rs`

Scans inline scripts for `fetch()`, `XMLHttpRequest.open()`, `$.ajax()`, `$.get()`, `$.post()` calls. Discovers hidden API endpoints.

---

## 15. Session Management

**Module:** `session.rs`

Persistent session cookies, OAuth 2.0 flow handling, and login form detection. All state is serializable JSON.

---

## 16. Multi-page Workflow Orchestration

**Module:** `orchestrator.rs`

Stateful workflow engine: auto-navigation, rollback/retry, cross-page temporal memory, session integration.

---

## 17. Vision — YOLOv8 + Blitz Rendering

**Module:** `vision.rs`

Embedded YOLOv8-nano via `rten` (pure Rust ONNX runtime). Renders pages to PNG using Blitz (pure Rust, no Chrome).

| Mode | `fast_render` | Latency |
|------|:---:|---|
| **Fast** (default) | `true` | ~50ms |
| **Full** | `false` | ≤2s |

**Detected UI classes:** button, input, link, icon, text, image, checkbox, radio, select, heading.

---

## 18. SSR Hydration Extraction (Tier 0)

**Module:** `hydration.rs`

Extracts server-side rendered data without running JavaScript. Supports 10 frameworks:
Next.js (Pages + App Router), Nuxt 2/3, Angular Universal, Remix, Gatsby, SvelteKit, Qwik, Astro, Apollo GraphQL.

---

## 19. Arena DOM

**Module:** `arena_dom.rs`

SlotMap-based DOM — one allocation per page, ~5-10x faster traversal, generational index safety.

---

## 20. Progressive Escalation

**Module:** `escalation.rs`

| Tier | Strategy | When selected | Latency |
|------|----------|---------------|---------|
| 0 | Hydration extraction | SSR framework data found | ~0 ms |
| 1 | Static HTML parse | No JS detected | ~1 ms |
| 2 | QuickJS + DOM sandbox | Inline scripts with DOM access | ~10-50 ms |
| 3 | Blitz render | CSS layout needed | ~10-50 ms |
| 4 | Chrome CDP | Heavy JS (SPA shell) | ~500-2000 ms |

---

## 21. Goal-Driven Adaptive DOM Streaming

**Module:** `stream_state.rs`, `stream_engine.rs`

LLM-directed branch expansion via directives: `expand(node_id)`, `stop`, `next_branch`, `lower_threshold(value)`. 95–99% token savings on real-world pages.
