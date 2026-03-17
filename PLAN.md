# AetherAgent v0.2 — Komplett implementationsplan

## Översikt

Två stora features för v0.2:

1. **Fas 10: XHR Network Interception** — Fånga priser och dynamiskt innehåll från XHR/fetch-anrop
2. **Fas 11: Inbyggd YOLOv8-inferens via rten** — Screenshot-only agenter med UI-elementdetektering i WASM

---

## Del 1: XHR Network Interception (Fas 10)

### Bakgrund

Priser på apple.com, amazon.com, zalando.se etc. laddas via separata XHR/fetch-anrop efter initial sidladdning. AetherAgent v0.1 är blind för dessa. Se `debug/aether_xhr_network_interception.docx` för fullständig analys.

### Steg 1.1: `src/intercept.rs` — XhrCapture + NetworkInterceptor

Ny modul med:

```rust
pub struct XhrCapture {
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Vec<u8>>,
    pub response: Option<Vec<u8>>,
}

pub struct NetworkInterceptor {
    firewall: Arc<SemanticFirewall>,
    goal: String,
    max_requests: usize,       // default 10
    timeout_ms: u64,           // default 2000
}
```

- `intercept()` — tar emot XhrCapture, kör Semantic Firewall L1/L2/L3, hämtar via reqwest om godkänd
- `normalize_xhr_response()` — parsar JSON → `SemanticNode` med `role: "price"` och `source: NodeSource::Xhr`
- Kräver feature `fetch` (redan befintlig)

### Steg 1.2: `src/js_bridge.rs` — Injicera fetch-interceptor i Boa

Utöka befintlig `js_bridge.rs`:

- `inject_fetch_interceptor(ctx, tx)` — ersätt `window.fetch` och `XMLHttpRequest` med stub-funktioner som skickar URL:er till en `mpsc::Sender<XhrCapture>`
- JS-koden tror att fetch() anropades normalt, får tillbaka `undefined` / tom response
- Interceptade URL:er hanteras parallellt av `NetworkInterceptor`

### Steg 1.3: `src/types.rs` — Utöka SemanticNode och SemanticTree

- Lägg till `NodeSource::Xhr(String)` variant i source-enum
- Lägg till `xhr_intercepted: u32` och `xhr_blocked: u32` i `SemanticTree`
- Lägg till `xhr_injection_warnings: Vec<String>` i output

### Steg 1.4: Tree Merger — Infoga XHR-noder i semantiskt träd

I `intercept.rs` eller som del av `semantic.rs`:

- Matcha XHR-prisdata mot befintliga platshållarnoder (tomma `role: "text"` noder nära produktlänkar)
- Om ingen matchning: lägg till som barn till närmaste relevanta produktnod
- Re-score relevance efter merge

### Steg 1.5: API-uppdatering (bakåtkompatibel)

Uppdatera `fetch_parse` i `src/fetch.rs` och `src/bin/server.rs`:

```rust
// Ny opt-in parameter
pub struct FetchOptions {
    pub intercept_xhr: bool,       // default false
    pub intercept_max: usize,      // default 10
    pub intercept_timeout_ms: u64, // default 2000
}
```

MCP-tool `fetch_parse` får nya optional-parametrar. Befintliga anrop utan parametrar fungerar identiskt.

### Steg 1.6: Säkerhet

- Alla XHR-noder märks `TrustLevel::Untrusted`
- `check_injection()` från `trust.rs` körs på alla XHR-responses
- Semantic Firewall L1/L2/L3 filtrerar innan fetch (blockerar analytics, tracking, media)
- SSRF-skydd gäller även XHR-URL:er

### Steg 1.7: Tester

- **Unit tests** i `intercept.rs`: XhrCapture → normalize → SemanticNode med role:price
- **Unit tests** i `js_bridge.rs`: fetch-interceptor injiceras korrekt i Boa
- **Integration tests**: HTML med tom prisplatshållare + mock XHR-response → komplett träd med priser
- **Firewall tests**: Verifiera att analytics/tracking-XHR blockeras, pris-XHR tillåts

---

## Del 2: YOLOv8-inferens via rten (Fas 11)

### Bakgrund

Screenshot-only path: PNG → objektdetektering → bboxes → grounding mot semantiskt träd. Gated bakom `--features vision`.

### Steg 2.1: Cargo.toml — Feature flag `vision`

```toml
[dependencies]
rten = { version = "0.15", optional = true }
image = { version = "0.25", optional = true }

[features]
vision = ["rten", "image"]
```

### Steg 2.2: `src/vision.rs` — Bildavkodning + rten-inferens

```rust
pub struct VisionDetector {
    model: rten::Model,
    input_size: (u32, u32),  // 640x640 eller 416x416
    confidence_threshold: f32,
}

pub struct UiDetection {
    pub class: String,        // "button", "input", "link", "icon", "text", "image"
    pub confidence: f32,
    pub bbox: BoundingBox,    // x, y, width, height (normaliserade)
}
```

- `VisionDetector::new(model_bytes)` — ladda ONNX-modell
- `detect(png_bytes)` → `Vec<UiDetection>`
- Preprocessing: decode PNG → resize till input_size → RGB → tensor (CHW format)
- Postprocessing: NMS (non-max suppression), confidence-filter

### Steg 2.3: Modellhantering — YOLOv8-nano ONNX

- Modell: `yolov8n-ui.onnx` — fintrimmad för UI-element (knappar, inputs, länkar, ikoner, text, bilder)
- **INT8-quantisering** (kritisk optimering):
  - Minskar storlek ~75%: 6 MB FP32 → ~1.5–2 MB INT8
  - 2–4x snabbare inference i WASM
  - Minimal accuracy-förlust för UI-element
- Modellen laddas runtime (inte inbakad i WASM-binären) via `VisionDetector::new(model_bytes: &[u8])`

### Steg 2.4: WASM-prestandaoptimering

Baserat på rten best practices:

**Cargo.toml (release profile — redan korrekt):**
```toml
[profile.release]
opt-level = 3          # "z" för storlek, "3" för hastighet — välj "3" för vision
lto = true
codegen-units = 1
panic = "abort"
```

**OBS:** Nuvarande `opt-level = "z"` bör bli `"3"` i vision-feature-builds för maximal inference-hastighet. Alternativt: separat profil.

**SIMD-aktivering:**
- Bygg med `RUSTFLAGS="-C target-feature=+simd128"` för WASM
- rten använder SIMD internt → signifikant snabbare
- Alla moderna browsers stödjer WASM SIMD (Chrome 91+, Firefox 89+, Safari 16.4+)

**wasm-opt efterbehandling:**
```bash
wasm-opt -O4 --enable-simd --strip-debug -o optimized.wasm target/wasm32-unknown-unknown/release/aether_agent.wasm
```

**Bildpreprocessing-optimeringar:**
- Skala ner till 640x640 (YOLOv8-standard) eller 416x416 för snabbare inference
- All preprocessing i Rust (inte JS)
- Perceptual hash för att skippa inference om screenshot inte ändrats

### Steg 2.5: `parse_screenshot()` — Ny WASM-export

```rust
#[wasm_bindgen]
#[cfg(feature = "vision")]
pub fn parse_screenshot(png_bytes: &[u8], goal: &str, model_bytes: &[u8]) -> String {
    // 1. Ladda/cacha modell
    // 2. Detect UI-element
    // 3. Bygg SemanticTree från detections
    // 4. Filtrera mot goal-relevance
    // 5. JSON-output
}
```

### Steg 2.6: Integration med grounding.rs

- `ground_visual_detections(detections, semantic_tree)` — matcha visuella bboxes mot DOM-noder
- Verifiering: noder som finns i DOM men inte visuellt synliga → flagga som `potentially_hidden`
- Noder som syns visuellt men inte i DOM → flagga som `visual_only` (canvas, iframe-content)

### Steg 2.7: Web Worker-strategi (JS-sidan)

- Kör hela inference i en Web Worker så main thread inte fryser
- Asynkron modell-laddning med progress-callback
- Fallback: om inference > 800 ms → timeout, returnera tomt resultat
- Dokumentera i README hur man sätter upp Worker

### Steg 2.8: Tester

- **Unit tests** i `vision.rs`: tensor-preprocessing, NMS, confidence-filter
- **Integration tests**: mock-detections → grounding mot semantic tree
- **Performance test**: 640x640 PNG → detect → assert < 500ms (native), < 800ms (WASM target)
- **Feature gate test**: `cargo test` utan `vision` feature → kompilerar och passerar

---

## Implementationsordning

| Prio | Fas | Steg | Beskrivning | Beroenden |
|------|-----|------|-------------|-----------|
| 1 | 10 | 1.1 | `intercept.rs` — XhrCapture + NetworkInterceptor | fetch feature |
| 2 | 10 | 1.2 | `js_bridge.rs` — fetch-interceptor i Boa | js-eval feature, steg 1.1 |
| 3 | 10 | 1.3 | `types.rs` — NodeSource::Xhr, nya fält | steg 1.1 |
| 4 | 10 | 1.4 | Tree Merger — infoga XHR-noder | steg 1.1, 1.3 |
| 5 | 10 | 1.5 | API-uppdatering — opt-in parameter | steg 1.1–1.4 |
| 6 | 10 | 1.6 | Säkerhet — injection checks på XHR | steg 1.1 |
| 7 | 10 | 1.7 | Tester — unit + integration | alla ovan |
| 8 | 11 | 2.1 | Cargo.toml — vision feature flag | — |
| 9 | 11 | 2.2 | `vision.rs` — VisionDetector + rten | steg 2.1 |
| 10 | 11 | 2.3 | Modellhantering — INT8 quantisering | steg 2.2 |
| 11 | 11 | 2.4 | WASM-prestandaoptimering — SIMD, wasm-opt | steg 2.2 |
| 12 | 11 | 2.5 | `parse_screenshot()` WASM-export | steg 2.2 |
| 13 | 11 | 2.6 | Grounding-integration | steg 2.5, grounding.rs |
| 14 | 11 | 2.7 | Web Worker-strategi (dokumentation) | steg 2.5 |
| 15 | 11 | 2.8 | Tester | alla ovan |

---

## Prestandamål

| Metrik | XHR Interception | Vision (YOLOv8-nano) |
|--------|-------------------|----------------------|
| Latens | ~180ms (nätverksberoende) | 50–200ms (desktop SIMD+INT8) |
| WASM-storlek (delta) | +0 (redan i fetch) | +2–3 MB (INT8 modell + rten) |
| Accuracy | N/A | >90% mAP@0.5 för UI-element |
| Fallback | Timeout 2s → returnera träd utan XHR | Timeout 800ms → returnera tomt |

## Risker och mitigeringar

| Risk | Sannolikhet | Mitigation |
|------|-------------|------------|
| rten WASM-binär för stor | Medel | INT8 + `opt-level="z"` + feature gate |
| Boa fetch-stub bryter JS-logik | Låg | Returnera tom response, JS fortsätter |
| XHR-responses med injection | Medel | `check_injection()` på alla responses |
| SIMD ej stödd i äldre browsers | Låg | Fallback till scalar — rten hanterar automatiskt |
| INT8 accuracy-förlust | Låg | UI-element (knappar, inputs) är enkla att detektera |
