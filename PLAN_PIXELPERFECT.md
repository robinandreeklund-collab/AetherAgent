# Pixel-Perfect Rendering — Plan till 100%

## Nuläge (efter PR #16)

| Del | Status | % |
|-----|--------|---|
| JS-engine + sandbox | Fullt portad + förbättrad | 100% |
| Event loop + lifecycle | Fullt implementerad + integrerad i render | 100% |
| DOM API coverage | 98% av 129 APIs (+ attachShadow) | 98% |
| MutationObserver + dynamic | Fungerar via ArenaDom | 95% |
| Viewport-sync | Dynamisk width/height → JS | 100% |
| Noscript-stripping | Före CSS-pipeline | 100% |
| Shadow DOM + Web Components | attachShadow + deklarativ | ~75% |
| Resource loading + bilder | lazy→eager, picture/srcset, bättre idle | ~75% |
| CSS gradients + complex styling | Fallback-färg istf transparent | ~80% |
| CSS Variables (var()) | Inte stödd (Chrome 40 target) | 0% |
| Canvas | Inte stödd | 0% |
| Custom fonts | Laddar via blitz_net, fallback risk | ~70% |

---

## Fas A: CSS Variables (0% → 100%) — QUICK WIN

**Problem**: `css_compiler.rs` använder `Browsers::chrome(40)` som LightningCSS-target.
Chrome 40 stödjer inte CSS custom properties → `var(--primary-color)` ignoreras →
alla sidor med modern CSS tappar färger, avstånd, typografi.

**Alla moderna sajter använder CSS variables** (GitHub, CNN, Apple, SVT, eBay).

### Steg A1: Höj LightningCSS browser-target
- **Fil**: `src/css_compiler.rs` rad 142-176
- **Ändring**: `chrome: Some(49 << 16)` → `chrome: Some(120 << 16)` (Chrome 120)
- Chrome 120 stödjer: CSS vars, @layer, :is(), :where(), nesting, color-mix()
- LightningCSS resolvar fortfarande `var()` till fallback-värden vid behov

### Steg A2: CSS Variable resolution fallback
- Om LightningCSS inte resolvar var() (saknar :root-block) → extrahera
  `:root { --x: value; }` → inline-ersätt `var(--x)` manuellt
- Ny funktion: `resolve_css_variables(html: &str) -> String`
- Hanterar: `var(--name)`, `var(--name, fallback)`, nästlade `var()`

### Steg A3: @container query polyfill
- Blitz stödjer inte container queries → konvertera till viewport-baserade @media
- `@container (min-width: 600px)` → `@media (min-width: 600px)` (approximation)

**Estimat**: ~50 rader kod. Största pixel-perfect-vinsten per rad.

---

## Fas B: Gradient Sanitization (80% → 95%)

**Problem**: Vi strippar ALLA gradienter med fallback-färg. Bättre: sanitize
gradient-stops så Vello inte kraschar, och behåll fungerande gradienter.

### Steg B1: Gradient validator
- **Fil**: `src/lib.rs`, ny funktion `sanitize_gradient()`
- Parsa gradient: extrahera color stops
- Vello kraschar på: <2 stops, identiska positioner, NaN-värden
- Fix: Om <2 stops → lägg till duplicerad stop. Om position saknas → interpolera.
- Om gradient passerar validering → BEHÅLL den (ingen strippning)
- Om den inte kan fixas → fallback-färg (befintlig logik)

### Steg B2: Gradient-typ-specifik hantering
- `linear-gradient(direction, stops...)` — behåll direction
- `radial-gradient(shape size at pos, stops...)` — förenklat till `radial-gradient(circle, stops...)`
- `conic-gradient` → fallback (Vello stödjer troligen inte)

### Steg B3: Testa med GitHub, CNN, Apple
- GitHub: mörk header-gradient → ska renderas som gradient, inte solid
- CNN: röd→mörkröd gradient-header → behållen
- Apple: subtila UI-gradienter → bevarade

**Estimat**: ~100 rader. Ger visuellt mycket bättre resultat på alla gradient-tunga sajter.

---

## Fas C: Image Loading (75% → 95%)

**Problem**: Bilder saknas fortfarande för:
1. `data:` placeholder + JS-driven src-swap (SVT-mönstret)
2. CSS `background-image: url()` laddas men renderas inte alltid
3. Bilder bakom auth/cookies (GitHub)

### Steg C1: resolve_lazy_images i render-pipeline
- **Fil**: `src/lib.rs`, `render_html_to_png()`
- `arena_dom::resolve_lazy_images()` finns redan men körs bara i parse, inte render
- Anropa det efter prepare_html_for_render, före CSS-compile
- Hanterar: `data-src`, `data-lazy-src`, `data-original`, `data-lazy`, `data-srcset`

### Steg C2: Utökad placeholder-detection
- **Fil**: `src/lib.rs`, `prepare_html_for_render()`
- Nuvarande: checkar `src="data:image/gif"` och `data:image/svg`
- Lägg till: `src="data:image/png;base64,iVBOR..."` (tiny PNG placeholders)
- Lägg till: `src=""` (tom src) + data-src
- Lägg till: `src` med tracking-pixel (1x1 eller <100 bytes base64)

### Steg C3: Cookie-forwarding till blitz_net
- **Fil**: `src/lib.rs` eller `src/bin/server.rs`
- `/api/fetch/render` hämtar redan HTML med cookies
- Men `blitz_net::Provider` laddar bilder UTAN cookies
- Lösning: Skicka session-cookies till blitz_net (om API stödjer det)
  eller pre-fetch bilder separat och injicera som data: URIs

### Steg C4: CSS background-image pre-fetch
- Extrahera `background-image: url(X)` från inlinad CSS
- Pre-fetch dessa bilder och konvertera till `background-image: url(data:...)`
- Eliminerar Blitz-nätverksberoende för CSS-bilder

**Estimat**: C1 ~20 rader, C2 ~30 rader, C3 ~50 rader, C4 ~80 rader.

---

## Fas D: Shadow DOM Slots (75% → 90%)

### Steg D1: Slot distribution
- **Fil**: `src/dom_bridge.rs`
- Implementera `<slot>` element som projicerar barn-noder från host
- `slot.assignedNodes()` → returnerar projicerade noder
- `<slot name="header">` → matcha `<div slot="header">`

### Steg D2: CSS scoping (begränsad)
- Shadow root styles ska inte läcka ut
- :host pseudo-class → matcha shadow host
- Skippa: ::slotted(), :host-context() (sällan använda)

### Steg D3: Rendering av shadow content
- I render-pipeline: om element har shadowRoot → rendera shadow-content
  istället för light DOM barn
- Behåll slot-projicering för visuell korrekthet

**Estimat**: D1 ~100 rader, D2 ~60 rader, D3 ~40 rader.

---

## Fas E: Font Pre-warming (70% → 95%)

### Steg E1: @font-face extraction
- Extrahera alla `@font-face { src: url(...) }` från CSS
- Pre-fetch font-filer parallellt med CSS-inlining
- Konvertera till `@font-face { src: url(data:font/woff2;base64,...) }`

### Steg E2: Font-display strategy
- Respektera `font-display: swap` → rendera med fallback, byt vid laddning
- `font-display: block` → vänta max 3s
- Default: `swap` (bättre rendering-tid)

### Steg E3: System font mapping
- Mappa vanliga webfonter till system-equivalenter:
  - Inter → system-ui
  - Roboto → Arial
  - Open Sans → Helvetica
- Använd som fallback om font-fetch failar

**Estimat**: E1 ~80 rader, E2 ~30 rader, E3 ~20 rader.

---

## Fas F: Canvas Fallback (0% → 50%)

### Steg F1: Canvas placeholder rendering
- Detektera `<canvas>` i HTML
- Om canvas har `width`/`height` → rendera grå placeholder-box
- Visa text "Canvas content" i boxen (bättre än tomt)

### Steg F2: Chart.js/D3 detection
- Om `<canvas>` + `chart.js` eller `d3` import detekteras
- Escalate till CDP-tier automatiskt (`TierHint::RequiresJs`)
- CDP kan faktiskt rendera Canvas

**Estimat**: F1 ~30 rader, F2 ~20 rader.

---

## Prioritering

| Prio | Fas | Vinst | Effort | ROI |
|------|-----|-------|--------|-----|
| 1 | **A: CSS Variables** | Alla moderna sajter | Liten | Extremt hög |
| 2 | **B: Gradient Sanitize** | Gradient-tunga sajter | Medel | Hög |
| 3 | **C1-C2: Image resolve** | SVT, lazy-load sajter | Liten | Hög |
| 4 | **E1: Font pre-warm** | Alla sajter med custom fonts | Medel | Medel |
| 5 | **C3-C4: Cookie + CSS-img** | Auth-sidor, CSS backgrounds | Medel | Medel |
| 6 | **D: Shadow DOM** | Web Component-sajter | Stor | Låg |
| 7 | **F: Canvas** | Chart/graph-sajter | Medel | Låg |

## Mål

**Fas A ensamt** tar oss till ~90% pixel-perfect (CSS vars är #1 orsak till broken rendering).
**Fas A+B+C** tar oss till ~95%.
**Alla fasar** tar oss till ~98% (100% kräver full browser-engine — vi är en lightweight renderer).
