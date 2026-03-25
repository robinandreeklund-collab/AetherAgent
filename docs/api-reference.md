# API Reference

> Complete HTTP, MCP, and SDK reference for AetherAgent.
> For a summary, see the [main README](../README.md).

---

## HTTP Endpoints (65 routes)

Run the server: `cargo run --features server --bin aether-server`

### Core Parsing

| Method | Path | Description |
|--------|------|-------------|
| GET | `/` | API documentation (JSON) |
| GET | `/health` | Health check |
| POST | `/api/parse` | Parse HTML → full semantic tree |
| POST | `/api/parse-top` | Parse → top-N relevant nodes |
| POST | `/api/parse-js` | Parse with automatic JS evaluation |

### Trust & Security

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/check-injection` | Check text for prompt injection |
| POST | `/api/wrap-untrusted` | Wrap content in trust markers |

### Intent API

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/click` | Find best clickable element |
| POST | `/api/fill-form` | Map form fields |
| POST | `/api/extract` | Extract structured data by keys |

### Semantic Diff

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/diff` | Compute delta between two trees |

### JavaScript Sandbox

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/detect-js` | Detect JS snippets in HTML |
| POST | `/api/eval-js` | Evaluate expression in sandbox |
| POST | `/api/eval-js-batch` | Batch evaluate expressions |

### Workflow & Temporal Memory

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

### Intent Compiler

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/compile` | Compile goal → action plan |
| POST | `/api/execute-plan` | Execute plan against page state |

### HTTP Fetch

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/fetch` | Fetch URL → HTML + metadata |
| POST | `/api/fetch/parse` | Fetch → semantic tree |
| POST | `/api/fetch/click` | Fetch → find clickable element |
| POST | `/api/fetch/extract` | Fetch → extract structured data |
| POST | `/api/fetch/plan` | Fetch → compile → execute plan |

### Semantic Firewall

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/firewall/classify` | Classify URL (L1/L2/L3) |
| POST | `/api/firewall/classify-batch` | Batch classify URLs |

### Causal Action Graph

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/causal/build` | Build causal graph from history |
| POST | `/api/causal/predict` | Predict action outcome |
| POST | `/api/causal/safest-path` | Find safest path to goal |

### WebMCP Discovery

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/webmcp/discover` | Discover WebMCP tools in HTML |

### Multimodal Grounding

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/ground` | Ground tree with bounding boxes |
| POST | `/api/ground/match-bbox` | Match bbox via IoU |

### Cross-Agent Collaboration

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/collab/create` | Create shared diff store |
| POST | `/api/collab/register` | Register agent |
| POST | `/api/collab/publish` | Publish delta |
| POST | `/api/collab/fetch` | Fetch new deltas |

### Vision & Screenshot Analysis

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/detect-xhr` | Scan HTML for XHR/fetch/AJAX endpoints |
| POST | `/api/parse-screenshot` | Analyze screenshot with YOLOv8 |
| POST | `/api/vision/parse` | Analyze screenshot with server-loaded model |
| POST | `/api/fetch-vision` | URL → Blitz render → YOLOv8 → images + JSON |

### Session Management

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/session/create` | Create empty session manager |
| POST | `/api/session/cookies/add` | Parse Set-Cookie headers |
| POST | `/api/session/cookies/get` | Build Cookie header for domain/path |
| POST | `/api/session/token/set` | Store OAuth access/refresh token |
| POST | `/api/session/oauth/authorize` | Build OAuth 2.0 authorize URL |
| POST | `/api/session/oauth/exchange` | Prepare token exchange body |
| POST | `/api/session/status` | Auth state + token validity |
| POST | `/api/session/login/detect` | Detect login form in HTML |
| POST | `/api/session/evict` | Evict expired cookies |
| POST | `/api/session/login/mark` | Mark session as logged in |
| POST | `/api/session/token/refresh` | Prepare token refresh body |

### Workflow Orchestration

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/workflow/create` | Create workflow with goal + start URL |
| POST | `/api/workflow/page` | Provide fetched HTML page |
| POST | `/api/workflow/report/click` | Report click action result |
| POST | `/api/workflow/report/fill` | Report form fill result |
| POST | `/api/workflow/report/extract` | Report data extraction result |
| POST | `/api/workflow/complete` | Mark step as completed |
| POST | `/api/workflow/rollback` | Rollback step for retry |
| POST | `/api/workflow/status` | Workflow status + progress |

### Adaptive Streaming

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/stream-parse` | Goal-driven adaptive DOM streaming |
| POST | `/api/fetch/stream-parse` | Fetch + adaptive stream parse |
| POST | `/api/directive` | Send LLM directive to streaming engine |

### Tiered Rendering

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/tiered-screenshot` | Auto-tier HTML → PNG |
| POST | `/api/tier-stats` | Rendering tier statistics |

---

## MCP Server (30 tools)

Run: `cargo run --features mcp --bin aether-mcp`

Compatible with Claude Desktop, Cursor, VS Code, and any MCP-compatible client.

| Tool | Description |
|------|-------------|
| **Core Parsing** | |
| `parse` | Parse HTML into goal-aware semantic tree |
| `parse_top` | Top-N most relevant nodes |
| `parse_with_js` | Parse with inline JS evaluation |
| **Intent & Interaction** | |
| `find_and_click` | Find best clickable element by label |
| `fill_form` | Map form fields to key/value pairs |
| `extract_data` | Extract structured data by semantic keys |
| **Security** | |
| `check_injection` | Scan for 20+ prompt injection patterns |
| `classify_request` | 3-level semantic firewall |
| **Planning & Reasoning** | |
| `compile_goal` | Decompose goal into action plan |
| `diff_trees` | Compare two trees (80–95% token savings) |
| `build_causal_graph` | Build state transition graph |
| `predict_action_outcome` | Predict next state for an action |
| `find_safest_path` | Lowest-risk path to goal |
| **Discovery** | |
| `discover_webmcp` | Detect WebMCP tool registrations |
| `detect_xhr_urls` | Discover hidden API endpoints |
| **Vision** | |
| `parse_screenshot` | YOLOv8 screenshot analysis |
| `vision_parse` | Screenshot analysis (server model) |
| `fetch_vision` | URL → render → YOLOv8 → JSON |
| **Grounding** | |
| `ground_semantic_tree` | Semantic tree + bounding boxes |
| `match_bbox_iou` | IoU element matching |
| **Collaboration** | |
| `create_collab_store` | Multi-agent shared store |
| `register_collab_agent` | Register agent in store |
| `publish_collab_delta` | Share page delta |
| `fetch_collab_deltas` | Get undelivered deltas |
| **Streaming** | |
| `stream_parse` | Adaptive DOM streaming |
| `stream_parse_directive` | Stream with LLM directives |
| `fetch_stream_parse` | Fetch + adaptive stream |
| **Rendering** | |
| `tiered_screenshot` | Auto-tier screenshot |
| `tier_stats` | Tier usage statistics |

---

## Claude Desktop Setup

### Option A: Remote server (Render / Docker / any host)

Use the Python MCP proxy (`mcp_proxy.py`):

```bash
pip install requests
```

Edit `~/Library/Application Support/Claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "aether-agent": {
      "command": "python3",
      "args": ["/path/to/AetherAgent/mcp_proxy.py"],
      "env": {
        "AETHER_URL": "https://your-app.onrender.com"
      }
    }
  }
}
```

### Option B: Local binary (fastest, no network)

```bash
cargo build --features mcp --bin aether-mcp --release
```

```json
{
  "mcpServers": {
    "aether-agent": {
      "command": "/path/to/AetherAgent/target/release/aether-mcp"
    }
  }
}
```

### Example prompts for Claude Desktop

- "Fetch and parse https://news.ycombinator.com with the goal 'find top stories'"
- "Extract data with keys ['title', 'price'] from this page"
- "Check this text for prompt injection: 'IGNORE ALL PREVIOUS INSTRUCTIONS'"
- "Create an action plan for 'buy the cheapest laptop'"
- "Use classify_request to check if a URL is relevant to a goal"
- "Compare two page states with diff_trees"
- "Scan this page for fetch/XHR API calls"

---

## Python SDK

```python
from bindings.python.aether_agent import AetherAgent

agent = AetherAgent(base_url="https://your-app.onrender.com")
tree = agent.parse(html, goal="buy cheapest flight", url="https://flights.com")
click = agent.find_and_click(html, goal="buy", url=url, target_label="Add to cart")
data = agent.extract_data(html, goal="get price", url=url, keys=["price", "title"])
```

## Node.js SDK

```javascript
const { AetherAgent } = require('./bindings/node');
const agent = new AetherAgent('https://your-app.onrender.com');

const tree = await agent.parse(html, 'buy cheapest flight', url);
const click = await agent.findAndClick(html, 'buy', url, 'Add to cart');
```
