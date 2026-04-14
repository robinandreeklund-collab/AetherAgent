# AetherAgent vs Scrapling

## How Does AetherAgent Compare to Scrapling?

[Scrapling](https://github.com/D4Vinci/Scrapling) (37k+ stars) is the leading Python scraping framework. It's fast, has excellent anti-bot features, and is great at extracting data from known page structures. Here's how it compares:

### Key Numbers

| | AetherAgent | Scrapling | Playwright | Browser Use | Scrapy |
|---|:-:|:-:|:-:|:-:|:-:|
| Startup time | **<1 ms** | ~15 ms | ~2,000 ms | ~3,000 ms | ~50 ms |
| Memory | **~27 MB** | ~50 MB | ~150 MB | ~200 MB | ~30 MB |
| Semantic understanding | **Yes** | No | No | Partial | No |
| Goal-driven ranking | **Yes** | No | No | No | No |
| Prompt injection protection | **Yes** | No | No | No | No |
| Anti-bot / TLS fingerprint | No | **Yes** | Partial | Partial | No |
| Embeddable in WASM | **Yes** | No | No | No | No |
| Full JS (V8) | No | Via Playwright | Yes | Yes | No |

### Parse Speed (5,000 elements)

| Library | Median | What it does |
|---------|--------|-------------|
| **Scrapling** | **15.75 ms** | DOM parse + CSS selector match (lxml) |
| Parsel/lxml | 18.96 ms | DOM parse + CSS selector match |
| **AetherAgent CRFR** | **83.95 ms** | DOM parse + semantic roles + BM25/HDC scoring + goal ranking |
| AetherAgent (cached) | **< 1 ms** | Re-query same page from CRFR field cache |

> AetherAgent is ~5x slower on first parse because it does fundamentally more work: role identification, accessibility labels, relevance scoring, injection detection. On repeat queries, the CRFR cache makes it **15x faster** than Scrapling.

### The Real Difference: Semantic Understanding

```python
# Scrapling — you must know the CSS selector
page.css('.price::text')  # What if the class changes?

# AetherAgent — you describe what you want
parse_crfr(goal="price cost amount $")  # Finds all prices, any site, any structure
```

Scrapling requires you to **know the page structure**. AetherAgent requires you to **know what you're looking for**. For AI agents encountering unknown websites, this is the difference that matters.

### Feature Comparison

| Capability | Scrapling | AetherAgent |
|-----------|:---------:|:-----------:|
| HTML parsing speed | Faster (lxml C) | Slower first, faster cached |
| Semantic understanding | No | **Yes** — roles, labels, goal-relevance |
| Goal-driven node ranking | No | **Yes** — "find the price" without selectors |
| Causal learning | No | **Yes** — improves with `crfr_feedback` |
| Prompt injection protection | No | **Yes** — Trust Shield (20+ patterns) |
| Anti-bot / TLS fingerprinting | **Yes** (curl_cffi) | No |
| JS evaluation | Playwright (external) | **QuickJS** (embedded, sandboxed) |
| Streaming DOM (token savings) | No | **Yes** — 95-99% savings |
| WASM / browser-embeddable | No | **Yes** |
| Vision (screenshot analysis) | No | **Yes** — YOLOv8 (22 UI classes) |
| MCP tools | 10 | **35+** |

### When to Use Which

- **Scrapling**: You know exactly what data you need, from specific sites, and need anti-bot bypass.
- **AetherAgent**: Your AI agent needs to understand and act on *any* website it encounters, safely.

### Which is More Future-Proof?

**AetherAgent.** The web is moving toward AI agents, not scrapers.

1. **Anti-bot is an arms race** — Scrapling's TLS fingerprinting tricks have a shelf life of months. Sites keep closing down scrapers.
2. **CSS selectors don't scale** — Every new site needs new selectors, manually. Can't be automated.
3. **Zero semantics** — Scrapling doesn't know that `<span class="price">$79.99</span>` is a price. It just knows it matches `.price`.

AetherAgent builds for what comes next: agents that **understand** pages, learn from feedback, protect against injection, and stream only what matters.

> Full technical analysis: [scrapling-analysis-benchmark.md](scrapling-analysis-benchmark.md)
>
> Benchmark script: [../tests/bench_vs_scrapling.py](../tests/bench_vs_scrapling.py)
>
> Raw results: [../tests/bench_scrapling_results.json](../tests/bench_scrapling_results.json)
