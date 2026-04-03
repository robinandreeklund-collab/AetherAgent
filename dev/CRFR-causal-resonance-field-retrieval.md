# Causal Resonance Field Retrieval (CRFR) v3

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

  Fas 2 — Convergent query-conditioned propagation (max 6, delta-styrt):
    Förälder → barn:
      damping = 0.35 × √(parent.amplitude) × role_weight(parent)
      heading/table: 1.3× | price/button: 0.6× | nav: 0.3×
    Barn → förälder:
      amplification = 0.25 × √(child.amplitude) × role_weight(child)
      price/data: 1.4× | heading/text: 1.1× | nav: 0.3×
    Fassynk: om |parent.phase - child.phase| < π/4 → ×1.08
    Konvergens: stoppa om total_delta < 0.001 (typiskt 2-3 steg)

  Fas 3 — Deterministic amplitud-gap top-k:
    Sortera efter amplitude DESC, tie-break node_id ASC (deterministic)
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
│ CRFR v3 (cold)               │ 4/6  67% │   1 761   │ baseline │
│ CRFR v3 (kausal feedback)    │ 5/6  83% │     —     │    —     │
│ Pipeline (BM25+HDC+Embed)    │ 4/6  67% │  35 971   │ 20.4x   │
│ ColBERT (MaxSim)             │ 5/6  83% │  89 550   │ 50.9x   │
└──────────────────────────────┴──────────┴───────────┴──────────┘
```

### 20 offline-tester (riktiga sajter + fixtures, ONNX)

```
┌──────────────────────────────┬──────┬──────┬───────┬───────┬──────────┬────────┐
│ Metod                        │  @1  │  @3  │  @10  │  @20  │  Avg µs  │ Output │
├──────────────────────────────┼──────┼──────┼───────┼───────┼──────────┼────────┤
│ CRFR v3 (BM25+HDC+cache)    │10/20 │15/20 │ 17/20 │ 17/20 │  32 165  │ 10.8   │
│ Pipeline (BM25+HDC+Embed)    │ 6/20 │10/20 │ 18/20 │ 19/20 │ 395 256  │ 19.9   │
└──────────────────────────────┴──────┴──────┴───────┴───────┴──────────┴────────┘

Speedup:          12.3x
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

### Nyckeltal

| Dimension | CRFR v3 | Pipeline (BM25+HDC+Embed) | ColBERT (MaxSim) |
|-----------|:-------:|:-------------------------:|:----------------:|
| **Recall@3** | **75%** | 50% | 83% (6 tester) |
| **Latens (cold)** | **32 ms** | 395 ms | 90 ms |
| **Latens (cache hit)** | **0.6 ms** | 395 ms | 90 ms |
| **Output-noder** | **6-11** | 16-20 | 5-8 |
| **Token-reduktion** | **99%** | 98.4% | 99.2% |
| **Deterministic** | **Ja** | Nej | Nej |
| **Value-aware** | **Ja** | Nej | Nej |
| **Lär sig** | **Ja** | Nej | Nej |
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
| **Benchmark @1** | 10/20 | **10/20** | Bibehållen |
| **Cache-hit** | — | **617 µs** | Sub-ms uppnådd |

---

## Varför är CRFR snabbare?

Pipeline-metoden kör tre steg sekventiellt:
1. BM25 keyword retrieval (~0.1 ms)
2. HDC 4096-bit pruning (~0.5 ms)
3. **ONNX embedding inference (~30-80 ms)** — flaskhalsen

CRFR v3 eliminerar steg 3 och cachar steg 1:

**Cold (första besöket):**
1. Bygg BM25-index (label+value) + HDC HV:er (~5 ms)
2. BM25 query + HDC similarity + roll-prioritet (~0.5 ms)
3. Convergent vågpropagation (~0.1 ms)
4. Deterministic gap-filter (~0.01 ms)

**Cache-hit (återbesök):**
1. BM25 query på cachad index (~0.3 ms)
2. HDC similarity (~0.2 ms)
3. Propagation + filter (~0.1 ms)
**Totalt: ~0.6 ms**

Ingen neural network inference. BM25-indexet cachas i `ResonanceField` och återanvänds.
CRFR kan köras **flera gånger per query** (multi-query orchestration) tack vare sub-ms latens.

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
- **Multi-query orchestration** — kör flera query-varianter, merge/union resultat (utnyttjar sub-ms)
- **SIMD-optimering** — propagation kan parallelliseras med portable_simd
- **Persistent cache** — spara resonansfält till disk mellan server-omstarter
