# html5ever Parser Tweaks — Research 2026-03-24

## Prioriterade Optimeringar

| # | Optimering | Sparar | Insats | Risk |
|---|-----------|--------|--------|------|
| 1 | `.one(StrTendril)` istf `.from_utf8().read_from()` | 0.5-1.0ms | 3 rader | Minimal |
| 2 | Per-crate `opt-level = 3` for html5ever | 1.0-2.0ms | 4 rader Cargo.toml | Minimal |
| 3 | `Option<QualName>` i DomNode, ta bort tag_names HashMap | 0.3-0.5ms | Medium refactor | Lag |
| 4 | `SmallVec<[(String,String); 4]>` for attribut | 0.3-0.4ms | Medium | Lag |
| 5 | `StrTendril` for text istf `String` | 0.3-0.5ms | API-andring | Medium |
| 6 | HTML-langd-baserad pre-allokering | 0.1-0.2ms | Trivial | Minimal |
| 7 | Uppgradera html5ever 0.27 -> 0.39 | 0.5-1.0ms | Stor refactor | Medium |

### Konservativ uppskattning (1+2+3+4+6): 2.2-4.1ms sparad
### Aggressiv uppskattning (alla): 3.0-5.6ms sparad

---

## Detaljer

### 1. `.one(StrTendril)` istf `.from_utf8().read_from()`

**Nuvarande:**
```rust
html5ever::parse_document(sink, Default::default())
    .from_utf8()
    .read_from(&mut html.as_bytes())
```

**Problem:** `.from_utf8().read_from()` wrappas i `Utf8LossyDecoder` och laser i **4KB chunks**.
For en 75KB sida = 19 separata read/process-cykler, var och en skapar ny StrTendril.

**Battre:**
```rust
html5ever::parse_document(sink, Default::default())
    .one(StrTendril::from(html))
```

Matar hela strangen som en enda tendril. Undviker:
- UTF-8 lossy decoding (input ar redan validerad &str)
- 4KB chunking overhead
- Multipla tendril-allokeringar

### 2. Per-crate opt-level = 3

**Nuvarande:** `opt-level = "z"` (minimera binarstorlek) — aggressivt avaktiverar
vektorisering, loop unrolling, inlining.

**Fix:**
```toml
[profile.release.package.html5ever]
opt-level = 3
[profile.release.package.markup5ever]
opt-level = 3
```

html5ever far full hastighetsoptimering medan egen kod behallar "z" for WASM-storlek.

### 3. QualName i DomNode

`elem_name()` anropas ~30 ganger per tree-builder-steg. HashMap-lookup for varje.
Lagra `Option<QualName>` direkt i DomNode — SlotMap-lookup ar O(1) array-indexering.

### 4. SmallVec for attribut

De flesta element har 0-3 attribut. HashMap har hog overhead for sma storlekar.
`SmallVec<[(String, String); 4]>` undviker heap-allokering for <= 4 attribut.

### 5. StrTendril for text

`StrTendril.to_string()` tvingar full kopia. ~500 textnoder per sida.
Lagra StrTendril direkt — implementerar `Deref<Target=str>`.

### 6. Pre-allokering

Empirisk regel: ~1 nod per 60 bytes HTML.
```rust
let estimated_nodes = (html.len() / 60).max(256);
```

### 7. html5ever 0.39

Byt fran string_cache till phf (perfect hash). Diverse tree-builder-optimeringar.
Stor API-migration kravs.

---

## TreeBuilderOpts (inget av varde)

- `scripting_enabled` (default true): Paverkar bara noscript-hantering
- `exact_errors` (default false): Redan av
- `drop_doctype` (default false): Vi ignorerar doctype anda
- Ingen flagga for att hoppa over error recovery
