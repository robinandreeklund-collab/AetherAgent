# Anti-Bot & Cookie Consent Bypass — Research 2026-03-24

## Sammanfattning

### Top 4 Prioriteter for AetherAgent

1. **TLS/HTTP2 Fingerprint Impersonation** — `rquest` crate (reqwest-fork med BoringSSL)
2. **Cookie Consent Pre-Setting** — TCF v2.2 consent string + CMP-specifika cookies
3. **DOM-baserad Consent Dismissal** — Semantisk knapp-sokning (redan i arkitekturen)
4. **CDP Stealth Patches** — JS-injection fore sidladdning i Tier 2

---

## 1. Anti-Bot Detection (2025-2026 State of the Art)

### 1.1 TLS Fingerprinting (JA3/JA4)

Anti-bot-tjanster fingeravtrycker TLS ClientHello: cipher suites, extensions, elliptiska kurvor och deras **ordning**. Default `reqwest` med `rustls` matchar INGEN riktig webblasare.

**Detektionshierarki:**
1. TLS/HTTP2 fingerprint (primart)
2. JS-miljocheck (navigator.webdriver, CDP-artefakter)
3. IP-reputation (sekundart)
4. Beteendeanalys (tertiart)

**Losning:** `rquest` crate — fork av reqwest som lankar mot BoringSSL (Chromes TLS-bibliotek):

```toml
[dependencies]
rquest = { version = "1.0", features = ["impersonate"] }
```

```rust
let client = rquest::Client::builder()
    .impersonate(rquest::Impersonate::Chrome126)
    .build()?;
```

Hanterar TLS fingerprint, HTTP/2 SETTINGS-ordning, och header-ordning i ett steg.

**Begransning:** BoringSSL ar C-beroende — fungerar for nativ/server men INTE for WASM. WASM anvander browserns nativa fetch anda.

### 1.2 HTTP/2 Fingerprinting

Utover TLS fingeravtryckar anti-bot HTTP/2:
- SETTINGS frame: initial window size, max concurrent streams, header table size + **ordning**
- WINDOW_UPDATE frame: initial varde
- PRIORITY frames: Chrome/Firefox/Safari har helt olika prioritetsscheman
- HEADERS pseudo-header ordning: `:method`, `:authority`, `:scheme`, `:path`

Default hyper/h2 (reqwest) skickar SETTINGS i en annan ordning an nagon riktig browser.

**Losning:** `rquest` hanterar detta automatiskt.

### 1.3 Header Order Fingerprinting

Cloudflare kollar header-ordning. Chrome skickar:
```
sec-ch-ua → sec-ch-ua-mobile → sec-ch-ua-platform → upgrade-insecure-requests → user-agent → accept → sec-fetch-* → accept-encoding → accept-language
```

Om User-Agent sager Chrome men headers ar i alfabetisk ordning = flaggad.

### 1.4 Cloudflare Turnstile

- Invisible JS-challenge som samlar browsersignaler
- TLS + HTTP/2 + JS-miljo + canvas + WebGL + timing
- **HTTP-only path kan inte losa Turnstile** — maste eskalera till CDP-tier
- Grundlaggande Cloudflare-check ("checking your browser") klaras av korrekt TLS+HTTP2

### 1.5 CDP Detection & Evasion

Anti-bot detekterar CDP genom:
- `navigator.webdriver === true`
- `window.chrome.cdc_*` — CDP-injicerade properties
- `Runtime.evaluate` stack traces
- Tomma `navigator.plugins`

**Stealth-patches (injiceras fore sidladdning):**
```javascript
Object.defineProperty(navigator, 'webdriver', { get: () => undefined });
window.chrome = { runtime: {}, loadTimes: function(){}, csi: function(){} };
// + permissions, plugins, languages, WebGL vendor patches
```

Injiceras via `Page.addScriptToEvaluateOnNewDocument`.

### 1.6 Anti-Bot Services

| Service | Primar detektion | Svarast att bypassa |
|---------|-----------------|---------------------|
| **Cloudflare** | TLS + HTTP/2 | Turnstile managed challenge |
| **Akamai** | HTTP/2 fingerprint, sensor JS | Sensor data collection |
| **DataDome** | Device fingerprint, beteende | ML pa mus/tangentbord |
| **HUMAN (PerimeterX)** | JS-challenges, biometri | Kontinuerlig beteendeovervakning |
| **Kasada** | Obfuskerad JS-challenge | Andras ofta |

---

## 2. Cookie Consent / GDPR

### 2.1 CMP Detection Patterns

| CMP | DOM-detektion | Accept-knapp |
|-----|---------------|-------------|
| **OneTrust** | `#onetrust-banner-sdk` | `#onetrust-accept-btn-handler` |
| **Cookiebot** | `#CybotCookiebotDialog` | `#CybotCookiebotDialogBodyLevelButtonLevelOptinAllowAll` |
| **Quantcast** | `.qc-cmp2-container` | `.qc-cmp2-summary-buttons button[mode="primary"]` |
| **CookieYes** | `.cky-consent-container` | `.cky-btn-accept` |
| **TrustArc** | `#truste-consent-track` | `#truste-consent-button` |
| **Didomi** | `#didomi-popup` | `#didomi-notice-agree-button` |
| **Usercentrics** | `#usercentrics-root` (shadow DOM) | `button[data-testid="uc-accept-all-button"]` |
| **Klaro** | `.klaro .cookie-modal` | `.cm-btn-success` |
| **Iubenda** | `#iubenda-cs-banner` | `.iubenda-cs-accept-btn` |
| **Complianz** | `.cmplz-cookiebanner` | `.cmplz-accept` |

### 2.2 IAB TCF v2.2 Consent String

Pre-satt `eupubconsent-v2` cookie med "accept all" = skippar consent-modalen helt.

TC-strangens struktur: base64url-kodad bitfalt med version, CMP ID, purposes consent, vendor consent.

```rust
// Pre-set fore navigation:
cookie_jar.add(Cookie::new("eupubconsent-v2", generate_tcf_accept_all()));
cookie_jar.add(Cookie::new("OptanonAlertBoxClosed", now_iso8601()));
```

### 2.3 CMP-Specifika Cookies

```
// OneTrust
OptanonConsent=groups=C0001:1,C0002:1,C0003:1,C0004:1

// Cookiebot
CookieConsent={necessary:true,preferences:true,statistics:true,marketing:true}

// CookieYes
cookieyes-consent=consent:yes,necessary:yes,functional:yes,analytics:yes

// Complianz
cmplz_consent_status=allow
```

### 2.4 Tiered Consent Strategy

```
Tier 1: Pre-set cookies (fore fetch)
  - TCF consent string
  - CMP-specifika acceptance-cookies

Tier 2: DOM detection + click (efter parse)
  - Detektera CMP fran DOM-struktur
  - Klicka accept-knapp via semantisk nod-matchning

Tier 3: JS injection (i sandbox)
  - Anropa __tcfapi('setConsent', ...)
  - Trigga CMPs accept-all API

Tier 4: Overlay removal (kosmetisk)
  - Ta bort modal overlay-divvar
  - Aterstall body overflow:hidden
```

---

## 3. Etik & Juridik

- **robots.txt**: Redan implementerat (Fas 8)
- **noai/noimageai meta tags**: Bor implementeras (ny standard)
- **AI-agents ar analogt med hjalpmedelsteknologi**: Auto-acceptera cookies ar vad anvandaren skulle gora
- **EU AI Act (aug 2025)**: Krav pa transparens i datainsamling
- **W3C**: Diskussioner om standardiserat maskinlasbart AI-samtycke

---

## 4. Implementationsplan

### Fas A: rquest-integration (Prioritet 1)
- Byt reqwest mot rquest i fetch.rs (feature-gatad)
- Chrome/Firefox impersonation
- Insats: 2-3 dagar

### Fas B: consent.rs modul (Prioritet 2)
- TCF v2.2 consent string generator
- CMP cookie pre-setting
- DOM-baserad CMP-detektion + accept-klick
- Insats: 3-5 dagar

### Fas C: CDP stealth (Prioritet 3)
- Stealth JS-patches i vision_backend.rs
- Page.addScriptToEvaluateOnNewDocument
- Insats: 1-2 dagar

### Fas D: noai meta tag (Prioritet 4)
- Parsa `<meta name="robots" content="noai">`
- Respektera i semantic layer
- Insats: 0.5 dagar

---

## Relevanta Crates

| Crate | Syfte | Status |
|-------|-------|--------|
| `rquest` | Browser fingerprint impersonation | Aktiv, rekommenderad |
| `boring` | BoringSSL-bindningar | Aktiv |
| `cookie_store` | Cookie jar | Stabil |
| `chromiumoxide` | CDP-klient | Aktiv |
| `base64` | TCF consent string | Stabil |
