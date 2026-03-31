# WPT Workflow Guide — Steg-för-steg

> Denna guide följer du VARJE gång du jobbar med WPT-tester i AetherAgent.
> Referera alltid hit innan du börjar en ny session.

## Snabbstart

```bash
# 1. Setup (första gången)
./wpt/setup.sh

# 2. Kör tester mot den subkategori du jobbar med
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/ --verbose

# 3. Se detaljerat resultat
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/Document-getElementById.html --verbose
```

---

## Arkitektur: Hur DOM-implementation fungerar

### Pipeline: WebIDL → Codegen → Rust Handlers → JS Bridge

```
webidl/*.webidl          WebIDL interface-definitioner (HTML spec)
       ↓
codegen/src/             Rust-baserat code generator
  ├── webidl_parser.rs   Parsar WebIDL-filer
  ├── attr_classify.rs   Klassificerar: REFLECTED / STATE-BACKED / COMPUTED
  ├── type_map.rs        WebIDL → Rust typ-mappning
  └── rust_gen.rs        Genererar JsHandler structs
       ↓
src/dom_bridge/generated/  Auto-genererade property accessors (växer med varje codegen-körning)
  ├── register.rs          Master-registrering per tag-namn
  ├── htmlinput_element.rs En fil per WebIDL interface
  └── ...                  (antal filer = antal interfaces i webidl/*.webidl)
       ↓
src/dom_bridge/dom_impls/  Manuellt portad beteendelogik (från jsdom)
  ├── constraint_validation.rs  ValidityState + checkValidity
  ├── input_value.rs            Value modes, dirty state, sanitization
  ├── form_association.rs       form.elements, form owner
  ├── select_element.rs         selectedIndex, options
  └── xml_serializer.rs         innerHTML/outerHTML
       ↓
src/dom_bridge/mod.rs      Orkestrering: make_element_object() binder allt
```

### Tre lager av DOM-implementation

**Lager 1: Codegen (auto-genererat)**
- Parsar `.webidl`-filer med `webidl-rs`
- Genererar getter/setter för varje HTML-attribut
- Antal filer/rader växer automatiskt när nya `.webidl`-interfaces läggs till
- Kör med: `cd codegen && cargo run`

**Lager 2: dom_impls (manuellt portat)**
- Beteendelogik som INTE kan auto-genereras
- Portad från jsdom MIT-licensierade `-impl`-filer
- Constraint validation, input value modes, form association

**Lager 3: Computed properties**
- `computed.rs` — beräknade properties (willValidate, labels, URL-decomposition)
- Anropas av genererad kod via `super::super::computed::funktionsnamn`

---

## Arbetsflöde: Implementera ny DOM-funktionalitet

### Metod 1: Codegen + dom_impls (för HTML element properties)

**Steg 1: Skapa WebIDL-fil**

```webidl
// webidl/html_input.webidl
interface HTMLInputElement : HTMLElement {
    attribute DOMString accept;
    attribute DOMString value;
    readonly attribute ValidityState validity;
};
```

**Steg 2: Kör codegen**

```bash
cd codegen && cargo run
# Genererar: src/dom_bridge/generated/htmlinput_element.rs
```

**Steg 3: Klassificera attribut**

I `codegen/src/attr_classify.rs`:
```rust
("HTMLInputElement", "accept") => Reflected,     // Direkt HTML-attribut
("HTMLInputElement", "value") => StateBacked,     // ElementState
("HTMLInputElement", "validity") => Computed("create_validity_state"),
```

**Steg 4: Implementera beteendelogik**

I `src/dom_bridge/dom_impls/` — porta från jsdom:

```rust
// constraint_validation.rs
pub fn compute_validity(state: &BridgeState, key: NodeKey) -> ValidityState {
    // Riktig beteendelogik portad från jsdom
}
```

**Steg 5: Registrera i dom_bridge**

I `register_dom_impl_properties()` i `mod.rs`:
```rust
if tag == "input" {
    obj.prop("validity", Accessor::new_get(JsFn(ValidityStateGetter { ... })).configurable())?;
}
```

### Metod 2: Direkt Rust-implementation (för DOM Core APIs)

**Steg 1: Identifiera saknad API**

```bash
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/events/ --verbose 2>&1 | grep "^   -"
```

**Steg 2: Implementera i rätt modul**

| API-typ | Fil |
|---------|-----|
| Node-metoder (appendChild, cloneNode) | `node_ops.rs` |
| Event dispatch, listeners | `events.rs` |
| Element attributes | `attributes.rs` |
| CSS selectors | `selectors.rs` |
| CharacterData (substringData etc.) | `chardata.rs` |
| Window/Document/Console | `window.rs` |
| Form validation/behavior | `dom_impls/` |

**Steg 3: Registrera som JsHandler**

```rust
struct MyNewHandler { state: SharedState, key: NodeKey }
impl JsHandler for MyNewHandler {
    fn handle<'js>(&self, ctx: &Ctx<'js>, args: &[Value<'js>]) -> rquickjs::Result<Value<'js>> {
        // Implementation
    }
}
// Registrera i make_element_object() eller register_document()
obj.set("myMethod", Function::new(ctx.clone(), JsFn(MyNewHandler { ... }))?)?;
```

### Metod 3: Porta från jsdom (för komplex beteendelogik)

jsdom (MIT-licens) har `-impl`-filer med exakt den beteendelogik vi behöver.

**Process:**
1. Hitta jsdom-filen: `lib/jsdom/living/nodes/HTMLInputElement-impl.js`
2. Identifiera algoritmen (t.ex. value mode-logik, radio group unchecking)
3. Skriv om i Rust med AetherAgent's API:er:
   - `BridgeState` istället för jsdom's `this`
   - `NodeKey` istället för node-referens
   - `arena.nodes.get(key)` istället för DOM traversal

**Exempel — Input Value Modes (portad från jsdom):**

```rust
// jsdom: HTMLInputElement-impl.js getValueMode()
// → Rust: dom_impls/input_value.rs
pub fn get_value_mode(input_type: &str) -> &'static str {
    match input_type {
        "hidden" | "submit" | "image" | "reset" | "button" => "default",
        "checkbox" | "radio" => "default/on",
        "file" => "filename",
        _ => "value",
    }
}
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
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/ 2>&1 | grep "Passed:"
```

### Steg 3: Analysera failures

```bash
# Top failure-mönster
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/ --verbose 2>&1 \
  | grep "^   -" | sed 's/: .*//' | sort | uniq -c | sort -rn | head -20

# Specifik fil
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/Node-cloneNode.html --verbose
```

**Vanliga failure-mönster:**

| Mönster | Orsak | Lösning |
|---------|-------|---------|
| `TypeError: X is not a function` | API saknas | Implementera i dom_bridge/ |
| `X is undefined` | Property saknas | Lägg till getter i make_element_object() |
| `assert_equals: got null expected "..."` | Returnerar fel värde | Fixa logik i dom_impls/ |
| `property is not configurable` | Accessor saknar .configurable() | Lägg till i generated code |
| `no test suite completion` | Kräver iframe/external resource | Ej fixbar utan iframe-stöd |
| `ReferenceError: X is not defined` | QuickJS strict mode | Variabel saknar var/let deklaration |

### Steg 4: Implementera fix

**Var implementerar du?**

| Fil | Vad |
|-----|-----|
| `src/dom_bridge/mod.rs` | Core: make_element_object, register_document, register_dom_impl_properties |
| `src/dom_bridge/dom_impls/` | Beteendelogik: validation, input values, form association |
| `src/dom_bridge/generated/` | Auto-genererat (redigera INTE manuellt, kör codegen istället) |
| `src/dom_bridge/computed.rs` | Beräknade properties (willValidate, labels, URL-decomposition) |
| `src/dom_bridge/events.rs` | Event dispatch, addEventListener, removeEventListener |
| `src/dom_bridge/node_ops.rs` | appendChild, removeChild, insertBefore, cloneNode |
| `src/dom_bridge/window.rs` | Event constructors, DOM type hierarchy, NodeFilter |
| `src/dom_bridge/chardata.rs` | CharacterData methods (native Rust, UTF-16 aware) |
| `src/dom_bridge/element_state.rs` | Per-element mutable state (value, checked, dirty flags) |

### Steg 5: Verifiera + Regressionskontroll

```bash
# Suite du jobbade med
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/

# Regressionskontroll — Tier 1 (alla MÅSTE passa)
for s in dom/nodes dom/events dom/traversal dom/collections; do
  echo "=== $s ==="
  cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/$s/ 2>&1 | grep "Passed:"
done

# Standard CI
cargo test --features js-eval,blitz,fetch --lib && cargo clippy --features js-eval,blitz,fetch -- -D warnings && cargo fmt --check
```

### Steg 6: Uppdatera dokumentation + Commit

1. Uppdatera `docs/wpt-dashboard.md` med nya scores
2. Commit-meddelande format:

```
feat: implement input value sanitization (color, text, number, date)

WPT Results:
- html/semantics/forms/the-input-element: 8→504 (+496)
- color.html: 1→18, text.html: 12→14, range.html: 11→22
```

---

## Arbetsflöde: Migrera Polyfill → Rust Native

### Princip: Allt ska vara native Rust

Polyfills i `wpt/polyfills.js` är TEMPORÄRA. Varje polyfill ska migreras till Rust.

### Säker migrationsprocess

**Steg 1: Verifiera att Rust redan implementerar funktionen**

```bash
# Kolla om API:et redan finns i dom_bridge
grep -rn "ownerDocument\|createEvent\|textContent" src/dom_bridge/
```

**Steg 2: Ta bort polyfill**

Ersätt med kommentar:
```js
// ─── document.createEvent() — MIGRERAD till native Rust (dom_bridge/mod.rs) ──
// Registrerad som NativeCreateEvent i register_document().
```

**Steg 3: Verifiera — score får INTE gå ner**

```bash
# Kör INNAN (med polyfill)
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/ 2>&1 | grep "Passed:"
# Ta bort polyfill, kör EFTER
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/ 2>&1 | grep "Passed:"
# Vid regression: revert med git checkout
```

### Migrerade polyfills (2026-03-31)

| Polyfill | Rader | Rust-ersättning |
|----------|-------|-----------------|
| CharacterData .data/.nodeValue/.length | 40 | Rust getter/setter i make_element_object() |
| document.createEvent | 70 | NativeCreateEvent handler |
| document.title | 20 | DocTitleGetter/DocTitleSetter |
| document.URL/location | 24 | Statisk i register_document() |
| ownerDocument patching | 80 | OwnerDocumentGetter (lazy) — wrapper borttagen Session 4 |
| NodeFilter constants | 16 | window.rs JS eval |
| createElementNS fallback | 19 | Native CreateElementNS |
| getElementsByTagNameNS | 9 | Native GetElementsByTagNameNSDoc |
| createAttribute fallback | 14 | Native CreateAttribute |
| compareDocumentPosition | 9 | Native i register_window |
| TouchEvent (simpleTypes) | 3 | Native Touch/TouchEvent/TouchList i window.rs — Session 4 |
| id/className polyfill | 15 | Native AttrGetter/AttrSetter i make_element_object() — Session 4 |
| prefix/namespaceURI/localName | 8 | Native i make_element_object() — Session 4 |
| Simple event types (18 st) | 20 | Native i window.rs — Session 5 |
| __patchPrototype | 30 | Native Object.setPrototypeOf i make_element_object() — Session 5 |
| __patchCharacterData | 1 | No-op borttagen — redan native i chardata.rs — Session 5 |
| __patchChildNode (partial) | 15 | Förenklad — prototype+chardata hanteras native — Session 5 |

### Kvar i polyfills.js (med motivering)

| Polyfill | Rader | Varför kvar |
|----------|-------|-------------|
| createHTMLDocument | 150 | Kräver ALLA document-metoder i Rust (inkl. XPath-patching) |
| NamedNodeMap .attributes | 130 | Behöver Proxy-baserad Rust-implementation |
| Window-globalThis sync | 34 | JS Proxy, svårt i Rust |
| NS-metadata tracking | 30 | setAttributeNS prefix/namespace tracking |
| Document/XMLDocument constructor | 20 | new Document() delegerar till createHTMLDocument |
| DOM Type Hierarchy | 180 | HTMLDivElement etc. konstruktorer för instanceof |
| NodeList stub | 6 | Trivial |
| NodeList/HTMLCollection stubs | 10 | Utility-typer |

---

## Checklista: Innan PR

```
□ Baseline WPT-score noterad (before)
□ Ändringar implementerade (native Rust, ej polyfill)
□ WPT-score efter (after) — ingen regression
□ cargo test --features js-eval,blitz,fetch --lib PASS
□ cargo clippy --features js-eval,blitz,fetch -- -D warnings PASS
□ cargo fmt --check PASS
□ docs/wpt-dashboard.md uppdaterad
□ Commit-meddelande innehåller WPT before/after
□ Generated code: alla Accessor properties har .configurable()
```

---

## Vanliga kommandon — Referens

```bash
# === SETUP ===
./wpt/setup.sh                    # Ladda ner WPT-tester

# === KÖR TESTER ===
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/           # Hel svit
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/ --verbose  # Med detaljer
cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/dom/nodes/Node-cloneNode.html --verbose  # Specifik fil

# === CODEGEN ===
cd codegen && cargo run            # Regenerera från webidl/*.webidl

# === CI PIPELINE ===
cargo test --features js-eval,blitz,fetch --lib && cargo clippy --features js-eval,blitz,fetch -- -D warnings && cargo fmt --check

# === ALLA SVITER ===
for s in dom/nodes dom/events dom/ranges dom/traversal dom/collections dom/lists domparsing html/semantics css/selectors; do
  echo "=== $s ===" && cargo run --bin aether-wpt --features js-eval,blitz,fetch -- wpt-suite/$s/ 2>&1 | grep "Passed:"
done
```

---

## Felsökning

### "no test suite completion" (vanligaste felet)

**Orsak:** Testet kräver iframes, external resources, eller DOMContentLoaded-event.
**Status:** ~2100 tester blockerade. Fixas inte av harness-ändringar.
**Workaround:** Fokusera på tester som KÖR men failar (mer impact).

### "property is not configurable"

**Orsak:** Generated Accessor saknar `.configurable()`.
**Fix:** Kör codegen med uppdaterad `rust_gen.rs`, eller fixa manuellt.

### Score gick ner efter ändring

1. `git stash` → kör WPT → bekräfta att stashed code har rätt score
2. `git stash pop` → kör specifik failande fil med `--verbose`
3. Fixa regression INNAN commit (Bug Policy i CLAUDE.md)
