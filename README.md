# AetherAgent

> **The world's first embeddable, serverless, LLM-native browser engine** – built exclusively for AI agents, not humans.

[![CI](https://github.com/robinandreeklund-collab/AetherAgent/actions/workflows/ci.yml/badge.svg)](https://github.com/robinandreeklund-collab/AetherAgent/actions)
[![Rust](https://img.shields.io/badge/rust-stable-orange.svg)](https://www.rust-lang.org)
[![WASM](https://img.shields.io/badge/target-wasm32--unknown--unknown-blue.svg)](https://webassembly.org)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/robinandreeklund-collab/AetherAgent)

```
Chrome is built for humans.
AetherAgent is built for Claude, GPT-4o, Llama – any LLM.
10–50× faster. 10–30× less memory. Built-in intelligence.
```

---

## What is AetherAgent?

AetherAgent is **not** a headless browser (like Playwright or Puppeteer).  
AetherAgent is **not** a cloud service (like Browserbase or Hyperbrowser).  
AetherAgent is **not** a finished agent (like Claude Computer Use).

It is the **engine** – a perception + action layer – that you embed directly inside your own agent. Written in Rust, compiled to WebAssembly, it runs in the same process as your LLM with zero network latency.

> **Important:** AetherAgent is a **perception + fetch layer**, not a full browser. Since Fas 7, it can fetch pages itself (with cookies, redirects, SSRF protection), but it does not execute full JavaScript runtimes or render CSS. For JS-heavy SPAs, pair AetherAgent with a headless browser. For static/SSR pages, AetherAgent works standalone end-to-end.

Instead of handing your agent raw HTML or screenshots, AetherAgent delivers a **semantic accessibility tree** with goal-aware JSON – the page already understood, filtered, and ranked by relevance to your agent's current goal.

```python
# Without AetherAgent – LLM receives 50,000 tokens of raw HTML
html = requests.get(url).text
llm.send(html)  # slow, expensive, unreliable

# With AetherAgent – LLM receives 200 tokens of semantic JSON
tree = agent.parse_to_semantic_tree(html, goal="buy cheapest flight", url=url)
llm.send(tree)  # fast, cheap, goal-aware
```

---

## Core Features

### Semantic Perception Layer
Every page is translated into structured JSON with roles, labels, states, and a built-in **goal-relevance score** (e.g. `"this button is 98% relevant to 'buy cheapest flight'"`). Your LLM only sees what matters.

```json
{
  "url": "https://example.com/shop",
  "goal": "add to cart",
  "nodes": [
    {
      "id": 42,
      "role": "button",
      "label": "Add to cart – 199 kr",
      "action": "click",
      "relevance": 0.97,
      "trust": "Untrusted"
    },
    {
      "id": 17,
      "role": "textbox",
      "label": "Search products...",
      "action": "type",
      "relevance": 0.31,
      "trust": "Untrusted"
    }
  ],
  "injection_warnings": [],
  "parse_time_ms": 14
}
```

### Trust Shield – Prompt Injection Protection
AetherAgent filters prompt injection **at the perception layer**, not as an afterthought. All web content is marked `Untrusted`, wrapped in content-boundary markers before reaching the LLM, and scanned for 20+ known injection patterns including zero-width character attacks.

```
<UNTRUSTED_WEB_CONTENT>
  ... page content here ...
</UNTRUSTED_WEB_CONTENT>
```

> Research from Anthropic (2025): Prompt injection is the #1 security risk for browser agents. AetherAgent's architecture makes it structural, not optional.

### Intent-Aware API
Instead of raw coordinate clicks, agents call goal-oriented methods:

```python
agent.find_and_click("log in with Google")
agent.extract_prices("hotels in Paris under 1500 kr")
agent.fill_form({"email": "user@example.com", "password": "..."})
```

### Minimal Footprint
| Metric | AetherAgent | Lightpanda | Playwright + Chrome |
|--------|------------|-----------|---------------------|
| Binary size | ~2–6 MB | ~50 MB | ~300 MB |
| Startup time | <1 ms | ~250 ms | 1,000–3,000 ms |
| Memory per instance | ~9.5 MB | ~19 MB | ~150 MB |
| 100 parallel tasks | 182 ms | 1,300 ms | N/A |
| Throughput | ~550 req/s | ~77 req/s | ~5 req/s |

### Honest Comparison: AetherAgent vs Lightpanda vs Playwright

AetherAgent is **not** a browser replacement. It occupies a different niche. Here's an honest feature comparison:

| Capability | AetherAgent | Lightpanda | Playwright + Chrome |
|-----------|------------|-----------|---------------------|
| **Category** | Perception layer | Headless browser | Full browser automation |
| Fetches pages (HTTP) | **Yes** (reqwest, Fas 7) | Yes (libcurl) | Yes (Chrome DevTools) |
| Full JavaScript (V8/SpiderMonkey) | No (Boa sandbox only) | Yes (V8) | Yes (V8) |
| CSS rendering / layout | No | Partial | Yes |
| Semantic Firewall (request filtering) | **Yes** | No | No |
| Cookies / sessions | No | Yes | Yes |
| CDP protocol | No | Yes | Yes |
| Playwright/Puppeteer compatible | No | Yes | Yes (native) |
| Semantic tree with goal scoring | **Yes** | No | No |
| Prompt injection detection | **Yes** | No | No |
| Semantic diff (token savings) | **Yes** | No | No |
| Temporal adversarial detection | **Yes** | No | No |
| Intent compiler (goal → plan) | **Yes** | No | No |
| Embeddable in WASM | **Yes** | No | No |
| Startup time | <1 ms | ~250 ms | 1,000–3,000 ms |
| Memory per instance | ~9.5 MB | ~19 MB | ~150 MB |
| License | MIT | AGPL-3.0 | Apache-2.0 |

**When to use AetherAgent:** Your agent already fetches HTML (via `requests`, `httpx`, `fetch`, or any HTTP client) and needs fast, goal-aware semantic understanding with built-in security. Perfect for data extraction, form filling, navigation planning – anywhere you need perception, not rendering.

**When to use Lightpanda/Playwright:** You need a full browser: JavaScript-heavy SPAs, sites behind authentication flows, visual rendering, or CDP-based automation.

**Best of both worlds:** Use AetherAgent as the perception layer on top of a browser's HTML output. Fetch with Lightpanda/Playwright, perceive with AetherAgent.

### Runs Everywhere
Compiles to WebAssembly and runs in Python, Node.js, Cloudflare Workers, WasmEdge, and browser PWAs – with zero vendor lock-in.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│              LLM Agent (Claude / GPT-4o / Llama)        │
│         Receives semantic JSON → plans → acts           │
└────────────────────────┬────────────────────────────────┘
                         │ goal-aware JSON
┌────────────────────────▼────────────────────────────────┐
│                  AetherAgent Core (Rust → WASM)         │
│  ┌─────────────┐  ┌──────────────────┐  ┌───────────┐  │
│  │ HTML Parser │  │  Semantic Layer  │  │Intent API │  │
│  │ html5ever   │  │  A11y tree +     │  │find_click │  │
│  │ rcdom       │  │  goal scoring +  │  │fill_form  │  │
│  │ 10–50ms     │  │  trust shield    │  │extract    │  │
│  └─────────────┘  └──────────────────┘  └───────────┘  │
└────────────────────────┬────────────────────────────────┘
                         │
┌────────────────────────▼────────────────────────────────┐
│           Runtime (your choice, zero lock-in)           │
│  Python (wasmtime)  │  Node.js  │  Cloudflare Workers   │
└─────────────────────────────────────────────────────────┘
```

---

## Quick Start

### Prerequisites

```bash
# Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown

# WASM tools
cargo install wasm-pack

# Python runtime
pip install wasmtime requests
```

### Build

```bash
git clone https://github.com/robinandreeklund-collab/AetherAgent.git
cd AetherAgent

# Build WASM binary
wasm-pack build --target web --release

# Run tests
cargo test
```

### Python Example

```bash
python examples/python_test.py
```

```python
import json
from wasmtime import Store, Module, Instance, Linker, WasiConfig

# Load the WASM module
store = Store()
# ... (see examples/python_test.py for full setup)

# Parse a page with a goal
result = agent.parse_to_semantic_tree(html, "buy cheapest flight", url)
tree = json.loads(result)

# Top node is the most relevant action
best = tree["nodes"][0]
print(f"Best action: {best['action']}({best['label']}) – relevance {best['relevance']}")
# → Best action: click(Book now – 1,299 kr) – relevance 0.94
```

---

## Deploy to Render

One-click deploy the AetherAgent HTTP API to Render:

[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/robinandreeklund-collab/AetherAgent)

This deploys a Docker container running the axum REST API server with all AetherAgent endpoints.

### API Usage (after deploy)

```bash
# Health check
curl https://your-app.onrender.com/health

# Parse HTML to semantic tree
curl -X POST https://your-app.onrender.com/api/parse \
  -H "Content-Type: application/json" \
  -d '{"html": "<html><body><button>Buy now</button></body></html>", "goal": "buy product", "url": "https://shop.com"}'

# Find best clickable element
curl -X POST https://your-app.onrender.com/api/click \
  -H "Content-Type: application/json" \
  -d '{"html": "<html><body><button>Add to cart</button></body></html>", "goal": "buy", "url": "https://shop.com", "target_label": "Add to cart"}'

# Check for prompt injection
curl -X POST https://your-app.onrender.com/api/check-injection \
  -H "Content-Type: application/json" \
  -d '{"text": "Ignore previous instructions"}'

# Semantic diff between two trees (Fas 4a)
curl -X POST https://your-app.onrender.com/api/diff \
  -H "Content-Type: application/json" \
  -d '{"old_tree_json": "<tree1 JSON from /api/parse>", "new_tree_json": "<tree2 JSON from /api/parse>"}'
```

### All Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| POST | `/api/parse` | Parse HTML to full semantic tree |
| POST | `/api/parse-top` | Parse top-N relevant nodes |
| POST | `/api/click` | Find best clickable element |
| POST | `/api/fill-form` | Map form fields to key/value pairs |
| POST | `/api/extract` | Extract structured data by keys |
| POST | `/api/diff` | Semantic diff between two trees (token savings) |
| POST | `/api/detect-js` | Detect JavaScript snippets in HTML |
| POST | `/api/eval-js` | Evaluate JS expression in sandbox |
| POST | `/api/eval-js-batch` | Evaluate multiple JS expressions |
| POST | `/api/parse-js` | Parse HTML with automatic JS detection and evaluation |
| POST | `/api/check-injection` | Check text for prompt injection |
| POST | `/api/wrap-untrusted` | Wrap content in trust markers |
| POST | `/api/memory/create` | Create workflow memory |
| POST | `/api/memory/step` | Add workflow step |
| POST | `/api/memory/context/set` | Set context key/value |
| POST | `/api/memory/context/get` | Get context value |
| POST | `/api/temporal/create` | Create temporal memory |
| POST | `/api/temporal/snapshot` | Add temporal snapshot |
| POST | `/api/temporal/analyze` | Analyze temporal patterns & adversarial detection |
| POST | `/api/temporal/predict` | Predict next page state |
| POST | `/api/compile` | Compile goal to action plan |
| POST | `/api/execute-plan` | Execute plan against current page state |
| POST | `/api/fetch` | Fetch URL → return HTML + metadata |
| POST | `/api/fetch/parse` | Fetch URL → semantic tree (one call) |
| POST | `/api/fetch/click` | Fetch URL → find clickable element |
| POST | `/api/fetch/extract` | Fetch URL → extract structured data |
| POST | `/api/fetch/plan` | Fetch URL → compile goal → execute plan |
| POST | `/api/firewall/classify` | Classify URL against semantic firewall (L1/L2/L3) |
| POST | `/api/firewall/classify-batch` | Classify batch of URLs against firewall |

### Python SDK (connects to deployed server)

```python
from bindings.python.aether_agent import AetherAgent

agent = AetherAgent(base_url="https://your-app.onrender.com")
tree = agent.parse(html, goal="buy cheapest flight", url="https://flights.com")
click = agent.find_and_click(html, goal="buy", url="https://shop.com", target_label="Add to cart")
```

---

## Project Structure

```
AetherAgent/
├── src/
│   ├── lib.rs          # WASM entrypoint – public API
│   ├── parser.rs       # html5ever + rcdom DOM builder
│   ├── semantic.rs     # Accessibility tree → semantic JSON
│   ├── trust.rs        # Trust shield – prompt injection filter
│   ├── intent.rs       # Intent API – find_and_click, fill_form, extract_data
│   ├── diff.rs         # Semantic DOM Diffing – minimal delta between trees
│   ├── js_eval.rs      # JS Sandbox – Boa engine for safe snippet evaluation
│   ├── js_bridge.rs    # Selective Execution – detect, eval, apply JS to tree
│   ├── temporal.rs     # Temporal Memory – time-series tracking, adversarial detection
│   ├── compiler.rs     # Intent Compiler – goal decomposition, action plan optimization
│   ├── fetch.rs        # HTTP Fetch – reqwest-based page fetching with SSRF protection
│   ├── firewall.rs     # Semantic Firewall – 3-level goal-aware request filtering
│   ├── memory.rs       # Workflow memory – stateless context across WASM
│   ├── types.rs        # Core data structures
│   └── bin/
│       ├── server.rs     # HTTP API server (axum) for deployment
│       └── mcp_server.rs # MCP server (rmcp) for Claude/Cursor/VS Code
├── bindings/
│   ├── node/           # Node.js SDK with TypeScript types
│   └── python/         # Python SDK (HTTP + WASM)
├── benches/
│   └── bench_main.rs   # Performance benchmark suite
├── examples/
│   └── python_test.py  # Complete Python agent loop demo
├── tests/
│   ├── integration_test.rs  # WebArena-inspired integration tests
│   ├── fixture_tests.rs     # 20 real-site HTML scenario tests
│   └── fixtures/            # 20 realistic HTML test pages
├── .github/
│   └── workflows/
│       └── ci.yml      # CI: build, test, WASM size check, security audit
├── Dockerfile          # Multi-stage Docker build for deployment
├── render.yaml         # Render.com deployment blueprint
└── Cargo.toml
```

---

## Development Roadmap

### MVP – 6 Weeks

| Phase | Weeks | Status | Description |
|-------|-------|--------|-------------|
| **Fas 1** – Grund & säkerhet | 1–2 | ✅ Done | HTML parser, semantic layer, trust shield, WASM build |
| **Fas 2** – Intent API & minne | 3–4 | ✅ Done | find_and_click, fill_form, extract_data, workflow memory |
| **Fas 3** – Runtime & integration | 5–6 | ✅ Done | HTTP API, Python + Node bindings, benchmarks, 20 real-site tests |

### Post-MVP

| Phase | Status | Description |
|-------|--------|-------------|
| **Fas 4a** – Semantic DOM Diffing | ✅ Done | Compare two trees → minimal delta, 80–95% token savings for multi-step flows |
| **Fas 4b** – JS Sandbox (Boa) | ✅ Done | Embedded Boa JS engine for evaluating inline scripts, event handlers, expressions |
| **Fas 4c** – Selective Execution | ✅ Done | Smart detection of JS-dependent content, targeted eval instead of full browser |
| **Fas 5** – Temporal Memory & Adversarial Modeling | ✅ Done | Time-series page change tracking, predictive injection defense |
| **Fas 6** – Intent Compiler | ✅ Done | Multi-step goal → optimized action plan with speculative prefetch |
| **Fas 7** – HTTP Fetch Integration | ✅ Done | Built-in URL fetching with cookies, redirects, robots.txt, SSRF protection |
| **Fas 8** – Semantic Firewall & Ethical Engine | ✅ Done | Goal-aware request filtering (L1/L2/L3), Google robots.txt parser, per-domain rate limiting |

### Design Principles

**Security first.** Trust shield is Fas 1, not Fas 5. Every byte from the web is `Untrusted` by default.

**Goal-native perception.** The LLM receives an answer to "what's relevant to my goal right now?" – not a browser view to interpret.

**No JavaScript required for MVP.** AetherAgent targets static HTML and SSR pages first (~30–40% of the web, including the entire high-value data extraction niche). CDP fallback for SPAs comes in Fas 4.

**Embedded, not remote.** Zero network latency because the engine runs in the same process as the agent.

### Semantic DOM Diffing (Fas 4a)

Instead of sending the full semantic tree after every agent action, AetherAgent can now compute a minimal **delta** between two trees. This reduces token usage by 80–95% for multi-step agent flows.

```python
# Step 1: Parse the initial page
tree1 = agent.parse(html1, goal="buy product", url="https://shop.se")
llm.send(tree1)  # Full tree: ~200 tokens

# Step 2: After clicking "Add to cart", parse the updated page
tree2 = agent.parse(html2, goal="buy product", url="https://shop.se")
delta = agent.diff_trees(tree1, tree2)
llm.send(delta)  # Delta: ~20 tokens (90% savings)
```

```json
{
  "total_nodes_before": 15,
  "total_nodes_after": 16,
  "changes": [
    {
      "node_id": 42,
      "change_type": "Modified",
      "role": "button",
      "label": "1 i varukorg",
      "changes": [
        {"field": "label", "before": "0 i varukorg", "after": "1 i varukorg"}
      ]
    },
    {
      "node_id": 99,
      "change_type": "Added",
      "role": "link",
      "label": "Gå till kassan"
    }
  ],
  "token_savings_ratio": 0.87,
  "summary": "2 changes (1 modified, 1 added), 87% token savings"
}
```

### JavaScript Sandbox (Fas 4b)

Many modern pages use inline JavaScript for pricing, dynamic text, and conditional rendering. AetherAgent embeds the **Boa** JavaScript engine (pure Rust, no C dependencies) in a sandboxed environment to evaluate these snippets safely.

**Detection** – scan HTML for inline scripts, event handlers, and framework markers:

```python
detection = agent.detect_js(html)
# → {"total_inline_scripts": 2, "total_event_handlers": 3,
#    "has_framework": true, "framework_hint": "React",
#    "snippets": [{"snippet_type": "InlineScript", "affects_content": true, ...}]}
```

**Evaluation** – run safe JS expressions in a sandbox (no DOM, no fetch, no timers):

```python
result = agent.eval_js("29.99 * 2")
# → {"value": "59.98", "error": null, "timed_out": false}

result = agent.eval_js("`Total: ${(199 * 1.25).toFixed(2)} kr`")
# → {"value": "Total: 248.75 kr", "error": null}

# Dangerous operations are blocked
result = agent.eval_js("fetch('https://evil.com')")
# → {"value": null, "error": "Blocked: 'fetch' is not allowed in sandbox"}
```

**Batch evaluation** for multiple snippets:

```python
results = agent.eval_js_batch(["1+1", "'a'+'b'", "Math.PI.toFixed(2)"])
# → {"results": [{"value": "2"}, {"value": "ab"}, {"value": "3.14"}]}
```

**Security model:**
- No DOM access (`document.*`, `window.*` blocked)
- No network (`fetch`, `XMLHttpRequest` blocked)
- No timers (`setTimeout`, `setInterval` blocked)
- No module system (`import`, `require`, `eval` blocked)
- Pure computation only: math, strings, arrays, objects, template literals

### Selective Execution (Fas 4c)

The selective execution pipeline combines detection (Fas 4b) with targeted evaluation and application back to the semantic tree. Instead of running a full headless browser, AetherAgent intelligently identifies JS-dependent content and evaluates only the relevant expressions.

```python
# One-call pipeline: detect JS → extract expressions → evaluate → apply to tree
result = agent.parse_with_js(html, goal="buy product", url="https://shop.se")

# Result includes the enhanced tree + metadata about what JS was processed
print(result["analysis"])
# → {"total_snippets": 3, "evaluable_expressions": 2,
#    "dom_targeted_expressions": 1, "successful_bindings": 1,
#    "failed_evaluations": 0, "frameworks_detected": ["React"]}

print(result["bindings"])
# → [{"node_id": 5, "target_selector": "#price",
#     "target_property": "textContent", "expression": "199 * 0.8",
#     "computed_value": "159.2", "applied": true}]
```

**How it works:**

1. **Detect** – Scan HTML for inline scripts, event handlers, and framework markers
2. **Extract** – Parse `getElementById('id').textContent = expr` and `querySelector('sel').property = expr` patterns
3. **Match** – Map DOM targets (IDs, selectors) to semantic tree nodes
4. **Evaluate** – Run extracted expressions in the Boa sandbox (Fas 4b)
5. **Apply** – Update matched node labels/values with computed results

**When to use:**
- Pages with dynamic pricing (`document.getElementById('price').textContent = basePrice * discount`)
- Server-rendered pages with client-side hydration scripts
- Any page where key content is set via inline JavaScript

### Temporal Memory & Adversarial Modeling (Fas 5)

AetherAgent tracks page state over time, detecting adversarial patterns that single-snapshot analysis misses:

```python
agent = AetherAgent("https://your-url.onrender.com")

# Track page state across multiple visits
mem = agent.create_temporal_memory()
for step, html in enumerate(page_snapshots):
    mem = agent.add_temporal_snapshot(mem, html, "köp produkt", url, step * 1000)

# Analyze for adversarial patterns
analysis = agent.analyze_temporal(mem)
print(f"Risk: {analysis['risk_score']}")           # 0.0–1.0
print(f"Patterns: {analysis['adversarial_patterns']}")  # EscalatingInjection, etc.

# Predict next page state
prediction = agent.predict_temporal(mem)
print(f"Expected nodes: {prediction['expected_node_count']}")
print(f"Confidence: {prediction['confidence']}")
```

**Adversarial detection types:**
- `EscalatingInjection` – Injection warnings increase monotonically across steps
- `GradualInjection` – Clean nodes gradually acquire injection patterns
- `SuspiciousVolatility` – Text nodes change too frequently (>70% of observations)
- `StructuralManipulation` – >50% of nodes change in a single step

### Intent Compiler (Fas 6)

Compile complex goals into optimized action plans with dependency tracking and parallel execution:

```python
# Compile a goal into sub-goals with dependencies
plan = agent.compile_goal("köp iPhone 16 Pro")
print(f"Steps: {plan['total_steps']}")         # 7 (navigate→click→fill→verify)
print(f"Parallel groups: {plan['parallel_groups']}")  # 5 (some steps can run in parallel)

# Execute plan against current page state
result = agent.execute_plan(plan, html, "köp produkt", url, completed_steps=[0])
print(f"Next action: {result['next_action']}")  # {action_type: "Click", target_label: "Lägg i varukorg"}
print(f"Prefetch: {result['prefetch_suggestions']}")  # URLs to pre-parse
```

**Supported goal templates:** `buy/purchase`, `login/sign in`, `search/find`, `register/sign up`, `extract/scrape`. Unknown goals get a generic 3-step plan (Navigate → Act → Verify).

### HTTP Fetch Integration (Fas 7)

AetherAgent can now fetch pages itself – no external HTTP client needed. The fetch layer includes cookie storage, redirect following, gzip/brotli decompression, robots.txt respect, and SSRF protection (blocks localhost, private IPs, non-HTTP schemes).

```python
agent = AetherAgent("https://your-url.onrender.com")

# One-call: fetch URL → parse to semantic tree
result = agent.fetch_parse("https://shop.se/products", goal="buy cheapest product")
print(f"Fetched {result['fetch']['body_size_bytes']} bytes in {result['fetch']['fetch_time_ms']}ms")
print(f"Found {len(result['tree']['nodes'])} semantic nodes")

# One-call: fetch URL → find clickable element
click = agent.fetch_click("https://shop.se/product/42", goal="buy", target_label="Add to cart")
print(f"Best match: {click['click']['label']} (relevance: {click['click']['relevance']})")

# One-call: fetch URL → extract structured data
data = agent.fetch_extract("https://shop.se/product/42", goal="get price", keys=["price", "name"])
print(f"Extracted: {data['extract']['entries']}")

# One-call: fetch URL → compile goal → execute plan
plan = agent.fetch_plan("https://shop.se", goal="köp iPhone 16 Pro")
print(f"Next action: {plan['execution_json']}")

# Custom config: robots.txt, custom headers, timeout
config = {"respect_robots_txt": True, "timeout_ms": 5000, "extra_headers": {"Authorization": "Bearer ..."}}
result = agent.fetch_parse("https://api.example.com/products", goal="extract data", config=config)
```

**Security features:**
- SSRF protection: blocks `localhost`, `127.0.0.1`, `10.x.x.x`, `192.168.x.x`, `172.16.x.x`, non-HTTP schemes
- Optional robots.txt compliance
- Configurable timeouts and redirect limits
- Cookie jar with automatic session management

**Endpoints:**

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/fetch` | Fetch URL → return HTML + metadata |
| POST | `/api/fetch/parse` | Fetch URL → semantic tree |
| POST | `/api/fetch/click` | Fetch URL → find clickable element |
| POST | `/api/fetch/extract` | Fetch URL → extract structured data |
| POST | `/api/fetch/plan` | Fetch URL → compile goal → execute plan |

### Semantic Firewall & Ethical Engine (Fas 8)

AetherAgent now includes a **Semantic Firewall** – a three-level goal-aware request filter that blocks irrelevant subrequests before they waste bandwidth and tokens:

```python
agent = AetherAgent("https://your-url.onrender.com")

# Classify a single URL against the firewall
verdict = agent.classify_request(
    "https://www.google-analytics.com/collect",
    goal="buy product"
)
print(verdict)  # {"allowed": false, "blocked_by": "L1UrlPattern", ...}

# Batch classification for page subrequests
verdicts = agent.classify_request_batch(
    urls=["https://shop.se/api/products", "https://cdn.hotjar.com/track.js",
          "https://shop.se/hero.jpg", "https://shop.se/checkout"],
    goal="buy product"
)
print(verdicts["summary"])  # {"blocked_l1": 1, "blocked_l2": 1, ...}
```

**Firewall Levels:**

| Level | What it checks | Speed | Coverage |
|-------|---------------|-------|----------|
| **L1** | URL pattern – 45+ tracking domains (Google Analytics, Facebook, Hotjar, etc.) | <1μs | Ads, analytics, fingerprinting, chat widgets |
| **L2** | File extension/MIME – blocks images, fonts, video, archives | <1μs | .jpg, .woff2, .mp4, .pdf, .zip, etc. |
| **L3** | Semantic relevance – scores URL against agent's current goal | ~1ms | Keyword matching + path segment analysis |

**Ethical Engine:**

- **robots.txt compliance** – Google's official `robotstxt` crate (RFC 9309) for accurate Allow/Disallow parsing
- **Per-domain rate limiting** – `governor` crate with GCRA algorithm (default 2 req/s per domain)
- **Retry-After header** – respects 429/503 rate limit responses
- **SSRF protection** – blocks localhost, private IPs, non-HTTP schemes

**Endpoints:**

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/firewall/classify` | Classify URL against semantic firewall |
| POST | `/api/firewall/classify-batch` | Classify batch of URLs |

### MCP Server (Model Context Protocol)

AetherAgent exposes all core tools via MCP – the de facto AI agent standard used by Claude, ChatGPT, Gemini, Cursor, and VS Code.

```bash
# Run the MCP server on stdio
cargo run --features mcp --bin aether-mcp
```

**Available MCP Tools:**

| Tool | Description |
|------|-------------|
| `parse` | Parse HTML to semantic tree with goal-relevance scoring |
| `parse_top` | Parse top-N most relevant nodes (token-efficient) |
| `find_and_click` | Find best clickable element matching a label |
| `fill_form` | Map form fields to key/value pairs |
| `extract_data` | Extract structured data by semantic keys |
| `check_injection` | Check text for prompt injection patterns |
| `compile_goal` | Compile goal into action plan with dependencies |
| `classify_request` | Classify URL against semantic firewall |
| `diff_trees` | Semantic diff between two trees (80-95% token savings) |
| `parse_with_js` | Parse with automatic JS detection and evaluation |

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

### Architecture Decisions & Research Notes

**Why MCP over CDP (sCDP):**
The project initially considered implementing a simplified CDP (Chrome DevTools Protocol) layer. After research, we concluded that the market is moving toward **MCP (Model Context Protocol)** – the de facto AI agent standard adopted by Claude, ChatGPT, Gemini, Cursor, and VS Code. Google's **WebMCP** standard (Feb 2026) envisions websites exposing structured tools to AI agents – which maps directly to AetherAgent's semantic tree format. CDP is a browser debugging protocol, not an AI interaction standard. AetherAgent is a perception layer, not a browser, making MCP the natural integration point.

**Why wreq/stealth is deferred:**
`wreq` (v6.0.0-rc.28) offers JA4 TLS fingerprint emulation for anti-bot evasion. We deferred it because: (1) still in RC, not stable; (2) anti-bot evasion requires more than TLS fingerprints (JS challenges, CAPTCHAs, behavioral analysis); (3) ethically questionable under EU AI Act Art. 52 transparency requirements; (4) AetherAgent targets ethical use cases where robots.txt compliance is preferred over evasion. Will revisit when wreq reaches stable release, as an optional feature.

**JS validation prototype (Gap 3 – why prototype first):**
The claim that "only 1-5% of JavaScript on web pages actually mutates the DOM" is unsubstantiated. Before investing in a full oxc_parser + rquickjs migration (replacing Boa), we need empirical validation. Our fixture analysis showed 0% of test fixtures contain any JavaScript – by design (they test parser/semantic features). Proper validation requires crawling 1,000+ real websites and measuring DOM-mutation vs. analytics/tracking script ratios. Until validated, we keep Boa (pure Rust, no C deps) as the sandbox engine. A future phase may add oxc_parser for faster AST analysis and rquickjs for better ES2023+ coverage, but only if the DOM-mutation ratio validates the investment.

### Future Work

Ideas under evaluation for future phases:

- **Causal Action Graph** – Model page state transitions as a directed graph, enabling the agent to reason about action consequences before executing them.
- **Cross-Agent Semantic Diffing** – Share semantic deltas between multiple agents working on the same site, reducing redundant parsing.
- **Multimodal Grounding** – Combine semantic tree data with vision model bounding boxes for elements that lack accessible labels.

---

## Benchmark Goals

| Metric | Target | Comparison |
|--------|--------|------------|
| WebArena success rate | >65% | SOTA: 61.7% (IBM CUGA, 2025) |
| Parse time (median page) | <50ms | Playwright: ~800ms |
| Memory per agent | <20MB | Chrome headless: ~150MB |
| WASM binary size | <5MB | — |
| Parallel agents (8GB RAM) | >200 | Playwright: ~5 |

### Live Benchmarks (Render Free Tier)

Tested against the live deployment at `aether-agent-api.onrender.com`. Network baseline (TTFB) is ~110ms; engine processing time is the remainder.

```
Benchmark                                        N      Avg     P50     Min     Max
─────────────────────────────────────────────────────────────────────────────────────
Health & Parse
  GET /health                                   20    457ms   152ms   106ms  3474ms
  POST /api/parse (simple, 3 elements)          20    175ms   142ms   101ms   379ms
  POST /api/parse (ecommerce, 10 elements)      20    143ms   132ms    92ms   357ms
  POST /api/parse (login form)                  20    134ms   117ms    90ms   298ms
  POST /api/parse (medium, 10 products)         15    126ms   114ms    92ms   245ms
  POST /api/parse (large, 50 products)          10    181ms   176ms   110ms   303ms

Intent API
  POST /api/click (find button)                 20    124ms   108ms    89ms   281ms
  POST /api/fill-form (3 fields)                20    119ms   111ms    93ms   218ms
  POST /api/extract (2 keys)                    20    115ms   108ms    88ms   294ms

Trust Shield
  POST /api/check-injection (safe text)         20    114ms   111ms   101ms   147ms
  POST /api/check-injection (malicious text)    20    109ms   109ms    90ms   138ms

Workflow Memory
  POST /api/memory/create                       20    125ms   114ms    88ms   351ms
  POST /api/memory/step                         20    124ms   112ms    89ms   267ms
  POST /api/memory/context/set                  20    120ms   114ms    90ms   281ms
  POST /api/memory/context/get                  20    130ms   112ms    93ms   391ms
```

**Key takeaways:**

- **Engine processing: ~1–70ms** (total latency minus ~110ms network baseline)
- **50-product page parsed in ~70ms** engine time – well under 500ms target
- **Injection check: <1ms** engine time – negligible overhead
- **All endpoints under 200ms P50** including network round-trip
- First request after cold start (~3.5s) reflects Render free tier spin-up

```
Semantic Diff (Fas 4a)
  POST /api/diff (identical, 3 nodes)            10    423ms   482ms   141ms   729ms
  POST /api/diff (small change, 3 nodes)         10    319ms   216ms   142ms   790ms
  POST /api/diff (1 change in 20 nodes)          10    299ms   289ms   165ms   602ms
  POST /api/diff (identical, 20 nodes)           10    433ms   436ms   149ms   917ms
```

**Diff verification results (live):**

| Test | Result | Detail |
|------|--------|--------|
| Identical pages | ✅ | 0 changes, 100% token savings |
| Added node detected | ✅ | Correctly identifies new elements |
| Removed node detected | ✅ | Correctly identifies deleted elements |
| State change (disabled→enabled) | ✅ | Detects `state.disabled` field change |
| Token savings (20 nodes, 1 change) | ✅ | 86% token savings |
| Invalid JSON → error | ✅ | Returns descriptive error message |

**JS Sandbox verification results (live – Fas 4b):**

| Test | Result | Detail |
|------|--------|--------|
| Detect: inline script + handlers | ✅ | 1 script, 2 event handlers found |
| Detect: static page → 0 JS | ✅ | No scripts, no handlers, no framework |
| Detect: Next.js framework | ✅ | `framework_hint: "Next.js"` |
| Detect: affects_content (innerHTML) | ✅ | DOM-modifying scripts flagged |
| Eval: `29.99 * 2` | ✅ | `"59.98"` |
| Eval: template literal | ✅ | `"Pris: 248.75 kr"` |
| Eval: array.map.join | ✅ | `"2,4,6"` |
| Eval: JSON.stringify | ✅ | `{"price":199,"currency":"SEK"}` |
| Eval: fetch → BLOCKED | ✅ | `"Blocked: 'fetch' is not allowed"` |
| Eval: document.cookie → BLOCKED | ✅ | `"Blocked: 'cookie' is not allowed"` |
| Eval: batch (3 expressions) | ✅ | `["2", "ab", "3.14"]` |
| Eval: ternary operator | ✅ | `"I lager"` |

**Selective Execution verification results (live – Fas 4c):**

| Test | Result | Detail |
|------|--------|--------|
| parse-js: inline script + DOM target | ✅ | `getElementById('buy')` → `"Köp: 59.98 kr"` applied to node |
| parse-js: static page (no JS) | ✅ | 0 bindings, 0 evals, 0ms overhead |
| parse-js: multiple DOM targets | ✅ | 2 bindings, 2 successful: `"Betala 597 kr"`, `"149.25 kr moms"` |
| parse-js: framework detection | ✅ | `has_framework: true`, `framework_hint: "Next.js"` |
| parse-js: blocked fetch in sandbox | ✅ | `"Blocked: 'fetch' is not allowed in sandbox"` |
| parse-js: performance (10 elements) | ✅ | <1ms exec time, binding correctly applied |

### Local Benchmarks (no network overhead)

Run `cargo run --bin aether-bench` for pure engine performance:

```
Benchmark                                     Avg (µs)   Min (µs)
──────────────────────────────────────────────────────────────────
parse: simple page (3 elements)                    284        261
parse: ecommerce (12 elements)                   1 211      1 151
parse: login form (6 elements)                     603        556
parse: complex page (100 products)              26 106     25 141
click: ecommerce find button                       937        888
fill_form: login (2 fields)                        501        476
extract: ecommerce price                           961        915
injection: safe text                                13         13
injection: malicious text                           14         13
```

**All local benchmarks pass performance targets** (simple <50ms, complex <500ms).

### AetherAgent vs Lightpanda – Head-to-Head

Tested locally on the same machine (4 CPU, 16 GB RAM) with identical HTML fixtures. AetherAgent runs as an HTTP server; Lightpanda runs as a CLI process per request (`--dump semantic_tree`).

> **Methodology disclaimer:** This comparison is partially unfair. AetherAgent runs as a persistent server with connection pooling (warm). Lightpanda spawns a new CLI process per request (cold startup + HTTP fetch + parse). Even adjusted for this, AetherAgent is extremely lightweight – but the 100–400x speedup numbers reflect this asymmetry. See "Fair Mode" benchmark below for cold-start adjusted measurements. Additionally, AetherAgent is a perception layer (you provide HTML) while Lightpanda is a full browser (it fetches pages itself) – they serve different purposes.

Run: `python3 benches/bench_vs_lightpanda.py`

**Parse Speed (median, 20 iterations):**

| Fixture | AetherAgent | Lightpanda | Speedup | AE tokens | LP tokens |
|---------|------------|-----------|---------|-----------|-----------|
| Simple (3 elements) | 725 µs | 295 ms | **406x** | 495 | 330 |
| E-commerce (10 elements) | 835 µs | 289 ms | **346x** | 1,678 | 931 |
| Login form (6 elements) | 785 µs | 273 ms | **348x** | 1,231 | 618 |
| Complex (50 products) | 2.6 ms | 258 ms | **100x** | 31,898 | 20,999 |
| Complex (100 products) | 4.3 ms | 287 ms | **67x** | 63,696 | 41,954 |

> Note: Lightpanda's time includes process startup + HTTP fetch + DOM parse + tree serialization. AetherAgent's time is pure parse + semantic tree build over HTTP. Both produce accessibility/semantic trees from the same HTML.

**Parallel Throughput (mixed ecommerce + complex fixtures):**

| Tasks | AetherAgent | Lightpanda | Ratio |
|-------|------------|-----------|-------|
| 25 | 57 ms (436/s) | 487 ms (51/s) | **8.5x** |
| 50 | 91 ms (550/s) | 1,294 ms (39/s) | **14.2x** |
| 100 | 182 ms (548/s) | 1,300 ms (77/s) | **7.1x** |

**Memory (RSS):**

| Engine | Scenario | RSS |
|--------|---------|-----|
| AetherAgent | Server idle | 9.4 MB |
| AetherAgent | After 50× complex parse | 9.5 MB |
| Lightpanda | Per process (any fixture) | 19.1 MB |

**Token Savings with Fas 4a Diff (AetherAgent-only feature):**

| Scenario | Raw tokens | Delta tokens | Savings |
|----------|-----------|-------------|---------|
| Same page (no change) | 495 | 54 | **89%** |
| E-commerce: add to cart | 1,823 | 547 | **70%** |
| Complex 50 (no change) | 31,898 | 55 | **99.8%** |
| 10-step agent loop | 17,505 | 6,605 | **62%** |

**Output Quality Comparison:**

| Feature | AetherAgent | Lightpanda |
|---------|------------|-----------|
| Semantic tree | Yes | Yes |
| Goal-relevance scoring | **Yes** | No |
| Prompt injection detection | **Yes** | No |
| Semantic diff (delta) | **Yes** | No |
| JS sandbox evaluation | **Yes** | No |
| Intent API (click/fill/extract) | **Yes** | No |
| Workflow memory | **Yes** | No |
| JavaScript execution (V8) | No | **Yes** |
| CSS rendering | No | No |

**JS Sandbox Performance (Fas 4b – AetherAgent-only):**

| Operation | Median | Detail |
|-----------|--------|--------|
| Detect: static page | 722 µs | 0 scripts, 0 handlers |
| Detect: inline + handlers | 718 µs | 1 script, 2 handlers |
| Detect: heavy (20 scripts) | 847 µs | 20 scripts, 20 handlers |
| Eval: math expression | 1.1 ms | `29.99 * 2` → `59.98` |
| Eval: template literal | 1.1 ms | `` `Pris: ${(199*1.25).toFixed(2)} kr` `` |
| Eval: JSON.stringify | 1.1 ms | `{"price":199,"currency":"SEK"}` |
| Blocked: fetch/eval/document | 700 µs | Rejected before execution |
| Batch: 3 expressions | 1.6 ms | 0.5 ms/expr |
| Batch: 20 expressions | 8.0 ms | 0.4 ms/expr |

**Selective Execution Performance (Fas 4c – AetherAgent-only):**

| Scenario | Median | Bindings | Applied |
|----------|--------|----------|---------|
| Static page (no JS) | 757 µs | 0 | 0 |
| Single DOM target | 1.2 ms | 1 | 1 |
| Multiple DOM targets | 1.4 ms | 2 | 2 |
| Heavy (20 scripts) | 8.9 ms | 20 | 20 |
| **Overhead vs plain parse** | | | |
| E-commerce | +2% | — | — |
| Complex (50 products) | +6% | — | — |

> **Summary:** AetherAgent is **100–400x faster** for semantic parsing (with the caveat that AetherAgent runs as a warm server while Lightpanda spawns per request – see methodology note above). It uses **half the memory** and includes AI-native features (goal scoring, injection protection, semantic diff, JS sandbox, intent API, temporal adversarial modeling, intent compiler) that Lightpanda does not offer. Lightpanda's advantage is full V8 JavaScript execution, HTTP fetching, cookies, CDP protocol, and Playwright compatibility – it is a complete browser. AetherAgent is a specialized perception layer designed to work alongside (not replace) HTTP clients or browsers. The benchmark suite covers all 6 phases (Fas 1–6) including WebArena-inspired multi-step scenarios.

---

## Security

AetherAgent takes prompt injection seriously as a structural design constraint, not a feature.

- All web content is marked `TrustLevel::Untrusted` at the type level
- Content-boundary markers wrap all web output before LLM delivery
- 20+ injection patterns scanned at parse time (EN + SV)
- Zero-width character detection (invisible text attacks)
- Sanitization replaces matched patterns with `[FILTERED]`

See [`src/trust.rs`](src/trust.rs) for implementation.

> "Prompt injection, much like scams and social engineering on the web, is unlikely to ever be fully 'solved'." – OpenAI, December 2025

AetherAgent's approach: make it structural, not probabilistic.

---

## Contributing

Issues and PRs welcome. The project is in active MVP development – see the roadmap above for what's coming next.

```bash
# Run all tests
cargo test

# Check formatting
cargo fmt --check

# Lint
cargo clippy -- -D warnings

# Security audit
cargo audit
```

---

## License

MIT © 2026 robinandreeklund-collab
