# Slaash vs Scrapling — Why They're Not Even in the Same Category

*2026-04-14*

## The core problem

Every time an AI agent visits a web page, it receives the full HTML source. A typical page is **50,000-500,000 characters**. A Wikipedia article can exceed **2.7 million characters**. A React SPA can be **1.3 million+**. Most LLM context windows can't even hold these pages — and the ones that can charge you dearly for every token.

**Raw HTML to an LLM costs $0.50-$3.50 per page.** For an agent processing 1,000 pages/day, that's **$4,000/day** in input tokens alone. $1.46M/year.

Slaash reduces that to **$2/day**. $730/year. Same pages, correct answers.

## What Scrapling does

[Scrapling](https://github.com/D4Vinci/Scrapling) (37k+ stars) is a Python scraping framework. It does three things well:

1. **Fast CSS/XPath extraction** via lxml (C backend)
2. **Anti-bot bypass** — TLS fingerprint impersonation, Cloudflare Turnstile bypass
3. **Adaptive element tracking** — finds elements even after site redesigns

But it returns **the data you asked for with a CSS selector**. Not the answer to a question.

## Why Scrapling doesn't solve the problem

```python
# Scrapling — you write the selector, you get the data
page.css('.price::text')
# Returns: ["$79.99"]
# But: What if the class changes? What about a site you've never seen?

# Slaash — you ask the question, you get the answer
parse_crfr(goal="price cost amount $")
# Returns: the 3 price nodes, ranked by relevance, from any site
```

Scrapling requires you to **know the page structure in advance**. That works for scraping known sites. It doesn't work for AI agents encountering arbitrary pages for the first time.

More fundamentally: **Scrapling still returns raw data to the LLM**. It extracts HTML elements — the agent still has to parse, interpret, and reason about the output. The token cost shifts, it doesn't disappear.

## The numbers

### Token output — same page, same question

| Tool | Output (Hacker News) | Goal-aware |
|------|---------------------|:----------:|
| Lightpanda (headless) | 79,406 tokens | No |
| Raw HTML | 8,694 tokens | No |
| Scrapling | ~8,000 tokens (full extract) | No |
| **Slaash** | **523 tokens** | **Yes** |

Slaash doesn't just compress. It **answers**. 523 tokens of ranked, relevant nodes vs 79,406 tokens of everything on the page.

### Parse speed (5,000 elements)

| Library | Median | What it does |
|---------|--------|-------------|
| Scrapling | 15.75 ms | DOM parse + CSS selector match |
| Parsel/lxml | 18.96 ms | DOM parse + CSS selector match |
| Slaash CRFR (cold) | 83.95 ms | Parse + semantic roles + BM25/HDC scoring + goal ranking |
| Slaash CRFR (cached) | **< 1 ms** | Re-query from resonance field cache |

Scrapling is ~5x faster on a cold first parse — because it does fundamentally less work. It parses HTML. Slaash **understands** HTML: identifies roles, scores relevance, detects injection, builds a semantic tree.

On repeat queries (which is the common case for agents revisiting pages), Slaash's CRFR cache makes it **15x faster** than Scrapling.

### Cost at scale

| Metric | Raw HTML | Scrapling | Slaash |
|--------|----------|-----------|--------|
| Tokens per page (avg) | ~50,000 | ~8,000 | ~500 |
| Cost per 1K pages/day | $4,000 | ~$640 | **$2** |
| Cost per year | $1.46M | ~$234K | **$730** |
| Goal-aware ranking | No | No | Yes |
| Prompt injection protection | No | No | Yes |
| Learns from feedback | No | No | Yes |

## The real comparison

| | Scrapling | Slaash |
|---|---|---|
| **What it is** | Scraping framework for developers | Perception layer for AI agents |
| **Input** | CSS selector you write | Natural language goal |
| **Output** | HTML elements matching selector | Ranked semantic nodes answering the question |
| **Token efficiency** | Returns matched elements (~8K tokens) | Returns only signal (~500 tokens, 99.9% reduction) |
| **Unknown sites** | Need new selectors per site | Works on any site, any structure |
| **Learning** | None — every query starts from zero | CRFR feedback — improves with use |
| **Safety** | None — raw content passed through | Trust Shield — injection detected at parse time |
| **JS execution** | Playwright (external process, ~150MB) | QuickJS (embedded, sandboxed, 1.8MB) |
| **Anti-bot** | TLS fingerprinting, Cloudflare bypass | Not a focus (different layer) |
| **Deployment** | Python runtime required | 1.8 MB Rust binary, runs anywhere (WASM, edge, embedded) |

## What Scrapling does better

Let's be honest: Scrapling is better at one thing — **getting past anti-bot defenses**. Its `curl_cffi` TLS fingerprint impersonation and Cloudflare Turnstile bypass are state-of-the-art for Python. Slaash doesn't try to solve this problem — it's a different layer.

For sites with aggressive bot protection, the right architecture is: **Scrapling/curl_cffi fetches the HTML → Slaash distills it**. They can be complementary.

## Why this matters for the future

The question isn't "which parses HTML faster." The question is: **what does the AI agent actually need?**

An AI agent doesn't need HTML. It doesn't need CSS selectors. It doesn't need every div and span on the page. It needs **the answer to its question**, in as few tokens as possible, verified as safe.

That's what Slaash builds. Not a faster scraper — a perception layer that reduces the entire web to the 1% that matters, in under 1 millisecond, without a GPU, and protects against adversarial content doing it.

Pages grow 9.5% per year. LLM context is expensive. The answer isn't bigger context windows or cheaper models — it's **sending less data**.

> Full technical deep-dive: [scrapling-analysis-benchmark.md](scrapling-analysis-benchmark.md)
>
> Benchmark script: [../tests/bench_vs_scrapling.py](../tests/bench_vs_scrapling.py)
>
> Raw results: [../tests/bench_scrapling_results.json](../tests/bench_scrapling_results.json)
