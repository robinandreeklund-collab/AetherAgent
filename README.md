<p align="center">
  <img src="image/AetherAgent.png" alt="AetherAgent" width="400" />
</p>

<h1 align="center">AetherAgent</h1>

<p align="center">
  <strong> AetherAgent is the Rust browser engine that makes AI agents faster, cheaper, safer, and greener.</strong><br>
  Semantic perception, goal-aware intelligence, and prompt injection protection — in a single embeddable Rust/WASM library.
</p>

<p align="center">
  <a href="https://github.com/robinandreeklund-collab/AetherAgent/actions/workflows/ci.yml"><img src="https://github.com/robinandreeklund-collab/AetherAgent/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://www.rust-lang.org"><img src="https://img.shields.io/badge/rust-2021_edition-orange.svg" alt="Rust"></a>
  <a href="https://webassembly.org"><img src="https://img.shields.io/badge/target-wasm32--unknown--unknown-blue.svg" alt="WASM"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="License: MIT"></a>
</p>

<p align="center">
  <a href="https://render.com/deploy?repo=https://github.com/robinandreeklund-collab/AetherAgent">
    <img src="https://render.com/images/deploy-to-render-button.svg" alt="Deploy to Render">
  </a>
</p>

---

## What is AetherAgent?

Instead of handing your LLM 50,000 tokens of raw HTML, AetherAgent delivers ~200 tokens of semantic JSON — the page already understood, filtered, and ranked by relevance to the agent's current goal.

```python
# Without AetherAgent — raw HTML
html = requests.get(url).text
llm.send(html)  # 50,000 tokens, slow, expensive, no structure

# With AetherAgent — one call: fetch + semantic parse
tree = agent.fetch_parse(url, goal="buy cheapest flight")
llm.send(tree)  # 200 tokens, goal-aware, injection-protected
```

**No headless browser. No Chrome process. No V8.** Just Rust compiled to WebAssembly — fetching pages, parsing HTML into semantic accessibility trees with goal-relevance scoring and prompt injection protection — in under 1 ms per page and ~27 MB RAM.

---

## Key Numbers

| | AetherAgent | Playwright | Browser Use | Scrapy |
|---|:-:|:-:|:-:|:-:|
| Startup time | **<1 ms** | ~2,000 ms | ~3,000 ms | ~50 ms |
| Memory | **~27 MB** | ~150 MB | ~200 MB | ~30 MB |
| Semantic understanding | **Yes** | No | Partial | No |
| Prompt injection protection | **Yes** | No | No | No |
| Embeddable in WASM | **Yes** | No | No | No |
| Full JS (V8) | No | Yes | Yes | No |

> AetherAgent is not a Chrome replacement. For JS-heavy SPAs, pair it with a headless browser for rendering, then feed the HTML to AetherAgent. For static/SSR pages (~80% with tiered architecture), it works fully standalone.

---

## Quick Start

```bash
git clone https://github.com/robinandreeklund-collab/AetherAgent.git
cd AetherAgent

# Run tests
cargo test && cargo clippy -- -D warnings && cargo fmt --check

# HTTP server (65 endpoints)
cargo run --features server --bin aether-server

# MCP server (30 tools — for Claude Desktop, Cursor, VS Code)
cargo run --features mcp --bin aether-mcp

# WASM library
wasm-pack build --target web --release
```

> Full build guide with feature flags, deployment, and bootstrap scripts: **[docs/building.md](docs/building.md)**

---

## Core Capabilities

| Capability | Description |
|-----------|-------------|
| **Semantic Perception** | HTML → accessibility tree with roles, labels, goal-relevance scores |
| **Trust Shield** | 20+ prompt injection patterns detected at parse time (EN + SV) |
| **Intent API** | `find_and_click`, `fill_form`, `extract_data` — goal-oriented actions |
| **Semantic Diff** | 80–99% token savings between page snapshots |
| **JS Sandbox** | QuickJS with DOM bridge, 55+ DOM methods, event loop |
| **SSR Hydration** | Extract data from 10 frameworks without running JS |
| **Semantic Firewall** | 3-level URL filtering (blocklist → MIME → semantic relevance) |
| **Intent Compiler** | Decompose goals into action plans with dependencies |
| **Causal Graph** | Model state transitions, predict outcomes, find safest paths |
| **Vision** | YOLOv8-nano screenshot analysis + Blitz rendering (pure Rust) |
| **Workflow Engine** | Multi-page orchestration with auto-nav, rollback, sessions |
| **Hybrid Scoring** | BM25 → HDC → Neural 3-stage retrieval pipeline |
| **ColBERT Reranker** | Optional MaxSim late interaction — 41% higher node quality |
| **Adaptive Streaming** | LLM-directed DOM exploration, 95–99% token savings |
| **Cross-Agent Collab** | Shared diff store for multi-agent workflows |
| **XHR Interception** | Discover hidden `fetch()`/XHR API endpoints |
| **WebMCP Discovery** | Detect W3C WebMCP tool registrations |

> Full feature documentation with all functions and modules: **[docs/features.md](docs/features.md)**

---

## API Surface

- **62 WASM functions** — embeddable in any WASM host
- **65 HTTP endpoints** — deployable Axum server
- **30 MCP tools** — Claude Desktop, Cursor, VS Code
- **Python + Node.js SDKs**

```bash
# Example: parse a page
curl -X POST http://localhost:3000/api/parse \
  -H "Content-Type: application/json" \
  -d '{"html": "<button>Buy now</button>", "goal": "buy product", "url": "https://shop.com"}'
```

```python
from bindings.python.aether_agent import AetherAgent
agent = AetherAgent(base_url="http://localhost:3000")
tree = agent.fetch_parse("https://example.com", goal="buy cheapest flight")
```

> Full API reference (all endpoints, MCP tools, Claude Desktop setup, SDKs): **[docs/api-reference.md](docs/api-reference.md)**

---

## Performance

| Benchmark | AetherAgent | Lightpanda | Speedup |
|-----------|-------------|------------|---------|
| 100 page loads (Campfire) | **171 ms** | 31,165 ms | **183x** |
| 100 page crawl (Amiibo) | **102 ms** | 26,541 ms | **259x** |
| Single parse (simple) | **760 us** | 253 ms | **333x** |
| 100 concurrent parses | **142 ms** | 785 ms | **6x** |

**427 tests** (256 unit + 30 fixture + 49 integration + 13 benchmarks), all passing.

> Full benchmarks, test breakdown, and live site results: **[docs/testing.md](docs/testing.md)**

---

## Architecture

```
  LLM Agent (Claude / GPT / Llama / Gemini)
       │  goal-aware JSON (~200 tokens)
       ▼
  AetherAgent Core (Rust → WASM)
  ┌─────────────────────────────────────────────┐
  │  Progressive Escalation (Tier 0→4)          │
  │  Parser → Arena DOM → Semantic Tree         │
  │  Trust Shield · Intent API · Diff Engine    │
  │  JS Sandbox · Firewall · Causal Graph       │
  │  Vision · Streaming · Workflows             │
  │  28 modules · 62 WASM · 65 HTTP · 30 MCP   │
  └─────────────────────────────────────────────┘
       │
  Runtime: WASM │ HTTP (Axum) │ MCP │ Python │ Node.js
```

> Full architecture diagram and tier breakdown: **[docs/architecture.md](docs/architecture.md)**

---

## Documentation

| Document | Contents |
|----------|----------|
| **[docs/features.md](docs/features.md)** | All 21 features with functions, modules, and usage |
| **[docs/api-reference.md](docs/api-reference.md)** | 65 HTTP endpoints, 30 MCP tools, SDKs, Claude Desktop setup |
| **[docs/building.md](docs/building.md)** | Feature flags, build matrix, deployment, project structure |
| **[docs/testing.md](docs/testing.md)** | Test suite, benchmarks, performance data, live site results |
| **[docs/architecture.md](docs/architecture.md)** | System diagram, tier breakdown, comparison table |
| **[docs/dom-api-coverage.md](docs/dom-api-coverage.md)** | 55+ DOM methods, CSS selectors, framework coverage |
| **[docs/dom-implementation-status.md](docs/dom-implementation-status.md)** | Rust-native vs JS-polyfill breakdown |
| **[docs/wpt-dashboard.md](docs/wpt-dashboard.md)** | Web Platform Tests results (69.0% dom/ pass rate) |

---

## License

MIT
