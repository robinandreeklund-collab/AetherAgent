# WPT Workflow Guide — Steg-för-steg

> Denna guide följer du VARJE gång du jobbar med WPT-tester i AetherAgent.
> Referera alltid hit innan du börjar en ny session.

## Snabbstart

```bash
# 1. Setup (första gången)
./wpt/setup.sh

# 2. Kör tester mot den subkategori du jobbar med
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose

# 3. Se detaljerat resultat
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/Document-getElementById.html --verbose
```

---

## Arbetsflöde: Förbättra WPT-score

### Steg 1: Välj fokusområde

Titta i `docs/wpt-dashboard.md` och välj den subkategori som ger störst impact:

**Prioriteringsordning:**
1. Subkategori med flest failures men redan delvis fungerande (>30% pass)
2. Subkategori som blockerar andra tester
3. Subkategori med enklast fix (låg effort)

### Steg 2: Kör baseline

```bash
# Spara baseline INNAN du gör ändringar
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --json > /tmp/wpt-before.json

# Eller anteckna summary-raden:
# dom/nodes: 1234/1500 passed (82.3%)
```

### Steg 3: Analysera failures

```bash
# Kör med --verbose för att se varje enskilt testcase
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose 2>&1 | grep "FAIL"

# Kör specifik fil som failar
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/Node-cloneNode.html --verbose
```

**Vanliga failure-mönster:**

| Mönster | Orsak | Lösning |
|---------|-------|---------|
| `TypeError: X is not a function` | API saknas | Implementera i dom_bridge.rs eller polyfills.js |
| `X is undefined` | Property saknas | Lägg till getter i dom_bridge.rs |
| `assert_equals: got null expected "..."` | Returnerar fel värde | Fixa logik i Rust-implementation |
| `assert_throws_dom` | DOMException saknas | Lägg till felhantering |
| `TIMEOUT` | Oändlig loop eller för långsam | Optimera eller öka timeout |

### Steg 4: Implementera fix

**Beslutsträd: Polyfill eller Native?**

```
Är API:et enkelt och fristående?
├── JA → Implementera direkt i Rust (dom_bridge.rs)
│         Exempel: prepend(), append(), replaceChildren()
│
└── NEJ, komplext med många edge cases?
    ├── Finns redan i polyfills.js?
    │   ├── JA → Fixa i polyfills.js, planera Rust-migration
    │   └── NEJ → Lägg till polyfill TEMPORÄRT, skapa migrations-task
    │
    └── Är det en helt ny API-yta?
        └── Implementera i Rust från start (native first)
```

**Var implementerar du?**

| Fil | Vad |
|-----|-----|
| `src/dom_bridge.rs` | JS ↔ Rust DOM bridge — alla DOM-metoder exponerade till QuickJS |
| `src/arena_dom.rs` | ArenaDom — underliggande DOM-datastruktur och operationer |
| `src/event_loop.rs` | Event loop — setTimeout, rAF, MutationObserver |
| `src/js_eval.rs` | JS sandbox — eval, säkerhetsfilter |
| `wpt/polyfills.js` | Temporära JS-polyfills (MIGRERA TILL RUST) |

### Steg 5: Verifiera förbättring

```bash
# Kör samma suite igen
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --json > /tmp/wpt-after.json

# Jämför
echo "Before: $(cat /tmp/wpt-before.json | grep passed)"
echo "After:  $(cat /tmp/wpt-after.json | grep passed)"
```

### Steg 6: Kör regressionskontroll

```bash
# Kör ALLA Tier 1 tester — ingen regression tillåten
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/events/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/ranges/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/traversal/

# Plus standard CI
cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

### Steg 7: Uppdatera dokumentation

1. Uppdatera `docs/wpt-dashboard.md` med nya scores
2. Om polyfill → Rust migration: uppdatera `docs/dom-implementation-status.md`
3. Commit-meddelande format:

```
feat: implement prepend/append/replaceChildren native

WPT Results:
- dom/nodes: 1234/1500 (82.3%) → 1290/1500 (86.0%)
- Nya pass: Node-appendChild, ParentNode-prepend, ParentNode-append
- Regression: inga
```

---

## Arbetsflöde: Migrera Polyfill → Rust

### Steg 1: Identifiera polyfill

Läs `wpt/polyfills.js` och hitta den polyfill du vill migrera.
Notera exakt vilka metoder/properties den implementerar.

### Steg 2: Kör baseline med polyfill aktiv

```bash
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose > /tmp/baseline-with-polyfill.txt
```

### Steg 3: Implementera i Rust

Lägg till implementation i `src/dom_bridge.rs`:

```rust
// I register_element_methods() eller liknande
// Implementera metoden som en QuickJS-funktion
```

### Steg 4: Ta bort polyfill

Ta bort motsvarande kod i `wpt/polyfills.js`.

### Steg 5: Verifiera — score får INTE gå ner

```bash
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose > /tmp/after-migration.txt

# Jämför pass-counts
diff <(grep "passed" /tmp/baseline-with-polyfill.txt) <(grep "passed" /tmp/after-migration.txt)
```

### Steg 6: Uppdatera docs

I `docs/dom-implementation-status.md`:
- Flytta API:et från "Polyfill" till "Native"
- Lägg till migrationsdatum

---

## Checklista: Innan PR

```
□ Baseline WPT-score noterad (before)
□ Ändringar implementerade
□ WPT-score efter (after) — ingen regression
□ cargo test PASS
□ cargo clippy -- -D warnings PASS
□ cargo fmt --check PASS
□ docs/wpt-dashboard.md uppdaterad
□ docs/dom-implementation-status.md uppdaterad (om migration)
□ Commit-meddelande innehåller WPT before/after
```

---

## Vanliga kommandon — Referens

```bash
# === SETUP ===
./wpt/setup.sh                    # Ladda ner WPT-tester

# === KÖR TESTER ===
# Hel svit
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/

# Subkategori
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/

# Specifik fil
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/Node-cloneNode.html

# Med verbose (varje testcase)
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --verbose

# JSON-output
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/nodes/ --json

# Filtrering
cargo run --bin aether-wpt --features js-eval -- wpt-suite/dom/ --filter querySelector

# === ALLA SVITER ===
for suite in dom/nodes dom/events dom/ranges dom/traversal dom/collections \
             dom/abort dom/lists domparsing html/syntax html/dom \
             css/selectors css/cssom encoding webstorage xhr hr-time console url; do
  echo "=== $suite ==="
  cargo run --bin aether-wpt --features js-eval -- wpt-suite/$suite/
done

# === CI PIPELINE ===
cargo test && cargo clippy -- -D warnings && cargo fmt --check
```

---

## Felsökning

### "Stack overflow" vid stor svit

Kör subkategorier istället för hela sviten:
```bash
# Istället för: wpt-suite/html/dom/  (stack overflow)
# Kör:
cargo run --bin aether-wpt --features js-eval -- wpt-suite/html/dom/elements/
cargo run --bin aether-wpt --features js-eval -- wpt-suite/html/dom/documents/
```

### "TIMEOUT" på specifik fil

Filen har tester som kräver async-operationer som tar för lång tid.
- Kontrollera om testet kräver nätverks-access (inte stött)
- Kontrollera om testet har infinite loop
- Öka timeout i runner om motiverat

### Noll testcases detekterade

- Kontrollera att filen har `<script src="/resources/testharness.js">`
- Kontrollera att polyfills.js laddas korrekt
- Kör med `--verbose` för att se eventuella JS-fel

### Score gick ner efter ändring

1. Kör `git diff` för att se vad som ändrades
2. Kör den specifika failande filen med `--verbose`
3. Jämför output med baseline
4. Fixa regression INNAN du commitar (Bug Policy i CLAUDE.md)
