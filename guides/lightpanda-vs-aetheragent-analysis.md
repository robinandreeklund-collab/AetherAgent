# LightPanda vs AetherAgent — Djupanalys

## Sammanfattning

LightPanda och AetherAgent har fundamentalt olika arkitekturer och mål, men opererar i samma ekosystem: **AI-agenters webbinteraktion**. De konkurrerar inte direkt — de är komplementära. LightPanda är en **headless browser** (HTTP + DOM + JS). AetherAgent är en **semantisk perceptionslager** (HTML-analys + tillgänglighetstree + förtroendeskydd, kompilerat till WASM).

---

## 1. Vad är LightPanda?

LightPanda är en **headless browser byggd från grunden i Zig**, designad specifikt för AI-agenter, webbautomation och webbskrapning. Den är **inte** en Chromium-fork — det är en clean-room-implementation som medvetet utelämnar all visuell rendering (ingen CSS-layout, ingen bildavkodning, ingen GPU-kompositing).

**Kärntes**: "Hur skulle en browser se ut om den byggdes specifikt för maskiner?"

| Egenskap | Detalj |
|----------|--------|
| **Språk** | Zig 0.15.2 |
| **HTML-parser** | html5ever (Servos Rust-parser via C FFI) |
| **JS-motor** | Google V8 (full) |
| **DOM** | "zigdom" — custom ren Zig-implementation |
| **Nätverk** | libcurl (boringssl, nghttp2, brotli, zlib) |
| **Protokoll** | CDP (Chrome DevTools Protocol) + MCP (stdio) |
| **Licens** | AGPL-3.0 |
| **GitHub-stjärnor** | 22 000+ |
| **Status** | Beta — ~95% av sajter fungerar |

---

## 2. Vad är AetherAgent?

AetherAgent är en **LLM-native inbäddbar browser-motor i Rust**, kompilerad till WebAssembly. Den tillhandahåller ett semantiskt perceptionslager för AI-agenter med inbyggt promptinjektionsskydd.

| Egenskap | Detalj |
|----------|--------|
| **Språk** | Rust (2021 edition) |
| **HTML-parser** | Egen (i `parser.rs`) |
| **JS-motor** | Boa (sandboxad, lättviktig) |
| **Mål** | `wasm32-unknown-unknown` + native |
| **Protokoll** | HTTP API + MCP (rmcp) + WASM API |
| **Unika features** | Trust shield, semantic firewall, adversarial detection, causal action graph, vision (YOLOv8), tiered rendering, goal-driven adaptive DOM streaming |

---

## 3. Arkitekturjämförelse

### 3.1 Vad LightPanda har som AetherAgent saknar

| Kapabilitet | LightPanda | AetherAgent |
|-------------|------------|-------------|
| **Full JS-exekvering (V8)** | Ja — full V8, alla Web APIs | Boa — sandboxad, begränsad |
| **CDP-kompatibilitet** | 17-22 domäner, Puppeteer/Playwright fungerar | Nej |
| **Nätverksstack** | libcurl med cookies, redirects, HTTP/2, brotli | reqwest (enklare) |
| **DOM-manipulation från JS** | Full — JS kan ändra DOM fritt | Begränsad — `getElementById`/`querySelector` |
| **SPA-hantering** | Naturlig (V8 kör React/Vue/Angular) | Begränsad (XHR-interception, tier escalation) |
| **WebSocket-stöd** | Ja | Nej |
| **Skalbarhet** | 150+ instanser på 8GB, linjär skalning | Single-page focus |

### 3.2 Vad AetherAgent har som LightPanda saknar

| Kapabilitet | AetherAgent | LightPanda |
|-------------|-------------|------------|
| **Promptinjektionsskydd** | Trust shield med mönsterdetektion, zero-width-teckendetektion, sanitering | Inget — arkitektonisk säkerhet, ingen innehållsanalys |
| **Semantic Firewall** | 3-nivå målmedveten requestfiltrering (L1/L2/L3) | Inget |
| **Adversarial Pattern Detection** | Eskalerande/gradvisa injektioner, misstänkt volatilitet, strukturell manipulation | Inget |
| **Tillgänglighetstree** | WCAG-labelkedja, rolldetektion, synlighetsdetektion | Inget — "semantic_tree" men ingen a11y |
| **Målrelevansscoring** | Goal-relevance scoring per nod | Inget |
| **Causal Action Graph** | Handling-konsekvensmodellering, `find_safest_path` | Inget |
| **Vision/YOLOv8** | Inbyggd ONNX-inferens, UI-elementdetektion | Inget — ingen rendering |
| **Tiered Rendering** | Blitz (ren Rust) + CDP-eskalering | Inget — aldrig renderar |
| **Temporal Memory** | Tidsserie-sidtillståndshantering, nod-volatilitet | Inget |
| **Goal-Driven Streaming** | Adaptiv DOM-streaming med 95-99% tokenbesparing | Inget |
| **Semantic Diffing** | Minimal delta mellan träd, 80-95% tokenbesparing | Inget |
| **WASM-distribution** | Kompilerar till wasm32-unknown-unknown | Planerat men ej levererat |
| **WebMCP Discovery** | Detekterar MCP-registreringar på webbsidor | Inget |
| **Workflow Orchestration** | Multi-sida arbetsflöden med rollback/retry | Inget (CDP-baserat, externt) |

### 3.3 Överlapp

Båda har:
- **MCP-server** med verktyg för AI-agenter
- **Semantic tree output** (olika format och djup)
- **Markdown-extraktion** (LightPanda inbyggt, AetherAgent via `extract_data`)
- **Interaktiva element-listning**
- **Nod-ID-baserad interaktion** (klick, formulärfyllning)
- **HTTP-hämtning** med robots.txt-compliance

---

## 4. Prestandajämförelse

### LightPanda vs Chrome (deras benchmarks)

| Metrik | LightPanda | Chrome Headless |
|--------|------------|-----------------|
| 100 sidor (singel) | 2.3s / 24MB | 25.2s / 207MB |
| 933 sidor (25 parallella) | 4.81s / 123MB | 46.70s / 2GB |
| Faktor | **11x snabbare, 9x mindre minne** | — |
| Uppstart | ~100ms | ~800ms |
| Per instans | ~50MB | ~450MB |

### AetherAgent (Fas 15 Streaming Parser)

- **Early stopping**: Stoppar vid `max_nodes`, djupbegränsning, relevansfiltrering
- **Tokenbesparing**: 95-99% med goal-driven adaptive streaming
- **Fokus**: Perceptionseffektivitet, inte sidladdningshastighet

**Slutsats**: LightPanda optimerar **sidhämtning/rendering-hastighet**. AetherAgent optimerar **perceptions-effektivitet** (vad LLM:en ser). Olika optimeringsmål.

---

## 5. Säkerhetsmodeller

### LightPanda: Arkitektonisk säkerhet
- Minimal attackyta (färre Web APIs = färre sårbarheter)
- "En browser, en uppgift" — isolerade instanser
- Granulära per-uppgift-behörigheter
- Lokal exekvering — ingen molnberoende
- **Ingen promptinjektionsdetektion** — förlitar sig på att begränsa capabilities

### AetherAgent: Innehållsmedveten säkerhet
- `TrustLevel::Untrusted` som default för allt webbinnehåll
- Promptinjektionsmönsterdetektion (dolda texter, zero-width-tecken, multi-mönster)
- Sanitering och säker wrapping
- Semantic Firewall med 3-nivå filtrering
- Adversarial pattern detection (eskalerande, gradvisa injektioner)
- Temporal memory detekterar misstänkt volatilitet
- Causal action graph hittar säkraste vägen

**Slutsats**: LightPanda skyddar **infrastrukturen**. AetherAgent skyddar **LLM-agenten från manipulation via webbinnehåll**. Dessa kompletterar varandra.

---

## 6. Integrationsmöjligheter

### Scenario A: LightPanda som fetch-backend för AetherAgent

```
[LLM-agent] → [AetherAgent WASM] → [LightPanda CDP] → [Webb]
                     ↓
            Semantic tree + Trust analysis
            från LightPandas DOM-output
```

**Fördelar:**
- LightPanda hanterar JS-tunga SPA:er som AetherAgents Boa inte klarar
- AetherAgent lägger på trust shield, relevansscoring, adversarial detection
- CDP-kompatibilitet ger fullständig browserkapabilitet
- Bevarar AetherAgents unika perceptions- och säkerhetslager

**Utmaningar:**
- Kräver att AetherAgent kan konsumera DOM från extern källa (inte bara rå HTML)
- Arkitekturkomplexitet ökar
- AGPL-3.0 licens kan begränsa kommersiell integration

### Scenario B: Parallell deployment

```
[LLM-agent] → [AetherAgent] för: statiska sidor, säkerhetskritiska flöden
            → [LightPanda]  för: JS-tunga SPA:er, automation, skalning
```

**Fördelar:**
- Varje motor används där den är starkast
- Ingen arkitekturberoende mellan dem
- AetherAgent för säkerhetskritiska flöden (bank, e-handel)
- LightPanda för volymskrapning och SPA-navigation

### Scenario C: AetherAgent som LightPanda-plugin

LightPanda saknar:
- Promptinjektionsskydd
- Målrelevansscoring
- Adversarial detection
- Semantic diffing/streaming

AetherAgent kunde exponeras som ett postprocessing-steg efter LightPandas DOM-konstruktion, antingen via CDP-hook eller som en MCP-tool-chain.

---

## 7. Strategisk bedömning

### LightPandas styrkor
1. **Full V8 = full webbkompatibilitet** — React, Vue, Angular, alla SPA:er
2. **CDP-kompatibilitet** — befintliga Puppeteer/Playwright-skript fungerar
3. **11x snabbare än Chrome** — dramatisk prestandafördel
4. **22 000 GitHub-stjärnor** — stark community-momentum
5. **Skalbarhet** — 150+ instanser på 8GB

### AetherAgents styrkor
1. **Promptinjektionsskydd** — ingen annan browser-motor har detta
2. **WASM-distribution** — inbäddbar var som helst, ingen binärberoende
3. **Goal-driven perception** — LLM ser bara relevant innehåll (95-99% tokenbesparing)
4. **Adversarial modeling** — temporal, causal, och strukturell attack-detektion
5. **Vision + tiered rendering** — screenshot-baserad UI-detektion utan extern browser
6. **Komplett AI-agent-stack** — från fetch till workflow orchestration

### LightPandas svagheter (ur AetherAgent-perspektiv)
1. **Ingen säkerhet mot webbinnehåll** — LLM-agenten är oskyddad mot prompt injection
2. **Ingen relevansfiltrering** — hela DOM:en skickas till LLM (dyrt, brusigt)
3. **Ingen WASM** — kräver native binary, kan inte köras i browser/edge
4. **AGPL-3.0** — begränsar kommersiell inbäddning
5. **Beta-status** — ~5% av sajter kraschar

### AetherAgents svagheter (ur LightPanda-perspektiv)
1. **Begränsad JS** — Boa klarar inte moderna SPA-ramverk
2. **Ingen CDP** — kan inte ersätta Puppeteer/Playwright
3. **Single-page fokus** — inte optimerad för storskalig parallell skrapning
4. **Mindre community** — ny i ekosystemet

---

## 8. Rekommendation

**AetherAgent och LightPanda konkurrerar inte. De löser olika problem i samma pipeline.**

| Pipeline-steg | Bäst lämpad |
|---------------|-------------|
| Sidhämtning, JS-exekvering | LightPanda |
| DOM → Semantisk perception | AetherAgent |
| Promptinjektionsskydd | AetherAgent |
| Målrelevansscoring | AetherAgent |
| SPA-navigation | LightPanda |
| Storskalig parallell skrapning | LightPanda |
| Säkerhetskritiska agentflöden | AetherAgent |
| WASM/edge-deployment | AetherAgent |
| Workflow orchestration | AetherAgent (inbyggt) / LightPanda (via CDP+extern) |

**Optimal arkitektur**: LightPanda för sidladdning + JS-exekvering, AetherAgent för semantisk perception + säkerhet + LLM-optimering ovanpå.

---

## Källor

- [LightPanda Website](https://lightpanda.io/)
- [LightPanda GitHub](https://github.com/lightpanda-io/browser) (22k+ stars)
- [Why Build a New Browser — LightPanda Blog](https://lightpanda.io/blog/posts/why-build-a-new-browser)
- [Migrating Our DOM to Zig — LightPanda Blog](https://lightpanda.io/blog/posts/migrating-our-dom-to-zig)
- [From Local to Real World Benchmarks — LightPanda Blog](https://lightpanda.io/blog/posts/from-local-to-real-world-benchmarks)
- [Browser Security in the Age of AI Agents — LightPanda Blog](https://lightpanda.io/blog/posts/browser-security-in-the-age-of-ai-agents)
- [LightPanda Usage Docs](https://lightpanda.io/docs/open-source/usage)
- [Agentic Browser Landscape 2026](https://www.nohackspod.com/blog/agentic-browser-landscape-2026)
