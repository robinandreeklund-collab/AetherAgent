# AetherAgent Hybrid Scoring Pipeline — Technical Whitepaper

**Version:** 1.0 · **Date:** 2026-03-31
**Authors:** AetherAgent Core Team

---

## Abstract

AetherAgent's hybrid scoring pipeline replaces single-pass embedding scoring with a three-stage architecture: BM25 keyword retrieval, Hyperdimensional Computing (HDC) structural pruning, and bottom-up neural embedding. On 20 real-world websites, the hybrid pipeline achieves **90% correctness** (vs 80% legacy), runs **1.8× faster**, and fixes structural bugs where wrapper nodes stole relevance from content nodes.

---

## 1. Problem Statement

### 1.1 The Wrapper-Bias Bug

Legacy AetherAgent scores nodes **top-down** during DOM traversal. Parent nodes aggregate their children's text into a single label, then run embedding similarity against the goal. This causes wrapper `<div>` elements — which contain the entire page's text — to score highest, pushing actual content nodes (paragraphs, table cells, links) out of `top_n` results.

```
Legacy scoring (top-down):

<body>  label="Hacker News new past comments ask show jobs..."  → rel=0.21  ← WINS
  <table>  label="Hacker News new past comments..."              → rel=0.20
    <tr>  label="Show HN: My cool project"                       → rel=0.18  ← CORRECT ANSWER
```

### 1.2 The Embedding Cost Problem

Running neural embedding (all-MiniLM-L6-v2, 384-dim) on every node costs ~10ms per call. A 500-node page requires ~5 seconds of embedding time. Legacy limits this to 30 calls, but that means 470 nodes get zero embedding score.

### 1.3 The top_n Enforcement Bug

Legacy `parse_top_nodes` built the full tree, then applied `top_n` as a post-filter — but wrapper nodes at the top of the sort displaced content nodes, making `top_n=5` return 5 wrappers instead of 5 answers.

---

## 2. Architecture Overview

```
                        ┌─────────────┐
                        │   HTML Input  │
                        └──────┬──────┘
                               │
                    ┌──────────▼──────────┐
                    │  Parse to Semantic   │
                    │  Tree (ArenaDom)     │
                    └──────────┬──────────┘
                               │
              ┌────────────────▼────────────────┐
              │         BUILD PHASE (~20ms)       │
              │  ┌───────────┐  ┌──────────────┐ │
              │  │ BM25 Index │  │  HDC Tree    │ │
              │  │ (postings) │  │ (4096-bit HV)│ │
              │  └───────────┘  └──────────────┘ │
              │     Cached per content-hash       │
              └────────────────┬────────────────┘
                               │
    ┌──────────────────────────▼──────────────────────────┐
    │                   QUERY PHASE                        │
    │                                                      │
    │  Stage 1: BM25 Candidate Retrieval      (~0.1ms)    │
    │  ┌──────────────────────────────────────────────┐   │
    │  │ goal tokens → inverted index lookup           │   │
    │  │ "population statistics" → nodes with matches  │   │
    │  │ Output: ~50-300 candidates ranked by BM25     │   │
    │  │ Fallback: prefix-match if 0 exact matches     │   │
    │  └──────────────────────┬───────────────────────┘   │
    │                         │                            │
    │  Stage 2: HDC Pruning (two-step)        (~0.5ms)    │
    │  ┌──────────────────────▼───────────────────────┐   │
    │  │ Step 2a: Adaptive threshold per role/depth    │   │
    │  │   navigation → strict (0.10)                  │   │
    │  │   button/link → pass always (-1.0)            │   │
    │  │   generic deep → moderate (0.08)              │   │
    │  │                                               │   │
    │  │ Step 2b: If still > cap → rank by             │   │
    │  │   60% BM25 score + 40% HDC similarity         │   │
    │  │   Truncate to adaptive cap (20-100)           │   │
    │  │ Output: ~20-80 survivors                      │   │
    │  └──────────────────────┬───────────────────────┘   │
    │                         │                            │
    │  Stage 3: Bottom-Up Embedding           (~50-400ms) │
    │  ┌──────────────────────▼───────────────────────┐   │
    │  │ Score leaf nodes first via embedding           │   │
    │  │ Parents inherit: max(children) × 0.75          │   │
    │  │ Role-multiplier: links ×0.4-0.85, data ×1.2   │   │
    │  │ Wrapper-penalty: structural >200ch → -0.20     │   │
    │  │ Label dedup: identical labels → keep highest    │   │
    │  │ Leaf-link boost: 30-200ch links → ×1.15        │   │
    │  │ Output: scored + ranked + deduped nodes         │   │
    │  └──────────────────────┬───────────────────────┘   │
    │                         │                            │
    └─────────────────────────┼────────────────────────────┘
                              │
                    ┌─────────▼─────────┐
                    │  Apply top_n       │
                    │  Return to agent   │
                    └───────────────────┘
```

---

## 3. BM25 Scoring (Stage 1)

### 3.1 Why BM25 over TF-IDF

| Problem | TF-IDF | BM25 |
|---------|--------|------|
| Common terms ("rust" in 3/4 nodes) | IDF = ln(4/4) = 0 → **no candidates** | IDF = ln((4-3+0.5)/(3+0.5)+1) = 0.29 → **finds all** |
| Repeated terms | Linear growth (spam wins) | Saturates at k1=1.2 |
| Long wrapper vs short content | Same TF weight | Short docs boosted (b=0.75) |

### 3.2 BM25 Formula

```
score(q, d) = Σ IDF(qi) × tf(qi,d) × (k1 + 1)
                          ─────────────────────────────
                          tf(qi,d) + k1 × (1 - b + b × |d|/avgdl)

Where:
  IDF(qi)  = ln((N - df(qi) + 0.5) / (df(qi) + 0.5) + 1)
  k1       = 1.2  (term frequency saturation)
  b        = 0.75 (document length normalization)
  |d|      = number of tokens in node label
  avgdl    = average label length across all nodes
```

### 3.3 Prefix-Match Fallback

When BM25 returns 0 candidates (no keyword overlap between goal and any node), a prefix-match step scans all index terms for prefix matches. Score is reduced to 70% to indicate lower confidence.

```
Goal: "population" → no exact match
Prefix scan: "popul" matches "populationdata" → candidate at 0.7× score
```

---

## 4. Hyperdimensional Computing (Stage 2)

### 4.1 What is HDC?

Hyperdimensional Computing represents concepts as high-dimensional binary vectors (hypervectors). Similarity between concepts is measured by Hamming distance, which can be computed in nanoseconds using XOR + popcount CPU instructions.

### 4.2 Hypervector Construction

Each node's hypervector is built from three components:

```
Node HV = bundle(
    bind(text_hv, role_hv).permute(depth × 7),
    child_hvs...
)

Where:
  text_hv  = from_text_ngrams(label)    ← word n-gram binding
  role_hv  = from_seed("__role_" + role) ← deterministic per role
  bind     = XOR (composition)
  permute  = cyclic bit-shift (position encoding)
  bundle   = majority vote (aggregation)
```

### 4.3 N-gram Binding for Order Sensitivity

Simple seed-hashing loses word order ("cat chases dog" = "dog chases cat"). N-gram binding preserves order:

```
from_text_ngrams("cat chases dog"):

  Unigrams (position-bound):
    HV("cat").permute(0)
    HV("chases").permute(3)
    HV("dog").permute(6)

  Bigrams (XOR-bound + position):
    bind(HV("cat"), HV("chases").permute(1)).permute(0)
    bind(HV("chases"), HV("dog").permute(1)).permute(5)

  Trigrams:
    bind(HV("cat"), HV("chases").permute(1), HV("dog").permute(2)).permute(0)

  Final: bundle(all components via majority vote)
```

### 4.4 Similarity via Hamming Distance

```
similarity(a, b) = 1 - 2 × hamming(a, b) / DIM

Where hamming = popcount(a XOR b)
```

At 4096 bits, this takes ~2ns per comparison (single POPCNT instruction per 64-bit word, 64 words).

### 4.5 Dimension Selection: 4096-bit

Tested 1024, 2048, and 4096 bits on 20 real sites:

```
                    1024-bit    2048-bit    4096-bit
Correctness:        18/20       18/20       18/20       (all identical)
Avg parse time:     333ms       356ms       365ms
HDC build (MDN):    13ms        19ms        22ms
HDC build (DDG):    39ms        98ms        110ms
Memory/vector:      128 bytes   256 bytes   512 bytes
```

All three dimensions produce identical ranking and correctness. The cost difference is only in HDC build time, which is dwarfed by embedding scoring (95%+ of pipeline).

**Selected 4096-bit** — provides theoretical headroom for 10k+ node DOMs where hash collisions in lower dimensions could degrade separation. The +10% build cost vs 1024 is negligible.

### 4.6 Two-Step Pruning

```
Step 2a: Broad Adaptive Threshold
  ┌──────────────────┬────────────┐
  │ Role + Depth     │ Threshold  │
  ├──────────────────┼────────────┤
  │ depth ≤ 1        │ -1.0 (all) │
  │ navigation, d≥2  │  0.10      │
  │ generic, d≥3     │  0.08      │
  │ button/link/text │ -1.0 (all) │
  │ other            │  0.05      │
  └──────────────────┴────────────┘

Step 2b: Strict Ranking (if survivors > cap)
  Combined score = 0.6 × BM25 + 0.4 × HDC-similarity
  Sort descending, truncate to adaptive cap
```

### 4.7 Adaptive Survivor Cap

```
  DOM < 50 nodes  → keep all
  DOM 50-200      → cap at 60  (× 0.6 if BM25 found >100 candidates)
  DOM 200-500     → cap at 80
  DOM > 500       → cap at 100
```

---

## 5. Bottom-Up Embedding Scoring (Stage 3)

### 5.1 Inversion of Scoring Direction

Legacy (top-down): parent scored first → children inherit or get less.
Hybrid (bottom-up): **leaf nodes scored first** → parents inherit max(children) × 0.75.

```
Bottom-up scoring:

  <p>"367,924 inhabitants"</p>     → embed("367,924 inhabitants", goal) = 0.82
  <div> (parent of <p>)             → max(0.82) × 0.75 = 0.615
  <body> (grandparent)              → max(0.615) × 0.75 = 0.461

Result: The <p> with the actual answer ranks HIGHEST.
```

### 5.2 Scoring Formula

```
score = (semantic × 0.45 + role_priority × 0.25 + bm25_norm × 0.30
         - wrapper_penalty) × role_multiplier

Where:
  semantic       = max(word_overlap, embedding_similarity)
  role_priority  = 0.95 (cta) .. 0.2 (unknown)
  bm25_norm      = min(bm25_score / 3.0, 1.0)
  wrapper_penalty = 0.20 if structural + label >200ch
                    0.10 if structural + label >100ch
  role_multiplier = 0.4  (quoted reference links)
                    0.7  (short nav links <40ch)
                    1.15 (table rows, list items)
                    1.2  (data/jsonLd nodes)
```

### 5.3 Post-Scoring Filters

1. **Leaf-link boost**: Leaf `<a>` nodes with label 30-200 chars get ×1.15 (story titles, article links)
2. **Label dedup**: Identical labels (first 80 chars) → keep only highest-scored (eliminates wrapper duplicates at different DOM depths)

---

## 6. Supporting Infrastructure

### 6.1 Build Cache (Arc-wrapped LRU)

BM25 index + HDC tree + node index cached per content-hash (FNV-1a of HTML). Max 32 entries. Second query to same page skips ~20ms build.

```
First query:   build (20ms) + query+prune+embed (300ms) = 320ms
Cached query:  query+prune+embed (300ms) = 300ms
Savings:       ~6% per cached query
```

### 6.2 Page-Level URL Cache (TTL-based)

Full semantic tree + HTML cached per URL. TTL varies by site type:

| Site Type | TTL | Examples |
|-----------|-----|---------|
| Reference sites | 10 min | Wikipedia, Britannica, docs.rs |
| Government | 5 min | gov.uk, riksdagen.se |
| Default | 5 min | Most sites |
| SPAs | 1 min | worldpopulationreview, npmjs |
| News/realtime | 30 sec | BBC, Reuters, CNN |

Verified: 3.3× speedup on second query with different goal (369ms → 111ms).

### 6.3 Async Fetch-Bridge (BUGG J)

For JS-rendered SPAs that load data via `fetch()`:

```
┌─────────────┐     ┌──────────────────┐     ┌──────────────────┐
│  QuickJS     │────▶│  __fetchedUrls   │────▶│  Rust async      │
│  sandbox     │     │  [url1, url2...] │     │  reqwest fetch   │
│              │     │                  │     │                  │
│  fetch(url)  │     │  Captured at     │     │  intercept_xhr() │
│  → stub resp │     │  JS eval time    │     │  → SemanticNodes │
└─────────────┘     └──────────────────┘     └────────┬─────────┘
                                                       │
                                              ┌────────▼─────────┐
                                              │  Merge into tree  │
                                              │  + hybrid score   │
                                              └──────────────────┘
```

Verified: HTML with `fetch("https://jsonplaceholder.typicode.com/users")` → XHR intercepted: 1 → user data merged into tree.

---

## 7. Injection Protection Integration

The trust shield runs **before** scoring — injection-detected nodes get sanitized labels but are still scored (so warnings are reported even for filtered content).

### 7.1 Unicode False Positive Fix

Only U+200B (ZWSP) and U+FEFF (BOM) trigger warnings. Mathematical notation characters whitelisted:

| Character | Code Point | Use | Status |
|-----------|-----------|-----|--------|
| Zero-width joiner | U+200D | Superscript refs, emoji | ✅ Whitelisted |
| Zero-width non-joiner | U+200C | Persian/Arabic text | ✅ Whitelisted |
| Word joiner | U+2060 | Math notation (⁠c/v⁠) | ✅ Whitelisted |
| Soft hyphen | U+00AD | Hyphenation | ✅ Whitelisted |
| **Zero-width space** | **U+200B** | **Hides text** | ⚠️ **Flagged** |
| **BOM in text** | **U+FEFF** | **Hides text** | ⚠️ **Flagged** |

### 7.2 Contextual "you are now" Pattern

Replaced broad `"you are now"` match (false positive on "You are now subscribed") with contextual variants:

```
"you are now a "   → matches "you are now a helpful assistant"
"you are now an "  → matches "you are now an unrestricted AI"
"you are now the " → matches "you are now the DAN persona"
"you are now in "  → matches "you are now in jailbreak mode"
"you are now my "  → matches "you are now my personal AI"

Does NOT match: "You are now subscribed to our newsletter"
```

---

## 8. Results

### 8.1 Real-World Validation (20 Sites, Embeddings Enabled)

| Metric | Legacy | Hybrid |
|--------|--------|--------|
| **Correctness** | 16/20 (80%) | **18/20 (90%)** |
| **Avg parse time** | 640ms | **365ms (1.8×)** |
| Misses | 4 | 2 (JS SPAs with no static content) |

### 8.2 Quality Improvements

| Site | Legacy Top-1 | Hybrid Top-1 | Improvement |
|------|-------------|-------------|-------------|
| PyPI | 0.660 | **0.945** | +43% |
| MDN | 0.700 | **0.900** | +29% |
| GitHub | 0.505 | **0.825** | +63% |
| pkg.go.dev | 0.455 | **0.888** | +95% |
| Rust Lang | 0.379 | **0.637** ("Install" link) | +68% |

### 8.3 Structural Bug Fixes

| Bug | Before | After |
|-----|--------|-------|
| Wrapper-bias (B) | Wrappers rank #1 | Content nodes rank #1 |
| top_n ignored (A) | Returns all nodes | Strict enforcement |
| Label truncation (F) | 80 chars (lost facts) | 300 chars |
| Reference-link dominance (G) | Wikipedia refs rank #1 | Role-multiplier penalizes |
| Math unicode false positive (H) | "299 792 458" filtered | Whitelisted |
| "you are now" false positive (I) | Newsletters filtered | Contextual match |
| Async data loading (J) | 0 nodes on SPAs | Fetch-bridge merges data |

---

## 9. API

| Interface | Endpoint | Default top_n |
|-----------|----------|---------------|
| **MCP** | `parse_hybrid` tool | 20 |
| **HTTP** | `POST /api/parse-hybrid` | 20 |
| **WebSocket** | `{"method": "parse_hybrid"}` | 100 |
| **WASM** | `parse_top_nodes_hybrid()` | caller-specified |
| **Unified parse** | `parse` tool with `hybrid: true` | caller-specified |

Response includes pipeline metadata:
```json
{
  "pipeline": {
    "method": "hybrid_bm25_hdc_embedding",
    "bm25_candidates": 133,
    "hdc_survivors": 60,
    "total_pipeline_us": 330732,
    "cache_hit": true
  }
}
```

---

## 10. Source Files

| File | Purpose |
|------|---------|
| `src/scoring/tfidf.rs` | BM25 index (build, query, prefix-match, incremental update) |
| `src/scoring/hdc.rs` | HDC 4096-bit hypervectors (bind, permute, bundle, n-grams, prune) |
| `src/scoring/embed_score.rs` | Bottom-up scoring, role-multiplier, dedup, leaf-link boost |
| `src/scoring/pipeline.rs` | Pipeline orchestration, adaptive survivor cap, two-step HDC |
| `src/scoring/cache.rs` | BM25+HDC build cache (Arc-wrapped LRU, 32 entries) |
| `src/scoring/page_cache.rs` | Page-level URL cache with TTL |
| `src/tools/parse_hybrid_tool.rs` | MCP/HTTP tool implementation |
| `src/dom_bridge/window.rs` | QuickJS fetch() URL capture |
| `src/dom_bridge/state.rs` | DomEvalResult with fetched_urls |
| `src/tools/mod.rs` | resolve_pending_fetches() async pipeline |
