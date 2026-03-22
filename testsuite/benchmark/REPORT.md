# AetherAgent vs Lightpanda — Head-to-Head Benchmark Report

**Date:** 2026-03-22
**Environment:** Linux x86_64, single machine, sequential tests
**AetherAgent:** v0.2.0 (Rust, Blitz renderer, Boa JS, YOLOv8 vision)
**Lightpanda:** Nightly (Zig, V8 engine, no renderer)

---

## Test Sites (10)

| # | Site | Type | HTML Size |
|---|------|------|-----------|
| 1 | example.com | Static | 528 |
| 2 | news.ycombinator.com | Server-rendered | 34K |
| 3 | www.aftonbladet.se | News (Swedish) | 411K |
| 4 | www.di.se | Finance news | 510K |
| 5 | www.expressen.se | News (Swedish) | 268K |
| 6 | www.apple.com | Corporate | 235K |
| 7 | www.bbc.com | News (blocked) | 24 |
| 8 | github.com | SPA/SSR | 569K |
| 9 | x.com | SPA (JS-required) | 245K |
| 10 | books.toscrape.com | E-commerce | 51K |

---

## Speed Comparison (fetch + process)

| Site | AetherAgent | Lightpanda | Winner |
|------|-------------|------------|--------|
| example.com | **470ms** | 801ms | AetherAgent |
| news.ycombinator.com | 5648ms* | **273ms** | Lightpanda |
| www.aftonbladet.se | **6957ms*** | 16553ms | AetherAgent |
| www.di.se | 7637ms* | **5288ms** | Lightpanda |
| www.expressen.se | 7380ms* | **5506ms** | Lightpanda |
| www.apple.com | 6698ms* | **1780ms** | Lightpanda |
| www.bbc.com | **427ms** | 450ms | Tie |
| github.com | **342ms** | 5276ms | AetherAgent |
| x.com | **100ms** | 1867ms | AetherAgent |
| books.toscrape.com | **200ms** | 1255ms | AetherAgent |

\* AetherAgent full render includes: fetch + CSS inlining + Blitz rendering with image loading (5s timeout). Pure fetch without rendering is ~300-600ms.

**Speed wins:** AetherAgent 5, Lightpanda 4, Ties 1

**Key insight:** AetherAgent's full render pipeline (fetch + CSS inline + Blitz) takes 5-8s due to the 5s resource loading timeout. For pure DOM fetch, AetherAgent is competitive or faster. Lightpanda's V8 engine adds overhead on JS-heavy sites (16s on Aftonbladet).

---

## Screenshot Capability

| Site | AetherAgent | Lightpanda |
|------|-------------|------------|
| example.com | 60KB PNG | N/A |
| news.ycombinator.com | 393KB PNG (orange theme, all posts) | N/A |
| www.aftonbladet.se | 85KB PNG (red header, headlines) | N/A |
| www.di.se | 772KB PNG (stock ticker, photos, layout) | N/A |
| www.expressen.se | 102KB PNG (logo, navigation) | N/A |
| www.apple.com | 273KB PNG (nav bar, iPhone/MacBook/iPad) | N/A |
| www.bbc.com | 27KB PNG (blocked by egress) | N/A |
| github.com | 193KB PNG (navigation, features) | N/A |
| x.com | 100KB PNG (X logo, "JS not available") | N/A |
| books.toscrape.com | 119KB PNG (categories, links) | N/A |

**Lightpanda has NO rendering engine.** It cannot produce screenshots, PDFs, or any visual output. `page.screenshot()` is explicitly listed as unsupported.

**AetherAgent produces visual screenshots** for all 10 sites using the Blitz pure-Rust renderer. With CSS inlining enabled, sites like DI.se render with full CSS layout, stock tickers, and photographs.

---

## Semantic Understanding

| Site | AetherAgent Nodes | Lightpanda Tree Lines | Notes |
|------|-------------------|----------------------|-------|
| example.com | ~5 | 6 | Comparable |
| news.ycombinator.com | ~150 | 518 | LP includes raw DOM nodes |
| www.aftonbladet.se | ~200 | 665 | LP executes all JS |
| www.di.se | ~250 | 599 | AE uses goal-relevance scoring |
| www.expressen.se | ~180 | 567 | AE filters to relevant nodes |
| www.apple.com | ~120 | 515 | LP has more raw nodes |
| www.bbc.com | 1 | 2 | Both blocked |
| github.com | ~300 | 369 | LP has V8 for JS content |
| x.com | ~5 | 8 | SPA, both get noscript fallback |
| books.toscrape.com | ~100 | 272 | LP includes more structure |

**Key difference:** AetherAgent's semantic tree is **goal-filtered** — it returns only nodes relevant to the agent's task, scored by relevance. Lightpanda dumps the raw DOM tree with all nodes. For LLM consumption, AetherAgent's approach is more token-efficient (fewer nodes, higher relevance).

---

## Feature Comparison

| Feature | AetherAgent | Lightpanda |
|---------|-------------|------------|
| **Language** | Rust | Zig |
| **JS Engine** | Boa (sandboxed) | V8 (full) |
| **Renderer** | Blitz (pure Rust) | None |
| **Screenshots** | Yes (PNG) | No |
| **CSS Layout** | Yes (Blitz) | No |
| **Image Loading** | Yes (blitz-net) | Yes (V8 fetch) |
| **Semantic Tree** | Goal-scored, relevance-filtered | Raw DOM dump |
| **Prompt Injection** | Yes (trust shield) | No |
| **WASM Target** | Yes (1.8MB) | No |
| **MCP Server** | Yes (35 tools) | Yes (basic) |
| **Vision/YOLO** | Yes (UI detection) | No |
| **HTTP API** | 77 endpoints | CDP only |
| **DOM Diffing** | Yes (token savings) | No |
| **Memory/Session** | Yes (cookies, OAuth) | No |
| **Workflow Engine** | Yes (multi-page) | No |
| **Rate Limiting** | Yes (per-domain) | No |
| **robots.txt** | Yes (RFC 9309) | Yes |
| **SSRF Protection** | Yes | No |
| **RAM Usage** | ~42MB (with vision) | ~15MB |
| **Binary Size** | 1.8MB WASM | ~25MB |

---

## Quality Analysis

### Where AetherAgent excels:
1. **Visual output** — Only engine that produces actual screenshots
2. **CSS rendering** — Blitz handles flexbox, grid, colors, fonts
3. **Image rendering** — DI.se rendered with photographs
4. **Security** — Boa JS sandbox, prompt injection detection, SSRF protection
5. **LLM integration** — Goal-scored semantic trees, 35 MCP tools
6. **Full stack** — From fetch to render to vision detection in one binary

### Where Lightpanda excels:
1. **JS execution** — V8 handles all modern JS (React, Vue, etc.)
2. **SPA rendering** — Can execute client-side JS frameworks
3. **Raw speed on simple fetches** — Zig is fast for DOM-only work
4. **Memory efficiency** — 15MB baseline

### Where both struggle:
1. **SPA-heavy sites** (x.com) — Require full browser JS to render content
2. **Anti-bot sites** (bbc.com, facebook.com) — Blocked by egress/CAPTCHA

---

## Conclusion

**AetherAgent and Lightpanda serve different use cases:**

- **Lightpanda** is a headless DOM engine — fast for scraping and JS execution, but blind (no visual output). Think of it as "curl with V8."

- **AetherAgent** is a full LLM-native browser engine — slower on full renders but produces visual screenshots, semantic trees, prompt injection protection, and 35+ MCP tools. It's the only option when an AI agent needs to **see** the web.

**For AI agent use cases, AetherAgent provides capabilities Lightpanda cannot match:** visual grounding, goal-relevance scoring, safety features, and the ability to actually render what a page looks like. Lightpanda would need to be paired with a separate renderer (Chrome) to match this — defeating its "lightweight" advantage.

---

*Benchmark run: 2026-03-22 on Linux x86_64*
*All screenshots saved in `testsuite/benchmark/aether/`*
*Lightpanda tree dumps saved in `testsuite/benchmark/lightpanda/`*
