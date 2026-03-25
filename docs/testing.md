# Testing & Benchmarks

> Test suite, benchmarks, and performance data for AetherAgent.
> For a summary, see the [main README](../README.md).

---

## Running Tests

```bash
cargo test              # All 427 tests
cargo clippy -- -D warnings  # Zero warnings required
cargo fmt --check       # Zero diffs required
```

---

## Unit Tests (256 tests)

| Module | Tests | Coverage |
|--------|------:|----------|
| `lib.rs` | 53 | All 58 WASM bindings + smoke tests |
| `session.rs` | 22 | Cookie parsing, OAuth flow, login detection, token refresh |
| `intercept.rs` | 20 | Price extraction, node normalization, merging, config, XHR caching |
| `vision.rs` | 18 | Config defaults, NMS, detections-to-tree, preprocessing |
| `orchestrator.rs` | 17 | Workflow engine, auto-nav, rollback/retry, cross-page memory |
| `js_eval.rs` | 16 | Detection, evaluation, safety blocking, fetch URL extraction |
| `firewall.rs` | 16 | L1/L2/L3 filtering, batch, MIME types, whitelisting |
| `causal.rs` | 13 | Graph building, prediction, safest path, serialization |
| `js_bridge.rs` | 12 | Selective execution, DOM targeting, XHR extraction |
| `intent.rs` | 11 | Click, fill_form, extract_data edge cases |
| `collab.rs` | 10 | Store operations, agent registration, versioning |
| `compiler.rs` | 9 | Goal compilation, plan execution, serialization |
| `diff.rs` | 9 | Tree comparison, change detection, token savings |
| `grounding.rs` | 9 | Tree grounding, IoU computation, Set-of-Marks |
| `webmcp.rs` | 8 | Tool discovery, schema extraction, polyfill detection |
| `temporal.rs` | 7 | Memory, adversarial detection, prediction, volatility |
| `streaming.rs` | 6 | Streaming parse, early-stopping, depth limit |
| `trust.rs` | 4 | Injection detection, zero-width chars, boundary wrapping |
| `memory.rs` | 4 | Serialization, context operations, invalid JSON |
| `parser.rs` | 2 | HTML parsing, aria-label priority |
| `semantic.rs` | 2 | Relevance scoring, injection detection |

---

## Fixture Tests (30 tests)

20 realistic HTML test fixtures:

| Fixture | Scenario | Tests |
|---------|----------|------:|
| 01 | E-commerce product page | 3 |
| 02 | Login form | 2 |
| 03 | Search results | 2 |
| 04 | Registration form | 1 |
| 05 | Checkout flow | 2 |
| 06 | News article | 1 |
| 07 | Flight booking | 1 |
| 08 | Restaurant menu | 2 |
| 09 | Dashboard | 1 |
| 10 | Hidden injection attack | 1 |
| 11 | Social engineering injection | 1 |
| 12 | Banking transfer | 1 |
| 13 | Real estate listing | 2 |
| 14 | Job listing | 1 |
| 15 | Grocery store | 1 |
| 16 | Settings page | 1 |
| 17 | Wiki article | 1 |
| 18 | Social media | 1 |
| 19 | Contact form | 1 |
| 20 | Large catalog (performance) | 2 |
| — | Injection pattern library | 2 |

---

## Integration Tests (49 tests)

End-to-end tests: HTML → parse → tree → JSON.

Covers: e-commerce, forms, injection detection, semantic diff, JS sandbox, selective execution, temporal memory, intent compiler, workflow memory, vision stubs.

---

## Benchmarks (13 scenarios)

```bash
cargo run --release --bin aether-bench
```

| Benchmark | Avg (us) | Target |
|-----------|----------|--------|
| Parse: simple page (3 elements) | **46** | <50 ms |
| Parse: ecommerce (12 elements) | **186** | <50 ms |
| Parse: login form (6 elements) | **79** | <50 ms |
| Parse: complex page (100 products) | **3,738** | <500 ms |
| Parse: injection page | **48** | <50 ms |
| Top-5: ecommerce | **167** | <50 ms |
| Top-10: complex (100 products) | **3,579** | <500 ms |
| Click: ecommerce find button | **183** | <50 ms |
| Click: complex find button #42 | **3,498** | <500 ms |
| Fill form: login (2 fields) | **82** | <50 ms |
| Extract: ecommerce price | **177** | <50 ms |
| Injection check: safe text | **<1** | <1 ms |
| Injection check: malicious text | **1** | <1 ms |

---

## Head-to-Head: AetherAgent vs Lightpanda

| Benchmark | AetherAgent | Lightpanda | Speedup |
|-----------|-------------|------------|---------|
| Campfire Commerce (100 pages) | **171 ms** | 31,165 ms | **183x** |
| Amiibo crawl (100 pages) | **102 ms** | 26,541 ms | **259x** |
| Parse: simple page | **760 us** | 253 ms | **333x** |
| Parse: ecommerce | **818 us** | 255 ms | **312x** |
| Parse: complex (400+ elements) | **3.7 ms** | 256 ms | **70x** |
| 100 concurrent parses | **142 ms** wall | 785 ms wall | **6x** |

> Full methodology: [`benches/README.md`](../benches/README.md)

---

## Memory

| Scenario | AetherAgent | Lightpanda |
|----------|-------------|------------|
| Idle | **26 MB** RSS | -- |
| Under load (50x complex) | **27 MB** RSS | 19 MB/instance |
| 100 concurrent | **~27 MB** total | **~1.9 GB** total |

---

## Token Savings (Semantic Diff)

| Scenario | Full tree | Delta | Savings |
|----------|-----------|-------|---------|
| Simple page (no change) | 165 tokens | 54 tokens | **67%** |
| Complex page: price update | 8,038 tokens | 55 tokens | **99.3%** |

---

## WebArena-Style Scenarios

| Task | Steps | Total | Per step |
|------|-------|-------|----------|
| Buy cheapest product | 3 | 7.0 ms | 2.3 ms |
| Post a comment | 2 | 4.9 ms | 2.4 ms |
| Create GitLab issue | 2 | 4.9 ms | 2.4 ms |

---

## Live Site Tests

| Test | Site | Time |
|------|------|------|
| fetch/parse | books.toscrape.com | 292 ms |
| fetch/extract | books.toscrape.com | 292 ms |
| fetch/click "Add to basket" | books.toscrape.com | 306 ms |
| fetch/parse | news.ycombinator.com | 159 ms |
| fetch/plan "buy this book" | books.toscrape.com | 217 ms |
| check-injection | — | <1 ms |
| firewall/classify | google-analytics.com | <1 ms |
| diff (price change) | — | <1 ms |

---

## JS Engine: Boa → QuickJS Migration

| Metric | Boa 0.21 | QuickJS | Notes |
|--------|----------|---------|-------|
| Blocked call detection | ~700 us | ~565 us | ~20% faster |
| Heavy selective exec | ~7,895 us | ~7,100 us | ~10% faster |
| ES2023 compliance | Partial | Full | |
| Binary size impact | ~2.5 MB | ~1.5 MB | ~1 MB smaller |
