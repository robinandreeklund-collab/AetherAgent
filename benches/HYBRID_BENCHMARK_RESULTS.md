# Hybrid Scoring Pipeline — Benchmark Results

**Date:** 2026-03-30
**Platform:** Linux x86_64, release build (`cargo run --release`)
**Baseline:** Legacy single-pass embedding scoring

## Speed Comparison

| Scenario | Legacy (µs) | Hybrid (µs) | Speedup | Notes |
|----------|-------------|-------------|---------|-------|
| Simple (3 nodes) | 32 | 57 | 0.56x | Hybrid overhead dominates on tiny pages |
| Medium (50 items) | 902 | 2,107 | 0.43x | Build phase ~80% of hybrid time |
| Large (500 items) | 10,146 | 17,865 | 0.57x | Build amortized over queries |
| Large top_5 | 9,910 | 17,002 | 0.58x | top_n correctly applied |

### Hybrid Pipeline Breakdown (Large Page, 500+ nodes)

| Phase | Time (µs) | % of Total |
|-------|-----------|------------|
| TF-IDF build | 4,215 | 58.6% |
| HDC build | 2,410 | 33.5% |
| TF-IDF query | 89 | 1.2% |
| HDC prune | 41 | 0.6% |
| Embedding score | 179 | 2.5% |
| Overhead | 262 | 3.6% |
| **Total** | **7,196** | **100%** |

### Amortized Performance (Build Cached per URL)

When the TF-IDF and HDC indices are cached (e.g., same page, different goals):

| Metric | Value |
|--------|-------|
| Build phase (one-time) | 6,625 µs |
| Query phase (per goal) | 309 µs |
| Legacy full parse | 9,910 µs |
| **Amortized speedup** | **32.1x** |

## Correctness Comparison

| Test Case | Legacy | Hybrid |
|-----------|--------|--------|
| Population in medium page | PASS | PASS |
| Data in large page | PASS | PASS |
| Simple population | PASS | PASS |
| **Score** | **3/3** | **3/3** |

## Architectural Improvements

The hybrid pipeline fixes two known bugs:

1. **Bugg B (wrapper-bias)**: Legacy scores top-down, causing wrapper nodes to
   "steal" relevance from children. Hybrid scores bottom-up: leaf nodes are
   scored directly, parents inherit max(child) × 0.75.

2. **Bugg A (top_n ignored)**: `parse_top_nodes_hybrid()` correctly applies
   `top_n` as the final step after scoring and ranking.

## Key Metrics

| Metric | Legacy | Hybrid |
|--------|--------|--------|
| Scoring method | Single-pass embedding (30 max calls) | TF-IDF → HDC → Embedding |
| Scoring direction | Top-down (parent first) | Bottom-up (leaf first) |
| top_n enforcement | Post-hoc (may return more) | Strict (always ≤ top_n) |
| External dependencies | None new | None new (pure Rust HDC) |
| Build amortization | N/A | Yes (per URL cache) |
| Query-only time (large page) | ~10ms | ~0.3ms |

## When to Use Hybrid

- **Multi-query per page**: Use hybrid when multiple goals will be queried
  against the same page (32x faster per query after build)
- **Correctness-critical**: When wrapper-bias (Bugg B) causes wrong nodes
  to be ranked highest
- **Large pages (1000+ nodes)**: Build amortization pays off more

## When to Use Legacy

- **Single query per page**: Legacy is ~2x faster for one-shot queries
- **Small pages (<50 nodes)**: Build overhead not justified
- **Backward compatibility**: Legacy API unchanged
