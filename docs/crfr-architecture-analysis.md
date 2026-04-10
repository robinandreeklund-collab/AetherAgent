# CRFR — Komplett Arkitekturanalys

**Causal Resonance Field Retrieval**
Senast uppdaterad: 2026-04-10

---

## 1. Systemöversikt

CRFR behandlar DOM:en som ett **levande resonansfält** — varje nod är en oscillator
med en amplitud som bestäms av mål-likhet, kausalt minne och vågpropagation genom
trädstrukturen. Systemet **lär sig över tid** via feedback-loopar.

### 1.1 Vad CRFR gör

Givet en DOM (semantiskt träd) och ett mål (goal), returnerar CRFR de N mest
relevanta noderna — rankade, filtrerade och med full spårbarhet.

### 1.2 Arkitekturdiagram — Komplett Pipeline

```
 ┌─────────────────────────────────────────────────────────────────────────────┐
 │                         CRFR KOMPLETT PIPELINE                             │
 │                                                                            │
 │  HTML ──► Semantic Tree ──► ResonanceField ──► Propagation ──► Resultat    │
 │                                   │                                │       │
 │                                   │          ┌─────────────────────┘       │
 │                                   ▼          ▼                             │
 │                            ┌─────────────────────┐                         │
 │                            │   Feedback Loop      │                        │
 │                            │                      │                        │
 │                            │  Explicit feedback   │                        │
 │                            │  Implicit feedback   │                        │
 │                            │  Suppression learn   │                        │
 │                            └──────────┬──────────┘                         │
 │                                       │                                    │
 │                                       ▼                                    │
 │                            ┌──────────────────────┐                        │
 │                            │   Persistence Layer   │                       │
 │                            │                       │                       │
 │                            │  RAM Cache (1024 LRU) │                       │
 │                            │  SQLite WAL           │                       │
 │                            │  Domain Registry      │                       │
 │                            └──────────────────────┘                        │
 └─────────────────────────────────────────────────────────────────────────────┘
```

### 1.3 Datastrukturer — Kärntyper

```
ResonanceField                          ResonanceState (per nod)
├── nodes: HashMap<u32, ResonanceState>  ├── text_hv: Hypervector (2048-bit)
├── parent_map: HashMap<u32, u32>        ├── role: String
├── children_map: HashMap<u32, Vec<u32>> ├── depth: u32
├── node_labels: HashMap<u32, String>    ├── phase: f32
├── node_values: HashMap<u32, String>    ├── amplitude: f32
├── bm25_cache: Option<TfIdfIndex>       ├── causal_memory: Hypervector
├── propagation_stats: HashMap           ├── hit_count: u32
├── concept_memory: HashMap              ├── last_goal_hash: u64
├── structure_hash: u64                  ├── last_hit_ms: u64
├── content_hash: u64                    ├── query_count: u32
├── url_hash / domain_hash               └── miss_count: u32
├── total_queries / total_feedback
└── latency_samples: Vec<u64>
```

### 1.4 Fält-livscykel

```
┌──────────────────────────────────────────────────────────────────────┐
│                    FIELD LIFECYCLE                                    │
│                                                                      │
│  Request ──► get_or_build_field_with_variant()                       │
│                    │                                                  │
│                    ├─ 1. RAM Cache (1024 entries, LRU)               │
│                    │     Hit + content_hash match ──► return          │
│                    │     Hit + content_hash diff  ──► migrate learn   │
│                    │                                                  │
│                    ├─ 2. SQLite (persistent disk)                     │
│                    │     Found ──► content hash validate ──► return   │
│                    │                                                  │
│                    └─ 3. Domain Registry (warm-start)                 │
│                          Ny URL men känd domän ──► kopiera priors     │
│                          Helt ny ──► bygg från scratch                │
│                                                                      │
│  Efter propagation ──► save_field() ──► RAM + SQLite                 │
│  Var 60:e sek ──► checkpoint() ──► flush allt                        │
│  Vid startup ──► restore() ──► ladda allt + domain profiles          │
└──────────────────────────────────────────────────────────────────────┘
```

### 1.5 Fält-byggnad: `from_semantic_tree`

Stegen vid byggnad av ett nytt ResonanceField:

1. **Platta ut trädet** — Rekursiv walk av SemanticNode-trädet.
   Varje nod får en `ResonanceState` med:
   - `text_hv` = `Hypervector::from_text_ngrams(label)` (2048-bit HDC)
   - `role` = nodens semantiska roll (heading, button, link, etc.)
   - `depth` = djup i DOM-trädet
   - Alla amplituder = 0.0, kausal memory = zero-vektor

2. **Bygg relationer** — `parent_map` och `children_map` skapas.
   Max 10 000 noder (skydd mot enorma DOM:ar).

3. **Samla value-data** — action/href/value per nod (`node_values`).
   Dessa används i BM25F-approximation (value vägs dubbelt).

4. **Hierarkisk HDC** — Föräldra-noder blandas med barnens HV:er:
   80% egen text + 20% barn-kontext (bundle av max 8 barns HV:er).
   Detta ger strukturell kontext i vektorn.

5. **Domain warm-start** — Om domänen redan finns i `DOMAIN_REGISTRY`,
   kopieras `propagation_stats` och `concept_memory` som priors.
   Ny URL på BBC.com ärver alltså vad vi lärt oss från andra BBC-sidor.
