#![recursion_limit = "256"]
/// AetherAgent HTTP API Server
///
/// Lightweight REST wrapper around the AetherAgent engine.
/// Deploy to Render, Fly.io, or any container host.
///
/// Run: cargo run --features server --bin aether-server
use axum::{
    extract::Json,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use tower_http::cors::{Any, CorsLayer};

// ─── Request/Response types ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct ParseRequest {
    html: String,
    goal: String,
    url: String,
}

#[derive(Deserialize)]
struct ParseTopRequest {
    html: String,
    goal: String,
    url: String,
    top_n: u32,
}

#[derive(Deserialize)]
struct ClickRequest {
    html: String,
    goal: String,
    url: String,
    target_label: String,
}

#[derive(Deserialize)]
struct FillFormRequest {
    html: String,
    goal: String,
    url: String,
    fields: HashMap<String, String>,
}

#[derive(Deserialize)]
struct ExtractRequest {
    html: String,
    goal: String,
    url: String,
    keys: Vec<String>,
}

#[derive(Deserialize)]
struct InjectionCheckRequest {
    text: String,
}

#[derive(Deserialize)]
struct WrapRequest {
    content: String,
}

#[derive(Deserialize)]
struct AddStepRequest {
    memory_json: String,
    action: String,
    url: String,
    goal: String,
    summary: String,
}

#[derive(Deserialize)]
struct ContextSetRequest {
    memory_json: String,
    key: String,
    value: String,
}

#[derive(Deserialize)]
struct ContextGetRequest {
    memory_json: String,
    key: String,
}

#[derive(Deserialize)]
struct DiffRequest {
    old_tree_json: String,
    new_tree_json: String,
}

#[derive(Deserialize)]
struct DetectJsRequest {
    html: String,
}

#[derive(Deserialize)]
struct EvalJsRequest {
    code: String,
}

#[derive(Deserialize)]
struct EvalJsBatchRequest {
    snippets: Vec<String>,
}

// ─── Fas 5: Temporal Memory request types ────────────────────────────────────

#[derive(Deserialize)]
struct TemporalSnapshotRequest {
    memory_json: String,
    html: String,
    goal: String,
    url: String,
    timestamp_ms: u64,
}

#[derive(Deserialize)]
struct TemporalAnalyzeRequest {
    memory_json: String,
}

#[derive(Deserialize)]
struct TemporalPredictRequest {
    memory_json: String,
}

// ─── Fas 6: Compiler request types ───────────────────────────────────────────

#[derive(Deserialize)]
struct CompileGoalRequest {
    goal: String,
}

#[derive(Deserialize)]
struct ExecutePlanRequest {
    plan_json: String,
    html: String,
    goal: String,
    url: String,
    completed_steps: Vec<u32>,
}

// ─── Fas 8: Firewall request types ──────────────────────────────────────────

#[derive(Deserialize)]
struct FirewallClassifyRequest {
    url: String,
    goal: String,
    #[serde(default)]
    config: Option<aether_agent::firewall::FirewallConfig>,
}

#[derive(Deserialize)]
struct FirewallBatchRequest {
    urls: Vec<String>,
    goal: String,
    #[serde(default)]
    config: Option<aether_agent::firewall::FirewallConfig>,
}

// ─── Fas 7: Fetch request types ─────────────────────────────────────────────

#[derive(Deserialize)]
struct FetchParseRequest {
    url: String,
    goal: String,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
}

#[derive(Deserialize)]
struct FetchClickRequest {
    url: String,
    goal: String,
    target_label: String,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
}

#[derive(Deserialize)]
struct FetchExtractRequest {
    url: String,
    goal: String,
    keys: Vec<String>,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
}

#[derive(Deserialize)]
struct FetchPlanRequest {
    url: String,
    goal: String,
    #[serde(default)]
    completed_steps: Vec<u32>,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
}

#[derive(Deserialize)]
struct FetchRawRequest {
    url: String,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
}

// ─── Fas 9a: Causal Action Graph request types ──────────────────────────────

#[derive(Deserialize)]
struct BuildCausalGraphRequest {
    snapshots_json: String,
    actions_json: String,
}

#[derive(Deserialize)]
struct PredictOutcomeRequest {
    graph_json: String,
    action: String,
}

#[derive(Deserialize)]
struct SafestPathRequest {
    graph_json: String,
    goal: String,
}

// ─── Fas 9b: WebMCP Discovery request types ─────────────────────────────────

#[derive(Deserialize)]
struct WebMcpDiscoverRequest {
    html: String,
    url: String,
}

// ─── Fas 9c: Multimodal Grounding request types ─────────────────────────────

#[derive(Deserialize)]
struct GroundTreeRequest {
    html: String,
    goal: String,
    url: String,
    annotations: serde_json::Value,
}

#[derive(Deserialize)]
struct MatchBboxRequest {
    tree_json: String,
    bbox: serde_json::Value,
}

// ─── Fas 9d: Cross-Agent Diffing request types ──────────────────────────────

#[derive(Deserialize)]
struct CollabRegisterRequest {
    store_json: String,
    agent_id: String,
    goal: String,
    timestamp_ms: u64,
}

#[derive(Deserialize)]
struct CollabPublishRequest {
    store_json: String,
    agent_id: String,
    url: String,
    delta_json: String,
    timestamp_ms: u64,
}

#[derive(Deserialize)]
struct CollabFetchRequest {
    store_json: String,
    agent_id: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn root() -> impl IntoResponse {
    let body = serde_json::json!({
        "engine": "AetherAgent",
        "version": "0.2.0",
        "description": "LLM-native browser engine – semantic perception layer for AI agents",
        "endpoints": {
            "GET /health": "Health check",
            "POST /api/parse": "Parse HTML to semantic tree",
            "POST /api/parse-top": "Parse top-N relevant nodes",
            "POST /api/click": "Find best clickable element",
            "POST /api/fill-form": "Map form fields",
            "POST /api/extract": "Extract structured data",
            "POST /api/diff": "Semantic diff between two trees (token savings)",
            "POST /api/parse-js": "Parse HTML with automatic JS evaluation",
            "POST /api/detect-js": "Detect JavaScript snippets in HTML",
            "POST /api/eval-js": "Evaluate JS expression in sandbox",
            "POST /api/eval-js-batch": "Evaluate multiple JS expressions",
            "POST /api/check-injection": "Check text for prompt injection",
            "POST /api/wrap-untrusted": "Wrap content in trust markers",
            "POST /api/memory/create": "Create workflow memory",
            "POST /api/memory/step": "Add workflow step",
            "POST /api/memory/context/set": "Set context key/value",
            "POST /api/memory/context/get": "Get context value",
            "POST /api/temporal/create": "Create temporal memory",
            "POST /api/temporal/snapshot": "Add temporal snapshot",
            "POST /api/temporal/analyze": "Analyze temporal patterns",
            "POST /api/temporal/predict": "Predict next page state",
            "POST /api/compile": "Compile goal to action plan",
            "POST /api/execute-plan": "Execute plan against page state",
            "POST /api/fetch": "Fetch URL and return HTML + metadata",
            "POST /api/fetch/parse": "Fetch URL → parse to semantic tree",
            "POST /api/fetch/click": "Fetch URL → find clickable element",
            "POST /api/fetch/extract": "Fetch URL → extract structured data",
            "POST /api/fetch/plan": "Fetch URL → compile goal → execute plan",
            "POST /api/firewall/classify": "Classify URL against semantic firewall (L1/L2/L3)",
            "POST /api/firewall/classify-batch": "Classify batch of URLs against firewall",
            "POST /api/causal/build": "Build causal action graph from temporal history",
            "POST /api/causal/predict": "Predict outcome of an action",
            "POST /api/causal/safest-path": "Find safest path to goal state",
            "POST /api/webmcp/discover": "Discover WebMCP tools in HTML page",
            "POST /api/ground": "Ground semantic tree with bounding boxes",
            "POST /api/ground/match-bbox": "Match bounding box via IoU against tree nodes",
            "POST /api/collab/create": "Create shared diff store for cross-agent collaboration",
            "POST /api/collab/register": "Register agent in collab store",
            "POST /api/collab/publish": "Publish semantic delta to collab store",
            "POST /api/collab/fetch": "Fetch new deltas for agent",
            "POST /mcp": "MCP Streamable HTTP endpoint (JSON-RPC, spec 2025-03-26)"
        },
        "example": {
            "curl": "curl -X POST /api/parse -H 'Content-Type: application/json' -d '{\"html\": \"<button>Buy</button>\", \"goal\": \"buy\", \"url\": \"https://shop.com\"}'",
        },
        "source": "https://github.com/robinandreeklund-collab/AetherAgent"
    });
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        serde_json::to_string_pretty(&body).unwrap_or_default(),
    )
}

async fn health() -> impl IntoResponse {
    let result = aether_agent::health_check();
    (StatusCode::OK, result)
}

async fn parse(Json(req): Json<ParseRequest>) -> impl IntoResponse {
    let result = aether_agent::parse_to_semantic_tree(&req.html, &req.goal, &req.url);
    (StatusCode::OK, result)
}

async fn parse_top(Json(req): Json<ParseTopRequest>) -> impl IntoResponse {
    let result = aether_agent::parse_top_nodes(&req.html, &req.goal, &req.url, req.top_n);
    (StatusCode::OK, result)
}

async fn click(Json(req): Json<ClickRequest>) -> impl IntoResponse {
    let result = aether_agent::find_and_click(&req.html, &req.goal, &req.url, &req.target_label);
    (StatusCode::OK, result)
}

async fn fill_form(Json(req): Json<FillFormRequest>) -> impl IntoResponse {
    let fields_json = match serde_json::to_string(&req.fields) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                serde_json::to_string(&ErrorResponse {
                    error: format!("Invalid fields: {}", e),
                })
                .unwrap_or_default(),
            )
        }
    };
    let result = aether_agent::fill_form(&req.html, &req.goal, &req.url, &fields_json);
    (StatusCode::OK, result)
}

async fn extract(Json(req): Json<ExtractRequest>) -> impl IntoResponse {
    let keys_json = match serde_json::to_string(&req.keys) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                serde_json::to_string(&ErrorResponse {
                    error: format!("Invalid keys: {}", e),
                })
                .unwrap_or_default(),
            )
        }
    };
    let result = aether_agent::extract_data(&req.html, &req.goal, &req.url, &keys_json);
    (StatusCode::OK, result)
}

async fn check_injection(Json(req): Json<InjectionCheckRequest>) -> impl IntoResponse {
    let result = aether_agent::check_injection(&req.text);
    (StatusCode::OK, result)
}

async fn wrap_untrusted(Json(req): Json<WrapRequest>) -> impl IntoResponse {
    let result = aether_agent::wrap_untrusted(&req.content);
    (StatusCode::OK, result)
}

async fn diff(Json(req): Json<DiffRequest>) -> impl IntoResponse {
    let result = aether_agent::diff_semantic_trees(&req.old_tree_json, &req.new_tree_json);
    (StatusCode::OK, result)
}

async fn parse_with_js(Json(req): Json<ParseRequest>) -> impl IntoResponse {
    let result = aether_agent::parse_with_js(&req.html, &req.goal, &req.url);
    (StatusCode::OK, result)
}

async fn detect_js(Json(req): Json<DetectJsRequest>) -> impl IntoResponse {
    let result = aether_agent::detect_js(&req.html);
    (StatusCode::OK, result)
}

async fn eval_js_handler(Json(req): Json<EvalJsRequest>) -> impl IntoResponse {
    let result = aether_agent::eval_js(&req.code);
    (StatusCode::OK, result)
}

async fn eval_js_batch_handler(Json(req): Json<EvalJsBatchRequest>) -> impl IntoResponse {
    let snippets_json = match serde_json::to_string(&req.snippets) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                serde_json::to_string(&ErrorResponse {
                    error: format!("Invalid snippets: {}", e),
                })
                .unwrap_or_default(),
            )
        }
    };
    let result = aether_agent::eval_js_batch(&snippets_json);
    (StatusCode::OK, result)
}

async fn create_memory() -> impl IntoResponse {
    let result = aether_agent::create_workflow_memory();
    (StatusCode::OK, result)
}

async fn add_step(Json(req): Json<AddStepRequest>) -> impl IntoResponse {
    let result = aether_agent::add_workflow_step(
        &req.memory_json,
        &req.action,
        &req.url,
        &req.goal,
        &req.summary,
    );
    (StatusCode::OK, result)
}

async fn set_context(Json(req): Json<ContextSetRequest>) -> impl IntoResponse {
    let result = aether_agent::set_workflow_context(&req.memory_json, &req.key, &req.value);
    (StatusCode::OK, result)
}

async fn get_context(Json(req): Json<ContextGetRequest>) -> impl IntoResponse {
    let result = aether_agent::get_workflow_context(&req.memory_json, &req.key);
    (StatusCode::OK, result)
}

// ─── Fas 5: Temporal Memory handlers ─────────────────────────────────────────

async fn create_temporal_memory() -> impl IntoResponse {
    let result = aether_agent::create_temporal_memory();
    (StatusCode::OK, result)
}

async fn add_temporal_snapshot(Json(req): Json<TemporalSnapshotRequest>) -> impl IntoResponse {
    let result = aether_agent::add_temporal_snapshot(
        &req.memory_json,
        &req.html,
        &req.goal,
        &req.url,
        req.timestamp_ms,
    );
    (StatusCode::OK, result)
}

async fn analyze_temporal(Json(req): Json<TemporalAnalyzeRequest>) -> impl IntoResponse {
    let result = aether_agent::analyze_temporal(&req.memory_json);
    (StatusCode::OK, result)
}

async fn predict_temporal(Json(req): Json<TemporalPredictRequest>) -> impl IntoResponse {
    let result = aether_agent::predict_temporal(&req.memory_json);
    (StatusCode::OK, result)
}

// ─── Fas 6: Compiler handlers ───────────────────────────────────────────────

async fn compile_goal_handler(Json(req): Json<CompileGoalRequest>) -> impl IntoResponse {
    let result = aether_agent::compile_goal(&req.goal);
    (StatusCode::OK, result)
}

async fn execute_plan_handler(Json(req): Json<ExecutePlanRequest>) -> impl IntoResponse {
    let completed_json = match serde_json::to_string(&req.completed_steps) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                serde_json::to_string(&ErrorResponse {
                    error: format!("Invalid completed_steps: {}", e),
                })
                .unwrap_or_default(),
            )
        }
    };
    let result = aether_agent::execute_plan(
        &req.plan_json,
        &req.html,
        &req.goal,
        &req.url,
        &completed_json,
    );
    (StatusCode::OK, result)
}

// ─── Fas 8: Firewall handlers ───────────────────────────────────────────────

async fn firewall_classify(Json(req): Json<FirewallClassifyRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();
    let verdict = aether_agent::firewall::classify_request(&req.url, &req.goal, &config);
    (
        StatusCode::OK,
        serde_json::to_string(&verdict).unwrap_or_default(),
    )
}

async fn firewall_classify_batch(Json(req): Json<FirewallBatchRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();
    let (verdicts, summary) = aether_agent::firewall::classify_batch(&req.urls, &req.goal, &config);
    let result = serde_json::json!({
        "verdicts": verdicts,
        "summary": summary,
    });
    (
        StatusCode::OK,
        serde_json::to_string(&result).unwrap_or_default(),
    )
}

// ─── Fas 7: Fetch handlers ──────────────────────────────────────────────────

async fn fetch_raw(Json(req): Json<FetchRawRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();

    if let Err(e) = aether_agent::fetch::validate_url(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        );
    }

    match aether_agent::fetch::fetch_page(&req.url, &config).await {
        Ok(result) => (
            StatusCode::OK,
            serde_json::to_string(&result).unwrap_or_default(),
        ),
        Err(e) => (
            StatusCode::BAD_GATEWAY,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        ),
    }
}

async fn fetch_parse(Json(req): Json<FetchParseRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();

    if let Err(e) = aether_agent::fetch::validate_url(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        );
    }

    let total_start = std::time::Instant::now();

    let fetch_result = match aether_agent::fetch::fetch_page(&req.url, &config).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
            )
        }
    };

    let tree_json = aether_agent::parse_to_semantic_tree(
        &fetch_result.body,
        &req.goal,
        &fetch_result.final_url,
    );
    let total_time_ms = total_start.elapsed().as_millis() as u64;

    let result = aether_agent::types::FetchAndParseResult {
        fetch: fetch_result,
        tree: serde_json::from_str(&tree_json).unwrap_or_else(|_| {
            aether_agent::types::SemanticTree {
                url: String::new(),
                title: String::new(),
                goal: req.goal.clone(),
                nodes: vec![],
                injection_warnings: vec![],
                parse_time_ms: 0,
                xhr_intercepted: 0,
                xhr_blocked: 0,
            }
        }),
        total_time_ms,
    };

    (
        StatusCode::OK,
        serde_json::to_string(&result).unwrap_or_default(),
    )
}

async fn fetch_click(Json(req): Json<FetchClickRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();

    if let Err(e) = aether_agent::fetch::validate_url(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        );
    }

    let total_start = std::time::Instant::now();

    let fetch_result = match aether_agent::fetch::fetch_page(&req.url, &config).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
            )
        }
    };

    let click_json = aether_agent::find_and_click(
        &fetch_result.body,
        &req.goal,
        &fetch_result.final_url,
        &req.target_label,
    );
    let total_time_ms = total_start.elapsed().as_millis() as u64;

    let result = aether_agent::types::FetchAndClickResult {
        fetch: fetch_result,
        click: serde_json::from_str(&click_json)
            .unwrap_or_else(|_| aether_agent::types::ClickResult::not_found(vec![], 0)),
        total_time_ms,
    };

    (
        StatusCode::OK,
        serde_json::to_string(&result).unwrap_or_default(),
    )
}

async fn fetch_extract(Json(req): Json<FetchExtractRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();

    if let Err(e) = aether_agent::fetch::validate_url(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        );
    }

    let total_start = std::time::Instant::now();

    let fetch_result = match aether_agent::fetch::fetch_page(&req.url, &config).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
            )
        }
    };

    let keys_json = match serde_json::to_string(&req.keys) {
        Ok(j) => j,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                serde_json::to_string(&ErrorResponse {
                    error: format!("Invalid keys: {e}"),
                })
                .unwrap_or_default(),
            )
        }
    };

    let extract_json = aether_agent::extract_data(
        &fetch_result.body,
        &req.goal,
        &fetch_result.final_url,
        &keys_json,
    );
    let total_time_ms = total_start.elapsed().as_millis() as u64;

    let result = aether_agent::types::FetchAndExtractResult {
        fetch: fetch_result,
        extract: serde_json::from_str(&extract_json).unwrap_or_else(|_| {
            aether_agent::types::ExtractDataResult {
                entries: vec![],
                missing_keys: req.keys.clone(),
                injection_warnings: vec![],
                parse_time_ms: 0,
            }
        }),
        total_time_ms,
    };

    (
        StatusCode::OK,
        serde_json::to_string(&result).unwrap_or_default(),
    )
}

async fn fetch_plan(Json(req): Json<FetchPlanRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();

    if let Err(e) = aether_agent::fetch::validate_url(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        );
    }

    let total_start = std::time::Instant::now();

    let fetch_result = match aether_agent::fetch::fetch_page(&req.url, &config).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
            )
        }
    };

    let plan_json = aether_agent::compile_goal(&req.goal);
    let completed_json = serde_json::to_string(&req.completed_steps).unwrap_or_default();
    let execution_json = aether_agent::execute_plan(
        &plan_json,
        &fetch_result.body,
        &req.goal,
        &fetch_result.final_url,
        &completed_json,
    );
    let total_time_ms = total_start.elapsed().as_millis() as u64;

    let result = aether_agent::types::FetchAndPlanResult {
        fetch: fetch_result,
        plan_json,
        execution_json,
        total_time_ms,
    };

    (
        StatusCode::OK,
        serde_json::to_string(&result).unwrap_or_default(),
    )
}

// ─── Fas 9a: Causal Action Graph handlers ────────────────────────────────────

async fn build_causal_graph(Json(req): Json<BuildCausalGraphRequest>) -> impl IntoResponse {
    let result = aether_agent::build_causal_graph(&req.snapshots_json, &req.actions_json);
    (StatusCode::OK, result)
}

async fn predict_outcome(Json(req): Json<PredictOutcomeRequest>) -> impl IntoResponse {
    let result = aether_agent::predict_action_outcome(&req.graph_json, &req.action);
    (StatusCode::OK, result)
}

async fn safest_path(Json(req): Json<SafestPathRequest>) -> impl IntoResponse {
    let result = aether_agent::find_safest_path(&req.graph_json, &req.goal);
    (StatusCode::OK, result)
}

// ─── Fas 9b: WebMCP Discovery handlers ──────────────────────────────────────

async fn webmcp_discover(Json(req): Json<WebMcpDiscoverRequest>) -> impl IntoResponse {
    let result = aether_agent::discover_webmcp(&req.html, &req.url);
    (StatusCode::OK, result)
}

// ─── Fas 9c: Multimodal Grounding handlers ──────────────────────────────────

async fn ground_tree(Json(req): Json<GroundTreeRequest>) -> impl IntoResponse {
    let annotations_json =
        serde_json::to_string(&req.annotations).unwrap_or_else(|_| "[]".to_string());
    let result =
        aether_agent::ground_semantic_tree(&req.html, &req.goal, &req.url, &annotations_json);
    (StatusCode::OK, result)
}

async fn match_bbox(Json(req): Json<MatchBboxRequest>) -> impl IntoResponse {
    let bbox_json = serde_json::to_string(&req.bbox).unwrap_or_else(|_| "{}".to_string());
    let result = aether_agent::match_bbox_iou(&req.tree_json, &bbox_json);
    (StatusCode::OK, result)
}

// ─── Fas 9d: Cross-Agent Diffing handlers ───────────────────────────────────

async fn collab_create() -> impl IntoResponse {
    let result = aether_agent::create_collab_store();
    (StatusCode::OK, result)
}

async fn collab_register(Json(req): Json<CollabRegisterRequest>) -> impl IntoResponse {
    let result = aether_agent::register_collab_agent(
        &req.store_json,
        &req.agent_id,
        &req.goal,
        req.timestamp_ms,
    );
    (StatusCode::OK, result)
}

async fn collab_publish(Json(req): Json<CollabPublishRequest>) -> impl IntoResponse {
    let result = aether_agent::publish_collab_delta(
        &req.store_json,
        &req.agent_id,
        &req.url,
        &req.delta_json,
        req.timestamp_ms,
    );
    (StatusCode::OK, result)
}

async fn collab_fetch(Json(req): Json<CollabFetchRequest>) -> impl IntoResponse {
    let result = aether_agent::fetch_collab_deltas(&req.store_json, &req.agent_id);
    (StatusCode::OK, result)
}

// ─── MCP Streamable HTTP (spec 2025-03-26) ───────────────────────────────────

/// Tool schema som exponeras via MCP tools/list
fn mcp_tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "parse",
            "description": "Parse HTML to a semantic accessibility tree with goal-relevance scoring.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"}
                },
                "required": ["html", "goal", "url"]
            }
        },
        {
            "name": "parse_top",
            "description": "Parse HTML and return only the top-N most relevant nodes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"},
                    "top_n": {"type": "integer", "description": "Max nodes to return"}
                },
                "required": ["html", "goal", "url", "top_n"]
            }
        },
        {
            "name": "fetch_parse",
            "description": "Fetch a URL and parse it into a semantic tree in one call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch and parse"},
                    "goal": {"type": "string", "description": "The agent's current goal"}
                },
                "required": ["url", "goal"]
            }
        },
        {
            "name": "find_and_click",
            "description": "Find the best clickable element matching a target label.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"},
                    "target_label": {"type": "string", "description": "What to click"}
                },
                "required": ["html", "goal", "url", "target_label"]
            }
        },
        {
            "name": "fill_form",
            "description": "Map form fields to provided key/value pairs.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"},
                    "fields": {"type": "object", "description": "Form fields as key-value map", "additionalProperties": {"type": "string"}}
                },
                "required": ["html", "goal", "url", "fields"]
            }
        },
        {
            "name": "extract_data",
            "description": "Extract structured data from a page by semantic keys.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"},
                    "keys": {"type": "array", "items": {"type": "string"}, "description": "Keys to extract"}
                },
                "required": ["html", "goal", "url", "keys"]
            }
        },
        {
            "name": "check_injection",
            "description": "Check text for prompt injection patterns.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {"type": "string", "description": "Text to check"}
                },
                "required": ["text"]
            }
        },
        {
            "name": "compile_goal",
            "description": "Compile a complex goal into an optimized action plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": {"type": "string", "description": "The agent's goal"}
                },
                "required": ["goal"]
            }
        },
        {
            "name": "classify_request",
            "description": "Classify URL against semantic firewall (L1/L2/L3).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to classify"},
                    "goal": {"type": "string", "description": "The agent's current goal"}
                },
                "required": ["url", "goal"]
            }
        },
        {
            "name": "diff_trees",
            "description": "Compare two semantic trees and return only the delta. 70-99% token savings.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "old_tree_json": {"type": "string", "description": "Previous semantic tree JSON"},
                    "new_tree_json": {"type": "string", "description": "Current semantic tree JSON"}
                },
                "required": ["old_tree_json", "new_tree_json"]
            }
        },
        {
            "name": "fetch_extract",
            "description": "Fetch a URL and extract structured data in one call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "keys": {"type": "array", "items": {"type": "string"}, "description": "Keys to extract"}
                },
                "required": ["url", "goal", "keys"]
            }
        },
        {
            "name": "fetch_click",
            "description": "Fetch a URL and find a clickable element in one call.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "target_label": {"type": "string", "description": "What to click"}
                },
                "required": ["url", "goal", "target_label"]
            }
        },
        {
            "name": "parse_with_js",
            "description": "Parse HTML with automatic JS evaluation in sandboxed Boa engine.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"}
                },
                "required": ["html", "goal", "url"]
            }
        },
        {
            "name": "build_causal_graph",
            "description": "Build a causal action graph from temporal page snapshots and actions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "snapshots_json": {"type": "string", "description": "JSON array of temporal snapshots"},
                    "actions_json": {"type": "string", "description": "JSON array of actions"}
                },
                "required": ["snapshots_json", "actions_json"]
            }
        },
        {
            "name": "predict_action_outcome",
            "description": "Predict the outcome of an action using the causal graph.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "graph_json": {"type": "string", "description": "Causal graph JSON"},
                    "action": {"type": "string", "description": "Action to predict"}
                },
                "required": ["graph_json", "action"]
            }
        },
        {
            "name": "find_safest_path",
            "description": "Find the safest path to a goal state through the causal graph.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "graph_json": {"type": "string", "description": "Causal graph JSON"},
                    "goal": {"type": "string", "description": "Target goal state"}
                },
                "required": ["graph_json", "goal"]
            }
        },
        {
            "name": "discover_webmcp",
            "description": "Discover WebMCP tool definitions embedded in an HTML page.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "url": {"type": "string", "description": "The page URL"}
                },
                "required": ["html", "url"]
            }
        },
        {
            "name": "ground_semantic_tree",
            "description": "Ground semantic tree with visual bounding box annotations.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"},
                    "annotations": {"type": "array", "description": "Bounding box annotations"}
                },
                "required": ["html", "goal", "url", "annotations"]
            }
        },
        {
            "name": "match_bbox_iou",
            "description": "Match a bounding box against semantic tree nodes using IoU.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tree_json": {"type": "string", "description": "Semantic tree JSON"},
                    "bbox": {"type": "object", "description": "Bounding box {x, y, width, height}"}
                },
                "required": ["tree_json", "bbox"]
            }
        },
        {
            "name": "create_collab_store",
            "description": "Create a shared diff store for cross-agent collaboration.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "register_collab_agent",
            "description": "Register an agent in a collaboration store.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "store_json": {"type": "string", "description": "Collab store JSON"},
                    "agent_id": {"type": "string", "description": "Unique agent identifier"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "timestamp_ms": {"type": "integer", "description": "Current timestamp in ms"}
                },
                "required": ["store_json", "agent_id", "goal", "timestamp_ms"]
            }
        },
        {
            "name": "publish_collab_delta",
            "description": "Publish a semantic delta to the collaboration store.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "store_json": {"type": "string", "description": "Collab store JSON"},
                    "agent_id": {"type": "string", "description": "Publishing agent's ID"},
                    "url": {"type": "string", "description": "URL the delta applies to"},
                    "delta_json": {"type": "string", "description": "Semantic delta JSON"},
                    "timestamp_ms": {"type": "integer", "description": "Current timestamp in ms"}
                },
                "required": ["store_json", "agent_id", "url", "delta_json", "timestamp_ms"]
            }
        },
        {
            "name": "fetch_collab_deltas",
            "description": "Fetch new semantic deltas from the collaboration store.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "store_json": {"type": "string", "description": "Collab store JSON"},
                    "agent_id": {"type": "string", "description": "Fetching agent's ID"}
                },
                "required": ["store_json", "agent_id"]
            }
        }
    ])
}

/// Dispatcha MCP tools/call till rätt aether_agent-funktion
async fn mcp_dispatch_tool(name: &str, args: &serde_json::Value) -> Result<String, String> {
    match name {
        "parse" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            Ok(aether_agent::parse_to_semantic_tree(html, goal, url))
        }
        "parse_top" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let top_n = args["top_n"].as_u64().unwrap_or(10) as u32;
            Ok(aether_agent::parse_top_nodes(html, goal, url, top_n))
        }
        "fetch_parse" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            aether_agent::fetch::validate_url(url)?;
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(url, &config).await {
                Ok(r) => {
                    let tree = aether_agent::parse_to_semantic_tree(&r.body, goal, &r.final_url);
                    Ok(tree)
                }
                Err(e) => Err(e),
            }
        }
        "find_and_click" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let target = args["target_label"].as_str().unwrap_or("");
            Ok(aether_agent::find_and_click(html, goal, url, target))
        }
        "fill_form" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let fields_json = serde_json::to_string(&args["fields"]).unwrap_or_default();
            Ok(aether_agent::fill_form(html, goal, url, &fields_json))
        }
        "extract_data" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let keys_json = serde_json::to_string(&args["keys"]).unwrap_or_default();
            Ok(aether_agent::extract_data(html, goal, url, &keys_json))
        }
        "check_injection" => {
            let text = args["text"].as_str().unwrap_or("");
            Ok(aether_agent::check_injection(text))
        }
        "compile_goal" => {
            let goal = args["goal"].as_str().unwrap_or("");
            Ok(aether_agent::compile_goal(goal))
        }
        "classify_request" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let config = aether_agent::firewall::FirewallConfig::default();
            let verdict = aether_agent::firewall::classify_request(url, goal, &config);
            serde_json::to_string(&verdict).map_err(|e| e.to_string())
        }
        "diff_trees" => {
            let old = args["old_tree_json"].as_str().unwrap_or("");
            let new = args["new_tree_json"].as_str().unwrap_or("");
            Ok(aether_agent::diff_semantic_trees(old, new))
        }
        "fetch_extract" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let keys_json = serde_json::to_string(&args["keys"]).unwrap_or_default();
            aether_agent::fetch::validate_url(url)?;
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(url, &config).await {
                Ok(r) => Ok(aether_agent::extract_data(
                    &r.body,
                    goal,
                    &r.final_url,
                    &keys_json,
                )),
                Err(e) => Err(e),
            }
        }
        "fetch_click" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let target = args["target_label"].as_str().unwrap_or("");
            aether_agent::fetch::validate_url(url)?;
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(url, &config).await {
                Ok(r) => Ok(aether_agent::find_and_click(
                    &r.body,
                    goal,
                    &r.final_url,
                    target,
                )),
                Err(e) => Err(e),
            }
        }
        "parse_with_js" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            Ok(aether_agent::parse_with_js(html, goal, url))
        }
        "build_causal_graph" => {
            let snapshots = args["snapshots_json"].as_str().unwrap_or("");
            let actions = args["actions_json"].as_str().unwrap_or("");
            Ok(aether_agent::build_causal_graph(snapshots, actions))
        }
        "predict_action_outcome" => {
            let graph = args["graph_json"].as_str().unwrap_or("");
            let action = args["action"].as_str().unwrap_or("");
            Ok(aether_agent::predict_action_outcome(graph, action))
        }
        "find_safest_path" => {
            let graph = args["graph_json"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            Ok(aether_agent::find_safest_path(graph, goal))
        }
        "discover_webmcp" => {
            let html = args["html"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            Ok(aether_agent::discover_webmcp(html, url))
        }
        "ground_semantic_tree" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let ann_json = serde_json::to_string(&args["annotations"]).unwrap_or_default();
            Ok(aether_agent::ground_semantic_tree(
                html, goal, url, &ann_json,
            ))
        }
        "match_bbox_iou" => {
            let tree = args["tree_json"].as_str().unwrap_or("");
            let bbox_json = serde_json::to_string(&args["bbox"]).unwrap_or_default();
            Ok(aether_agent::match_bbox_iou(tree, &bbox_json))
        }
        "create_collab_store" => Ok(aether_agent::create_collab_store()),
        "register_collab_agent" => {
            let store = args["store_json"].as_str().unwrap_or("");
            let agent_id = args["agent_id"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let ts = args["timestamp_ms"].as_u64().unwrap_or(0);
            Ok(aether_agent::register_collab_agent(
                store, agent_id, goal, ts,
            ))
        }
        "publish_collab_delta" => {
            let store = args["store_json"].as_str().unwrap_or("");
            let agent_id = args["agent_id"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let delta = args["delta_json"].as_str().unwrap_or("");
            let ts = args["timestamp_ms"].as_u64().unwrap_or(0);
            Ok(aether_agent::publish_collab_delta(
                store, agent_id, url, delta, ts,
            ))
        }
        "fetch_collab_deltas" => {
            let store = args["store_json"].as_str().unwrap_or("");
            let agent_id = args["agent_id"].as_str().unwrap_or("");
            Ok(aether_agent::fetch_collab_deltas(store, agent_id))
        }
        _ => Err(format!("Unknown tool: {name}")),
    }
}

/// Skapa JSON-RPC-svar
fn jsonrpc_result(id: &serde_json::Value, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    })
}

fn jsonrpc_error(id: &serde_json::Value, code: i32, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {"code": code, "message": message}
    })
}

/// MCP Streamable HTTP POST handler
/// Hanterar initialize, tools/list, tools/call, notifications, och ping
async fn mcp_post(headers: HeaderMap, Json(msg): Json<serde_json::Value>) -> impl IntoResponse {
    let method = msg["method"].as_str().unwrap_or("");
    let id = &msg["id"];
    let params = &msg["params"];

    // Notification (inget id) — acceptera tyst
    if id.is_null() {
        return (StatusCode::ACCEPTED, HeaderMap::new(), String::new());
    }

    let response = match method {
        "initialize" => {
            let session_id = format!(
                "aether-{:016x}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            );
            let result = jsonrpc_result(
                id,
                serde_json::json!({
                    "protocolVersion": "2025-03-26",
                    "capabilities": {
                        "tools": {"listChanged": false}
                    },
                    "serverInfo": {
                        "name": "aether-agent",
                        "version": "0.3.0"
                    }
                }),
            );
            let body = serde_json::to_string(&result).unwrap_or_default();
            let mut resp_headers = HeaderMap::new();
            resp_headers.insert(
                "content-type",
                "application/json"
                    .parse()
                    .unwrap_or_else(|_| "application/json".parse().unwrap()),
            );
            if let Ok(v) = session_id.parse() {
                resp_headers.insert("mcp-session-id", v);
            }
            return (StatusCode::OK, resp_headers, body);
        }
        "tools/list" => jsonrpc_result(
            id,
            serde_json::json!({
                "tools": mcp_tool_definitions()
            }),
        ),
        "tools/call" => {
            let tool_name = params["name"].as_str().unwrap_or("");
            let arguments = &params["arguments"];
            match mcp_dispatch_tool(tool_name, arguments).await {
                Ok(result_text) => jsonrpc_result(
                    id,
                    serde_json::json!({
                        "content": [{"type": "text", "text": result_text}]
                    }),
                ),
                Err(e) => jsonrpc_result(
                    id,
                    serde_json::json!({
                        "content": [{"type": "text", "text": format!("Error: {e}")}],
                        "isError": true
                    }),
                ),
            }
        }
        "ping" => jsonrpc_result(id, serde_json::json!({})),
        _ => jsonrpc_error(id, -32601, &format!("Method not found: {method}")),
    };

    // Vidarebefordra session-id från klienten
    let mut resp_headers = HeaderMap::new();
    resp_headers.insert(
        "content-type",
        "application/json"
            .parse()
            .unwrap_or_else(|_| "application/json".parse().unwrap()),
    );
    if let Some(session) = headers.get("mcp-session-id") {
        resp_headers.insert("mcp-session-id", session.clone());
    }
    (
        StatusCode::OK,
        resp_headers,
        serde_json::to_string(&response).unwrap_or_default(),
    )
}

/// MCP Streamable HTTP GET handler — SSE stream (returnerar tom 405 för nu)
async fn mcp_get() -> impl IntoResponse {
    // Server-initiated notifications behövs inte ännu
    (
        StatusCode::METHOD_NOT_ALLOWED,
        "SSE stream not implemented — use POST",
    )
}

/// MCP Streamable HTTP DELETE handler — avsluta session
async fn mcp_delete() -> impl IntoResponse {
    (StatusCode::OK, "Session terminated")
}

// ─── Router ──────────────────────────────────────────────────────────────────

fn build_router() -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Root & Health
        .route("/", get(root))
        .route("/health", get(health))
        // Fas 1: Semantic parsing
        .route("/api/parse", post(parse))
        .route("/api/parse-top", post(parse_top))
        .route("/api/check-injection", post(check_injection))
        .route("/api/wrap-untrusted", post(wrap_untrusted))
        // Fas 4a: Semantic diff
        .route("/api/diff", post(diff))
        // Fas 4c: Selective execution
        .route("/api/parse-js", post(parse_with_js))
        // Fas 4b: JS sandbox
        .route("/api/detect-js", post(detect_js))
        .route("/api/eval-js", post(eval_js_handler))
        .route("/api/eval-js-batch", post(eval_js_batch_handler))
        // Fas 2: Intent API
        .route("/api/click", post(click))
        .route("/api/fill-form", post(fill_form))
        .route("/api/extract", post(extract))
        // Fas 2: Workflow memory
        .route("/api/memory/create", post(create_memory))
        .route("/api/memory/step", post(add_step))
        .route("/api/memory/context/set", post(set_context))
        .route("/api/memory/context/get", post(get_context))
        // Fas 5: Temporal Memory
        .route("/api/temporal/create", post(create_temporal_memory))
        .route("/api/temporal/snapshot", post(add_temporal_snapshot))
        .route("/api/temporal/analyze", post(analyze_temporal))
        .route("/api/temporal/predict", post(predict_temporal))
        // Fas 6: Intent Compiler
        .route("/api/compile", post(compile_goal_handler))
        .route("/api/execute-plan", post(execute_plan_handler))
        // Fas 8: Semantic Firewall
        .route("/api/firewall/classify", post(firewall_classify))
        .route(
            "/api/firewall/classify-batch",
            post(firewall_classify_batch),
        )
        // Fas 7: HTTP Fetch
        .route("/api/fetch", post(fetch_raw))
        .route("/api/fetch/parse", post(fetch_parse))
        .route("/api/fetch/click", post(fetch_click))
        .route("/api/fetch/extract", post(fetch_extract))
        .route("/api/fetch/plan", post(fetch_plan))
        // Fas 9a: Causal Action Graph
        .route("/api/causal/build", post(build_causal_graph))
        .route("/api/causal/predict", post(predict_outcome))
        .route("/api/causal/safest-path", post(safest_path))
        // Fas 9b: WebMCP Discovery
        .route("/api/webmcp/discover", post(webmcp_discover))
        // Fas 9c: Multimodal Grounding
        .route("/api/ground", post(ground_tree))
        .route("/api/ground/match-bbox", post(match_bbox))
        // Fas 9d: Cross-Agent Diffing
        .route("/api/collab/create", post(collab_create))
        .route("/api/collab/register", post(collab_register))
        .route("/api/collab/publish", post(collab_publish))
        .route("/api/collab/fetch", post(collab_fetch))
        // MCP Streamable HTTP (spec 2025-03-26)
        .route("/mcp", post(mcp_post).get(mcp_get).delete(mcp_delete))
        .layer(cors)
}

#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("AetherAgent API server starting on http://{}", addr);
    println!("Endpoints:");
    println!("  GET  /health              – Health check");
    println!("  GET  /                    – API documentation");
    println!("  POST /api/parse           – Parse HTML to semantic tree");
    println!("  POST /api/parse-top       – Parse top-N relevant nodes");
    println!("  POST /api/click           – Find best clickable element");
    println!("  POST /api/fill-form       – Map form fields");
    println!("  POST /api/extract         – Extract structured data");
    println!("  POST /api/diff            – Semantic diff between trees");
    println!("  POST /api/parse-js        – Parse with JS evaluation");
    println!("  POST /api/detect-js       – Detect JS snippets in HTML");
    println!("  POST /api/eval-js         – Evaluate JS in sandbox");
    println!("  POST /api/eval-js-batch   – Batch JS evaluation");
    println!("  POST /api/check-injection – Check text for injection");
    println!("  POST /api/wrap-untrusted  – Wrap content in trust markers");
    println!("  POST /api/memory/*        – Workflow memory operations");
    println!("  POST /api/temporal/*      – Temporal memory & adversarial modeling");
    println!("  POST /api/compile         – Compile goal to action plan");
    println!("  POST /api/execute-plan    – Execute plan against page state");
    println!("  POST /api/fetch           – Fetch URL → HTML + metadata");
    println!("  POST /api/fetch/parse     – Fetch URL → semantic tree");
    println!("  POST /api/fetch/click     – Fetch URL → find element");
    println!("  POST /api/fetch/extract   – Fetch URL → extract data");
    println!("  POST /api/fetch/plan      – Fetch URL → compile + execute plan");
    println!("  POST /api/firewall/classify      – Classify URL against firewall");
    println!("  POST /api/firewall/classify-batch – Classify batch of URLs");
    println!("  POST /api/causal/build           – Build causal action graph");
    println!("  POST /api/causal/predict         – Predict action outcome");
    println!("  POST /api/causal/safest-path     – Find safest path to goal");
    println!("  POST /api/webmcp/discover        – Discover WebMCP tools in HTML");
    println!("  POST /api/ground                 – Ground tree with bounding boxes");
    println!("  POST /api/ground/match-bbox      – Match bbox via IoU");
    println!("  POST /api/collab/create          – Create collab diff store");
    println!("  POST /api/collab/register        – Register agent in collab");
    println!("  POST /api/collab/publish         – Publish delta to collab store");
    println!("  POST /api/collab/fetch           – Fetch new deltas for agent");
    println!();
    println!("  POST /mcp                        – MCP Streamable HTTP endpoint");
    println!("  GET  /mcp                        – MCP SSE stream (not yet implemented)");
    println!("  DELETE /mcp                      – Terminate MCP session");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");
    axum::serve(listener, build_router())
        .await
        .expect("Server error");
}
