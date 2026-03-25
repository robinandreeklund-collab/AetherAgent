# Building & Deploying

> Build configurations, deployment, and development setup for AetherAgent.
> For a summary, see the [main README](../README.md).

---

## Quick Start

### One-command bootstrap (WSL/Linux)

```bash
git clone https://github.com/robinandreeklund-collab/AetherAgent.git
cd AetherAgent
chmod +x tools/bootstrap_wsl.sh && ./tools/bootstrap_wsl.sh
```

With vision training: `./tools/bootstrap_wsl.sh --with-vision`

See `./tools/bootstrap_wsl.sh --help` for flags (`--skip-node`, `--skip-python`, `--skip-wasm`, `--skip-tests`).

### Manual setup

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack

git clone https://github.com/robinandreeklund-collab/AetherAgent.git
cd AetherAgent

cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

---

## Feature Flags

| Feature | What it enables | Key crates |
|---------|----------------|------------|
| `server` | HTTP API server (Axum) + all core features | axum, tokio, tower-http |
| `mcp` | MCP stdio server (rmcp) + all core features | rmcp, tokio |
| `cdp` | Chrome DevTools Protocol (Tier 2 rendering) | headless_chrome |
| `fetch` | HTTP fetching, cookies, robots.txt, SSRF protection | reqwest, robotstxt, governor |
| `vision` | YOLOv8 screenshot analysis (ONNX Runtime) | ort, ndarray, image |
| `blitz` | Pure Rust browser engine (HTML → PNG) | blitz-html, blitz-dom |
| `js-eval` | QuickJS JavaScript sandbox | rquickjs |

> `server` and `mcp` are umbrella features — they include `blitz`, `vision`, `fetch`, `js-eval`, and `base64` automatically.

---

## Binaries

| Binary | Feature | Description |
|--------|---------|------------|
| `aether-server` | `server` | HTTP API on port 3000 (65+ endpoints) |
| `aether-mcp` | `mcp` | MCP stdio server (Claude Desktop, Cursor, VS Code) |
| `aether-bench` | *(none)* | Benchmark runner |

---

## Common Commands

```bash
# HTTP server with everything (recommended)
cargo run --release --features "server cdp" --bin aether-server

# MCP stdio server
cargo run --release --features "mcp cdp" --bin aether-mcp

# HTTP server without Chrome
cargo run --release --features server --bin aether-server

# Minimal: just fetch + parse
cargo build --release --features fetch

# WASM library (core only)
wasm-pack build --target web --release

# Run benchmarks
cargo run --release --bin aether-bench

# Run all tests
cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

---

## Build Matrix

| Capability | `server` | `server cdp` | `mcp` | `mcp cdp` | `fetch` only |
|-----------|:---:|:---:|:---:|:---:|:---:|
| HTTP API (65 endpoints) | Yes | Yes | — | — | — |
| MCP via `/mcp` (HTTP) | Yes | Yes | — | — | — |
| MCP via stdio | — | — | Yes | Yes | — |
| Blitz screenshots | Yes | Yes | Yes | Yes | — |
| Chrome screenshots | — | Yes | — | Yes | — |
| YOLOv8 vision | Yes | Yes | Yes | Yes | — |
| JS sandbox (QuickJS) | Yes | Yes | Yes | Yes | — |
| HTTP fetch + cookies | Yes | Yes | Yes | Yes | Yes |
| Core parse/diff/intent | Yes | Yes | Yes | Yes | Yes |

> **Tip:** For Claude Desktop / Cursor / VS Code, use `aether-mcp` (stdio). For HTTP clients, use `aether-server`.

---

## Deploy to Render

[![Deploy to Render](https://render.com/images/deploy-to-render-button.svg)](https://render.com/deploy?repo=https://github.com/robinandreeklund-collab/AetherAgent)

```bash
curl https://your-app.onrender.com/health

curl -X POST https://your-app.onrender.com/api/parse \
  -H "Content-Type: application/json" \
  -d '{"html": "<button>Buy now</button>", "goal": "buy product", "url": "https://shop.com"}'
```

---

## Dependencies

```toml
# Core (always included)
html5ever = "0.27"          # HTML5 spec-compliant parser
slotmap = "1.0"             # Arena DOM
serde = "1.0"               # Serialization
serde_json = "1.0"          # JSON
wasm-bindgen = "0.2"        # WASM interop

# Optional (feature-gated)
rquickjs = "0.11"           # JS sandbox (js-eval)
reqwest = "0.12"            # HTTP client (fetch)
axum = "0.7"                # HTTP server (server)
rmcp = "1.2"                # MCP protocol (mcp)
rten = "0.15"               # ONNX runtime for YOLOv8 (vision)
blitz-html = "0.2"          # Pure Rust browser engine (blitz)
```

---

## Project Structure

```
AetherAgent/
├── src/
│   ├── lib.rs            # WASM API surface — 62 public functions
│   ├── parser.rs         # html5ever HTML parser
│   ├── arena_dom.rs      # SlotMap Arena DOM
│   ├── semantic.rs       # Accessibility tree, goal-relevance scoring
│   ├── trust.rs          # Prompt injection detection (20+ patterns)
│   ├── intent.rs         # find_and_click, fill_form, extract_data
│   ├── diff.rs           # Semantic DOM diffing
│   ├── js_eval.rs        # QuickJS sandbox
│   ├── js_bridge.rs      # Selective execution, DOM targeting
│   ├── dom_bridge.rs     # QuickJS DOM bridge
│   ├── hydration.rs      # SSR hydration extraction (10 frameworks)
│   ├── escalation.rs     # Progressive tier selection (Tier 0→4)
│   ├── temporal.rs       # Time-series memory, adversarial detection
│   ├── compiler.rs       # Intent compiler, goal decomposition
│   ├── fetch.rs          # HTTP fetching, SSRF, robots.txt
│   ├── firewall.rs       # L1/L2/L3 semantic firewall
│   ├── causal.rs         # Causal action graph
│   ├── webmcp.rs         # WebMCP tool discovery
│   ├── grounding.rs      # Multimodal grounding, IoU matching
│   ├── collab.rs         # Cross-agent semantic diff store
│   ├── intercept.rs      # XHR network interception
│   ├── streaming.rs      # Streaming parse with early-stopping
│   ├── vision.rs         # YOLOv8-nano inference (feature: vision)
│   ├── session.rs        # Session cookies, OAuth 2.0
│   ├── orchestrator.rs   # Multi-page workflow engine
│   ├── memory.rs         # Workflow memory persistence
│   ├── types.rs          # Core data structures
│   └── bin/
│       ├── server.rs     # Axum HTTP API (65 endpoints)
│       └── mcp_server.rs # MCP server (30 tools, stdio)
├── tests/
│   ├── integration_test.rs
│   ├── fixture_tests.rs
│   └── fixtures/            # 20 HTML test pages
├── benches/
├── bindings/
│   ├── python/
│   └── node/
├── docs/
├── Dockerfile
├── render.yaml
└── Cargo.toml
```
