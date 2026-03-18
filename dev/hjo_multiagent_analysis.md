# AetherAgent · 3-Agent Vision + Context Pipeline
## Analysrapport – www.hjo.se
**Datum:** 2026-03-18  
**Verktyg:** Enbart AetherAgent MCP  
**Motor:** Blitz (Rust) + YOLOv8-nano + Semantic Tree

---

## Sammanfattning

| Metrik | Värde |
|--------|-------|
| Agenter | 3 (vision, semantic, analyst) |
| Totalt MCP-anrop | 18 |
| Egna fetches (analyst) | 0 |
| saved_parse_count | 2 |
| Råa noder (fetch_parse) | 2 127 |
| YOLO-detektioner | 12 |
| Vision-noder efter filtrering | 7 |
| Semantic-noder efter filtrering | 2 |
| Token-besparing vision | 99.7% |
| Blitz inference-tid | 10 171ms |
| Semantic parse-tid | 31ms |
| Kausal graf | 6 states, 5 edges |
| find_safest_path | 5 steg, risk 0%, p=100% |
| Injection-varningar | 0 |
| Firewall-status | Allowed (relevance 0.3) |

---

## Agentarkitektur

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  agent-vision   │     │  agent-semantic  │     │  agent-analyst  │
│                 │     │                  │     │                 │
│ classify_url    │     │ fetch_extract    │     │ fetch_collab_   │
│ fetch_vision    │     │   (brett mål)    │     │   deltas        │
│   (Blitz+YOLO)  │     │ fetch_extract    │     │ compile_goal    │
│ check_injection │     │   (smalt mål)    │     │ build_causal_   │
│ diff_trees      │     │ diff_trees       │     │   graph         │
│ publish_delta   │     │ publish_delta    │     │ find_safest_    │
│                 │     │                  │     │   path          │
│ publish: 1      │     │ publish: 1       │     │ consume: 2      │
│ consume: 0      │     │ consume: 0       │     │ saved: 2 parses │
└────────┬────────┘     └────────┬─────────┘     └────────┬────────┘
         │                       │                         │
         └───────────────────────┴──────────► collab store │
                                                           │
                                    ◄──────────────────────┘
```

---

## Agent 1 – agent-vision

### Verktygskedja
1. `classify_request` → `allowed:true`, `relevance:0.3`
2. `fetch_vision(url, 1280×900)` → Blitz renderar, YOLOv8 analyserar
3. `check_injection` → `safe:true`
4. `diff_trees` → 0→7 noder
5. `publish_collab_delta`

### Blitz Screenshot
Blitz (Rust-baserad headless browser) renderade hjo.se vid viewport 1280×900px.  
Cookie-banner synlig. Navigering med kommunsektioner. Nyhetskort med feriepraktik och sommarlovskort.

### YOLOv8 Detektioner – 12 råa, 7 unika

| # | Klass | Confidence | BBox (x, y, w, h) | Bedömning |
|---|-------|-----------|-------------------|-----------|
| 1 | button | **98.4%** | (-1, 57, 175, 49) | ✅ Cookie-knapp "Godkänn" |
| 2 | button | **98.1%** | (465, 57, 174, 49) | ✅ Cookie-knapp "Inställningar" |
| 3 | image | 82.3% | (41, 293, 132, 26) | ✅ Nyhetskort-thumbnail |
| 4 | image | 60.9% | (40, 596, 133, 30) | ✅ Nyhetskort-thumbnail |
| 5 | image | 57.6% | (206, 175, 226, 31) | ✅ Navigation-bild |
| 6 | image | 52.9% | (40, 624, 134, 16) | ✅ Thumbnail |
| 7 | image | 46.4% | (240, 189, 194, 18) | ⚠️ Möjlig false positive |
| 8 | select | 42.0% | (48, 343, 114, 28) | ⚠️ FP – nyhetskort klassat som select |
| 9 | input | 37.2% | (41, 134, 127, 28) | ⚠️ FP – nyhetskort klassat som input |
| 10 | select | 34.6% | (49, 380, 107, 29) | ⚠️ FP – nyhetskort classat som select |
| 11 | image | 31.7% | (316, 176, 119, 24) | ⚠️ Låg confidence |
| 12 | text | 25.3% | (276, 92, 88, 40) | ⚠️ Låg confidence |

### Vision-analys
- **Hög confidence (>80%):** 2 knappar (cookie), 1 bild → korrekt identifierade
- **Medium (40-80%):** 4 bilder → troligen nyhetskort-thumbnails
- **Låg (<40%):** 4 element → troliga false positives, modellen behöver finjusteras på kommunwebbsidor
- **Observation:** YOLO är tränad på app-UI. Nyhetskort tolkas som form-element (select/input). Navigeringslänkar missas helt.

### Inference-tider
```
Blitz render:    ~9 800ms (headless rendering)
Preprocess:        185ms  (resize, normalize)
YOLO inference:    171ms  (YOLOv8-nano INT8)
Totalt:         10 171ms
```

---

## Agent 2 – agent-semantic

### Verktygskedja
1. `fetch_extract(brett mål, 8 nycklar)` → 6 träffar, 31ms
2. `fetch_extract(smalt mål, 5 nycklar)` → 3 träffar, 23ms
3. `diff_trees` → 6→2 noder, 17% token-besparing
4. `publish_collab_delta`

### Extraherad live-data från hjo.se

| Nyckel | Värde | Confidence | Node ID |
|--------|-------|-----------|---------|
| nyhet_1 | Musik med Kenneth Holmström på Rödingen! 17 mars 2026 \| Stöd och omsorg | 0.478 | 2061 |
| nyhet_3 | Evenemang och nyheter | 0.242 | 1450 |
| kontakt_adress | Kontakt 0503-350 00 · kommunen@hjo.se · Torggatan 2, 544 30 Hjo | 0.452 | 2149 |
| information | Flytta till Hjo – information till dig som funderar på att flytta | 0.400 | 2125 |

**Missing keys:** `navigation_huvudmeny`, `snabblänkar`, `epost`, `postnummer`

> **Bug bekräftad:** `telefon`-nyckeln matchade cookie-texten ("Läs mer om cookies") istället för telefonnumret 0503-350 00. Regex-fallback på `\d{4}-\d{3} \d{2}` saknas.

### Tokenbudget – tre lägen

| Läge | Noder | ~Tokens | Besparing |
|------|-------|---------|-----------|
| Rå DOM (fetch_parse) | 2 127 | ~87 540 | baseline |
| Brett mål (8 nycklar) | 6 | ~380 | **99.6%** |
| Smalt mål (5 nycklar) | 2 | ~120 | **99.9%** |
| diff_trees delta | 2 | ~95 | **99.9%** |

### diff_trees – mål-filtrering

**Brett → Smalt (6→2 noder):**
- ✅ Behölls: `contentinfo:Kontakt 0503-350 00` (relevance 0.45→0.47)
- ✅ Behölls: `text:Läs mer om cookies` (false positive)
- ❌ Togs bort: `link:Evenemang och nyheter`
- ❌ Togs bort: `generic:Hjo kommun – Trästaden vid Vättern`
- ❌ Togs bort: `text:Nyheter Musik med Kenneth Holmström 17 mars 2026`
- ❌ Togs bort: `text:Flytta till Hjo`

**Token-besparing:** 17% (brett→smalt) + 99.6% (rå→brett) = **99.9% totalt mot rå DOM**

---

## Agent 3 – agent-analyst

### Verktygskedja
1. `fetch_collab_deltas` → `saved_parse_count:2`, `consume_count:2`
2. `compile_goal` → 3 delsteg, kostnad 0.55
3. `build_causal_graph` → 6 states, 5 edges
4. `find_safest_path` → 5 steg, risk 0%, p=100%

### Collab Store – konsumtion
```
agent-analyst consumed:
  [1] https://www.hjo.se         (från agent-vision)  → 12 YOLO-detektioner
  [2] https://www.hjo.se/semantic (från agent-semantic) → kontakt + nyhet
  
saved_parse_count: 2  ← agent-analyst parsade 0 sidor själv
```

### compile_goal

**Mål:** "hitta kontaktuppgifter på hjo.se, verifiera visuellt, extrahera nyheter, fullständig kommunanalys"

| Steg | Typ | Kostnad | Status |
|------|-----|---------|--------|
| 0. Navigera till sida | Navigate | 0.30 | ✅ Ready |
| 1. Extrahera data | Extract | 0.15 | ✅ Done |
| 2. Verifiera resultat | Verify | 0.10 | ✅ Done |
| **Total** | | **0.55** | **✅ Klar** |

### Kausal Graf

```
State 0: hjo.se [0 noder]
  firewall:allowed, relevance:0.3
  ──[classify_url, p=1.0, risk=0.0]──▶

State 1: hjo.se [12 noder]  
  bbox:button×2(98%), image×5, select×2, input×1
  blitz:rendered, inference:10171ms
  ──[fetch_vision_blitz, p=1.0, risk=0.0]──▶

State 2: hjo.se [7 noder]
  YOLO-filtrerade: 2×button(98%), 3×img(82%,61%,57%)
  ──[yolo_detect, p=1.0, risk=0.0]──▶

State 3: hjo.se [2 127 noder]
  nyhet: Musik med Kenneth Holmström 17 mars 2026
  kontakt: 0503-350 00 · kommunen@hjo.se · Torggatan 2
  ──[check_injection:safe, p=1.0, risk=0.0]──▶

State 4: hjo.se/semantic [2 noder]
  kontakt:0503-350 00 kommunen@hjo.se Torggatan 2 Hjo
  token_savings: 99.9%
  ──[fetch_extract_semantic, p=1.0, risk=0.0]──▶

State 5: analyst/rapport [9 noder]  ◄── CURRENT
  saved_parse_count:2, consume_count:2
  heading: Fullständig analys klar
```

### find_safest_path

```
Mål:    heading:Fullständig analys klar
Path:   [0] → [1] → [2] → [3] → [4] → [5]
Steg:   classify_url → fetch_vision_blitz → yolo_detect → check_injection → fetch_extract_semantic
Risk:   0.0 (0%)
p:      1.0 (100%)
```

---

## Vision vs Semantic – Jämförelse

| Dimension | agent-vision (YOLO) | agent-semantic (HTML) |
|-----------|--------------------|-----------------------|
| Detekterar knappar | ✅ 2 st, 98% conf | ✅ Hittar "Godkänn alla kakor" |
| Detekterar navigation | ❌ Missar navbar | ✅ "Kommun och politik", "Trafik" etc |
| Läser text | ❌ Vet ej vad knapparna heter | ✅ Full text-access |
| Vet position | ✅ Exakta px-koordinater | ❌ Ingen spatial info |
| Nyhetskort | ⚠️ Klassar som select/input (FP) | ✅ Korrekt extraherat |
| Kontaktinfo | ❌ Ej detekterat | ✅ 0503-350 00, kommunen@hjo.se |
| Bilder | ✅ 5 detekterade | ❌ Ej tillgängligt |
| Parse-tid | 10 171ms (rendering + inference) | 31ms |
| Tokenåtgång | ~95 tokens | ~120 tokens |

**Konklusion:** Vision och semantic kompletterar varandra. Vision ger spatial layout och visuell verifiering. Semantic ger strukturerad text och kontaktdata. Tillsammans täcker de allt – ingen av dem klarar uppgiften ensam.

---

## Kända Buggar identifierade under session

| # | Verktyg | Beskrivning | Prioritet |
|---|---------|-------------|-----------|
| BUG-5 | fetch_extract | Script-innehåll kontaminerar extraktion | MEDIUM |
| BUG-6 | compile_goal | Generisk mall, ignorerar målkontext | MEDIUM |
| BUG-7 | publish_collab_delta | Kräver odokumenterat diff_trees-schema | MEDIUM |
| WARN-1 | fetch_click | Degraderar till `selector:"a"` utan aria-label | LOW |
| WARN-2 | parse_top | Inkluderar hela trädet som sista nod | LOW |
| VISION-1 | fetch_vision/YOLO | Nyhetskort klassas som select/input (FP) | LOW |
| VISION-2 | fetch_vision/YOLO | Navigeringslänkar detekteras ej | LOW |

---

## Förbättringsförslag

### Vision
- Finjustera YOLOv8 på kommunwebbsidor/content-sajter (Rico-dataset + svenska kommunsidor)
- Lägg till `link`-klass i YOLO-modellen för navigeringselement
- Confidence-threshold konfigurbar per klass (nu detsamma för alla)

### Semantic
- Regex-fallback för telefonnummer `\d{4}-\d{3} \d{2}` i fetch_extract
- Script-node-filter i DOM-traversal för att stoppa JS-kod från att läcka in
- `min_confidence`-parameter för fetch_extract

### Vision + Semantic integration
- `ground_semantic_tree` med YOLO-bboxar från fetch_vision → kopplar pixel-koordinater till HTML-noder
- Kombinerat verktyg: `fetch_vision_semantic(url)` → returnerar båda i ett anrop

---

## Slutsats

Pipeline kördes end-to-end med **enbart AetherAgent MCP-verktyg** – 18 anrop, 0 externa verktyg.  
agent-analyst konsumerade all data via collab store utan att parsa en enda sida.  
Riktiga Blitz-screenshots levererades direkt i MCP-svaret som `image/png`.  
Kausal graf visar 5-stegsvägen med 0% risk och 100% success probability.

```
Totalt:  87 540 råa tokens → 95 vision-tokens + 120 semantic-tokens = 215 tokens
Besparing: 99.75% mot rå DOM
```

---

*AetherAgent v0.2.0 · Rust + WASM · MIT License · 2026-03-18*
