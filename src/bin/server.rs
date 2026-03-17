/// AetherAgent HTTP API Server
///
/// Lightweight REST wrapper around the AetherAgent engine.
/// Deploy to Render, Fly.io, or any container host.
///
/// Run: cargo run --features server --bin aether-server
use axum::{
    extract::Json,
    http::StatusCode,
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
            "POST /api/firewall/classify-batch": "Classify batch of URLs against firewall"
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

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");
    axum::serve(listener, build_router())
        .await
        .expect("Server error");
}
