# Hybrid Scoring Pipeline — Benchmark Results

**Date:** 2026-03-30
**Platform:** Linux x86_64, release build
**Embedding model:** all-MiniLM-L6-v2 (384-dim, ONNX)
**Scoring:** BM25 → HDC (2048-bit) → Bottom-up Embedding

## Real-World Validation (20 Sites, WITH Embeddings)

19 of 20 sites successfully fetched and tested (1 timeout: DuckDuckGo).

### Summary

| Metric | Legacy | Hybrid |
|--------|--------|--------|
| **Correctness (keyword in top 3)** | 15/19 (79%) | **16/19 (84%)** |
| **Avg parse time** | 418ms | **168ms (2.5x faster)** |
| Unique wins | — | HN Newest: MISS→PASS |

### Per-Site Quality Wins

| Site | Legacy top-1 | Hybrid top-1 | Improvement |
|------|-------------|-------------|-------------|
| PyPI | 0.660 | **0.900** | +36% |
| MDN HTML | 0.700 | **0.900** | +29% |
| GitHub Explore | 0.505 | **0.825** | +63% |
| pkg.go.dev | 0.455 | **0.806** | +77% |
| Docker Hub | 0.437 | **0.650** | +49% |
| CNN Lite | 0.324 | **0.606** | +87% |
| NPR Text | 0.504 | **0.618** | +23% |
| OpenStreetMap | 0.360 | **0.544** | +51% |
| NPM | 0.292 | **0.471** | +61% |

### Per-Site Speed

| Site | HTML | Legacy | Hybrid | Speedup |
|------|------|--------|--------|---------|
| NPR Text | 5KB | 739ms | **3ms** | **246x** |
| CNN Lite | 326KB | 769ms | **67ms** | **11x** |
| docs.rs | 16KB | 600ms | **29ms** | **21x** |
| OpenStreetMap | 32KB | 573ms | **55ms** | **10x** |
| PyPI | 21KB | 330ms | **27ms** | **12x** |
| NPM | 28KB | 151ms | **28ms** | **5x** |
| pkg.go.dev | 32KB | 526ms | **116ms** | **5x** |
| Docker Hub | 387KB | 290ms | **156ms** | **1.9x** |
| GitHub Explore | 386KB | 692ms | **403ms** | **1.7x** |

## Pipeline Architecture

```
HTML → BM25 candidate retrieval → HDC two-step pruning → Bottom-up embedding
       ~0.1ms (keyword match)     ~0.5ms (bitvector)     ~50-400ms (neural)
```

### BM25 vs TF-IDF (why we switched)

| Problem | TF-IDF | BM25 |
|---------|--------|------|
| Common terms (e.g. "rust" in 3/4 nodes) | IDF → 0, **no candidates** | IDF always positive, **finds all** |
| Long wrapper labels vs short content | Same score | **Short nodes boosted** (b=0.75) |
| Repeated terms | Linear growth | **Saturates** (k1=1.2) |
| Fallback when 0 candidates | Pass entire DOM to embedding | **HDC prune_pure** (structural top-K) |

### Adaptive Survivor Cap

Prevents embedding from running on hundreds of nodes:

| DOM size | Max survivors | With high BM25 confidence |
|----------|---------------|---------------------------|
| < 50 nodes | all | all |
| 50-200 | 60 | 36 (×0.6) |
| 200-500 | 80 | 48 (×0.6) |
| > 500 | 100 | 60 (×0.6) |

### Two-Step HDC Pruning

1. **Broad filter**: Adaptive threshold per role/depth (navigation stricter, buttons pass)
2. **Strict ranking**: If still > cap, rank by 60% BM25 + 40% HDC-similarity, truncate

### Pipeline Stage Breakdown (MDN, 173KB, 1050 nodes)

| Stage | Time | % |
|-------|------|---|
| BM25 build | 1.9ms | 0.4% |
| HDC build (2048-bit, n-grams) | 18.9ms | 4.3% |
| BM25 query | 0.02ms | 0% |
| HDC prune (two-step) | 0.05ms | 0% |
| Embedding score (~80 survivors) | 441ms | 95% |
| **Total pipeline** | **462ms** | |

### Build Cache (Arc-wrapped LRU, 32 entries)

Second query to same page skips build entirely:

| Metric | First query | Cached query |
|--------|-------------|-------------|
| Build phase | ~20ms | **0ms** |
| Query + prune + embed | ~450ms | ~450ms |
| **Amortized savings** | — | **~20ms per query** |

## API Endpoints

| Interface | Endpoint | Default top_n |
|-----------|----------|---------------|
| **MCP (stdio)** | `parse_hybrid` tool | 100 |
| **HTTP** | `POST /api/parse-hybrid` | 100 |
| **WebSocket** | `{"method": "parse_hybrid"}` | 100 |
| **WASM** | `parse_top_nodes_hybrid()` | caller-specified |
| Legacy | `parse_top` / `POST /api/parse-top` | caller-specified |

## Architectural Fixes

1. **Bugg B (wrapper-bias)**: Bottom-up scoring — leaf nodes scored first, parents inherit max(child) × 0.75
2. **Bugg A (top_n ignored)**: Strict enforcement after ranking
3. **0-candidate fallback**: HDC prune_pure replaces "pass entire DOM" fallback

## When to Use

| Scenario | Recommended |
|----------|-------------|
| Any new integration | `parse_hybrid` |
| Large pages (>100 nodes) | `parse_hybrid` (2.5x faster) |
| Quality-critical ranking | `parse_hybrid` (84% vs 79% correctness) |
| WASM/browser embedding | `parse_top_nodes_hybrid` |
| Backward compatibility | `parse_top` (unchanged) |
