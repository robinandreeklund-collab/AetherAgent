# Adaptive Multi-Page Mode + Rich Link Extraction
## Komplett forskningsanalys och implementationsplan

**Datum:** 2026-04-08
**Status:** Research & Plan (ingen kod ännu)

---

## Innehåll

1. [Kodbasanalys — Vad vi redan har](#1-kodbasanalys)
2. [Algoritmval och vetenskaplig grund](#2-algoritmval)
3. [Arkitektur och datastrukturer](#3-arkitektur)
4. [Implementationsplan](#4-implementationsplan)
5. [Test- och valideringsstrategi](#5-teststrategi)

---

## 1. Kodbasanalys — Befintliga byggstenar

### 1.1 CRFR-pipelinen (resonance.rs, ~3846 rader)

**Vad den gör:** Behandlar DOM:en som ett resonansfält där noder oscillerar med
amplituder bestämda av goal-similarity, kausalt minne från tidigare lyckade
interaktioner, och vågpropagation genom trädstrukturen.

**Nyckelprimitiver vi kan återanvända:**

| Primitiv | Fil:rad | Vad den ger oss |
|----------|---------|-----------------|
| `ResonanceField::propagate()` | resonance.rs:854 | Goal-relevans-scoring av alla noder, returnerar amplitude 0.0–1.0 |
| `ResonanceField::feedback()` | resonance.rs:2083 | DCFR-baserad inlärning — lär sig vilka noder som var framgångsrika |
| `ResonanceField::transfer_from()` | resonance.rs:2572 | Överför kausalt minne mellan liknande sidor (cross-page learning) |
| `ResonanceField::propagate_multi_variant()` | resonance.rs:1684 | Kör N goal-varianter med delad BM25-cache (~0.6ms per variant) |
| `ResonanceField::propagate_broad()` | resonance.rs:1632 | Bred propagation för abstrakta queries (auto-detekterar via amplitude-distribution) |
| `goal_cluster_id()` | resonance.rs:571 | Grupperar liknande goals — "latest news" och "breaking news" → samma kluster |
| `compute_content_hash()` | resonance.rs:288 | FNV-1a hash av allt nod-innehåll — detekterar textändringar |
| Causal memory (hit_count, causal_memory HV) | resonance.rs:176–186 | Per-nod långtidsminne som överlever mellan queries |
| Suppression learning (query_count, miss_count) | resonance.rs:189–193 | Undertrycker noder som ofta dyker upp men aldrig är framgångsrika |
| Concept memory (field-level) | resonance.rs:244–247 | Aggregerade HV per goal-koncept — lär sig "vad pris-frågor matchar" globalt |

**Scoring-vikter (resonance.rs:43–54):**
- BM25: 0.75 (keyword precision)
- HDC text: 0.20 (n-gram strukturell likhet)
- Roll-prioritet: 0.05 (ren prioritetstabell)
- Kausal boost: 0.30 (inlärt minne)
- Chebyshev spectral filter: 5-koefficient polynom [0.50, 0.30, 0.12, 0.05, 0.03]

**Kritiskt för adaptive crawling:** ResonanceField har redan `content_hash` och
`structure_hash` — vi kan jämföra sidor utan att spara hela trädet.

### 1.2 Scoring Pipeline (scoring/pipeline.rs, ~701 rader)

**3-stegs pipeline:**
1. **BM25 kandidatretrieval** (~0.05ms) — tfidf.rs, top-300 kandidater
2. **HDC pruning** (~0.1ms) — hdc.rs, 2048-bit bitvektorer, XOR+popcount
3. **Stage 3 reranking** (~2-5ms) — ColBERT/MiniLM/Hybrid

**Nyckel för link scoring:** `ScoringPipeline::run_cached()` (pipeline.rs:237)
bygger index en gång, återanvänder vid cache-hit. Perfekt för att scora
alla länkar på en sida mot ett goal utan att bygga om index.

### 1.3 HDC-motorn (scoring/hdc.rs, ~674 rader)

- 2048-bit hypervektorer, FNV-1a-seedad
- `Hypervector::similarity()` — cosine via popcount, nanosekunder
- `Hypervector::bundle()` — majority-vote, SIMD-optimerad (4-wide unrolled)
- `HdcTree::project_goal()` — projicerar goal-text till HV

**Kritiskt:** Vi kan skapa en "kumulativ information-HV" som bundlar alla
hittills crawlade sidors text-HV. Ny sidas HV jämförs → information gain =
1.0 - similarity(kumulativ, ny_sida). Kostnaden: ~100 nanosekunder.

### 1.4 Fetch-infrastruktur (fetch.rs, ~1049 rader)

- `fetch_page(url, config)` — async, reqwest, gzip/brotli
- Per-domän rate limiter via governor (GCRA, default 2 req/s)
- robots.txt-compliance (Googles robotstxt crate)
- SSRF-skydd (blockerar privata IP-ranges)
- Cookie-hantering via SessionManager (inte globalt)
- Delad `SHARED_CLIENT` (TLS-session återanvänds)
- `FetchConfig` har: timeout, max_redirects, rate_limit_rps, extra_headers, cookies

**Redan klart:** Vi behöver INTE bygga ny HTTP-infrastruktur. `fetch_page` + rate
limiter + robots.txt + SSRF-skydd finns redan.

### 1.5 Orchestrator (orchestrator.rs, ~1206 rader)

- `WorkflowOrchestrator` — stateful, serialiserbar, multi-page
- Stöd för rollback/retry, max_pages-skydd, cross-page temporal memory
- Integration med SessionManager och TemporalMemory
- `PageVisit`-historik med URL + node_count + extracted_data

**Begränsning:** Orchestrator kör en förkompilerad ActionPlan steg-för-steg.
Den gör INTE dynamiska beslut om "vilken länk ska jag följa härnäst?" eller
"har jag samlat tillräckligt med information?". Det är precis vad den nya
adaptiva crawlern ska lösa.

### 1.6 Persistence (persist.rs, ~709 rader)

- SQLite WAL-mode med connection pool (1 writer + 4 readers)
- Sparar ResonanceField per URL-hash
- Sparar DomainProfile per domän
- `save_field()` / `load_field()` / `list_fields()`
- Kan utökas med tabeller för crawl-state och link-metadata

### 1.7 Firewall (firewall.rs)

- L3 semantic relevance scoring: `classify_request(url, goal, config)`
- Redan beräknar URL-goal-relevans (0.0–1.0)
- Kan återanvändas för att pre-filtrera länkar innan vi crawlar dem

### 1.8 Causal Graph (causal.rs)

- `find_safest_path(goal, max_depth)` — 3-nivå semantisk goal-matching
- Risk-medveten BFS genom sidstates
- `predict_outcome(action, from_state)` — förutsäger konsekvenser

### 1.9 Stream Engine (stream_engine.rs, ~1052 rader)

- Chunked nod-emission med relevansfiltrering
- `StreamParseConfig` — chunk_size, min_relevance, max_nodes
- Directive-system: expand/stop/next_branch/lower_threshold

### 1.10 Collab Store (collab.rs)

- `SharedDiffStore` — delad cache mellan agenter
- `publish_delta()` / `fetch_deltas()` — undviker omparning
- Max 5000 cached deltas, LRU-eviction

### 1.11 Vad saknas (och ska byggas)

| Saknas | Behövs för | Prioritet |
|--------|-----------|-----------|
| **Link-extraktion med metadata** | Rich link scoring | Hög |
| **Information gain-beräkning** | Adaptive stopping | Hög |
| **MAB/Thompson Sampling-loop** | Länkval | Hög |
| **Bayesian sufficiency-modell** | "Har vi tillräckligt?" | Hög |
| **Kumulativ HDC-bundle** | Cross-page novelty | Medel |
| **Head-only fetch** | Lightweight metadata | Medel |
| **SQLite crawl-state-tabell** | Resume/persistens | Medel |
| **MCP tool + HTTP endpoints** | Integration | Låg (sist) |

---

## 2. Algoritmval och vetenskaplig grund

### 2.1 Crawl4AI:s approach (vad vi slår)

Crawl4AI använder **Information Foraging Theory** med tre statiska metrics:

```
confidence = w1 * coverage + w2 * consistency + w3 * saturation

coverage    = |unique_topics_found| / |expected_topics|
consistency = mean(pairwise_similarity(pages))
saturation  = 1.0 - (new_info_last_page / total_info)
```

**Svagheter:**
- Statiska vikter (w1, w2, w3) — anpassar sig inte till domänen
- Coverage kräver fördefinierade "expected topics" — svårt att veta i förväg
- Saturation mäter bara senaste sidan — missar multi-hop informationskedjor
- Ingen exploration/exploitation-balans — följer alla länkar lika
- Inget kausalt minne — lär sig inte mellan crawl-sessioner
- Kräver embeddings för similarity — vi kör zero-model

### 2.2 Vår approach: Restless MAB + Bayesian Stopping + CRFR

Vi kombinerar tre algoritmer som alla mappar direkt till befintliga CRFR-primitiver:

#### Algoritm A: Thompson Sampling Multi-Armed Bandit (länkval)

**Varför:** Varje outforskad länk är en "arm". Vi vill maximera total
information gain per fetch (dyrt) genom att balansera exploration (prova
okända länkar) med exploitation (följa länkar som liknar redan framgångsrika).

**Mapping till CRFR:**

| MAB-koncept | CRFR-primitiv |
|-------------|---------------|
| Arm reward | `ResonanceResult.amplitude` medelvärde för sidan |
| Prior distribution | `propagation_stats` Beta(α, β) — redan finns! |
| Posterior update | `feedback()` — redan uppdaterar Beta-parametrar |
| Arm feature vector | `Hypervector::from_text_ngrams(anchor_text + context)` |
| Contextual info | `goal_cluster_id()` + `role_priority()` + structural zone |

**Algoritm (Thompson Sampling med HDC-kontext):**

```
FÖR varje outforskad länk L:
  1. text_hv = Hypervector::from_text_ngrams(L.anchor_text + L.context)
  2. goal_sim = text_hv.similarity(goal_hv)  // ~100ns
  3. structural_bonus = zone_score(L.structural_role)
  4. prior_α = goal_sim * 10 + 1  // Informerad prior
  5. prior_β = (1.0 - goal_sim) * 10 + 1
  6. sample = Beta(prior_α, prior_β).sample()  // Thompson sampling
  7. expected_reward = sample * (1.0 - cumulative_similarity(L))
  
VÄLJ: L med högst expected_reward
```

**Varför Thompson Sampling istället för UCB1:**
- TS ger bättre empirisk prestanda på kort horisont (vi crawlar <20 sidor)
- TS hanterar non-stationary rewards naturligt (sidinnehåll ändras)
- Vi har redan Beta-distributioner i `propagation_stats`

#### Algoritm B: Bayesian Optimal Stopping (när vi slutar)

**Varför:** Istället för Crawl4AI:s fasta `confidence_threshold` använder vi
en Bayesian posterior som uppdateras efter varje crawlad sida.

**Modell: Beta-Bernoulli sufficiency**

```
Observation: Varje crawlad sida ger antingen "ny information" (1) eller "redundant" (0)

"Ny information" definieras som:
  info_gain = 1.0 - cumulative_hv.similarity(page_hv)
  is_novel = info_gain > min_gain_threshold  // default 0.08

Prior: Beta(α₀=1, β₀=1)  // Uniform — vi vet inget i början

Efter varje sida:
  if is_novel: α += 1
  else: β += 1

Posterior mean = α / (α + β)  = P(nästa sida ger ny info)

STOPP NÄR:
  P(nästa sida ger ny info) < stop_threshold  // default 0.20
  OCH vi har crawlat minst min_pages  // default 3
  ELLER max_pages nådd
  ELLER gain_streak = 0 senaste K sidor  // K=3
```

**Fördelar vs Crawl4AI:**
- Adaptiv: confidence_threshold justeras automatiskt baserat på data
- Osäkerhetsmedveten: med 2 sidor crawlade → stor osäkerhet → fortsätt
- Inga fördefinierade "expected topics" behövs
- Kumulativ HDC-bundle: ~100ns per sida att uppdatera

#### Algoritm C: HDC Information Gain (novitetsmätning)

**Nyckeln** som binder ihop A och B. Vi mäter informationsvinst per sida
via HDC-similarity mot en kumulativ bundle:

```
cumulative_hv = Hypervector::zero()

FÖR varje crawlad sida P:
  page_hv = Hypervector::from_text_ngrams(P.top_nodes_text)
  gain = 1.0 - cumulative_hv.similarity(page_hv)
  cumulative_hv = Hypervector::bundle(&[&cumulative_hv, &page_hv])
  
  // gain → reward signal för MAB (Algoritm A)
  // gain → observation för Bayesian stopping (Algoritm B)
```

**Varför HDC och inte embeddings:**
- Hypervector similarity: ~100ns (XOR + popcount)
- Embedding similarity: ~2-5ms (matrix multiply)
- Bundle (information accumulation): ~200ns vs ~10ms
- Ingen modell att ladda, ingen GPU, WASM-kompatibelt

**Chebyshev-amplituds information gain (avancerad variant):**

Utöver ren HDC-similarity kan vi använda CRFR-amplituden som richer signal:

```
page_amplitude_sum = Σ resonance_results[i].amplitude  // alla noder
page_unique_amplitude = Σ (amplitude[i] where gain[i] > threshold)

spectral_gain = page_unique_amplitude / page_amplitude_sum
// Hög spectral_gain = sidan har unika noder med hög resonans
// Låg spectral_gain = sidan repeterar redan kända mönster
```

### 2.3 Jämförelse: Vår approach vs alternativ

| Egenskap | Crawl4AI | Vår MAB+Bayesian | Ren RL/POMDP |
|----------|----------|------------------|--------------|
| Länkval | Alla lika / enkel heuristik | Thompson Sampling + HDC | Q-learning |
| Stopping | Fast threshold | Bayesian posterior | Reward threshold |
| Lärande | Ingen | Kausalt minne (CRFR) | Neural policy |
| Cross-session | Ingen | `transfer_from()` + SQLite | Kräver träning |
| Latens per beslut | ~1ms | ~0.5ms (HDC-dominant) | ~10-50ms |
| Modellstorlek | ~100MB (embeddings) | 0 bytes | ~50MB+ |
| WASM-kompatibel | Nej (Python) | Ja | Svårt |
| Regret bound | Ingen garanti | O(√(KT log K)) | Asymptotiskt optimal |
| Implementation | ~500 rader Python | ~800 rader Rust | ~2000+ rader |

### 2.4 Varför vi INTE gör Deep RL (ännu)

- Kräver träningsdata som vi inte har
- Inference-latens (10-50ms) per beslut
- Modell-storlek spränger WASM binary
- Thompson Sampling ger provably near-optimal resultat utan träning
- Vi kan alltid lägga till RL-policy som Stage 2 i framtiden

### 2.5 Event Model via Goal-Clustered HDC

Istället för Crawl4AI:s term-matching använder vi `goal_cluster_id()` + HDC
för att bygga en rik "event model" per goal-typ:

```
Mål: "Find AI agent tools and developments"
  → goal_cluster: "agent+developments+tools"
  → Matchar: "autonomous agents", "LLM tools", "AI development framework"
  → INTE: "real estate agent", "travel tools"

Implementeras genom concept_memory i ResonanceField:
  concept_memory["agent:agent+developments+tools"] = aggregerad HV
  → Nästa sida som innehåller liknande noder → högt expected_gain
```

Detta ger oss **implicit query expansion** utan embeddings.

---

## 3. Arkitektur och datastrukturer

### 3.1 Nya filer

| Fil | Ansvar | Beroenden |
|-----|--------|-----------|
| `src/adaptive.rs` | AdaptiveCrawler, MAB, Bayesian stopping, info gain | resonance, scoring/hdc, fetch, firewall |
| `src/link_extract.rs` | Rich link extraction + metadata + scoring | parser, semantic, resonance, scoring/hdc |
| `src/tools/adaptive_tool.rs` | MCP tool wrapper | adaptive, tools/mod |
| `src/tools/link_tool.rs` | MCP tool wrapper | link_extract, tools/mod |

**Ingen ändring av befintliga filer** utom:
- `src/lib.rs` — lägga till `mod adaptive; mod link_extract;` + WASM-exports
- `src/tools/mod.rs` — registrera nya tools
- `src/bin/mcp_server.rs` — lägga till 2-3 MCP tools
- `src/bin/server.rs` — lägga till 3-4 HTTP endpoints
- `src/persist.rs` — lägga till crawl_state-tabell

### 3.2 Datastrukturer — Rich Link Extraction

```rust
// src/link_extract.rs

/// En extraherad länk med rik metadata
pub struct EnrichedLink {
    /// Rå href (som den stod i HTML:en)
    pub href: String,
    /// Absolut URL (resolved mot base)
    pub absolute_url: String,
    /// Anchor-text (synlig text i <a>-taggen)
    pub anchor_text: String,
    /// Title-attribut från <a> eller närliggande heading
    pub title: Option<String>,
    /// Meta description (om head_fetch gjordes)
    pub meta_description: Option<String>,
    /// CRFR goal-relevans (0.0–1.0, via HDC + BM25)
    pub relevance_score: f32,
    /// Förväntad informationsvinst (för adaptive crawling)
    pub expected_gain: f32,
    /// Omgivande text-kontext (±50 tecken runt länken)
    pub context_snippet: Option<String>,
    /// Strukturell roll: "navigation", "content", "footer", "sidebar", "card"
    pub structural_role: String,
    /// Intern vs extern länk
    pub is_internal: bool,
    /// Djup i DOM-trädet
    pub depth: u32,
    /// Position i DOM (ordning)
    pub position_in_dom: u32,
    /// HDC-hypervector (för snabb similarity i MAB)
    pub text_hv: Hypervector,  // Intern, serialiseras inte i JSON
}

/// Konfiguration för link-extraktion
pub struct LinkExtractionConfig {
    /// Hämta <head> för title + meta (async, HEAD-first)
    pub include_head_data: bool,
    /// Inkludera omgivande text
    pub include_context: bool,
    /// Inkludera strukturell roll
    pub include_structural_role: bool,
    /// Max antal länkar att returnera
    pub max_links: usize,
    /// Minimum relevance score (0.0 = alla)
    pub min_relevance: f32,
    /// Goal för relevance scoring (None = ingen scoring)
    pub goal: Option<String>,
}
```

**Hur link-extraktion fungerar (pipeline):**

```
HTML input
  ↓
1. parser::parse_document() → RcDom
  ↓
2. Walk DOM, samla alla <a href="...">:
   - anchor_text = extract_text(a_element)
   - href = get_attr("href")
   - absolute_url = resolve(href, base_url)
   - structural_role = infer_zone(parent_chain)  // nav/content/footer/card
   - context_snippet = extract_surrounding_text(±50 chars)
   - depth = count_ancestors()
   - position_in_dom = incrementing counter
  ↓
3. Scoring (om goal anges):
   - text_hv = Hypervector::from_text_ngrams(anchor_text + context)
   - goal_hv = Hypervector::from_text_ngrams(goal)
   - hdc_sim = text_hv.similarity(goal_hv)
   - bm25_score = TfIdfIndex(all_anchor_texts).query(goal, 1)[link_idx]
   - relevance_score = BM25_WEIGHT * bm25 + HDC_WEIGHT * hdc_sim
   - expected_gain beräknas av AdaptiveCrawler (om kopplad)
  ↓
4. Filtrering + sortering:
   - Filtrera: min_relevance, is_internal (om önskat)
   - Sortera: relevance_score descending
   - Truncate: max_links
  ↓
5. Optional: Head-fetch (async, parallellt, max 5 samtida):
   - HTTP HEAD → Content-Type check (bara text/html)
   - HTTP GET range:0-4096 → parse <title> + <meta description>
   - Cache: redan fetchade URLs skippas
  ↓
6. Returnera Vec<EnrichedLink>
```

### 3.3 Datastrukturer — Adaptive Multi-Page Crawler

```rust
// src/adaptive.rs

/// Strategi för link-scoring
pub enum AdaptiveStrategy {
    /// Ren HDC-similarity (snabbast, ~100ns per länk)
    Hdc,
    /// Hybrid: BM25 + HDC (bättre kvalitet, ~0.5ms per länk)
    HybridBm25Hdc,
}

/// Konfiguration för adaptiv crawling
pub struct AdaptiveConfig {
    /// Bayesian stopping: P(ny info) < detta → stopp
    pub confidence_threshold: f32,      // default 0.20
    /// Max antal sidor att crawla
    pub max_pages: usize,               // default 20
    /// Antal bästa länkar att utforska per sida
    pub top_k_links: usize,             // default 5
    /// Max djup från start-URL
    pub max_depth: u32,                 // default 3
    /// Minimum information gain för att räkna som "ny info"
    pub min_gain_threshold: f32,        // default 0.08
    /// Scoring-strategi
    pub strategy: AdaptiveStrategy,     // default HybridBm25Hdc
    /// Respektera robots.txt
    pub respect_robots_txt: bool,       // default true
    /// Timeout per sida i ms
    pub timeout_ms: u64,                // default 10000
    /// Antal sidor utan gain innan tidig stopp
    pub max_no_gain_streak: usize,      // default 3
}

/// Tillståndet för en pågående adaptiv crawl
pub struct CrawlState {
    /// Alla besökta sidor
    pub visited: Vec<CrawlPage>,
    /// Frontier: outforskade länkar med scores
    pub frontier: Vec<FrontierEntry>,
    /// Kumulativ HDC-bundle (all information hittills)
    pub cumulative_hv: Hypervector,
    /// Bayesian stopping state
    pub bayesian_alpha: f32,            // successes + 1
    pub bayesian_beta: f32,             // failures + 1
    /// Antal sidor i rad utan signifikant gain
    pub no_gain_streak: usize,
    /// Start-URL
    pub start_url: String,
    /// Goal
    pub goal: String,
    /// Config (sparas för resume)
    pub config: AdaptiveConfig,
    /// Totalt antal sidor crawlade
    pub pages_crawled: usize,
    /// Nuvarande djup
    pub current_depth: u32,
    /// Anledning till stopp (fylls vid completion)
    pub stop_reason: Option<StopReason>,
}

/// En crawlad sida i resultatet
pub struct CrawlPage {
    pub url: String,
    pub title: String,
    pub depth: u32,
    pub information_gain: f32,          // HDC-baserad gain
    pub amplitude_sum: f32,             // CRFR-total
    pub top_nodes: Vec<DistilledNode>,  // Top-N noder efter CRFR
    pub links_found: usize,
    pub fetch_time_ms: u64,
    pub parse_time_ms: u64,
}

/// Komprimerad nod i crawl-output (token-effektiv)
pub struct DistilledNode {
    pub role: String,
    pub label: String,
    pub amplitude: f32,
    pub resonance_type: ResonanceType,
}

/// En entry i crawl-frontier (outforskade länkar)
pub struct FrontierEntry {
    pub link: EnrichedLink,
    /// Thompson Sampling: Beta-prior α
    pub ts_alpha: f32,
    /// Thompson Sampling: Beta-prior β
    pub ts_beta: f32,
    /// Senaste samplat score
    pub sampled_score: f32,
    /// Djup om vi följer denna länk
    pub depth: u32,
    /// Käll-URL (vilken sida länken hittades på)
    pub source_url: String,
}

/// Anledning till att crawlen stoppade
pub enum StopReason {
    /// Bayesian posterior < threshold
    InformationSaturated { posterior_mean: f32 },
    /// max_pages nådd
    MaxPagesReached,
    /// max_depth nådd (alla frontier-entries > max_depth)
    MaxDepthReached,
    /// Ingen gain senaste K sidor
    NoGainStreak { streak: usize },
    /// Frontier tom (inga fler länkar att utforska)
    FrontierEmpty,
    /// Explicit stopp från användaren
    UserStopped,
}

/// Resultat från en komplett adaptiv crawl
pub struct AdaptiveDigestResult {
    /// Alla crawlade sidor (i ordning)
    pub pages: Vec<CrawlPage>,
    /// Samlad extraherad data (deduplikerad)
    pub combined_nodes: Vec<DistilledNode>,
    /// Anledning till stopp
    pub stop_reason: StopReason,
    /// Total tid i ms
    pub total_time_ms: u64,
    /// Bayesian posterior vid stopp
    pub final_confidence: f32,
    /// Total information gain (summa)
    pub total_information_gain: f32,
    /// Antal links i frontier som inte utforskades
    pub frontier_remaining: usize,
    /// Statistik
    pub stats: CrawlStats,
}

pub struct CrawlStats {
    pub pages_crawled: usize,
    pub total_nodes_seen: usize,
    pub total_links_found: usize,
    pub avg_gain_per_page: f32,
    pub avg_fetch_time_ms: f32,
    pub avg_parse_time_ms: f32,
    pub bayesian_trajectory: Vec<f32>,   // posterior mean per steg
}
```

### 3.4 Algoritmflöde — adaptive_digest()

```
adaptive_digest(start_url, goal, config):
  ╔═══════════════════════════════════════╗
  ║  1. INIT                              ║
  ║  - cumulative_hv = zero()             ║
  ║  - bayesian = Beta(1, 1)             ║
  ║  - frontier = [start_url]             ║
  ║  - goal_hv = from_text_ngrams(goal)  ║
  ╚═══════════════════╤═══════════════════╝
                      │
  ╔═══════════════════▼═══════════════════╗
  ║  2. LOOP: while !should_stop()        ║
  ║                                       ║
  ║  a) SELECT: Thompson Sample frontier  ║
  ║     → Välj länk med högst            ║
  ║       Beta(α,β).sample() * novelty   ║
  ║                                       ║
  ║  b) FETCH: fetch_page(selected_url)   ║
  ║     → robots.txt, rate limit, SSRF   ║
  ║                                       ║
  ║  c) PARSE: parse → semantic tree      ║
  ║     → ResonanceField::from_tree()    ║
  ║     → propagate(goal)                ║
  ║     → top_k noder med amplitude      ║
  ║                                       ║
  ║  d) MEASURE: information gain         ║
  ║     page_hv = bundle(top_nodes.text) ║
  ║     gain = 1.0 - sim(cumul, page_hv) ║
  ║     cumulative_hv = bundle(cumul, pg)║
  ║                                       ║
  ║  e) UPDATE: Bayesian stopping         ║
  ║     if gain > min_threshold:          ║
  ║       α += 1 (novel)                 ║
  ║     else:                             ║
  ║       β += 1 (redundant)             ║
  ║                                       ║
  ║  f) EXPAND: extract_links(page)       ║
  ║     → Score med HDC + BM25           ║
  ║     → Filtrera via Firewall L3       ║
  ║     → Lägg top_k_links i frontier    ║
  ║                                       ║
  ║  g) LEARN: crfr feedback              ║
  ║     Om gain hög → feedback(goal,     ║
  ║       high_amplitude_nodes)           ║
  ║     → Nästa sida med liknande        ║
  ║       struktur → bättre scoring      ║
  ║                                       ║
  ║  h) TRANSFER: crfr_transfer           ║
  ║     Om samma domän → transfer        ║
  ║     kausalt minne till nästa sida     ║
  ╚═══════════════════╤═══════════════════╝
                      │
  ╔═══════════════════▼═══════════════════╗
  ║  3. STOP DECISION: should_stop()      ║
  ║                                       ║
  ║  posterior = α / (α + β)             ║
  ║  STOP om:                             ║
  ║    posterior < confidence_threshold   ║
  ║    && pages_crawled >= 3              ║
  ║  ELLER max_pages nådd                 ║
  ║  ELLER no_gain_streak >= 3            ║
  ║  ELLER frontier tom                   ║
  ╚═══════════════════╤═══════════════════╝
                      │
  ╔═══════════════════▼═══════════════════╗
  ║  4. COMPILE RESULT                    ║
  ║  - Samla alla CrawlPages              ║
  ║  - Dedup + merge top_nodes            ║
  ║  - Sortera efter amplitude            ║
  ║  - Returnera AdaptiveDigestResult     ║
  ╚═══════════════════════════════════════╝
```

### 3.5 Structural zone detection (för links)

Befintlig `infer_role()` i parser.rs ger oss roll per element. Men för links
behöver vi veta vilken **zon** de befinner sig i (navigation vs content vs footer).

```
fn infer_zone(parent_chain: &[&str]) -> &'static str {
    // parent_chain = ["html", "body", "div", "nav", "ul", "li", "a"]
    for tag in parent_chain.iter().rev() {
        match *tag {
            "nav" => return "navigation",
            "header" => return "header",
            "footer" => return "footer",
            "aside" => return "sidebar",
            "main" | "article" => return "content",
        }
    }
    // Fallback: check ARIA roles + class names
    "unknown"
}
```

**Zone scoring for MAB:**

| Zone | Score multiplier | Rationale |
|------|-----------------|-----------|
| content | 1.0 | Primary content links |
| navigation | 0.6 | Often site structure, not info |
| sidebar | 0.7 | Related but often tangential |
| header | 0.4 | Logo, login, etc. |
| footer | 0.3 | Legal, privacy, sitemap |
| unknown | 0.8 | Assume content until proven |

### 3.6 SQLite schema-tillägg (persist.rs)

```sql
-- Ny tabell: crawl state (för resume)
CREATE TABLE IF NOT EXISTS crawl_states (
    id TEXT PRIMARY KEY,          -- UUID
    start_url TEXT NOT NULL,
    goal TEXT NOT NULL,
    config_json TEXT NOT NULL,
    state_json TEXT NOT NULL,     -- Serialiserat CrawlState
    created_at_ms INTEGER,
    updated_at_ms INTEGER,
    status TEXT DEFAULT 'running' -- running, completed, paused
);

-- Ny tabell: link metadata cache
CREATE TABLE IF NOT EXISTS link_meta_cache (
    url_hash INTEGER PRIMARY KEY,
    url TEXT NOT NULL,
    title TEXT,
    meta_description TEXT,
    content_type TEXT,
    fetched_at_ms INTEGER,
    ttl_ms INTEGER DEFAULT 3600000  -- 1 timme
);
```

### 3.7 Integration med befintliga system

```
                    ┌─────────────┐
                    │  MCP Tool   │  adaptive_digest
                    │  HTTP API   │  extract_links_with_meta
                    └──────┬──────┘
                           │
                    ┌──────▼──────┐
                    │  adaptive.rs │  ← NY
                    │  Crawler     │
                    └──┬───┬───┬──┘
                       │   │   │
          ┌────────────┘   │   └────────────┐
          ▼                ▼                 ▼
  ┌──────────────┐  ┌───────────┐   ┌──────────────┐
  │ link_extract │  │ resonance │   │   fetch.rs   │
  │  .rs (NY)    │  │   .rs     │   │  (befintlig) │
  └──────┬───────┘  └─────┬─────┘   └──────────────┘
         │                │
         ▼                ▼
  ┌──────────────┐  ┌───────────┐
  │  parser.rs   │  │ scoring/  │
  │  semantic.rs │  │ hdc.rs    │
  │  (befintlig) │  │ tfidf.rs  │
  └──────────────┘  └───────────┘
```

**Inga cirkulära beroenden.** adaptive.rs beror på link_extract.rs och
resonance.rs, men ingen av dem beror på adaptive.rs.

---

## 4. Implementationsplan

### 4.1 Steg-för-steg (6 implementationssteg)

#### Steg 1: `link_extract.rs` — Rich Link Extraction (~250 rader)

**Input:** HTML + base_url + optional goal
**Output:** `Vec<EnrichedLink>`

**Vad som byggs:**
1. `extract_links(html, base_url, config) -> Vec<EnrichedLink>`
   - Walk DOM, samla alla `<a href>` element
   - Resolve relativa URLs mot base_url
   - Extrahera anchor_text, title-attribut, context_snippet
   - Beräkna structural_role via `infer_zone()`
   - Dedup: samma absolute_url → behåll den med bäst position
   
2. `score_links(links, goal) -> Vec<EnrichedLink>`
   - Bygg BM25-index av alla anchor_texts
   - Beräkna HDC-similarity per länk mot goal
   - Combinerad score: `0.75 * bm25_norm + 0.20 * hdc_sim + 0.05 * zone_score`
   
3. `infer_zone(parent_chain) -> &str`
   - Walk uppåt i DOM: nav→"navigation", footer→"footer", main/article→"content"

4. `is_internal_link(url, base_url) -> bool`
   - Jämför domäner

**Beroenden:** parser.rs (DOM-traversal), scoring/hdc.rs (Hypervector),
scoring/tfidf.rs (BM25). Alla befintliga.

**Test:** Unit tests med handskrivna HTML-snippets:
- Navigations-links (roll = "navigation")
- Content-links i artikeltext (roll = "content")
- Footer-links (roll = "footer")
- Relativa vs absoluta URLs
- Scoring: hög relevans-link ska scora högre

---

#### Steg 2: `adaptive.rs` del 1 — CrawlState + Information Gain (~200 rader)

**Vad som byggs:**
1. `CrawlState::new(start_url, goal, config) -> CrawlState`
   - Initialisera cumulative_hv, bayesian_alpha/beta, frontier
   
2. `CrawlState::measure_gain(page_hv) -> f32`
   - `gain = 1.0 - cumulative_hv.similarity(page_hv)`
   - Uppdatera cumulative_hv med bundle
   
3. `CrawlState::update_bayesian(gain) -> f32`
   - if gain > min_threshold: α += 1 else β += 1
   - return α / (α + β)
   
4. `CrawlState::should_stop() -> Option<StopReason>`
   - Alla 5 stopp-villkor

5. Serde Serialize/Deserialize för hela CrawlState
   - Möjliggör resume/persistens

**Beroenden:** scoring/hdc.rs (Hypervector, similarity, bundle). Inget annat.

**Test:** Unit tests:
- Information gain sjunker med identiska sidor
- Bayesian posterior sjunker vid repeated redundans
- should_stop() triggers korrekt vid varje StopReason
- Serialisering round-trip

---

#### Steg 3: `adaptive.rs` del 2 — Thompson Sampling + Frontier (~200 rader)

**Vad som byggs:**
1. `CrawlState::add_to_frontier(links, source_url, depth)`
   - Skapa FrontierEntry per länk
   - Dedup: om URL redan i frontier, uppdatera score om bättre
   - Firewall-filter: `classify_request(url, goal, config)` — skippa L1/L2-blockade
   
2. `CrawlState::select_next() -> Option<FrontierEntry>`
   - Thompson Sampling:
     ```
     for entry in frontier:
       sample = beta_sample(entry.ts_alpha, entry.ts_beta)
       novelty = 1.0 - cumulative_hv.similarity(entry.link.text_hv)
       score = sample * novelty * zone_multiplier
     return argmax(score)
     ```
   - Beta sampling: `beta_sample(α, β)` via inverse CDF approximation
     (ingen rand crate behövs — FNV hash av tidstämpel som seed)

3. `CrawlState::update_frontier_feedback(url, gain)`
   - Uppdatera ts_alpha/ts_beta för den valda armen
   - Om gain > threshold: α += gain * 5 (proportional reward)
   - Om gain < threshold: β += (1.0 - gain) * 3

4. `beta_sample(alpha, beta) -> f32`
   - Lightweight approximation: median av Beta = (α - 1/3) / (α + β - 2/3)
   - Plus jitter: ±0.1 baserat på hash
   - (Full inversion kan läggas till senare med `rand` crate)

**Beroenden:** link_extract.rs, firewall.rs.

**Test:**
- Thompson Sampling föredrar högt scorade länkar
- Frontier dedup fungerar
- Firewall-blockerade URLs hamnar inte i frontier
- Beta sampling ger rimliga värden

---

#### Steg 4: `adaptive.rs` del 3 — adaptive_digest() huvudloop (~200 rader)

**Vad som byggs:**
1. `async fn adaptive_digest(start_url, goal, config) -> Result<AdaptiveDigestResult, String>`
   - Huvudloopen (se flödet i §3.4)
   - Anropar fetch_page(), parse(), propagate(), extract_links(), etc.
   - Samlar CrawlPages
   - Kör CRFR transfer_from() mellan sidor på samma domän
   - Kör CRFR feedback() för högamplitud-noder

2. `fn compile_result(state) -> AdaptiveDigestResult`
   - Aggregera alla CrawlPages
   - Dedup combined_nodes (samma label → behåll högst amplitude)
   - Beräkna statistik

**Beroenden:** Allt från steg 1–3 + fetch.rs + resonance.rs.

**Test:** Integration test med mock HTML (ingen nätverks-fetch):
- 3-sidors crawl → korrekt gain-beräkning
- Stopp vid saturation
- CRFR transfer fungerar cross-page

---

#### Steg 5: Head-fetch + SQLite persistence (~150 rader)

**Vad som byggs:**
1. `async fn fetch_head_metadata(urls, max_concurrent) -> Vec<HeadMetadata>`
   - HTTP HEAD → Content-Type check
   - HTTP GET range:0-4096 → parse <title> + <meta name="description">
   - Max 5 samtida requests
   - Cache i link_meta_cache-tabell

2. SQLite-tabeller i persist.rs:
   - `crawl_states` — för resume
   - `link_meta_cache` — för head-metadata
   - `save_crawl_state()` / `load_crawl_state()` / `list_crawl_states()`

**Beroenden:** fetch.rs, persist.rs.

**Test:**
- Head-fetch returnerar title + meta
- Cache-hit undviker ny fetch
- SQLite round-trip av crawl state

---

#### Steg 6: MCP tools + HTTP endpoints + WASM exports (~200 rader)

**MCP tools (mcp_server.rs):**
- `adaptive_digest` — full crawl
- `extract_links_with_meta` — rich link extraction
- `resume_crawl` — resume pausad crawl från SQLite

**HTTP endpoints (server.rs):**
- `POST /api/adaptive/digest` — full crawl
- `POST /api/adaptive/links` — extract links
- `POST /api/adaptive/resume` — resume
- `GET /api/adaptive/states` — lista pågående/pausade crawls

**WASM exports (lib.rs):**
- `adaptive_digest()` — returnar JSON-serialiserat AdaptiveDigestResult
- `extract_links_with_meta()` — returnerar JSON Vec<EnrichedLink>

**Test:** Integration test end-to-end via MCP tool interface.

---

### 4.2 Estimerad storlek

| Steg | Fil | Nya rader | Ändrade rader |
|------|-----|-----------|---------------|
| 1 | link_extract.rs | ~250 | 0 |
| 2 | adaptive.rs (del 1) | ~200 | 0 |
| 3 | adaptive.rs (del 2) | ~200 | 0 |
| 4 | adaptive.rs (del 3) | ~200 | 0 |
| 5 | persist.rs + head fetch | ~150 | ~30 |
| 6 | tools + endpoints + WASM | ~200 | ~50 |
| **Total** | | **~1200** | **~80** |

**Totalt ~1200 nya rader + ~80 ändrade.** Rimligt för en feature av denna storlek.

### 4.3 Beroende-ordning

```
Steg 1 ────→ Steg 2 ────→ Steg 3 ────→ Steg 4 ────→ Steg 5 ────→ Steg 6
link_extract  CrawlState   Thompson    Main loop    Persistence  Integration
              InfoGain     Sampling
              
Steg 1 och 2 kan köras parallellt (inga beroenden mellan dem).
Steg 3 beror på både 1 och 2.
Steg 5 och 6 kan köras parallellt efter steg 4.
```

### 4.4 Risker och mitigering

| Risk | Sannolikhet | Impact | Mitigering |
|------|------------|--------|------------|
| Beta-sampling ger dålig exploration | Medel | Medel | Fallback: ε-greedy (10% random) |
| Information gain konvergerar för snabbt | Hög | Hög | Tuneable min_gain_threshold + min_pages |
| fetch_page() timeout → crawler hänger | Låg | Hög | Per-page timeout + retry (redan i fetch.rs) |
| SQLite lock contention vid resume | Låg | Låg | WAL mode + connection pool (redan löst) |
| WASM binary size ökar markant | Låg | Medel | Alla nya filer är pure Rust — minimal size impact |
| Robots.txt blockerar viktiga sidor | Medel | Medel | Logga i CrawlStats, informera användaren |

---

## 5. Test- och valideringsstrategi

### 5.1 Unit tests per modul

**link_extract.rs:**
```
- test_extract_links_basic: <nav><a href="/about">Om oss</a></nav> → 1 link, zone="navigation"
- test_extract_links_content: <article><a href="/ai-tools">AI tools</a></article> → zone="content"
- test_resolve_relative_urls: href="/page" + base="https://ex.com" → "https://ex.com/page"
- test_scoring_with_goal: goal="AI agents" → link "AI tools" > link "Cookie policy"
- test_dedup_same_url: 2x same href → 1 result
- test_is_internal: same domain = true, different = false
- test_context_snippet: text around link extracted correctly
```

**adaptive.rs (CrawlState):**
```
- test_information_gain_decreases: 3 identical pages → gain approaches 0
- test_information_gain_novel: 3 totally different pages → gain stays high
- test_bayesian_stopping: after 5 redundant pages → should_stop = true
- test_bayesian_continues: after 5 novel pages → should_stop = false
- test_min_pages_respected: even if redundant, don't stop before min_pages
- test_max_pages_stop: stop at max even if gaining
- test_no_gain_streak: 3 no-gain in a row → stop
- test_serialization_roundtrip: CrawlState → JSON → CrawlState
```

**adaptive.rs (Thompson Sampling):**
```
- test_thompson_prefers_high_score: link with high relevance selected first
- test_thompson_explores: low-score link eventually selected (stochastic)
- test_frontier_dedup: same URL added twice → kept once with best score
- test_firewall_filters_frontier: analytics URL blocked
- test_feedback_updates_prior: high gain → α increases
```

### 5.2 Integration tests

**test_adaptive_digest_mock:**
```rust
// 3 mock HTML-sidor med inter-links
// Sida 1: "AI tools" → link till sida 2 och 3
// Sida 2: "LLM agents" → ny info, hög gain
// Sida 3: "AI tools" repeat → redundant, låg gain
// → Crawler bör besöka 1 och 2, möjligen stoppa vid/efter 3
// → combined_nodes bör ha noder från alla besökta sidor
// → stop_reason bör vara InformationSaturated eller NoGainStreak
```

**test_link_extraction_with_head_fetch:**
```rust
// Mock HTTP server som svarar på HEAD + GET range
// Verifiera att title + meta_description hämtas korrekt
// Verifiera att cache fungerar (andra anropet → ingen HTTP)
```

**test_crfr_transfer_cross_page:**
```rust
// Crawla 2 sidor på samma domän
// Verifiera att transfer_from() förbättrar scoring på sida 2
// (Noder som var framgångsrika på sida 1 → högre amplitude på sida 2)
```

### 5.3 Benchmark-scenario

```
Scenario: Crawla Hacker News för "latest AI agent developments"
- Start: https://news.ycombinator.com
- Expected: 5-10 sidor crawlade, stopp vid saturation
- Metrics:
  - Information gain per page (bör sjunka)
  - Bayesian posterior trajectory (bör sjunka)
  - Total unique relevant nodes (bör vara > 20)
  - Total time (bör vara < 30s med rate limiting)
  - Token savings vs "crawla alla sidor" (bör vara > 60%)
```

### 5.4 Jämförelse med Crawl4AI (validering)

| Metric | Crawl4AI förväntat | Vår approach förväntat |
|--------|-------------------|----------------------|
| Pages crawled for 80% coverage | ~15 | ~8–10 (bättre link-val) |
| Time per page | ~2s (Python) | ~0.5s (Rust native) |
| Token cost per page | ~4000 (full HTML) | ~200 (CRFR distillation) |
| Cross-session learning | Ingen | Ja (SQLite causal memory) |
| Stopping accuracy | Statisk threshold | Bayesian adaptive |
| Link selection | Random/all | Thompson Sampling |

### 5.5 Acceptanskriterier

1. `cargo test` — alla befintliga + nya tester passerar
2. `cargo clippy -- -D warnings` — inga nya varningar
3. `cargo fmt --check` — inga diffs
4. Link extraction returnerar korrekta strukturella roller
5. Adaptive crawler stoppar vid information saturation (inte bara max_pages)
6. CRFR transfer fungerar mellan crawlade sidor
7. SQLite persistence möjliggör resume av pausade crawls
8. MCP tools fungerar end-to-end
9. Ingen binary size regression > 100KB (inga nya dependencies)

---

## Appendix A: Crawl4AI-paritet — feature mapping

| Crawl4AI Feature | Vår motsvarighet | Status |
|------------------|------------------|--------|
| `AdaptiveCrawler.run()` | `adaptive_digest()` | Planned |
| `CrawlResult.links` | `extract_links_with_meta()` | Planned |
| `link.metadata.title` | `EnrichedLink.title` (head-fetch) | Planned |
| `link.metadata.description` | `EnrichedLink.meta_description` | Planned |
| `link.context` | `EnrichedLink.context_snippet` | Planned |
| `link.relevance_score` | `EnrichedLink.relevance_score` (CRFR) | Planned |
| `config.confidence_threshold` | `AdaptiveConfig.confidence_threshold` | Planned |
| `config.max_pages` | `AdaptiveConfig.max_pages` | Planned |
| Information Foraging stopping | Bayesian Optimal Stopping | Planned |
| No link scoring | Thompson Sampling MAB | Planned (vi är bättre) |
| Embedding-based similarity | HDC-based (zero-model) | Planned (vi är bättre) |
| No cross-session learning | CRFR causal memory + SQLite | Planned (vi är bättre) |
| No structural awareness | Zone detection + role scoring | Planned (vi är bättre) |

## Appendix B: Nya dependencies

**Inga nya crate-dependencies krävs.** Allt byggs med befintliga:
- `serde` / `serde_json` — serialisering
- `reqwest` — HTTP (redan i fetch.rs)
- `rusqlite` — SQLite (redan i persist.rs)
- `html5ever` / `markup5ever_rcdom` — HTML-parsing (redan)
- `governor` — rate limiting (redan)

Beta-sampling görs med en enkel FNV-hash-baserad pseudo-random generator.
Om vi senare vill ha bättre randomness kan vi lägga till `rand` crate
bakom en feature flag.
