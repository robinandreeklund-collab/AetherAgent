# WPT Dashboard

> AetherAgent Web Platform Tests resultat per svit.
> Uppdaterad: 2026-03-25

## Resultat

| Suite | Cases | Passed | Rate | Status |
|-------|-------|--------|------|--------|
| **dom/** | 19,938 | 13,383 | **67.1%** | Aktivt arbete |
| **html/syntax/** | 588 | 123 | **20.9%** | html5ever parser |
| **domparsing/** | 457 | 25 | **5.5%** | DOMParser, innerHTML |
| **custom-elements/** | 930 | 18 | **1.9%** | customElements.define |
| **shadow-dom/** | 1,393 | 24 | **1.7%** | attachShadow, slots |
| **encoding/** | 331 | 0 | 0.0% | atob/btoa stubs |
| **webstorage/** | 7 | 0 | 0.0% | localStorage in-memory |
| **hr-time/** | 5 | 0 | 0.0% | performance.now() |
| **html/dom/** | ~2000 | — | — | Stack overflow vid batch-körning |
| **TOTAL** | **23,649** | **13,573** | **57.4%** | |

### Historik dom/

| Datum | Cases | Passed | Rate | Kommentar |
|-------|-------|--------|------|-----------|
| 2026-03-24 | 2,004 | 1,382 | 69.0% | Baseline (CLAUDE.md) |
| 2026-03-25 | 19,938 | 13,383 | 67.1% | +12,001 pass, 10x fler testfall |

**Notering:** Pass-raten sjönk marginellt (69% → 67%) men testfallen ökade 10x.
Range-tester som tidigare timade ut (5s) körs nu med 30s timeout. Absolut antal pass: **+12,001**.

## Kör tester

```bash
# Setup
./wpt/setup.sh

# Kör en svit
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/

# Kör specifik fil
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/Node-cloneNode.html --verbose

# Alla sviter (tar ~10 min)
for suite in dom domparsing encoding webstorage custom-elements hr-time shadow-dom html/syntax; do
  cargo run --bin aether-wpt --features js-eval -- wpt-suite/$suite/
done
```

## Analys per svit

### dom/ (67.1%) — Starkast

#### Topp-testfiler (flest pass)

| Fil | Pass | Fail | Rate |
|-----|------|------|------|
| Range-comparePoint | 2,323 | 248 | 90.4% |
| Range-isPointInRange | 1,900 | 572 | 76.9% |
| Range-intersectsNode | 2,170 | 186 | 92.1% |
| Node-compareDocumentPosition | 1,215 | 229 | 84.1% |
| Node-contains | 1,358 | 124 | 91.6% |
| Range-set | 1,049 | 1,115 | 48.5% |
| Range-compareBoundaryPoints | 868 | 1,289 | 40.2% |
| Element-classlist | 895 | 525 | 63.0% |
| TreeWalker | 487 | 274 | 64.0% |
| Document-createEvent | 261 | 18 | 93.6% |
| DOMTokenList-coverage | 168 | 7 | 96.0% |
| Range-collapse | 168 | 18 | 90.3% |

#### Hög pass-rate (>90%)
- CharacterData: ~100% (alla 6 testfiler)
- ChildNode (before/after/replaceWith): 100%
- Node-cloneNode: 93%
- Document-getElementById: 78%
- querySelector-escapes: 91%
- Document-createEvent: 94%
- Range-intersectsNode: 92%
- Node-contains: 92%
- Range-collapse: 90%

### Största kvarvarande failures

| Fil | Fail | Orsak |
|-----|------|-------|
| Range-compareBoundaryPoints | 1,289 | Detached range, boundary edge cases |
| Range-set | 1,115 | Offset-validering, boundary constraints |
| Element-classlist | 525 | Live index-uppdatering, attribute sync |
| NodeIterator | 764 | common.js testNodes kräver processPI |
| Range-isPointInRange | 572 | Timeout (30s) |
| TreeWalker | 274 | Filter-hantering, whatToShow edge cases |
| Range-comparePoint | 248 | Timeout (30s) |

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

## Prioritering framåt

### Nästa steg (störst impact)
1. **Range API → Rust** — 4,000+ testfall, migrera från JS polyfill
2. **DOMException → Rust** — renare error-hantering
3. **NodeIterator/TreeWalker** — fixa common.js-stöd (createProcessingInstruction)
4. **Event subclasses** — MouseEvent/KeyboardEvent med properties

### Snabbaste vinster (effort vs impact)
5. **encoding/** — Implementera `TextEncoder`/`TextDecoder` → ~100+ pass
6. **custom-elements/** — Lifecycle callbacks → ~100 pass
7. **domparsing/** — Förbättra DOMParser, XMLSerializer → ~50 pass

### Långsiktigt
8. **shadow-dom/** — Slot distribution, composed events → ~200 pass
9. **html/dom/** — Kräver stack overflow fix i runnern
