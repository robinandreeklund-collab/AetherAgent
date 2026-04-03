# Causal Resonance Field Retrieval (CRFR) v2

**Status:** Produktionsredo, live-verifierad | **Modul:** `src/resonance.rs`
**MCP:** `parse_crfr` + `crfr_feedback` | **HTTP:** `/api/parse-crfr` + `/api/crfr-feedback`

---

## Vad är CRFR?

CRFR är ett nytt retrieval-paradigm som behandlar DOM-trädet som ett **levande resonansfält** istället för ett statiskt index. När en fråga (goal) kommer in skapas en resonansvåg som propagerar genom trädets förälder-barn-relationer. Noder som matchar frågan "lyser upp" — och deras grannar får en svagare glöd via vågpropagation.

Det som gör CRFR unikt:
- **Ingen ONNX-modell krävs** — fungerar med BM25 + HDC bitvektorer + roll-aspekt
- **Lär sig i realtid** — varje lyckad extraktion gör systemet bättre, utan reträning
- **Naturlig top-k** — hittar naturliga kluster via amplitud-gap istället för hård gräns
- **Query-conditioned propagation** — noder med hög confidence sprider mer energi
- **Temporal decay** — kausalt minne dämpas exponentiellt (halvering var 10 min)

---

## Hur fungerar det?

### Steg 1: Bygg resonansfält (v2 multi-field)

Varje DOM-nod får ett **multi-aspekt resonanstillstånd**:

```
ResonanceState {
    text_hv:        4096-bit Hypervector (text n-gram encoding)
    role:           String (heading, price, button, etc.)
    depth:          u32 (djup i DOM-trädet)
    phase:          0.0 (oscillatorfas)
    amplitude:      0.0 (resonansstyrka)
    causal_memory:  noll-vektor (ackumulerat lärande)
    hit_count:      0 (antal lyckade matchningar)
    last_hit_ms:    0 (tidpunkt för senaste feedback — temporal decay)
}
```

Fältet byggs en gång per URL och cachas (LRU, 64 entries).

### Steg 2: Propagation (per fråga)

```
propagate(goal):

  Fas 1 — Multi-field initial resonans:
    Bygg BM25-index från alla nod-labels
    Beräkna tre separata signaler:
      Signal 1: BM25 keyword-matchning (65%)
      Signal 2: HDC text n-gram similarity (20%)
      Signal 3: Roll-aspekt prioritet (15%)
    amplitude = 0.65×BM25 + 0.20×HDC + 0.15×roll_boost + kausal_boost

    Kausal boost (temporal decay):
      raw = causal_memory.similarity(goal_hv)
      decay = exp(-λ × seconds_since_last_hit)   [halvering var 10 min]
      causal_boost = raw² × 0.3 × decay

  Fas 2 — Query-conditioned propagation (2 iterationer):
    Förälder → barn:
      damping = 0.35 × √(parent.amplitude) × role_weight(parent)
      heading/table: 1.3× (barn är ofta svaret)
      price/button:  0.6× (de ÄR svaret)
      nav/footer:    0.3× (dämpar brus)

    Barn → förälder:
      amplification = 0.25 × √(child.amplitude) × role_weight(child)
      price/data:    1.4× (föräldern behöver kontext)
      heading/text:  1.1× (bubblar relevant info)
      nav:           0.3× (dämpar brus)

    Fassynk: om |parent.phase - child.phase| < π/4 → ×1.08

  Fas 3 — Amplitud-gap top-k:
    Sortera efter amplitude (fallande)
    Klipp vid >30% relativ drop (naturlig klustergräns)
    Returnera max top_n noder
```

### Steg 3: Kausal feedback (lärande)

```
feedback(goal, successful_node_ids):
  goal_hv = Hypervector::from_text_ngrams(goal)
  for each node_id:
    node.causal_memory = bundle(node.causal_memory, goal_hv)
    node.hit_count += 1
    node.last_hit_ms = now()   ← temporal decay-referens
```

Nästa gång en liknande fråga ställs boostar det kausala minnet de noder som gav rätt svar. Ingen global modellträning — bara lokal VSA-binding.

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
│ CRFR v2 (cold)               │ 6/6 100% │   6 828   │ baseline │
│ CRFR v2 (med kausal feedback)│ 6/6 100% │     —     │    —     │
│ Pipeline (BM25+HDC+Embed)    │ 4/6  67% │  31 896   │  4.7x    │
│ ColBERT (MaxSim)             │ 5/6  83% │  89 550   │ 13.1x   │
└──────────────────────────────┴──────────┴───────────┴──────────┘
```

### 20 offline-tester (riktiga sajter + fixtures, ONNX)

```
┌──────────────────────────────┬──────┬──────┬───────┬───────┬──────────┬────────┐
│ Metod                        │  @1  │  @3  │  @10  │  @20  │  Avg µs  │ Output │
├──────────────────────────────┼──────┼──────┼───────┼───────┼──────────┼────────┤
│ CRFR v2 (BM25+HDC+cache)    │10/20 │17/20 │ 18/20 │ 18/20 │  30 568  │  9.8   │
│ Pipeline (BM25+HDC+Embed)    │ 6/20 │10/20 │ 18/20 │ 19/20 │ 390 541  │ 20.1   │
└──────────────────────────────┴──────┴──────┴───────┴───────┴──────────┴────────┘

Speedup:          12.8x
Token-reduktion:  99% (22 236 HTML-tokens → 408 CRFR-tokens)
```

### Nyckeltal

| Dimension | CRFR v2 | Pipeline (BM25+HDC+Embed) | ColBERT (MaxSim) |
|-----------|:-------:|:-------------------------:|:----------------:|
| **Recall@3** | **85%** | 50% | 83% (6 tester) |
| **Latens (cold)** | **6.8 ms** | 32 ms | 90 ms |
| **Latens (cache hit)** | **0.3 ms** | 32 ms | 90 ms |
| **Output-noder** | **6-10** | 16-20 | 5-8 |
| **Token-reduktion** | **99%** | 98.6% | 99.2% |
| **Lär sig** | **Ja** | Nej | Nej |
| **Kräver ONNX** | **Nej** | Ja | Ja |

---

## CRFR v2 — Vad ändrades från v1

| Optimering | v1 | v2 | Effekt |
|------------|----|----|--------|
| **Multi-field** | text_hv + hv (XOR) | text_hv + role (string) + depth | Renare signaler, -512 bytes/nod |
| **Scoring** | 0.7×BM25 + 0.3×HDC | 0.65×BM25 + 0.20×HDC + 0.15×roll | Roll-prioritet (price=1.0, nav=0.2) |
| **Propagation** | Fasta vikter (0.35/0.25) | Query-conditioned: √amp × role_factor | Högre confidence → mer spridning |
| **Learned weights** | Alla roller lika | heading sprider 1.3× nedåt, price bubblar 1.4× uppåt | Strukturmedveten propagation |
| **Temporal decay** | Ingen | exp(-λ×elapsed), halvering var 10 min | Stale bias försvinner gradvis |
| **Benchmark @3** | 16/20 | **17/20** | +1 |
| **Benchmark @1** | 9/20 | **10/20** | +1 |

---

## Varför är CRFR snabbare?

Pipeline-metoden kör tre steg sekventiellt:
1. BM25 keyword retrieval (~0.1 ms)
2. HDC 4096-bit pruning (~0.5 ms)
3. **ONNX embedding inference (~30-80 ms)** — flaskhalsen

CRFR eliminerar steg 3 helt:
1. BM25 keyword + HDC + roll-aspekt hybrid (~0.6 ms)
2. Query-conditioned vågpropagation (~0.1 ms)
3. Amplitud-gap top-k (~0.01 ms)

**Ingen neural network inference.** På cache-hit (~0.3 ms) hoppar vi över steg 1.

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
| `src/resonance.rs` | Kärn-implementation: ResonanceField, multi-field propagation, feedback, LRU cache |
| `src/lib.rs` | WASM API: `parse_crfr()`, `crfr_feedback()` |
| `src/bin/mcp_server.rs` | MCP stdio-server: `parse_crfr`, `crfr_feedback` verktyg |
| `src/bin/server.rs` | HTTP-server: endpoints + MCP dispatch (tools/list + tools/call) |
| `benches/crfr_vs_colbert.rs` | Kontrollerad benchmark (6 tester, CRFR vs Pipeline vs ColBERT) |
| `benches/crfr_final_benchmark.rs` | Offline benchmark (20 sajter, @1/@3/@10/@20) |
| `benches/crfr_live_test.py` | Live-verifiering (20 sajter via HTTP-server) |
| `src/scoring/hdc.rs` | Hypervector (4096-bit, XOR bind, majority bundle) |
| `src/scoring/tfidf.rs` | BM25 (integrerad i CRFR fas-1) |

---

## Kvarvarande optimeringar

- **I2**: stream_engine kontext-aware re-ranking — löst av CRFR:s propagation, men stream_engine.rs orörd
- **I7**: Jämförande extraktion ("X vs Y") — kräver multi-match per nyckel i extract_data
- **Value-matching**: rustup saknas i label men finns i href — matcha mot `node.value`/`node.action` i CRFR
- **SIMD-optimering** — propagation kan parallelliseras med portable_simd
- **Persistent cache** — spara resonansfält till disk mellan server-omstarter
