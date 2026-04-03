# Causal Resonance Field Retrieval (CRFR) v10

**Status:** Produktionsredo, live-verifierad | **Modul:** `src/resonance.rs`
**MCP:** `parse_crfr` + `crfr_feedback` | **HTTP:** `/api/parse-crfr` + `/api/crfr-feedback`

---

## Vad är CRFR?

CRFR är ett nytt retrieval-paradigm som behandlar DOM-trädet som ett **levande resonansfält** istället för ett statiskt index. När en fråga (goal) kommer in skapas en resonansvåg som propagerar genom trädets förälder-barn-relationer. Noder som matchar frågan "lyser upp" — och deras grannar får en svagare glöd via vågpropagation.

### Designprinciper

1. **Determinism > Intelligence** — samma input → samma output, alltid
2. **Structure > Semantics** — DOM-struktur är signalen, inte språkförståelse
3. **Speed > Everything** — sub-ms möjliggör system-design ovanpå
4. **Local optimization > Global models** — ingen träning, ingen modell

Det som gör CRFR unikt:
- **Ingen ONNX-modell krävs** — BM25 + 2048-bit HDC bitvektorer
- **Sub-millisecond** — 637µs cold (6-test), ~0.6ms cache-hit
- **Value-aware** — matchar href, action, name — inte bara synlig text
- **Second-order wave propagation** (GWN) — bevarar peaks, resists over-smoothing
- **Answer-shape scoring** — noder som SER UT som svar rankas högre
- **Bayesian learned weights** — Beta(α,β) med confidence-weighted + negative signal
- **BTSP plasticity** — snabb feedback = starkare inlärning
- **Field-level concept memory** — lär sig begrepp globalt utan LLM
- **LSH pre-filter** — O(1) candidate lookup vid >100 noder
- **Thompson Sampling** — controlled exploration i propagation weights
- **Deterministic ranking** — stabil tie-break, ingen jitter
- **Temporal decay** — kausalt minne + stats dämpas exponentiellt

---

## Hur fungerar det?

### Steg 1: Bygg resonansfält

Varje DOM-nod får ett **resonanstillstånd**:

```
ResonanceState {
    text_hv:        2048-bit Hypervector (text n-gram encoding)
    role:           String (heading, price, button, etc.)
    depth:          u32 (djup i DOM-trädet)
    phase:          f32 (oscillatorfas)
    amplitude:      f32 (resonansstyrka)
    prev_amplitude: f32 (GWN second-order tracking)
    causal_memory:  Hypervector (ackumulerat lärande)
    hit_count:      u32
    last_hit_ms:    u64 (temporal decay + BTSP plasticity)
}
```

**Fältet lagrar också:**
- `node_labels` + `node_values`: BM25-indexerad text + href/action/name
- `bm25_cache`: cachad BM25-index (byggs en gång, sub-ms vid cache-hit)
- `eager_scores`: BM25S pre-computed scores per token (R1)
- `lsh_index`: LSH hash-tabeller för snabb HDC-filtrering (R4)
- `concept_memory`: globala begrepps-HV:er per goal-token (v7)
- `propagation_stats`: Beta(α,β) per roll+riktning (v6 Bayesian)

Fältet byggs en gång per URL och cachas (LRU, 64 entries).

### Steg 2: Propagation (per fråga)

```
propagate(goal):

  Fas 1 — Multi-field initial resonans:
    BM25 eager scores (BM25S, pre-computed per token) + HDC 2048-bit
    Signaler:
      BM25 keyword + value-matchning (75%)
      HDC text n-gram similarity (20%)
      Roll-prioritet (5%, ren tabell)
      Concept memory boost (max 15%, v7 field-level learning)
      DOM depth signal (depth 3-8: +0.05, R8)
    amplitude = Σ signaler + kausal_boost + answer_shape_boost

    Answer-shape scoring (v7):
      +0.3 siffror | +0.2 kort text | +0.15 enheter | +0.15 strukturkontext

    Kausal boost (temporal decay + BTSP plasticity):
      raw = causal_memory.similarity(goal_hv)
      decay = exp(-λ × seconds_since_last_hit)
      causal_boost = raw² × 0.3 × decay

  Fas 2 — GWN convergent propagation (second-order, O(N)):
    Adaptive fan-out: min(4 + ln(N)×8, N) per nod
    GWN second-order: target = max(2×current - previous, propagated)
    Förälder → barn: damping = 0.35 × √(amp) × learned_weight(role:down)
    Barn → förälder: amp = 0.25 × √(amp) × learned_weight(role:up)
    Fassynk: |Δphase| < π/4 → ×1.08
    Konvergens: total_delta < 0.001 (typiskt 2-3 steg, max 6)

    learned_weight() — Beta-distribution + Thompson Sampling:
      stats = propagation_stats["heading:down"]  // Beta(α, β)
      mean = α / (α + β)
      variance = α×β / ((α+β)² × (α+β+1))
      sample = mean ± √variance × 0.5 (deterministic via key hash)
      weight = 0.2 + sample × 1.3

  Fas 2b — Multi-hop micro propagation (v7):
    Value-match noder (amp > 0.3) → boost syskon 15% + 2-hop 8%

  Fas 3 — Deterministic amplitud-gap top-k:
    Sortera amplitude DESC, tie-break node_id ASC
    Klipp vid >30% relativ drop
    Returnera max top_n noder
```

### Steg 3: Kausal feedback (lärande)

```
feedback(goal, successful_node_ids):

  Steg 0 — Temporal decay på all propagation_stats:
    for each (alpha, beta) in stats:
      alpha *= 0.95
      beta *= 0.95
    → Nyare data väger mer, stale bias försvinner gradvis

  Steg 1 — Kausalt minne med BTSP plasticity (per nod):
    goal_hv = Hypervector::from_text_ngrams(goal)
    for each node_id:
      plasticity = 1.5 if feedback < 1s, 1.0 if < 10s, 0.5 if delayed
      if plasticity > 1.2:
        node.causal_memory = bundle(memory, goal_hv, goal_hv)  // dubbel
      else:
        node.causal_memory = bundle(memory, goal_hv)
      node.hit_count += 1

  Steg 2 — Beta-distribution update (per roll):
    For each parent→child edge:
      confidence = child.amplitude (0-1)
      if child was successful:
        stats["parent_role:down"].alpha += confidence      ← Fix 1
      else:
        stats["parent_role:down"].beta += (1 - confidence) ← Fix 2
    For each child→parent edge:
      (samma logik uppåt)
```

Tredelat lärande:
- **Kausalt minne**: vilka noder som hade svaret (VSA-binding)
- **Propagation weights**: Beta(α,β) per roll — confidence-weighted + negative signal
- **Temporal decay**: stats × 0.95 per feedback → nyare data dominerar

`learned_weight()` = Beta mean: `α/(α+β)` → mappas till vikt 0.2-1.5.
Heuristik = initial prior `(h, 1.0)`. Med mer data tar observationer över automatiskt.
Ingen manuell blend-faktor kvar.

---

## Live-verifiering: 20 riktiga sajter

Kört via lokal HTTP-server (`/api/fetch` → `/api/parse-crfr`):

```
  Svar hittat:    15/20 (75%)        — av 16 som gick att fetcha
  Fetch failures:  4/20              — robots.txt / WAF (Wikipedia, BBC, SO)
  Missar:          1/20              — rust-lang.org (rustup i href, inte label)
  Avg latens:      1 046ms           — inkl nätverksfetch
  Avg svar-rank:   3.1               — svaret i topp-3 i snitt
```

| # | Sajt | Fråga | Status | Rank |
|---|------|-------|:------:|:----:|
| 1 | Wikipedia SV | Malmö invånare | FETCH FAIL | — |
| 2 | Wikipedia EN | Sveriges huvudstad | FETCH FAIL | — |
| 3 | **Hacker News** | Senaste nyheter | **OK** | 10 |
| 4 | rust-lang.org | Installera Rust | MISS | — |
| 5 | **python.org** | Python-version | **OK** | **1** |
| 6 | **MDN** | Vad är HTML | **OK** | **1** |
| 7 | **GitHub trending** | Trendande repos | **OK** | **1** |
| 8 | **lobste.rs** | Tekniknyheter | **OK** | 13 |
| 9 | **PyPI** | Vad är PyPI | **OK** | **1** |
| 10 | **crates.io** | Vad är crates.io | **OK** | **1** |
| 11 | BBC News | Senaste nyheter | FETCH FAIL | — |
| 12 | **SVT Nyheter** | SVT nyheter | **OK** | **1** |
| 13 | Stack Overflow | Vad är SO | FETCH FAIL | — |
| 14 | **Expressen** | Expressen nyheter | **OK** | **2** |
| 15 | **DN** | DN nyheter | **OK** | **1** |
| 16 | **Arch Wiki** | Vad är pacman | **OK** | 9 |
| 17 | **Docker Docs** | Vad är Docker | **OK** | **1** |
| 18 | **Node.js** | Node-version | **OK** | **1** |
| 19 | **Go.dev** | Vad är Go | **OK** | **1** |
| 20 | **Aftonbladet** | Aftonbladet nyheter | **OK** | **2** |

**10 av 15 hittade sajter har svaret på rank 1-2.**

---

## Benchmark: CRFR vs ColBERT vs Pipeline

### 6 kontrollerade tester (colbert-small-int8.onnx)

```
┌──────────────────────────────┬──────────┬───────────┬──────────┐
│ Metod                        │ Recall@3 │  Avg µs   │ Speedup  │
├──────────────────────────────┼──────────┼───────────┼──────────┤
│ CRFR v10 (cold)              │ 3/6  50% │     669   │ baseline │
│ CRFR v10 (kausal feedback)   │ 5/6  83% │     —     │    —     │
│ Pipeline (BM25+HDC+Embed)    │ 4/6  67% │  33 147   │ 49.5x   │
│ ColBERT (MaxSim)             │ 5/6  83% │  89 550   │ 133.9x  │
└──────────────────────────────┴──────────┴───────────┴──────────┘
```

### 20 offline-tester (riktiga sajter + fixtures, ONNX)

```
┌──────────────────────────────┬──────┬──────┬───────┬───────┬──────────┬────────┐
│ Metod                        │  @1  │  @3  │  @10  │  @20  │  Avg µs  │ Output │
├──────────────────────────────┼──────┼──────┼───────┼───────┼──────────┼────────┤
│ CRFR v10 (BM25+HDC+cache)   │10/20 │16/20 │ 17/20 │ 17/20 │  14 007  │ 11.7   │
│ Pipeline (BM25+HDC+Embed)    │ 6/20 │10/20 │ 18/20 │ 19/20 │ 407 445  │ 19.8   │
└──────────────────────────────┴──────┴──────┴───────┴───────┴──────────┴────────┘

Speedup:          29.1x
Cache-hit:        617 µs (sub-millisecond)
Token-reduktion:  99% (22 236 HTML-tokens → 273 CRFR-tokens)
```

### 20 live-sajter (via HTTP-server med nätverksfetch)

```
  Svar hittat:    15/20 (75%)
  Fetch failures:  4/20 (Wikipedia, BBC, SO — robots/WAF)
  Missar:          1/20 (rust-lang.org)
  Avg latens:      1 046 ms (inkl nätverksfetch)
  Avg svar-rank:   3.1 (svaret i topp-3 i snitt)
```

### 50 live-sajter — CRFR vs Pipeline head-to-head

```
  Metod                         @1     @3     @5    @10    @20   Avg ms
  CRFR v9                    32/45  40/45  43/45  44/45  44/45     395
  Pipeline (BM25+HDC+Embed)  36/45  43/45  43/45  44/45  44/45     505

  Paritet @20: 97.8% (44/45)
  CRFR @3: 40/45 (89%) — upp från 39/45 i v6
  CRFR 1.3x snabbare (395ms vs 505ms inkl nätverksfetch)
  Gov: 5/5 @3 (perfekt! BoE rank 1)
  Enda miss: IMDB (1 nod — JS-renderad)
```

### Nyckeltal

| Dimension | CRFR v10 | Pipeline (BM25+HDC+Embed) | ColBERT (MaxSim) |
|-----------|:--------:|:-------------------------:|:----------------:|
| **Recall@1 (20 offline)** | **50%** | 30% | — |
| **Recall@3 (20 offline)** | **80%** | 50% | — |
| **Recall@20 (50 live)** | **97.8%** | **97.8%** | — |
| **Latens (cold)** | **14 ms** | 407 ms | 90 ms |
| **Latens (cache hit)** | **0.6 ms** | 407 ms | 90 ms |
| **Latens (6-test cold)** | **0.67 ms** | 33.1 ms | 89.5 ms |
| **Speedup** | **29-50x** | baseline | 0.23x |
| **HV dimension** | **2048-bit** | 4096-bit | 768-dim float |
| **Output-noder** | **6-10** | 16-20 | 5-8 |
| **Token-reduktion** | **99%** | 98.4% | 99.2% |
| **O(N)-garanterad** | **Ja (fan-out cap)** | Ja | Ja |
| **Learned weights** | **Beta-distribution** | Nej | Nej |
| **Negative signal** | **Ja (beta += 1-conf)** | Nej | Nej |
| **Stats decay** | **Ja (×0.95/feedback)** | Nej | Nej |
| **Confidence output** | **Platt-kalibrerad** | Nej | Nej |
| **Incremental update** | **Ja** | Nej | Nej |
| **Cross-URL transfer** | **Ja** | Nej | Nej |
| **Deterministic** | **Ja** | Nej | Nej |
| **Value-aware** | **Ja** | Nej | Nej |
| **Kräver ONNX** | **Nej** | Ja | Ja |

---

## Versionshistorik

### v1 → v2
| Optimering | v1 | v2 |
|------------|----|----|
| Multi-field | text_hv + hv (XOR) | text_hv + role (string) + depth |
| Scoring | 0.7×BM25 + 0.3×HDC | 0.65×BM25 + 0.20×HDC + 0.15×roll |
| Propagation | Fasta vikter | Query-conditioned: √amp × role_factor |
| Learned weights | Alla roller lika | heading 1.3× ned, price 1.4× upp |
| Temporal decay | Ingen | exp(-λ×elapsed), halvering var 10 min |

### v2 → v3
| Optimering | v2 | v3 | Princip |
|------------|----|----|---------|
| **BM25-index** | Byggs per query | Cachad i fältet (617µs hit) | Speed > Everything |
| **Value-aware** | Bara labels | label + href + action + name | Structure > Semantics |
| **Propagation** | Fixed 2 steg | Convergent (delta < 0.001, max 6) | Signal-styrt |
| **Roll-signal** | HV-matchning mot goal | Ren prioritetstabell | Zero semantic |
| **Ranking** | Instabil (HashMap-order) | Deterministic tie-break (node_id) | Determinism > Intelligence |
| **Vikter** | 65/20/15 | 75/20/5 | BM25+value dominerar |
| **Cache-hit** | — | **617 µs** | Sub-ms uppnådd |

### v3 → v4
| Optimering | v3 | v4 | Princip |
|------------|----|----|---------|
| **Propagation O(N)** | O(E×K) men obegränsad fan-out | Fan-out cap (MAX_FAN_OUT=32) | Determinism > Intelligence |
| **Vikter** | Hårdkodade (heading 1.3×, price 1.4×) | Adaptiva: `base × (1 + hit_count×0.05)` | Local optimization > Global models |
| **SIMD** | Standard loops | 4-wide unrolled bind/hamming/bundle | Speed > Everything |
| **Bundle(N)** | Bit-för-bit O(4096×N) | Word-level O(64×N) | Speed > Everything |
| **Batch-ops** | — | similarity_batch(), hamming_batch() | Infrastruktur |
| **Multi-query** | — | propagate_multi_variant() (union merge) | Utnyttjar sub-ms |
| **Persistent cache** | Enbart in-memory LRU | to_json()/from_json() + WASM save/load | Bevarar lärande |
| **I2 stream_engine** | Oberoende scoring | Kontext-boost (barn-grannar +20%) | Structure > Semantics |
| **I7 jämförande** | 1 match per nyckel | extract_by_keys_multi (N per nyckel) | Flexibilitet |
| **Latens cold** | 32 ms | **22 ms** | -30% |
| **Speedup** | 12.3x | **17.0x** | SIMD + optimering |

### v4 → v5
| Optimering | v4 | v5 | Princip |
|------------|----|----|---------|
| **Propagation weights** | `base × (1 + hit×0.05)` (linjär) | Bayesian blend: `(1-b)×heuristic + b×observed` | Local optimization > Global models |
| **Weight tracking** | Per-nod hit_count | Per-roll success/attempts stats | Äkta inlärning, inte heuristik |
| **Blend factor** | Linjär boost | `min(attempts/20, 0.8)` (Bayesian) | Data → dominerar med tid |
| **Stats persistens** | Enbart i minne | Sparas i to_json, överförs via transfer_from | Bevarar lärande mellan sessioner |
| **Incremental update** | Full rebuild | update_node/add_node/remove_node | DOM-mutation utan rebuild |
| **Cross-URL transfer** | — | transfer_from(donor, recipient, min_sim) | Lärande mellan liknande sajter |
| **Confidence calibration** | Raw amplitude | Platt scaling → probability (0-1) | Kalibrerad output |
| **parse_crfr output** | relevance (amplitude) | + confidence (kalibrerad probability) | LLM-vänlig signal |

### v5 → v6
| Optimering | v5 | v6 | Princip |
|------------|----|----|---------|
| **Stats-typ** | `(u32, u32)` räknare | `(f32, f32)` Beta-distribution | Äkta Bayesian |
| **Success signal** | `alpha += 1` | `alpha += confidence` | Confidence-weighted |
| **Negative signal** | Ingen | `beta += (1 - confidence)` | Lär vad som INTE funkar |
| **Stats decay** | Ingen | `(α, β) × 0.95` per feedback | Motverkar stale bias |
| **Blend-faktor** | `min(attempts/20, 0.8)` manuell | Beta mean: `α/(α+β)` automatisk | Ingen manuell konstant |
| **Heuristik-roll** | Prior + linjär boost | Enbart initial prior `(h, 1.0)` | Data tar över naturligt |
| **6-test speedup** | 20.4x | **25.7x** | Snabbare cold-start |
| **6-test causal** | 5/6 | **6/6** | Bättre lärande |
| **50-sajt @20** | — | **44/45 (97.8%)** | PoC-validated |

---

## Varför är CRFR snabbare?

Pipeline-metoden kör tre steg sekventiellt:
1. BM25 keyword retrieval (~0.1 ms)
2. HDC 4096-bit pruning (~0.5 ms)
3. **ONNX embedding inference (~30-80 ms)** — flaskhalsen

CRFR v9 eliminerar steg 3 och optimerar steg 1-2:

**Cold (första besöket — ~12ms):**
1. Bygg BM25S eager scores + 2048-bit HDC HV:er + LSH index (~5 ms)
2. BM25S token-lookup + HDC similarity (fused popcount) (~0.3 ms)
3. GWN second-order propagation (adaptive fan-out, convergent) (~0.1 ms)
4. Answer-shape scoring + gap-filter (~0.01 ms)

**Cache-hit (återbesök — ~0.6ms):**
1. BM25S eager lookup (~0.2 ms)
2. HDC similarity (2048-bit, fused XOR-popcount) (~0.15 ms)
3. GWN propagation + multi-hop + filter (~0.1 ms)

**Multi-query (N varianter — ~N×0.6ms):**
1. `propagate_multi_variant(["price kr", "cost amount", "pris belopp"])`
2. Union merge: max amplitude per nod
3. 3 varianter ≈ 1.8ms

Ingen neural network inference. SIMD-optimerade bitvektoroperationer.
`propagate_multi_variant()` utnyttjar sub-ms cache-hit för högre recall.

---

## Varför är CRFR bättre på recall?

### 1. Vågpropagation ger kontext-medvetenhet

Om en tabellrubrik "National Living Wage" har hög amplitude sprider den energi nedåt till cellen "£12.21" — som inte matchar sökord "wage" men är det faktiska svaret. Pipeline scorar varje nod oberoende och missar detta.

### 2. Answer-shape scoring (v7)

Noder som SER UT som svar rankas högre — utan semantik:
- +0.3 om noden innehåller siffror (priser, datum, befolkning)
- +0.2 om kort text (< 50 tecken — svar är koncisa)
- +0.15 om enhetsmarkörer ($, £, %, kr, km)
- +0.15 om strukturerad kontext (tabellcell med syskon)

### 3. Bayesian learned weights + Thompson Sampling

Propagation-vikter lär sig per sajt via Beta(α,β):
- `feedback()` uppdaterar stats: `α += confidence`, `β += 1-confidence`
- `learned_weight()` = Beta mean + Thompson-noise (utforskar vid lite data)
- Konvergerar mot sajtspecifika optimala vikter automatiskt

### 4. Multi-hop + field-level concept memory (v7/v9)

- Value-match noder boostar syskon (15%) och 2-hop grannar (8%)
- `concept_memory`: aggregerar framgångsrika noders HV per goal-token
- Systemet lär sig "vad price-frågor matchar" globalt — utan LLM

---

## Kausal inlärning — hur systemet blir bättre

```
Besök 1: parse_crfr("price pris cost") → nod 12 har svaret
         crfr_feedback(url, "price pris cost", [12])

Besök 2: parse_crfr("amount total")    → nod 12 får kausal boost
         (causal_memory.similarity("amount") > 0 via delad HDC-rymd)

Besök 3: parse_crfr("belopp kr")       → nod 12 ännu starkare
         (tre goals ackumulerade via majority-vote bundle)
         
         Men: boost dämpas exponentiellt — efter 10 min halverat,
         efter 30 min ~12% kvar. Förhindrar stale bias.
```

---

## API-referens

### MCP-verktyg

**`parse_crfr`** — CRFR-parsing med vågpropagation

```json
{
  "html": "<html>...",
  "goal": "price pris cost £ $ kr amount total",
  "url": "https://shop.com/product",
  "top_n": 20,
  "run_js": false,
  "output_format": "json"
}
```

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `html` | string | — | Raw HTML (eller utelämna om url anges via MCP) |
| `url` | string | — | URL att hämta, eller sidans URL för caching |
| `goal` | string | **required** | **EXPANDERA:** synonymer + översättningar + förväntade värden |
| `top_n` | int | 20 | Max noder. Gap-detection klipper ofta tidigare. |
| `run_js` | bool | false | Kör QuickJS sandbox före parsing (SPA-stöd) |
| `output_format` | string | "json" | "json" eller "markdown" |

**`crfr_feedback`** — Lär systemet vilka noder som var rätt

```json
{
  "url": "https://shop.com/product",
  "goal": "price pris cost",
  "successful_node_ids": [12, 45]
}
```

### HTTP-endpoints

```bash
# Parsning (kräver html-parameter)
curl -X POST http://localhost:3000/api/parse-crfr \
  -H "Content-Type: application/json" \
  -d '{"html":"<h1>Price: $99</h1>","goal":"price cost","url":"https://shop.com"}'

# Feedback
curl -X POST http://localhost:3000/api/crfr-feedback \
  -H "Content-Type: application/json" \
  -d '{"url":"https://shop.com","goal":"price","successful_node_ids":[12]}'
```

### MCP via HTTP (tools/call — stöder URL-fetch)

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"parse_crfr","arguments":{"url":"https://example.com","goal":"example domain"}}}'
```

### WASM API

```javascript
parse_crfr(html, "price pris cost", url, 20, false, "json")
crfr_feedback(url, "price pris cost", "[12, 45]")
```

### Goal expansion (viktigt!)

LLM:en MÅSTE expandera frågan med synonymer innan anrop:

| Användarfråga | Dålig goal | Bra goal |
|---------------|-----------|----------|
| "Vad kostar det?" | "price" | "price pris cost £ $ kr amount total fee belopp" |
| "Vem skrev artikeln?" | "author" | "author författare writer journalist by publicerad reporter" |
| "Hur många bor i Malmö?" | "population Malmö" | "invånare befolkning folkmängd population 357377 Malmö kommun" |

### Output-format

**JSON** (default):
```json
{
  "nodes": [
    {
      "id": 12,
      "role": "price",
      "label": "£12.21",
      "relevance": 0.85,
      "resonance_type": "Direct",
      "causal_boost": 0.0
    }
  ],
  "crfr": {
    "method": "causal_resonance_field",
    "build_tree_ms": 5,
    "propagation_ms": 1,
    "cache_hit": false,
    "js_eval": false
  }
}
```

**Markdown** (token-effektivt):
```markdown
# National Minimum Wage rates

- **[£12.21]** (button)
- National Living Wage (21 and over) £12.21 6.7%

<!-- CRFR: 6/33 nodes, 5ms, cache=false, js=false -->
```

---

## Arkitektur

```
                    ┌─────────────────────┐
  HTML ────────────→│  html5ever parser   │
                    │  + ArenaDom         │
                    └────────┬────────────┘
                             │
                    ┌────────▼────────────┐
  [run_js=true] ──→│  QuickJS sandbox    │ (valfritt)
                    └────────┬────────────┘
                             │
                    ┌────────▼────────────┐
                    │  SemanticBuilder    │
                    │  (roller, labels,   │
                    │   trust, relevans)  │
                    └────────┬────────────┘
                             │ SemanticTree
                    ┌────────▼────────────┐
  Goal ────────────→│  ResonanceField     │
                    │                     │
                    │  Fas 1: Multi-field │
                    │    BM25 + HDC + Roll│
                    │  Fas 2: Propagation │
                    │    Query-conditioned│
                    │    Learned weights  │
                    │  Fas 3: Gap-filter  │
                    └────────┬────────────┘
                             │ Vec<ResonanceResult>
                    ┌────────▼────────────┐
                    │  JSON / Markdown    │
  Agent ◄───────────│  output             │
                    └─────────────────────┘
                             │
  Agent feedback ───→ causal_memory uppdaterad
                     (per nod, per URL, i LRU cache)
```

---

## Filer

| Fil | Beskrivning |
|-----|-------------|
| `src/resonance.rs` | Kärna: ResonanceField, learned propagation, Bayesian weights, confidence calibration, incremental update, cross-URL transfer, multi-query, persistent cache |
| `src/lib.rs` | WASM: `parse_crfr`, `crfr_feedback`, `parse_crfr_multi`, `crfr_save_field`, `crfr_load_field`, `crfr_update_node`, `crfr_transfer`, `extract_data_multi` |
| `src/bin/mcp_server.rs` | MCP stdio: `parse_crfr`, `crfr_feedback` verktyg |
| `src/bin/server.rs` | HTTP: endpoints + MCP dispatch (tools/list + tools/call) |
| `src/stream_engine.rs` | I2: kontext-aware re-ranking (barn-grann boost +20%) |
| `src/intent.rs` | I7: `extract_by_keys_multi()` — jämförande extraktion |
| `src/scoring/hdc.rs` | 2048-bit HV, fused XOR-popcount, LshIndex, batch ops |
| `src/scoring/tfidf.rs` | BM25 (cachad i CRFR, Clone-deriverad) |
| `benches/crfr_vs_colbert.rs` | Kontrollerad benchmark (6 tester) |
| `benches/crfr_final_benchmark.rs` | Offline benchmark (20 sajter, @1/@3/@10/@20) |
| `benches/crfr_live_test.py` | Live-verifiering (20 sajter via HTTP-server) |
| `benches/poc_50_crfr.py` | POC 50-sajt head-to-head (CRFR vs Pipeline) |

---

### v6 → v7
| Optimering | v6 | v7 |
|------------|----|----|
| **Answer-shape scoring** | Ingen | +0.3 siffror, +0.2 kort, +0.15 enheter |
| **Multi-hop** | 1-hop propagation | Value-match → boost syskon + 2-hop |
| **Field memory** | Per-nod minne | + globalt concept_memory per goal-token |
| **Adaptive fan-out** | Fixed 32 | `4 + ln(N)×8` (min N) |
| **Query decomposition** | Enkelt goal | 3-token sliding window + merge |
| **@3** | 15/20 | **16/20** |

### v7 → v8
| Optimering | v7 | v8 | Effekt |
|------------|----|----|--------|
| **HV dimension** | 4096-bit (512 B/HV) | 2048-bit (256 B/HV) | 2× minne + popcount |
| **Popcount** | 4-wide unrolled | Fused simple loop (LLVM auto-vec) | Bättre för 32 words |
| **learned_weight** | format!() per edge | Pre-computed keys | Noll-allokering |
| **transfer_from** | O(N²) nested loop | O(N×bucket) roll-bucketing | Skalbart |
| **BM25 rebuild** | HashSet mellanlager | Single-pass | Färre allokeringar |
| **phase output** | Serialiserades | skip_serializing | Mindre JSON |
| **Dead code** | Gammal learned_weight | Borttagen | Rent |
| **Latens cold** | 28 ms | **12 ms** | -57% |
| **6-test cold** | 1.3 ms | **0.66 ms** | -49% |
| **Speedup** | 14x | **30x** | +114% |

### v8 → v9 (Research-optimeringar)
| Optimering | Källa | Implementation |
|------------|-------|---------------|
| **R1: Eager BM25S** | arXiv 2407.03618 | Pre-compute top-50 scores per token vid build |
| **R2: GWN wave** | arXiv 2505.20034 | Second-order: target = max(2×cur-prev, propagated) |
| **R3: Thompson Sampling** | Stanford | Deterministic pseudo-sample via key hash, variance shrinks |
| **R4: LSH pre-filter** | Springer 2025 | 8 tables × 12 bits, O(1) candidate lookup (>100 noder) |
| **R6: BTSP plasticity** | 2025 preprint | Quick feedback 1.5× imprint, delayed 0.5× |
| **R8: DOM depth signal** | Yun & Masukawa | Depth 3-8: +0.05, depth 2-12: +0.02 |
| **6-test cold** | — | 660 → **637 µs** |
| **6-test causal** | — | 5/6 → **6/6** (BTSP förbättrar feedback) |
| **6-test speedup** | — | 46x → **45.9x** |

### v9 → v10 (Research SOTA integration)
| Optimering | Källa | Implementation | Effekt |
|------------|-------|---------------|--------|
| **CombMNZ fusion** | Cormack 2009 | Multiplicera amp med antal samstämmiga signaler | Reward consensus |
| **Boilerplate zone** | Readability-inspired | nav/footer → ×0.5, wrapper → ×0.7 | Dämpar brus |
| **CMR signal** | DOM extraction 2024 | Mid-length text (20-200 chars) → +0.1 | Svar är mellanlånga |
| **Answer-type detection** | Query classification | price→currency, rate→%, population→large num | **@1: 9→10** |
| **Sibling pattern** | Template detection | Syskongrupp 3+ → boost identiska syskon 10% | Listor/grids |
| **BM25F field-weight** | Robertson 2004 | Value-text dubbel TF-vikt | URL-matchning starkare |
| **PPR restart** | Andersen 2006 | BM25 seed-noder 10% restart i propagation | Anti-over-smoothing |
| **20-test @1** | — | **9→10/20** | +11% precision |

## Kvarvarande optimeringar

Alla identifierade buggar, features och research-optimeringar implementerade (v1→v9).

Implementerade research items (12/20):
✅ #1 CombMNZ | ✅ #2 BM25F | ✅ #3 Zone penalty | ✅ #4 Score norm (min-max)
✅ #6 D-TS (decay) | ✅ #7 Answer-type | ✅ #8 Table-aware | ✅ #9 Sibling pattern
✅ #10 PPR restart | ✅ #11 HDC weighting (concept) | ✅ #13 CMR | ✅ #14 Query expansion (concept)

Deferred (medium complexity, arkitekturella):
- **#5 Cascade architecture** — 3-stage filter (BM25→HDC→shape) för -50% latens
- **#12 WASM SIMD** — i64x2 Hamming för -60% HDC latens
- **#15 LinUCB contextual bandits** — per-sida feature-vektor
- **#16 Chebyshev spectral filter** — ersätt wave med polynomial filter
- **#17 Hierarchical HDC** — subtree-encodade HV:er
- **#18 Learned RMI** — O(1) concept memory lookup
- **#19 Template detection** — Zhang-Shasha tree edit distance
- **#20 Sparse block codes** — 2048-bit med blockstruktur
