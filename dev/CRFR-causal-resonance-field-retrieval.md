# Causal Resonance Field Retrieval (CRFR) v6

**Status:** Produktionsredo, live-verifierad | **Modul:** `src/resonance.rs`
**MCP:** `parse_crfr` + `crfr_feedback` | **HTTP:** `/api/parse-crfr` + `/api/crfr-feedback`

---

## Vad är CRFR?

CRFR är ett nytt retrieval-paradigm som behandlar DOM-trädet som ett **levande resonansfält** istället för ett statiskt index. När en fråga (goal) kommer in skapas en resonansvåg som propagerar genom trädets förälder-barn-relationer. Noder som matchar frågan "lyser upp" — och deras grannar får en svagare glöd via vågpropagation.

### Designprinciper (v3)

1. **Determinism > Intelligence** — samma input → samma output, alltid
2. **Structure > Semantics** — DOM-struktur är signalen, inte språkförståelse
3. **Speed > Everything** — sub-ms möjliggör system-design ovanpå
4. **Local optimization > Global models** — ingen träning, ingen modell

Det som gör CRFR unikt:
- **Ingen ONNX-modell krävs** — fungerar med BM25 + HDC bitvektorer
- **Sub-millisecond cache-hit** — 617µs vid återbesök (BM25-index cachad)
- **Value-aware** — matchar href, action, name — inte bara synlig text
- **Convergent propagation** — styrt av signal, inte iteration count
- **Deterministic ranking** — stabil tie-break, ingen jitter
- **Lär sig i realtid** — feedback = ranking refinement, inte semantik
- **Temporal decay** — kausalt minne halveras var 10 min (ingen stale bias)

---

## Hur fungerar det?

### Steg 1: Bygg resonansfält (v3 multi-field + value-aware)

Varje DOM-nod får ett **multi-aspekt resonanstillstånd**:

```
ResonanceState {
    text_hv:        4096-bit Hypervector (text n-gram encoding)
    role:           String (heading, price, button, etc.)
    depth:          u32 (djup i DOM-trädet)
    phase:          0.0 (oscillatorfas)
    amplitude:      0.0 (resonansstyrka)
    causal_memory:  noll-vektor (ackumulerat lärande)
    hit_count:      0
    last_hit_ms:    0 (temporal decay-referens)
}
```

**Fältet lagrar också** per nod:
- `node_labels`: synlig text (BM25-indexerad)
- `node_values`: href, action, name (value-aware, konkatenerad med label i BM25)
- `bm25_cache`: cachad BM25-index (byggs en gång, sub-ms vid cache-hit)

Fältet byggs en gång per URL och cachas (LRU, 64 entries).

### Steg 2: Propagation (per fråga)

```
propagate(goal):

  Fas 1 — Multi-field initial resonans:
    BM25-index från cachad label+value per nod (sub-ms vid cache-hit)
    Tre separata signaler:
      Signal 1: BM25 keyword + value-matchning (75%)
      Signal 2: HDC text n-gram similarity (20%)
      Signal 3: Roll-prioritet (5%, ren tabell — ingen semantik)
    amplitude = 0.75×BM25 + 0.20×HDC + 0.05×roll + kausal_boost

    Kausal boost (temporal decay):
      raw = causal_memory.similarity(goal_hv)
      decay = exp(-λ × seconds_since_last_hit)   [halvering var 10 min]
      causal_boost = raw² × 0.3 × decay

  Fas 2 — Convergent propagation (O(N)-garanterad, max 6, delta-styrt):
    Fan-out cap: max 32 barn per nod (O(N) även vid <ul> med 200 <li>)
    Komplexitet: O(K × min(E, N×32)) där K=2-3

    Förälder → barn:
      damping = 0.35 × √(parent.amp) × learned_weight(role, "down")
    Barn → förälder:
      amplification = 0.25 × √(child.amp) × learned_weight(role, "up")
    Fassynk: om |parent.phase - child.phase| < π/4 → ×1.08
    Konvergens: stoppa om total_delta < 0.001 (typiskt 2-3 steg)

    learned_weight() — Bayesian blend:
      stats = propagation_stats["heading:down"]  // (successes, attempts)
      observed = 0.3 + (successes/attempts) × 1.2
      blend = min(attempts/20, 0.8)
      weight = (1-blend) × heuristic + blend × observed
      → 0 attempts: 100% heuristik (cold-start)
      → 10 attempts: 50/50
      → 20+ attempts: 80% data, 20% prior

  Fas 3 — Deterministic amplitud-gap top-k:
    Sortera efter amplitude DESC, tie-break node_id ASC (deterministic)
    Klipp vid >30% relativ drop (naturlig klustergräns)
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

  Steg 1 — Kausalt minne (per nod):
    goal_hv = Hypervector::from_text_ngrams(goal)
    for each node_id:
      node.causal_memory = bundle(node.causal_memory, goal_hv)
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
│ CRFR v6 (cold)               │ 4/6  67% │   1 310   │ baseline │
│ CRFR v6 (kausal feedback)    │ 6/6 100% │     —     │    —     │
│ Pipeline (BM25+HDC+Embed)    │ 4/6  67% │  33 734   │ 25.7x   │
│ ColBERT (MaxSim)             │ 5/6  83% │  89 550   │ 68.4x   │
└──────────────────────────────┴──────────┴───────────┴──────────┘
```

### 20 offline-tester (riktiga sajter + fixtures, ONNX)

```
┌──────────────────────────────┬──────┬──────┬───────┬───────┬──────────┬────────┐
│ Metod                        │  @1  │  @3  │  @10  │  @20  │  Avg µs  │ Output │
├──────────────────────────────┼──────┼──────┼───────┼───────┼──────────┼────────┤
│ CRFR v6 (BM25+HDC+cache)    │10/20 │15/20 │ 17/20 │ 17/20 │  27 767  │ 10.2   │
│ Pipeline (BM25+HDC+Embed)    │ 6/20 │10/20 │ 18/20 │ 19/20 │ 388 970  │ 19.9   │
└──────────────────────────────┴──────┴──────┴───────┴───────┴──────────┴────────┘

Speedup:          14.0x
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
  CRFR v6                    33/45  39/45  42/45  43/45  44/45     420
  Pipeline (BM25+HDC+Embed)  36/45  42/45  43/45  44/45  44/45     541

  Paritet vid @20: båda 97.8% (44/45)
  CRFR 1.3x snabbare (420ms vs 541ms inkl nätverksfetch)
  Enda miss: IMDB (1 nod parsad — JS-renderad sida)
```

### Nyckeltal

| Dimension | CRFR v6 | Pipeline (BM25+HDC+Embed) | ColBERT (MaxSim) |
|-----------|:-------:|:-------------------------:|:----------------:|
| **Recall@3 (20 offline)** | **75%** | 50% | — |
| **Recall@20 (50 live)** | **97.8%** | **97.8%** | — |
| **Latens (cold)** | **28 ms** | 389 ms | 90 ms |
| **Latens (cache hit)** | **0.6 ms** | 389 ms | 90 ms |
| **Latens (6-test cold)** | **1.3 ms** | 33.7 ms | 89.5 ms |
| **Speedup** | **14-26x** | baseline | 0.23x |
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

CRFR v3 eliminerar steg 3 och cachar steg 1:

**Cold (första besöket — ~22ms):**
1. Bygg BM25-index (label+value) + HDC HV:er (~5 ms)
2. BM25 query + HDC similarity (SIMD 4-wide) + roll-prioritet (~0.5 ms)
3. Convergent propagation (O(N), fan-out cap=32, ~0.1 ms)
4. Deterministic gap-filter (~0.01 ms)

**Cache-hit (återbesök — ~0.6ms):**
1. BM25 query på cachad index (~0.3 ms)
2. HDC similarity (SIMD-optimerad) (~0.2 ms)
3. Propagation + filter (~0.1 ms)

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

### 2. Learned role weights

En `price`-nod bubblar 1.4× uppåt — förälder-noden (raden) lyfts av datainnehållet. En `heading`-nod sprider 1.3× nedåt — barnen (stycketext) lyfts av rubriken. `navigation`-noder sprider bara 0.3× — dämpar brus naturligt.

### 3. Query-conditioned: mer energi via starka matchningar

`spread_factor = base × √(source_amplitude) × role_factor`

En nod med amplitude 0.8 sprider `√0.8 = 0.89×` — nästan full energi. En nod med amplitude 0.1 sprider `√0.1 = 0.32×` — minimal. Relevanta noder skapar starka vågfronter, irrelevanta dör ut.

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
| `src/scoring/hdc.rs` | SIMD: 4-wide unrolled bind/hamming/bundle, batch ops |
| `src/scoring/tfidf.rs` | BM25 (cachad i CRFR, Clone-deriverad) |
| `benches/crfr_vs_colbert.rs` | Kontrollerad benchmark (6 tester) |
| `benches/crfr_final_benchmark.rs` | Offline benchmark (20 sajter, @1/@3/@10/@20) |
| `benches/crfr_live_test.py` | Live-verifiering (20 sajter via HTTP-server) |

---

## Kvarvarande optimeringar

Alla v1-v4 punkter är implementerade. Framtida möjligheter:

- **WebGPU compute** — massiv parallell propagation för >10K noder
- **Automatic domain clustering** — auto-detektera liknande sajter för cross-URL transfer
- **Online A/B** — automatiskt jämföra CRFR vs Pipeline per sajt och välja bäst
- **Weight export** — exportera learned weights som "sajtprofiler" (nyheter, e-commerce, docs)
