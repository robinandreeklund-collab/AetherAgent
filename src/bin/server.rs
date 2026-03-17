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
            "POST /api/memory/context/get": "Get context value"
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

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");
    axum::serve(listener, build_router())
        .await
        .expect("Server error");
}
