<p align="center">
  <img src="image/AetherAgent.png" alt="AetherAgent" width="400" />
</p>

<h1 align="center">AetherAgent</h1>

<p align="center">
  <strong>The LLM-native browser engine.</strong><br>
  Semantic perception, goal-aware intelligence, and prompt injection protection — in a single embeddable Rust/WASM library.
</p>

<p align="center">
  <a href="https://github.com/robinandreeklund-collab/AetherAgent/actions/workflows/ci.yml"><img src="https://github.com/robinandreeklund-collab/AetherAgent/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2021_edition-orange.svg" alt="Rust"></a>
  <a href="https://webassembly.org"><img src="https://img.shields.io/badge/target-wasm32--unknown--unknown-blue.svg" alt="WASM"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="License: MIT"></a>
</p>

<p align="center">
  <a href="https://render.com/deploy?repo=https://github.com/robinandreeklund-collab/AetherAgent"><img src="https://render.com/images/deploy-to-render-button.svg" alt="Deploy to Render"></a>
</p>

---

## The Problem

Every AI browser agent today faces the same trade-off:

```
                        S M A R T
                  (semantic understanding,
                   goal-relevance, security)
                           │
                           │
        Smart + Slow       │       Smart + Fast
        ─────────────      │       ────────────
        Browser Use        │
        Stagehand          │        AetherAgent ◆
        SeeAct             │
        AgentQL            │
                           │
    ───────────────────────┼───────────────────────── F A S T
                           │                    (no browser overhead,
                           │                     sub-ms startup)
        Dumb + Slow        │       Dumb + Fast
        ─────────────      │       ────────────
        Playwright         │       BeautifulSoup
        Puppeteer          │       Cheerio
        Selenium           │       Scrapy
                           │       regex
                           │
```

**The upper-right quadrant is empty.** Every tool is either:

- **Fast but dumb** — regex scrapers, CSS selectors, no understanding of what the page *means*
- **Smart but slow** — headless browsers + LLM calls, 1-3 second startup, 150+ MB RAM per instance

AetherAgent is the first engine built to occupy that empty quadrant: **fast *and* smart**.

No headless browser. No Chrome process. No V8. Just Rust compiled to WebAssembly — fetching pages, parsing HTML into semantic accessibility trees with goal-relevance scoring, prompt injection protection, and intent-aware actions — in under 1 millisecond startup and ~12 MB RAM. Built-in HTTP fetch with cookies, redirects, robots.txt compliance, and SSRF protection means AetherAgent works end-to-end: URL in, semantic tree out.

### Honest Positioning

AetherAgent is **not** a Chrome replacement. It fetches pages and builds semantic trees, but does not execute full JavaScript runtimes (V8) or render CSS. For JS-heavy SPAs, pair it with a headless browser for rendering, then feed the HTML to AetherAgent. For static/SSR pages (~40% of the web, and the entire data extraction niche), AetherAgent works fully standalone: URL in, semantic tree out.

| Capability | AetherAgent | Playwright | Browser Use | Scrapy |
|-----------|:-----------:|:----------:|:-----------:|:------:|
| Semantic tree with goal scoring | **Yes** | No | Partial | No |
| Prompt injection protection | **Yes** | No | No | No |
| Startup time | <1 ms | ~2,000 ms | ~3,000 ms | ~50 ms |
| Memory per instance | ~9.5 MB | ~150 MB | ~200 MB | ~30 MB |
| Full JavaScript (V8) | No | Yes | Yes | No |
| CSS rendering | No | Yes | Yes | No |
| Embeddable in WASM | **Yes** | No | No | No |
| Semantic diff (token savings) | **Yes** | No | No | No |
| MCP server built-in | **Yes** | No | No | No |
| License | MIT | Apache-2.0 | MIT | BSD |

**When to use AetherAgent:** Your agent needs a fast, end-to-end browser engine — fetch pages, build semantic trees, plan actions, and detect injection — with no browser overhead. Works standalone for static/SSR pages (~40% of the web), or as a perception layer on top of browser-rendered HTML.

**When to use Playwright/Browser Use:** You need full JavaScript execution: SPAs, visual rendering, or CDP automation.

**Best of both worlds:** For JS-heavy SPAs, fetch with a browser, perceive with AetherAgent.

---

## What AetherAgent Does

Instead of handing your LLM 50,000 tokens of raw HTML, AetherAgent delivers ~200 tokens of semantic JSON — the page already understood, filtered, and ranked by relevance to the agent's current goal.

```python
# Without AetherAgent — raw HTML
html = requests.get(url).text
llm.send(html)  # 50,000 tokens, slow, expensive, no structure

# With AetherAgent — one call: fetch + semantic parse
tree = agent.fetch_parse(url, goal="buy cheapest flight")
llm.send(tree)  # 200 tokens, goal-aware, injection-protected
```

```json
{
  "goal": "add to cart",
  "nodes": [
    {
      "id": 42, "role": "button",
      "label": "Add to cart – 199 kr",
      "action": "click", "relevance": 0.97,
      "trust": "Untrusted"
    }
  ],
  "injection_warnings": [],
  "parse_time_ms": 1
}
```

---

## Features

AetherAgent contains **18 Rust modules**, **35 WASM-exported functions**, **41 HTTP endpoints**, and **20 MCP tools**. Here is every feature, grouped by capability.

### 1. Semantic Perception

**Module:** `parser.rs`, `semantic.rs`, `types.rs`

Parses HTML into a structured accessibility tree with roles, labels, states, and goal-relevance scores. Uses `html5ever` + `rcdom` for spec-compliant parsing.

| Function | What it does |
|----------|-------------|
| `parse_to_semantic_tree` | Full semantic tree with goal-relevance scoring |
| `parse_top_nodes` | Top-N most relevant nodes (token-efficient) |
| `parse_with_js` | Parse with automatic JS detection and evaluation |

### 2. Trust Shield — Prompt Injection Protection

**Module:** `trust.rs`

All web content is `TrustLevel::Untrusted` at the type level. 20+ injection patterns scanned at parse time (English + Swedish), including zero-width character attacks. Content wrapped in boundary markers before LLM delivery.

| Function | What it does |
|----------|-------------|
| `check_injection` | Scan text for prompt injection patterns |
| `wrap_untrusted` | Wrap content in `<UNTRUSTED_WEB_CONTENT>` markers |

**Detection patterns:** "ignore previous instructions", "you are now", "system prompt", invisible Unicode (zero-width space, joiners), role-play triggers, instruction override attempts.

### 3. Intent API

**Module:** `intent.rs`

Goal-oriented actions instead of raw coordinate clicks:

| Function | What it does |
|----------|-------------|
| `find_and_click` | Find best clickable element matching a label |
| `fill_form` | Map form fields to key/value pairs with selector hints |
| `extract_data` | Extract structured data by semantic keys (price, title, etc.) |

### 4. Semantic DOM Diffing

**Module:** `diff.rs`

Computes minimal deltas between two semantic trees. 80–95% token savings for multi-step agent flows.

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

### 5. JavaScript Sandbox

**Module:** `js_eval.rs`, `js_bridge.rs`

Embedded **Boa** JS engine (pure Rust, no C deps) for safe snippet evaluation. Sandboxed: no DOM, no fetch, no timers, no module system. Combined with selective execution that detects JS-dependent content and evaluates only relevant expressions.

| Function | What it does |
|----------|-------------|
| `detect_js` | Scan HTML for scripts, handlers, framework markers |
| `eval_js` | Evaluate single JS expression in sandbox |
| `eval_js_batch` | Evaluate multiple expressions |

**Selective execution pipeline:** Detect JS → extract `getElementById`/`querySelector` patterns → match to tree nodes → evaluate in sandbox → apply computed values back to semantic tree.

### 6. Temporal Memory & Adversarial Modeling

**Module:** `temporal.rs`, `memory.rs`

Tracks page state across multiple visits. Detects adversarial patterns that single-snapshot analysis misses.

| Function | What it does |
|----------|-------------|
| `create_workflow_memory` | Create stateless workflow memory |
| `add_workflow_step` | Record action in memory |
| `set_workflow_context` / `get_workflow_context` | Key-value context store |
| `create_temporal_memory` | Create time-series memory |
| `add_temporal_snapshot` | Record page state at timestamp |
| `analyze_temporal` | Detect adversarial patterns (risk score 0.0–1.0) |
| `predict_temporal` | Predict next page state |

**Adversarial detection types:**
- `EscalatingInjection` — injection warnings increase monotonically
- `GradualInjection` — clean nodes gradually acquire injection patterns
- `SuspiciousVolatility` — text nodes change too frequently (>70%)
- `StructuralManipulation` — >50% of nodes change in a single step

### 7. Intent Compiler

**Module:** `compiler.rs`

Compiles complex goals into optimized action plans with dependency tracking and parallel execution groups.

| Function | What it does |
|----------|-------------|
| `compile_goal` | Decompose goal into sub-steps with dependencies |
| `execute_plan` | Execute plan against current page state |

**Supported templates:** `buy/purchase`, `login/sign in`, `search/find`, `register/sign up`, `extract/scrape`. Unknown goals get a generic Navigate → Act → Verify plan.

### 8. HTTP Fetch Integration

**Module:** `fetch.rs`

Built-in page fetching with cookie jar, redirect following, gzip/brotli decompression, robots.txt compliance, and SSRF protection.

| Function | What it does |
|----------|-------------|
| `fetch` | Fetch URL → HTML + metadata |
| `fetch/parse` | Fetch → semantic tree (one call) |
| `fetch/click` | Fetch → find clickable element |
| `fetch/extract` | Fetch → extract structured data |
| `fetch/plan` | Fetch → compile goal → execute plan |

**Security:** Blocks `localhost`, `127.0.0.1`, private IP ranges, non-HTTP schemes. Optional robots.txt compliance. Configurable timeouts and redirect limits.

### 9. Semantic Firewall

**Module:** `firewall.rs`

Three-level goal-aware request filter. Blocks irrelevant subrequests before they waste bandwidth and tokens.

| Level | What it checks | Speed | Example |
|-------|---------------|-------|---------|
| **L1** | URL pattern — 45+ tracking domains | <1 us | Google Analytics, Facebook Pixel, Hotjar |
| **L2** | File extension/MIME — non-semantic resources | <1 us | `.jpg`, `.woff2`, `.mp4`, `.pdf` |
| **L3** | Semantic relevance — scores URL against goal | ~1 ms | Is this URL relevant to "buy product"? |

| Function | What it does |
|----------|-------------|
| `classify_request` | Classify single URL against firewall |
| `classify_request_batch` | Classify batch of URLs |

**Ethical engine:** robots.txt compliance (RFC 9309 via Google's `robotstxt` crate), per-domain rate limiting (GCRA via `governor`), Retry-After header support.

### 10. Causal Action Graph

**Module:** `causal.rs`

Models page state transitions as a directed graph. Enables agents to reason about action consequences before executing them.

| Function | What it does |
|----------|-------------|
| `build_causal_graph` | Build graph from temporal history |
| `predict_action_outcome` | Predict probability, risk, and expected state changes |
| `find_safest_path` | BFS with risk-weighting to goal state |

### 11. WebMCP Discovery

**Module:** `webmcp.rs`

Detects W3C-incubated WebMCP tool registrations (`navigator.modelContext.registerTool()`) in HTML pages. Extracts tool names, descriptions, and JSON Schema input definitions.

| Function | What it does |
|----------|-------------|
| `discover_webmcp` | Scan HTML for WebMCP tool registrations |

### 12. Multimodal Grounding

**Module:** `grounding.rs`

Combines semantic tree with bounding box coordinates from vision models or `getBoundingClientRect()`. Enables Set-of-Mark integration.

| Function | What it does |
|----------|-------------|
| `ground_semantic_tree` | Match bounding boxes to tree nodes |
| `match_bbox_iou` | IoU (Intersection over Union) matching |

**Matching:** Exact match via `html_id`, fuzzy match via `role` + `label`. Generates numbered Set-of-Mark annotations for interactive elements.

### 13. Cross-Agent Collaboration

**Module:** `collab.rs`

Shared diff store for multiple agents working on the same site. Reduces redundant parsing and token cost.

| Function | What it does |
|----------|-------------|
| `create_collab_store` | Create empty shared store |
| `register_collab_agent` | Register agent with goal |
| `publish_collab_delta` | Publish semantic delta for others |
| `fetch_collab_deltas` | Fetch new deltas (excludes own + already consumed) |
| `collab_stats` | Active agents, cached deltas, publish/consume counts |
| `cleanup_collab_store` | Remove inactive agents |

---

## API Reference

### HTTP Endpoints (41 routes)

Run the server: `cargo run --features server --bin aether-server`

#### Core Parsing

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | API documentation (JSON) |
| GET | `/health` | Health check |
| POST | `/api/parse` | Parse HTML → full semantic tree |
| POST | `/api/parse-top` | Parse → top-N relevant nodes |
| POST | `/api/parse-js` | Parse with automatic JS evaluation |

#### Trust & Security

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/check-injection` | Check text for prompt injection |
| POST | `/api/wrap-untrusted` | Wrap content in trust markers |

#### Intent API

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/click` | Find best clickable element |
| POST | `/api/fill-form` | Map form fields |
| POST | `/api/extract` | Extract structured data by keys |

#### Semantic Diff

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/diff` | Compute delta between two trees |

#### JavaScript Sandbox

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/detect-js` | Detect JS snippets in HTML |
| POST | `/api/eval-js` | Evaluate expression in sandbox |
| POST | `/api/eval-js-batch` | Batch evaluate expressions |

#### Workflow & Temporal Memory

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/memory/create` | Create workflow memory |
| POST | `/api/memory/step` | Add workflow step |
| POST | `/api/memory/context/set` | Set context key/value |
| POST | `/api/memory/context/get` | Get context value |
| POST | `/api/temporal/create` | Create temporal memory |
| POST | `/api/temporal/snapshot` | Add temporal snapshot |
| POST | `/api/temporal/analyze` | Analyze adversarial patterns |
| POST | `/api/temporal/predict` | Predict next page state |

#### Intent Compiler

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/compile` | Compile goal → action plan |
| POST | `/api/execute-plan` | Execute plan against page state |

#### HTTP Fetch

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/fetch` | Fetch URL → HTML + metadata |
| POST | `/api/fetch/parse` | Fetch → semantic tree |
| POST | `/api/fetch/click` | Fetch → find clickable element |
| POST | `/api/fetch/extract` | Fetch → extract structured data |
| POST | `/api/fetch/plan` | Fetch → compile → execute plan |

#### Semantic Firewall

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/firewall/classify` | Classify URL (L1/L2/L3) |
| POST | `/api/firewall/classify-batch` | Batch classify URLs |

#### Causal Action Graph

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/causal/build` | Build causal graph from history |
| POST | `/api/causal/predict` | Predict action outcome |
| POST | `/api/causal/safest-path` | Find safest path to goal |

#### WebMCP Discovery

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/webmcp/discover` | Discover WebMCP tools in HTML |

#### Multimodal Grounding

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/ground` | Ground tree with bounding boxes |
| POST | `/api/ground/match-bbox` | Match bbox via IoU |

#### Cross-Agent Collaboration

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/collab/create` | Create shared diff store |
| POST | `/api/collab/register` | Register agent |
| POST | `/api/collab/publish` | Publish delta |
| POST | `/api/collab/fetch` | Fetch new deltas |

### MCP Server (20 tools)

Run: `cargo run --features mcp --bin aether-mcp`

Compatible with Claude Desktop, Cursor, VS Code, and any MCP-compatible client.

| Tool | Description |
|------|-------------|
| `parse` | Parse HTML to semantic tree |
| `parse_top` | Top-N most relevant nodes |
| `parse_with_js` | Parse with JS evaluation |
| `find_and_click` | Find clickable element |
| `fill_form` | Map form fields |
| `extract_data` | Extract structured data |
| `check_injection` | Check for prompt injection |
| `compile_goal` | Compile goal to action plan |
| `classify_request` | Classify URL against firewall |
| `diff_trees` | Semantic diff (80-95% token savings) |
| `build_causal_graph` | Build causal action graph |
| `predict_action_outcome` | Predict action consequences |
| `find_safest_path` | Safest path to goal state |
| `discover_webmcp` | Discover WebMCP tools |
| `ground_semantic_tree` | Ground tree with bounding boxes |
| `match_bbox_iou` | Match bbox via IoU |
| `create_collab_store` | Create collaboration store |
| `register_collab_agent` | Register agent for collaboration |
| `publish_collab_delta` | Publish semantic delta |
| `fetch_collab_deltas` | Fetch new deltas |

**Claude Desktop configuration:**
```json
{
  "mcpServers": {
    "aether-agent": {
      "command": "/path/to/target/release/aether-mcp"
    }
  }
}
```

### Python SDK

```python
from bindings.python.aether_agent import AetherAgent

agent = AetherAgent(base_url="https://your-app.onrender.com")
tree = agent.parse(html, goal="buy cheapest flight", url="https://flights.com")
click = agent.find_and_click(html, goal="buy", url=url, target_label="Add to cart")
data = agent.extract_data(html, goal="get price", url=url, keys=["price", "title"])
```

### Node.js SDK

```javascript
const { AetherAgent } = require('./bindings/node');
const agent = new AetherAgent('https://your-app.onrender.com');

const tree = await agent.parse(html, 'buy cheapest flight', url);
const click = await agent.findAndClick(html, 'buy', url, 'Add to cart');
```

---

## Tests

**232 tests** across 4 levels. All must pass on every commit.

```bash
cargo test              # Run all 232 tests
cargo clippy -- -D warnings  # Zero warnings required
cargo fmt --check       # Zero diffs required
```

### Unit Tests (162 tests)

Every module has tests at the bottom of the source file:

| Module | Tests | Coverage |
|--------|------:|----------|
| `lib.rs` | 41 | All 35 WASM bindings + smoke tests |
| `js_eval.rs` | 10 | Detection, evaluation, safety blocking |
| `compiler.rs` | 9 | Goal compilation, plan execution, serialization |
| `causal.rs` | 13 | Graph building, prediction, safest path, serialization |
| `collab.rs` | 10 | Store operations, agent registration, versioning |
| `diff.rs` | 9 | Tree comparison, change detection, token savings |
| `firewall.rs` | 16 | L1/L2/L3 filtering, batch, MIME types, whitelisting |
| `intent.rs` | 11 | Click, fill_form, extract_data edge cases |
| `js_bridge.rs` | 7 | Selective execution, DOM targeting, variable extraction |
| `temporal.rs` | 7 | Memory, adversarial detection, prediction, volatility |
| `grounding.rs` | 9 | Tree grounding, IoU computation, Set-of-Marks |
| `webmcp.rs` | 8 | Tool discovery, schema extraction, polyfill detection |
| `trust.rs` | 4 | Injection detection, zero-width chars, boundary wrapping |
| `memory.rs` | 4 | Serialization, context operations, invalid JSON |
| `parser.rs` | 2 | HTML parsing, aria-label priority |
| `semantic.rs` | 2 | Relevance scoring, injection detection |

### Fixture Tests (30 tests)

20 realistic HTML test fixtures covering real-world scenarios:

| Fixture | Scenario | Tests |
|---------|----------|------:|
| 01 | E-commerce product page | 3 (parse, click, extract) |
| 02 | Login form | 2 (click, fill) |
| 03 | Search results | 2 (click cheapest, extract prices) |
| 04 | Registration form | 1 (fill) |
| 05 | Checkout flow | 2 (click, fill) |
| 06 | News article | 1 (extract) |
| 07 | Flight booking | 1 (book cheapest) |
| 08 | Restaurant menu | 2 (book table, extract prices) |
| 09 | Dashboard | 1 (click export) |
| 10 | Hidden injection attack | 1 (detected) |
| 11 | Social engineering injection | 1 (detected) |
| 12 | Banking transfer | 1 (fill) |
| 13 | Real estate listing | 2 (extract price, book viewing) |
| 14 | Job listing | 1 (apply) |
| 15 | Grocery store | 1 (add item) |
| 16 | Settings page | 1 (save) |
| 17 | Wiki article | 1 (extract info) |
| 18 | Social media | 1 (like post) |
| 19 | Contact form | 1 (fill) |
| 20 | Large catalog (performance) | 2 (parse, top-N) |
| — | Injection pattern library | 2 (safe + dangerous texts) |

### Integration Tests (40 tests)

End-to-end tests exercising the full pipeline (HTML → parse → tree → JSON):

- E-commerce scenarios (parse, click, extract, fill, performance)
- Form scenarios (login, registration)
- Injection scenarios (detection, mixed content)
- Semantic diff scenarios (identical, added, removed, label change, state change, token savings, performance)
- JS sandbox scenarios (detect, eval, batch, blocked operations)
- Selective execution (static, DOM targets, framework detection)
- Temporal memory (ecommerce flow, adversarial escalation, prediction, safe pages)
- Intent compiler (buy, login, search, plan execution, ecommerce flow)
- Workflow memory (end-to-end with context)

### Benchmarks (13 scenarios)

```bash
cargo bench
```

| Benchmark | Target |
|-----------|--------|
| Parse: simple page (3 elements) | <50 ms |
| Parse: ecommerce (12 elements) | <50 ms |
| Parse: login form | <50 ms |
| Parse: complex (100 products) | <500 ms |
| Parse: injection content | <50 ms |
| Top-N: 5, 10, 20 nodes | <50 ms |
| Click: find button | <50 ms |
| Fill: login form | <50 ms |
| Extract: product price | <50 ms |
| Injection check: safe text | <1 ms |
| Injection check: malicious text | <1 ms |

---

## Architecture

```
┌───────────────────────────────────────────────────────────────────┐
│               LLM Agent (Claude / GPT / Llama / Gemini)           │
│            Receives semantic JSON → reasons → acts                │
└──────────────────────────────┬────────────────────────────────────┘
                               │ goal-aware JSON (200 tokens)
┌──────────────────────────────▼────────────────────────────────────┐
│                    AetherAgent Core (Rust → WASM)                 │
│                                                                   │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │ Parser   │ │ Semantic  │ │  Trust   │ │   Intent API     │   │
│  │ html5ever│ │ A11y tree │ │  Shield  │ │ click/fill/      │   │
│  │ rcdom    │ │ goal      │ │ 20+      │ │ extract          │   │
│  │          │ │ scoring   │ │ patterns │ │                  │   │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │ Diff     │ │ JS        │ │ Temporal │ │   Compiler       │   │
│  │ 80-95%   │ │ Sandbox   │ │ Memory & │ │ goal → plan →    │   │
│  │ token    │ │ Boa       │ │ Adversar.│ │ execute          │   │
│  │ savings  │ │ engine    │ │ Detection│ │                  │   │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │ Fetch    │ │ Firewall  │ │ Causal   │ │   Grounding      │   │
│  │ HTTP     │ │ L1/L2/L3  │ │ Action   │ │ BBox + IoU +     │   │
│  │ cookies  │ │ goal-aware│ │ Graph    │ │ Set-of-Mark      │   │
│  │ SSRF prot│ │ filtering │ │          │ │                  │   │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────┐ ┌───────────┐                                     │
│  │ WebMCP   │ │ Collab    │    18 modules · 35 WASM functions   │
│  │ Discovery│ │ Cross-    │    41 HTTP endpoints · 20 MCP tools │
│  │          │ │ Agent     │                                     │
│  └──────────┘ └───────────┘                                     │
└──────────────────────────────┬────────────────────────────────────┘
                               │
┌──────────────────────────────▼────────────────────────────────────┐
│                   Runtime (zero vendor lock-in)                   │
│  WASM (any host)  │  HTTP API (Axum)  │  MCP (stdio)  │  Python  │
│  Node.js          │  Cloudflare Workers│  Claude Desktop│  SDK    │
└───────────────────────────────────────────────────────────────────┘
```

### Project Structure

```
AetherAgent/
├── src/
│   ├── lib.rs            # WASM API surface — 35 public functions
│   ├── parser.rs         # html5ever + rcdom DOM builder
│   ├── semantic.rs       # Accessibility tree, goal-relevance scoring
│   ├── trust.rs          # Prompt injection detection (20+ patterns)
│   ├── intent.rs         # find_and_click, fill_form, extract_data
│   ├── diff.rs           # Semantic DOM diffing, delta computation
│   ├── js_eval.rs        # Boa JS sandbox, detection, evaluation
│   ├── js_bridge.rs      # Selective execution, DOM targeting
│   ├── temporal.rs       # Time-series memory, adversarial detection
│   ├── compiler.rs       # Intent compiler, goal decomposition
│   ├── fetch.rs          # HTTP fetching, SSRF, robots.txt, rate limiting
│   ├── firewall.rs       # L1/L2/L3 semantic firewall
│   ├── causal.rs         # Causal action graph, outcome prediction
│   ├── webmcp.rs         # WebMCP tool discovery
│   ├── grounding.rs      # Multimodal grounding, IoU matching
│   ├── collab.rs         # Cross-agent semantic diff store
│   ├── memory.rs         # Workflow memory persistence
│   ├── types.rs          # Core data structures
│   └── bin/
│       ├── server.rs     # Axum HTTP API (41 endpoints)
│       └── mcp_server.rs # MCP server (20 tools, stdio transport)
├── tests/
│   ├── integration_test.rs   # 40 end-to-end tests
│   ├── fixture_tests.rs      # 30 fixture-based scenario tests
│   └── fixtures/             # 20 realistic HTML test pages
├── benches/
│   └── bench_main.rs         # 13 performance benchmarks
├── bindings/
│   ├── python/               # Python SDK
│   └── node/                 # Node.js SDK + TypeScript types
├── examples/
│   └── python_test.py        # Complete agent loop demo
├── .github/workflows/
│   └── ci.yml                # CI: test, WASM build, security audit
├── Dockerfile                # Multi-stage Docker build
├── render.yaml               # One-click Render deployment
├── Cargo.toml
└── LICENSE                   # MIT
```

---

## Quick Start

### Build & Test

```bash
# Prerequisites
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

# Clone and build
git clone https://github.com/robinandreeklund-collab/AetherAgent.git
cd AetherAgent

# Run all tests (must pass before any commit)
cargo test && cargo clippy -- -D warnings && cargo fmt --check

# Build WASM binary
wasm-pack build --target web --release

# Run HTTP server
cargo run --features server --bin aether-server

# Run MCP server
cargo run --features mcp --bin aether-mcp
```

### Deploy to Render

[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/robinandreeklund-collab/AetherAgent)

```bash
# After deploy:
curl https://your-app.onrender.com/health

curl -X POST https://your-app.onrender.com/api/parse \
  -H "Content-Type: application/json" \
  -d '{"html": "<button>Buy now</button>", "goal": "buy product", "url": "https://shop.com"}'
```

### Example: Full Agent Loop (Python)

```python
from bindings.python.aether_agent import AetherAgent

agent = AetherAgent(base_url="https://your-app.onrender.com")

# 1. Compile goal into action plan
plan = agent.compile_goal("buy cheapest flight to Paris")

# 2. Fetch and parse the page
result = agent.fetch_parse("https://flights.example.com", goal="buy cheapest flight")

# 3. Find the "Book" button
click = agent.find_and_click(result["tree"], goal="buy", target_label="Book now")

# 4. Fill the booking form
form = agent.fill_form(html, goal="book flight", fields={
    "name": "Robin Eklund", "email": "robin@example.com"
})

# 5. Check for injection in page content
safety = agent.check_injection(page_text)
```

---

## Development

### Current Status

AetherAgent is a fully functional AI browser engine with:

- **18 Rust source modules** — parser, semantic, trust, intent, diff, JS sandbox, selective execution, temporal memory, adversarial modeling, intent compiler, HTTP fetch, semantic firewall, causal graph, WebMCP discovery, multimodal grounding, cross-agent collaboration, workflow memory, core types
- **35 WASM-exported functions** — complete API surface for any WASM host
- **41 HTTP REST endpoints** — deployable Axum server with CORS
- **20 MCP tools** — Claude Desktop, Cursor, VS Code compatible
- **232 tests** — 162 unit + 30 fixture + 40 integration, all passing
- **13 benchmarks** — parse, intent, injection, all within targets
- **2 SDK bindings** — Python + Node.js (with TypeScript types)
- **CI/CD pipeline** — test, clippy, fmt, WASM build, security audit

### Dependencies

```toml
# Core (always included)
html5ever = "0.27"          # HTML5 spec-compliant parser
markup5ever_rcdom = "0.3"   # DOM tree builder
serde = "1.0"               # Serialization
serde_json = "1.0"          # JSON
wasm-bindgen = "0.2"        # WASM interop

# Optional (feature-gated)
boa_engine = "0.21"         # JS sandbox (feature: js-eval)
reqwest = "0.12"            # HTTP client (feature: fetch)
robotstxt = "0.3"           # robots.txt parser (feature: fetch)
governor = "0.10"           # Rate limiting (feature: fetch)
axum = "0.7"                # HTTP server (feature: server)
tokio = "1"                 # Async runtime (feature: server)
tower-http = "0.5"          # CORS middleware (feature: server)
rmcp = "1.2"                # MCP protocol (feature: mcp)
```

### Design Principles

**Security first.** Trust shield is a foundational module, not an afterthought. Every byte from the web is `Untrusted` by default.

**Goal-native perception.** The LLM receives an answer to "what's relevant to my goal right now?" — not a browser view to interpret.

**Embedded, not remote.** Zero network latency when running as WASM in the same process as the agent.

**Stateless over WASM boundary.** All state (memory, temporal data, collab stores) serializes to JSON strings that cross the WASM boundary cleanly.

**No feature creep.** Every module exists because it was needed, not because it might be useful someday.

### Security Model

| Layer | Protection |
|-------|-----------|
| **Type system** | All web content is `TrustLevel::Untrusted` — cannot be promoted without explicit justification |
| **Parse-time scan** | 20+ injection patterns (EN + SV) checked during HTML parsing |
| **Content boundaries** | `<UNTRUSTED_WEB_CONTENT>` markers wrap all web output |
| **Zero-width detection** | Invisible Unicode characters (ZWS, ZWNJ, ZWJ) flagged |
| **Temporal analysis** | Multi-snapshot adversarial pattern detection (escalating, gradual, volatility, structural) |
| **Semantic firewall** | 3-level goal-aware request filtering blocks tracking, irrelevant resources |
| **SSRF protection** | Blocks localhost, private IPs, non-HTTP schemes in fetch |
| **JS sandbox** | No DOM, no fetch, no timers, no modules — pure computation only |
| **Causal reasoning** | Risk-weighted path finding avoids high-risk actions |

---

## Contributing

Issues and PRs welcome.

```bash
# Development loop (run before every commit)
cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

All bugs found during any PR — even if unrelated — must be fixed in the same PR. The codebase must always be in a working state on every commit.

---

## License

MIT © 2026 robinandreeklund-collab
