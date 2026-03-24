# WPT Dashboard

> AetherAgent Web Platform Tests resultat per svit.
> Uppdaterad: 2026-03-24

## Resultat

| Suite | Cases | Passed | Rate | Status |
|-------|-------|--------|------|--------|
| **dom/** | 3,738 | 1,572 | **42.1%** | Aktivt arbete |
| **html/syntax/** | 588 | 123 | **20.9%** | html5ever parser |
| **domparsing/** | 457 | 25 | **5.5%** | DOMParser, innerHTML |
| **custom-elements/** | 930 | 18 | **1.9%** | customElements.define |
| **shadow-dom/** | 1,393 | 24 | **1.7%** | attachShadow, slots |
| **encoding/** | 331 | 0 | 0.0% | atob/btoa stubs |
| **webstorage/** | 7 | 0 | 0.0% | localStorage in-memory |
| **hr-time/** | 5 | 0 | 0.0% | performance.now() |
| **html/dom/** | ~2000 | — | — | Stack overflow vid batch-körning |
| **TOTAL** | **7,449** | **1,762** | **23.7%** | |

## Kör tester

```bash
# Setup
./wpt/setup.sh

# Kör en svit
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/

# Kör specifik fil
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/Node-cloneNode.html --verbose

# Alla sviter (tar ~5 min)
for suite in dom domparsing encoding webstorage custom-elements hr-time shadow-dom html/syntax; do
  cargo run --bin aether-wpt --features js-eval -- wpt-suite/$suite/
done
```

## Analys per svit

### dom/ (42.1%) — Starkast
- CharacterData: ~100% (alla 6 testfiler)
- ChildNode (before/after/replaceWith): 100%
- Node-cloneNode: 93%
- Document-getElementById: 78%
- querySelector-escapes: 91%
- Document-createEvent: 94%

### html/syntax/ (20.9%) — HTML parsing
- html5ever-baserad parser ger bra bas
- Saknar: template-parsing, foreign content (SVG/MathML), encoding detection

### domparsing/ (5.5%) — DOMParser
- Grundläggande `parseFromString()` fungerar
- Saknar: XMLSerializer, Range.createContextualFragment

### custom-elements/ (1.9%)
- `customElements.define/get/whenDefined` fungerar
- Saknar: connectedCallback, disconnectedCallback, attributeChangedCallback, adoptedCallback

### shadow-dom/ (1.7%)
- `attachShadow({mode: "open"})` fungerar
- Saknar: slot distribution, event retargeting, composed: true traversal

### encoding/ (0.0%)
- `atob/btoa` finns men testerna kräver `TextEncoder`/`TextDecoder`
- Behöver: TextEncoder, TextDecoder, encoding labels

### webstorage/ (0.0%)
- localStorage/sessionStorage finns (in-memory)
- Testerna kräver troligen storage events och cross-window access

### hr-time/ (0.0%)
- `performance.now()` finns
- Testerna kräver troligen `PerformanceObserver`, `performance.timeOrigin`

## Prioritering framåt

### Snabbaste vinster (effort vs impact)
1. **encoding/** — Implementera `TextEncoder`/`TextDecoder` i Rust → ~100+ pass
2. **webstorage/** — Fixa storage event dispatching → ~7 pass
3. **hr-time/** — Lägg till `performance.timeOrigin` → ~5 pass
4. **domparsing/** — Förbättra DOMParser, XMLSerializer → ~50 pass
5. **custom-elements/** — Lifecycle callbacks → ~100 pass

### Långsiktigt
6. **shadow-dom/** — Slot distribution, composed events → ~200 pass
7. **html/dom/** — Kräver stack overflow fix i runnern
