# Hybrid Scoring Pipeline — Integration Map i AetherAgent

> Beskriver exakt var i AetherAgents kodstruktur de tre komponenterna
> (TF-IDF, HDC, Embedding) integreras, vilka filer som ändras och i
> vilken ordning callchain:en går från HTTP-request till `top_n`-svar.

---

## Nuvarande callchain (förenklad)

```
HTTP POST /parse
  └── src/handlers/parse.rs          ← HTTP-handler, deserialiserar params
        └── src/semantic.rs           ← parse_to_semantic_tree()
              ├── src/fetch.rs        ← hämtar HTML via wreq
              ├── src/parser.rs       ← html5ever → rcdom → DOM-träd
              └── semantic.rs         ← walker: tilldelar role/label/relevance
                    └── relevance_score(node, goal)   ← PROBLEMET SITTER HÄR
                          └── embed(node.label)        ← körs på ALLA noder
                          └── cosine_sim(emb, goal_emb)
        └── apply_top_n(tree, top_n)  ← SAKNAS — top_n ignoreras
        └── serialize(tree)
        └── HTTP response
```

---

## Ny callchain efter PR

```
HTTP POST /parse
  └── src/handlers/parse.rs
        └── src/fetch.rs              ← oförändrad
        └── src/parser.rs             ← oförändrad (html5ever → DOM)
        └── src/semantic.rs           ← semantic walker, oförändrad
        └── [NY] src/scoring/pipeline.rs   ← ny orchestrator
              ├── [NY] scoring/tfidf.rs    ← Steg 1: byggs vid parse-tid
              ├── [NY] scoring/hdc.rs      ← Steg 2: byggs vid parse-tid (parallellt)
              └── [NY] scoring/embed.rs    ← Steg 3: bottom-up, bara på survivors
        └── src/handlers/parse.rs     ← apply_top_n() läggs till HÄR
        └── HTTP response
```

---

## Exakta filer som ändras

### 1. `src/semantic.rs` — minimalt ingrepp

**Nuläge:** `relevance_score()` anropas inne i tree-walkern för varje nod.

**Ändring:** Ta bort `relevance_score()`-anropet ur walkern. Semantic-walkern sätter `node.relevance = 0.0` som default. Scoring sker nu separat i `pipeline.rs` efter att hela trädet är byggt.

```rust
// FÖRE (src/semantic.rs, ungefär rad 140-160)
fn walk_node(node: &Handle, goal: &str, goal_emb: &Embedding) -> SemanticNode {
    let sem_node = SemanticNode {
        role: detect_role(node),
        label: extract_label(node),
        relevance: embed_and_score(&label, goal_emb),  // ← TA BORT DETTA
        children: node.children.iter().map(|c| walk_node(c, goal, goal_emb)).collect(),
        // ...
    };
    sem_node
}

// EFTER
fn walk_node(node: &Handle, goal: &str) -> SemanticNode {
    SemanticNode {
        role: detect_role(node),
        label: extract_label(node),
        relevance: 0.0,   // ← scoring sker senare i pipeline.rs
        children: node.children.iter().map(|c| walk_node(c, goal)).collect(),
        // ...
    }
}
```

**Vad som INTE ändras:** `SemanticNode`-struct, role-detektering, label-extraktion, injection_warnings, trust-nivå — allt oförändrat.

---

### 2. `src/parser.rs` — ny build-fas

**Nuläge:** `parse_to_semantic_tree()` returnerar trädet direkt.

**Ändring:** Efter att trädet är byggt, bygg TF-IDF-index och HDC-träd parallellt via `rayon`.

```rust
// src/parser.rs

use rayon::join;
use crate::scoring::{TfIdfIndex, HdcTree};

pub fn parse_to_semantic_tree(
    html: &str,
    goal: &str,
    js: Option<bool>,
) -> (SemanticTree, TfIdfIndex, HdcTree) {

    // Befintlig kod oförändrad
    let dom = parse_html(html);
    let tree = walk_dom(&dom, goal);   // semantic.rs — nu utan scoring

    // NY: bygg scoring-strukturer parallellt
    let flat_nodes = flatten(&tree);
    let (tfidf, hdc) = join(
        || TfIdfIndex::build(&flat_nodes),
        || HdcTree::build(&tree),
    );

    (tree, tfidf, hdc)
}
```

**Byggtiden (~5ms) amorteras** om samma URL parsas igen — strukturerna cachas i `src/cache.rs` per URL+content-hash.

---

### 3. `src/handlers/parse.rs` — orchestrering och top_n fix

Det är här pipelinen orchestreras och `top_n`-buggen fixas.

```rust
// src/handlers/parse.rs

use crate::scoring::pipeline::ScoringPipeline;

pub async fn handle_parse(params: ParseParams) -> ParseResponse {

    // Befintlig: hämta HTML
    let html = fetch(&params.url, &session).await?;

    // Befintlig: bygg semantic tree + NY: bygg scoring-strukturer
    let (tree, tfidf, hdc) = parse_to_semantic_tree(&html, &params.goal, params.js);

    // NY: kör hybrid pipeline
    let pipeline = ScoringPipeline::new(&tfidf, &hdc, &embed_model);
    let scored_nodes = pipeline.run(
        &tree,
        &params.goal,
        HdcThreshold::adaptive(),   // threshold per nod-nivå
    );

    // NY: top_n appliceras här — FIXAR BUGG A
    let output = match params.top_n {
        Some(n) => scored_nodes.into_iter().take(n).collect(),
        None    => scored_nodes,
    };

    // Befintlig serialisering
    ParseResponse {
        nodes: output,
        node_count: output.len(),   // nu korrekt: == top_n om satt
        total_nodes: tree.total_node_count(),
        // ...
    }
}
```

---

### 4. Ny modul: `src/scoring/`

Hela scoring-logiken lever i en ny modul. Strukturen:

```
src/scoring/
  mod.rs           ← pub use pipeline::ScoringPipeline
  pipeline.rs      ← orchestrerar steg 1→2→3
  tfidf.rs         ← TfIdfIndex: build() + query()
  hdc.rs           ← HdcTree: build() + prune()
  embed.rs         ← bottom-up embedding scoring
  threshold.rs     ← AdaptiveThreshold: nivå-baserade HDC-trösklar
  projection.rs    ← cachad Johnson-Lindenstrauss random projection
```

`pipeline.rs` är navet:

```rust
// src/scoring/pipeline.rs

pub struct ScoringPipeline<'a> {
    tfidf: &'a TfIdfIndex,
    hdc:   &'a HdcTree,
    embed: &'a dyn EmbedModel,
}

impl<'a> ScoringPipeline<'a> {
    pub fn run(
        &self,
        tree: &SemanticTree,
        goal: &str,
        threshold: AdaptiveThreshold,
    ) -> Vec<ScoredNode> {

        // Steg 1: TF-IDF kandidatretrieval (~0.05ms)
        let candidates = self.tfidf.query(goal, 300);

        // Steg 2: HDC pruning (~0.1ms)
        let goal_hv = self.hdc.project_goal(goal);
        let survivors = self.hdc.prune(&candidates, &goal_hv, &threshold);

        // Steg 3: Embedding scoring bottom-up (~2-5ms)
        let goal_emb = self.embed.embed(goal);
        embed::score_bottom_up(survivors, tree, &goal_emb, self.embed)
    }
}
```

---

### 5. `src/cache.rs` — caching av build-fas

TF-IDF och HDC byggs en gång per URL+content-hash och cachas. Vid DOM-mutation (JavaScript-rendered sidor) invalideras cachen automatiskt.

```rust
// src/cache.rs  (befintlig fil, utökas)

struct ParseCache {
    // Befintlig
    semantic_trees: LruCache<CacheKey, SemanticTree>,

    // NY
    tfidf_indexes:  LruCache<CacheKey, TfIdfIndex>,
    hdc_trees:      LruCache<CacheKey, HdcTree>,
}

// CacheKey = hash(url + html_content)
// Invalideras när: ny fetch ger annat content-hash
```

---

## Hur det ser ut i relation till hela AetherAgent-arkitekturen

```
┌─────────────────────────────────────────────────────────────┐
│  MCP / HTTP API                                             │
│  src/handlers/parse.rs  ←── NY orchestrering här           │
└──────────────┬──────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────┐
│  Fetch Layer                                                │
│  src/fetch.rs (wreq + Ghost Protocol Stack)   OFÖRÄNDRAD    │
│  src/security/firewall.rs (Semantic Firewall) OFÖRÄNDRAD    │
└──────────────┬──────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────┐
│  Parse Layer                                                │
│  src/parser.rs (html5ever + QuickJS)          MINIMALT      │
│    + bygg TfIdfIndex + HdcTree parallellt     NY            │
└──────────────┬──────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────┐
│  Semantic Layer                                             │
│  src/semantic.rs (role/label/trust/injection) MINIMALT      │
│    - ta bort relevance_score() ur walkern     ÄNDRAT        │
└──────────────┬──────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────┐
│  Scoring Layer  [NY MODUL]                                  │
│  src/scoring/pipeline.rs                                    │
│    ├── tfidf.rs     kandidatretrieval  ~0.05ms              │
│    ├── hdc.rs       subträd-pruning    ~0.1ms               │
│    └── embed.rs     bottom-up scoring  ~2-5ms               │
└──────────────┬──────────────────────────────────────────────┘
               │
┌──────────────▼──────────────────────────────────────────────┐
│  Cache Layer                                                │
│  src/cache.rs   TfIdfIndex + HdcTree cachas per URL         │
│                 (amorterar ~5ms build-tid)    UTÖKAS        │
└─────────────────────────────────────────────────────────────┘
```

---

## Vad som är helt opåverkat

Dessa delar av AetherAgent berörs inte alls:

| Komponent | Fil | Anledning |
|---|---|---|
| Ghost Protocol Stack | `src/fetch.rs` | Fetch sker innan scoring |
| Selective JS Runtime | `src/parser.rs` (QuickJS) | JS-eval sker innan scoring |
| Semantic Firewall | `src/security/firewall.rs` | Körs vid fetch-tid |
| Injection detection | `src/security/injection.rs` | Körs i semantic-walker |
| Session management | `src/session.rs` | Oberoende av scoring |
| Diff-engine | `src/diff.rs` | Arbetar på färdiga träd |
| Collab-store | `src/collab.rs` | Oberoende av scoring |
| Vision/CDP | `src/vision.rs` | Egen pipeline |
| `:stream`-verktyget | `src/handlers/stream.rs` | Separat kodväg — adresseras i annan PR |

---

## Sammanfattning — minimalt fotavtryck

Trots att tre nya komponenter läggs till är ingreppet i befintlig kod minimalt:

| Fil | Ändring |
|---|---|
| `src/semantic.rs` | Ta bort ett `embed_and_score()`-anrop ur walkern |
| `src/parser.rs` | Lägg till `rayon::join` för parallell build efter DOM-parse |
| `src/handlers/parse.rs` | Lägg till pipeline-anrop + applicera `top_n` på output |
| `src/cache.rs` | Lägg till två nya `LruCache`-fält |
| `src/scoring/` | **Ny modul** — påverkar inget befintligt |
| `Cargo.toml` | Lägg till `hypervector` och `rayon` |

Befintliga tester bör passera utan ändringar. De nya integrationstesterna verifierar att rätt nod rankas i `top_n`.

---

*Relaterat: `PR_hybrid_scoring_pipeline.md`, `AetherAgent_EN_Relevance_Bugganalys.md`*
