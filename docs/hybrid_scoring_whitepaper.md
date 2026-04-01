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
