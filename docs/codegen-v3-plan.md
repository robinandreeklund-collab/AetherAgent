# Codegen v3 Plan — Port jsdom DOM Behavior to Rust

> **Status:** Planerad — ej påbörjad
> **Föregående arbete:** Session 014JyAuqUwqrE9W6VBYzfgdc (2026-03-26)
> **Branch:** `claude/review-aetheragent-wpt-C7pD8`

---

## Bakgrund: Vad vi gjort (v1 + v2)

### Codegen v1 (commit a2d5b96)
- Byggde `codegen/` — Rust-baserat verktyg som parsar `.webidl`-filer via webidl-rs
- Genererar JsHandler-structs med getter/setter för varje WebIDL-attribut
- 14 HTML element interfaces → 4917 rader genererad kod
- **Resultat: +220 WPT pass i html/semantics**

### Codegen v1.5 (commit 80790f4)
- Utökade till 63 HTML element interfaces → 15149 rader genererad kod
- Lade till html_media.webidl (video, audio, source, track)
- Lade till html_misc.webidl (div, span, table, iframe, dialog, script, etc.)
- **Resultat: +91 WPT pass i html/semantics (totalt +311 från start)**

### Codegen v2 (commit 37c2ff8)
- Lade till attribut-klassificering: REFLECTED / STATE-BACKED / COMPUTED
- ElementState store i BridgeState för value/checked/selectedIndex
- computed.rs med 25+ beräkningsfunktioner (willValidate, URL-dekomponering, etc.)
- **Resultat: -10 WPT regression, NOLL nya pass**

### Varför v2 inte fungerade
Djupanalys (agent ade18bbbdd8e4cd27) visade:
1. **Property accessors FUNGERAR korrekt** — verifierat med property descriptors
2. **Men de gör SAMMA sak som html_properties.rs** — läser attribut
3. **WPT testar BETEENDE, inte property-existens** — form validation, URL-parsing, etc.
4. **Saknade DOM-objekt** — `input.validity` (ValidityState), `form.elements` (Collection)
5. **Saknade testharness-features** — `add_cleanup` (574 failures), `on_event` (125)

### Slutsats från forskning
Ingen browser-motor (Servo, Chromium, Firefox, jsdom) auto-genererar beteendelogik.
Alla genererar wrapper/glue-kod och skriver -impl-filer manuellt.

jsdom (MIT-licens) har exakt de -impl-filer vi behöver. Deras algoritmer kan portas
till Rust och integreras med vår codegen-wrapper.

---

## Nuvarande WPT-scores (baseline för v3)

| Suite | Cases | Passed | Rate |
|-------|-------|--------|------|
| dom/nodes | 6,676 | 5,666 | 84.9% |
| dom/events | 318 | 224 | 70.4% |
| dom/collections | 48 | 27 | 56.2% |
| domparsing | 453 | 85 | 18.8% |
| html/semantics | ~4,856 | 1,068 | 22.0% |

---

## V3 Plan: 4 faser

### Fas 1: Testharness-fixar (HÖGST PRIORITET — +700 WPT pass)

**Problem:** WPT-tester använder hjälpfunktioner som saknas i vår testharness-shim.

**Fixar:**

1. **`add_cleanup` (574 failures i domparsing + 350 i html/semantics)**
   - Testerna skapar dynamiska test-objekt via hjälpfunktioner
   - `this.add_cleanup()` anropas men `this` är undefined i vissa kontexter
   - **Fix i `wpt/testharness-shim.js`:** Tracka `_current_test` och exponera global `add_cleanup`
   ```js
   var _current_test = null;
   // I test() funktionen, före fn.call(t):
   _current_test = t;
   fn.call(t);
   _current_test = null;
   // Global fallback:
   globalThis.add_cleanup = function(fn) {
       if (_current_test) _current_test.add_cleanup(fn);
   };
   ```

2. **`on_event` (125 failures i html/semantics)**
   - WPT-hjälpfunktion som saknas
   - **Fix i `wpt/polyfills.js`:**
   ```js
   function on_event(target, event, callback) {
       target.addEventListener(event, callback);
   }
   globalThis.on_event = on_event;
   ```

3. **`newHTMLDocument` (84 failures)**
   - WPT-hjälpfunktion som skapar ett nytt tomt HTML-dokument
   - **Fix i `wpt/polyfills.js`:**
   ```js
   function newHTMLDocument() {
       return document.implementation.createHTMLDocument('');
   }
   globalThis.newHTMLDocument = newHTMLDocument;
   ```

**Fil:** `wpt/testharness-shim.js`, `wpt/polyfills.js`
**Förväntat:** +700 pass (574 + 125 + 84 = 783 errors eliminerade → ~700 nya pass)
**Verifiering:** `domparsing` 85 → ~250+, `html/semantics` 1068 → ~1400+

### Fas 2: ValidityState + checkValidity (+200 WPT pass)

**Problem:** `input.validity` existerar inte. `input.checkValidity()` returnerar alltid `true`.

**jsdom-källa:** `DefaultConstraintValidation-impl.js` (67 rader) + `ValidityState-impl.js` (58 rader)

**Implementera i:** `src/dom_bridge/dom_impls/constraint_validation.rs`

**Algoritmer att porta:**
```rust
// ValidityState — 11 boolean properties
pub struct ValidityState {
    pub value_missing: bool,      // required men tom
    pub type_mismatch: bool,      // email/url-format fel
    pub pattern_mismatch: bool,   // regex-pattern matchar ej
    pub too_long: bool,           // maxlength överskriden
    pub too_short: bool,          // minlength ej uppnådd
    pub range_underflow: bool,    // under min
    pub range_overflow: bool,     // över max
    pub step_mismatch: bool,      // step-violation
    pub bad_input: bool,          // ogiltigt input
    pub custom_error: bool,       // setCustomValidity anropad
    pub valid: bool,              // alla ovan false
}

// Skapa JS-objekt med alla properties
pub fn create_validity_state_object(ctx, state, key) -> Object { ... }

// checkValidity — beräkna validity + dispatcha "invalid" event om ogiltig
pub fn check_validity(state, key) -> bool { ... }
```

**Integration med codegen:**
- `attr_classify.rs`: `("HTMLInputElement", "validity") => Computed("create_validity_state")`
- `rust_gen.rs`: Generera getter som returnerar JS Object (inte bara sträng/bool)
- Gäller: HTMLInputElement, HTMLSelectElement, HTMLTextAreaElement, HTMLButtonElement

**Fil:** `src/dom_bridge/dom_impls/constraint_validation.rs`
**Förväntat:** +200 pass i html/semantics
**Verifiering:** `html/semantics/forms/` underkataloger

### Fas 3: Input value/checked dirty state (+150 WPT pass)

**Problem:** `input.value` setter skriver till attribut — borde skriva till intern state.

**jsdom-källa:** `HTMLInputElement-impl.js` — value mode-logik

**Implementera i:** `src/dom_bridge/dom_impls/input_value.rs`

**Value modes per input type (från HTML spec):**
```
"value"       → text, search, url, tel, email, password, number, range, color
"default"     → hidden, submit, image, reset, button
"default/on"  → checkbox, radio
"filename"    → file
```

**Algoritmer att porta:**
- `get_value_mode(type)` → bestäm semantik
- `get_value(state, key)` → respektera value mode + dirty flag
- `set_value(state, key, val)` → skriv till element_state, sätt dirty flag
- `defaultValue` getter → alltid läsa attribut
- `checked` getter → respektera dirty checkedness flag
- Radio button unchecking — när en radio checkas, unchecka andra i samma group

**Element state extension:**
```rust
pub struct ElementState {
    // ... befintliga fält ...
    pub value_mode: Option<String>,  // "value", "default", "default/on", "filename"
}
```

**Fil:** `src/dom_bridge/dom_impls/input_value.rs`
**Förväntat:** +150 pass
**Verifiering:** `html/semantics/forms/the-input-element/`

### Fas 4: Form association + HTMLSelectElement (+100 WPT pass)

**Implementera i:** `src/dom_bridge/dom_impls/form_association.rs` + `select_element.rs`

**form.elements:**
```rust
// Samla alla form controls vars form-owner är detta element
pub fn get_form_elements(state, form_key) -> Vec<NodeKey> {
    // Traversera hela DOM, hitta input/select/textarea/button
    // vars ancestor-form eller form=attribut matchar form_key
}
```

**form.reset():**
```rust
// Rensa dirty flags på alla form controls
pub fn reset_form(state, form_key) {
    for control in get_form_elements(state, form_key) {
        let es = state.element_state.entry(control).or_default();
        es.value = None;
        es.checked = None;
        es.value_dirty = false;
        es.checked_dirty = false;
    }
}
```

**select.selectedIndex:**
```rust
pub fn get_selected_index(state, key) -> i32 {
    // Hitta första option-barn med selected=true
    // Returnera dess index, eller -1
}
```

**Fil:** `src/dom_bridge/dom_impls/form_association.rs`, `select_element.rs`
**Förväntat:** +100 pass

---

## Exakt vad som ska testas efter v3 är genomförd

### Obligatoriska WPT-körningar (alla 5 fokus-suiter):
```bash
./wpt/setup.sh  # Om wpt-suite saknas

for s in dom/nodes dom/events dom/collections domparsing html/semantics; do
  echo "=== $s ==="
  cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/$s/ 2>&1 | grep "Passed:"
done
```

### Förväntade resultat:
| Suite | Före v3 | Efter v3 | Minimum ökning |
|-------|---------|----------|----------------|
| html/semantics | 1,068 | **1,600+** | +500 |
| domparsing | 85 | **250+** | +150 |
| dom/events | 224 | **240+** | +15 |
| dom/nodes | 5,666 | **5,700+** | +30 |
| dom/collections | 27 | **30+** | +3 |

### Om resultaten INTE uppnås:
- Kör `--verbose` på failing suite
- Identifiera de 3 vanligaste error-mönstren
- Fixa rotorsaken — INTE fler stubs

### Standardchecks:
```bash
cargo test --features js-eval,blitz,fetch --lib  # 664 pass (1 flaky OK)
cargo clippy --features js-eval,blitz,fetch -- -D warnings  # Inga errors
cargo fmt --check  # Inga diffs
```

---

## Filstruktur efter v3

```
src/dom_bridge/
├── mod.rs                    Core: entry points, make_element_object
├── dom_impls/                NYT: Beteendelogik (portat från jsdom)
│   ├── mod.rs
│   ├── constraint_validation.rs  ValidityState + checkValidity
│   ├── input_value.rs            Value modes + dirty flags
│   ├── form_association.rs       form.elements + form owner
│   └── select_element.rs        selectedIndex + options
├── generated/                Auto-genererat: wrapper/glue-kod
│   ├── mod.rs
│   ├── register.rs
│   └── html*.rs              63 interface-filer
├── computed.rs               Computed properties (URL, table indices, etc.)
├── element_state.rs          Per-element mutable state
├── attributes.rs             Attribute get/set
├── events.rs                 Event handling
├── node_ops.rs               Node tree manipulation
├── style.rs                  classList + style
├── chardata.rs               CharacterData methods
├── selectors.rs              CSS selector engine
├── html_properties.rs        HTML reflected properties (gradvis ersätts)
├── window.rs                 Window/Console/Storage
├── state.rs                  SharedState, BridgeState
└── utils.rs                  Utility functions

codegen/                      WebIDL→QuickJS code generator
├── src/
│   ├── main.rs
│   ├── webidl_parser.rs      webidl-rs integration
│   ├── rust_gen.rs           Rust code generation
│   ├── attr_classify.rs      REFLECTED/STATE-BACKED/COMPUTED dispatch
│   └── type_map.rs           WebIDL→Rust type mapping
└── Cargo.toml

webidl/                       WebIDL source files
├── html_input.webidl         14 form element interfaces
├── html_media.webidl         5 media element interfaces
└── html_misc.webidl          44 misc element interfaces

wpt/
├── testharness-shim.js       Uppdateras: add_cleanup global, _current_test
├── polyfills.js              Uppdateras: on_event, newHTMLDocument
├── runner.rs                 WPT test runner
└── setup.sh                  WPT sparse checkout (~45 suiter)
```

---

## Sammanfattning för nästa agent

**Vi har byggt:** WebIDL-parser + codegen som genererar 15K rader wrapper-kod.
**Det ger:** +381 WPT pass (properties existerar).
**Det ger INTE:** Beteendelogik (det WPT faktiskt testar).
**Lösningen:** Porta jsdom's -impl-algoritmer till Rust i `dom_impls/`.
**Snabbaste vinsten:** Fixa testharness (add_cleanup/on_event) → +700 pass utan Rust-ändringar.
**Börja med:** Fas 1 (testharness-fixar) → Fas 2 (ValidityState) → Fas 3 (input state) → Fas 4 (form/select).
