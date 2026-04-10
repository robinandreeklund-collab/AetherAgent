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

---

## 2. Scoring Pipeline — Fas 1: Initial Resonans

Varje nod får en amplitud beräknad från **7 oberoende signaler** som kombineras.
Hela scoringen sker i `propagate_inner()`.

### 2.1 Pipeline-diagram

```
┌─────────────────────────────────────────────────────────────────────────┐
│              FAS 1: INITIAL RESONANS (per nod)                         │
│                                                                        │
│  ┌─────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────┐           │
│  │  BM25   │  │   HDC    │  │   Roll   │  │   Concept    │           │
│  │ keyword │  │  n-gram  │  │ priority │  │   memory     │           │
│  │ × 0.75  │  │  × 0.20  │  │  × 0.05  │  │  (learned)   │           │
│  └────┬────┘  └────┬─────┘  └────┬─────┘  └──────┬───────┘           │
│       │            │             │                │                    │
│       └────────────┴──────┬──────┴────────────────┘                   │
│                           ▼                                            │
│                    base_resonance                                      │
│                           │                                            │
│                    ┌──────┴──────┐                                     │
│                    │  + kausal   │ (additivt, temporal decay)          │
│                    │  + answer   │ (typ-matchning, 0.0–0.25)          │
│                    │  × CombMNZ  │ (konsensus-bonus 1.0–1.45)         │
│                    │  × template │ (×1.2 om igenkänd struktur)        │
│                    │  × shape    │ (×1.0–1.9 baserat på form)         │
│                    │  × zone     │ (×0.5–1.0 baserat på position)     │
│                    │  × meta     │ (×0.15–1.0 brus-penalty)           │
│                    │  × suppress │ (×0.15–1.0 inlärd penalty)         │
│                    └──────┬──────┘                                     │
│                           ▼                                            │
│                    final amplitude                                     │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Signal 1: BM25 (vikt 0.75)

**Fil:** `src/scoring/tfidf.rs`
**Formel:** Okapi BM25 med k1=1.2, b=0.75

BM25 är den tyngsta signalen. Per nod byggs ett "dokument" av:
- `label` (nodens synliga text)
- `value` × 2 (action/href/value — dubbelvikt via BM25F-approximation)

```
dokument = "{label} {value} {value}"    ← BM25F: value räknas dubbelt
```

**Cascade-optimering:** Av alla noder i fältet (kan vara 5000+),
väljs först topp-200 via BM25 som kandidater. Resten scoreas inte alls.
Undantag:
- Noder med `hit_count > 0` (kausalt minne) — alltid med
- Noder med roll `heading|article|text|paragraph` — strukturellt bypass

Om cascade-mängden > 300 noder, MCCFR-sampling: behåll de 300 bästa
viktade efter BM25 + roll-prioritet + kausalt minne.

### 2.3 Signal 2: HDC Text-likhet (vikt 0.20)

**Fil:** `src/scoring/hdc.rs`
**Dimension:** 2048-bit bitvektorer

Hyperdimensional Computing ger strukturell n-gram-likhet:
```
goal_hv  = Hypervector::from_text_ngrams(goal)
text_hv  = nodens förberäknade HV (80% egen text + 20% barns HV:er)
raw_sim  = hamming-baserad cosine-likhet (-1.0 till 1.0)
hdc_norm = ((raw_sim + 1) / 2)²     ← kvadrerad för att trycka ner svaga
```

HDC fångar delvis-matchningar som BM25 missar:
- "nyheter" vs "nyhetssida" — delad n-gram-sekvens
- Liknande ordstruktur utan exakt ordmatchning

**Hierarkisk HDC:** Vid fältbyggnad blandas barnens HV:er in i
föräldern: `parent_hv = bundle(own⁴, children_bundle¹)`.
Detta ger t.ex. en `<heading>` viss kontext från sina barn-noder.

### 2.4 Signal 3: Roll-prioritet (vikt 0.05)

**Funktion:** `role_priority(role) → 0.0–1.0`

Statisk tabell — ingen semantik, bara strukturell rollvikt:

| Roll | Prioritet | Motivering |
|------|-----------|------------|
| price, data, cell | 1.0 | Nästan alltid relevant data |
| heading, text, paragraph | 0.9 | Innehållsnoder |
| button, cta, product_card | 0.85 | Interaktionsnoder |
| link, listitem, row | 0.7 | Strukturella noder |
| img, table | 0.6 | Container/media |
| textbox, searchbox, select | 0.5 | Form-element |
| generic | 0.4 | Okänd roll |
| navigation, complementary | 0.2 | Boilerplate |

Låg vikt (0.05) — rollen är inte mål-beroende och ska inte dominera.

### 2.5 Signal 4: Concept Memory (inlärt)

**Fält:** `concept_memory: HashMap<String, HvData>`

Field-level minne som aggregerar HV:er per mål-koncept:
```
Om goal = "latest news headlines"
→ tokens: ["latest", "news", "headlines"]
→ Kolla om concept_memory["news"] finns
→ similarity(nod.text_hv, concept_memory["news"])
→ max_boost = norm² × 0.15
```

Concept memory uppdateras via `feedback()` — framgångsrika noders
text-HV:er bundlas in per goal-token. Max 256 entries (FIFO eviction).

### 2.6 Signal 5: Kausal Boost (additivt)

```
kausal_boost = 0.0   (om hit_count == 0)

Om hit_count > 0:
  raw = similarity(nod.causal_memory, goal_hv)
  norm = ((raw + 1) / 2).clamp(0, 1)
  decay = e^(-0.00115 × sekunder_sedan_senaste_hit)
  kausal_boost = norm × 0.3 × decay
```

**Viktigt:** Kausalt boost är **additivt** (inte multiplikativt).
En nod med BM25=0.75 + kausal=0.2 → 0.95.
Temporal decay: halvering var 10:e minut.

### 2.7 Signal 6: Answer-Type Boost (0.0–0.25)

`answer_type_boost(goal, label)` matchar frågetyp mot svarsmönster:

| Frågetyp | Trigger-ord | Svarsmönster | Boost |
|----------|-------------|--------------|-------|
| Pris | price, cost, pris, kr, fee | $, £, €, kr i label | +0.25 |
| Population | population, invånare, antal | Siffra ≥ 4 tecken | +0.20 |
| Datum | date, when, datum, year, år | 200x, 201x, 202x | +0.15 |
| Procent | rate, percent, ränta | % i label | +0.25 |

### 2.8 Multiplikatorer

Efter `base_resonance + kausal_boost + answer_type_boost`:

**CombMNZ** — Belönar konsensus mellan signaler:
```
signal_count = antal av [BM25>0.01, HDC>0.01, role>0.1, concept>0.001] som är sanna
combmnz = 1.0 + (signal_count - 1) × 0.15   (om ≥ 2 signaler)
```
En nod som matchar på 3 signaler: ×1.30. Alla 4: ×1.45.

**Template Match** — Om sidan har samma struktur (top-20 rollsekvens)
som förra gången och noden har kausalt minne: ×1.2.

**Answer Shape** — `answer_shape_score(label, role, siblings)`:
- Innehåller siffror: +0.3
- Kort text (<50 tecken): +0.2
- Enhetsmarkörer ($, %, kr, kg): +0.15
- Strukturerad kontext (tabell/lista med ≥2 syskon): +0.15
- Data-roll (price, data, cell): +0.1
- Medellång text (20–200 tecken): +0.1

**Zone Penalty** — Straffar boilerplate-positioner:
- navigation, complementary, contentinfo, banner: ×0.5
- Top-level generic (depth ≤ 1): ×0.7

**Metadata Penalty** — Straffar brus-mönster:
- Serialiserad state (__APOLLO_STATE__, __NEXT_DATA__): ×0.15
- HN/Reddit metadata ("points by...hours ago"): ×0.4
- Error messages ("uh oh! there was an error"): ×0.5
- Wikipedia fotnoter ("^ ...Retrieved 20"): ×0.3
- Cookie consent: ×0.2

**Site-Name Penalty** — Om >60% av nodens ord matchar site-name: ×0.7.

**Suppression** — Inlärd penalty (efter ≥3 queries):
```
success_ratio = hit_count / query_count
Om success_ratio < 0.25:
  factor = 0.15 + 0.85 × (success_ratio / 0.25)
  amplitude × factor
```
En nod som dykt upp 10 gånger men aldrig markerats som korrekt → ×0.15.

### 2.9 Kontextuell Page-Profile Justering

`page_profile()` analyserar sidans rollfördelning:
- Hög nav-densitet (>30%): content-noder ×1.1
- Hög tabell-densitet (>15%): cell/data/row ×1.15

---

## 3. Wave Propagation — Fas 2: Chebyshev Spectral Filter

Efter initial scoring sprids signalen genom DOM-trädets kanter
via ett Chebyshev polynomfilter på graf-Laplacianen.

### 3.1 Varför Chebyshev?

Iterativ propagation (vågspridning i loopar) har problem:
- Over-smoothing: alla noder konvergerar mot samma amplitud
- Osäker konvergens: hur många iterationer räcker?
- Ingen garanti för optimal spektral respons

Chebyshev löser detta:
- **Bevisbart optimal** low-pass filter
- **Fixad O(K×|E|)** med K=2-4 (adaptivt)
- **Ingen over-smoothing** (bevarar skarpa toppar)
- **PPR-restart** matematiskt integrerat

### 3.2 Hur det fungerar

```
┌─────────────────────────────────────────────────────────────────────┐
│            CHEBYSHEV SPECTRAL FILTER                                │
│                                                                     │
│   Seed signal (Fas 1 amplituder)                                    │
│         │                                                           │
│         ▼                                                           │
│   T₀ = seed signal (identitet)              θ₀ = 0.50              │
│   T₁ = (2/λ_max)·L̃·seed - seed            θ₁ = 0.30              │
│   T₂ = 2·L̃_scaled·T₁ - T₀                 θ₂ = 0.12              │
│   T₃ = 2·L̃_scaled·T₂ - T₁                 θ₃ = 0.05              │
│   T₄ = 2·L̃_scaled·T₃ - T₂                 θ₄ = 0.03              │
│         │                                                           │
│         ▼                                                           │
│   output = θ₀·T₀ + θ₁·T₁ + θ₂·T₂ + θ₃·T₃ + θ₄·T₄               │
│         │                                                           │
│         ▼                                                           │
│   PPR restart: (1-α)·output + α·BM25_seed    α = 0.15             │
│         │                                                           │
│         ▼                                                           │
│   propagation_boost = output - seed × θ₀                           │
│   final = fas1_amplitude + propagation_boost                        │
└─────────────────────────────────────────────────────────────────────┘
```

**θ-koefficienter** = low-pass filter biased mot lokal signal:
- θ₀=0.50: direkt signal (egen nod)
- θ₁=0.30: 1-hop grannar (förälder + barn)
- θ₂=0.12: 2-hop (farförälder, barnbarn, syskon)
- θ₃=0.05: 3-hop
- θ₄=0.03: 4-hop

### 3.3 Adaptive K (Chebyshev-ordning)

K anpassas efter DOM-storlek:

| DOM-storlek | Max djup | K | Motivering |
|-------------|----------|---|------------|
| < 50 noder | - | 2 | Liten DOM, 2-hop räcker |
| < 200 noder | - | 3 | Medel, 3-hop fångar struktur |
| > 200 noder | > 15 | 4 | Djup/stor DOM, full 4-hop |
| övrigt | - | 3 | Standard |

**Storleksbegränsning:** För DOM:ar > 500 noder begränsas Chebyshev
till topp-500 noder sorterade efter Fas 1-amplitud. Resten behåller
sin Fas 1-amplitud utan propagation-boost.

### 3.4 Laplacian-multiply: Kantvikter

Varje parent→child-kant har **inlärda vikter**:

```
Normalized Laplacian:
  L̃·x vid nod i = x_i - Σ_j∈grannar w_ij·x_j / √(d_i·d_j)

Kantvikt (symmetrisk):
  w_sym = √(w_down × w_up)

  w_down = learned_weight("heading:down:news+latest", heuristic_down)
  w_up   = learned_weight("text:up:news+latest", heuristic_up)
```

**Heuristiska priors (cold-start):**

| Roll | Down-vikt | Up-vikt | Tolkning |
|------|-----------|---------|----------|
| heading, table, row, list | 1.2 | — | Sprider signal nedåt starkt |
| text, paragraph, heading | — | 1.1 | Sprider signal uppåt |
| price, data, cell | 0.7 | 1.3 | Data bubbles up, inte ner |
| navigation, complementary | 0.3 | 0.3 | Låg spridning åt alla håll |

**Inlärda vikter:** DCFR (Discounted CFR) regret matching.
Varje riktning+roll+mål-kluster lagras som `(cum_positive, cum_negative)`.
```
signal = max(cum_pos, 0) / (max(cum_pos, 0) + |cum_neg| + 1)
weight = heuristic × (0.5 + signal × 1.3)
```

### 3.5 RBP — Regret-Based Pruning

Hela subträd kan pruneas om förälderns nedåt-propagation
konsekvent misslyckas:

```
Om total_queries ≥ 5
  OCH learned_down_weight < 0.3
  OCH roll ∈ {navigation, complementary, contentinfo, banner}
→ skippa hela subtreedet i Laplacian-multiply
```

### 3.6 Adaptive Fan-Out

Inte alla barn propageras — storlek-beroende begränsning:

```
fan_out(children_count):
  0 barn → 0
  ≤ 8 barn → alla
  > 8 barn → 4 + ln(N) × 8, max N
```

### 3.7 Post-Propagation: Multi-hop Micro-boost

Efter Chebyshev körs två extra steg:

**Value-matched micro-boost:**
```
Om en nod har amplitude > 0.3 OCH har value-data (href/action):
  → Syskon: +15% av nodens amplitud
  → Förälders syskon (2-hop): +8% av nodens amplitud
```

**Sibling pattern recognition:**
```
Om en nod har amplitude > 0.4 OCH finns i grupp med ≥3 syskon:
  → Syskon med SAMMA roll: +10% av matched nodens amplitud
```
Hanterar produktlistor, artikellistor, tabellrader — om ett
listelement matchar, får strukturellt identiska syskon en boost.

### 3.8 Komplett Propagation-flöde (visuellt)

```
  Fas 1 amplituder                    Chebyshev K=4
        │                                  │
        ▼                                  ▼
  ┌──────────┐    ┌──────────────┐   ┌──────────────┐
  │ heading  │    │  heading     │   │  heading     │
  │ amp=0.8  │───>│  +0.12 prop  │──>│  amp=0.92    │
  └──────────┘    └──────────────┘   └──────────────┘
        │                                  │
        │ down w=1.2                       │
        ▼                                  ▼
  ┌──────────┐    ┌──────────────┐   ┌──────────────┐
  │  text    │    │   text       │   │  text        │
  │ amp=0.3  │───>│  +0.24 prop  │──>│  amp=0.54    │
  └──────────┘    └──────────────┘   └──────────────┘
        │                                  │
        │ sibling                          │ value-match
        ▼                                  ▼
  ┌──────────┐    ┌──────────────┐   ┌──────────────┐
  │  link    │    │   link       │   │  link        │
  │ amp=0.0  │───>│  +0.08 sib   │──>│  amp=0.08    │
  └──────────┘    └──────────────┘   └──────────────┘
```
