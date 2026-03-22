# Fas 17 AetherDOM — Djupanalys & Luckor
**Datum:** 2026-03-21
**Syfte:** Identifiera allt som saknas, missats eller underskattats i Fas 17-planen

---

## 1. KRITISKA LUCKOR I PLANEN

### 1.1 Hydration-formaten har ändrats — planen är utdaterad

Planen utgår från att `__NEXT_DATA__` och `window.__NUXT__` är enkla JSON-blobbar. Verkligheten 2026:

| Ramverk | Planens antagande | Verklighet 2026 |
|---------|-------------------|-----------------|
| **Next.js App Router** | `__NEXT_DATA__` JSON | **Finns inte!** Använder `self.__next_f.push([1, "..."])` — React Flight Protocol, streamade chunks, custom serialisering |
| **Nuxt 3/4** | `window.__NUXT__` JSON | Använder **devalue** (inte JSON!) — stöder Date, BigInt, Map, Set, cykliska refs |
| **SvelteKit** | Enkel JSON | Också **devalue** — plus `__sveltekit_*` namespace med `teleported.set()` |
| **Angular 19** | `<script id="ng-state">` | **Incremental hydration** — deferred content förblir dehydrerat, HTML-kommentarer som `ngh`-attribut |
| **Qwik** | Hydration | **Resumability ≠ hydration!** — State i `<script type="qwik/json">`, event handlers som QRL-attribut |
| **Remix v2** | `window.__remixContext` | Merged till **React Router v7** — kontextformat möjligen ändrat till `__reactRouterContext` |

**Saknas i planen:**
- [ ] **devalue-parser** — Nuxt och SvelteKit använder inte JSON. Behöver custom deserializer
- [ ] **React Flight Protocol-parser** — Next.js App Router kräver parsing av streamade RSC-chunks
- [ ] **Qwik-resumability** — helt annorlunda modell, inte hydration. QRL-parsing behövs
- [ ] **Angular incremental hydration** — `ngh`-attribut + HTML-kommentarer, inte bara `<script>`-block
- [ ] **Astro Island-detektion** — `client:load`, `client:visible` etc. direktiv i HTML

### 1.2 Boa-versionen stämmer men `#[boa_class]` saknar DOM-prejudikat

**Nuläge:** Boa 0.21, ECMAScript 94.12% conformance. `#[boa_class]`-makrot finns.

**Ingen har byggt DOM med Boa förut.** Vi är first-movers — inget existerande projekt att kopiera från.

**Saknas i planen:**
- [ ] **GC-strategi** — SlotMap kan INTE direkt derivera `Trace` från boa_gc (extern typ). Behöver wrapper med manuell `Trace`-impl eller hålla DOM-arenan utanför GC
- [ ] **Minnesägande** — Planen specificerar inte vem som äger arenan: Rust eller Boa GC? Rekommendation: **Rust äger arenan, JS-sidan får bara NodeKey-handles**
- [ ] **Context-delning mellan eval-anrop** — Nuvarande `eval_js()` skapar ny `Context::default()` varje gång. Planens DOM Bridge kräver persistent context med registrerade globaler
- [ ] **Timeout/minnesbegränsning** — Boa har ingen inbyggd timeout. Infinite loops i användar-JS hänger hela processen
- [ ] **Boa GC-rewrite pågår** — Teamet medger att GC:n är "pushed to its limits". Risk för breaking changes i 0.22+

### 1.3 SlotMap-versionen i planen är fel

Planen refererar generiskt till "slotmap". Aktuellt:
- **slotmap 1.0.7** (inte 0.4 som planen antyder)
- Tre varianter: `SlotMap` (snabbast insert), `HopSlotMap` (bäst general-purpose), `DenseSlotMap` (bäst iteration)

**Saknas i planen:**
- [ ] **Val av SlotMap-variant** — `DenseSlotMap` är bäst för DOM-traversering (kontiguöst minne, snabbast iteration)
- [ ] **`generational-arena-dom`** — existerande crate specifikt för html5ever DOM med generational arenas. Utvärderades inte

### 1.4 servo/selectors är instabilt

- Senaste version: 0.26.0, men **synkning till crates.io är manuell och oregelbunden**
- 13 breaking changes bara under 2016
- Fork existerar: `parcel_selectors` (v0.26.1) från lightningcss

**Saknas i planen:**
- [ ] **Alternativ till servo/selectors**: `scraper`-craten (13.5M+ downloads) wrappar redan html5ever + selectors. Kan vara lättare att använda som referens
- [ ] **Minimalt selektors-stöd** som alternativ — handskriven matchning för ID, klass, tag, attribut (täcker 95% av verklig användning utan tungt beroende)

---

## 2. ARKITEKTURELLA LUCKOR

### 2.1 TreeSink-implementationen underskattas

Planen nämner "ersätt RcDom med SlotMap" men specificerar inte HUR html5ever-parsern ska mata arenan:

**Option A: Custom TreeSink** (bäst men svårast)
- Kräver impl av ~20 trait-metoder
- Exempel: `create_element()`, `append()`, `append_before_sibling()`, `remove_from_parent()`, `reparent_children()`, `mark_script_already_started()`, etc.
- **Uppskattning: 200-300 rader kod, 3-5 dagar**

**Option B: Post-process RcDom → Arena** (enklast)
- Parse till RcDom, konvertera i en pass
- 2x minnesanvändning temporärt
- **Uppskattning: 50-80 rader kod, 1 dag**

**Option C: Hybrid** (rekommenderad)
- Behåll RcDom för parsing, konvertera direkt efter
- Fas-vis migration: parser.rs oförändrad först

**Saknas i planen:**
- [ ] Explicit val mellan Option A/B/C
- [ ] Minnesbudget per sida (RcDom ~7.2KB overhead/300 noder, Arena ~2.4KB)

### 2.2 Parser & Semantic-modulerna har bara 4 tester totalt

**Kritisk testlucka upptäckt:**

| Modul | Antal tester | Risk |
|-------|-------------|------|
| `parser.rs` | **2 tester** | KRITISK — ingen felhantering, UTF-8, ARIA-fallback |
| `semantic.rs` | **2 tester** | KRITISK — ingen relevansscoring, rolldetektering |
| `streaming.rs` | 6 tester | Medel |
| `stream_engine.rs` | 9 tester | Låg |
| Integration | 84 tester | Bra täckning |

**Saknas i planen:**
- [ ] Testplan innan refactoring — man kan inte migrera parser.rs med bara 2 tester som skyddsnät
- [ ] Minst 20+ parser-tester behövs FÖRE Arena-migrering
- [ ] Minst 10+ semantic-tester behövs FÖRE Arena-migrering

### 2.3 84 integrationstester har låst JSON-kontraktet

Alla 84 integrationstester validerar detta JSON-format:

```json
{
  "nodes": [{ "id", "role", "label", "relevance", "children", "html_id", "action" }],
  "injection_warnings": [],
  "parse_time_ms": number,
  "xhr_intercepted": number,
  "xhr_blocked": number
}
```

**Saknas i planen:**
- [ ] Garanti att SemanticTree-output INTE ändras — Arena DOM måste vara en intern implementation detail
- [ ] types.rs (SemanticNode, SemanticTree) får INTE modifieras

### 2.4 Saknade integrationstester

Dessa pipelines har **0 tester**:

| Pipeline | Tester |
|----------|--------|
| `parse_to_semantic_tree()` → `compile_goal()` → `execute_plan()` | 0 |
| `parse_to_semantic_tree()` → `build_causal_graph()` | 0 |
| `parse_to_semantic_tree()` → `ground_tree()` | 0 |
| `parse_to_semantic_tree()` → temporal tracking | 0 |

**Saknas i planen:**
- [ ] End-to-end-tester för alla pipelines FÖRE och EFTER migrering

---

## 3. TEKNISKA RISKER

### 3.1 Blockerare: `document.` och `window.` är BLOCKADE i sandbox

Nuvarande `js_eval.rs` rad 244-261 har en **hård blocklista**:

```rust
const FORBIDDEN: &[&str] = &[
    "document.",   // ← Blockerar ALL document-access
    "window.",     // ← Blockerar ALL window-access
    // ...
];
```

**Planen specificerar inte hur detta löses!** Om vi registrerar `document` och `window` i Boa-kontexten måste blocklistan ändras — men den finns för säkerhet (förhindra sandbox escape).

**Saknas i planen:**
- [ ] Ny säkerhetsmodell: tillåt `document.*` och `window.*` i Boa DOM Bridge men blockera farliga operationer (cookies, localStorage, eval)
- [ ] Granulär allowlist istället för blocklist
- [ ] Trust-nivå per JS-snippet (inline script vs event handler)

### 3.2 Boa-kontexten delas inte mellan eval-anrop

Nuvarande kod (js_eval.rs:238):
```rust
let mut context = Context::default();
// Ny kontext varje gång!
```

DOM Bridge kräver persistent kontext med registrerade `document`/`window`-objekt.

**Saknas i planen:**
- [ ] Livscykelhantering av Boa Context
- [ ] Hur länge lever kontexten? Per-sida? Per-session?
- [ ] Minneshantering — vad händer med GC vid lång livscykel?

### 3.3 Eval-batch delar inte variabler

`eval_js_batch()` skapar separata kontexter per snippet. Script 1 kan inte definiera en variabel som Script 2 använder.

**Saknas i planen:**
- [ ] Delad kontext för alla scripts på samma sida
- [ ] Script-exekveringsordning (DFS-ordning i DOM)
- [ ] `<script defer>` / `<script async>` hantering

### 3.4 Ingen timeout/minneshantering i Boa

En oändlig loop i användarkod hänger hela processen.

**Saknas i planen:**
- [ ] Boa `Interner`-baserad minnesgräns
- [ ] Instruktion-counter-baserad timeout (Boa stöder detta via `Context::set_can_block()` etc.)
- [ ] WASM-specifik timeout (kan inte använda threads)

### 3.5 MutationObserver-shim är mer komplex än planerat

Planen nämner "MutationObserver-shim" som en punkt. I verkligheten:
- MutationObserver kräver asynkron callback-scheduling
- Boa har ingen event loop
- Behöver egen microtask-queue
- React/Vue/Svelte använder MutationObserver för DOM-uppdateringsdetektering

**Saknas i planen:**
- [ ] Event loop / microtask-queue implementation
- [ ] `requestAnimationFrame`-shim med manuell frame-triggering
- [ ] `Promise.resolve().then()` — kräver microtask-stöd

---

## 4. KONKURRENT-UPPDATERINGAR

### 4.1 LightPanda — Korrigerad information

Planen säger "SpiderMonkey". Verklighet:
- **Språk:** Zig (inte Rust)
- **JS-motor:** V8 (inte SpiderMonkey)
- **Licens:** AGPL-3.0 (korrekt)
- **Status:** Fortfarande beta, pre-seed Q2 2025, 22K+ GitHub-stjärnor

### 4.2 Nya konkurrenter sedan planen skrevs

| Projekt | Språk | Approach | Relevant? |
|---------|-------|----------|-----------|
| **Agent-Browser** (Jan 2026) | Rust | Headless Chrome CLI för AI-agenter | Ja — direkt konkurrent |
| **very-happy-dom** | JS (Bun) | Snabbaste lightweight DOM | Nej — JS-only |
| **deno-dom** | Rust(WASM) + TS | Rust parser + TS DOM API | Delvis — liknande approach |

### 4.3 Nytt crate att utvärdera

**`generational-arena-dom`** — specifikt byggt för html5ever DOM med generational arenas. Kan ersätta handskriven SlotMap-integration.

---

## 5. SAKNADE FEATURES I DOM API-PRIORITERINGEN

### 5.1 React 19 kräver fler APIs

React 19 (aktuell version 2026) använder:
- `document.createTreeWalker()` — för effektiv DOM-traversering
- `document.createRange()` — för textmarkeringar
- `MutationObserver` — för hydration-matching
- `requestIdleCallback` — för scheduling
- `queueMicrotask` — för state-batching

**Ingen av dessa finns i planens DOM API-lista.**

### 5.2 Vue 3 kräver

- `document.createComment()` — Vue använder kommentarer som ankare
- `Node.insertBefore()` med kommentar-referens
- `template.content` (DocumentFragment från `<template>`)

**`createComment()` saknas i planen.**

### 5.3 Saknade globaler

Planen nämner `console`, `URL`, `TextEncoder` via boa_runtime. Men saknar:
- [ ] `JSON.parse()` / `JSON.stringify()` — Boa har detta, men behöver verifieras med custom typer
- [ ] `Array.from()` — React använder detta på NodeList
- [ ] `Object.assign()` / `Object.keys()` — standard men måste verifieras
- [ ] `Symbol.iterator` — för `for...of` på NodeList
- [ ] `WeakMap` / `WeakRef` — React internals

---

## 6. REVIDERAD IMPLEMENTATIONSORDNING

Baserat på analysen, reviderad prioritering:

| Prio | Komponent | Original | Reviderad | Ändring |
|------|-----------|----------|-----------|---------|
| **P-1** | **Testinfrastruktur** | *Saknades* | 2-3 dagar | **NYTT** — 30+ tester i parser/semantic FÖRE migrering |
| **P0** | Hydration extraction | 3-5 dagar | **5-8 dagar** | Uppjusterat — devalue-parser, Flight Protocol |
| **P0.5** | **Säkerhetsmodell** | *Saknades* | 2-3 dagar | **NYTT** — allowlist ersätter blocklist |
| **P1** | Arena DOM (SlotMap) | 3-4 dagar | 3-4 dagar | Oförändrad |
| **P1.5** | **Persistent Boa Context** | *Saknades* | 2-3 dagar | **NYTT** — delad kontext, timeout, minneshantering |
| **P2** | Boa DOM Bridge (25 metoder) | 5-7 dagar | **7-10 dagar** | Uppjusterat — GC-wrapper, createComment, TreeWalker |
| **P3** | Selector-matchning | 2-3 dagar | 2-3 dagar | Oförändrad (men överväg scraper istället för servo/selectors) |
| **P4** | 30 extra DOM-metoder | 3-4 dagar | 3-4 dagar | Oförändrad |
| **P5** | Progressive escalation | 2-3 dagar | 2-3 dagar | Oförändrad |
| **P6** | MutationObserver + rAF | 2-3 dagar | **5-7 dagar** | Uppjusterat — kräver microtask-queue, event loop |

**Total reviderad uppskattning: 33-48 dagar** (vs planens ~20-28 dagar)

---

## 7. REKOMMENDATIONER

### 7.1 Gör först (blockerare)

1. **Skriv 30+ tester för parser.rs och semantic.rs INNAN något ändras**
2. **Välj GC-strategi: Rust äger arenan, JS får handles** (undvik Boa GC-problem)
3. **Implementera persistent Boa Context med timeout** (instruction counter)
4. **Byt från blocklist → allowlist** i js_eval.rs

### 7.2 Ändra approach

1. **Hydration: Börja med Next.js Pages Router + Nuxt 2** (enkel JSON) — skjut devalue/Flight Protocol till P4
2. **Selektorer: Använd `scraper`-crate som referens** istället för direkt servo/selectors
3. **Arena: Använd Option C (Hybrid)** — post-process RcDom, migrera gradvis
4. **SlotMap: Använd `DenseSlotMap`** för bäst iterationsprestanda

### 7.3 Lägg till i planen

1. **Event loop / microtask-queue** — krävs för React/Vue
2. **`document.createComment()`** — krävs för Vue
3. **Timeout-mekanism** — säkerhetskritisk
4. **devalue-deserializer** — krävs för Nuxt 3 + SvelteKit
5. **React Flight Protocol-parser** — krävs för Next.js App Router
6. **End-to-end integrationstester** för alla pipelines

### 7.4 Undvik

1. **Implementera inte full MutationObserver** i första iterationen — använd synkron DOM-diffing istället
2. **Implementera inte requestAnimationFrame** — Boa har ingen event loop, stubb räcker
3. **Byt inte ut RcDom i parser.rs** förrän alla tester finns — använd post-processing

---

*Analys genomförd: 2026-03-21 · 5 parallella forskningsagenter · ~500 rader källkodsanalys*
