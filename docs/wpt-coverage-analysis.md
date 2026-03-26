# WPT Coverage Analysis — AetherAgent

> Djupanalys: vilka WPT-suiter testar AetherAgents implementerade APIs?
> Datum: 2026-03-26

## Sammanfattning

AetherAgent implementerar **40+ DOM APIs**, CSS selectors, events, Range, TreeWalker,
MutationObserver, QuickJS sandbox med event loop, HTTP fetch, XHR-interception, och mer.

**WPT-repot har 299 kataloger.** Vi testar 16. Denna analys identifierar **27+ nya
suiter** som direkt testar APIs vi redan implementerar.

---

## Nuvarande täckning (16 suiter)

```
dom/              — Core DOM (nodes, events, ranges, traversal, collections, lists, abort)
domparsing/       — DOMParser, innerHTML, outerHTML, XMLSerializer
encoding/         — TextEncoder/TextDecoder
webstorage/       — localStorage/sessionStorage
custom-elements/  — customElements.define (stub)
shadow-dom/       — attachShadow (basic)
html/dom/         — HTML-specifika DOM APIs
html/syntax/      — HTML5 parsing edge cases
html/semantics/   — HTML element semantik
html/webappapis/  — setTimeout, setInterval
console/          — console.log/warn/error
hr-time/          — performance.now()
url/              — URL constructor
css/selectors/    — CSS selector matching
css/cssom/        — CSS Object Model
xhr/              — XMLHttpRequest
fetch/            — Fetch API
```

---

## NYA suiter att lägga till — ordnade per prioritet

### Tier A — Direkt relevant, hög impact (testar saker vi HAR)

| Suite | Testar | AetherAgent-implementation | Förväntad score |
|-------|--------|---------------------------|----------------|
| **uievents/** | UIEvent, MouseEvent, KeyboardEvent, FocusEvent, InputEvent, WheelEvent konstruktorer + properties | Alla event-konstruktorer i `dom_bridge/events.rs` + polyfills | 30-50% |
| **pointerevents/** | PointerEvent, pointerdown/up/move/over/out | PointerEvent-konstruktor med pointerId, pressure, tiltX/Y | 10-30% |
| **focus/** | focus(), blur(), autofocus, tabindex, activeElement | `focus()`/`blur()` i events.rs, `activeElement` getter | 20-40% |
| **selection/** | window.getSelection(), Selection API, Range integration | getSelection stub, Range API native | 5-15% |
| **webidl/** | WebIDL type coercion, DOMString, unsigned long, toString, valueOf | Grundläggande i all DOM-interaktion | 10-30% |
| **ecmascript/** | ES features i DOM-kontext, Symbol, Promise, generators | QuickJS stöder ES2020+ | 20-40% |
| **wai-aria/** | ARIA roles, states, properties (aria-label, aria-hidden, etc.) | Kärna i semantic tree builder | 10-20% |
| **accname/** | Accessible Name Computation (text alternatives) | `extract_label_with_text()` i arena_dom.rs | 5-15% |
| **html-aam/** | HTML Accessibility API Mappings (vilken roll har `<button>`?) | `infer_role_with_text()` i arena_dom.rs | 5-15% |
| **css/css-cascade/** | CSS cascade, specificity, !important, inheritance | `css_cascade.rs`, LightningCSS | 10-30% |

### Tier B — Relevant, medium impact

| Suite | Testar | AetherAgent-implementation | Förväntad score |
|-------|--------|---------------------------|----------------|
| **input-events/** | InputEvent, beforeinput, data property | InputEvent-konstruktor i polyfills | 10-20% |
| **touch-events/** | TouchEvent, touchstart/move/end | TouchEvent-stub i polyfills | 5-10% |
| **editing/** | contenteditable, execCommand, Selection | Delvis via innerHTML | 5-10% |
| **css/css-values/** | CSS units (px, em, rem, %), calc() | style.setProperty, getComputedStyle | 5-15% |
| **css/css-display/** | display property, visibility, none/block/flex | display:none-filtrering i parser.rs | 10-20% |
| **webmessaging/** | postMessage, MessageEvent, MessageChannel | MessageChannel-stub i dom_bridge | 5-10% |
| **domxpath/** | XPath (document.evaluate) | Ej implementerat — men enkel stub ger 0% baseline | 0% |
| **inert/** | inert attribute — blockar fokus/events | Kan ge snabba vinster | 5-15% |
| **quirks/** | Quirks mode parsing | html5ever hanterar det | 10-20% |
| **trusted-types/** | TrustedTypes, innerHTML-begränsningar | Säkerhetsrelevant, trust shield | 5-10% |
| **sanitizer-api/** | Sanitizer API (ny) | Relaterat till trust.rs | 0-5% |
| **svg/** | SVG DOM, SVG parsing | html5ever parsar SVG, vi har SVG-namespace | 5-10% |
| **FileAPI/** | Blob, File, FileReader | Blob-stub i dom_bridge | 0-5% |
| **requestidlecallback/** | requestIdleCallback | Kan stubbas enkelt | 0-10% |

### Tier C — Kompletterande, låg effort

| Suite | Testar | AetherAgent-implementation | Förväntad score |
|-------|--------|---------------------------|----------------|
| **core-aam/** | Core Accessibility API Mappings | ARIA/role | 5-10% |
| **mathml/** | MathML element parsing | html5ever + namespace | 5-10% |
| **streams/** | ReadableStream, WritableStream | Streaming parse? | 0-5% |
| **compression/** | CompressionStream (gzip, deflate) | fetch.rs har brotli/gzip | 0% |
| **user-timing/** | performance.mark/measure | Enkel stub | 0-5% |
| **performance-timeline/** | PerformanceObserver | Enkel stub | 0-5% |
| **html/infrastructure/** | HTML infrastructure tester | Grundläggande | 5-15% |
| **css/css-color/** | CSS colors, named colors, hsl() | style-hantering | 0-5% |
| **css/css-flexbox/** | Flexbox layout | Blitz+Taffy | 0% |

---

## API-mappning: AetherAgent-modul → WPT-suite

| AetherAgent-modul | Fil | WPT-suiter |
|-------------------|-----|------------|
| DOM Core | `dom_bridge/mod.rs` | dom/nodes/, dom/collections/ |
| DOM Events | `dom_bridge/events.rs` | dom/events/, **uievents/**, **pointerevents/**, **touch-events/**, **input-events/** |
| DOM Attributes | `dom_bridge/attributes.rs` | dom/nodes/ (attribut-tester) |
| DOM Node Ops | `dom_bridge/node_ops.rs` | dom/nodes/ (appendChild, insertBefore, etc.) |
| CSS Selectors | `dom_bridge/selectors.rs` | css/selectors/ |
| CSS Cascade | `css_cascade.rs` | css/cssom/, **css/css-cascade/** |
| Style/classList | `dom_bridge/style.rs` | dom/lists/, css/cssom/ |
| CharacterData | `dom_bridge/chardata.rs` | dom/nodes/ (CharacterData-*) |
| Window/Console | `dom_bridge/window.rs` | console/, hr-time/, **user-timing/** |
| Event Loop | `event_loop.rs` | html/webappapis/timers/ |
| HTML Parser | `parser.rs` | html/syntax/, domparsing/ |
| Arena DOM | `arena_dom.rs` | dom/nodes/, dom/traversal/ |
| Range API | (inline JS + Rust) | dom/ranges/ |
| Trust Shield | `trust.rs` | (**trusted-types/**, **sanitizer-api/**) |
| Semantic Builder | `semantic.rs` | **wai-aria/**, **accname/**, **html-aam/** |
| Fetch | `fetch.rs` | fetch/, xhr/ |
| JS Sandbox | `js_eval.rs` | **ecmascript/**, **webidl/** |
| Hydration | `hydration.rs` | (inget direkt WPT, men html/semantics/) |
| Focus | `dom_bridge/events.rs` | **focus/**, **selection/** |
| Storage | `dom_bridge/window.rs` | webstorage/ |

---

## Konkreta nästa steg

### 1. Uppdatera setup.sh (KLAR)
Utökad från 16 → ~45 suiter.

### 2. Kör baseline på nya suiter
```bash
rm -rf wpt-suite && bash wpt/setup.sh
for suite in uievents pointerevents focus selection webidl ecmascript \
             wai-aria accname html-aam css/css-cascade css/css-display \
             input-events touch-events editing domxpath quirks inert \
             trusted-types webmessaging; do
  echo "=== $suite ==="
  cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/$suite/ 2>&1 | tail -5
done
```

### 3. Prioritera implementation baserat på resultat
- Suiter med 20-50% → Low-hanging fruit, fixa failures
- Suiter med 0-5% → Behöver grundläggande implementation
- Suiter med >50% → Polera och optimera

---

## Vad vi INTE behöver testa

Dessa WPT-suiter är **irrelevanta** för AetherAgent (browser-specifika, hardware, nätverk):

| Suite | Anledning |
|-------|-----------|
| webgl/, webgpu/ | GPU rendering — vi har Blitz/CDP |
| webaudio/ | Audio API — ej relevant |
| webrtc/ | Real-time communication |
| webauthn/ | Authentication hardware |
| bluetooth/, webusb/, webhid/ | Hardware APIs |
| service-workers/, workers/ | Bakgrundstrådar — QuickJS är single-threaded |
| payment-request/ | Betalnings-API |
| mediacapture-*/ | Kamera/mikrofon |
| gamepad/ | Controller-input |
| geolocation/ | GPS |
| speech-api/ | Talsyntes/igenkänning |
| notifications/ | Push-notiser |
| IndexedDB/ | Databas — ej implementerad |
| webdriver/ | Browser automation protocol |
| websockets/ | WebSocket — ej implementerad |
| screen-orientation/, screen-wake-lock/ | Skärm-APIs |
| battery-status/ | Batteri-API |
| vibration/ | Vibrations-API |
| fullscreen/ | Fullscreen API |

---

## Sammanfattning

**Före:** 16 WPT-suiter, ~26 700 testcases
**Efter:** ~45 WPT-suiter, uppskattningsvis ~50 000+ testcases

De viktigaste nya suiterna:
1. **uievents/** + **pointerevents/** — Event-systemet vi redan byggt
2. **wai-aria/** + **accname/** + **html-aam/** — Kärnan i AetherAgents semantic tree
3. **webidl/** + **ecmascript/** — Grundläggande JS/DOM-interaktion
4. **focus/** + **selection/** — Interaktion vi redan stöder
5. **css/css-cascade/** — CSS-motorn vi redan har
