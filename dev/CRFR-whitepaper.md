# Causal Resonance Field Retrieval: A Neural-Network-Free Paradigm for DOM Content Extraction

**Authors:** AetherAgent Team
**Date:** April 2026
**Version:** CRFR v12

---

## Abstract

We present Causal Resonance Field Retrieval (CRFR), a novel information retrieval paradigm that treats the DOM tree as a living resonance field rather than a static index. CRFR achieves 97.8% recall@20 across 50 diverse live websites and 99.2% token reduction (22,236 → 185 tokens) without requiring neural network inference, embedding models, or GPU hardware.

The system combines BM25 keyword matching with 2048-bit Hyperdimensional Computing (HDC) bitvectors and physics-inspired wave propagation through parent-child DOM relationships. A Bayesian feedback loop with Beta-distribution learned weights enables the system to improve with use — each successful extraction strengthens future queries on the same site.

Empirical evaluation on 8 real-world websites demonstrates that CRFR reduces a 2.7-million character Wikipedia article to 521 characters while preserving the answer, and cuts LLM API costs from $3.97 to $0.002 per batch. Cold-start latency is 14ms (29× faster than BM25+ColBERT pipeline), with sub-millisecond cache hits at 0.6ms.

CRFR is implemented in 2,100 lines of Rust, compiles to a 1.8 MB binary (without server dependencies), uses 14 MB RSS at idle, and requires zero external model files. It is production-deployed as an MCP tool, HTTP API, and WASM library.

---

## 1. Introduction

### 1.1 The Problem: Raw HTML is Unusable for LLMs

Modern AI agents need to extract information from web pages. The naive approach — sending raw HTML to a Large Language Model — fails at scale:

- A typical news article is 50,000–500,000 characters of HTML
- A Wikipedia article can exceed 2,700,000 characters
- An e-commerce page with React/Vue SPA can be 1,300,000+ characters
- LLM context windows are 128K–1M tokens, but cost scales linearly
- At GPT-4o pricing ($2.50/Mtok input), raw HTML costs $0.50–$3.50 per page

**The core question:** Can we reduce web page content to the 0.1% that actually answers the user's question, in under 15 milliseconds, without neural networks?

### 1.2 Existing Approaches and Their Limitations

| Approach | Recall@3 | Latency | Requires | Limitation |
|----------|:--------:|:-------:|:--------:|------------|
| Raw HTML to LLM | 100% | 0ms | Nothing | Unusable cost, exceeds context |
| BM25 keyword search | ~60% | 1ms | Nothing | Vocabulary mismatch |
| TF-IDF + reranking | ~70% | 5ms | Nothing | No structural awareness |
| ColBERT (neural) | ~83% | 90ms | 23MB ONNX model | GPU recommended, cold start |
| BM25 + HDC + ColBERT | ~80% | 30ms | 23MB ONNX model | Three-stage pipeline overhead |
| Readability extraction | N/A | 5ms | Nothing | Extracts article, not answers |
| CSS selector rules | N/A | 1ms | Manual rules | Brittle, per-site maintenance |

All existing approaches either require neural network inference (slow, heavy) or lack the structural awareness to find specific answers within pages (BM25 alone).

### 1.3 Our Contribution: CRFR

CRFR introduces a fundamentally different approach:

1. **The DOM as a resonance field.** Instead of scoring nodes independently, CRFR treats the DOM tree as a physical medium through which relevance propagates as waves. A heading's relevance flows down to its content; a data cell's relevance flows up to its row.

2. **Zero neural network dependency.** All scoring uses BM25 term matching + 2048-bit HDC bitvector similarity + structural heuristics. No embedding model, no ONNX runtime, no GPU.

3. **Causal learning without retraining.** The system learns from agent feedback via local VSA (Vector Symbolic Architecture) binding. No backpropagation, no gradient descent — just hypervector bundling that strengthens nodes associated with successful extractions.

4. **Answer-shape awareness.** CRFR recognizes that answers have structural signatures: they contain numbers, currency symbols, units, and appear in structured contexts (tables, lists). This is not semantic understanding — it is statistical pattern recognition on DOM structure.

**Result:** 97.8% recall@20 on 50 live websites, 99.2% token reduction, 14ms cold latency, 0.6ms cache hit, 1.8 MB binary, zero model files.

---

## 2. Architecture Overview

CRFR processes a web page in three phases:

```
Phase 1: Field Construction (once per URL, cached)
  HTML → html5ever parser → ArenaDom → SemanticBuilder → SemanticTree
  SemanticTree → ResonanceField (per-node HV + BM25 index + metadata)

Phase 2: Goal Propagation (per query, ~14ms cold / ~0.6ms cached)
  Goal text → BM25 scoring + HDC similarity + structural signals
  → Cascade pre-filter (top-200 candidates)
  → GWN second-order wave propagation (convergent, O(N))
  → Multi-hop expansion + answer-shape + diversity filter
  → Amplitude-gap top-k selection

Phase 3: Causal Feedback (optional, per successful extraction)
  Successful node IDs → VSA binding into causal memory
  → Beta-distribution update of propagation weights
  → Domain-level aggregation for cross-URL transfer
```

### 2.1 The Resonance Field

Each DOM node is assigned a **ResonanceState**:

```rust
ResonanceState {
    text_hv:        [u64; 32],    // 2048-bit Hypervector (text n-gram encoding)
    role:           String,       // Semantic role (heading, price, button, ...)
    depth:          u32,          // DOM tree depth
    amplitude:      f32,          // Current resonance strength
    prev_amplitude: f32,          // Previous (for GWN second-order)
    causal_memory:  [u64; 32],    // Accumulated learning from past successes
    hit_count:      u32,          // Number of successful feedback events
    last_hit_ms:    u64,          // Timestamp for temporal decay + BTSP plasticity
}
```

The field also maintains:
- **BM25 inverted index** — cached, incrementally updatable
- **LSH hash tables** — 8 tables × 12 bits for O(1) candidate pre-filtering
- **Concept memory** — aggregated HVs per goal-token (field-level learning)
- **Propagation stats** — Beta(α,β) per role+direction (Bayesian learned weights)
- **Domain profile** — shared priors across URLs from the same domain

Memory per field: ~5 MB for a 10,000-node page. LRU cache holds 64 fields with 3-minute TTL.

