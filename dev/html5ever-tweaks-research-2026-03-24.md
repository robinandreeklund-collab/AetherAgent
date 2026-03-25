# html5ever Parser Tweaks — Research 2026-03-24

## ALLA IMPLEMENTERADE ✅

| # | Optimering | Status | Commit |
|---|-----------|--------|--------|
| 1 | `.one(StrTendril)` — eliminera 4KB chunking | ✅ | `2f1d53b` |
| 2 | Per-crate `opt-level = 3` | ✅ | `2f1d53b` |
| 3 | SecondaryMap for tag_names (ersatte HashMap) | ✅ | `e79e435` |
| 4 | SmallVec Attrs (ersatte HashMap per element) | ✅ | `e79e435` |
| 5 | StrTendril for text (eliminerar .to_string()) | ✅ | `7901105` |
| 6 | HTML-langd-baserad pre-allokering | ✅ | `2f1d53b` |
| 7 | html5ever 0.27 → 0.38 (phf atoms, RefCell) | ✅ | `a90b10c` |
| 8 | Whitespace text filtering (41% av noder) | ✅ | `79066a9` |
| 9 | Script/style/noscript text skip | ✅ | `b4dbbd5` |

### Slutresultat vs baseline:
- **html5ever stage: 8.7ms → 5.2ms (-40%)**
- **semantic stage: 6.6ms → 4.5ms (-32%)**
- **heavy page total: ~19.9ms → 10.6ms (-47%)**
- **click:complex: 3332µs → 1615µs (-52%)**

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
