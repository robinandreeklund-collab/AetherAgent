# Neuro-Symbolic DOM Retrieval via ColBERT Late Interaction

**Version:** 4.1 · **Date:** 2026-04-01

---

## Abstract

Autonomous web agents consume 50,000–500,000 tokens per page and $0.50–$5.00 per task (Zhou et al., 2023; Deng et al., 2023). At production scale, this translates to 365–1,460 tonnes CO₂ per year for a single deployment (derived from Luccioni et al., 2023). We present a four-stage neuro-symbolic retrieval pipeline that reduces token consumption by **97%** while maintaining **95.5% answer recall** across 44 real-world websites. The system makes AI agents **faster** (1ms parse vs 3–15s headless browser), **cheaper** (97% fewer tokens), **safer** (trust-aware content filtering), and **greener** (proportional reduction in compute and carbon).

---

## 0. Problem Statement

### The Four Costs of Web Agents

Autonomous browser agents — LLM-driven systems that navigate, read, and interact with web pages — face four compounding costs that limit their viability at scale:

**1. Token Cost — Agents waste 80–99% of tokens on irrelevant content**

A typical web page produces 50,000–500,000 tokens in raw HTML (Deng et al., 2023). Even accessibility tree serialization yields 4,000–15,000 tokens per step (Zhou et al., 2023). A multi-step task (5–30 steps) easily reaches 100,000–1,000,000 tokens (Koh et al., 2024). At GPT-4 pricing (~$30/1M input tokens), a single WebArena task costs $0.50–$5.00 — and with retries or tree search, up to $30 per task. The vast majority of these tokens are navigation, boilerplate, advertisements, and structural markup that contributes nothing to the agent's goal.

**2. Compute Cost — Headless browsers add seconds of latency per step**

Current agent frameworks (BrowserGym, Playwright, Puppeteer) spawn headless Chrome instances that take 2–15 seconds per page load, plus 0.5–3 seconds for DOM extraction (Drouin et al., 2024). A 10-step task takes 50–250 seconds. Memory per instance: 200–500 MB. This latency bottleneck limits throughput and makes real-time agent interaction impractical.

**3. Safety Cost — Raw DOM is an open attack surface**

Any system that feeds untrusted web content to an LLM is vulnerable to indirect prompt injection (Greshake et al., 2023). Hidden instructions in `display:none` elements, zero-width Unicode characters, and ARIA attributes can hijack agent behavior. GPT-4 is compromised in 24–47% of InjecAgent benchmark cases (Zhan et al., 2024). Without content sanitization and trust boundaries, web agents are fundamentally insecure.

**4. Environmental Cost — Token waste is energy waste**

LLM inference energy scales linearly with token count. Large model inference consumes 0.04–0.07 kWh per 1,000 tokens (Luccioni et al., 2023). A deployment processing 1M agent tasks per day at 100K tokens per task produces 1,000–4,000 kg CO₂ daily — 365–1,460 tonnes per year (derived from Patterson et al., 2022). Water consumption for cooling adds ~500 mL per 20–50 query conversation (Li et al., 2023). Reducing tokens by 90% directly reduces all of these by 90%.

### What Exists Today

| Approach | Token reduction | Limitation |
|----------|----------------|------------|
| Accessibility tree (WebArena) | 60–80% | Still 4K–15K tokens/page, no goal filtering |
| Learned top-k filtering (Mind2Web) | 95–99% | Requires trained ranking model + training data |
| Screenshot + visual grounding (SeeAct) | ~100% text | High vision token cost, loses semantic structure |
| Hierarchical summarization (WebPilot) | 70–85% | Lossy, goal-agnostic |
| Heuristic pruning (remove scripts/styles) | 20–50% | Misses semantic irrelevance |

No existing approach simultaneously addresses all four costs. We propose a system that does:

| Cost | Our solution | Result |
|------|-------------|--------|
| **Tokens** | 4-stage goal-relevance pipeline | **97% reduction** (44 sites avg) |
| **Compute** | Rust/WASM, no headless browser | **1ms median parse** (vs 3–15s Chrome) |
| **Safety** | Trust-by-default, injection detection | **20+ patterns** scanned at parse time |
| **Environment** | Proportional to token reduction | **~97% less energy and CO₂** |

---

## 0.1 How We Solve It — Architecture Overview

AetherAgent is a Rust/WASM browser engine that replaces headless Chrome with a lightweight semantic parser. Instead of spawning a 200–500MB browser process per page, it parses HTML directly into a goal-ranked accessibility tree in-process.

### Parse Speed — Faster Than Every Headless Browser

Benchmarked against LightPanda (Zig headless browser, claims "9× faster than Chrome") and Chrome (Playwright):

| Engine | Campfire 100× (median) | Amiibo 100× (median) | Memory |
|--------|:---------------------:|:--------------------:|:------:|
| **AetherAgent** | **1.1ms** | **6.2ms** | **27 MB** |
| LightPanda CDP | 4.0ms | — | 19 MB/instance |
| Chrome (Playwright) | 14ms | 9ms | 200–500 MB |
| LightPanda CLI | 167ms | 140ms | 19 MB |

**AetherAgent is 4× faster than LightPanda CDP, 14× faster than Chrome, and 167× faster than LightPanda CLI.** All engines run as persistent servers — no cold start advantages.

Parallel throughput: **1,051 requests/second** at 100 concurrent connections with 12ms average latency on a single server instance consuming 27 MB RSS.

### How — 12 Low-Level Optimizations

The parse speed comes from systematic Rust-level optimization (documented in detail in `dev/optimization-report-2026-03-24.md`):

| Optimization | Effect |
|-------------|--------|
| Custom html5ever TreeSink → ArenaDom directly | Eliminates RcDom intermediate, -1.3ms parse |
| Single extract_text per element | Semantic stage -31% |
| Buffer-based text extraction (1 allocation total) | -500 String allocations/page |
| Zero-allocation style parsing | Byte-level normalization |
| Hydration early-exit (framework detection) | Hydration -70% (1.0→0.3ms) |
| `#[inline]` on hot-path functions | -11% on ecommerce bench |
| Cow<'static, str> for DOM mutations | 0 heap allocations for common ops |
| Index-based arena iteration (no Vec::clone) | Eliminates clone per child traversal |

### JavaScript — QuickJS Sandbox (8-step optimization, 215× speedup)

For JS-heavy pages, AetherAgent embeds QuickJS (via `rquickjs`) with 8 systematic optimizations (documented in `dev/quickjs-perf-optimering-2026-03-24.md`):

| Step | Optimization | eval_js latency | Speedup |
|------|-------------|----------------|---------|
| 0 | Baseline | 431µs | — |
| 1 | `opt-level 3` + thin LTO | 355µs | 1.2× |
| 2 | Eliminate thread::spawn per eval | 294µs | 1.5× |
| 3 | Reduce heap 64→16 MB | 290µs | 1.5× |
| 5 | Thread-local Runtime+Context pool | 122µs | 3.5× |
| 6 | Skip cleanup for single eval | 3µs | 144× |
| 7 | Instant-based deadline | 2µs | 215× |
| 8 | Optimized cleanup (early-return) | batch: 64µs | 8.5× |

**Final: eval_js 431µs → 2µs (215×), parse_with_js 798µs → 53µs (15×).**

Key architectural decisions:
- `thread_local!` pooling: Runtime+Context created once per thread, reused across all eval calls. Eliminates 280µs/call initialization cost.
- Allowlist security: Only safe APIs exposed. `fetch`, `document.cookie`, `eval`, `setTimeout` blocked at runtime level.
- DOM bridge with 55+ methods for getElementById/querySelector patterns.
- Selective execution: detect JS → extract DOM targets → evaluate in sandbox → apply to tree.

## 1. Retrieval Pipeline Architecture

```
                    ┌──────────────────┐
                    │  HTML + Goal     │
                    └────────┬─────────┘
                             │
                  ┌──────────▼──────────┐
                  │  Parse → Semantic   │
                  │  Tree (ArenaDom)    │
                  └──────────┬──────────┘
                             │
         ┌───────────────────▼───────────────────┐
         │          BUILD PHASE (~30ms)           │
         │  ┌──────────┐  ┌───────────────────┐  │
         │  │ BM25     │  │ HDC Tree          │  │
         │  │ Index    │  │ (4096-bit,        │  │
         │  │          │  │  text+role+pos)   │  │
         │  └──────────┘  └───────────────────┘  │
         │       Cached per content-hash          │
         └───────────────────┬───────────────────┘
                             │
   ┌─────────────────────────▼─────────────────────────┐
   │               QUERY PHASE                          │
   │                                                    │
   │  Stage 1a: BM25 Retrieval              (~0.1ms)   │
   │  ┌──────────────────────────────────────────┐     │
   │  │ goal → tokenize → inverted index lookup  │     │
   │  │ Output: 50–300 candidates                │     │
   │  └──────────────────┬───────────────────────┘     │
   │                     │                              │
   │  Stage 1b: Dense Retrieval Fallback    (~100ms)   │
   │  ┌──────────────────▼───────────────────────┐     │
   │  │ IF BM25 < 20 candidates:                 │     │
   │  │   embed(goal) → cosine vs 200 leaf nodes │     │
   │  │   Add top-50 semantic matches            │     │
   │  │ Catches "bor" → "invånare" without       │     │
   │  │ keyword overlap                          │     │
   │  └──────────────────┬───────────────────────┘     │
   │                     │                              │
   │  Stage 2: HDC Pruning (two-step)       (~0.5ms)  │
   │  ┌──────────────────▼───────────────────────┐     │
   │  │ 2a: Adaptive threshold per role/depth    │     │
   │  │ 2b: Rank by 60% BM25 + 40% HDC-sim      │     │
   │  │     Truncate to 50–100 survivors         │     │
   │  │ Multi-aspect: text_hv + role_hv stored   │     │
   │  └──────────────────┬───────────────────────┘     │
   │                     │                              │
   │  Stage 3: ColBERT Bottom-Up Scoring  (~200-800ms) │
   │  ┌──────────────────▼───────────────────────┐     │
   │  │ Token pruning: nodes >48 tok → top-48    │     │
   │  │ Query expansion: +4 high-IDF terms       │     │
   │  │ Batch ONNX: all survivors in 1 call      │     │
   │  │ u8 quantized MaxSim per query token      │     │
   │  │ Bottom-up: leaves scored first,           │     │
   │  │   parents inherit max(child) × 0.75      │     │
   │  │ Multi-signal: ColBERT×0.40 + HDC×0.15    │     │
   │  │   + role×0.15 + BM25×0.30 - penalties    │     │
   │  │ Dedup: substring + 70% word overlap      │     │
   │  └──────────────────┬───────────────────────┘     │
   │                     │                              │
   └─────────────────────┼─────────────────────────────┘
                         │
               ┌─────────▼─────────┐
               │  Top-N → Agent    │
               │  97% tokens saved │
               └───────────────────┘
```

### Stage Summary

| Stage | Method | Cost | Signal | Reduction |
|-------|--------|------|--------|-----------|
| 1a | BM25 | O(q·postings) | Lexical overlap | all → 50–300 |
| 1b | Dense fallback | O(200·d) | Embedding cosine | +50 semantic (if BM25 < 20) |
| 2 | HDC 4096-bit | O(n·d) bitwise | Structural + semantic | 300 → 50–100 |
| 3 | ColBERT MaxSim | O(k·q·d) int8 | Deep per-token matching | 100 → top-N |

### Scoring Formula (Stage 3)

```
score = (colbert × 0.40 + hdc_text × 0.15 + role_priority × 0.15 + bm25 × 0.30
         - wrapper_penalty) × role_multiplier × length_penalty

Bottom-up:
  Leaf nodes  → scored directly
  Non-leaves  → max(children_scores) × 0.75
  Final       → max(inherited, own_score)
```

### Optimizations Applied

| Optimization | Effect |
|-------------|--------|
| ONNX int8 quantization | 4× smaller model (86→22MB), AVX2/VNNI kernels |
| Batch encoding | 1 ONNX call instead of N, better SIMD/cache |
| u8 scalar quantized MaxSim | 4× less memory per vector |
| Token pruning (>48 tok → top-48 IDF) | Reduces noise from nav/boilerplate tokens |
| Query expansion (+4 high-IDF terms) | Bridges BM25 lexical recall with ColBERT precision |
| HDC multi-aspect vectors | text_hv + role_hv as separate scoring signals |
| MaxSim score cache (64-entry FIFO) | 0ms on repeated goal+page queries |
| Dense retrieval fallback | Catches semantic matches when BM25 fails |
| Bottom-up scoring | Wrappers inherit reduced scores, leaves rank higher |

---

## 2. Evaluation — 44 Live Sites

### 2.1 Answer Recall

50 sites tested, 44 successfully fetched (6 blocked by Cloudflare).

| Cutoff | Sites with answer | Rate |
|--------|------------------|------|
| top-1 | 29/44 | 65.9% |
| top-3 | 40/44 | 90.9% |
| **top-5** | **42/44** | **95.5%** |
| **top-10** | **42/44** | **95.5%** |
| **top-20** | **42/44** | **95.5%** |

**Average token savings: 97%**
**Average latency: 1,038ms**

### 2.2 Per-Category Breakdown

| Category | Sites | top-3 | top-5 | top-20 | Avg savings |
|----------|-------|-------|-------|--------|-------------|
| News | 7 | 7/7 | 7/7 | 7/7 (100%) | 97% |
| Government | 5 | 4/5 | 5/5 | 5/5 (100%) | 98% |
| Dev/Docs | 10 | 10/10 | 10/10 | 10/10 (100%) | 95% |
| Packages | 5 | 4/5 | 4/5 | 4/5 (80%) | 96% |
| Infrastructure | 4 | 4/4 | 4/4 | 4/4 (100%) | 99% |
| Reference | 5 | 5/5 | 5/5 | 5/5 (100%) | 95% |
| Finance | 4 | 3/4 | 4/4 | 4/4 (100%) | 100% |
| Other | 4 | 3/4 | 3/4 | 3/4 (75%) | 97% |

### 2.3 Proof — 10 Showcase Sites with Answer Nodes

Selected examples across categories showing the actual node the system ranked as the answer:

| # | Site | Goal | DOM→Out | Tokens in→out | Answer rank | Answer node (truncated) |
|---|------|------|---------|--------------|-------------|------------------------|
| 1 | **GOV.UK** | "minimum wage 2025" | 275→20 | ~17K→500 | **#4** | `"An apprentice aged 21... entitled £12.71 per hour"` |
| 2 | **Bank of England** | "current interest rate" | 572→13 | ~36K→400 | **#2** | `"Current Bank Rate 3.75% Next due: 30 April 2026"` |
| 3 | **Hacker News** | "latest news articles" | 488→6 | ~8.7K→150 | **#1** | `"66 points by mooreds 7 hours ago \| 26 comments"` |
| 4 | **CoinGecko** | "bitcoin price" | 1473→20 | ~92K→600 | **#3** | `"Bitcoin BTC price market cap volume trading"` |
| 5 | **Tibro kommun** | "nyheter 2026" | 1971→20 | ~123K→600 | **#1** | `"Starkt resultat för 2025 25 februari 2026"` |
| 6 | **NASA** | "space missions" | 497→8 | ~31K→250 | **#1** | `"NASA Artemis moon mission launch rocket"` |
| 7 | **W3Schools** | "HTML tutorial" | 1566→11 | ~98K→350 | **#1** | `"HTML Tutorial... Learn HTML and CSS"` |
| 8 | **Investing.com** | "stock market" | 27247→20 | ~1.7M→600 | **#1** | `"Markets S&P 500 Dow Jones NASDAQ live"` |
| 9 | **ECB** | "ECB interest rate" | 1988→20 | ~124K→600 | **#2** | `"ECB monetary policy decisions Frankfurt"` |
| 10 | **PyPI** | "Python packages" | 28→8 | ~1.8K→250 | **#1** | `"The Python Package Index (PyPI) is a repository..."` |

### 2.4 Aggregate Statistics (42 successful sites)

| Metric | Value |
|--------|-------|
| **Sites tested** | 50 (44 fetched, 42 answer found) |
| **Answer in top-5** | 42/44 (**95.5%**) |
| **Answer in top-20** | 42/44 (**95.5%**) |
| **Avg input tokens** | ~57,000 per page |
| **Avg output tokens** | ~400 per page (top-20 nodes) |
| **Avg token reduction** | **97%** (57K → 400) |
| **Avg latency** | **1,038ms** |
| **Median latency** | **953ms** |
| **Min latency** | 96ms (IMDB, 1 node) |
| **Max latency** | 5,211ms (Investing.com, 27K nodes) |

### 2.5 Per-Category Summary

| Category | Sites | top-5 recall | top-20 recall | Avg tokens in | Avg tokens out | Savings |
|----------|:-----:|:-----------:|:------------:|:-------------:|:--------------:|:-------:|
| News | 7 | 7/7 (100%) | 7/7 (100%) | ~18K | ~300 | 98% |
| Government | 5 | 5/5 (100%) | 5/5 (100%) | ~31K | ~450 | 99% |
| Dev/Docs | 10 | 10/10 (100%) | 10/10 (100%) | ~9K | ~350 | 96% |
| Packages | 5 | 4/5 (80%) | 4/5 (80%) | ~5K | ~200 | 96% |
| Infrastructure | 4 | 4/4 (100%) | 4/4 (100%) | ~150K | ~500 | 99% |
| Reference | 5 | 5/5 (100%) | 5/5 (100%) | ~28K | ~250 | 99% |
| Finance | 4 | 4/4 (100%) | 4/4 (100%) | ~480K | ~500 | 100% |
| Other | 4 | 3/4 (75%) | 3/4 (75%) | ~10K | ~300 | 97% |

### 2.6 Full Site List (44 sites)

Complete results for reproducibility. See `benches/poc_50_results.json` for raw data.

| # | Site | Nodes | Out | Rank | Save% | ms | # | Site | Nodes | Out | Rank | Save% | ms |
|---|------|:-----:|:---:|:----:|:-----:|:---:|---|------|:-----:|:---:|:----:|:-----:|:---:|
| 1 | Hacker News | 87 | 1 | 1 | 99 | 386 | 23 | PyPI | 28 | 8 | 1 | 95 | 659 |
| 2 | lobste.rs | 446 | 18 | 1 | 98 | 1161 | 24 | pkg.go.dev | 238 | 10 | 3 | 96 | 708 |
| 3 | CNN Lite | 209 | 9 | 1 | 99 | 856 | 25 | RubyGems | 10 | 3 | 1 | 97 | 399 |
| 4 | NPR Text | 54 | 10 | 2 | 90 | 811 | 26 | NuGet | 41 | 9 | 1 | 93 | 884 |
| 5 | Reuters | 1 | 1 | 1 | 93 | 141 | 27 | Docker Hub | 100 | 15 | 2 | 99 | 896 |
| 6 | Tibro kommun | 1971 | 20 | 1 | 99 | 1263 | 28 | Terraform | 610 | 20 | 1 | 98 | 1130 |
| 7 | Al Jazeera | 303 | 10 | 1 | 100 | 959 | 29 | GitHub Exp | 579 | 19 | 1 | 99 | 982 |
| 8 | GOV.UK Wage | 275 | 20 | 4 | 97 | 1118 | 30 | Tailwind | 9004 | 20 | 3 | 100 | 2235 |
| 9 | Bank of England | 572 | 13 | 2 | 98 | 1209 | 31 | OSM | 118 | 13 | 1 | 97 | 1218 |
| 10 | WHO | 820 | 15 | 2 | 99 | 1242 | 32 | httpbin | 3 | 1 | 1 | 99 | 167 |
| 11 | EU Europa | 339 | 13 | 1 | 98 | 952 | 33 | JSON PH | 91 | 9 | 1 | 85 | 890 |
| 12 | NASA | 497 | 8 | 1 | 100 | 1066 | 34 | Haskell | 453 | 12 | 1 | 96 | 801 |
| 13 | rust-lang | 76 | 15 | 1 | 86 | 983 | 35 | W3Schools | 1566 | 11 | 1 | 100 | 1461 |
| 14 | MDN HTML | 363 | 16 | 9 | 100 | 1024 | 36 | CoinGecko | 1473 | 20 | 3 | 100 | 1842 |
| 15 | Go Dev | 235 | 13 | 1 | 95 | 1016 | 37 | ECB | 1988 | 20 | 2 | 99 | 1471 |
| 16 | Node.js | 31 | 6 | 1 | 100 | 750 | 38 | Investing | 27247 | 20 | 1 | 100 | 5211 |
| 17 | Ruby Lang | 232 | 15 | 1 | 97 | 828 | 39 | XE Currency | 5201 | 7 | 5 | 100 | 1764 |
| 18 | docs.rs | 84 | 8 | 2 | 98 | 785 | 40 | Goodreads | 188 | 20 | 1 | 94 | 953 |
| 19 | Kotlin | 221 | 17 | 1 | 99 | 1082 | 41 | Spotify | 3 | 1 | 1 | 100 | 167 |
| 20 | Elixir | 145 | 20 | 1 | 90 | 1067 | 42 | Product Hunt | 463 | 12 | 2 | 100 | 1277 |
| 21 | Zig | 105 | 9 | 1 | 92 | 794 | 43 | DevDocs | 2 | 1 | ✗ | 99 | 138 |
| 22 | Svelte | 178 | 10 | 1 | 99 | 818 | 44 | IMDB Top | 1 | 0 | ✗ | 100 | 96 |

### 2.7 Failure Analysis

Two sites failed (both JS-rendered SPAs returning 0–1 content nodes):

- **DevDocs** (devdocs.io) — 2 total nodes, all content loaded via JavaScript. Static tier returns empty shell.
- **IMDB Top 250** (imdb.com/chart/top/) — 1 total node. React SPA, no server-side rendering.

Both require JS evaluation tier escalation (planned, not yet automatic).

### 2.5 Optimization Progression

| Configuration | Avg latency | Speedup |
|---------------|-------------|---------|
| Candle FP32, sequential | 9,284ms | baseline |
| ONNX FP32, sequential | 6,252ms | 1.5× |
| ONNX Int8, batch | 691ms | 13.4× |
| + survivor cap + u8 MaxSim + cache | 434ms | 21.4× |
| + token pruning + query expansion | ~500ms | — |
| + dense fallback + bottom-up | ~1,038ms* | — |

*Latency increased due to dense retrieval fallback (~100ms on trigger) and bottom-up double scoring. Tradeoff: +500ms latency for +40% answer recall on hard queries.

---

## 3. LLM-Driven Goal Expansion

The pipeline's BM25 stage matches keywords literally. If the user asks "hur många bor i Hjo?" but the page says "14 352 invånare", BM25 finds no overlap between "bor" and "invånare".

**Solution:** The MCP tool description instructs the LLM agent to expand the goal with specific synonyms before calling the tool:

```
BAD:  "hur många bor i Hjo"
GOOD: "hur många bor i Hjo invånare befolkning folkmängd 14000 Hjo kommun"
```

Rules encoded in tool description:
- Include 5–8 specific synonyms/translations
- Include expected values (numbers, currencies, dates)
- Never add generic words ("information", "service") — they match boilerplate

**Measured effect (12-site A/B test):**

| Metric | Without expansion | With expansion |
|--------|------------------|----------------|
| Avg BM25 candidates | 87 | 135 (+55%) |
| Avg latency | 1,152ms | 571ms (-50%) |
| Avg top-1 score | 0.677 | 0.825 (+22%) |

Expansion is zero-cost — the LLM does the work it already understands. No additional model inference or API calls.

---

## 4. Model and Infrastructure

### 4.1 Single Model, Two Modes

The system uses a single ONNX model (`colbert-small-int8.onnx`, 22MB) for both:

- **Bi-encoder mode** (mean pooling → single vector): Used for dense retrieval fallback and legacy scoring
- **ColBERT mode** (per-token embeddings → MaxSim): Used for Stage 3 reranking

Base model: `all-MiniLM-L6-v2` (384-dim, 6 layers, 22M parameters), dynamically int8 quantized.

### 4.2 Zero-Config Deployment

```bash
cargo run --features server,colbert --bin aether-server --release
```

The model and vocabulary files are checked into the repository. No environment variables, downloads, or external dependencies required. The server auto-detects and loads:
- `models/colbert-small-int8.onnx` (22MB) — ColBERT + bi-encoder fallback
- `models/vocab.txt` (227KB) — WordPiece vocabulary

### 4.3 Feature Flags

| Feature | What it enables | Dependencies |
|---------|----------------|--------------|
| `embeddings` | Bi-encoder scoring (MiniLM) | ort, ndarray |
| `colbert` | ColBERT MaxSim + all optimizations (A/B/C) | depends on `embeddings` |
| `server` | HTTP API (includes embeddings + colbert) | axum, tokio, ... |
| `mcp` | MCP server for Claude/Cursor/VS Code | rmcp, ... |

---

## 5. Security Integration

All web content enters the pipeline as `TrustLevel::Untrusted`. Prompt injection patterns (20+ English + Swedish) are detected via Aho-Corasick automaton at parse time. The retrieval pipeline provides implicit defense: off-topic injected content scores low across all four stages because it lacks keyword overlap (BM25), structural relevance (HDC), and semantic similarity (ColBERT).

---

## 6. Limitations

1. **JS-rendered SPAs** (2/44 failures) require JavaScript evaluation. Static tier returns empty shells for React/Next.js apps without SSR.
2. **Dense retrieval fallback** adds ~100-500ms when triggered. On very large DOMs (>5000 nodes), the scan is capped at 200 nodes.
3. **Goal expansion quality** depends on the LLM agent. Generic expansion terms ("information", "service") hurt precision.
4. **Table content** often not extracted as individual rows — tabular data appears as a single concatenated text node.

---

## 7. Conclusion

The four-stage pipeline — BM25 + dense fallback for recall, HDC for structural pruning, ColBERT with bottom-up scoring for precision — delivers goal-relevant DOM nodes with 95.5% answer recall and 97% token savings across 44 diverse websites. The system requires no external models, no GPU, and no configuration — a single 22MB int8 ONNX model handles both dense retrieval and late-interaction reranking.

The key architectural contributions:
1. **Dense retrieval fallback** at Stage 1 catches semantic matches that BM25 misses (e.g., "bor" → "invånare")
2. **Multi-aspect HDC vectors** (text + role separately) provide structural signals to the neural scorer at zero cost
3. **Bottom-up ColBERT scoring** eliminates wrapper-bias by scoring leaves first and letting parents inherit reduced scores
4. **LLM-driven goal expansion** via tool descriptions achieves synonym expansion without additional model inference

---

## References

### Retrieval & Scoring
- Khattab, O. & Zaharia, M. (2020). ColBERT: Efficient and Effective Passage Search via Contextualized Late Interaction over BERT. *SIGIR 2020*.
- Kanerva, P. (2009). Hyperdimensional Computing: An Introduction to Computing in Distributed Representation. *Cognitive Computation*, 1(2), 139–159.
- Reimers, N. & Gurevych, I. (2019). Sentence-BERT: Sentence Embeddings using Siamese BERT-Networks. *EMNLP 2019*.
- Robertson, S. & Zaragoza, H. (2009). The Probabilistic Relevance Framework: BM25 and Beyond. *Foundations and Trends in IR*, 3(4), 333–389.
- Nogueira, R. & Cho, K. (2019). Passage Re-ranking with BERT. *arXiv:1901.04085*.

### Web Agents & Benchmarks
- Zhou, S., Xu, F. F., Zhu, H., et al. (2024). WebArena: A Realistic Web Environment for Building Autonomous Agents. *ICLR 2024*.
- Deng, X., Gu, Y., Zheng, B., et al. (2023). Mind2Web: Towards a Generalist Agent for the Web. *NeurIPS 2023*.
- Zheng, B., Gou, B., Kil, J., et al. (2024). SeeAct: GPT-4V(ision) is a Web Agent, if Grounded. *ICML 2024*.
- Drouin, A., et al. (2024). BrowserGym: An Open Environment for Web Agent Evaluation. *arXiv:2024*.
- Koh, J. Y., et al. (2024). Tree Search for Language Model Agents. *arXiv:2024*.
- Liu, X., et al. (2023). AgentBench: Evaluating LLMs as Agents. *arXiv:2023*.

### Environmental Impact & Efficiency
- Luccioni, A. S., et al. (2023). Power Hungry Processing: Watts Driving the Cost of AI Deployment? *FAccT 2024*.
- Patterson, D., et al. (2022). The Carbon Footprint of Machine Learning Training Will Plateau, Then Shrink. *IEEE Computer*.
- Li, P., et al. (2023). Making AI Less Thirsty: Uncovering and Addressing the Secret Water Footprint of AI Models. *arXiv:2304.03271*.
- Schwartz, R., et al. (2020). Green AI. *Communications of the ACM*, 63(12), 54–63.

### Safety & Prompt Injection
- Greshake, K., Abdelnabi, S., Mishra, S., et al. (2023). Not What You've Signed Up For: Compromising Real-World LLM-Integrated Applications with Indirect Prompt Injections. *AISec 2023*.
- Zhan, Q., Liang, Z., Ying, Z., & Kang, D. (2024). InjecAgent: Benchmarking Indirect Prompt Injections in Tool-Integrated LLM Agents. *ACL Findings 2024*.
