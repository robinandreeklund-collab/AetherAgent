# Causal Resonance Field Retrieval (CRFR)

**Status:** Produktionsredo | **Modul:** `src/resonance.rs`
**MCP:** `parse_crfr` + `crfr_feedback` | **HTTP:** `/api/parse-crfr` + `/api/crfr-feedback`

---

## Vad är CRFR?

CRFR är ett nytt retrieval-paradigm som behandlar DOM-trädet som ett **levande resonansfält** istället för ett statiskt index. När en fråga (goal) kommer in skapas en resonansvåg som propagerar genom trädets förälder-barn-relationer. Noder som matchar frågan "lyser upp" — och deras grannar får en svagare glöd via vågpropagation.

Det som gör CRFR unikt:
- **Ingen ONNX-modell krävs** — fungerar med bara BM25 + HDC bitvektorer
- **Lär sig i realtid** — varje lyckad extraktion gör systemet bättre, utan reträning
- **Naturlig top-k** — istället för en hård gräns hittar systemet naturliga kluster via amplitud-gap

---

## Hur fungerar det?

### Steg 1: Bygg resonansfält

Varje DOM-nod får ett **resonanstillstånd**:

```
ResonanceState {
    text_hv:        4096-bit Hypervector (text n-gram encoding)
    hv:             text_hv XOR roll_hv (kombinerad text+roll)
    phase:          0.0 (oscillatorfas)
    amplitude:      0.0 (resonansstyrka)
    causal_memory:  noll-vektor (ackumulerat lärande)
    hit_count:      0 (antal lyckade matchningar)
}
```

Fältet byggs en gång per URL och cachas (LRU, 64 entries).

### Steg 2: Propagation (per fråga)

```
propagate(goal):

  Fas 1 — Initial resonans (BM25 + HDC hybrid):
    Bygg BM25-index från alla nod-labels
    Kör BM25-query → normalisera scores till [0, 1]
    Beräkna HDC text-similarity → normalisera [-1,1] → [0,1]²
    amplitude = 0.7 × BM25 + 0.3 × HDC + kausal_boost
    
  Fas 2 — Vågpropagation (2 iterationer):
    Förälder → barn:  child.amp = max(child.amp, parent.amp × 0.35)
    Barn → förälder:  parent.amp = max(parent.amp, child.amp × 0.25)
    Fassynk-bonus: om |parent.phase - child.phase| < π/4 → ×1.08
    
  Fas 3 — Samla resultat:
    Sortera noder efter amplitud (fallande)
    Klipp vid amplitud-gap > 30% relativ drop (naturlig top-k)
    Returnera max top_n noder
```

**Nyckelinsikt**: BM25 ger keyword-precision (steg 1), HDC ger strukturell signal, och vågpropagation sprider relevans till kontext-noder (steg 2). Kombinationen ger bättre recall än varje del för sig.

### Steg 3: Kausal feedback (lärande)

```
feedback(goal, successful_node_ids):
  goal_hv = Hypervector::from_text_ngrams(goal)
  for each node_id:
    node.causal_memory = bundle(node.causal_memory, goal_hv)
    node.hit_count += 1
```

Nästa gång en liknande fråga ställs på samma URL boostar det kausala minnet de noder som gav rätt svar förra gången. Ingen global modellträning — bara lokal VSA-binding.

---

## Benchmark: CRFR vs ColBERT vs Pipeline

### 6 kontrollerade tester (colbert-small-int8.onnx)

Samma 6 testfall körda med alla metoder. Varje test: HTML-sida + fråga + förväntat svar.

```
┌──────────────────────────────┬──────────┬───────────┬──────────┐
│ Metod                        │ Recall@3 │  Avg µs   │ Speedup  │
├──────────────────────────────┼──────────┼───────────┼──────────┤
│ CRFR (cold, ingen lärande)   │ 6/6 100% │   6 828   │ baseline │
│ CRFR (med kausal feedback)   │ 6/6 100% │     —     │    —     │
│ Pipeline (BM25+HDC+Embed)    │ 4/6  67% │  31 896   │  4.7x    │
│ ColBERT (MaxSim)             │ 5/6  83% │  89 550   │ 13.1x   │
└──────────────────────────────┴──────────┴───────────┴──────────┘
```

CRFR hittar svaret i **alla** 6 tester redan utan lärande.
Pipeline missar 2 (BoE styrränta + Living Wage tabell).
ColBERT missar 1 (Living Wage).

### 20 riktiga sajter (Apple, GitHub, Expressen, DI, HN + 12 fixtures)

```
┌──────────────────────────────┬──────┬──────┬───────┬───────┬──────────┬────────┐
│ Metod                        │  @1  │  @3  │  @10  │  @20  │  Avg µs  │ Output │
├──────────────────────────────┼──────┼──────┼───────┼───────┼──────────┼────────┤
│ CRFR (BM25+HDC+cache)        │ 9/20 │16/20 │ 18/20 │ 18/20 │  29 173  │  9.2   │
│ Pipeline (BM25+HDC+Embed)    │ 6/20 │10/20 │ 18/20 │ 19/20 │ 378 764  │ 20.1   │
└──────────────────────────────┴──────┴──────┴───────┴───────┴──────────┴────────┘

Speedup:          13x
Token-reduktion:  99% (22 236 HTML-tokens → 249 CRFR-tokens)
```

### Nyckeltal

| Dimension | CRFR | Pipeline (BM25+HDC+Embed) | ColBERT (MaxSim) |
|-----------|:----:|:-------------------------:|:----------------:|
| **Recall@3** | **80%** | 50% | 83% (6 tester) |
| **Latens (cold)** | **6.8 ms** | 32 ms | 90 ms |
| **Latens (cache hit)** | **0.3 ms** | 32 ms | 90 ms |
| **Output-noder** | **6-10** | 16-20 | 5-8 |
| **Token-reduktion** | **99%** | 98.6% | 99.2% |
| **Lär sig** | **Ja** | Nej | Nej |
| **Kräver ONNX** | **Nej** | Ja | Ja |

---

## Varför är CRFR snabbare?

Pipeline-metoden kör tre steg sekventiellt:
1. BM25 keyword retrieval (~0.1 ms)
2. HDC 4096-bit pruning (~0.5 ms)
3. **ONNX embedding inference (~30-80 ms)** ← flaskhalsen

CRFR eliminerar steg 3 helt. Istället:
1. BM25 keyword retrieval (~0.1 ms)
2. HDC similarity + BM25 hybrid scoring (~0.5 ms)
3. Vågpropagation genom trädrelationer (~0.1 ms)

**Ingen neural network inference.** All scoring sker via bitvektoroperationer (XOR + popcount) och BM25 term-matchning. På cache-hit (samma URL besökt igen) hoppar vi över steg 1-2 och kör bara propagation (~0.3 ms).

---

## Varför är CRFR bättre på recall?

Två anledningar:

### 1. Vågpropagation ger kontext-medvetenhet

Om en nod har hög BM25-score sprider den sin amplitude till barn och föräldrar. En tabellcell med "£12.21" som inte matchar sökordet "wage" kan fortfarande rankas högt om dess granne (tabellrubriken "National Living Wage") har hög amplitude.

Pipeline-metoden scorar varje nod oberoende — den ser inte att "£12.21" sitter bredvid "National Living Wage".

### 2. BM25 i fas-1 ger exakta keyword-matchningar

Pipeline-metoden viktlägger embeddings som ibland föredrar semantiskt liknande men fel noder (t.ex. "The National Living Wage is the minimum pay..." istället för den faktiska tabellraden med siffran).

CRFR viktar BM25 70% i fas-1, vilket ger exakta keyword-matchningar hög initial amplitude som sedan sprids via propagation.

---

## Kausal inlärning — hur systemet blir bättre

```
Besök 1: parse_crfr("price") → nod 12 har svaret
         crfr_feedback(url, "price", [12])
         
Besök 2: parse_crfr("cost")  → nod 12 får kausal boost
         (causal_memory.similarity("cost") > 0 eftersom "price" och "cost"
          delar n-gram-mönster i HDC-rymden)
          
Besök 3: parse_crfr("pris")  → nod 12 får ännu starkare boost
         (tre goals ackumulerade i causal_memory via majority-vote bundle)
```

Causal memory lagras per nod i resonansfältet. Fältet cachas per URL (LRU, 64 entries). Inget behöver sparas till disk — minnet lever i processens livstid.

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
| `html` | string | — | Raw HTML (eller utelämna om url anges) |
| `url` | string | — | URL att hämta, eller sidans URL för caching |
| `goal` | string | **required** | Expanderad fråga med synonymer |
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
# Parsning
curl -X POST http://localhost:3000/api/parse-crfr \
  -H "Content-Type: application/json" \
  -d '{"goal":"price pris cost","url":"https://shop.com"}'

# Feedback
curl -X POST http://localhost:3000/api/crfr-feedback \
  -H "Content-Type: application/json" \
  -d '{"url":"https://shop.com","goal":"price","successful_node_ids":[12]}'
```

### WASM API

```javascript
// Parsning
const result = parse_crfr(html, "price pris cost", url, 20, false, "json");

// Feedback
crfr_feedback(url, "price pris cost", "[12, 45]");
```

### Goal expansion (viktigt!)

LLM:en MÅSTE expandera frågan med synonymer innan anrop:

| Användarfråga | Dålig goal | Bra goal |
|---------------|-----------|----------|
| "Vad kostar det?" | "price" | "price pris cost £ $ kr amount total fee belopp" |
| "Vem skrev artikeln?" | "author" | "author författare writer journalist by publicerad reporter" |
| "Hur många bor i Malmö?" | "population Malmö" | "invånare befolkning folkmängd population 357377 Malmö kommun antal" |

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
- [Current rates (from 1 April 2025)](...)

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
                    │  SemanticBuilder    │
                    │  (roller, labels,   │
                    │   trust, relevans)  │
                    └────────┬────────────┘
                             │ SemanticTree
                    ┌────────▼────────────┐
  Goal ────────────→│  ResonanceField     │
                    │                     │
                    │  Fas 1: BM25 + HDC  │
                    │  Fas 2: Propagation │
                    │  Fas 3: Gap-filter  │
                    └────────┬────────────┘
                             │ Vec<ResonanceResult>
                    ┌────────▼────────────┐
                    │  JSON / Markdown    │
  Agent ◄───────────│  output             │
                    └─────────────────────┘
                             │
  Agent feedback ───→ causal_memory uppdaterad
                     (per nod, per URL, i cache)
```

---

## Filer

| Fil | Beskrivning |
|-----|-------------|
| `src/resonance.rs` | Kärn-implementation: ResonanceField, propagation, feedback, LRU cache |
| `src/lib.rs` | WASM API: `parse_crfr()`, `crfr_feedback()` |
| `src/bin/mcp_server.rs` | MCP stdio-server: `parse_crfr`, `crfr_feedback` verktyg |
| `src/bin/server.rs` | HTTP-server: `/api/parse-crfr`, `/api/crfr-feedback`, MCP dispatch |
| `benches/crfr_vs_colbert.rs` | Kontrollerad benchmark (6 tester, CRFR vs Pipeline vs ColBERT) |
| `benches/crfr_final_benchmark.rs` | Stor benchmark (20 sajter, @1/@3/@10/@20) |
| `src/scoring/hdc.rs` | Hypervector (4096-bit, XOR bind, majority bundle) |
| `src/scoring/tfidf.rs` | BM25 (integrerad i CRFR fas-1) |

---

## Kvarvarande optimeringar

- **I2**: stream_engine kontext-aware re-ranking — löst av CRFR:s propagation, men stream_engine.rs orörd
- **I7**: Jämförande extraktion ("X vs Y") — kräver multi-match per nyckel i extract_data
- **Temporal phase decay** — noder som inte matchats på länge kunde "somna" (sänka amplitude)
- **SIMD-optimering** — propagation kan parallelliseras med portable_simd
- **Persistent cache** — spara resonansfält till disk mellan server-omstarter
