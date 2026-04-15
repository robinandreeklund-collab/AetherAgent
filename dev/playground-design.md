# Slaash Developer Playground — Design & Analysis

*2026-04-15*

## Part 1: SDK Naming & Agent Primitives

### The Slaash Language

Every tool call follows one pattern:

```json
{
  "tool": "slaash.<verb>",
  "input": { ... }
}
```

The verb IS the primitive. No namespaces, no versioning in the path. Just `slaash.<what-you-want-to-do>`.

### The 12 Primitives

| Primitive | What it does | Maps to MCP tool(s) |
|-----------|-------------|---------------------|
| `slaash.extract` | Core extraction — URL + goal → ranked answer nodes | `parse_crfr`, `parse`, `parse_hybrid` |
| `slaash.learn` | Teach the system which nodes were correct | `crfr_feedback`, `crfr_save`, `crfr_load`, `crfr_transfer` |
| `slaash.links` | Discover and rank links on a page | `extract_links` (from `discover`) |
| `slaash.explore` | Find hidden API endpoints (XHR/fetch) | `discover` (XHR detection part) |
| `slaash.crawl` | Multi-page intelligent crawling | `adaptive_crawl` |
| `slaash.inspect` | Debug — view raw semantic tree, trust analysis | `parse` (verbose mode), `secure` |
| `slaash.search` | Web search (DuckDuckGo) + optional deep parse | `search` |
| `slaash.stream` | Token-efficient streaming for huge pages | `stream` |
| `slaash.act` | Click buttons, fill forms, extract structured data | `act` |
| `slaash.plan` | Decompose goals into action steps | `plan` |
| `slaash.render` | Screenshot a page (pure Rust or Chrome) | `vision` |
| `slaash.diff` | Compare two page snapshots, get only changes | `diff` |

### Why These 12?

They map to the **agent workflow loop**:

```
1. slaash.search   → find relevant URLs
2. slaash.extract  → get the answer from a page
3. slaash.learn    → teach what was useful
4. slaash.links    → discover more pages
5. slaash.crawl    → follow the trail
6. slaash.act      → interact with the page
7. slaash.plan     → figure out multi-step workflows
8. slaash.stream   → handle huge pages efficiently
9. slaash.explore  → find hidden API endpoints
10. slaash.render  → see what the page looks like
11. slaash.diff    → detect what changed
12. slaash.inspect → debug when things go wrong
```

### SDK Naming: `slaash`

The Python/JS SDK is called `slaash`. Not `slaash-sdk`, not `slaash-client`. Just `slaash`.

```bash
pip install slaash
npm install slaash
```

```python
from slaash import Slaash

s = Slaash(api_key="sk-...")

# Extract
result = s.extract("https://news.ycombinator.com", goal="top tech stories")

# Learn
s.learn(url="https://news.ycombinator.com", goal="top tech stories", node_ids=[5, 12])

# Search + extract
results = s.search("latest AI news", deep=True)

# Crawl
pages = s.crawl("https://docs.python.org/3/", goal="async programming guide", max_pages=5)
```

```javascript
import Slaash from 'slaash'

const s = new Slaash({ apiKey: 'sk-...' })

const result = await s.extract('https://news.ycombinator.com', { goal: 'top tech stories' })
const pages = await s.crawl('https://docs.python.org/3/', { goal: 'async guide', maxPages: 5 })
```

### API Base URL

```
https://api.slaash.ai/v1/<primitive>
```

Examples:
```
POST https://api.slaash.ai/v1/extract
POST https://api.slaash.ai/v1/learn
POST https://api.slaash.ai/v1/search
POST https://api.slaash.ai/v1/crawl
POST https://api.slaash.ai/v1/act
POST https://api.slaash.ai/v1/stream
POST https://api.slaash.ai/v1/plan
POST https://api.slaash.ai/v1/render
POST https://api.slaash.ai/v1/diff
POST https://api.slaash.ai/v1/links
POST https://api.slaash.ai/v1/explore
POST https://api.slaash.ai/v1/inspect
```

All POST. All JSON. All with `Authorization: Bearer sk-...` header.

---

## Part 2: Playground UI Layout & UX

### Tavily vs Slaash — What to Copy, What to Do Better

**Tavily's approach** (from the screenshot):
- Left panel: form with tabs (Search/Extract/Crawl/Research)
- Right panel: code examples (Python/JS/Shell)
- Sidebar: navigation (Overview, API Playground, Use Cases, Billing, Settings, Docs, MCP)
- Top bar: API status indicator ("Operational")

**What works in Tavily's design:**
- Split layout — try + code side by side
- Tab-based tool switching (maps perfectly to our 12 primitives)
- "Try an example" button with pre-filled queries
- Code auto-generates from form inputs
- Clean, minimal, developer-focused

**What Slaash should do differently:**
- **Live response panel** — Tavily only shows code, we show the ACTUAL response inline with token savings metrics
- **Learning feedback loop** — after getting results, a "Was this correct?" button that calls `slaash.learn`
- **Visual diff** — for `slaash.diff`, show side-by-side before/after
- **Streaming mode** — for `slaash.stream`, show nodes appearing in real-time
- **Token savings counter** — prominent display: "52,000 tokens → 487 tokens (99.1% savings)"

### Page Layout

```
┌─────────────────────────────────────────────────────────────────────┐
│  sl/sh   Docs   Playground   MCP   Pricing          [Sign in]      │
├───────┬─────────────────────────────────────────────────────────────┤
│       │  Slaash API Playground           ● Operational    v1       │
│ Side  │─────────────────────────────────────────────────────────────│
│ bar   │  [Extract] [Search] [Crawl] [Act] [Plan] [More ▾]         │
│       │─────────────────────────────────────────────────────────────│
│ Over  │   LEFT: Request Builder     │  RIGHT: Response + Code      │
│ view  │                             │                              │
│       │  URL  [________________]    │  ┌─ Response  Code ──────┐   │
│ Play  │                             │  │                       │   │
│ ground│  Goal [________________]    │  │  523 tokens (99.1%↓)  │   │
│       │                             │  │                       │   │
│ Docs  │  ▸ Advanced options         │  │  #1 [heading] ...     │   │
│       │    top_n: 10                │  │  #2 [link] ...        │   │
│ MCP   │    format: json ▾           │  │  #3 [text] ...        │   │
│       │    run_js: □                │  │                       │   │
│ Keys  │                             │  │                       │   │
│       │  [▶ Run]  [Try example]     │  └───────────────────────┘   │
│ Usage │                             │                              │
│       │  ▸ Was this helpful?        │  Python  JS  Shell  cURL     │
│       │    [✓ Yes] [✗ No]           │  ┌───────────────────────┐   │
│       │    Teaches slaash.learn     │  │ from slaash import... │   │
│       │                             │  │ s = Slaash("sk-...")  │   │
│       │                             │  │ r = s.extract(...)    │   │
│       │                             │  └───────────────────────┘   │
├───────┴─────────────────────────────────────────────────────────────┤
│  Footer: API Status • Docs • GitHub • Discord                      │
└─────────────────────────────────────────────────────────────────────┘
```

### Tab → Primitive Mapping

Main tabs (always visible):
| Tab | Primitive | Description shown |
|-----|-----------|-------------------|
| **Extract** | `slaash.extract` | "URL + question → answer" |
| **Search** | `slaash.search` | "Web search → parsed results" |
| **Crawl** | `slaash.crawl` | "Multi-page intelligent crawl" |
| **Act** | `slaash.act` | "Click, fill forms, extract data" |
| **Plan** | `slaash.plan` | "Decompose goals into steps" |

"More" dropdown:
| Tab | Primitive | Description |
|-----|-----------|-------------|
| Stream | `slaash.stream` | "Token-efficient streaming for large pages" |
| Diff | `slaash.diff` | "Compare page snapshots" |
| Render | `slaash.render` | "Screenshot a page" |
| Links | `slaash.links` | "Discover and rank links" |
| Explore | `slaash.explore` | "Find hidden API endpoints" |
| Inspect | `slaash.inspect` | "Debug: raw tree + trust analysis" |
| Learn | `slaash.learn` | "Teach the system (feedback)" |

### Rate Limiting for Open Playground

**Without API key (anonymous/playground):**
- 10 requests per minute per IP
- 50 requests per day per IP
- Max URL size: 1MB (prevents abuse with huge pages)
- No crawl (requires API key — too expensive)
- No stream (requires API key)
- Response truncated to first 10 nodes
- Watermark in response: `"playground": true, "upgrade": "api.slaash.ai/keys"`

**With API key (free tier):**
- 100 requests per minute
- 1,000 requests per day
- All primitives available
- Full response (no truncation)
- Dashboard with usage stats

**Paid tiers (future):**
- Pro: 10,000 req/day, priority queue, webhook callbacks
- Enterprise: unlimited, SLA, dedicated instance

### Sidebar Navigation

```
📊 Overview        — usage stats, recent queries, quick metrics
⚡ API Playground  — the interactive tool (current page)
📖 Documentation   — full API reference
🔌 MCP Setup       — Claude/Cursor/VS Code integration guide
🔑 API Keys        — manage keys, view usage
📈 Usage           — detailed analytics, cost tracking
⚙️ Settings        — account, notifications, preferences
```

### Pre-filled Examples per Primitive

**Extract:**
- "What is the latest Bitcoin price?" → coinmarketcap.com
- "Find the main headline" → bbc.com
- "What are the system requirements?" → docs.docker.com/get-docker

**Search:**
- "Latest AI research papers 2026"
- "Best Rust web frameworks comparison"
- "Sweden election results 2026"

**Crawl:**
- "Python async programming tutorial" → docs.python.org/3/ (max 5 pages)
- "Rust ownership explained" → doc.rust-lang.org/book/ (max 3 pages)

**Act:**
- "Click 'Sign Up'" → example signup page
- "Fill login form" → example login page
- "Extract all product prices" → example e-commerce page

---

## Part 3: API Endpoint Details

### Request/Response Format

Every endpoint follows the same shape:

**Request:**
```json
POST https://api.slaash.ai/v1/extract
Authorization: Bearer sk-...
Content-Type: application/json

{
  "url": "https://news.ycombinator.com",
  "goal": "top tech stories links articles programming",
  "options": {
    "top_n": 10,
    "format": "json",
    "run_js": false,
    "follow_links": false
  }
}
```

**Response:**
```json
{
  "id": "req_abc123",
  "primitive": "extract",
  "status": "success",
  "data": {
    "nodes": [...],
    "node_count": 8,
    "total_dom_nodes": 494,
    "token_savings": "99.1%",
    "tokens_in": 52000,
    "tokens_out": 487
  },
  "meta": {
    "parse_time_ms": 42,
    "cache_hit": false,
    "url": "https://news.ycombinator.com",
    "title": "Hacker News"
  },
  "usage": {
    "requests_today": 47,
    "requests_limit": 1000
  }
}
```

### Per-Primitive Request Bodies

**slaash.extract:**
```json
{
  "url": "...",           // required: URL to extract from
  "goal": "...",          // required: what you're looking for
  "options": {
    "top_n": 10,          // max nodes (default: 10)
    "format": "json",     // "json" or "markdown"
    "run_js": false,      // evaluate JavaScript
    "follow_links": false // auto-follow relevant links
  }
}
```

**slaash.learn:**
```json
{
  "url": "...",           // required: URL that was extracted
  "goal": "...",          // required: goal used during extraction
  "node_ids": [5, 12]    // required: IDs of nodes that had the answer
}
```

**slaash.search:**
```json
{
  "query": "...",         // required: search query
  "options": {
    "deep": true,         // fetch+parse each result (default: true)
    "max_results": 5,     // number of results (default: 5)
    "nodes_per_result": 3 // nodes extracted per result page
  }
}
```

**slaash.crawl:**
```json
{
  "url": "...",           // required: start URL
  "goal": "...",          // required: what you're gathering
  "options": {
    "max_pages": 10,      // page limit (default: 10)
    "max_depth": 3,       // link depth (default: 3)
    "nodes_per_page": 10  // top nodes per page (default: 10)
  }
}
```

**slaash.act:**
```json
{
  "url": "...",           // required: page URL
  "action": "click",     // "click", "fill", or "extract"
  "target": "Add to Cart", // button label / form fields / data keys
  "options": { ... }
}
```

**slaash.plan:**
```json
{
  "goal": "Buy the cheapest flight from Stockholm to London",
  "context": { "url": "..." } // optional starting page
}
```

**slaash.render:**
```json
{
  "url": "...",           // required: page to screenshot
  "options": {
    "width": 1280,
    "height": 900,
    "format": "png"       // "png" or "base64"
  }
}
```

**slaash.diff:**
```json
{
  "before": { "html": "..." }, // or "url" for live fetch
  "after": { "html": "..." },
  "goal": "..."               // optional: focus diff on relevant changes
}
```

**slaash.stream:**
```json
{
  "url": "...",
  "goal": "...",
  "options": {
    "max_nodes": 50,
    "min_relevance": 0.3
  }
}
```

**slaash.links:**
```json
{
  "url": "...",
  "goal": "...",           // optional: filter links by relevance
  "options": {
    "max_links": 20
  }
}
```

**slaash.explore:**
```json
{
  "url": "..."             // page to scan for hidden API endpoints
}
```

**slaash.inspect:**
```json
{
  "url": "...",
  "checks": ["injection", "trust", "tree"]  // what to inspect
}
```

---

## Part 4: SDK Code Examples

### Python SDK

```python
# pip install slaash
from slaash import Slaash

s = Slaash(api_key="sk-...")

# ── Extract ──
result = s.extract(
    url="https://news.ycombinator.com",
    goal="top tech stories programming"
)
print(f"Found {result.node_count} nodes ({result.token_savings} saved)")
for node in result.nodes:
    print(f"  [{node.role}] {node.label[:60]}")

# ── Learn ──
s.learn(
    url="https://news.ycombinator.com",
    goal="top tech stories programming",
    node_ids=[result.nodes[0].id, result.nodes[1].id]
)

# ── Search ──
results = s.search("latest AI breakthroughs 2026", deep=True, max_results=3)
for r in results:
    print(f"  {r.title}: {r.url}")

# ── Crawl ──
pages = s.crawl(
    url="https://docs.python.org/3/",
    goal="async programming tutorial",
    max_pages=5
)
for page in pages:
    print(f"  {page.url}: {len(page.nodes)} nodes")

# ── Act ──
action = s.act(
    url="https://shop.example.com",
    action="extract",
    target=["price", "title", "availability"]
)
print(action.data)  # {"price": "$49.99", "title": "Widget", ...}

# ── Plan ──
plan = s.plan("Buy the cheapest flight Stockholm → London")
for step in plan.steps:
    print(f"  {step.order}. {step.action}: {step.description}")
```

### JavaScript SDK

```javascript
// npm install slaash
import Slaash from 'slaash'

const s = new Slaash({ apiKey: 'sk-...' })

// Extract
const result = await s.extract('https://news.ycombinator.com', {
  goal: 'top tech stories programming'
})
console.log(`${result.nodeCount} nodes (${result.tokenSavings} saved)`)

// Learn
await s.learn({
  url: 'https://news.ycombinator.com',
  goal: 'top tech stories programming',
  nodeIds: [result.nodes[0].id]
})

// Search
const results = await s.search('latest AI news', { deep: true })

// Crawl
const pages = await s.crawl('https://docs.python.org/3/', {
  goal: 'async programming',
  maxPages: 5
})
```

### Shell (cURL)

```bash
# Extract
curl -X POST https://api.slaash.ai/v1/extract \
  -H "Authorization: Bearer sk-..." \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://news.ycombinator.com",
    "goal": "top tech stories programming"
  }'

# Learn (after getting node IDs from extract)
curl -X POST https://api.slaash.ai/v1/learn \
  -H "Authorization: Bearer sk-..." \
  -H "Content-Type: application/json" \
  -d '{
    "url": "https://news.ycombinator.com",
    "goal": "top tech stories programming",
    "node_ids": [5, 12]
  }'

# Search
curl -X POST https://api.slaash.ai/v1/search \
  -H "Authorization: Bearer sk-..." \
  -d '{"query": "latest AI news", "options": {"deep": true}}'
```

---

## Part 5: Implementation Priority

### Phase A: Core Playground (ship first)

1. **Landing page** at `/playground` (replaces `/try`)
2. **3 tabs**: Extract, Search, Act
3. **Split layout**: form left, response + code right
4. **Rate limiting**: 10 req/min anonymous, IP-based
5. **Code generation**: Python, JS, cURL auto-generated from form
6. **Pre-filled examples** per tab

### Phase B: Full Primitives

7. All 12 primitives as tabs
8. **Learning UI**: "Was this correct?" feedback button
9. **Streaming mode**: real-time node display for `slaash.stream`
10. **Crawl visualization**: page-by-page results with gain bars

### Phase C: Developer Portal

11. **Sign-up / Sign-in** (email + OAuth)
12. **API key management** at `/keys`
13. **Usage dashboard** at `/usage`
14. **Per-key rate limiting** (replaces IP-based)
15. **Billing integration** (Stripe)

### Phase D: SDK Release

16. **Python SDK** on PyPI (`pip install slaash`)
17. **JavaScript SDK** on npm (`npm install slaash`)
18. **OpenAPI spec** auto-generated from endpoint definitions
19. **Postman collection** for team testing

