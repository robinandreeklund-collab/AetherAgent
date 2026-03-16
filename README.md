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
| Metric | AetherAgent | Playwright + Chrome |
|--------|------------|---------------------|
| Binary size | ~2–6 MB | ~300 MB |
| Startup time | 10–100 ms | 1,000–3,000 ms |
| Memory per instance | ~15 MB | ~150 MB |
| Parallel agents (laptop) | 500–1,000 | 5–10 |

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
```

### All Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Health check |
| POST | `/api/parse` | Parse HTML to full semantic tree |
| POST | `/api/parse/top` | Parse top-N relevant nodes |
| POST | `/api/click` | Find best clickable element |
| POST | `/api/fill-form` | Map form fields to key/value pairs |
| POST | `/api/extract` | Extract structured data by keys |
| POST | `/api/check-injection` | Check text for prompt injection |
| POST | `/api/wrap-untrusted` | Wrap content in trust markers |
| POST | `/api/memory/create` | Create workflow memory |
| POST | `/api/memory/step` | Add workflow step |
| POST | `/api/memory/context/set` | Set context key/value |
| POST | `/api/memory/context/get` | Get context value |

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
│   ├── memory.rs       # Workflow memory – stateless context across WASM
│   ├── types.rs        # Core data structures
│   └── bin/
│       └── server.rs   # HTTP API server (axum) for deployment
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

| Phase | Description |
|-------|-------------|
| **Fas 4** | CDP fallback for JavaScript-heavy SPAs (React, Next.js, Vue) |
| **Fas 5** | Hybrid vision fallback (Gemini 2.5 Pro bounding boxes for unlabeled elements) |
| **Fas 6** | Open source launch, WebArena benchmarks, community |

### Design Principles

**Security first.** Trust shield is Fas 1, not Fas 5. Every byte from the web is `Untrusted` by default.

**Goal-native perception.** The LLM receives an answer to "what's relevant to my goal right now?" – not a browser view to interpret.

**No JavaScript required for MVP.** AetherAgent targets static HTML and SSR pages first (~30–40% of the web, including the entire high-value data extraction niche). CDP fallback for SPAs comes in Fas 4.

**Embedded, not remote.** Zero network latency because the engine runs in the same process as the agent.

---

## Benchmark Goals

| Metric | Target | Comparison |
|--------|--------|------------|
| WebArena success rate | >65% | SOTA: 61.7% (IBM CUGA, 2025) |
| Parse time (median page) | <50ms | Playwright: ~800ms |
| Memory per agent | <20MB | Chrome headless: ~150MB |
| WASM binary size | <5MB | — |
| Parallel agents (8GB RAM) | >200 | Playwright: ~5 |

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
