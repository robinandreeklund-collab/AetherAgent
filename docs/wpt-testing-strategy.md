# WPT Testing Strategy — AetherAgent

> Huvudstrategi för hur vi använder Web Platform Tests för att bygga kvalitet i AetherAgent.
> Senast uppdaterad: 2026-03-25

## Översikt

AetherAgent testar mot [Web Platform Tests](https://github.com/web-platform-tests/wpt) —
webbens officiella spec-tester som alla browsers använder. Genom att köra WPT mot vår
DOM-implementation mäter vi exakt hur spec-kompatibel AetherAgent är och var vi behöver förbättra.

### Princip: Polyfill → Verify → Migrate → Native

```
1. POLYFILL  — Implementera API i JS (wpt/polyfills.js) för att snabbt se WPT-score
2. VERIFY    — Kör WPT-tester, bekräfta att integrationen fungerar (pass-rate stabil)
3. MIGRATE   — Flytta till native Rust i dom_bridge.rs / arena_dom.rs
4. NATIVE    — Ta bort polyfill, verifiera att WPT-score inte reggressar
```

**Viktigt:** Polyfills är temporära. Varje polyfill MÅSTE migreras till Rust.
Se `docs/dom-implementation-status.md` för aktuell status på varje API.

---

## Test-suiter: Mappning till AetherAgent

### Tier 1 — Kärna (kör ALLTID, varje PR)

Dessa testar AetherAgents primära funktionalitet och ska ha hög pass-rate.

| Suite | Filer | AetherAgent-koppling | Mål |
|-------|-------|---------------------|-----|
| `dom/nodes/` | ~254 | Core DOM: createElement, appendChild, cloneNode, textContent, querySelector | >85% |
| `dom/traversal/` | ~16 | TreeWalker, NodeIterator — används i semantic tree building | >80% |
| `dom/collections/` | ~9 | HTMLCollection, NodeList — getElementsBy* returntyper | >75% |
| `dom/events/` | ~156 | addEventListener, dispatchEvent, bubbling, capture | >70% |
| `dom/ranges/` | ~55 | Range API — 4,000+ testcases, störst enskild svit | >75% |

**Total Tier 1:** ~490 filer, ~20,000+ testcases

### Tier 2 — Stödjande (kör vid relevanta ändringar)

| Suite | Filer | AetherAgent-koppling | Mål |
|-------|-------|---------------------|-----|
| `dom/abort/` | ~5 | AbortController/AbortSignal — fetch integration | >50% |
| `dom/lists/` | ~5 | DOMTokenList (classList) | >80% |
| `domparsing/` | ~49 | DOMParser, innerHTML, outerHTML — parser integration | >40% |
| `html/syntax/` | ~319 | HTML5 parsing — html5ever baseline | >30% |
| `html/dom/` | ~463 | HTML-specifika DOM-APIer | >30% |
| `css/selectors/` | ~636 | CSS selector matching — querySelector/matches/closest | >60% |

**Total Tier 2:** ~1,477 filer

### Tier 3 — Utökad (kör vid milstolpar)

| Suite | Filer | AetherAgent-koppling | Mål |
|-------|-------|---------------------|-----|
| `html/semantics/` | ~2,803 | HTML element-semantik, formulär, tabeller | >20% |
| `encoding/` | ~153 | TextEncoder/TextDecoder, atob/btoa | >30% |
| `webstorage/` | ~25 | localStorage/sessionStorage | >60% |
| `xhr/` | ~53 | XMLHttpRequest — XHR interception layer | >30% |
| `fetch/` | ~258 | Fetch API — reqwest-baserad fetch | >20% |
| `css/cssom/` | ~210 | CSSOM — getComputedStyle, style manipulation | >20% |
| `hr-time/` | ~10 | performance.now() | >50% |
| `console/` | ~6 | console.log/warn/error | >70% |
| `url/` | ~4 | URL constructor | >50% |

**Total Tier 3:** ~3,522 filer

### Tier 4 — Framtida (ej prioriterade nu)

| Suite | Filer | Status |
|-------|-------|--------|
| `custom-elements/` | ~172 | Stubbat — customElements.define |
| `shadow-dom/` | ~295 | Stubbat — attachShadow |
| `html/webappapis/timers/` | ~4 | setTimeout/setInterval via event loop |

---

## Arbetsmodell: Pyramiden

```
                    ╔════════════════╗
                    ║   Hela dom/    ║  ← Kör när subdirektiv >80%
                    ╠════════════════╣
                ╔═══╩════════════════╩═══╗
                ║  dom/nodes + events +  ║  ← Kör per subkategori
                ║  ranges + traversal    ║
                ╠════════════════════════╣
            ╔═══╩════════════════════════╩═══╗
            ║  Enskilda testfiler med         ║  ← Börja här
            ║  --verbose för detaljerad debug ║
            ╚════════════════════════════════╝
```

### Steg-för-steg arbetsordning

**Fas A: Fokuserad kvalitet (per subkategori)**

1. Välj EN subkategori (t.ex. `dom/nodes/`)
2. Kör med `--verbose` för att se varje testcase
3. Identifiera failure-patterns (vad saknas?)
4. Implementera fix (polyfill först om snabb vinst, native om möjligt)
5. Kör om — bekräfta förbättring
6. Uppdatera dashboard med ny score

**Fas B: Bredare körning (per svit)**

När en subkategori uppnår >75% pass-rate:
1. Kör hela sviten (t.ex. `dom/`)
2. Identifiera tvärsgående problem
3. Fixa systematiska fel

**Fas C: Full regression (alla sviter)**

Vid milstolpar (varje vecka eller major release):
1. Kör alla Tier 1 + Tier 2 sviter
2. Jämför med förra milstolpen
3. Dokumentera delta i dashboard

---

## Prioriteringsmatris: Impact vs Effort

### 🟢 Snabbaste vinsterna (låg effort, hög impact)

| Åtgärd | Svit | Uppskattade nya pass | Effort |
|--------|------|---------------------|--------|
| Fixa Range.compareBoundaryPoints edge cases | dom/ranges | +500-1000 | Medium |
| Fixa Range.set offset-validering | dom/ranges | +500 | Medium |
| Implementera prepend/append/replaceChildren | dom/nodes | +50-100 | Låg |
| TextEncoder/TextDecoder | encoding/ | +100 | Låg |
| Fixa NodeIterator ProcessingInstruction | dom/traversal | +300 | Medium |

### 🟡 Medium effort, hög impact

| Åtgärd | Svit | Uppskattade nya pass | Effort |
|--------|------|---------------------|--------|
| Range API → Native Rust | dom/ranges | +1000 (stabilitet) | Hög |
| CSS selectors: +, ~, :has(), :is() | css/selectors | +200 | Medium |
| Event subclasses (MouseEvent etc.) | dom/events | +100 | Medium |
| DOMException → Native Rust | alla | +50 (renare errors) | Medium |

### 🔴 Stor effort, strategisk

| Åtgärd | Svit | Uppskattade nya pass | Effort |
|--------|------|---------------------|--------|
| Shadow DOM distribution | shadow-dom/ | +200 | Hög |
| Custom Elements lifecycle | custom-elements/ | +100 | Hög |
| Full namespace-stöd | dom/nodes | +100 | Hög |

---

## Migrationsordning: Polyfill → Rust

### Redan migrerat (Fas 1-2, mars 2026)

| API | Migrationsdatum | WPT-impact |
|-----|-----------------|-----------|
| element.remove() | 2026-03-24 | ChildNode-tester 100% |
| before()/after()/replaceWith() | 2026-03-24 | ChildNode-tester 100% |
| toggleAttribute() | 2026-03-24 | +20 pass |
| CharacterData (alla metoder) | 2026-03-24 | ~100% på CharacterData-tester |
| ownerDocument | 2026-03-25 | +500 pass |
| compareDocumentPosition | 2026-03-25 | 84% pass-rate |
| classList live DOMTokenList | 2026-03-25 | +200 pass |
| addEventListener options | 2026-03-25 | Event-tester stabilare |
| MutationObserver | 2026-03-25 | Observer-tester |

### Nästa migration (Fas 3)

1. **Range API** — Högsta prioritet. ~4,000 testcases. Flytta från polyfills.js till dom_bridge.rs
2. **DOMException** — Ren error-hantering, påverkar alla sviter
3. **prepend/append/replaceChildren** — ParentNode convenience, enkla att migrera
4. **Event subclasses** — MouseEvent, KeyboardEvent med korrekta properties

### Framtida migration (Fas 4+)

5. Namespace-metoder (setAttributeNS etc.)
6. NamedNodeMap (element.attributes)
7. document.implementation (createHTMLDocument etc.)
8. document.createEvent()

---

## Verktyg och kommandon

### Daglig utveckling

```bash
# Kör specifik subkategori med detaljer
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose

# Kör specifik testfil
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/Document-getElementById.html --verbose

# Filtrera tester
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/ --filter querySelector

# JSON-output för CI
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --json
```

### PR-validering

```bash
# Tier 1 baseline (OBLIGATORISKT per PR)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/events/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/ranges/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/traversal/

# Tier 2 (vid relevanta ändringar)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/css/selectors/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/domparsing/
```

### Milstolpe-körning

```bash
# Full körning alla sviter
for suite in dom domparsing encoding webstorage hr-time console url css/selectors css/cssom xhr html/syntax; do
  echo "=== $suite ==="
  cargo run --bin aether-wpt --features js-eval -- wpt-suite/$suite/ --json
done
```

---

## Mätetal och mål

### Kortsiktigt (Q2 2026)

| Svit | Nu | Mål | Delta |
|------|----|-----|-------|
| dom/ | 67.1% | 80% | +13pp |
| dom/nodes/ | ~75% | 90% | +15pp |
| dom/ranges/ | ~70% | 85% | +15pp |
| css/selectors/ | TBD | 60% | — |
| domparsing/ | 5.5% | 30% | +25pp |

### Långsiktigt (Q4 2026)

| Svit | Mål |
|------|-----|
| dom/ | >90% |
| html/syntax/ | >50% |
| css/selectors/ | >80% |
| encoding/ | >60% |
| Total | >75% |

---

## Regler

1. **WPT-score får ALDRIG gå ner utan dokumenterad motivering**
2. **Varje PR som rör DOM/JS/CSS MÅSTE inkludera WPT before/after**
3. **Polyfills är temporära** — planera alltid migration till Rust
4. **Testa subkategori först** — kör inte hela dom/ om du bara ändrat Range
5. **Uppdatera dashboard** — efter varje signifikant förändring
6. **Native first** — implementera nya API:er direkt i Rust, inte som polyfill
