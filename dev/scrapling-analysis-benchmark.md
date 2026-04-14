# Scrapling vs AetherAgent: Analys & Benchmarks

*2026-04-14*

## 1. Vad ar Scrapling?

[Scrapling](https://github.com/D4Vinci/Scrapling) (v0.4.6, ~37k stars) ar ett Python-baserat adaptivt web scraping-ramverk. Det kombinerar:

- **Snabb HTML-parsing** via lxml (C-bibliotek)
- **Adaptiv elementsparing** som overlever webbplatsredesigner (SequenceMatcher + SQLite)
- **Tre-stegs anti-bot**: HTTP (curl_cffi med TLS-fingerprint), Stealth (patchright/Playwright med Cloudflare-bypass), Full Browser
- **Spider-ramverk** (Scrapy-liknande) med rate limiting, checkpoint/resume
- **MCP-server** for AI-integration (10 verktyg)

## 2. Benchmark-resultat

### 2.1 Parse-hastighet (5000 element, 671KB HTML)

| Bibliotek | Median | P95 | Min | Max |
|-----------|--------|-----|-----|-----|
| **Scrapling** | **15.75ms** | 21.37ms | 14.79ms | 23.68ms |
| Parsel/lxml | 18.96ms | 40.18ms | 17.33ms | 46.06ms |
| AetherAgent (CRFR) | 83.95ms | 242.82ms | 76.08ms | 242.82ms |

**Analys**: Scrapling/lxml ar ~5x snabbare pa ra parsning. Men AetherAgent gor betydligt mer: fullstandig semantisk analys, rollidentifiering, BM25+HDC-scoring, och gap-detektion -- inte bara DOM-traversering.

### 2.2 Text-extraktion (CSS .price)

| Bibliotek | Median | P95 |
|-----------|--------|-----|
| **Scrapling** | **1.64ms** | 2.84ms |
| Parsel/lxml | 4.92ms | 5.08ms |

### 2.3 Semantisk forstaelse (AetherAgent, liten sida)

| Fragestallning | Tid | Noder | Topp-resultat |
|-------------|------|-------|------------|
| Priser | 3.46ms | 8 | $34.99, $249.00 |
| Kop-knappar | 0.82ms | 6 | "Add to Cart" |
| Produktinnehall | 0.87ms | 10 | Produktnamn, priser |

**Scrapling kan INTE gora detta** -- den kanner inte igen vad element BETYDER, bara var de AR i DOM:en.

### 2.4 Cachad re-query (AetherAgent CRFR)

| Forsta frage | Cachad frage |
|-------------|-------------|
| ~84ms | **<1ms** |

AetherAgents CRFR-cache gor att upprepade fragor pa samma sida ar praktiskt taget gratis (~300us).

## 3. Feature-jamforelse

| Feature | Scrapling | AetherAgent |
|---------|-----------|-------------|
| HTML-parsing | lxml (C) | html5ever + arena DOM (Rust) |
| CSS-selektorer | Ja | Ja |
| XPath | Ja | Nej |
| Semantiska roller | **Nej** | **Ja** (button, link, heading, price) |
| Malmedveten rankning | **Nej** | **Ja** (CRFR resonansfalt) |
| Kausalt larande | **Nej** | **Ja** (crfr_feedback) |
| Prompt injection-skydd | **Nej** | **Ja** (trust shield) |
| JS-exekvering | Playwright (extern) | QuickJS (inbaddad, sandboxad) |
| Anti-bot/TLS-fingerprint | **Ja** (curl_cffi) | **Nej** |
| Adaptiv elementsparing | Ja (SequenceMatcher) | Ja (semantisk diff + HV) |
| Streaming DOM | **Nej** | **Ja** (95-99% token-besparing) |
| Vision/YOLO | **Nej** | **Ja** (22 UI-klasser) |
| WASM-kompilering | **Nej** | **Ja** |
| MCP-server | Ja (10 verktyg) | Ja (35+ verktyg) |
| Robots.txt | Ja (protego) | Ja (RFC 9309) |

## 4. Vad vi kan lara oss av Scrapling

### 4.1 TLS-fingerprint for fetch-hardening (PRIORITET: MEDIUM)

Scraplings `curl_cffi` impersonerar riktiga TLS-fingerprints (JA3/JA4). Var `reqwest`-baserade fetcher gor inte detta. For sajter med TLS-baserad bot-detektion ar detta vasentligt.

**Forslag**: Undersok `boring-rs` eller `rustls` med anpassade TLS-profiler. Alternativt: lata CDP-lagret (Tier 2) hantera anti-bot snarare an att implementera TLS-impersonering i Rust.

### 4.2 Blockerad-respons detektion med automatisk tier-eskalering (PRIORITET: HOG)

Scraplings `blocked_codes` (401, 403, 407, 429, 500-504) med automatisk retry pa hogre tier ar en bra pattern. Vi har TieredBackend men saknar automatisk eskalering baserat pa HTTP-statuskoder.

**Forslag**: I `fetch.rs`, detektera blockeringsstatuskoder. Om Tier 1 (ren reqwest) blockeras, eskalera automatiskt till Tier 2 (CDP) med stealth-headers.

```
Flode: reqwest (403) -> retry med anti-bot headers (403) -> CDP/Chrome (200)
```

### 4.3 Element-identitetspersistens over sidversioner (PRIORITET: MEDIUM)

Scraplings `relocate()` -- sparar elementfingeravtryck i SQLite och matchar mot nya sidversioner -- ar en pragmatisk losning for arbetsflodeskontinuitet. Var semantiska diff ar mer sofistikerad men saknar explicit persistens av element-ID:n for lang tids sparing.

**Forslag**: Utoka `session.rs` med element-fingerprint-lagring sa att orkestreraren kan hitta "samma knapp" aven efter sidredesign.

### 4.4 Checkpoint-baserad paus/resume for langvariga workflows (PRIORITET: LAG)

Spindlarnas formaga att serialisera crawl-tillstand till disk vid intervaller och ateruppta later ar anvandbar for langvariga orkestreringsworkflows.

**Forslag**: Lagg till checkpoint-stod i `orchestrator.rs` sa att multi-sid-workflows kan aterupptas efter avbrott.

### 4.5 Canvas/WebRTC/WebGL Stealth-flaggor (PRIORITET: LAG)

Specifika Chromium-argument for stealth:
- `--fingerprinting-canvas-image-data-noise`
- `--webrtc-ip-handling-policy=disable_non_proxied_udp`
- `--blink-settings` for pointer/hover-bypass

Anvandbart om vi bygger ut CDP-integrationen.

## 5. Vad Scrapling INTE har (var fordel)

| Saknas i Scrapling | AetherAgent-losning | Varfor det spelar roll |
|------|-------|--------|
| Semantisk forstaelse | CRFR resonansfalt | AI-agenter behover FORSTA sidor, inte bara extrahera |
| Prompt injection-skydd | Trust Shield | Webbinnehall kan attackera AI-agenter |
| Inbaddad JS | QuickJS sandbox | Ingen extern browser behovs for SPA:er |
| Streaming DOM | StreamingParser + Directives | 95-99% token-besparing |
| Vision | YOLOv8 ONNX | Visuell elementdetektion |
| WASM | wasm32-unknown-unknown | Korbar i webblasare |
| Kausalt larande | crfr_feedback | Systemet forbattras med anvandning |
| Maldriven DOM | CRFR + gap-detektion | "Hitta priset" utan CSS-selektor |

## 6. Slutsats

**Scrapling och AetherAgent loser fundamentalt olika problem:**

- **Scrapling** = *Utvecklarverktyg for dataextraktion fran skyddade sajter*
- **AetherAgent** = *Semantiskt perceptionslager for AI-agenter*

**Rekommendation**: Adoptera Scraplings anti-bot-tekniker (TLS-fingerprint, blockerad-respons eskalering) som hardening for var fetch-pipeline. Implementera INTE deras parsing (lxml ar snabbare men saknar semantik) eller elementsparing (SequenceMatcher ar grundare an var HV-baserade matchning).

De tre mest varldefulla forbattringarna att prioritera:
1. **Automatisk tier-eskalering vid blockering** (403/429 -> CDP)
2. **TLS-fingerprint i reqwest** (for anti-bot-hardening)
3. **Element-persistens i session.rs** (for workflow-kontinuitet)
