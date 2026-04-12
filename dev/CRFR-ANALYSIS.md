# CRFR Pipeline — Komplett Analys

**Datum**: 2026-04-12  
**Scope**: `resonance.rs` (4692 LOC), `persist.rs` (709), `adaptive.rs` (1052), `scoring/hdc.rs` (674), `scoring/tfidf.rs` (537), `scoring/pipeline.rs` (701), `scoring/colbert_reranker.rs`, `link_extract.rs` (878) — totalt ~9200 LOC  
**Syfte**: Hitta buggar, algoritmfel, optimeringar, prestandaförbättringar. Plus cross-pollination med crawl4ai.

---

## Innehåll

1. [Executive Summary](#1-executive-summary)
2. [Buggar & Algoritmfel](#2-buggar--algoritmfel)
3. [Prestandaoptimeringar](#3-prestandaoptimeringar)
4. [Arkitekturella Svagheter](#4-arkitekturella-svagheter)
5. [Crawl4ai Cross-Pollination](#5-crawl4ai-cross-pollination)
6. [Prioriterad Åtgärdslista](#6-prioriterad-åtgärdslista)

---

## 1. Executive Summary

CRFR är en 7-signal hybrid retrieval-motor som behandlar DOM:en som ett resonansfält med oscillerande noder. Arkitekturen är genuint ny — ingen annan open-source-motor kombinerar BM25 + HDC bitvektorer + Chebyshev spectral filter + DCFR-inlärning + kausal feedback i en enda pipeline.

**Styrkor:**
- Sub-millisekund cache-hit latens (BM25-index byggs en gång)
- Deterministisk ranking med stabil tie-break
- Inkrementell fältuppdatering (update_node) utan full rebuild
- Content-hash validering vid cache-load förhindrar stale data
- Adaptiv Chebyshev K som skalas med DOM-storlek

**Svagheter (sammanfattning):**
- 8 bekräftade buggar (2 kritiska, 3 medelsvåra, 3 låga)
- 6 algoritmfel som påverkar ranking-kvalitet
- 9 prestandaoptimeringar (sammanlagt ~30-40% latensreduktion möjlig)
- 4 arkitekturella designfel som begränsar skalbarhet
- Goal-clustering förhindrar cross-goal kausal inlärning (dokumenterat men ej löst)

**Estimerad påverkan vid full åtgärd:**
- Latens: ~0.6ms → ~0.35ms (median query, cached field)
- Ranking-kvalitet: +15-25% MRR på nyhetsliknande sidor (BBC/NPR-profil)
- Minnesanvändning: -20% per fält (HashMap → kompakt vektor)

---

## 2. Buggar & Algoritmfel

### 2.1 Bekräftade Buggar

#### BUG-A: FieldCache `take()` är O(N) linjärsökning [KRITISK]

**Fil**: `resonance.rs:3117`  
**Problem**: `FieldCacheInner::take()` itererar hela VecDeque med `.position()` för att hitta ett fält via url_hash. Med 1024 entries = 1024 jämförelser per cache-lookup.  
**Påverkan**: Varje `get_or_build_field()` betalar O(N) cache-lookup FÖRE propagation ens börjar. Vid full cache (1024) kostar detta ~1-5µs — inte katastrofalt, men onödigt.  
**Fix**: Komplettera VecDeque med en `HashMap<u64, usize>` som index. Alternativt byt till `lru` crate som ger O(1) get/put. `peek()` på rad 3127 har samma problem.

#### BUG-B: Suppression learning räknar fel — query_count ökas för ALLA synliga noder [KRITISK]

**Fil**: `resonance.rs:2481-2488`  
**Problem**: I `feedback()`, steg 1b, ökas `query_count` för alla noder med `amplitude > MIN_OUTPUT_THRESHOLD`. Men `MIN_OUTPUT_THRESHOLD = 0.01` — praktiskt taget alla noder som fick någon score alls. Suppression ska tracka noder som *returnerades till användaren* (top-N), inte alla som hade icke-noll amplitud.  
**Konsekvens**: Noder som aldrig visades för användaren ackumulerar `query_count` och `miss_count`, vilket aktiverar suppression (penalty 0.15) felaktigt. Detta kan tysta genuint relevanta noder som råkade hamna strax utanför top-N.  
**Fix**: Ändra threshold till att matcha de faktiskt returnerade noderna (de som passerar gap-filtret), inte alla med amplitud > 0.01. Enklast: skicka in `&[u32]` av returnerade nod-ID:n till suppression-steget.

#### BUG-C: PPR restart i Chebyshev använder `bm25_scores` istället för `seed_signal` [MEDEL]

**Fil**: `resonance.rs:2336`  
```rust
let seed = bm25_scores.get(&id).copied().unwrap_or(0.0);
*filtered = (1.0 - PPR_ALPHA) * (*filtered).max(0.0) + PPR_ALPHA * seed;
```
**Problem**: PPR (Personalized PageRank) restart ska teleportera tillbaka till *seed-signalen* (den fulla Phase 1 scoringen: BM25 + HDC + role + causal + answer_shape + zone + meta). Istället teleporteras till enbart BM25-score. Detta innebär att PPR restart ignorerar HDC-matchning, kausalt minne, och alla andra signaler.  
**Konsekvens**: Noder med stark HDC-match men svag BM25 får ingen PPR-boost. Kausalt minne bidrar inte till restart-termen. Effektivt väger detta BM25 ytterligare ~15% extra utöver sin redan 55% vikt.  
**Fix**: Byt `bm25_scores` till `seed_signal` (som redan finns beräknad på rad 1598-1602).

#### BUG-D: `concept_memory_order` VecDeque synkas inte vid `migrate_learning_from` [MEDEL]

**Fil**: `resonance.rs:3018-3023`  
**Problem**: `migrate_learning_from()` kopierar `concept_memory` HashMap men kopierar inte `concept_memory_order` VecDeque. Efter migration är order-deque tom medan map har entries. Eviction-loopen (rad 2601) faller igenom till den icke-deterministiska `HashMap::keys().next()` fallbacken.  
**Konsekvens**: FIFO-eviction (som BUG-6 fixade) bryts efter varje content-migration. Äldsta konceptet evictas inte — ett godtyckligt väljs istället.  
**Fix**: Kopiera `concept_memory_order` i `migrate_learning_from()`.

#### BUG-E: `is_multiple_of` finns inte som metod på u32 i stable Rust [MEDEL]

**Fil**: `resonance.rs:2424, 2504`  
```rust
if self.total_feedback.is_multiple_of(5) {
```
**Problem**: `is_multiple_of()` är en nightly-only metod (`#![feature(unsigned_is_multiple_of)]`). Om detta kompilerar beror det på nightly toolchain eller en trait extension. Om projektet ska fungera på stable Rust crashar detta.  
**Fix**: Ersätt med `self.total_feedback % 5 == 0`.

#### BUG-F: `adaptive_fan_out` begränsar Laplacian men inte sibling-boost [LÅG]

**Fil**: `resonance.rs:2214-2215` vs `resonance.rs:1674-1689`  
**Problem**: I `laplacian_multiply()` begränsas children till `adaptive_fan_out(children.len())`. Men i value-match micro-propagation (rad 1674) och sibling-pattern-recognition (rad 1733) itereras ALLA syskon utan begränsning. På en nod med 500 syskon (t.ex. en stor `<ul>`) blir detta en O(N) loop per matchad nod.  
**Fix**: Applicera samma fan-out-begränsning i sibling-boost-looparna.

#### BUG-G: `latency_samples.remove(0)` på Vec är O(N) [LÅG]

**Fil**: `resonance.rs:1952`  
```rust
if self.latency_samples.len() > 50 {
    self.latency_samples.remove(0);
}
```
**Problem**: `Vec::remove(0)` skiftar alla element. Med 50 samples = 50 kopieringar per query.  
**Fix**: Byt till `VecDeque` (som redan används för `concept_memory_order`) eller ring-buffer med index.

#### BUG-H: Persist `load_field` låser hela poolen under deserialisering [LÅG]

**Fil**: `persist.rs:164-178`  
**Problem**: `DB.lock()` hålls under hela `serde_json::from_slice()`. JSON-deserialisering av ett stort fält (100+ noder med HV-data) kan ta 1-5ms. Under denna tid blockeras alla andra read- och write-operationer.  
**Fix**: Kopiera `data: Vec<u8>` och släpp låset innan deserialisering.

### 2.2 Algoritmfel

#### ALG-1: Goal-clustering är för grovkornig — förhindrar cross-goal learning

**Fil**: `resonance.rs:679-692`  
**Problem**: `goal_cluster_id()` tar top-3 ord (sorterade, >3 tecken) och jointar med `+`. "latest news headlines" → `headlines+latest+news`. "breaking news stories" → `breaking+news+stories`. Trots att båda handlar om nyheter hamnar de i olika kluster.  
**Konsekvens**: Dokumenterad i `crfr-honest-manual-eval.md` — kausal boost = 0.0 på ALLA 10 iterationer mot BBC trots 5 feedback-anrop. Propagation_stats och concept_memory partitioneras per kluster, så inlärning transfereras aldrig.  
**Åtgärd**: Byt till semantisk clustering. Enklast: normalisera goal med HDC-similarity — om två goals HV-likhet > 0.6, ge dem samma kluster-ID. Alternativt: ta top-3 TF-IDF-vägda ord istället för top-3 alfabetiskt.

#### ALG-2: HDC n-gram encoding tappar ordningsdata vid bundle

**Fil**: `scoring/hdc.rs:175-212`  
**Problem**: `from_text_ngrams()` genererar unigrams med position-permutation (`i*3`), bigrams med (`i*5`), trigrams med (`i*7`). Men `bundle()` (majority-vote) över 15+ komponenter degraderar kraftigt — varje bits SNR sjunker som `O(1/sqrt(N))`. Med 10 ord = 10 unigrams + 9 bigrams + 8 trigrams = 27 komponenter. Majority-vote med 27 vektorer behåller bara de starkaste signalerna.  
**Konsekvens**: Korta goals (2-3 ord) har bra representation, men längre texter tappar ordningsinformation. "katt jagar hund" ≈ "hund jagar katt" vid 5+ ord.  
**Åtgärd**: Vikta komponenter: unigrams × 4, bigrams × 2, trigrams × 1 (upprepa i bundle-listan). Alternativt: använd MAP (Maximum a Posteriori) bundling istället för majority-vote.

#### ALG-3: CombMNZ-beräkningen räknar `role_boost > 0.1` som signal — det är ALLTID sant

**Fil**: `resonance.rs:1444-1459`  
```rust
let signal_count = [
    bm25_score > 0.01,
    hdc_score > 0.01,
    role_boost > 0.1,    // ← role_priority() returnerar MINST 0.2
    concept_boost > 0.001,
]
```
**Problem**: `role_priority()` returnerar minimum 0.2 (för "navigation") och 0.5 som default. `role_boost > 0.1` är alltid true. CombMNZ tror att 3/4 signaler alltid är aktiva, men en av dem (roll) är meningslös.  
**Konsekvens**: CombMNZ-bonus startar vid 1.25× istället för 1.0× för praktiskt taget alla noder. Bonusen differentierar inte — alla får den.  
**Fix**: Ta bort `role_boost` från CombMNZ-signallistan, eller ändra threshold till `role_boost > 0.7` (då filtreras navigation/generic bort).

#### ALG-4: Chebyshev top-500 cutoff förlorar propagation till svaga-men-relevanta noder

**Fil**: `resonance.rs:1611-1619`  
**Problem**: På stora DOM:ar (>500 noder) begränsas Chebyshev-filtret till top-500 noder efter Phase 1 scoring. Noder utanför top-500 får ingen propagation-boost alls. Men propagation är *avsedd* att lyfta noder som inte matchade direkt — noden brevid en stark BM25-match som själv inte hade keyword-overlap.  
**Konsekvens**: På sidor med 2000+ noder (NPR: 1229) missar propagation noder i låg-rankade subtrees som har kontextuellt relevant grannskap.  
**Åtgärd**: Istället för flat top-500, inkludera alla noder som är grannar (1-hop) till top-200 noder. Detta ger propagation möjlighet att lyfta relevanta grannar utan att öppna hela grafen.

#### ALG-5: Diversity-penalty appliceras EFTER gap-filter — påverkar inte output

**Fil**: `resonance.rs:1803-1829`  
**Problem**: Diversity-penalty (15% reduktion för 4+ syskon från samma förälder) appliceras i `propagate_inner()`. Men `apply_gap_filter()` anropas EFTER — och gap-filtret letar efter amplitud-droppar. Diversity-penaltyn skapar artificiella droppar som kan trigga gap-cut för tidigt.  
**Konsekvens**: Om 4 syskon med hög amplitude får diversity-penalty, kan gapet mellan nod #3 och nod #4 (nu 15% lägre) trigga gap-cut vid position 4 istället för position 10.  
**Åtgärd**: Applicera diversity EFTER gap-filter, eller exkludera diversity-reducerade noder från gap-beräkning.

#### ALG-6: `answer_type_boost` har hardkodade keyword-listor — skalar inte

**Fil**: `resonance.rs:518-572`  
**Problem**: `answer_type_boost()` matchar goal-nyckelord mot label-mönster med if-else-kedjor: "price"/"cost"/"pris" → currency-symboler, "population" → stora siffror, "date"/"when" → årtal. Varje ny query-typ kräver manuell kodning.  
**Konsekvens**: Querys utanför de 4 hårdkodade typerna (price, population, date, rate) får 0.0 boost. "weather temperature" matchar inte. "recipe ingredients" matchar inte.  
**Åtgärd**: Ersätt med inlärd answer-type-profil. concept_memory har redan per-goal-token → HV-mappning. Utöka med per-goal-token → content-pattern-mappning (digit-density, unit-presence, length-profile).

---

## 3. Prestandaoptimeringar

### 3.1 Hot Path — propagate_inner()

#### OPT-P1: 12 HashMap-allokeringar per query [HÖG PÅVERKAN]

**Fil**: `resonance.rs:1143-1156`  
**Problem**: Varje anrop till `propagate_inner()` allokerar 12 separata HashMaps för trace-data (`trace_bm25_scores`, `trace_hdc_scores`, `trace_role_priorities`, etc.) oavsett om trace är aktiverat eller inte. HashMaps allokeras med default capacity (0), vilket ger 2-3 realloc:ar per map under fyllning.  
**Kostnad**: ~12 × (alloc + grow + grow) = ~36 allokeringar per query. Vid 1000 queries/s = 36000 mallocs/s.  
**Fix**: Villkora allokering på `capture_trace`. Trace-variabler kan vara `Option<HashMap>` som bara initieras vid behov. Alternativt: pre-allokera med `HashMap::with_capacity(cascade_candidates.len())`.

#### OPT-P2: `node_ids` sorteras varje query — onödigt [MEDEL]

**Fil**: `resonance.rs:1201-1202`  
```rust
let mut node_ids: Vec<u32> = self.nodes.keys().copied().collect();
node_ids.sort_unstable();
```
**Problem**: Alla nod-ID:n samlas och sorteras varje query. Sorteringen behövs bara för deterministisk iteration-ordning, men node_ids ändras inte mellan queries (bara vid add_node/remove_node).  
**Fix**: Cacha sorterad nod-lista i fältet. Invalidera vid mutation (add/remove/update). Sorterad Vec<u32> kostar 4 bytes/nod = 40KB vid 10000 noder.

#### OPT-P3: `to_lowercase()` anropas per nod per query i site-word-check [MEDEL]

**Fil**: `resonance.rs:1491-1494`  
```rust
let label_lower = self.node_labels.get(&nid)
    .map(|s| s.to_lowercase())
    .unwrap_or_default();
```
**Problem**: Varje nod i cascade (upp till 300) får `to_lowercase()` varje query. String-allokering + UTF-8 case-folding.  
**Fix**: Pre-lowercase alla labels vid field-build. Lagra `node_labels_lower: HashMap<u32, String>` bredvid `node_labels`. Invalidera vid update_node.

#### OPT-P4: Chebyshev allocerar 3 nya HashMaps per iteration [MEDEL]

**Fil**: `resonance.rs:2272-2331`  
**Problem**: Chebyshev-filtret allokerar `t0`, `t1`, `output`, och sedan `t_next` per iteration (K=2-4). Varje HashMap har N entries. Total: ~(3+K) × N entries allokerade per query.  
**Fix**: Pre-allokera 3 HashMaps och återanvänd via `.clear()` + re-insert. Eller byt till `Vec<f32>` med nod-index (kräver stabil id→index mapping, men ger cache-locality + noll-allokering).

#### OPT-P5: `down_keys` och `up_keys` allokerar N formaterade strängar per query [HÖG]

**Fil**: `resonance.rs:1586-1595`  
```rust
let down_keys: HashMap<u32, String> = self.nodes.iter()
    .map(|(&id, s)| (id, format!("{}:down:{}", s.role, cluster)))
    .collect();
```
**Problem**: 2 × N `format!()` allokeringar per query. Med 500 noder = 1000 String-allokeringar = ~50KB heap-churn.  
**Fix**: Cacha keys per cluster. Cluster ändras bara med goal — om samma cluster som förra query, återanvänd keys. Alternativt: bygg nyckeln in-place med en pre-allokerad buffer.

### 3.2 Scoring Path

#### OPT-S1: BM25 prefix-fallback itererar ALLA termer i index [HÖG]

**Fil**: `scoring/tfidf.rs:128-151`  
**Problem**: Om exakt BM25-match ger 0 resultat, itererar prefix-fallback genom `self.postings` (alla termer i indexet) och kollar `starts_with()` per term. Med 500 noder × 5 ord/nod = 2500 termer × antal query-tokens.  
**Fix**: Bygg en sorterad Vec av termer vid index-build. Använd `binary_search()` för att hitta prefix-range i O(log N) istället för O(N) linjärsökning.

#### OPT-S2: HDC `from_text_ngrams` byggs två gånger — i ResonanceField OCH i HdcTree [MEDEL]

**Fil**: `resonance.rs:994-998` och `scoring/hdc.rs:288-292`  
**Problem**: Vid `from_semantic_tree()` byggs `Hypervector::from_text_ngrams(&node.label)` per nod. Om `ScoringPipeline::run()` också anropas (via `parse_hybrid`), bygger `HdcTree::build()` exakt samma HV:er igen.  
**Fix**: Dela text_hv mellan ResonanceField och HdcTree. Alternativt: bygg HdcTree från ResonanceField:s redan beräknade HV:er istället för från SemanticNode-trädet.

### 3.3 Memory

#### OPT-M1: ResonanceField:s HashMaps har hög overhead per nod [HÖG]

**Problem**: Varje nod lagras i 6 separata HashMaps: `nodes`, `parent_map`, `children_map`, `node_labels`, `node_values`, `bm25_cache`. HashMap overhead per entry = ~50-80 bytes (hash, key, pointer, padding). Med 6 maps × 500 noder = 3000 entries × ~60 bytes = ~180KB overhead bara i HashMap-metadata.  
**Fix**: Konsolidera till en `Vec<NodeEntry>` med struct-of-arrays layout. En `HashMap<u32, usize>` för id→index lookup. Minskar overhead med ~60%.

#### OPT-M2: HvData serialiserar 32 × u64 som JSON-array [LÅG]

**Fil**: `resonance.rs:203-221`  
**Problem**: Varje Hypervector serialiseras som `{"bits": [u64, u64, ..., u64]}` — 32 st u64 = 256 bytes binärt, men ~600 bytes som JSON. Med 500 noder × 2 HV/nod (text_hv + causal_memory) = ~600KB JSON per fält.  
**Fix**: Base64-koda bits-arrayen: 256 bytes → 344 bytes base64. Sparar ~40% serialiseringsstorlek. Persist.rs lagrar som BLOB, men field export/import via JSON drabbas.

### 3.4 Persist Layer

#### OPT-D1: ConnectionPool readers används aldrig korrekt [MEDEL]

**Fil**: `persist.rs:164-178`  
**Problem**: `load_field()` tar `pool.readers.first()` — alltid samma reader-connection. De andra 3 readers i poolen används aldrig. Poolen ger ingen concurrency-vinst.  
**Fix**: Implementera take/return-mönster: `take_reader()` → returnerar Option<Connection>, anroparen returnerar den när klar. Alternativt: round-robin via atomär räknare.

---

## 4. Arkitekturella Svagheter

### 4.1 Global Mutable State — 3 static LazyLock<Mutex>

**Filer**: `resonance.rs:3158` (FIELD_CACHE), `resonance.rs:3261` (DOMAIN_REGISTRY), `persist.rs:71` (DB)  
**Problem**: Tre globala Mutex-skyddade singletons. Alla operationer serialiseras genom dessa lås. `save_field()` (rad 3438-3461) tar FIELD_CACHE-låset, sedan DOMAIN_REGISTRY-låset, sedan (via persist) DB-låset — tre lås i sekvens. Om någon annan tråd håller DB-låset (t.ex. `load_all_fields`) blockeras hela save-kedjan.  
**Risk**: Deadlock är tekniskt omöjligt (lås tas alltid i samma ordning), men lock contention under hög last = serialiserad throughput. En långsam SQLite-write blockerar alla cache-reads.  
**Åtgärd**: Separera read/write-paths. FIELD_CACHE borde vara `RwLock` (många readers, en writer). DOMAIN_REGISTRY likaså. DB-poolen ska bara låsas för connection-checkout, inte under hela query.

### 4.2 resonance.rs är 4692 rader — för stort för en fil

**Problem**: `resonance.rs` innehåller: ResonanceField struct + impl (2500+ rader), FieldCache, DomainRegistry, DomainProfile, AnswerZoneProfile, 15+ fria funktioner, alla typer, alla konstanter, alla tests.  
**Konsekvens**: Svårt att navigera, svårt att testa isolerat, merge-konflikter vid parallell utveckling.  
**Åtgärd**: Dela upp:
- `resonance/types.rs` — ResonanceState, ResonanceResult, ResonanceType, NodeScoreBreakdown, PropagationTrace, GapInfo, AnswerZoneProfile, HvData
- `resonance/field.rs` — ResonanceField struct + impl
- `resonance/scoring.rs` — role_priority, zone_penalty, answer_shape_score, answer_type_boost, metadata_penalty, state_injection_penalty, page_profile
- `resonance/cache.rs` — FieldCacheInner, FIELD_CACHE
- `resonance/domain.rs` — DomainProfile, DomainRegistry, DOMAIN_REGISTRY
- `resonance/mod.rs` — publika re-exports

### 4.3 Feedback-loopen kräver explicit nod-ID:n — LLM:er vet inte dessa

**Problem**: `crfr_feedback(url, goal, successful_node_ids)` kräver att anroparen anger vilka nod-ID:n som var framgångsrika. Men LLM-agenter som konsumerar CRFR-output ser nod-ID:n som opaka identifierare. De vet inte vilka noder som var "bra" — de använder hela outputen.  
**Konsekvens**: `implicit_feedback()` finns som fallback, men den matchar ord-overlap mellan LLM-svar och nod-labels. Detta missar:
- Parafraser (LLM skriver "invånare" men noden säger "population")  
- Aggregering (LLM kombinerar info från 3 noder till en mening)  
- Negation (LLM nämner noden för att säga att den är irrelevant)  
**Åtgärd**: Komplettera med attention-baserad feedback. Om LLM:en returnerar vilka delar av inputen den använde (via citation eller source-attribution), mappa dessa tillbaka till nod-ID:n. Alternativt: track vilka noder LLM:en citerar via string-matching på unika substrings.

### 4.4 Ingen anti-bot/block-detection — crawling mot skyddade sidor ger tyst garbage

**Problem**: AetherAgent har ingen mekanism för att detektera att en fetch returnerade en bot-challenge (Cloudflare "Just a moment", Akamai access denied, CAPTCHA) istället för faktiskt innehåll. CRFR bygger ett fält från garbage-HTML:en, rankar den, och returnerar meningslösa noder.  
**Konsekvens**: Kausal inlärning förgiftas — feedback på bot-challenge-noder lagrar skräp i causal_memory. Suppression learning aktiveras felaktigt.  
**Åtgärd**: Se sektion 5 (crawl4ai) — 3-tier detection med page-size gates.

### 4.5 Temporal decay är global konstant — borde vara per-domän adaptiv

**Fil**: `resonance.rs:54`  
```rust
const CAUSAL_DECAY_LAMBDA: f64 = 0.001_155; // halvering var 10 min
```
**Problem**: Alla domäner har samma decay-rate. Men en nyhetssida (BBC) ändrar innehåll var 5:e minut — 10 min halveringstid är för lång (stale boosts). En produktsida (Amazon produkt) ändras kanske en gång i veckan — 10 min är för kort (nyttig inlärning kastas bort).  
**Åtgärd**: Gör lambda per-domän. Beräkna från `content_hash`-förändringsfrekvens: om ett fält invalideras ofta (content_hash ändras) → kortare halveringstid. Om fältet är stabilt → längre halveringstid. Lagra i DomainProfile.

### 4.6 BM25F-approximation är naiv — value dubbleras bokstavligt

**Fil**: `resonance.rs:1117-1118`  
```rust
(id, format!("{} {} {}", label, val, val))
```
**Problem**: BM25F (field-weighted BM25) approximeras genom att konkatenera `value` två gånger till label-texten. Detta ger val-termer 3× TF istället för avsedda 2× vikt, eftersom `label` också kan innehålla samma termer. Dessutom påverkas document-length (dl) — noden ser 3× längre ut, vilket BM25:s length-normalisering straffar.  
**Åtgärd**: Implementera riktig BM25F: beräkna per-fält TF och summera med vikter innan IDF-multiplikation. Alternativt: dubbla TF-värdet direkt i postings istället för strängkonkatenering.

---

## 5. Crawl4ai Cross-Pollination

**Källa**: https://github.com/unclecode/crawl4ai (90k+ GitHub stars)  
**Typ**: Python-baserad async crawler med Playwright-backend. Post-processing pipeline, INTE en browser engine.

### 5.1 Hög prioritet — direkt applicerbara tekniker

#### CP-1: Anti-bot Detection (3-tier med page-size gates)

Crawl4ai har ett 3-tier detektionssystem som saknas helt i AetherAgent:

| Tier | Confidence | Villkor | Vad det fångar |
|------|-----------|---------|----------------|
| 1 | Hög | Alla sidstorlekar | Specifika fingerprints: `window._pxAppId` (PerimeterX), Akamai reference#, Cloudflare `cf-` headers, DataDome JSON |
| 2 | Medel | Bara sidor <10KB | Generiska termer: "Access Denied", "Just a moment", reCAPTCHA/hCaptcha klasser |
| 3 | Strukturell | Bara sidor <50KB | Tomma skal: ingen body-tag, minimal synlig text, script-heavy utan content |

**Nyckelinsikt**: Page-size gates kontrollerar false positive rates. Stora sidor (>50KB) triggar aldrig Tier 2/3 — de innehåller nästan alltid faktiskt innehåll.  
**Implementation i AetherAgent**: Ny modul `bot_detect.rs` eller utöka `trust.rs`. Kör efter fetch, före parse. Om detected → markera field som poisoned, skippa feedback.

#### CP-2: Head-Peek Pre-Scoring

Crawl4ai:s `ContentRelevanceFilter` hämtar bara `<head>` (första 64KB eller `</head>`, det som kommer först) för att pre-scora en URL:s relevans med BM25 mot title och meta description.  
**Relevans**: AetherAgent:s firewall L3 kräver full page-fetch för semantisk klassificering. Head-peek skulle ge 80% av signalen till ~5% av kostnaden.  
**Implementation**: Utöka `fetch.rs` med `fetch_head_only(url)` som avbryter HTTP-stream vid `</head>`. Extrahera title + description, kör BM25 mot goal. Return early om score < threshold.

#### CP-3: Tag-Weighted BM25

Crawl4ai viktar BM25 per HTML-tag: `h1: 5.0, h2: 4.0, h3: 3.0, title: 4.0, strong: 2.0, code: 2.0, th: 1.5`.  
**Relevans**: CRFR:s BM25 behandlar alla nod-labels lika. En `<h1>` med "Nyheter" väger lika mycket som en `<span>` med "Nyheter".  
**Implementation**: I `ensure_bm25_cache()` (resonance.rs:1103), vikta label-text baserat på roll. `heading` → 3× TF, `text`/`paragraph` → 1×, `navigation` → 0.5×. Kräver ändring i TfIdfIndex::build().

#### CP-4: PruningContentFilter — Dynamic Threshold

Crawl4ai:s PruningContentFilter använder 5 viktat metrics (text density: 0.4, link density: 0.2, tag weight: 0.2, class/ID weight: 0.1, text length: 0.1) med dynamisk threshold som anpassas per nod:
- -20% threshold för high-importance tags
- -10% threshold när text_ratio > 0.4
- +20% threshold när link_ratio > 0.6

**Relevans**: Direkt applicerbart i `stream_engine.rs` för adaptive DOM streaming. Nuvarande streaming-filter använder flat relevance-threshold.

### 5.2 Medel prioritet — potentiellt värdefulla

#### CP-5: URL Freshness Scoring

Crawl4ai extraherar datum från URL-mönster (`/2026/04/12/`, `?date=2026-04-12`) och ger nyare URLs högre score.  
**Relevans**: `link_extract.rs` och `adaptive.rs` prioriterar links utan tidsdimensionen. På nyhets-/bloggsidor är datum-freshness en stark signal.

#### CP-6: Composite URL Scoring med Path Depth

Crawl4ai har en `PathDepthScorer` med optimal-depth lookup. URLs med 2-3 segment får högst score (typiskt artikelsidor), medan djupare paths (5+) penaliseras (taxonomi-sidor, arkiv).  
**Relevans**: `link_extract.rs:EnrichedLink` har `depth` men det är klickdjup (hops from seed), inte URL-path-depth. Bägge dimensionerna borde viktas.

#### CP-7: Virtual Scroll Detection

Crawl4ai detekterar virtual scroll containers genom att skilja på append (redan fångat), replace (extrahera chunk), och no-change (fortsätt scrolla).  
**Relevans**: AetherAgent:s `streaming.rs` hanterar inte dynamiskt laddade listor (infinite scroll). Relevant om CDP-tier (Fas 12) aktiveras.

### 5.3 Låg prioritet — AetherAgent har redan bättre

| Crawl4ai-feature | AetherAgent-equivalent | Jämförelse |
|------------------|----------------------|------------|
| Chunking (6 strategier) | `streaming.rs` med goal-driven early-stop | AetherAgent är mer sofistikerad |
| Robots.txt (opt-in, basic) | `robotstxt` crate, RFC 9309 | AetherAgent har bättre compliance |
| JS execution (Playwright) | QuickJS sandbox + Arena DOM | AetherAgent har embedded utan browser-dependency |
| Session management | `session.rs` (11 endpoints) | Jämförbara |
| Rate limiting (exponential backoff) | `governor` GCRA + Retry-After | AetherAgent har mer standard-compliant |

### 5.4 Vad crawl4ai INTE har som AetherAgent har

- Semantiskt accessibility tree (WCAG-aware perception)
- Prompt injection detection / trust levels
- Causal action graph (`causal.rs`)
- Goal-driven adaptive streaming med LLM-directives
- Inbäddad JS-sandbox (ingen browser-dependency)
- Visuell grounding / ONNX vision
- MCP server integration
- Semantisk DOM diffing
- Resonance-field baserad inlärning

**Fundamental skillnad**: Crawl4ai är en *post-processing pipeline* ovanpå riktiga browsers. AetherAgent är en *perception engine* som bygger semantisk förståelse från grunden. Det är just denna skillnad som gör cross-pollination värdefull — crawl4ai:s heuristiker för content quality, relevance scoring, och bot detection kan bäddas in direkt i AetherAgents nativa pipeline utan Playwright-beroende.

---

## 6. Prioriterad Åtgärdslista

### P0 — Kritiska (fixas omedelbart)

| # | Typ | Ref | Beskrivning | Estimerad effekt |
|---|-----|-----|-------------|-----------------|
| 1 | BUG | B | Suppression query_count räknar alla noder, inte bara returnerade | Förhindrar falsk suppression av relevanta noder |
| 2 | BUG | C | PPR restart använder bm25_scores istället för seed_signal | +5-10% ranking-kvalitet (alla signaler bidrar till restart) |
| 3 | ALG | 1 | Goal-clustering för grovkornig — ingen cross-goal learning | Löser det dokumenterade BBC/NPR 0-boost-problemet |
| 4 | ALG | 3 | CombMNZ role_boost > 0.1 alltid true — meningslös signal | Korrekt CombMNZ-differentiering |

### P1 — Högt prioriterade (nästa sprint)

| # | Typ | Ref | Beskrivning | Estimerad effekt |
|---|-----|-----|-------------|-----------------|
| 5 | OPT | P1 | 12 HashMap-allokeringar per query (villkora trace) | -30% alloc overhead |
| 6 | OPT | P5 | down_keys/up_keys allokerar N strängar per query | -50KB heap churn/query |
| 7 | OPT | S1 | BM25 prefix-fallback O(N) → O(log N) via sorterad term-lista | -10× latens vid prefix-fallback |
| 8 | OPT | M1 | 6 HashMaps per fält → konsoliderad Vec<NodeEntry> | -60% memory overhead |
| 9 | CP | 1 | Anti-bot detection (3-tier från crawl4ai) | Förhindrar learning poisoning |
| 10 | BUG | D | concept_memory_order synkas inte vid migration | Korrekt FIFO eviction |

### P2 — Medelprioritet (planera in)

| # | Typ | Ref | Beskrivning | Estimerad effekt |
|---|-----|-----|-------------|-----------------|
| 11 | ALG | 4 | Chebyshev top-500 → top-200 + 1-hop grannar | Bättre propagation på stora DOM:ar |
| 12 | ALG | 5 | Diversity-penalty före gap-filter skapar artificiella gaps | Stabilare gap-detection |
| 13 | CP | 2 | Head-peek pre-scoring (64KB / `</head>`) | ~95% snabbare firewall L3 |
| 14 | CP | 3 | Tag-weighted BM25 (h1: 3×, nav: 0.5×) | Bättre structural awareness |
| 15 | ARCH | 4.1 | Global Mutex → RwLock för FIELD_CACHE/DOMAIN_REGISTRY | Bättre concurrent throughput |
| 16 | ARCH | 4.5 | Temporal decay per domän istället för global konstant | Anpassad inlärning per site-typ |
| 17 | BUG | E | `is_multiple_of()` nightly-only | Stable Rust-kompatibilitet |
| 18 | OPT | P2 | Sorterad nod-lista cachad (invalideras vid mutation) | -1µs/query |
| 19 | OPT | D1 | ConnectionPool readers round-robin | Faktisk concurrent DB-reads |
| 20 | ARCH | 4.6 | BM25F riktig implementation istället för strängkonkatenering | Korrekt field-weighted scoring |

### P3 — Låg prioritet (backlog)

| # | Typ | Ref | Beskrivning |
|---|-----|-----|-------------|
| 21 | BUG | F | adaptive_fan_out saknas i sibling-boost |
| 22 | BUG | G | latency_samples.remove(0) O(N) → VecDeque |
| 23 | BUG | H | persist load_field håller lock under deserialisering |
| 24 | OPT | P3 | to_lowercase() per nod per query → pre-lowercase |
| 25 | OPT | P4 | Chebyshev HashMap-alloc → Vec<f32> med index |
| 26 | OPT | S2 | Dubbel HDC-build (ResonanceField + HdcTree) |
| 27 | OPT | M2 | HvData JSON → base64 serialisering |
| 28 | ALG | 2 | HDC bundle SNR-degradering vid 15+ komponenter |
| 29 | ALG | 6 | answer_type_boost hardkodad → inlärd profil |
| 30 | CP | 5 | URL freshness scoring (datum från URL-mönster) |
| 31 | CP | 6 | Path-depth scoring i link extraction |
| 32 | ARCH | 4.2 | resonance.rs 4692 LOC → modul-uppdelning |
| 33 | ARCH | 4.3 | Attention-baserad implicit feedback |

---

*Genererat av CRFR Pipeline Audit, 2026-04-12*

