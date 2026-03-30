AetherAgent Tool Consolidation Analysis
Current State
Surface	Count
MCP Tools (mcp_server.rs)	35
HTTP Endpoints (server.rs)	87
WASM Functions (lib.rs)	76
Total Public Functions	208+
This is genuinely unwieldy. An LLM agent seeing 35 MCP tools will waste tokens just reading tool descriptions, and the overlap between tools creates decision paralysis.

Problem Analysis
1. Redundant fetch-variants
Nearly every tool has a fetch_* twin: parse + fetch_parse, extract + fetch_extract, click + fetch_click, stream_parse + fetch_stream_parse, vision_parse + fetch_vision. This doubles the tool count for what is just "should I fetch first? yes/no".

Fix: A single url parameter on each tool. If url is provided, fetch first. If html is provided, use it directly. No separate tools needed.

2. Parsing fragmentation
There are 7 different parse tools: parse, parse_top, parse_with_js, stream_parse, stream_parse_directive, parse_screenshot, vision_parse. An agent doesn't know which to pick.

Fix: One parse tool that auto-selects the right strategy based on input (html vs screenshot vs url) and parameters.

3. Security is scattered
check_injection, classify_request, wrap_untrusted are separate tools. But security should be automatic, not opt-in. An agent that forgets to call check_injection is vulnerable.

Fix: Security integrated into every tool call automatically. Expose a dedicated secure tool only for explicit standalone checks.

4. Stateful operations are verbose
Session management has 11 endpoints. Workflow orchestration has 8 endpoints. These are CRUD-style APIs that should be single tools with an action parameter.

5. Collab/memory tools are rarely used together
create_collab_store → register_collab_agent → publish_collab_delta → fetch_collab_deltas is always a sequential flow. Four tools for one capability.

Proposed Consolidation: 35 → 12 MCP Tools
Core Tools (always needed)
#	Tool	Replaces	Key Parameters	Rationale
1	parse	parse, parse_top, parse_with_js, parse_screenshot, vision_parse, fetch_parse, fetch_vision, render_with_js, html_to_markdown	url|html|screenshot_b64, goal, format: "tree|markdown", top_n?, js: bool?	One entry point. Auto-detects input type. If screenshot → YOLO. If html → semantic parse. If url → fetch first. js: true enables JS eval. top_n limits results.
2	act	find_and_click, fill_form, extract_data, fetch_click, fetch_extract	url|html, goal, action: "click|fill|extract", target?, fields?, keys?	All intent-driven DOM interaction in one tool.
3	stream	stream_parse, stream_parse_directive, fetch_stream_parse	url|html, goal, directives?, top_n?, min_relevance?, max_nodes?	Adaptive streaming with optional LLM directives. Default tool for large pages. Always stream: true.
4	plan	compile_goal, build_causal_graph, predict_action_outcome, find_safest_path, execute_plan	goal, action?: "compile|predict|safest_path|execute", graph_json?, html?	Goal decomposition + causal reasoning unified. Default action: compile.
5	diff	diff_trees	old_tree|previous_snapshot_id, new_tree?	Semantic diffing. Keep separate — it's a core token-saving primitive.
6	search	search, fetch_search	query, goal?, deep: bool?, top_n?	Web search. deep: true fetches+parses results.
Support Tools (used when needed)
#	Tool	Replaces	Key Parameters	Rationale
7	secure	check_injection, classify_request, classify_request_batch, detect_adversarial	content?|url?|urls?, goal?	Standalone security check. Auto-detects: text → injection scan, url → firewall classify, urls → batch classify. Note: security runs automatically inside all other tools too.
8	vision	tiered_screenshot, parse_screenshot, ground_semantic_tree, match_bbox_iou, blitz_render	url|html|screenshot_b64, goal, mode: "screenshot|detect|ground", annotations?, bbox?	All visual processing. screenshot = render, detect = YOLO, ground = bbox matching.
9	discover	discover_webmcp, detect_xhr_urls	url|html, mode: "all|webmcp|xhr"	Discovery of dynamic content sources and WebMCP tools. Default: all.
10	session	All 11 session endpoints + detect_login_form	action: "create|cookies|token|oauth|status|detect_login", session_id?, + action-specific params	Session lifecycle in one tool.
11	workflow	All 8 orchestrator endpoints + workflow memory	action: "create|page|report|complete|rollback|status", workflow_id?, + action-specific params	Workflow orchestration in one tool.
12	collab	All 4 collab endpoints + tier_stats + memory stats	action: "create|register|publish|fetch|stats", store_id?, + action-specific params	Multi-agent collaboration + observability stats.
What disappears entirely (folded into other tools)
Removed	Where it went
fetch_parse, fetch_click, fetch_extract, fetch_stream_parse, fetch_vision	url parameter on parse, act, stream, vision
parse_top	top_n parameter on parse
parse_with_js	js: true parameter on parse
render_with_js	vision with mode: "screenshot" + js: true
tier_stats	collab with action: "stats"
wrap_untrusted	Internal-only (called automatically)
eval_js, eval_js_batch, detect_js	Internal implementation details, not agent-facing tools
Differences from Your Proposal
I largely agree with your groupings, with these changes:

parse kept separate from stream — Parsing and streaming have fundamentally different return shapes. parse returns a complete tree; stream returns chunks with directive support. Merging them forces every call to specify "do I want streaming or not?" which adds cognitive load.

act instead of just action — Shorter name. Same concept: click/fill/extract unified.

ground folded into vision — Grounding is always used with visual data. No point having it separate. vision with mode: "ground" covers it.

stats folded into collab — Stats alone is too small to be a tool. collab action: "stats" returns tier stats, memory usage, cache status, and collab store stats in one call.

stream_directive folded into stream — Directives are just a parameter on streaming, not a separate tool. Send directives: [{"expand": "node_42"}, "next_branch"] directly.

No separate memory tool — Temporal memory and workflow memory are folded into workflow and plan respectively. Creating standalone memory stores that the agent must manually manage is an anti-pattern — the tools should manage their own state.

Integration Design: How Tools Work Together
                    ┌─────────┐
        ┌──────────►│  parse  │◄── html/url/screenshot auto-detect
        │           └────┬────┘
        │                │ SemanticTree
        │                ▼
        │           ┌─────────┐
        │     ┌────►│   act   │◄── click/fill/extract on tree
        │     │     └─────────┘
        │     │
  ┌─────┴─┐  │     ┌─────────┐
  │ search ├──┼────►│ stream  │◄── large pages, directive-driven
  └────────┘  │     └─────────┘
              │
  ┌────────┐  │     ┌─────────┐
  │  plan  ├──┼────►│  diff   │◄── compare snapshots
  └───┬────┘  │     └─────────┘
      │       │
      │  ┌────┴────┐  ┌──────────┐
      └─►│workflow  │  │ discover │◄── find XHR/WebMCP
         └────┬────┘  └──────────┘
              │
         ┌────┴────┐  ┌─────────┐
         │ session  │  │  vision │◄── screenshots/YOLO/grounding
         └─────────┘  └─────────┘
              
         ┌─────────┐  ┌─────────┐
         │ collab   │  │ secure  │◄── explicit security checks
         └─────────┘  └─────────┘

Key integration points:

Every tool accepts url and auto-fetches (with firewall check built in)
Every tool runs check_injection on untrusted content automatically
stream: true is default on parse, act, stream — responses come as SSE chunks
plan output feeds directly into workflow input
vision output (bounding boxes) feeds into act for coordinate-based clicking
diff accepts snapshot IDs from workflow state, not just raw JSON
HTTP Endpoint Reduction: 87 → ~30
The HTTP API mirrors the MCP tool structure. Each tool gets one POST endpoint:

Endpoint	Tool
POST /api/parse	parse
POST /api/act	act
POST /api/stream	stream
POST /api/plan	plan
POST /api/diff	diff
POST /api/search	search
POST /api/secure	secure
POST /api/vision	vision
POST /api/discover	discover
POST /api/session	session
POST /api/workflow	workflow
POST /api/collab	collab
Plus infrastructure: GET /health, GET /api/endpoints, WebSocket endpoints, MCP streamable HTTP.

The old endpoints remain as deprecated aliases for one release cycle, then are removed.

WASM API Reduction: 76 → ~20
The WASM surface follows the same 12-tool model. Each MCP tool maps to 1-2 WASM functions. Internal helpers (like eval_js_batch, detect_js, individual session operations) become pub(crate) instead of pub + #[wasm_bindgen].

Migration Strategy
Phase A: Add the 12 new consolidated tools alongside existing ones. Old tools call into new implementations internally.
Phase B: Mark old tools #[deprecated] with messages pointing to new tool names.
Phase C: Remove old tools after one release.
This avoids breaking existing integrations while cleaning up the API surface.



parse hanterar allt detta:

parse(
  url: "https://example.com",
  goal: "hitta produktpriser",
  top_n: 20,                    // ← top 20 noder
  format: "markdown"            // ← "tree" | "markdown"
)

top_n: 20 → returnerar bara de 20 mest goal-relevanta noderna (ersätter parse_top)
format: "markdown" → returnerar markdown istället för JSON-träd (ersätter html_to_markdown / semantic_tree_to_markdown)
format: "tree" (default) → returnerar semantiskt träd som JSON
Så ett typiskt LLM-flöde blir:

1. parse(url, goal, top_n: 20, format: "markdown")  → kompakt markdown att skicka till LLM
2. act(url, goal, action: "click", target: "Lägg i kundvagn")  → interagera
3. diff(previous_snapshot_id)  → se vad som ändrades (token-sparande)

stream är för riktigt stora sidor där man vill ha chunked emission med LLM-styrda directives. Men för "ge mig top 20 noder som markdown" är parse rätt verktyg — enklare och snabbare.


parse(
  url | html | screenshot_b64,   // input (auto-detect)
  goal,
  top_n: 20,                     // begränsa antal noder
  format: "tree | markdown",     // output-format
  js: true | false,              // JS-eval före parse
  stream: true | false           // chunked SSE eller komplett svar
)

Exempel på vanliga anrop:

Use case  Anrop
Snabb LLM-kontext parse(url, goal, top_n: 20, format: "markdown")
Fullständigt träd parse(url, goal)
Screenshot-analys parse(screenshot_b64, goal) → auto-detect → YOLO
JS-tung sida  parse(url, goal, js: true)
Lokal HTML  parse(html: "<div>...", goal)
Samma verktyg, en implementation internt som routar baserat på vad som skickas in. Agenten behöver bara lära sig ett verktygsnamn för all parsing.

Vill du att jag börjar implementera konsolideringen, eller vill du justera något i de 12 verktygen först?

Och dom andra verktygen vi slår ihop. Hur funkar det för dom? Slår vi ihop dom? Och funkar visa med automatik utan parametrar?



Ja, samma princip överallt. Här är alla 12 med defaults och auto-detect:

1. parse — redan beskrivet
parse(url, goal, top_n?: 20, format?: "markdown")

2. act — all interaktion
act(
  url | html,
  goal,
  action: "click" | "fill" | "extract",  // krävs
  target?: "Köp nu",          // för click
  fields?: {"email": "a@b"},  // för fill
  keys?: ["pris", "namn"]     // för extract
)

Automatik: Firewall-check på url, injection-scan på alla fält, session-cookies bifogas om session finns.

3. stream — stora sidor, chunked
stream(url | html, goal)
// det är allt som behövs — resten har defaults

Automatik utan parametrar:

top_n: auto (baserat på sidstorlek)
min_relevance: 0.3 (default)
max_nodes: 50 (default)
directives: tom → full auto-mode
Skicka directives bara om LLM vill styra: directives: ["expand(node_42)", "next_branch"]

4. plan — goal → steg
plan(goal)
// det räcker — default action är "compile"

Vad du vill Anrop
Bryt ner mål  plan(goal)
Förutspå resultat plan(goal, action: "predict", graph_json)
Säkraste vägen  plan(goal, action: "safest_path", graph_json)
Kör plan  plan(goal, action: "execute", html, url)
Automatik: compile bygger kausal-graf internt om historik finns. Ingen manuell build_causal_graph behövs.

5. diff — jämför snapshots
diff(previous_snapshot_id)
// det är allt — hämtar automatiskt senaste som "new"

Eller explicit: diff(old_tree, new_tree) med rå JSON.

6. search — webbsökning
search(query, goal?)

Automatik: deep: true som default → söker + hämtar + parsar top 3 resultat. goal filtrerar relevans.

7. secure — explicit säkerhetscheck
secure(content)           // → injection-scan
secure(url, goal)         // → firewall-classify
secure(urls: [...], goal) // → batch-classify

Auto-detect baserat på vad som skickas. Men viktigast: detta körs automatiskt inuti alla andra verktyg. secure som explicit tool behövs bara för "kolla detta innan jag gör något".

8. vision — allt visuellt
vision(url, goal)
// default mode: "detect" → tar screenshot + YOLO

Vad du vill Anrop
Screenshot + YOLO vision(url, goal)
Bara screenshot vision(url, goal, mode: "screenshot")
Grounding med bbox  vision(url, goal, mode: "ground", annotations)
Matcha bbox → nod vision(bbox, mode: "match")
Automatik: Tiered backend väljer Blitz vs Chrome automatiskt.

9. discover — hitta dolda resurser
discover(url | html)
// default mode: "all" → söker WebMCP + XHR

Automatik: Returnerar allt den hittar utan att du behöver ange mode.

10. session — sessionshantering
session(action: "create")                              // skapa
session(action: "status", session_id)                  // kolla status
session(action: "cookies", session_id, domain, path?)  // hämta cookies

Automatik: Expired cookies rensas vid varje anrop. Token-refresh triggas automatiskt om token håller på att gå ut.

11. workflow — orkestrering
workflow(action: "create", goal, start_url)
workflow(action: "page", workflow_id, html, url)    // mata in sida
workflow(action: "status", workflow_id)              // var är vi?

Automatik: provide_page kör plan + act internt och rapporterar resultat automatiskt. Rollback vid fel.

12. collab — multi-agent
collab(action: "create")
collab(action: "register", store_id, agent_id, goal)
collab(action: "publish", store_id, agent_id, url, delta)
collab(action: "fetch", store_id, agent_id)
collab(action: "stats")   // inkl tier_stats, cache, memory

Automatik: Cleanup av inaktiva agenter körs vid varje fetch.

Sammanfattning: vad som är automatiskt
Sak Var det händer
Fetch från URL  Alla verktyg med url-parameter
Firewall-check  Automatiskt vid fetch
Injection-scan  Automatiskt på allt untrusted content
Cookie-hantering  Automatiskt om session finns
JS-eval beslut  parse avgör via select_parse_tier
Screenshot tier vision väljer Blitz/Chrome automatiskt
Token refresh session triggar automatiskt
Expired cleanup session + collab vid varje anrop
Principen: Noll parametrar = vettig default. Parametrar finns för att överstyrа, inte för att saker ska fungera.