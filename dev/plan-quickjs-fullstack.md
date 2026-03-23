# Plan: QuickJS → Full Browser Lifecycle

**Mål:** Gå från "expressions-only sandbox" till en pipeline som unlockar SPA-sidor (GitHub, X, etc.) utan Chrome CDP.

**Nuläge:** DOM-mutationer fungerar internt i BridgeState men arenan returneras korrekt via `eval_js_with_dom_and_arena()`. Interrupt handler triggar inte under `ctx.with()`. Ingen browser lifecycle (readyState/DOMContentLoaded/load).

---

## Fas A: Interrupt Handler FFI-fix (Säkerhet — måste komma först)

**Problem:** `rt.set_interrupt_handler()` triggar inte under `ctx.with()` i rquickjs 0.11. `while(true)` hänger processen.

**Lösning:** Raw FFI redan bevisad i `event_loop.rs:477-492` (`drain_pending_jobs_ctx`). Samma mönster.

**Steg:**

1. I `js_eval.rs`, skapa en `extern "C"` interrupt callback:
   ```rust
   static INTERRUPT_COUNTER: AtomicU64 = AtomicU64::new(0);

   unsafe extern "C" fn interrupt_handler(
       _rt: *mut rquickjs::qjs::JSRuntime,
       _opaque: *mut std::ffi::c_void,
   ) -> i32 {
       let n = INTERRUPT_COUNTER.fetch_add(1, Ordering::Relaxed);
       if n >= MAX_LOOP_ITERATIONS { 1 } else { 0 }
   }
   ```

2. I `create_sandboxed_runtime()`, sätt interrupt handler via raw FFI:
   ```rust
   unsafe {
       let rt_ptr = ... ; // extrahera raw Runtime-pekare
       rquickjs::qjs::JS_SetInterruptHandler(rt_ptr, Some(interrupt_handler), std::ptr::null_mut());
   }
   ```

3. Återställ INTERRUPT_COUNTER till 0 före varje eval-anrop.

4. Ta bort `#[ignore]` från `test_infinite_loop_aborts` och `test_large_for_loop_aborts`.

**Risk:** Global `AtomicU64` fungerar inte med flera parallella eval-anrop. Lösning: använd opaque-pekare till en per-eval counter via `Box::into_raw()`.

**Testplan:**
- `while(true) {}` → avbryts inom 100ms
- `for(var i=0; i<1e9; i++) {}` → avbryts
- Normal kod (100 iterationer) → kör klart

---

## Fas B: Browser Lifecycle Simulation

**Problem:** Många frameworks (React, Vue, Svelte) triggar hydration vid `DOMContentLoaded` eller `readyState === "interactive"`. Utan dessa events körs aldrig hydration-koden.

**Steg:**

1. I `dom_bridge.rs`, lägg till `readyState`-property på document som Accessor:
   ```rust
   // I register_document():
   doc.prop("readyState", Accessor::new_get(JsFn(ReadyStateGetter { state })))?;
   ```

2. Skapa en ny funktion `run_with_lifecycle()` i `dom_bridge.rs`:
   ```rust
   pub fn eval_js_with_lifecycle(
       scripts: &[&str],   // Inline <script>-block i ordning
       arena: ArenaDom,
   ) -> DomEvalWithArena {
       // Fas 1: readyState = "loading", kör synkrona scripts
       // Fas 2: readyState = "interactive"
       //        dispatchEvent(new Event("DOMContentLoaded")) på document
       //        kör deferred scripts
       // Fas 3: readyState = "complete"
       //        dispatchEvent(new Event("load")) på window
       //        dränera event loop
   }
   ```

3. I `register_document()`, registrera `readyState` som en mutable getter som läser från BridgeState:
   ```rust
   struct BridgeState {
       // ... befintliga fält ...
       ready_state: String,  // "loading" | "interactive" | "complete"
   }
   ```

4. Extrahera `<script>`-taggar i ordning från HTML (befintlig logik i `js_eval::detect_js_snippets`).

**Testplan:**
- Script som registrerar `DOMContentLoaded`-listener → callbacken körs
- `document.readyState` returnerar rätt värde i varje fas
- Framework-liknande mönster: `if (document.readyState !== 'loading') { init() }`

---

## Fas C: DOM Mutation Pipeline → Renderer

**Problem:** JS-mutationer i sandboxen uppdaterar BridgeState.arena korrekt, men `render_with_js()` i `lib.rs:1613-1698` använder redan `eval_js_with_dom_and_arena()` som returnerar den modifierade arenan. Frågan är om serialiseringen `serialize_inner_html()` korrekt reflekterar ändringarna.

**Steg:**

1. **Verifiera mutation → serialisering pipeline:**
   - Skriv test: `createElement + appendChild` → `serialize_inner_html()` → ny HTML innehåller elementet
   - Skriv test: `setAttribute("class", "new")` → serialiserad HTML har `class="new"`
   - Skriv test: `textContent = "nytt"` → serialiserad HTML visar "nytt"

2. **Om serialiseringen funkar** (troligt — arena modifieras in-place):
   - Pipeline fungerar redan! `render_with_js_opts()` (lib.rs:1613) gör redan:
     ```
     HTML → ArenaDom → eval_js_with_dom_and_arena → modified arena → serialize → Blitz render
     ```
   - Problemet är att ingen lifecycle kör scripts (Fas B löser det)

3. **Om serialiseringen inte funkar** (fallback):
   - Förbättra `serialize_inner_html()` i `arena_dom.rs` att hantera dynamiskt skapade noder
   - Säkerställ att `copy_subtree` (dom_bridge.rs:1750) sätter rätt parent/children

4. **Strukturerad mutation-tracking** (framtida optimering, inte kritiskt):
   ```rust
   pub enum DomMutation {
       SetAttribute { key: NodeKey, name: String, value: String },
       RemoveAttribute { key: NodeKey, name: String },
       SetTextContent { key: NodeKey, text: String },
       AppendChild { parent: NodeKey, child: NodeKey },
       RemoveChild { parent: NodeKey, child: NodeKey },
       SetInnerHTML { key: NodeKey, html_len: usize },
   }
   ```
   → Möjliggör inkrementell re-rendering (bara ändrade noder)

**Testplan:**
- `render_with_js("<div id='a'></div>", "document.getElementById('a').textContent='Hello'")` → PNG visar "Hello"
- `render_with_js` med `createElement + appendChild` → nya element syns
- `render_with_js` med `setAttribute('style', 'color:red')` → styling appliceras

---

## Fas D: Script Extraction & Execution Order

**Problem:** Idag kör `parse_with_js()` bara selektiva expressions. För SPA-sidor behöver vi köra inline `<script>`-taggar i dokumentordning.

**Steg:**

1. Utöka `detect_js_snippets()` i `js_eval.rs` att returnera scripts i dokumentordning med metadata:
   ```rust
   pub struct ScriptInfo {
       pub code: String,
       pub is_defer: bool,
       pub is_async: bool,
       pub is_module: bool,  // type="module"
       pub src: Option<String>,  // extern — kan inte köras utan fetch
   }
   ```

2. Skapa `extract_ordered_scripts(html: &str) -> Vec<ScriptInfo>` i `js_eval.rs`:
   - Parsa `<script>` i dokumentordning
   - Filtrera bort `type="application/json"` (SSR-data, inte körbara)
   - Filtrera bort `src="..."` (externa — kräver fetch, hanteras separat)
   - Behåll inline scripts som passerar safety check

3. Integrera med Fas B lifecycle:
   ```
   extract_ordered_scripts(html)
     → filter(safety_check)
     → synkrona scripts körs i "loading"-fas
     → defer scripts körs i "interactive"-fas
     → lifecycle events triggras
   ```

4. Koppla till `parse_with_js()` pipeline i `lib.rs`:
   ```rust
   pub fn parse_with_js_full(html: &str, goal: &str) -> String {
       let scripts = extract_ordered_scripts(html);
       let arena = parse_to_arena(html);
       let result = eval_js_with_lifecycle(&scripts, arena);
       // Bygg semantiskt träd från modifierad arena
       semantic_tree_from_arena(&result.arena, goal)
   }
   ```

**Testplan:**
- HTML med 3 inline scripts → alla körs i ordning
- `defer`-script → körs efter DOMContentLoaded
- `type="application/json"` → ignoreras
- `src="external.js"` → ignoreras (loggas som saknad)
- Script med `fetch()` → blockeras av safety check

---

## Fas E: Tiered Pipeline Integration

**Koppla ihop allt med befintlig tier-escalation.**

**Uppdatera `escalation.rs` tier-val:**

```
Tier 0: Hydration extraction (Next/Nuxt/Remix SSR data)     — 0ms JS
Tier 1: Static parse (ingen JS detekterad)                    — 0ms JS
Tier 2: QuickJS selective eval (expressions only)             — 10-50ms
Tier 2.5 (NY): QuickJS lifecycle (inline scripts + events)   — 50-200ms
Tier 3: Blitz render (layout-beroende)                        — 10-50ms
Tier 4: Chrome CDP (WebGL, Workers, externa scripts)          — 500-2000ms
```

**Tier 2.5 triggas när:**
- Inline scripts detekteras som modifierar DOM (`getElementById + textContent/innerHTML`)
- Framework-markörer hittade (React, Vue, Svelte) men SSR-data saknas
- Sidan har event-listeners som triggas vid lifecycle events

**Fallback:** Om Tier 2.5 producerar tomt/identiskt resultat → eskalera till Tier 4.

---

## Ordning & Beroenden

```
Fas A (Interrupt FFI)     ← MÅSTE göras först (säkerhet)
  ↓
Fas B (Lifecycle)         ← Kräver Fas A (scripts kan loopa)
  ↓
Fas C (Mutation Pipeline) ← Kan göras parallellt med B (verifiera befintligt)
  ↓
Fas D (Script Extraction) ← Kräver B + C
  ↓
Fas E (Tier Integration)  ← Kräver D
```

## Estimerad storlek

| Fas | Nya rader kod | Nya tester | Filer |
|-----|---------------|------------|-------|
| A   | ~40           | 2 (unignore) | js_eval.rs |
| B   | ~120          | 5-6        | dom_bridge.rs |
| C   | ~30 (verifiering) | 3-4   | dom_bridge.rs, lib.rs |
| D   | ~100          | 4-5        | js_eval.rs, lib.rs |
| E   | ~50           | 2-3        | escalation.rs |
| **Totalt** | **~340** | **~18** | 4 filer |
