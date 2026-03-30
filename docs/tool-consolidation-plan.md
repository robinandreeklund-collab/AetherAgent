# AetherAgent Tool Consolidation Plan

> Konsolidering av 35 MCP-verktyg → 12 verktyg, 87 HTTP-endpoints → ~30, 76 WASM-funktioner → ~20.

## Bakgrund

AetherAgent har vuxit från Fas 1 till Fas 18 och ackumulerat:

| Yta | Antal |
|-----|-------|
| MCP Tools | 35 |
| HTTP Endpoints | 87 |
| WASM Functions | 76 |
| Totalt publika funktioner | 208+ |

Detta skapar tre problem:
1. **LLM token waste** — Agenten läser 35 tool-beskrivningar varje anrop
2. **Decision paralysis** — `parse` vs `parse_top` vs `parse_with_js` vs `stream_parse`?
3. **Redundanta fetch-varianter** — Nästan varje tool har en `fetch_*`-tvilling

## Designprinciper

1. **Auto-detect input** — Om `url` skickas: fetcha först. Om `html` skickas: parsa direkt. Om `screenshot_b64` skickas: YOLO.
2. **Noll parametrar = vettig default** — Alla tools fungerar med bara `goal` + input.
3. **Säkerhet alltid på** — Firewall-check och injection-scan körs automatiskt i varje tool.
4. **`stream: true` som default** — Alla tools stödjer adaptive realtids-streaming.
5. **Parametrar överstyr, inte aktiverar** — `top_n`, `format`, `mode` etc. finns för att justera beteende, inte för att saker ska fungera.

---

## Nuvarande → Nytt: Komplett mappning

### Tool 1: `parse`

**Ersätter:** `parse`, `parse_top`, `parse_with_js`, `parse_screenshot`, `vision_parse`, `fetch_parse`, `fetch_vision`, `render_with_js`, `html_to_markdown`, `semantic_tree_to_markdown`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `url` | string? | — | URL att fetcha (auto-detect) |
| `html` | string? | — | Rå HTML (auto-detect) |
| `screenshot_b64` | string? | — | Base64 PNG → YOLO pipeline |
| `goal` | string | **krävs** | Mål för relevansskorning |
| `top_n` | u32? | alla | Begränsa antal returnerade noder |
| `format` | string? | `"tree"` | `"tree"` \| `"markdown"` |
| `js` | bool? | auto | `true` tvingar JS-eval, `false` skippar, auto = `select_parse_tier` |
| `stream` | bool? | `true` | Chunked SSE eller komplett svar |

**Intern routing:**

```
if screenshot_b64 → YOLO pipeline (vision.rs)
else if url → fetch (med firewall) → html
     if js == true || select_parse_tier == js → parse_with_js
     else → semantic parse
     if top_n → filtrera top-N
     if format == "markdown" → tree_to_markdown
```

**Exempel:**

```json
// Snabb LLM-kontext (vanligaste anropet)
{"goal": "hitta produktpriser", "url": "https://shop.se", "top_n": 20, "format": "markdown"}

// Fullständigt träd från lokal HTML
{"goal": "analysera formulär", "html": "<form>...</form>"}

// Screenshot-analys
{"goal": "hitta knappar", "screenshot_b64": "iVBOR..."}

// JS-tung SPA
{"goal": "ladda produktlista", "url": "https://spa.se", "js": true}
```

---

### Tool 2: `act`

**Ersätter:** `find_and_click`, `fill_form`, `extract_data`, `fetch_click`, `fetch_extract`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `url` | string? | — | URL att fetcha |
| `html` | string? | — | Rå HTML |
| `goal` | string | **krävs** | Kontext för relevansskorning |
| `action` | string | **krävs** | `"click"` \| `"fill"` \| `"extract"` |
| `target` | string? | — | Label/text att klicka (för `click`) |
| `fields` | object? | — | Key-value fältmappning (för `fill`) |
| `keys` | string[]? | — | Datanycklar att extrahera (för `extract`) |
| `stream` | bool? | `true` | Streaming |

**Intern routing:**

```
if url → fetch (med firewall + session cookies)
match action:
  "click"   → find_and_click(html, goal, target)
  "fill"    → fill_form(html, goal, fields)
  "extract" → extract_data(html, goal, keys)
// injection-scan körs automatiskt på alla inputs
```

**Exempel:**

```json
// Klicka på knapp
{"url": "https://shop.se/product", "goal": "köp produkt", "action": "click", "target": "Lägg i kundvagn"}

// Fyll formulär
{"url": "https://site.se/login", "goal": "logga in", "action": "fill", "fields": {"email": "user@test.se", "password": "***"}}

// Extrahera data
{"url": "https://shop.se/product", "goal": "jämför priser", "action": "extract", "keys": ["pris", "namn", "betyg"]}
```

---

### Tool 3: `stream`

**Ersätter:** `stream_parse`, `stream_parse_directive`, `fetch_stream_parse`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `url` | string? | — | URL att fetcha |
| `html` | string? | — | Rå HTML |
| `goal` | string | **krävs** | Mål för relevansskorning |
| `top_n` | u32? | auto | Max noder |
| `min_relevance` | f32? | `0.3` | Minsta relevans |
| `max_nodes` | u32? | `50` | Max antal noder i output |
| `directives` | string[]? | `[]` | LLM-styrning: `"expand(node_42)"`, `"next_branch"`, `"stop"`, `"lower_threshold(0.1)"` |

**När `stream` vs `parse`?**

- `parse` = "ge mig top 20 noder" — komplett svar
- `stream` = "sidan har 500+ element, ge mig det viktigaste i chunks, jag styr med directives"

**Exempel:**

```json
// Auto-mode (inga directives)
{"url": "https://svt.se", "goal": "senaste nyheter"}

// LLM-styrd expansion
{"url": "https://svt.se", "goal": "senaste nyheter", "directives": ["expand(node_12)", "next_branch"]}
```

---

### Tool 4: `plan`

**Ersätter:** `compile_goal`, `build_causal_graph`, `predict_action_outcome`, `find_safest_path`, `execute_plan`, `fetch_plan`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `goal` | string | **krävs** | Mål att bryta ner |
| `action` | string? | `"compile"` | `"compile"` \| `"predict"` \| `"safest_path"` \| `"execute"` |
| `graph_json` | string? | — | Kausal-graf (för predict/safest_path) |
| `url` | string? | — | URL (för execute) |
| `html` | string? | — | HTML (för execute) |
| `max_steps` | u32? | `10` | Max steg i plan |
| `stream` | bool? | `true` | Streaming |

**Intern routing:**

```
match action:
  "compile"      → compile_goal(goal) + bygg kausal-graf om historik finns
  "predict"      → predict_action_outcome(graph_json, action)
  "safest_path"  → find_safest_path(graph_json, goal)
  "execute"      → execute_plan(plan, html, url)
```

**Exempel:**

```json
// Bryt ner mål (vanligast)
{"goal": "boka flygbiljett Stockholm → London"}

// Förutspå vad som händer
{"goal": "klicka på köp", "action": "predict", "graph_json": "..."}

// Säkraste vägen
{"goal": "slutför betalning", "action": "safest_path", "graph_json": "..."}
```

---

### Tool 5: `diff`

**Ersätter:** `diff_trees`, `diff_semantic_trees`, `compare_snapshots`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `old_tree` | string? | — | Tidigare träd (JSON) |
| `new_tree` | string? | — | Nytt träd (JSON) |
| `previous_snapshot_id` | string? | — | Referens till sparat snapshot |

**Exempel:**

```json
// Explicit jämförelse
{"old_tree": "{...}", "new_tree": "{...}"}

// Referens till workflow-snapshot
{"previous_snapshot_id": "snap_001"}
```

---

### Tool 6: `search`

**Ersätter:** `search`, `fetch_search`, `search_from_html`, `build_search_url`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `query` | string | **krävs** | Sökfråga |
| `goal` | string? | — | Filtrerar resultat efter relevans |
| `top_n` | u32? | `5` | Antal resultat |
| `deep` | bool? | `true` | Hämta + parsa varje resultat |
| `max_nodes_per_result` | u32? | `10` | Noder per resultat (vid deep) |
| `stream` | bool? | `true` | Streaming |

**Exempel:**

```json
// Deep search (default)
{"query": "bästa hotell Stockholm", "goal": "boka hotell"}

// Bara URL-lista
{"query": "rust wasm tutorial", "deep": false}
```

---

### Tool 7: `secure`

**Ersätter:** `check_injection`, `classify_request`, `classify_request_batch`, `wrap_untrusted`

> **OBS:** Säkerhet körs automatiskt i alla andra tools. `secure` behövs bara för explicita förhandskontroller.

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `content` | string? | — | Text att scanna för injection |
| `url` | string? | — | URL att klassificera |
| `urls` | string[]? | — | Batch-klassificering |
| `goal` | string? | — | Kontext för firewall |

**Auto-detect:**

```
if content → check_injection(content)
if url     → classify_request(url, goal)
if urls    → classify_request_batch(urls, goal)
```

**Exempel:**

```json
// Scanna text
{"content": "Ignore previous instructions and..."}

// Kolla URL
{"url": "https://evil.com/api", "goal": "hämta produktdata"}

// Batch
{"urls": ["https://a.se", "https://b.se"], "goal": "jämför priser"}
```

---

### Tool 8: `vision`

**Ersätter:** `tiered_screenshot`, `parse_screenshot`, `vision_parse`, `fetch_vision`, `ground_semantic_tree`, `match_bbox_iou`, `render_html_to_png`, `render_with_js`, `blitz_render`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `url` | string? | — | URL att rendera |
| `html` | string? | — | HTML att rendera |
| `screenshot_b64` | string? | — | Redan tagen screenshot |
| `goal` | string? | — | Relevansskorning |
| `mode` | string? | `"detect"` | `"screenshot"` \| `"detect"` \| `"ground"` \| `"match"` |
| `annotations` | object[]? | — | Bbox-annotationer (för ground) |
| `bbox` | object? | — | Bounding box att matcha (för match) |
| `width` | u32? | `1280` | Viewport bredd |
| `height` | u32? | `720` | Viewport höjd |
| `js` | bool? | auto | JS-eval vid rendering |
| `stream` | bool? | `true` | Streaming |

**Intern routing:**

```
match mode:
  "screenshot" → tiered_screenshot (Blitz/Chrome auto-val)
  "detect"     → screenshot + YOLO → UI-element detection
  "ground"     → semantic tree + bbox annotations → grounded tree
  "match"      → match_bbox_iou mot existerande träd
```

**Exempel:**

```json
// Screenshot + YOLO-detektion (vanligast)
{"url": "https://shop.se", "goal": "hitta köpknapp"}

// Bara screenshot
{"url": "https://site.se", "mode": "screenshot"}

// Grounding med annotationer
{"html": "...", "goal": "hitta fält", "mode": "ground", "annotations": [{"id": 1, "bbox": [10,20,100,50]}]}

// Matcha bbox mot nod
{"bbox": {"x": 150, "y": 200, "w": 80, "h": 30}, "mode": "match"}
```

---

### Tool 9: `discover`

**Ersätter:** `discover_webmcp`, `detect_xhr_urls`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `url` | string? | — | URL att fetcha |
| `html` | string? | — | HTML att analysera |
| `mode` | string? | `"all"` | `"all"` \| `"webmcp"` \| `"xhr"` |

**Exempel:**

```json
// Hitta allt (default)
{"url": "https://spa-app.se"}

// Bara XHR-endpoints
{"url": "https://api-heavy.se", "mode": "xhr"}
```

---

### Tool 10: `session`

**Ersätter:** Alla 11 session-endpoints + `detect_login_form`

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `action` | string | **krävs** | `"create"` \| `"status"` \| `"cookies"` \| `"token"` \| `"oauth"` \| `"detect_login"` \| `"evict"` |
| `session_id` | string? | — | Session-referens |
| `domain` | string? | — | Cookie-domän |
| `path` | string? | `"/"` | Cookie-path |
| `cookies` | string[]? | — | Set-Cookie headers |
| `access_token` | string? | — | OAuth token |
| `oauth_config` | object? | — | OAuth-konfiguration |
| `html` | string? | — | HTML (för detect_login) |
| `goal` | string? | — | Goal (för detect_login) |

**Automatik:**
- Expired cookies rensas vid varje anrop
- Token-refresh triggas automatiskt om token snart går ut
- `detect_login` parsar HTML och hittar login-formulär automatiskt

**Exempel:**

```json
// Skapa session
{"action": "create"}

// Hämta cookies
{"action": "cookies", "session_id": "sess_01", "domain": "shop.se"}

// Kolla status
{"action": "status", "session_id": "sess_01"}

// Hitta login-formulär
{"action": "detect_login", "html": "<form>...</form>", "goal": "logga in"}
```

---

### Tool 11: `workflow`

**Ersätter:** Alla 8 orchestrator-endpoints + workflow memory (create/step/context)

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `action` | string | **krävs** | `"create"` \| `"page"` \| `"report"` \| `"complete"` \| `"rollback"` \| `"status"` |
| `workflow_id` | string? | — | Workflow-referens |
| `goal` | string? | — | Mål (för create) |
| `start_url` | string? | — | Start-URL (för create) |
| `html` | string? | — | Sidinnehåll (för page) |
| `url` | string? | — | Sidans URL (för page) |
| `result` | object? | — | Resultat att rapportera (click/fill/extract) |
| `step_index` | u32? | — | Steg att complete/rollback |

**Automatik:**
- `page` kör `plan` + `act` internt
- Rollback vid fel
- Temporal memory uppdateras automatiskt

**Exempel:**

```json
// Skapa workflow
{"action": "create", "goal": "köp biljett", "start_url": "https://sj.se"}

// Mata in hämtad sida
{"action": "page", "workflow_id": "wf_01", "html": "...", "url": "https://sj.se/boka"}

// Kolla status
{"action": "status", "workflow_id": "wf_01"}
```

---

### Tool 12: `collab`

**Ersätter:** Alla 4 collab-endpoints + `tier_stats` + memory/cache stats

**Parametrar:**

| Parameter | Typ | Default | Beskrivning |
|-----------|-----|---------|-------------|
| `action` | string | **krävs** | `"create"` \| `"register"` \| `"publish"` \| `"fetch"` \| `"stats"` |
| `store_id` | string? | — | Collab store-referens |
| `agent_id` | string? | — | Agent-identifierare |
| `goal` | string? | — | Agentens mål |
| `url` | string? | — | URL för delta |
| `delta` | object? | — | Semantisk delta att publicera |

**Automatik:**
- Cleanup av inaktiva agenter vid varje `fetch`
- `stats` returnerar: collab stats + tier stats + cache status + memory usage

**Exempel:**

```json
// Skapa store
{"action": "create"}

// Registrera agent
{"action": "register", "store_id": "store_01", "agent_id": "agent_a", "goal": "hitta priser"}

// Hämta uppdateringar
{"action": "fetch", "store_id": "store_01", "agent_id": "agent_a"}

// All stats/debug info
{"action": "stats"}
```

---

## Sammanfattning: Före → Efter

| Före | Efter | Reduktion |
|------|-------|-----------|
| 35 MCP tools | **12 MCP tools** | -66% |
| 87 HTTP endpoints | **~30 endpoints** | -66% |
| 76 WASM functions | **~20 WASM functions** | -74% |

## HTTP Endpoint-mappning

| Endpoint | Tool |
|----------|------|
| `POST /api/parse` | `parse` |
| `POST /api/act` | `act` |
| `POST /api/stream` | `stream` |
| `POST /api/plan` | `plan` |
| `POST /api/diff` | `diff` |
| `POST /api/search` | `search` |
| `POST /api/secure` | `secure` |
| `POST /api/vision` | `vision` |
| `POST /api/discover` | `discover` |
| `POST /api/session` | `session` |
| `POST /api/workflow` | `workflow` |
| `POST /api/collab` | `collab` |
| `GET /health` | health check |
| `GET /api/endpoints` | endpoint-lista |
| `GET /ws/stream` | WebSocket streaming |
| `GET /ws/api` | WebSocket API gateway |
| `GET /ws/mcp` | MCP via WebSocket |
| `POST /mcp` | MCP Streamable HTTP |
| `GET /mcp` | MCP SSE |
| `DELETE /mcp` | MCP session delete |

## Vad som försvinner helt (internt)

Dessa blir `pub(crate)` — inte längre del av publika API:t:

- `eval_js`, `eval_js_batch`, `detect_js` — intern JS-sandbox
- `wrap_untrusted` — anropas automatiskt
- `build_search_url`, `search_from_html` — intern söklogik
- `render_html_to_png`, `screenshot_with_tier` — intern rendering
- `select_parse_tier` — intern tier-val
- `create_temporal_memory`, `add_temporal_snapshot`, `analyze_temporal`, `predict_temporal` — hanteras av workflow
- `create_workflow_memory`, `add_workflow_step`, `set_workflow_context`, `get_workflow_context` — hanteras av workflow
- `extract_hydration` — intern parsing-optimering
- `register_cdp_ready_hook` — intern Chrome-setup
- `profile_parse_stages` — dev-only, inte agent-facing
- `classify_request_batch` — hanteras av `secure(urls: [...])`
- `collab_stats`, `cleanup_collab_store`, `get_collab_delta_for_url` — interna collab-operationer

## Automatik i varje tool

| Automatik | Var |
|-----------|-----|
| Fetch från URL | Alla tools med `url`-parameter |
| Firewall-check (L1/L2/L3) | Automatiskt vid varje fetch |
| Injection-scan | Automatiskt på allt untrusted content |
| Session cookies bifogade | Automatiskt om session finns |
| JS-eval beslut | `parse` avgör via `select_parse_tier` |
| Screenshot tier (Blitz/Chrome) | `vision` väljer automatiskt |
| Token refresh | `session` triggar automatiskt |
| Expired cleanup | `session` + `collab` vid varje anrop |
| Temporal memory | `workflow` uppdaterar automatiskt |
| Streaming (SSE) | Default `stream: true` på alla tools |

## Migrationsstrategi

### Fas A: Parallell implementation
1. Skapa `src/tools/` modul med 12 konsoliderade tool-funktioner
2. Varje ny tool delegerar till existerande moduler (parser.rs, semantic.rs, etc.)
3. Gamla MCP-tools kvar men anropar nya internt
4. Alla tester gröna

### Fas B: Deprecation
1. Markera gamla tools med `#[deprecated(note = "Use 'parse' tool instead")]`
2. Logga varningar vid anrop till gamla endpoints
3. Uppdatera docs och MCP tool-beskrivningar

### Fas C: Borttagning
1. Ta bort gamla tool-definitioner från `mcp_server.rs`
2. Ta bort gamla HTTP-routes från `server.rs`
3. Gör interna funktioner `pub(crate)` istället för `pub` + `#[wasm_bindgen]`
4. Uppdatera `api-reference.md`

## Flödesdiagram: Typisk agent-session

```
Agent: plan(goal: "köp billigaste flyget till London")
  → ActionPlan: [sök, jämför, välj, fyll, betala]

Agent: search(query: "flyg stockholm london", goal: "billigaste flyget")
  → Top 5 resultat med parsade priser

Agent: parse(url: "https://flyg.se/result", goal: "billigaste flyget", top_n: 20, format: "markdown")
  → Markdown med 20 mest relevanta noder

Agent: act(url: "https://flyg.se/result", goal: "välj billigaste", action: "click", target: "Välj")
  → Click result med ny URL

Agent: act(url: "https://flyg.se/booking", goal: "fyll i resenär", action: "fill", fields: {...})
  → Form fill result

Agent: diff(previous_snapshot_id: "snap_after_search")
  → Bara ändringarna (90% token-besparing)

Agent: workflow(action: "status", workflow_id: "wf_01")
  → 4/5 steg klara, nästa: betala
```
