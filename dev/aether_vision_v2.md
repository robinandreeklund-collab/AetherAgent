# AetherAgent Vision Layer — AetherShot v3
## Fullständig implementationsplan med tvånivå-fallback
### Blitz (Tier 1) → CDP/Chrome (Tier 2)

---

## Revision log — v1 → v2 → v3

| # | Förändring | v1 | v2 | v3 |
|---|---|---|---|---|
| 1 | Trigger-logik | Hot path | Fallback 1% | Oförändrad |
| 2 | Tab pool | 2–4 tabs | 1 varm tab lazy | Oförändrad |
| 3 | Abstraktionsnivå | Konkret struct | Trait | Utökat: TieredBackend |
| 4 | XHR-koppling | Saknad | Inbakad | Styr tier-val |
| 5 | **Tier 1** | **Saknad** | **Saknad** | **Blitz: ~10ms, noll Chrome** |
| 6 | **Tier 2** | **Alltid CDP** | **Alltid CDP** | **CDP: bara när Blitz inte räcker** |
| Bonus | Visual Firewall | Saknad | Inbakad | Kör på Tier 1 + Tier 2 |

---

## 1. Kontext — varför två nivåer

**AetherAgents baseline:** 653µs–1.39ms per sida (bench_main.rs).
**Timing-krav för screenshot:** under 100ms warm för att inte störa flödet.

Blitz (DioxusLabs/blitz) är ett Rust-native HTML/CSS-renderingsbibliotek
byggt på html5ever + stylo + vello + wgpu. ~12MB binär. Noll processstart.
Noll IPC. Ingen JavaScript-support — och det är precis rätt för Tier 1.

```
Blitz vet:    HTML structure, CSS layout, statisk text, bilder, SVG
Blitz vet ej: JavaScript-renderat innehåll, Canvas, WebGL, SPA-state

CDP vet allt — men kostar 500ms cold start + separat process.
```

De flesta webbsidor AetherAgent besöker är antingen:
- Server-renderat HTML (statisk e-commerce, dokumentation, nyheter) → Blitz
- JavaScript-renderat (React/SPA, Chart.js, dynamisk data) → CDP

Blitz klarar troligen 60–70% av alla screenshot-requests. CDP behövs
bara för resterande 30–40%. Chrome startar kanske aldrig alls under
många agent-sessioner.

---

## 2. Ny arkitektur — TieredBackend

```
AetherAgent core
      │
      ▼ (1% av requests behöver pixlar)
      │
      ▼
┌─────────────────────────────────────────┐
│          TieredBackend                  │
│                                         │
│  1. Blitz (~10–15ms, noll process)      │
│     ├── OK → returnera direkt           │
│     └── JS/Canvas/blank → eskalera      │
│                   │                     │
│  2. CDP (~60–80ms warm, lazy Chrome)    │
│     └── returnerar alltid               │
└─────────────────────────────────────────┘
```

### Trait — oförändrat från v2, men nu med tier-info i result

```rust
// src/vision/backend.rs

use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct ScreenshotRequest {
    pub url: Option<String>,
    pub clip: Option<ViewportClip>,
    pub quality: EncodeMode,
    pub source_hint: VisualSourceHint,
    /// Hint om vilken tier som troligen behövs — sätts av XHR-interceptorn
    pub tier_hint: TierHint,
}

#[derive(Debug, Clone, Default)]
pub enum TierHint {
    /// Prova Blitz först (default)
    #[default]
    TryBlitzFirst,
    /// XHR-data indikerar JavaScript-rendering → skippa Blitz direkt
    RequiresJs { reason: String },
}

#[derive(Debug)]
pub struct ScreenshotResult {
    pub image_b64: String,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    pub latency_ms: u64,
    pub size_bytes: usize,
    pub triggered_by: VisualSourceHint,
    /// Vilken tier som faktiskt levererade
    pub tier_used: ScreenshotTier,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ScreenshotTier {
    Blitz,  // Tier 1 — snabb, in-process
    Cdp,    // Tier 2 — fullständig, JS-kapabel
    Mock,   // Tester
}

#[async_trait]
pub trait PerceptionBackend: Send + Sync {
    async fn screenshot(
        &self,
        req: ScreenshotRequest,
    ) -> anyhow::Result<ScreenshotResult>;

    async fn is_visual_required(&self, hints: &VisualHints) -> bool;
}
```

---

## 3. TieredBackend — hjärtat i v3

```rust
// src/vision/tiered_backend.rs

pub struct TieredBackend {
    blitz: BlitzBackend,
    cdp: CdpBackend,
}

#[async_trait]
impl PerceptionBackend for TieredBackend {
    async fn screenshot(
        &self,
        req: ScreenshotRequest,
    ) -> anyhow::Result<ScreenshotResult> {

        // Om XHR-interceptorn redan vet att JS krävs → skippa Blitz
        if matches!(req.tier_hint, TierHint::RequiresJs { .. }) {
            tracing::debug!("tier_hint=RequiresJs → skipping Blitz");
            return self.cdp.screenshot(req).await;
        }

        // Tier 1: Blitz
        match self.blitz.screenshot(req.clone()).await {
            Ok(result) => {
                // Kvalitetskontroll innan vi returnerar
                if self.blitz_result_is_valid(&result, &req) {
                    tracing::debug!(
                        "Tier1=Blitz OK {}ms {}KB",
                        result.latency_ms,
                        result.size_bytes / 1024
                    );
                    return Ok(result);
                }
                tracing::debug!("Blitz result invalid → escalating to CDP");
            }

            Err(e) => {
                tracing::debug!("Blitz failed ({e}) → escalating to CDP");
            }
        }

        // Tier 2: CDP fallback
        self.cdp.screenshot(req).await
    }

    async fn is_visual_required(&self, hints: &VisualHints) -> bool {
        hints.has_canvas
            || hints.xhr_triggered
            || hints.llm_requested
    }
}

impl TieredBackend {
    /// Avgör om Blitz-resultatet faktiskt innehåller rätt innehåll
    fn blitz_result_is_valid(
        &self,
        result: &ScreenshotResult,
        req: &ScreenshotRequest,
    ) -> bool {
        // 1. Storlekskontroll — under 500 bytes = blank rendering
        if result.size_bytes < 500 {
            return false;
        }

        // 2. Om sidan är känd JS-heavy via XHR-hints
        if matches!(req.source_hint, VisualSourceHint::XhrTriggered { .. }) {
            // XHR-triggered screenshots behöver nästan alltid CDP
            // eftersom data renderas via JS
            return false;
        }

        // 3. Framtida: pixel-analys för att detektera blank/white image
        // (kan läggas till utan att ändra strukturen)

        true
    }
}
```

---

## 4. BlitzBackend — Tier 1 implementation

```rust
// src/vision/blitz_backend.rs

use blitz_html::HtmlDocument;
use blitz_renderer_vello::VelloRenderer;

pub struct BlitzBackend {
    /// Återanvändbar renderer — init en gång, använd många gånger
    renderer: Arc<Mutex<Option<VelloRenderer>>>,
}

impl BlitzBackend {
    pub fn new() -> Self {
        Self {
            renderer: Arc::new(Mutex::new(None)),
        }
    }

    async fn get_or_init_renderer(&self) -> anyhow::Result<()> {
        let mut guard = self.renderer.lock().await;
        if guard.is_none() {
            // Blitz renderer init är snabb (~5–20ms, en gång)
            *guard = Some(VelloRenderer::new().await?);
        }
        Ok(())
    }
}

#[async_trait]
impl PerceptionBackend for BlitzBackend {
    async fn screenshot(
        &self,
        req: ScreenshotRequest,
    ) -> anyhow::Result<ScreenshotResult> {
        let t0 = std::time::Instant::now();

        self.get_or_init_renderer().await?;

        // Hämta HTML (från cache eller fetch)
        let html = match &req.url {
            Some(url) => fetch_html(url).await?,
            None => return Err(anyhow::anyhow!("BlitzBackend requires url")),
        };

        // Rendera HTML → pixelbuffer via blitz
        let (width, height) = viewport_from_request(&req);
        let document = HtmlDocument::from_html(
            &html,
            Some(req.url.as_deref().unwrap_or("")),
            vec![],
            None,
            None,
        );

        let pixel_buffer = {
            let mut renderer_guard = self.renderer.lock().await;
            let renderer = renderer_guard.as_mut().unwrap();
            renderer.render_to_buffer(&document, width, height).await?
        };

        // Clip till ROI om begärt
        let pixel_buffer = match &req.clip {
            Some(clip) => crop_buffer(&pixel_buffer, clip, width, height)?,
            None => pixel_buffer,
        };

        // Encode
        let encoded = encode_pixels(
            &pixel_buffer,
            if req.clip.is_some() {
                req.clip.as_ref().unwrap().width as u32
            } else {
                width
            },
            if req.clip.is_some() {
                req.clip.as_ref().unwrap().height as u32
            } else {
                height
            },
            &req.quality,
        )?;

        Ok(ScreenshotResult {
            image_b64: base64::engine::general_purpose::STANDARD
                .encode(&encoded.data),
            format: encoded.format,
            width: encoded.width,
            height: encoded.height,
            latency_ms: t0.elapsed().as_millis() as u64,
            size_bytes: encoded.data.len(),
            triggered_by: req.source_hint,
            tier_used: ScreenshotTier::Blitz,
        })
    }

    async fn is_visual_required(&self, hints: &VisualHints) -> bool {
        // Delegeras uppåt till TieredBackend
        false
    }
}
```

---

## 5. CdpBackend — Tier 2, oförändrad från v2

En enda varm tab, lazy Chrome-init. Bara ändringen att `tier_used` sätts:

```rust
// src/vision/cdp_backend.rs — tillägg

Ok(ScreenshotResult {
    // ... samma som v2 ...
    tier_used: ScreenshotTier::Cdp,  // NY I V3
})
```

---

## 6. XHR-koppling — tier_hint sätts av intercept.rs

V2:s `is_visual_trigger()` utökas nu för att även ge `TierHint`:

```rust
// src/intercept.rs — v3 tillägg

impl XhrCapture {
    /// Returnerar TierHint baserat på XHR-innehåll
    pub fn tier_hint(&self) -> TierHint {
        if !self.is_visual_trigger() {
            return TierHint::TryBlitzFirst;
        }

        // Kända JS-rendering-indikatorer → skippa Blitz direkt
        let js_indicators = [
            "chartType", "canvasId", "plotly", "vega",
            "datasets", "d3", "echarts", "highcharts",
        ];

        let body = self.raw_body.as_deref().unwrap_or("");
        let is_js_chart = js_indicators.iter().any(|k| body.contains(k));

        if is_js_chart {
            TierHint::RequiresJs {
                reason: format!(
                    "XHR body contains JS chart indicators, endpoint={}",
                    self.url
                ),
            }
        } else {
            // Visual trigger men oklart om JS — prova Blitz ändå
            TierHint::TryBlitzFirst
        }
    }
}
```

I perception-steget byggs `ScreenshotRequest` med hint från XHR:

```rust
// src/perception.rs

let tier_hint = xhr
    .iter()
    .filter(|x| x.is_visual_trigger())
    .map(|x| x.tier_hint())
    .find(|h| matches!(h, TierHint::RequiresJs { .. }))
    .unwrap_or_default();

let req = ScreenshotRequest {
    url: Some(url.to_string()),
    clip: best_clip_from_tree(&tree),
    quality: EncodeMode::BalancedVision,
    source_hint: build_source_hint(&hints),
    tier_hint,   // NY I V3
};
```

---

## 7. Timing — uppdaterade targets

| Scenario | Tier | Target | Strategi |
|---|---|---|---|
| Normal parse (ingen screenshot) | — | 653µs–1.4ms | AetherAgent orörd |
| Statisk HTML/CSS screenshot | **Tier 1: Blitz** | **~10–15ms** | In-process render |
| Statisk element screenshot | **Tier 1: Blitz** | **~5–10ms** | Blitz + ROI crop |
| XHR=RequiresJs, warm CDP | Tier 2: CDP | ~60–80ms | Skip Blitz, CDP direkt |
| Blitz eskalerar → CDP warm | Tier 2: CDP | ~60–80ms | Blitz miss + CDP |
| Cold start (Blitz renderer) | Tier 1 | ~10–30ms | En gång, sedan cache |
| Cold start (Chrome process) | Tier 2 | < 1.5s | Lazy init |
| Visual firewall scan | Båda | < 30ms | YOLO nano post-shot |

**Typisk fördelning i produktion:**

```
Alla agent-steg:          100%
→ Kräver screenshot:        ~1%
  → Blitz klarar:          ~65%  (~10ms, Chrome startar aldrig)
  → CDP behövs:            ~35%  (~70ms warm)

Chrome startar bara om CDP-requests inträffar.
Många agent-sessioner slutar utan att Chrome startats.
```

---

## 8. Fullständig repo-struktur v3

```
src/
├── vision/
│   ├── mod.rs               ← pub use + TieredBackend som default
│   ├── backend.rs           ← PerceptionBackend trait + typer
│   ├── tiered_backend.rs    ← TieredBackend (NY I V3)
│   ├── blitz_backend.rs     ← BlitzBackend Tier 1 (NY I V3)
│   ├── cdp_backend.rs       ← CdpBackend Tier 2 (från v2)
│   ├── mock_backend.rs      ← MockBackend för tester
│   ├── capture.rs           ← CDP screenshot commands
│   ├── encode.rs            ← TurboJPEG + WebP pipeline
│   ├── roi.rs               ← Region-of-interest + crop
│   ├── visual_firewall.rs   ← YOLO injection scan (från v2)
│   └── types.rs             ← ScreenshotRequest/Result/EncodeMode/Tier
├── intercept.rs             ← is_visual_trigger() + tier_hint() (v3)
├── perception.rs            ← agent_perception_step() + tier_hint prop
└── lib.rs
```

---

## 9. Cargo.toml — uppdaterat

```toml
[dependencies]
# Tier 1: Blitz
blitz-html     = { git = "https://github.com/DioxusLabs/blitz" }
blitz-renderer-vello = { git = "https://github.com/DioxusLabs/blitz" }
# OBS: Blitz är alpha — pin till specifik commit i produktion
# blitz-html = { git = "...", rev = "abc1234" }

# Tier 2: CDP
chromiumoxide  = { version = "0.6", features = ["tokio-runtime"] }

# Encode pipeline (oförändrad)
turbojpeg      = { version = "1.0", features = ["image"] }
webp           = "0.3"
image          = { version = "0.25", features = ["jpeg", "webp"] }
base64         = "0.22"

# Runtime
async-trait    = "0.1"
tokio          = { version = "1", features = ["full"] }
tracing        = "0.1"
anyhow         = "1.0"

# HTTP för Blitz HTML-fetch
reqwest        = { version = "0.12", features = ["json"] }
```

**OBS om Blitz:** Blitz är officiellt i alpha-status (mål: beta Q4 2025,
produktion 2026). Pin till en specifik commit för reproducibla builds.
Feature flag rekommenderas:

```toml
[features]
default  = ["tier1-blitz", "tier2-cdp"]
tier1-blitz = ["dep:blitz-html", "dep:blitz-renderer-vello"]
tier2-cdp   = ["dep:chromiumoxide"]
```

---

## 10. Implementationsordning v3

### Fas A — Trait + Typer + Mock (dag 1)
`backend.rs` med `TierHint`, `ScreenshotTier`, `MockBackend`.
Ingen Chrome, ingen Blitz krävs. Testa trigger-logiken isolerat.

### Fas B — CdpBackend (dag 2–3)
Exakt v2:s implementation. Lägg bara till `tier_used: ScreenshotTier::Cdp`.
Benchmark: warm shot < 80ms.

### Fas C — TieredBackend utan Blitz (dag 3)
Implementera `TieredBackend` med Blitz-platshållare som alltid
eskalerar till CDP. Verifiera att fallback-logiken fungerar.

```rust
// Tillfällig platshållare tills Blitz är integrerad
impl BlitzBackend {
    async fn screenshot(&self, _req: ScreenshotRequest) 
        -> anyhow::Result<ScreenshotResult> 
    {
        Err(anyhow::anyhow!("BlitzBackend: not yet implemented"))
    }
}
```

### Fas D — BlitzBackend (dag 4–6)
Integrera blitz-html + blitz-renderer-vello.
Börja med `render_to_buffer` för statisk HTML utan nätverk.
Lägg till `blitz_result_is_valid` kvalitetskontroll.
Benchmark: Blitz-path < 15ms.

### Fas E — XHR-integration + tier_hint (dag 6–7)
`tier_hint()` i `intercept.rs`.
Propagera till `ScreenshotRequest` i `perception.rs`.
Testa mot Campfire Commerce (statisk → Blitz, XHR chart → CDP).

### Fas F — Visual Firewall (dag 7–8)
Kör på resultat från båda tiers.
Koppla till `check_injection` i MCP.

### Fas G — Benchmarks
Mät tier-fördelning på representativa sidor.
Target: >60% av screenshot-requests klaras av Blitz.

---

## 11. Tier-statistik i produktion

Logga alltid vilket tier som användes — det ger data för att
förbättra `blitz_result_is_valid` och `tier_hint`:

```rust
// I perception.rs efter screenshot
tracing::info!(
    tier = ?shot.tier_used,
    latency_ms = shot.latency_ms,
    size_kb = shot.size_bytes / 1024,
    source = ?shot.triggered_by,
    "screenshot completed"
);

// Metrics-counter (prometheus/opentelemetry)
SCREENSHOT_TIER_COUNTER
    .with_label_values(&[shot.tier_used.as_str()])
    .inc();
```

Med real data kan Blitz-träffraten kontinuerligt förbättras utan
att ändra arkitekturen.

---

## 12. Vad som är genuint världsunikt i v3

1. **Dual-tier screenshot med intelligent eskalering** — Blitz renderar
   statiska sidor på ~10ms in-process. Chrome aktiveras bara för
   JavaScript-tungt innehåll. Ingen annan agent-browser-stack gör detta.

2. **XHR-driven tier-val** — `intercept.rs` beslutar redan vid
   nätverkslevel vilket tier som behövs, 100–200ms innan DOM renderas.
   Resultatet: rätt tier, rätt tid, noll onödig Chrome-start.

3. **Chrome startar kanske aldrig** — I sessioner som bara träffar
   statiska sidor körs hela vision-lagret utan en enda Chrome-process.
   ~12MB Blitz-renderer vs ~150MB Chrome. Det är AetherAgents
   minimalistiska design genomförd hela vägen ner till pixellayret.

4. **Visual Firewall på båda tiers** — Blitz-renders och CDP-renders
   passerar samma YOLO-baserade injektionsdetektion. Dual-channel
   säkerhet oavsett vilket tier som använde.

5. **Benchmark-verified baseline** — AetherAgents 1.39ms parse-stig
   påverkas noll. Screenshot lever utanför hot path. Tier-statistik
   i produktion driver kontinuerlig förbättring utan arkitekturändring.
