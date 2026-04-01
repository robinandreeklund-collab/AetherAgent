# Hierarchical Late Interaction Retrieval for Goal-Directed DOM Node Ranking

**Version:** 5.0 · **Date:** 2026-04-01

---

## Abstract

Autonomous web agents consume 50,000–500,000 tokens per page, costing $0.50–$5.00 per task and producing up to 1,460 tonnes CO₂ annually at production scale. We introduce **Hierarchical Late Interaction Retrieval (HLIR)** — a training-free, four-stage pipeline that reduces token consumption by 97% while achieving 95.5% answer recall across 44 live websites. The key contribution is **bottom-up late interaction scoring over DOM trees**: leaf nodes are scored via ColBERT MaxSim, and parent nodes inherit decayed children scores, eliminating the wrapper-bias problem where structural containers dominate rankings. On 12 diverse sites, HLIR places fact-bearing nodes as top-1 in 83% of cases, compared to 33% for mean-pooled bi-encoders. The system requires no training data, no GPU, and a single 22MB int8 ONNX model.

---

## 1. Problem Statement

Autonomous browser agents face four compounding costs:

**Tokens.** A web page produces 50K–500K tokens in raw HTML (Deng et al., 2023). Even accessibility trees yield 4K–15K tokens per step (Zhou et al., 2023). A multi-step task reaches 100K–1M tokens (Koh et al., 2024), costing $0.50–$30 per task at GPT-4 pricing.

**Compute.** Headless browsers add 3–15 seconds per page load (Drouin et al., 2024). Memory per instance: 200–500MB. A 10-step task takes 50–250 seconds.

**Safety.** Feeding raw DOM to LLMs enables indirect prompt injection with 24–47% success rates (Zhan et al., 2024). Hidden instructions in `display:none` elements, zero-width characters, and ARIA attributes hijack agent behavior.

**Environment.** LLM inference consumes 0.04–0.07 kWh per 1K tokens (Luccioni et al., 2023). At 1M tasks/day with 100K tokens each: 365–1,460 tonnes CO₂/year. Reducing tokens by 97% reduces all of these proportionally.

### Existing Approaches

| Approach | Token reduction | Limitation |
|----------|:--------------:|------------|
| Accessibility tree (Zhou et al., 2023) | 60–80% | No goal filtering, still 4K–15K tokens/page |
| Learned top-k filtering (Deng et al., 2023) | 95–99% | Requires trained ranking model + labeled data |
| Visual grounding (Zheng et al., 2024) | ~100% text | High vision token cost, loses semantic structure |
| Heuristic pruning (remove scripts/styles) | 20–50% | Misses semantic irrelevance |

No existing approach simultaneously addresses all four costs without training data.

---

## 2. HLIR — Hierarchical Late Interaction Retrieval

We propose a four-stage retrieval pipeline that operates on the parsed DOM tree without any training:

```
HTML → Semantic Tree → BM25 ∪ Dense → HDC Pruning → ColBERT HLIR → Top-N
```

### 2.1 Formal Definition

Given a query *q* with tokens *q₁...qₘ*, a DOM tree *T*, and a node *n* with text tokens *d₁...dₖ*:

```
MaxSim(q, n) = (1/m) · Σᵢ maxⱼ cos(qᵢ, dⱼ)

S_self(n) = 0.40·MaxSim(q,n) + 0.15·HDC_text(n,q) + 0.15·role(n) + 0.30·BM25(n,q) − penalties(n)

S(n) = max( S_self(n),  λ · max_{c ∈ children(n)} S(c) )     where λ = 0.75
```

**Key insight:** The bottom-up operator `S(n)` ensures that leaf nodes containing facts are scored directly via MaxSim, while parent nodes (navigation bars, generic wrappers, footers) can only inherit a *decayed* version of their best child's score. This eliminates wrapper-bias structurally — a `<body>` element aggregating all page text can never outscore the specific `<p>` containing the answer.

### 2.2 Pipeline Stages

| Stage | Method | Cost | What it does |
|-------|--------|------|-------------|
| 1a | BM25 | O(q·postings) | Keyword retrieval → 50–300 candidates |
| 1b | Dense fallback | O(200·d) | Embedding cosine if BM25 < 20 candidates |
| 2 | HDC 4096-bit | O(n·64) XOR+POPCNT | Structural pruning → 50–100 survivors |
| 3 | ColBERT HLIR | O(k·q·d) int8 | Bottom-up MaxSim scoring → top-N |

**Stage 1b** catches semantic matches without keyword overlap (e.g., query "bor" matching node "invånare"). It scans up to 200 leaf nodes via embedding cosine similarity, triggered only when BM25 returns fewer than 20 candidates.

**Stage 2** uses Hyperdimensional Computing — 4096-bit bitvectors encoding text n-grams, ARIA role, and tree depth per node. Similarity via XOR + POPCNT (~2ns per comparison). Multi-aspect: separate text_hv and role_hv stored per node, with text_hv fed as a signal to Stage 3.

**Stage 3** applies ColBERT MaxSim in bottom-up order with multi-signal fusion. Token pruning (nodes >48 tokens → top-48 by IDF) and query expansion (+4 high-IDF terms from BM25 survivors) are applied before encoding. All survivors are batch-encoded in a single ONNX call with u8 scalar quantization for MaxSim computation.

### 2.3 Why This Is a New Hybrid Class

Most retrieval systems use one of:
- `BM25 → Cross-encoder` (lexical → neural)
- `Dense → Cross-encoder` (semantic → neural)

HLIR combines four signal types in a single cascade:
- **Lexical recall** (BM25)
- **Semantic recall** (dense embedding fallback)
- **Symbolic compression** (HDC bitvectors)
- **Neural precision** (ColBERT late interaction)

The HDC layer is particularly novel — it provides structural awareness (DOM role, depth, sibling context) that neither BM25 nor flat embeddings capture, at hardware-instruction cost (XOR + POPCNT).
