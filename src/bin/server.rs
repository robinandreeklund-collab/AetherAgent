#![recursion_limit = "256"]
/// AetherAgent HTTP API Server
///
/// Lightweight REST wrapper around the AetherAgent engine.
/// Deploy to Render, Fly.io, or any container host.
///
/// Run: cargo run --features server --bin aether-server
use axum::{
    extract::{DefaultBodyLimit, Json},
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse,
    },
    routing::{get, post},
    Router,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
// [RTEN-ROLLBACK-ID:server-rwlock] Gamla: use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

/// Max render-tid i sekunder — förhindrar att tunga sidor (t.ex. github.com ~569KB) hänger servern
const RENDER_TIMEOUT_SECS: u64 = 10;

/// Delat server-state med förladdad vision-modell (ORT session med Mutex för &mut run)
// [RTEN-ROLLBACK-ID:server-state] Gamla: vision_model: Arc<RwLock<Option<Arc<rten::Model>>>>
#[derive(Clone)]
struct AppState {
    #[cfg(feature = "vision")]
    vision_model: Arc<std::sync::Mutex<Option<ort::session::Session>>>,
    #[cfg(not(feature = "vision"))]
    vision_model: Arc<std::sync::Mutex<Option<()>>>,
    /// Broadcast-kanal för MCP SSE-events (dashboard live feed)
    mcp_events: Arc<tokio::sync::broadcast::Sender<String>>,
    /// Ring-buffer med senaste MCP-events för polling-fallback
    mcp_event_log: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
}

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
    #[serde(default = "default_timestamp_ms")]
    timestamp_ms: u64,
}

#[derive(Deserialize)]
struct CollabPublishRequest {
    store_json: String,
    agent_id: String,
    url: String,
    delta_json: String,
    #[serde(default = "default_timestamp_ms")]
    timestamp_ms: u64,
}

fn default_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Deserialize)]
struct CollabFetchRequest {
    store_json: String,
    agent_id: String,
}

#[derive(Deserialize)]
struct DetectXhrRequest {
    html: String,
}

#[derive(Deserialize)]
struct ParseScreenshotRequest {
    png_base64: String,
    model_base64: String,
    goal: String,
}

#[derive(Deserialize)]
struct ParseScreenshotServerModelRequest {
    png_base64: String,
    goal: String,
}

// ─── Fas 16: Stream Parse request types ─────────────────────────────────────

#[derive(Deserialize)]
struct StreamParseRequest {
    html: String,
    goal: String,
    url: String,
    #[serde(default = "default_top_n")]
    top_n: usize,
    #[serde(default = "default_min_relevance")]
    min_relevance: f32,
    #[serde(default = "default_max_nodes")]
    max_nodes: usize,
}

fn default_top_n() -> usize {
    10
}

fn default_min_relevance() -> f32 {
    0.3
}

fn default_max_nodes() -> usize {
    50
}

#[derive(Deserialize)]
struct FetchStreamParseRequest {
    url: String,
    goal: String,
    #[serde(default = "default_top_n")]
    top_n: usize,
    #[serde(default = "default_min_relevance")]
    min_relevance: f32,
    #[serde(default = "default_max_nodes")]
    max_nodes: usize,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
}

#[derive(Deserialize)]
struct DirectiveRequest {
    directives: Vec<serde_json::Value>,
    html: String,
    goal: String,
    url: String,
    #[serde(default = "default_top_n")]
    top_n: usize,
    #[serde(default = "default_min_relevance")]
    min_relevance: f32,
    #[serde(default = "default_max_nodes")]
    max_nodes: usize,
}

// ─── Fas 13: Session Management request types ──────────────────────────────

#[derive(Deserialize)]
struct SessionAddCookiesRequest {
    session_json: String,
    domain: String,
    cookies: Vec<String>,
}

#[derive(Deserialize)]
struct SessionGetCookiesRequest {
    session_json: String,
    domain: String,
    #[serde(default = "default_path")]
    path: String,
}

fn default_path() -> String {
    "/".to_string()
}

#[derive(Deserialize)]
struct SessionSetTokenRequest {
    session_json: String,
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in_secs: u64,
    #[serde(default)]
    scopes: Vec<String>,
}

#[derive(Deserialize)]
struct SessionOAuthRequest {
    session_json: String,
    /// OAuth config — antingen som nested objekt eller som individuella fält
    #[serde(default)]
    config: Option<serde_json::Value>,
    #[serde(default)]
    auth_url: Option<String>,
    #[serde(default)]
    client_id: Option<String>,
    #[serde(default)]
    redirect_uri: Option<String>,
    #[serde(default)]
    scopes: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct SessionTokenExchangeRequest {
    session_json: String,
    config: serde_json::Value,
    authorization_code: String,
}

#[derive(Deserialize)]
struct SessionStatusRequest {
    session_json: String,
}

#[derive(Deserialize)]
struct DetectLoginFormRequest {
    html: String,
    goal: String,
    url: String,
}

// ─── Fas 14: Workflow Orchestration request types ───────────────────────────

#[derive(Deserialize)]
struct CreateWorkflowRequest {
    goal: String,
    start_url: String,
    #[serde(default)]
    config: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct WorkflowProvidePageRequest {
    orchestrator_json: String,
    html: String,
    url: String,
}

#[derive(Deserialize)]
struct WorkflowReportClickRequest {
    orchestrator_json: String,
    click_result_json: String,
}

#[derive(Deserialize)]
struct WorkflowReportFillRequest {
    orchestrator_json: String,
    fill_result_json: String,
}

#[derive(Deserialize)]
struct WorkflowReportExtractRequest {
    orchestrator_json: String,
    extract_result_json: String,
}

#[derive(Deserialize)]
struct WorkflowStepRequest {
    orchestrator_json: String,
    step_index: u32,
}

#[derive(Deserialize)]
struct WorkflowStatusRequest {
    orchestrator_json: String,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn root() -> impl IntoResponse {
    let html = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>AetherAgent — LLM-native browser engine</title>
<style>
  @import url('https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;700&display=swap');
  *{margin:0;padding:0;box-sizing:border-box}
  body{
    background:#0a0e14;
    color:#b3b1ad;
    font-family:'JetBrains Mono',monospace;
    min-height:100vh;
    display:flex;
    flex-direction:column;
    align-items:center;
    justify-content:center;
    padding:2rem 1rem;
    overflow-x:hidden;
  }
  .scanline{
    position:fixed;top:0;left:0;width:100%;height:100%;
    pointer-events:none;z-index:10;
    background:repeating-linear-gradient(
      0deg,
      rgba(0,0,0,0.03) 0px,
      rgba(0,0,0,0.03) 1px,
      transparent 1px,
      transparent 2px
    );
  }
  .glow{
    position:fixed;top:50%;left:50%;
    transform:translate(-50%,-50%);
    width:600px;height:600px;
    background:radial-gradient(circle,rgba(255,180,50,0.04) 0%,transparent 70%);
    pointer-events:none;z-index:0;
  }
  .terminal{
    position:relative;z-index:1;
    max-width:820px;width:100%;
    border:1px solid #1d2433;
    border-radius:8px;
    background:#0d1117;
    box-shadow:0 0 40px rgba(0,0,0,0.5),0 0 80px rgba(255,180,50,0.03);
  }
  .titlebar{
    display:flex;align-items:center;gap:8px;
    padding:12px 16px;
    background:#161b22;
    border-bottom:1px solid #1d2433;
    border-radius:8px 8px 0 0;
  }
  .dot{width:12px;height:12px;border-radius:50%}
  .dot.r{background:#ff5f57}
  .dot.y{background:#ffbd2e}
  .dot.g{background:#28c840}
  .titlebar-text{
    flex:1;text-align:center;
    color:#484f58;font-size:13px;
  }
  .content{padding:24px 28px 32px;line-height:1.5}
  .mascot{
    color:#e6e1cf;
    font-size:10px;
    line-height:1.15;
    letter-spacing:0.5px;
    text-align:center;
    margin-bottom:16px;
    white-space:pre;
  }
  .mascot .hair{color:#8b949e}
  .mascot .visor{color:#58a6ff}
  .mascot .eyes{color:#1a1e24}
  .mascot .bolt{color:#ffbd2e}
  .mascot .wing{color:#c9d1d9}
  .mascot .body{color:#e6e1cf}
  .title-art{
    text-align:center;
    margin-bottom:8px;
    white-space:pre;
    line-height:1.1;
  }
  .title-art span{
    background:linear-gradient(135deg,#ffbd2e 0%,#ff8c00 50%,#ffbd2e 100%);
    -webkit-background-clip:text;
    -webkit-text-fill-color:transparent;
    background-clip:text;
    font-size:11px;
    font-weight:700;
  }
  .subtitle{
    text-align:center;
    color:#ffbd2e;
    font-size:14px;
    font-weight:700;
    letter-spacing:2px;
    margin-bottom:20px;
  }
  .tagline{
    text-align:center;
    color:#8b949e;
    font-size:13px;
    max-width:560px;
    margin:0 auto 28px;
    line-height:1.6;
  }
  .tagline em{color:#c9d1d9;font-style:normal}
  .prompt{color:#484f58;margin-bottom:4px;font-size:13px}
  .prompt .path{color:#58a6ff}
  .prompt .sym{color:#ffbd2e}
  .cmd{color:#c9d1d9}
  .response{color:#7ee787;margin-bottom:16px;font-size:13px}
  .endpoints{
    margin-top:20px;
    border-top:1px solid #1d2433;
    padding-top:20px;
  }
  .endpoints h3{
    color:#484f58;font-size:12px;
    text-transform:uppercase;letter-spacing:2px;
    margin-bottom:12px;
  }
  .ep-grid{
    display:grid;
    grid-template-columns:repeat(auto-fill,minmax(280px,1fr));
    gap:6px;
  }
  .ep{font-size:12px;display:flex;gap:8px}
  .ep .method{
    color:#1a1e24;
    background:#7ee787;
    padding:1px 5px;
    border-radius:3px;
    font-size:10px;
    font-weight:700;
    flex-shrink:0;
    min-width:36px;
    text-align:center;
  }
  .ep .method.post{background:#da8ee7;color:#1a1e24}
  .ep .route{color:#58a6ff}
  .ep .desc{color:#484f58}
  .footer{
    text-align:center;
    margin-top:24px;
    padding-top:16px;
    border-top:1px solid #1d2433;
  }
  .footer a{
    color:#58a6ff;text-decoration:none;font-size:13px;
  }
  .footer a:hover{text-decoration:underline}
  .cursor{
    display:inline-block;
    width:8px;height:16px;
    background:#ffbd2e;
    animation:blink 1s step-end infinite;
    vertical-align:middle;
    margin-left:4px;
  }
  @keyframes blink{50%{opacity:0}}
  @keyframes flicker{
    0%,100%{opacity:1}
    92%{opacity:1}
    93%{opacity:0.8}
    94%{opacity:1}
  }
  .terminal{animation:flicker 8s infinite}
</style>
</head>
<body>
<div class="scanline"></div>
<div class="glow"></div>
<div class="terminal">
  <div class="titlebar">
    <div class="dot r"></div>
    <div class="dot y"></div>
    <div class="dot g"></div>
    <div class="titlebar-text">aether@agent ~ /engine</div>
  </div>
  <div class="content">

<div class="mascot"><span class="hair">              ,  ~  ,
           ( ~  @@  ~ )
            ' ,_@@_. '</span>
        <span class="visor"> ┌──[<span class="eyes">●●●</span>]──┐</span>
        <span class="visor"> │          │</span>
        <span class="visor"> └────┬─────┘</span>
        <span class="body">      │ <span class="eyes">●</span>  <span class="eyes">●</span> │</span>
        <span class="body">      │  __  │</span>
        <span class="body">      └──┬───┘</span>
  <span class="wing">  ╱╲</span><span class="body">    ┌─┴─┐</span>
  <span class="wing"> ╱  ╲</span><span class="body">   │ <span class="bolt">⚡</span> │</span>
  <span class="wing">╱    ╲</span><span class="body">  │   │</span>
  <span class="wing"> ╲  ╱</span><span class="body">   ├───┤</span>
  <span class="wing">  ╲╱</span><span class="body">    │   │</span>
        <span class="body">   ╱╲  ╱╲</span>
        <span class="body">  ╱  ╲╱  ╲</span>
        <span class="body">  ▔▔   ▔▔</span></div>

<div class="title-art"><span>
 █████╗ ███████╗████████╗██╗  ██╗███████╗██████╗
██╔══██╗██╔════╝╚══██╔══╝██║  ██║██╔════╝██╔══██╗
███████║█████╗     ██║   ███████║█████╗  ██████╔╝
██╔══██║██╔══╝     ██║   ██╔══██║██╔══╝  ██╔══██╗
██║  ██║███████╗   ██║   ██║  ██║███████╗██║  ██║
╚═╝  ╚═╝╚══════╝   ╚═╝   ╚═╝  ╚═╝╚══════╝╚═╝  ╚═╝
 █████╗  ██████╗ ███████╗███╗   ██╗████████╗
██╔══██╗██╔════╝ ██╔════╝████╗  ██║╚══██╔══╝
███████║██║  ███╗█████╗  ██╔██╗ ██║   ██║
██╔══██║██║   ██║██╔══╝  ██║╚██╗██║   ██║
██║  ██║╚██████╔╝███████╗██║ ╚████║   ██║
╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝  ╚═══╝   ╚═╝</span></div>

<div class="subtitle">The LLM-native browser engine.</div>

<div class="tagline">
  <em>Semantic perception</em>, <em>goal-aware intelligence</em>, and
  <em>prompt injection protection</em> — in a single embeddable
  Rust/WASM library.
</div>

<div>
  <div class="prompt"><span class="path">~/agent</span> <span class="sym">$</span> <span class="cmd">curl -X POST /api/parse -d '{"html": "&lt;button&gt;Buy&lt;/button&gt;", "goal": "buy"}'</span></div>
  <div class="response">{"nodes": [{"role": "button", "label": "Buy", "relevance": 0.95, "trust": "safe"}]}</div>

  <div class="prompt"><span class="path">~/agent</span> <span class="sym">$</span> <span class="cmd">curl /api/endpoints</span></div>
  <div class="response">{"status": "ok", "count": 50, "docs": "/api/endpoints"}</div>

  <div class="prompt"><span class="path">~/agent</span> <span class="sym">$</span><span class="cursor"></span></div>
</div>

<div class="endpoints">
  <h3>// API Surface — 50+ endpoints</h3>
  <div class="ep-grid">
    <div class="ep"><span class="method post">POST</span><span class="route">/api/parse</span><span class="desc"> — semantic tree</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/click</span><span class="desc"> — find element</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/extract</span><span class="desc"> — structured data</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/fill-form</span><span class="desc"> — map fields</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/diff</span><span class="desc"> — semantic diff</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/compile</span><span class="desc"> — goal → plan</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/fetch/parse</span><span class="desc"> — fetch + parse</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/fetch/plan</span><span class="desc"> — fetch + plan</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/firewall/classify</span><span class="desc"> — L1/L2/L3</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/detect-xhr</span><span class="desc"> — XHR scan</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/search</span><span class="desc"> — DDG search (pre-fetched HTML)</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/fetch/search</span><span class="desc"> — DDG search (auto-fetch)</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/api/parse-screenshot</span><span class="desc"> — vision</span></div>
    <div class="ep"><span class="method post">POST</span><span class="route">/mcp</span><span class="desc"> — MCP endpoint</span></div>
    <div class="ep"><span class="method">GET</span><span class="route">/health</span><span class="desc"> — health check</span></div>
    <div class="ep"><span class="method">GET</span><span class="route">/api/endpoints</span><span class="desc"> — full API list</span></div>
  </div>
</div>

<div class="footer">
  <a href="https://github.com/robinandreeklund-collab/AetherAgent">github.com/robinandreeklund-collab/AetherAgent</a>
</div>

  </div>
</div>
</body>
</html>"##;
    (
        StatusCode::OK,
        [("content-type", "text/html; charset=utf-8")],
        html,
    )
}

/// JSON API endpoint listing (moved from root)
async fn api_endpoints() -> impl IntoResponse {
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
            "POST /api/fetch/markdown": "Fetch URL → convert to Markdown",
            "POST /api/markdown": "Convert HTML to Markdown",
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
            "POST /api/detect-xhr": "Scan HTML for XHR/fetch/AJAX endpoints in scripts",
            "POST /api/search": "Parse pre-fetched DDG HTML into structured search results",
            "POST /api/fetch/search": "Search via DuckDuckGo: fetch + parse in one call",
            "POST /api/parse-screenshot": "Analyze screenshot with YOLOv8-nano vision model",
            "POST /api/session/create": "Create empty session manager",
            "POST /api/session/cookies/add": "Add cookies from Set-Cookie headers",
            "POST /api/session/cookies/get": "Get Cookie header for domain/path",
            "POST /api/session/token/set": "Set OAuth access token",
            "POST /api/session/oauth/authorize": "Build OAuth 2.0 authorize URL",
            "POST /api/session/oauth/exchange": "Prepare token exchange parameters",
            "POST /api/session/status": "Check session auth status",
            "POST /api/session/login/detect": "Detect login form in HTML",
            "POST /api/session/evict": "Evict expired cookies/tokens",
            "POST /api/session/login/mark": "Mark session as logged in",
            "POST /api/session/token/refresh": "Prepare token refresh parameters",
            "POST /api/workflow/create": "Create workflow orchestrator from goal",
            "POST /api/workflow/page": "Provide fetched page to orchestrator",
            "POST /api/workflow/report/click": "Report click result to orchestrator",
            "POST /api/workflow/report/fill": "Report fill form result",
            "POST /api/workflow/report/extract": "Report extract result",
            "POST /api/workflow/complete": "Mark step as manually completed",
            "POST /api/workflow/rollback": "Rollback a completed step",
            "POST /api/workflow/status": "Get workflow status summary",
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
    log_rss(&format!("fetch_parse start: {}", req.url));
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
    let fetch_ms = fetch_result.fetch_time_ms;

    let parse_start = std::time::Instant::now();
    let tree_json = aether_agent::parse_to_semantic_tree(
        &fetch_result.body,
        &req.goal,
        &fetch_result.final_url,
    );
    let parse_ms = parse_start.elapsed().as_millis() as u64;

    // Fas C.13: Inline XHR-URLs i svaret (om de finns i HTML:en)
    let xhr_urls = aether_agent::detect_xhr_urls(&fetch_result.body);

    let total_time_ms = total_start.elapsed().as_millis() as u64;

    log_rss(&format!(
        "fetch_parse after parse: {} (body={}KB)",
        req.url,
        fetch_result.body_size_bytes / 1024
    ));

    // Fas C.12: Per-steg timing i svaret
    let mut result_value = serde_json::json!({
        "fetch": fetch_result,
        "tree": serde_json::from_str::<serde_json::Value>(&tree_json).unwrap_or_default(),
        "total_time_ms": total_time_ms,
        "timing": {
            "fetch_ms": fetch_ms,
            "parse_ms": parse_ms,
            "total_ms": total_time_ms,
        }
    });

    if let Ok(xhr_value) = serde_json::from_str::<serde_json::Value>(&xhr_urls) {
        if let Some(obj) = result_value.as_object_mut() {
            obj.insert("xhr_calls".to_string(), xhr_value);
        }
    }

    (
        StatusCode::OK,
        serde_json::to_string(&result_value).unwrap_or_default(),
    )
}

// ─── Markdown endpoints ──────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MarkdownRequest {
    html: String,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    url: String,
}

#[derive(Deserialize)]
struct FetchMarkdownRequest {
    url: String,
    #[serde(default)]
    goal: String,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
}

async fn parse_markdown(Json(req): Json<MarkdownRequest>) -> impl IntoResponse {
    let start = std::time::Instant::now();
    let md = aether_agent::html_to_markdown(&req.html, &req.goal, &req.url);
    let ms = start.elapsed().as_millis() as u64;

    let result = serde_json::json!({
        "markdown": md,
        "markdown_length": md.len(),
        "parse_time_ms": ms,
    });

    (
        StatusCode::OK,
        serde_json::to_string(&result).unwrap_or_default(),
    )
}

async fn fetch_markdown(Json(req): Json<FetchMarkdownRequest>) -> impl IntoResponse {
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
    let fetch_ms = fetch_result.fetch_time_ms;

    let parse_start = std::time::Instant::now();
    let md = aether_agent::html_to_markdown(&fetch_result.body, &req.goal, &fetch_result.final_url);
    let parse_ms = parse_start.elapsed().as_millis() as u64;
    let total_ms = total_start.elapsed().as_millis() as u64;

    let result = serde_json::json!({
        "markdown": md,
        "markdown_length": md.len(),
        "url": fetch_result.final_url,
        "status_code": fetch_result.status_code,
        "html_size": fetch_result.body_size_bytes,
        "timing": {
            "fetch_ms": fetch_ms,
            "parse_ms": parse_ms,
            "total_ms": total_ms,
        }
    });

    (
        StatusCode::OK,
        serde_json::to_string(&result).unwrap_or_default(),
    )
}

// ─── Fas 7: Fetch click handler ─────────────────────────────────────────────

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

// ─── Fas 10: XHR Interception ────────────────────────────────────────────────

async fn detect_xhr(Json(req): Json<DetectXhrRequest>) -> impl IntoResponse {
    let result = aether_agent::detect_xhr_urls(&req.html);
    (StatusCode::OK, result)
}

// ─── Fas 17: DDG Search handlers ────────────────────────────────────────────

#[derive(Deserialize)]
struct SearchRequest {
    query: String,
    html: String,
    #[serde(default)]
    top_n: Option<usize>,
    #[serde(default)]
    goal: Option<String>,
}

#[derive(Deserialize)]
struct FetchSearchRequest {
    query: String,
    #[serde(default)]
    top_n: Option<usize>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    config: Option<aether_agent::types::FetchConfig>,
    /// Deep fetch: hämta och parsa varje resultat-sida (default: true)
    #[serde(default)]
    deep: Option<bool>,
    /// Max semantiska noder per resultat-sida (default: 5)
    #[serde(default)]
    max_nodes_per_result: Option<usize>,
}

async fn search_handler(Json(req): Json<SearchRequest>) -> impl IntoResponse {
    let top_n = req.top_n.unwrap_or(3);
    let goal = req.goal.as_deref().unwrap_or("");
    let result = aether_agent::search_from_html(&req.query, &req.html, top_n, goal);
    (StatusCode::OK, result)
}

async fn fetch_search_handler(Json(req): Json<FetchSearchRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();
    let ddg_url = aether_agent::build_search_url(&req.query);

    if let Err(e) = aether_agent::fetch::validate_url(&ddg_url) {
        return (
            StatusCode::BAD_REQUEST,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        );
    }

    let html = match aether_agent::fetch::fetch_page(&ddg_url, &config).await {
        Ok(r) => r.body,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
            );
        }
    };

    let top_n = req.top_n.unwrap_or(3);
    let goal = req.goal.as_deref().unwrap_or("");
    let search_json = aether_agent::search_from_html(&req.query, &html, top_n, goal);

    let deep = req.deep.unwrap_or(true);
    let max_nodes_per_result = req.max_nodes_per_result.unwrap_or(5);

    if !deep {
        return (StatusCode::OK, search_json);
    }

    // Deep fetch: parsa DDG-resultat, fetcha varje URL parallellt
    let mut search_result: aether_agent::search::SearchResult =
        match serde_json::from_str(&search_json) {
            Ok(r) => r,
            Err(_) => return (StatusCode::OK, search_json),
        };

    if !search_result.results.is_empty() {
        let deep_start = std::time::Instant::now();
        let effective_goal = if goal.is_empty() {
            format!("hitta svar på: {}", req.query)
        } else {
            goal.to_string()
        };

        // Begränsa parallella deep fetches till max 3 för att förhindra OOM
        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(3));
        let mut join_set = tokio::task::JoinSet::new();
        for (idx, entry) in search_result.results.iter().enumerate() {
            let url = entry.url.clone();
            let g = effective_goal.clone();
            let mnpr = max_nodes_per_result;
            let sem = semaphore.clone();
            join_set.spawn(async move {
                let _permit = sem.acquire().await;
                let fetch_start = std::time::Instant::now();
                let cfg = aether_agent::types::FetchConfig::default();
                match tokio::time::timeout(
                    std::time::Duration::from_secs(8),
                    aether_agent::fetch::fetch_page(&url, &cfg),
                )
                .await
                {
                    Ok(Ok(result)) => {
                        // Hämta fler noder med låg tröskel — re-ranking i
                        // deep_extract_page_nodes prioriterar text-innehåll
                        let fetch_limit = (mnpr * 8).max(30);
                        let stream_json = aether_agent::stream_parse_adaptive(
                            &result.body,
                            &g,
                            &url,
                            fetch_limit as u32,
                            0.0,
                            fetch_limit as u32,
                        );
                        let nodes = deep_extract_page_nodes(&stream_json, mnpr);
                        (idx, nodes, fetch_start.elapsed().as_millis() as u64)
                    }
                    _ => (idx, Vec::new(), 0),
                }
            });
        }

        while let Some(Ok((idx, nodes, elapsed))) = join_set.join_next().await {
            if idx < search_result.results.len() && !nodes.is_empty() {
                search_result.results[idx].page_content = Some(nodes);
                search_result.results[idx].fetch_ms = Some(elapsed);
            }
        }

        search_result.deep = Some(true);
        search_result.deep_fetch_ms = Some(deep_start.elapsed().as_millis() as u64);

        // Berika snippets från page_content
        for entry in &mut search_result.results {
            if let Some(ref nodes) = entry.page_content {
                let best_text: Vec<&str> = nodes
                    .iter()
                    .filter(|n| n.label.len() > 30)
                    .take(2)
                    .map(|n| n.label.as_str())
                    .collect();
                if !best_text.is_empty()
                    && (entry.snippet.len() < 30 || entry.snippet.contains("www."))
                {
                    entry.snippet = best_text.join(" | ");
                }
            }
        }

        // Försök hitta direktsvar med berikade snippets
        if search_result.direct_answer.is_none() {
            let (direct_answer, confidence) =
                aether_agent::search::detect_direct_answer(&search_result.results)
                    .map(|(a, c)| (Some(a), c))
                    .unwrap_or((None, 0.0));
            if direct_answer.is_some() {
                search_result.direct_answer = direct_answer;
                search_result.direct_answer_confidence = confidence;
            }
        }
    }

    let final_json = serde_json::to_string(&search_result)
        .unwrap_or_else(|e| format!(r#"{{"error": "serialize: {e}"}}"#));
    (StatusCode::OK, final_json)
}

/// Extrahera PageNode:er från stream_parse JSON
fn deep_extract_page_nodes(json: &str, max: usize) -> Vec<aether_agent::search::PageNode> {
    let parsed: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let nodes = match parsed.get("nodes").and_then(|n| n.as_array()) {
        Some(arr) => arr,
        None => return Vec::new(),
    };

    // Re-rank för informationsextraktion (inte interaktion).
    // Text/heading med längre innehåll är mest värdefulla vid sökning.
    let mut scored: Vec<aether_agent::search::PageNode> = nodes
        .iter()
        .filter_map(|n| {
            let label = n.get("label")?.as_str()?.to_string();
            if label.len() < 10 {
                return None;
            }
            let role = n.get("role").and_then(|r| r.as_str()).unwrap_or("text");
            let base_rel = n.get("relevance").and_then(|r| r.as_f64()).unwrap_or(0.0) as f32;

            // Sök-optimerad re-ranking:
            // - text/paragraph med faktiskt innehåll → boost
            // - heading → boost (rubriker sammanfattar)
            // - link/button/cta/nav → nedprioritera (nav-brus)
            let info_boost = match role {
                "text" | "paragraph" => 0.35,
                "heading" => 0.25,
                "price" | "product_card" => 0.20,
                "generic" => 0.10,
                "link" => -0.20,
                "button" | "cta" => -0.30,
                "navigation" => -0.40,
                _ => 0.0,
            };
            // Längre text = mer informationsrikt
            let len_boost = (label.len() as f32 / 500.0).min(0.15);
            let final_rel = (base_rel + info_boost + len_boost).clamp(0.0, 1.0);

            Some(aether_agent::search::PageNode {
                role: role.to_string(),
                label,
                relevance: final_rel,
            })
        })
        .collect();

    scored.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));
    // Dedup: skippa noder vars label är substring av en redan vald nod
    let mut selected: Vec<aether_agent::search::PageNode> = Vec::with_capacity(max);
    for node in scored {
        if selected.len() >= max {
            break;
        }
        let dominated = selected
            .iter()
            .any(|s| s.label.contains(&node.label) || node.label.contains(&s.label));
        if !dominated {
            selected.push(node);
        }
    }
    selected
}

// ─── Fas 16: Stream Parse handlers ──────────────────────────────────────────

async fn stream_parse_handler(Json(req): Json<StreamParseRequest>) -> impl IntoResponse {
    let result = aether_agent::stream_parse_adaptive(
        &req.html,
        &req.goal,
        &req.url,
        req.top_n as u32,
        req.min_relevance,
        req.max_nodes as u32,
    );
    (StatusCode::OK, result)
}

async fn fetch_stream_parse(Json(req): Json<FetchStreamParseRequest>) -> impl IntoResponse {
    let config = req.config.unwrap_or_default();

    if let Err(e) = aether_agent::fetch::validate_url(&req.url) {
        return (
            StatusCode::BAD_REQUEST,
            serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
        );
    }

    let fetch_result = match aether_agent::fetch::fetch_page(&req.url, &config).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
            );
        }
    };

    let result = aether_agent::stream_parse_adaptive(
        &fetch_result.body,
        &req.goal,
        &fetch_result.final_url,
        req.top_n as u32,
        req.min_relevance,
        req.max_nodes as u32,
    );
    (StatusCode::OK, result)
}

async fn directive_handler(Json(req): Json<DirectiveRequest>) -> impl IntoResponse {
    let config_json = serde_json::json!({
        "top_n": req.top_n,
        "min_relevance": req.min_relevance,
        "max_nodes": req.max_nodes,
    })
    .to_string();
    let directives_json =
        serde_json::to_string(&req.directives).unwrap_or_else(|_| "[]".to_string());

    let result = aether_agent::stream_parse_with_directives(
        &req.html,
        &req.goal,
        &req.url,
        &config_json,
        &directives_json,
    );
    (StatusCode::OK, result)
}

// ─── Fas 12: TieredBackend ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct TieredScreenshotRequest {
    html: String,
    url: String,
    goal: String,
    #[serde(default = "default_width", alias = "viewport_width")]
    width: u32,
    #[serde(default = "default_height", alias = "viewport_height")]
    height: u32,
    #[serde(default = "default_fast_render")]
    fast_render: bool,
    #[serde(default)]
    xhr_captures_json: String,
}

fn default_width() -> u32 {
    1280
}
fn default_height() -> u32 {
    800
}
fn default_fast_render() -> bool {
    true
}

async fn tiered_screenshot_handler(Json(req): Json<TieredScreenshotRequest>) -> impl IntoResponse {
    let render_future = tokio::task::spawn_blocking(move || {
        aether_agent::tiered_screenshot(
            &req.html,
            &req.url,
            &req.goal,
            req.width,
            req.height,
            req.fast_render,
            &req.xhr_captures_json,
        )
    });

    match tokio::time::timeout(
        std::time::Duration::from_secs(RENDER_TIMEOUT_SECS),
        render_future,
    )
    .await
    {
        Ok(Ok(result)) => (StatusCode::OK, result),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(r#"{{"error":"Render task kraschade: {e}"}}"#),
        ),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                r#"{{"error":"Render timeout: screenshot tog längre än {RENDER_TIMEOUT_SECS}s"}}"#
            ),
        ),
    }
}

async fn tier_stats_handler() -> impl IntoResponse {
    let result = aether_agent::tier_stats();
    (StatusCode::OK, result)
}

// ─── QuickJS+Blitz: render_with_js ───────────────────────────────────────────────

#[derive(Deserialize)]
struct RenderWithJsRequest {
    html: String,
    js_code: String,
    #[serde(default = "default_base_url")]
    base_url: String,
    #[serde(default = "default_width")]
    width: u32,
    #[serde(default = "default_height")]
    height: u32,
}

fn default_base_url() -> String {
    "https://localhost".to_string()
}

#[cfg(all(feature = "js-eval", feature = "blitz"))]
async fn render_with_js_handler(Json(req): Json<RenderWithJsRequest>) -> impl IntoResponse {
    let render_future = tokio::task::spawn_blocking(move || {
        aether_agent::render_with_js(
            &req.html,
            &req.js_code,
            &req.base_url,
            req.width,
            req.height,
        )
    });

    match tokio::time::timeout(
        std::time::Duration::from_secs(RENDER_TIMEOUT_SECS),
        render_future,
    )
    .await
    {
        Ok(Ok(result)) => (StatusCode::OK, result),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(r#"{{"error":"Render task kraschade: {e}"}}"#),
        ),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            format!(
                r#"{{"error":"Render timeout: render_with_js tog längre än {RENDER_TIMEOUT_SECS}s"}}"#
            ),
        ),
    }
}

/// Fetch URL → inline CSS → Blitz full render (med bilder + CSS)
#[derive(Deserialize)]
struct FetchRenderRequest {
    url: String,
    #[serde(default)]
    js_code: String,
    #[serde(default = "default_width")]
    width: u32,
    #[serde(default = "default_height")]
    height: u32,
    /// true = skippa extern resursladdning i Blitz (~50ms), false = ladda allt (~2-5s cap)
    /// Om ej angivet: auto-detektera baserat på HTML-storlek (>200KB → fast)
    fast_render: Option<bool>,
}

#[cfg(all(feature = "fetch", feature = "blitz"))]
async fn fetch_render_handler(Json(req): Json<FetchRenderRequest>) -> impl IntoResponse {
    use aether_agent::fetch;

    // Steg 1: Fetcha HTML
    let config = aether_agent::types::FetchConfig::default();
    let fetch_result = match fetch::fetch_page(&req.url, &config).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!(r#"{{"error":"Fetch failed: {e}"}}"#),
            )
        }
    };

    let html = &fetch_result.body;
    let final_url = &fetch_result.final_url;

    if html.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                r#"{{"error":"Empty response from {}", "status": {}}}"#,
                final_url, fetch_result.status_code
            ),
        );
    }

    // Steg 2: Inline extern CSS med detaljerad felrapportering
    let css_result = fetch::inline_external_css_detailed(html, final_url).await;
    let html_with_css = &css_result.html;

    // Steg 3: Rendera med TieredBackend (Blitz → CDP-fallback) — med timeout-skydd
    // Auto-detektera fast_render baserat på original HTML-storlek (före CSS-inlining)
    // Tröskeln 500KB matchar riktigt tunga sidor (github 569KB) som kraschar Blitz
    const FAST_RENDER_THRESHOLD: usize = 500 * 1024;
    let fast_render = req
        .fast_render
        .unwrap_or(html.len() > FAST_RENDER_THRESHOLD);
    let html_for_render = html_with_css.clone();
    let url_for_render = final_url.clone();
    let status_code = fetch_result.status_code;
    let js_code = req.js_code.clone();
    let render_width = req.width;
    let render_height = req.height;
    let css_found = css_result.css_found;
    let css_loaded_count = css_result.css_loaded;
    let css_failed_count = css_result.css_failed;
    let css_bytes = css_result.css_bytes_added;
    let css_details_clone = css_result.css_details.clone();

    let render_future = tokio::task::spawn_blocking(move || {
        #[cfg(feature = "js-eval")]
        {
            if js_code.is_empty() {
                match aether_agent::screenshot_with_tier(
                    &html_for_render,
                    &url_for_render,
                    render_width,
                    render_height,
                    fast_render,
                ) {
                    Ok((png_bytes, tier_used)) => {
                        use base64::Engine;
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                        serde_json::json!({
                            "png_base64": b64,
                            "png_size_bytes": png_bytes.len(),
                            "url": url_for_render,
                            "status_code": status_code,
                            "html_length": html_for_render.len(),
                            "css_inlined": css_loaded_count > 0,
                            "css_found": css_found,
                            "css_loaded": css_loaded_count,
                            "css_failed": css_failed_count,
                            "css_bytes_added": css_bytes,
                            "css_details": css_details_clone,
                            "tier_used": format!("{:?}", tier_used),
                        })
                        .to_string()
                    }
                    Err(e) => serde_json::json!({
                        "error": format!("Render failed: {e}"),
                        "url": url_for_render,
                        "css_found": css_found,
                        "css_loaded": css_loaded_count,
                        "css_failed": css_failed_count,
                        "css_details": css_details_clone,
                    })
                    .to_string(),
                }
            } else {
                aether_agent::render_with_js_full(
                    &html_for_render,
                    &js_code,
                    &url_for_render,
                    render_width,
                    render_height,
                )
            }
        }
        #[cfg(not(feature = "js-eval"))]
        {
            match aether_agent::screenshot_with_tier(
                &html_for_render,
                &url_for_render,
                render_width,
                render_height,
                fast_render,
            ) {
                Ok((png_bytes, tier_used)) => {
                    use base64::Engine;
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                    serde_json::json!({
                        "png_base64": b64,
                        "png_size_bytes": png_bytes.len(),
                        "url": url_for_render,
                        "status_code": status_code,
                        "html_length": html_for_render.len(),
                        "css_inlined": css_loaded_count > 0,
                        "css_found": css_found,
                        "css_loaded": css_loaded_count,
                        "css_failed": css_failed_count,
                        "css_bytes_added": css_bytes,
                        "css_details": css_details_clone,
                        "tier_used": format!("{:?}", tier_used),
                    })
                    .to_string()
                }
                Err(e) => serde_json::json!({
                    "error": format!("Render failed: {e}"),
                    "url": url_for_render,
                    "css_found": css_found,
                    "css_loaded": css_loaded_count,
                    "css_failed": css_failed_count,
                    "css_details": css_details_clone,
                })
                .to_string(),
            }
        }
    });

    match tokio::time::timeout(
        std::time::Duration::from_secs(RENDER_TIMEOUT_SECS),
        render_future,
    )
    .await
    {
        Ok(Ok(result)) => (StatusCode::OK, result),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            serde_json::json!({
                "error": format!("Render task kraschade: {e}"),
                "url": final_url,
                "css_found": css_result.css_found,
                "css_loaded": css_result.css_loaded,
                "css_failed": css_result.css_failed,
            })
            .to_string(),
        ),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            serde_json::json!({
                "error": format!("Render timeout: rendering tog längre än {RENDER_TIMEOUT_SECS}s"),
                "url": final_url,
                "css_found": css_result.css_found,
                "css_loaded": css_result.css_loaded,
                "css_failed": css_result.css_failed,
            })
            .to_string(),
        ),
    }
}

// ─── Fas 11: Vision ─────────────────────────────────────────────────────────

#[cfg(feature = "vision")]
async fn parse_screenshot_handler(Json(req): Json<ParseScreenshotRequest>) -> impl IntoResponse {
    let png_bytes = match B64.decode(&req.png_base64) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!(r#"{{"error":"Invalid PNG base64: {e}"}}"#),
            )
        }
    };
    let model_bytes = match B64.decode(&req.model_base64) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!(r#"{{"error":"Invalid model base64: {e}"}}"#),
            )
        }
    };
    let result = aether_agent::parse_screenshot(&png_bytes, &model_bytes, &req.goal);
    (StatusCode::OK, result)
}

/// Screenshot-analys med serverns förladdade modell (kräver AETHER_MODEL_URL/PATH)
#[cfg(feature = "vision")]
async fn parse_screenshot_server_model(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<ParseScreenshotServerModelRequest>,
) -> impl IntoResponse {
    let png_bytes = match B64.decode(&req.png_base64) {
        Ok(b) => b,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                format!(r#"{{"error":"Invalid PNG base64: {e}"}}"#),
            )
        }
    };

    let mut model_guard = state.vision_model.lock().unwrap_or_else(|e| e.into_inner());
    let session = match model_guard.as_mut() {
        Some(s) => s,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                r#"{"error":"Ingen vision-modell laddad. Sätt AETHER_MODEL_URL eller AETHER_MODEL_PATH."}"#
                    .to_string(),
            )
        }
    };
    let result = aether_agent::parse_screenshot_with_model(&png_bytes, session, &req.goal);
    (StatusCode::OK, result)
}

// [RTEN-ROLLBACK-ID:server-conversion] Gamla: is_onnx_format(), convert_onnx_to_rten(),
// ensure_rten_format(), load_vision_model() med rten::Model, load_vision_model_bytes()
// Se git-historik för fullständig implementering

/// Hämta modell-bytes från URL eller fil (ONNX-format direkt — ingen konvertering behövs)
#[cfg(feature = "vision")]
async fn load_vision_model_bytes() -> Option<Vec<u8>> {
    // Prioritet: AETHER_MODEL_URL > AETHER_MODEL_PATH
    if let Ok(url) = std::env::var("AETHER_MODEL_URL") {
        println!("Laddar vision-modell från URL: {url}");
        match reqwest::get(&url).await {
            Ok(resp) => {
                if resp.status().is_success() {
                    match resp.bytes().await {
                        Ok(bytes) => {
                            println!(
                                "Vision-modell nedladdad: {:.1} MB",
                                bytes.len() as f64 / (1024.0 * 1024.0)
                            );
                            return Some(bytes.to_vec());
                        }
                        Err(e) => eprintln!("Kunde inte läsa modell-bytes: {e}"),
                    }
                } else {
                    eprintln!("Modell-URL returnerade {}", resp.status());
                }
            }
            Err(e) => eprintln!("Kunde inte hämta modell från URL: {e}"),
        }
    }

    if let Ok(path) = std::env::var("AETHER_MODEL_PATH") {
        println!("Laddar vision-modell från fil: {path}");
        match std::fs::read(&path) {
            Ok(bytes) => {
                println!(
                    "Vision-modell laddad: {:.1} MB",
                    bytes.len() as f64 / (1024.0 * 1024.0)
                );
                return Some(bytes);
            }
            Err(e) => eprintln!("Kunde inte läsa modell-fil: {e}"),
        }
    }

    println!("Ingen vision-modell konfigurerad (AETHER_MODEL_URL/AETHER_MODEL_PATH ej satta)");
    None
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
            "description": "Parse HTML with automatic JS evaluation in sandboxed QuickJS engine.",
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
                    "snapshots_json": {"type": "string", "description": "JSON array of snapshot objects: [{\"url\": \"...\", \"node_count\": 5, \"warning_count\": 0, \"key_elements\": [\"button:Buy\"]}]. Only url is required."},
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
            "description": "Publish a semantic delta to the collaboration store. Pass the FULL output from diff_trees as delta_json.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "store_json": {"type": "string", "description": "Collab store JSON (from create_collab_store)"},
                    "agent_id": {"type": "string", "description": "Publishing agent's ID (from register_collab_agent)"},
                    "url": {"type": "string", "description": "URL the delta applies to"},
                    "delta_json": {"type": "string", "description": "FULL diff_trees output JSON containing: token_savings_ratio, total_nodes_before, total_nodes_after, changes[]"},
                    "timestamp_ms": {"type": "integer", "description": "Current timestamp in ms (e.g. Date.now())"}
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
        },
        {
            "name": "detect_xhr_urls",
            "description": "Scan HTML for hidden XHR/fetch/AJAX network calls in inline scripts and event handlers. Discovers fetch(), XMLHttpRequest.open(), $.ajax(), $.get(), $.post() patterns. Returns array of {url, method, headers} objects.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string to scan for XHR/fetch calls"}
                },
                "required": ["html"]
            }
        },
        {
            "name": "tiered_screenshot",
            "description": "Take a screenshot using the intelligent TieredBackend. Tier 1 (Blitz, pure Rust) renders static HTML/CSS in ~10-50ms without Chrome. If Blitz fails or JavaScript rendering is needed, Tier 2 (CDP/Chrome) takes over automatically. Returns: tier_used, latency_ms, size_bytes, and escalation_reason if tier switching occurred.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "url": {"type": "string", "description": "The page URL"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "width": {"type": "integer", "description": "Viewport width (default: 1280)", "default": 1280},
                    "height": {"type": "integer", "description": "Viewport height (default: 800)", "default": 800},
                    "fast_render": {"type": "boolean", "description": "Skip external resources (default: true)", "default": true},
                    "xhr_captures_json": {"type": "string", "description": "Optional XHR captures JSON for tier selection"}
                },
                "required": ["html", "url", "goal"]
            }
        },
        {
            "name": "tier_stats",
            "description": "Get rendering tier statistics: how many screenshots were rendered by Blitz (Tier 1) vs CDP/Chrome (Tier 2), escalation count, and average latency per tier.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "parse_screenshot",
            "description": "Analyze a screenshot using YOLOv8-nano object detection to find UI elements (buttons, inputs, links, icons, text, images, checkboxes, selects, headings). Returns detected elements with bounding boxes, confidence scores, and a semantic tree. Requires vision feature flag.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "png_base64": {"type": "string", "description": "PNG screenshot as base64 string"},
                    "model_base64": {"type": "string", "description": "YOLOv8-nano ONNX model as base64 string"},
                    "goal": {"type": "string", "description": "The agent's current goal"}
                },
                "required": ["png_base64", "model_base64", "goal"]
            }
        },
        {
            "name": "vision_parse",
            "description": "Analyze a screenshot using the server's pre-loaded YOLOv8-nano model. Detects UI elements (buttons, inputs, links, icons, text, images, checkboxes, selects, headings) and returns bounding boxes, confidence scores, and a semantic tree. No model upload needed — uses the model configured via AETHER_MODEL_URL/AETHER_MODEL_PATH.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "png_base64": {"type": "string", "description": "PNG screenshot as base64 string"},
                    "goal": {"type": "string", "description": "The agent's current goal for relevance scoring"}
                },
                "required": ["png_base64", "goal"]
            }
        },
        {
            "name": "fetch_vision",
            "description": "ALL-IN-ONE: Fetch a URL, render it to a pixel-perfect screenshot with Blitz (pure Rust browser engine), then analyze with YOLOv8 vision. Returns: 1) the actual screenshot as image/png, 2) an annotated image with color-coded bounding boxes around detected UI elements, 3) JSON with all detections (class, confidence, bbox) and semantic tree. USE THIS TOOL WHEN: you want to visually analyze any web page — just provide the URL and goal. No external browser needed.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "The URL to screenshot and analyze (e.g. https://www.hjo.se)"},
                    "goal": {"type": "string", "description": "The agent's current goal for relevance scoring"},
                    "width": {"type": "integer", "description": "Viewport width in pixels (default: 1280)", "default": 1280},
                    "height": {"type": "integer", "description": "Viewport height in pixels (default: 800)", "default": 800}
                },
                "required": ["url", "goal"]
            }
        },
        {
            "name": "search",
            "description": "Build a DuckDuckGo search URL for a query. Returns the URL to fetch. For auto-fetch, use fetch_search instead.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Free-text search query"}
                },
                "required": ["query"]
            }
        },
        {
            "name": "fetch_search",
            "description": "Search the web via DuckDuckGo: fetches DDG HTML, parses results, and returns structured search results with title, URL, snippet, domain, confidence, and optional direct_answer. Use this when you don't know which URL to visit.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Free-text search query"},
                    "top_n": {"type": "integer", "description": "Number of results (1-10, default: 3)", "default": 3},
                    "goal": {"type": "string", "description": "Agent goal for relevance scoring (default: same as query)"}
                },
                "required": ["query"]
            }
        },
        {
            "name": "stream_parse",
            "description": "Goal-driven adaptive DOM streaming. Parses HTML and emits only the most relevant nodes for the given goal, with 90-99% token savings. Use instead of parse/parse_top when you want minimal output focused on what matters. Returns ranked nodes, token savings ratio, and chunk metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML to parse"},
                    "goal": {"type": "string", "description": "The agent's current goal for relevance ranking"},
                    "url": {"type": "string", "description": "Source URL (for context)", "default": ""},
                    "top_n": {"type": "integer", "description": "Max nodes per chunk (default: 10)", "default": 10},
                    "min_relevance": {"type": "number", "description": "Minimum relevance threshold 0.0-1.0 (default: 0.3)", "default": 0.3},
                    "max_nodes": {"type": "integer", "description": "Hard cap on total emitted nodes (default: 50)", "default": 50}
                },
                "required": ["html", "goal"]
            }
        },
        {
            "name": "fetch_stream_parse",
            "description": "ALL-IN-ONE: Fetch a URL and run goal-driven adaptive DOM streaming. Combines fetch + stream_parse in one call. Returns only the most relevant nodes for the given goal with 90-99% token savings. Use this instead of fetch_parse when you want minimal, goal-focused output.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch and parse"},
                    "goal": {"type": "string", "description": "The agent's current goal for relevance ranking"},
                    "top_n": {"type": "integer", "description": "Max nodes per chunk (default: 10)", "default": 10},
                    "min_relevance": {"type": "number", "description": "Minimum relevance 0.0-1.0 (default: 0.3)", "default": 0.3},
                    "max_nodes": {"type": "integer", "description": "Hard cap on total emitted nodes (default: 50)", "default": 50}
                },
                "required": ["url", "goal"]
            }
        },
        {
            "name": "stream_parse_directive",
            "description": "Goal-driven adaptive DOM streaming with LLM directives. Like stream_parse but accepts directives to control traversal: expand(node_id) to get children, next_branch to jump to next top-ranked unsent nodes, lower_threshold(value) to reduce min_relevance, stop to halt immediately. Use for interactive multi-step exploration.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML to parse"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "Source URL", "default": ""},
                    "directives_json": {"type": "string", "description": "JSON array of directives, e.g. [{\"action\":\"next_branch\"},{\"action\":\"expand\",\"node_id\":5}]"},
                    "config_json": {"type": "string", "description": "JSON config: {\"top_n\":10,\"min_relevance\":0.3,\"max_nodes\":50}"}
                },
                "required": ["html", "goal"]
            }
        },
        {
            "name": "render_with_js",
            "description": "Render HTML with JavaScript execution: evaluates JS code against the DOM (via QuickJS sandbox), then renders the modified DOM to a PNG screenshot (via Blitz). Returns: base64-encoded PNG, mutation count, JS eval stats, timing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "js_code": {"type": "string", "description": "JavaScript code to evaluate against the DOM"},
                    "base_url": {"type": "string", "description": "Base URL for resolving relative paths"},
                    "width": {"type": "integer", "description": "Viewport width (default: 1280)", "default": 1280},
                    "height": {"type": "integer", "description": "Viewport height (default: 800)", "default": 800}
                },
                "required": ["html", "js_code", "base_url", "width", "height"]
            }
        }
    ])
}

/// Avkoda base64-sträng till bytes
fn base64_decode(input: &str) -> Result<Vec<u8>, String> {
    B64.decode(input)
        .map_err(|e| format!("Base64 decode error: {e}"))
}

/// Rita bounding boxar med klasslabels på en screenshot och returnera som base64 PNG.
/// Gör bilden transparent och visuellt tydlig med färgkodade ramar.
#[cfg(feature = "vision")]
fn render_annotated_screenshot(png_bytes: &[u8], result_json: &str) -> Result<String, String> {
    use image::{ImageFormat, Rgba, RgbaImage};
    use std::io::Cursor;

    // Ladda originalbilden
    let img =
        image::load_from_memory(png_bytes).map_err(|e| format!("Kunde inte ladda bild: {e}"))?;
    let mut canvas: RgbaImage = img.to_rgba8();
    let (img_w, img_h) = (canvas.width(), canvas.height());

    // Parsa detektioner från JSON
    let parsed: serde_json::Value =
        serde_json::from_str(result_json).map_err(|e| format!("JSON-parse: {e}"))?;
    let detections = parsed["detections"].as_array();

    // Färgpalett per klass
    let class_color = |class: &str| -> Rgba<u8> {
        match class {
            "button" => Rgba([0, 200, 0, 255]),     // Grön
            "input" => Rgba([0, 150, 255, 255]),    // Blå
            "link" => Rgba([255, 165, 0, 255]),     // Orange
            "text" => Rgba([200, 200, 200, 180]),   // Grå
            "heading" => Rgba([255, 255, 0, 255]),  // Gul
            "image" => Rgba([180, 0, 255, 255]),    // Lila
            "icon" => Rgba([255, 100, 100, 255]),   // Röd-rosa
            "checkbox" => Rgba([0, 255, 200, 255]), // Turkos
            "select" => Rgba([255, 80, 180, 255]),  // Magenta
            _ => Rgba([255, 255, 255, 255]),        // Vit
        }
    };

    if let Some(dets) = detections {
        for det in dets {
            let class = det["class"].as_str().unwrap_or("unknown");
            let conf = det["confidence"].as_f64().unwrap_or(0.0);
            let bbox = &det["bbox"];
            let bx = bbox["x"].as_f64().unwrap_or(0.0);
            let by = bbox["y"].as_f64().unwrap_or(0.0);
            let bw = bbox["width"].as_f64().unwrap_or(0.0);
            let bh = bbox["height"].as_f64().unwrap_or(0.0);

            let color = class_color(class);

            // Rita bounding box (3px bred ram)
            let x1 = (bx.max(0.0) as u32).min(img_w.saturating_sub(1));
            let y1 = (by.max(0.0) as u32).min(img_h.saturating_sub(1));
            let x2 = ((bx + bw) as u32).min(img_w.saturating_sub(1));
            let y2 = ((by + bh) as u32).min(img_h.saturating_sub(1));

            // Horisontella linjer (topp + botten, 3px)
            for thickness in 0..3u32 {
                let yt = y1.saturating_add(thickness).min(img_h - 1);
                let yb = y2.saturating_sub(thickness).max(y1);
                for x in x1..=x2 {
                    canvas.put_pixel(x, yt, color);
                    canvas.put_pixel(x, yb, color);
                }
            }
            // Vertikala linjer (vänster + höger, 3px)
            for thickness in 0..3u32 {
                let xl = x1.saturating_add(thickness).min(img_w - 1);
                let xr = x2.saturating_sub(thickness).max(x1);
                for y in y1..=y2 {
                    canvas.put_pixel(xl, y, color);
                    canvas.put_pixel(xr, y, color);
                }
            }

            // Label-bakgrund (fylld rektangel ovanför bbox)
            let label = format!("{class} {conf:.0}%", conf = conf * 100.0);
            let label_w = (label.len() as u32 * 7).min(x2.saturating_sub(x1));
            let label_h = 14u32;
            let ly = y1.saturating_sub(label_h);
            for lx in x1..x1.saturating_add(label_w) {
                for ly_px in ly..y1 {
                    if lx < img_w && ly_px < img_h {
                        canvas.put_pixel(lx, ly_px, color);
                    }
                }
            }

            // Enkel textrendering (1-bit font, 5x7 pixlar per tecken)
            let char_w = 6u32;
            let text_y = ly + 3;
            for (ci, ch) in label.chars().enumerate() {
                let cx = x1 + 2 + ci as u32 * char_w;
                render_char_5x7(&mut canvas, cx, text_y, ch, Rgba([0, 0, 0, 255]));
            }
        }
    }

    // Koda tillbaka till PNG
    let mut buf = Cursor::new(Vec::new());
    canvas
        .write_to(&mut buf, ImageFormat::Png)
        .map_err(|e| format!("PNG encode: {e}"))?;
    Ok(B64.encode(buf.into_inner()))
}

/// Minimal 5x7 pixel-font för annotation-labels
#[cfg(feature = "vision")]
fn render_char_5x7(img: &mut image::RgbaImage, x: u32, y: u32, ch: char, color: image::Rgba<u8>) {
    // Enkel bitmap-font (5 bred x 7 hög)
    let bitmap: [u8; 7] = match ch {
        '0' => [0x0E, 0x11, 0x13, 0x15, 0x19, 0x11, 0x0E],
        '1' => [0x04, 0x0C, 0x04, 0x04, 0x04, 0x04, 0x0E],
        '2' => [0x0E, 0x11, 0x01, 0x06, 0x08, 0x10, 0x1F],
        '3' => [0x0E, 0x11, 0x01, 0x06, 0x01, 0x11, 0x0E],
        '4' => [0x02, 0x06, 0x0A, 0x12, 0x1F, 0x02, 0x02],
        '5' => [0x1F, 0x10, 0x1E, 0x01, 0x01, 0x11, 0x0E],
        '6' => [0x06, 0x08, 0x10, 0x1E, 0x11, 0x11, 0x0E],
        '7' => [0x1F, 0x01, 0x02, 0x04, 0x08, 0x08, 0x08],
        '8' => [0x0E, 0x11, 0x11, 0x0E, 0x11, 0x11, 0x0E],
        '9' => [0x0E, 0x11, 0x11, 0x0F, 0x01, 0x02, 0x0C],
        'a' | 'A' => [0x0E, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'b' | 'B' => [0x1E, 0x11, 0x11, 0x1E, 0x11, 0x11, 0x1E],
        'c' | 'C' => [0x0E, 0x11, 0x10, 0x10, 0x10, 0x11, 0x0E],
        'd' | 'D' => [0x1E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x1E],
        'e' | 'E' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x1F],
        'f' | 'F' => [0x1F, 0x10, 0x10, 0x1E, 0x10, 0x10, 0x10],
        'g' | 'G' => [0x0E, 0x11, 0x10, 0x17, 0x11, 0x11, 0x0F],
        'h' | 'H' => [0x11, 0x11, 0x11, 0x1F, 0x11, 0x11, 0x11],
        'i' | 'I' => [0x0E, 0x04, 0x04, 0x04, 0x04, 0x04, 0x0E],
        'j' | 'J' => [0x01, 0x01, 0x01, 0x01, 0x11, 0x11, 0x0E],
        'k' | 'K' => [0x11, 0x12, 0x14, 0x18, 0x14, 0x12, 0x11],
        'l' | 'L' => [0x10, 0x10, 0x10, 0x10, 0x10, 0x10, 0x1F],
        'm' | 'M' => [0x11, 0x1B, 0x15, 0x11, 0x11, 0x11, 0x11],
        'n' | 'N' => [0x11, 0x19, 0x15, 0x13, 0x11, 0x11, 0x11],
        'o' | 'O' => [0x0E, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'p' | 'P' => [0x1E, 0x11, 0x11, 0x1E, 0x10, 0x10, 0x10],
        'r' | 'R' => [0x1E, 0x11, 0x11, 0x1E, 0x14, 0x12, 0x11],
        's' | 'S' => [0x0E, 0x11, 0x10, 0x0E, 0x01, 0x11, 0x0E],
        't' | 'T' => [0x1F, 0x04, 0x04, 0x04, 0x04, 0x04, 0x04],
        'u' | 'U' => [0x11, 0x11, 0x11, 0x11, 0x11, 0x11, 0x0E],
        'x' | 'X' => [0x11, 0x11, 0x0A, 0x04, 0x0A, 0x11, 0x11],
        'y' | 'Y' => [0x11, 0x11, 0x0A, 0x04, 0x04, 0x04, 0x04],
        '%' => [0x18, 0x19, 0x02, 0x04, 0x08, 0x13, 0x03],
        ' ' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
        '.' => [0x00, 0x00, 0x00, 0x00, 0x00, 0x0C, 0x0C],
        _ => [0x0E, 0x11, 0x01, 0x06, 0x04, 0x00, 0x04], // ?
    };
    let (img_w, img_h) = (img.width(), img.height());
    for (row, &bits) in bitmap.iter().enumerate() {
        for col in 0..5u32 {
            if bits & (0x10 >> col) != 0 {
                let px = x + col;
                let py = y + row as u32;
                if px < img_w && py < img_h {
                    img.put_pixel(px, py, color);
                }
            }
        }
    }
}

/// Fallback-version utan vision feature
#[cfg(not(feature = "vision"))]
fn render_annotated_screenshot(_png_bytes: &[u8], _result_json: &str) -> Result<String, String> {
    Err("Vision feature inte aktiverad".to_string())
}

/// BUG-003 fix: Hämta HTML, analysera TierHint, rendera med rätt tier.
/// Returnerar (png_bytes, tier_used_label).
#[cfg(feature = "blitz")]
async fn render_url_tiered(
    url: &str,
    width: u32,
    height: u32,
    fast_render: bool,
) -> Result<(Vec<u8>, String), String> {
    let config = aether_agent::types::FetchConfig::default();
    let response = aether_agent::fetch::fetch_page(url, &config)
        .await
        .map_err(|e| format!("Kunde inte hämta {url}: {e}"))?;
    let base_url = url.to_string();

    let html = aether_agent::fetch::inline_external_css(&response.body, &base_url).await;

    let url_owned = url.to_string();
    let render_future = tokio::task::spawn_blocking(move || {
        let (png_bytes, tier) =
            aether_agent::screenshot_with_tier(&html, &url_owned, width, height, fast_render)?;
        let tier_label = format!("{:?}", tier);
        Ok((png_bytes, tier_label))
    });

    // Render-timeout: max 10s — förhindra att tunga sidor hänger servern
    match tokio::time::timeout(
        std::time::Duration::from_secs(RENDER_TIMEOUT_SECS),
        render_future,
    )
    .await
    {
        Ok(join_result) => join_result.map_err(|e| format!("Render task: {e}"))?,
        Err(_) => Err(format!(
            "Render timeout: rendering tog längre än {RENDER_TIMEOUT_SECS}s"
        )),
    }
}

#[cfg(not(feature = "blitz"))]
async fn render_url_tiered(
    _url: &str,
    _width: u32,
    _height: u32,
    _fast_render: bool,
) -> Result<(Vec<u8>, String), String> {
    Err("Blitz feature inte aktiverad".to_string())
}

/// REST-endpoint: POST /api/fetch-vision — hämta URL, ta screenshot, kör vision
///
/// BUG-003 fix: Använder nu screenshot_with_tier istället för ren Blitz-rendering.
/// HTML analyseras med determine_tier_hint_with_url → automatisk eskalering till CDP
/// för SPA/JS-tunga sidor.
#[cfg(feature = "vision")]
async fn fetch_vision_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    Json(req): Json<FetchVisionRequest>,
) -> impl IntoResponse {
    let width = req.width.unwrap_or(1280);
    let height = req.height.unwrap_or(800);
    // Default false: ladda CSS/bilder för visuell screenshot.
    // Sätt fast_render=true explicit om man bara vill ha snabb YOLO utan styling.
    let fast_render = req.fast_render.unwrap_or(false);

    // Hämta HTML och rendera med TieredBackend (analyserar TierHint automatiskt)
    let (png_bytes, tier_used) = match render_url_tiered(&req.url, width, height, fast_render).await
    {
        Ok(result) => result,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": e}).to_string(),
            );
        }
    };

    // Kör vision med förladdad ORT session
    let mut model_guard = state.vision_model.lock().unwrap_or_else(|e| e.into_inner());
    let session = match model_guard.as_mut() {
        Some(s) => s,
        None => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                serde_json::json!({"error": "Ingen vision-modell laddad"}).to_string(),
            );
        }
    };

    let result_json = aether_agent::parse_screenshot_with_model(&png_bytes, session, &req.goal);
    drop(model_guard);
    let annotated_b64 = render_annotated_screenshot(&png_bytes, &result_json).unwrap_or_default();
    let original_b64 = B64.encode(&png_bytes);

    let response = serde_json::json!({
        "tier_used": tier_used,
        "original_screenshot": original_b64,
        "annotated_screenshot": annotated_b64,
        "detections": serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default(),
        "url": req.url,
        "viewport": {"width": width, "height": height}
    });

    (StatusCode::OK, response.to_string())
}

#[derive(serde::Deserialize)]
struct FetchVisionRequest {
    url: String,
    goal: String,
    width: Option<u32>,
    height: Option<u32>,
    /// true (default): skippa externa resurser (~50ms). false: ladda allt (~2s cap).
    fast_render: Option<bool>,
}

/// Dispatcha MCP tools/call till rätt aether_agent-funktion
/// Returnerar content-array (kan innehålla text + image blocks)
async fn mcp_dispatch_tool(
    name: &str,
    args: &serde_json::Value,
    state: &AppState,
) -> Result<serde_json::Value, String> {
    // Hjälpfunktion: text-only content block
    let text_ok = |s: String| -> Result<serde_json::Value, String> {
        Ok(serde_json::json!([{"type": "text", "text": s}]))
    };

    match name {
        "parse" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            text_ok(aether_agent::parse_to_semantic_tree(html, goal, url))
        }
        "parse_top" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let top_n = args["top_n"].as_u64().unwrap_or(10) as u32;
            text_ok(aether_agent::parse_top_nodes(html, goal, url, top_n))
        }
        "fetch_parse" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            aether_agent::fetch::validate_url(url)?;
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(url, &config).await {
                Ok(r) => {
                    let tree = aether_agent::parse_to_semantic_tree(&r.body, goal, &r.final_url);
                    text_ok(tree)
                }
                Err(e) => Err(e),
            }
        }
        "find_and_click" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let target = args["target_label"].as_str().unwrap_or("");
            text_ok(aether_agent::find_and_click(html, goal, url, target))
        }
        "fill_form" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let fields_json = serde_json::to_string(&args["fields"]).unwrap_or_default();
            text_ok(aether_agent::fill_form(html, goal, url, &fields_json))
        }
        "extract_data" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let keys_json = serde_json::to_string(&args["keys"]).unwrap_or_default();
            text_ok(aether_agent::extract_data(html, goal, url, &keys_json))
        }
        "check_injection" => {
            let text = args["text"].as_str().unwrap_or("");
            text_ok(aether_agent::check_injection(text))
        }
        "compile_goal" => {
            let goal = args["goal"].as_str().unwrap_or("");
            text_ok(aether_agent::compile_goal(goal))
        }
        "classify_request" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let config = aether_agent::firewall::FirewallConfig::default();
            let verdict = aether_agent::firewall::classify_request(url, goal, &config);
            let s = serde_json::to_string(&verdict).map_err(|e| e.to_string())?;
            text_ok(s)
        }
        "diff_trees" => {
            let old = args["old_tree_json"].as_str().unwrap_or("");
            let new = args["new_tree_json"].as_str().unwrap_or("");
            text_ok(aether_agent::diff_semantic_trees(old, new))
        }
        "fetch_extract" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let keys_json = serde_json::to_string(&args["keys"]).unwrap_or_default();
            aether_agent::fetch::validate_url(url)?;
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(url, &config).await {
                Ok(r) => text_ok(aether_agent::extract_data(
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
                Ok(r) => text_ok(aether_agent::find_and_click(
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
            text_ok(aether_agent::parse_with_js(html, goal, url))
        }
        "build_causal_graph" => {
            let snapshots = args["snapshots_json"].as_str().unwrap_or("");
            let actions = args["actions_json"].as_str().unwrap_or("");
            text_ok(aether_agent::build_causal_graph(snapshots, actions))
        }
        "predict_action_outcome" => {
            let graph = args["graph_json"].as_str().unwrap_or("");
            let action = args["action"].as_str().unwrap_or("");
            text_ok(aether_agent::predict_action_outcome(graph, action))
        }
        "find_safest_path" => {
            let graph = args["graph_json"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            text_ok(aether_agent::find_safest_path(graph, goal))
        }
        "discover_webmcp" => {
            let html = args["html"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            text_ok(aether_agent::discover_webmcp(html, url))
        }
        "ground_semantic_tree" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let ann_json = serde_json::to_string(&args["annotations"]).unwrap_or_default();
            text_ok(aether_agent::ground_semantic_tree(
                html, goal, url, &ann_json,
            ))
        }
        "match_bbox_iou" => {
            let tree = args["tree_json"].as_str().unwrap_or("");
            let bbox_json = serde_json::to_string(&args["bbox"]).unwrap_or_default();
            text_ok(aether_agent::match_bbox_iou(tree, &bbox_json))
        }
        "create_collab_store" => text_ok(aether_agent::create_collab_store()),
        "register_collab_agent" => {
            let store = args["store_json"].as_str().unwrap_or("");
            let agent_id = args["agent_id"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let ts = args["timestamp_ms"].as_u64().unwrap_or(0);
            text_ok(aether_agent::register_collab_agent(
                store, agent_id, goal, ts,
            ))
        }
        "publish_collab_delta" => {
            let store = args["store_json"].as_str().unwrap_or("");
            let agent_id = args["agent_id"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let delta = args["delta_json"].as_str().unwrap_or("");
            let ts = args["timestamp_ms"].as_u64().unwrap_or(0);
            text_ok(aether_agent::publish_collab_delta(
                store, agent_id, url, delta, ts,
            ))
        }
        "fetch_collab_deltas" => {
            let store = args["store_json"].as_str().unwrap_or("");
            let agent_id = args["agent_id"].as_str().unwrap_or("");
            text_ok(aether_agent::fetch_collab_deltas(store, agent_id))
        }
        "detect_xhr_urls" => {
            let html = args["html"].as_str().unwrap_or("");
            text_ok(aether_agent::detect_xhr_urls(html))
        }
        "tiered_screenshot" => {
            let html = args["html"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let width = args["width"].as_u64().unwrap_or(1280) as u32;
            let height = args["height"].as_u64().unwrap_or(800) as u32;
            let fast_render = args["fast_render"].as_bool().unwrap_or(true);
            let xhr_json = args["xhr_captures_json"].as_str().unwrap_or("[]");
            text_ok(aether_agent::tiered_screenshot(
                html,
                url,
                goal,
                width,
                height,
                fast_render,
                xhr_json,
            ))
        }
        "tier_stats" => text_ok(aether_agent::tier_stats()),
        "search" => {
            let query = args["query"].as_str().unwrap_or("");
            let url = aether_agent::build_search_url(query);
            text_ok(serde_json::json!({
                "action": "fetch_required",
                "ddg_url": url,
                "query": query,
                "message": "Fetch the ddg_url and pass HTML to search endpoint, or use fetch_search for auto-fetch."
            }).to_string())
        }
        "fetch_search" => {
            let query = args["query"].as_str().unwrap_or("");
            let top_n = args.get("top_n").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
            let goal = args.get("goal").and_then(|v| v.as_str()).unwrap_or("");
            let ddg_url = aether_agent::build_search_url(query);
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(&ddg_url, &config).await {
                Ok(result) => text_ok(aether_agent::search_from_html(
                    query,
                    &result.body,
                    top_n,
                    goal,
                )),
                Err(e) => text_ok(format!(r#"{{"error": "DDG fetch failed: {e}"}}"#)),
            }
        }
        "parse_screenshot" => {
            #[cfg(feature = "vision")]
            {
                let png_b64 = args["png_base64"].as_str().unwrap_or("");
                let model_b64 = args["model_base64"].as_str().unwrap_or("");
                let goal = args["goal"].as_str().unwrap_or("");
                let png_bytes = base64_decode(png_b64)?;
                let model_bytes = base64_decode(model_b64)?;
                let result_json = aether_agent::parse_screenshot(&png_bytes, &model_bytes, goal);
                let annotated_b64 =
                    render_annotated_screenshot(&png_bytes, &result_json).unwrap_or_default();
                Ok(serde_json::json!([
                    {"type": "image", "data": png_b64, "mimeType": "image/png"},
                    {"type": "image", "data": annotated_b64, "mimeType": "image/png"},
                    {"type": "text", "text": result_json}
                ]))
            }
            #[cfg(not(feature = "vision"))]
            {
                Err("Vision feature inte aktiverad. Kompilera med --features vision".to_string())
            }
        }
        "vision_parse" => {
            #[cfg(feature = "vision")]
            {
                let png_b64 = args["png_base64"].as_str().unwrap_or("");
                let goal = args["goal"].as_str().unwrap_or("");
                let png_bytes = base64_decode(png_b64)?;
                let mut model_guard = state.vision_model.lock().unwrap_or_else(|e| e.into_inner());
                let session = model_guard
                    .as_mut()
                    .ok_or_else(|| "Ingen vision-modell laddad på servern. Sätt AETHER_MODEL_URL eller AETHER_MODEL_PATH.".to_string())?;
                let result_json =
                    aether_agent::parse_screenshot_with_model(&png_bytes, session, goal);
                drop(model_guard);
                let annotated_b64 =
                    render_annotated_screenshot(&png_bytes, &result_json).unwrap_or_default();
                Ok(serde_json::json!([
                    {"type": "image", "data": png_b64, "mimeType": "image/png"},
                    {"type": "image", "data": annotated_b64, "mimeType": "image/png"},
                    {"type": "text", "text": result_json}
                ]))
            }
            #[cfg(not(feature = "vision"))]
            {
                Err("Vision feature inte aktiverad. Kompilera med --features vision".to_string())
            }
        }
        "fetch_vision" => {
            #[cfg(feature = "vision")]
            {
                let url = args["url"].as_str().unwrap_or("");
                let goal = args["goal"].as_str().unwrap_or("");
                let width = args["width"].as_u64().unwrap_or(1280) as u32;
                let height = args["height"].as_u64().unwrap_or(800) as u32;

                // Validera URL
                aether_agent::fetch::validate_url(url)?;

                // BUG-003 fix: Rendera med TieredBackend (automatisk CDP-eskalering)
                let fast_render = args["fast_render"].as_bool().unwrap_or(true);
                let (png_bytes, tier_used) =
                    render_url_tiered(url, width, height, fast_render).await?;
                let png_b64 = B64.encode(&png_bytes);

                // Kör vision med förladdad ORT session
                let mut model_guard = state.vision_model.lock().unwrap_or_else(|e| e.into_inner());
                let session = model_guard
                    .as_mut()
                    .ok_or_else(|| "Ingen vision-modell laddad på servern.".to_string())?;

                let result_json =
                    aether_agent::parse_screenshot_with_model(&png_bytes, session, goal);
                drop(model_guard);
                let annotated_b64 =
                    render_annotated_screenshot(&png_bytes, &result_json).unwrap_or_default();

                // Lägg till tier_used i svaret (nu dynamiskt)
                let enriched_json = match serde_json::from_str::<serde_json::Value>(&result_json) {
                    Ok(mut v) => {
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert(
                                "tier_used".to_string(),
                                serde_json::Value::String(tier_used),
                            );
                        }
                        serde_json::to_string(&v).unwrap_or(result_json)
                    }
                    Err(_) => result_json,
                };

                Ok(serde_json::json!([
                    {"type": "image", "data": png_b64, "mimeType": "image/png"},
                    {"type": "image", "data": annotated_b64, "mimeType": "image/png"},
                    {"type": "text", "text": enriched_json}
                ]))
            }
            #[cfg(not(feature = "vision"))]
            {
                Err("Vision feature inte aktiverad. Kompilera med --features vision".to_string())
            }
        }
        "fetch_stream_parse" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let top_n = args["top_n"].as_u64().unwrap_or(10) as u32;
            let min_rel = args["min_relevance"].as_f64().unwrap_or(0.3) as f32;
            let max_nodes = args["max_nodes"].as_u64().unwrap_or(50) as u32;

            aether_agent::fetch::validate_url(url)?;
            let config = aether_agent::types::FetchConfig::default();
            let fetch_result = aether_agent::fetch::fetch_page(url, &config)
                .await
                .map_err(|e| format!("Fetch failed: {e}"))?;

            let result = aether_agent::stream_parse_adaptive(
                &fetch_result.body,
                goal,
                &fetch_result.final_url,
                top_n,
                min_rel,
                max_nodes,
            );
            text_ok(result)
        }
        "stream_parse" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let top_n = args["top_n"].as_u64().unwrap_or(10) as u32;
            let min_rel = args["min_relevance"].as_f64().unwrap_or(0.3) as f32;
            let max_nodes = args["max_nodes"].as_u64().unwrap_or(50) as u32;

            let result =
                aether_agent::stream_parse_adaptive(html, goal, url, top_n, min_rel, max_nodes);
            text_ok(result)
        }
        "stream_parse_directive" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let directives_json = args["directives_json"].as_str().unwrap_or("[]");
            let config_json = args["config_json"].as_str().unwrap_or("{}");

            let result = aether_agent::stream_parse_with_directives(
                html,
                goal,
                url,
                config_json,
                directives_json,
            );
            text_ok(result)
        }
        "render_with_js" => {
            let html = args["html"].as_str().unwrap_or("");
            let js_code = args["js_code"].as_str().unwrap_or("");
            let base_url = args["base_url"].as_str().unwrap_or("");
            let width = args["width"].as_u64().unwrap_or(1280) as u32;
            let height = args["height"].as_u64().unwrap_or(800) as u32;
            text_ok(aether_agent::render_with_js(
                html, js_code, base_url, width, height,
            ))
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
async fn mcp_post(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
    Json(msg): Json<serde_json::Value>,
) -> impl IntoResponse {
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
                axum::http::header::HeaderValue::from_static("application/json"),
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
            let call_start = std::time::Instant::now();
            let result = mcp_dispatch_tool(tool_name, arguments, &state).await;
            let call_ms = call_start.elapsed().as_millis();

            // Broadcast tool-anrop till SSE-dashboard
            let event = serde_json::json!({
                "jsonrpc": "2.0",
                "method": "notifications/tool_call",
                "params": {
                    "tool": tool_name,
                    "duration_ms": call_ms,
                    "success": result.is_ok(),
                    "timestamp": std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                }
            });
            let event_str = event.to_string();
            let _ = state.mcp_events.send(event_str.clone());
            // Spara i ring-buffer för polling-fallback
            if let Ok(mut log) = state.mcp_event_log.lock() {
                if log.len() >= 100 {
                    log.pop_front();
                }
                log.push_back(event_str);
            }

            match result {
                Ok(content_blocks) => jsonrpc_result(
                    id,
                    serde_json::json!({
                        "content": content_blocks
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

/// MCP Streamable HTTP GET handler — SSE stream eller browser-dashboard
///
/// Spec 2025-03-26: MCP-klienter öppnar GET /mcp för SSE-notifications.
/// Webbläsare (Accept: text/html) får en live-dashboard som visar events.
/// SSE-strömmen visar alla MCP tool-anrop i realtid via broadcast-kanal.
async fn mcp_get(
    axum::extract::State(state): axum::extract::State<AppState>,
    headers: HeaderMap,
) -> axum::response::Response {
    // Om webbläsare → returnera HTML-dashboard med EventSource
    let accept = headers
        .get("accept")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if accept.contains("text/html") {
        return axum::response::Html(MCP_DASHBOARD_HTML).into_response();
    }

    // MCP-klient / EventSource → SSE-ström med broadcast-events
    let session_id = headers
        .get("mcp-session-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(64);

    // Skicka initial notification
    let tx_init = tx.clone();
    tokio::spawn(async move {
        let server_info = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {
                "serverInfo": {
                    "name": "aether-agent",
                    "version": "0.3.0"
                },
                "session": session_id
            }
        });
        let _ = tx_init
            .send(Ok(Event::default()
                .event("message")
                .data(server_info.to_string())))
            .await;
    });

    // Prenumerera på broadcast-kanalen och vidarebefordra events till SSE
    let mut broadcast_rx = state.mcp_events.subscribe();
    tokio::spawn(async move {
        loop {
            match broadcast_rx.recv().await {
                Ok(event_data) => {
                    if tx
                        .send(Ok(Event::default().event("message").data(event_data)))
                        .await
                        .is_err()
                    {
                        break; // Klient stängde kopplingen
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    // Klienten hann inte med — skicka varning
                    let warning = serde_json::json!({
                        "jsonrpc": "2.0",
                        "method": "notifications/warning",
                        "params": {"message": format!("Missed {n} events (slow client)")}
                    });
                    let _ = tx
                        .send(Ok(Event::default()
                            .event("message")
                            .data(warning.to_string())))
                        .await;
                }
            }
        }
    });

    Sse::new(tokio_stream::wrappers::ReceiverStream::new(rx))
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Polling-fallback: GET /mcp/events?since=<timestamp_ms>
/// Returnerar senaste events som JSON-array. Cloudflare-tunnlar buffrar SSE,
/// så dashboarden pollar denna endpoint istället.
async fn mcp_events_poll(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> axum::response::Json<serde_json::Value> {
    let since: u64 = params
        .get("since")
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let events: Vec<serde_json::Value> = if let Ok(log) = state.mcp_event_log.lock() {
        log.iter()
            .filter_map(|e| serde_json::from_str::<serde_json::Value>(e).ok())
            .filter(|e| {
                e["params"]["timestamp"]
                    .as_u64()
                    .is_some_and(|ts| ts > since)
            })
            .collect()
    } else {
        vec![]
    };

    axum::response::Json(serde_json::json!({ "events": events }))
}

/// Live HTML-dashboard för MCP-endpoint — visar SSE-events i webbläsaren
const MCP_DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>AetherAgent MCP</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{background:#0a0a0f;color:#e0e0e8;font-family:'SF Mono',Monaco,'Cascadia Code',monospace;font-size:14px}
.header{background:linear-gradient(135deg,#1a1a2e,#16213e);padding:24px 32px;border-bottom:1px solid #2a2a4a}
.header h1{font-size:20px;color:#7c83ff;font-weight:600}
.header p{color:#888;font-size:12px;margin-top:4px}
.status{display:flex;align-items:center;gap:8px;margin-top:12px;font-size:13px}
.dot{width:8px;height:8px;border-radius:50%;background:#444;transition:background .3s}
.dot.connected{background:#4ade80;box-shadow:0 0 8px #4ade8066}
.dot.error{background:#f87171}
.panels{display:grid;grid-template-columns:1fr 1fr;gap:16px;padding:24px 32px;height:calc(100vh - 140px)}
.panel{background:#111118;border:1px solid #2a2a4a;border-radius:8px;display:flex;flex-direction:column;overflow:hidden}
.panel-title{padding:12px 16px;border-bottom:1px solid #2a2a4a;font-size:12px;color:#7c83ff;text-transform:uppercase;letter-spacing:1px}
.panel-body{flex:1;overflow-y:auto;padding:12px 16px}
.event{margin-bottom:12px;padding:8px 12px;background:#0d0d14;border-radius:6px;border-left:3px solid #7c83ff}
.event .time{color:#666;font-size:11px}
.event .type{color:#4ade80;font-size:12px;margin:2px 0}
.event pre{color:#ccc;font-size:12px;white-space:pre-wrap;word-break:break-all;margin-top:4px}
.tools{list-style:none}
.tools li{padding:6px 0;border-bottom:1px solid #1a1a2a;font-size:13px}
.tools li span.name{color:#7c83ff}
.tools li span.desc{color:#888;font-size:12px}
.info-row{display:flex;justify-content:space-between;padding:8px 0;border-bottom:1px solid #1a1a2a;font-size:13px}
.info-row .label{color:#888}
.info-row .value{color:#e0e0e8}
@media(max-width:768px){.panels{grid-template-columns:1fr}}
</style>
</head>
<body>
<div class="header">
  <h1>AetherAgent MCP Server</h1>
  <p>Model Context Protocol — Streamable HTTP</p>
  <div class="status">
    <div class="dot" id="statusDot"></div>
    <span id="statusText">Connecting...</span>
  </div>
</div>
<div class="panels">
  <div class="panel">
    <div class="panel-title">Live Events</div>
    <div class="panel-body" id="events"></div>
  </div>
  <div class="panel">
    <div class="panel-title">Server Info</div>
    <div class="panel-body" id="info">
      <div class="info-row"><span class="label">Endpoint</span><span class="value">POST /mcp</span></div>
      <div class="info-row"><span class="label">Protocol</span><span class="value">MCP 2025-03-26</span></div>
      <div class="info-row"><span class="label">Transport</span><span class="value">Streamable HTTP + SSE</span></div>
      <div class="info-row"><span class="label">Events received</span><span class="value" id="eventCount">0</span></div>
      <div style="margin-top:16px">
        <div class="panel-title" style="padding:0 0 8px 0;border:none">Available Tools</div>
        <ul class="tools" id="toolsList"><li style="color:#666">Loading tools...</li></ul>
      </div>
    </div>
  </div>
</div>
<script>
let count = 0;
const dot = document.getElementById('statusDot');
const statusText = document.getElementById('statusText');
const eventsDiv = document.getElementById('events');
const eventCount = document.getElementById('eventCount');

function addEvent(type, data) {
  count++;
  eventCount.textContent = count;
  const div = document.createElement('div');
  div.className = 'event';
  const time = new Date().toLocaleTimeString();
  let detail = '';
  if (type === 'notifications/tool_call' && data.params) {
    const p = data.params;
    const icon = p.success ? '&#10003;' : '&#10007;';
    const color = p.success ? '#4ade80' : '#f87171';
    detail = `<div class="type" style="color:${color}">${icon} ${p.tool} <span style="color:#888">${p.duration_ms}ms</span></div>`;
  } else {
    detail = `<div class="type">${type}</div><pre>${JSON.stringify(data, null, 2)}</pre>`;
  }
  div.innerHTML = `<div class="time">${time}</div>${detail}`;
  eventsDiv.prepend(div);
  if (eventsDiv.children.length > 100) eventsDiv.lastChild.remove();
}

// SSE med polling-fallback (Cloudflare-tunnlar buffrar SSE)
let sseGotMessage = false;
let pollTimer = null;
let lastSeenTs = 0;

// Försök SSE först
const sse = new EventSource('/mcp');
sse.onopen = () => {
  // onopen triggas av Cloudflare med 200 OK men data buffras —
  // vänta tills ett riktigt message kommer innan vi litar på SSE
  dot.className = 'dot connected';
  statusText.textContent = 'SSE connected — waiting for first event...';
};
sse.addEventListener('message', (e) => {
  if (!sseGotMessage) {
    sseGotMessage = true;
    // SSE funkar på riktigt — stäng av polling om den startade
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
    statusText.textContent = 'Connected — live SSE stream';
  }
  try {
    const data = JSON.parse(e.data);
    if (data.params?.timestamp) lastSeenTs = Math.max(lastSeenTs, data.params.timestamp);
    addEvent(data.method || 'event', data);
  } catch {
    addEvent('raw', e.data);
  }
});
sse.onerror = () => {
  if (!sseGotMessage) {
    dot.className = 'dot error';
    statusText.textContent = 'SSE failed — falling back to polling...';
    if (!pollTimer) startPolling();
  }
};

// Polling-fallback: hämta /mcp/events?since=<ts> var 2:a sekund
function startPolling() {
  dot.className = 'dot connected';
  statusText.textContent = 'Connected — polling mode (2s)';
  pollTimer = setInterval(async () => {
    try {
      const r = await fetch('/mcp/events?since=' + lastSeenTs);
      const data = await r.json();
      (data.events || []).forEach(ev => {
        if (ev.params?.timestamp) lastSeenTs = Math.max(lastSeenTs, ev.params.timestamp);
        addEvent(ev.method || 'event', ev);
      });
      dot.className = 'dot connected';
      statusText.textContent = 'Connected — polling mode (2s) | Events: ' + count;
    } catch {
      dot.className = 'dot error';
      statusText.textContent = 'Poll failed — retrying...';
    }
  }, 2000);
}

// Om inget SSE-message mottagits efter 5s → starta polling oavsett
setTimeout(() => { if (!sseGotMessage && !pollTimer) startPolling(); }, 5000);

// Hämta verktyg via MCP tools/list
fetch('/mcp', {
  method: 'POST',
  headers: {'Content-Type': 'application/json'},
  body: JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/list',params:{}})
}).then(r => r.json()).then(data => {
  const tools = data?.result?.tools || [];
  const list = document.getElementById('toolsList');
  if (tools.length === 0) { list.innerHTML = '<li style="color:#666">No tools found</li>'; return; }
  list.innerHTML = tools.map(t =>
    `<li><span class="name">${t.name}</span><br><span class="desc">${t.description || ''}</span></li>`
  ).join('');
}).catch(() => {});
</script>
</body>
</html>"##;

/// MCP Streamable HTTP DELETE handler — avsluta session
async fn mcp_delete() -> impl IntoResponse {
    (StatusCode::OK, "Session terminated")
}

// ─── Router ──────────────────────────────────────────────────────────────────

// ─── Fas 13: Session Management handlers ────────────────────────────────────

async fn session_create() -> impl IntoResponse {
    let result = aether_agent::create_session();
    (StatusCode::OK, result)
}

async fn session_add_cookies(Json(req): Json<SessionAddCookiesRequest>) -> impl IntoResponse {
    let cookies_json = serde_json::to_string(&req.cookies).unwrap_or_default();
    let result = aether_agent::session_add_cookies(&req.session_json, &req.domain, &cookies_json);
    (StatusCode::OK, result)
}

async fn session_get_cookies(Json(req): Json<SessionGetCookiesRequest>) -> impl IntoResponse {
    let result = aether_agent::session_get_cookies(&req.session_json, &req.domain, &req.path);
    (StatusCode::OK, result)
}

async fn session_set_token(Json(req): Json<SessionSetTokenRequest>) -> impl IntoResponse {
    let scopes_json = serde_json::to_string(&req.scopes).unwrap_or_default();
    let result = aether_agent::session_set_token(
        &req.session_json,
        &req.access_token,
        &req.refresh_token,
        req.expires_in_secs,
        &scopes_json,
    );
    (StatusCode::OK, result)
}

async fn session_oauth_authorize(Json(req): Json<SessionOAuthRequest>) -> impl IntoResponse {
    // Stöd både nested config-objekt och individuella fält
    let config_json = if let Some(config) = &req.config {
        serde_json::to_string(config).unwrap_or_default()
    } else if req.auth_url.is_some() || req.client_id.is_some() {
        serde_json::json!({
            "auth_url": req.auth_url.as_deref().unwrap_or(""),
            "client_id": req.client_id.as_deref().unwrap_or(""),
            "redirect_uri": req.redirect_uri.as_deref().unwrap_or(""),
            "scopes": req.scopes.as_deref().unwrap_or(&[]),
        })
        .to_string()
    } else {
        "{}".to_string()
    };
    let result = aether_agent::session_oauth_authorize(&req.session_json, &config_json);
    (StatusCode::OK, result)
}

async fn session_token_exchange(Json(req): Json<SessionTokenExchangeRequest>) -> impl IntoResponse {
    let config_json = serde_json::to_string(&req.config).unwrap_or_default();
    let result = aether_agent::session_prepare_token_exchange(
        &req.session_json,
        &config_json,
        &req.authorization_code,
    );
    (StatusCode::OK, result)
}

async fn session_status_handler(Json(req): Json<SessionStatusRequest>) -> impl IntoResponse {
    let result = aether_agent::session_status(&req.session_json);
    (StatusCode::OK, result)
}

async fn session_detect_login(Json(req): Json<DetectLoginFormRequest>) -> impl IntoResponse {
    let result = aether_agent::detect_login_form(&req.html, &req.goal, &req.url);
    (StatusCode::OK, result)
}

async fn session_evict(Json(req): Json<SessionStatusRequest>) -> impl IntoResponse {
    let result = aether_agent::session_evict_expired(&req.session_json);
    (StatusCode::OK, result)
}

async fn session_mark_login(Json(req): Json<SessionStatusRequest>) -> impl IntoResponse {
    let result = aether_agent::session_mark_logged_in(&req.session_json);
    (StatusCode::OK, result)
}

async fn session_token_refresh(Json(req): Json<SessionOAuthRequest>) -> impl IntoResponse {
    let config_json = if let Some(config) = &req.config {
        serde_json::to_string(config).unwrap_or_default()
    } else {
        "{}".to_string()
    };
    let result = aether_agent::session_prepare_refresh(&req.session_json, &config_json);
    (StatusCode::OK, result)
}

// ─── Fas 14: Workflow Orchestration handlers ────────────────────────────────

async fn workflow_create(Json(req): Json<CreateWorkflowRequest>) -> impl IntoResponse {
    let config_json = req
        .config
        .map(|c| serde_json::to_string(&c).unwrap_or_default())
        .unwrap_or_else(|| "{}".to_string());
    let result = aether_agent::create_workflow(&req.goal, &req.start_url, &config_json);
    (StatusCode::OK, result)
}

async fn workflow_page(Json(req): Json<WorkflowProvidePageRequest>) -> impl IntoResponse {
    let result = aether_agent::workflow_provide_page(&req.orchestrator_json, &req.html, &req.url);
    (StatusCode::OK, result)
}

async fn workflow_report_click(Json(req): Json<WorkflowReportClickRequest>) -> impl IntoResponse {
    let result =
        aether_agent::workflow_report_click(&req.orchestrator_json, &req.click_result_json);
    (StatusCode::OK, result)
}

async fn workflow_report_fill(Json(req): Json<WorkflowReportFillRequest>) -> impl IntoResponse {
    let result = aether_agent::workflow_report_fill(&req.orchestrator_json, &req.fill_result_json);
    (StatusCode::OK, result)
}

async fn workflow_report_extract(
    Json(req): Json<WorkflowReportExtractRequest>,
) -> impl IntoResponse {
    let result =
        aether_agent::workflow_report_extract(&req.orchestrator_json, &req.extract_result_json);
    (StatusCode::OK, result)
}

async fn workflow_complete(Json(req): Json<WorkflowStepRequest>) -> impl IntoResponse {
    let result = aether_agent::workflow_complete_step(&req.orchestrator_json, req.step_index);
    (StatusCode::OK, result)
}

async fn workflow_rollback(Json(req): Json<WorkflowStepRequest>) -> impl IntoResponse {
    let result = aether_agent::workflow_rollback_step(&req.orchestrator_json, req.step_index);
    (StatusCode::OK, result)
}

async fn workflow_status_handler(Json(req): Json<WorkflowStatusRequest>) -> impl IntoResponse {
    let result = aether_agent::workflow_status(&req.orchestrator_json);
    (StatusCode::OK, result)
}

fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        // Root & Health
        .route("/", get(root))
        .route("/api/endpoints", get(api_endpoints))
        .route("/health", get(health))
        .route("/api/memory-stats", get(memory_stats_handler))
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
        .route("/api/fetch/markdown", post(fetch_markdown))
        .route("/api/markdown", post(parse_markdown))
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
        // Fas 10: XHR Interception
        .route("/api/detect-xhr", post(detect_xhr))
        // Fas 17: DDG Search
        .route("/api/search", post(search_handler))
        .route("/api/fetch/search", post(fetch_search_handler))
        // Fas 16: Stream Parse
        .route("/api/stream-parse", post(stream_parse_handler))
        .route("/api/fetch/stream-parse", post(fetch_stream_parse))
        .route("/api/directive", post(directive_handler))
        // Fas 11: Vision (kräver --features vision)
        // 50 MB body limit — ONNX-modeller + screenshots i base64
        .route("/api/parse-screenshot", {
            #[cfg(feature = "vision")]
            {
                post(parse_screenshot_handler)
            }
            #[cfg(not(feature = "vision"))]
            {
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        r#"{"error":"Vision feature inte aktiverad"}"#,
                    )
                })
            }
        })
        .route("/api/vision/parse", {
            #[cfg(feature = "vision")]
            {
                post(parse_screenshot_server_model)
            }
            #[cfg(not(feature = "vision"))]
            {
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        r#"{"error":"Vision feature inte aktiverad"}"#,
                    )
                })
            }
        })
        .route("/api/fetch-vision", {
            #[cfg(feature = "vision")]
            {
                post(fetch_vision_handler)
            }
            #[cfg(not(feature = "vision"))]
            {
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        r#"{"error":"Vision feature inte aktiverad"}"#,
                    )
                })
            }
        })
        // Fas 12: TieredBackend
        .route("/api/tiered-screenshot", post(tiered_screenshot_handler))
        .route("/api/tier-stats", get(tier_stats_handler))
        // QuickJS+Blitz: render med JS
        .route("/api/render-with-js", {
            #[cfg(all(feature = "js-eval", feature = "blitz"))]
            {
                post(render_with_js_handler)
            }
            #[cfg(not(all(feature = "js-eval", feature = "blitz")))]
            {
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        r#"{"error":"js-eval + blitz features inte aktiverade"}"#,
                    )
                })
            }
        })
        // Fetch + CSS inline + full Blitz render (med bilder)
        .route("/api/fetch/render", {
            #[cfg(all(feature = "fetch", feature = "blitz"))]
            {
                post(fetch_render_handler)
            }
            #[cfg(not(all(feature = "fetch", feature = "blitz")))]
            {
                post(|| async {
                    (
                        StatusCode::NOT_IMPLEMENTED,
                        r#"{"error":"fetch + blitz features inte aktiverade"}"#,
                    )
                })
            }
        })
        // Fas 13: Session Management
        .route("/api/session/create", post(session_create))
        .route("/api/session/cookies/add", post(session_add_cookies))
        .route("/api/session/cookies/get", post(session_get_cookies))
        .route("/api/session/token/set", post(session_set_token))
        .route(
            "/api/session/oauth/authorize",
            post(session_oauth_authorize),
        )
        .route("/api/session/oauth/exchange", post(session_token_exchange))
        .route("/api/session/status", post(session_status_handler))
        .route("/api/session/login/detect", post(session_detect_login))
        .route("/api/session/evict", post(session_evict))
        .route("/api/session/login/mark", post(session_mark_login))
        .route("/api/session/token/refresh", post(session_token_refresh))
        // Fas 14: Workflow Orchestration
        .route("/api/workflow/create", post(workflow_create))
        .route("/api/workflow/page", post(workflow_page))
        .route("/api/workflow/report/click", post(workflow_report_click))
        .route("/api/workflow/report/fill", post(workflow_report_fill))
        .route(
            "/api/workflow/report/extract",
            post(workflow_report_extract),
        )
        .route("/api/workflow/complete", post(workflow_complete))
        .route("/api/workflow/rollback", post(workflow_rollback))
        .route("/api/workflow/status", post(workflow_status_handler))
        // MCP Streamable HTTP (spec 2025-03-26)
        .route("/mcp", post(mcp_post).get(mcp_get).delete(mcp_delete))
        .route("/mcp/events", get(mcp_events_poll))
        .with_state(state)
        .layer(cors)
        // 50 MB body limit — ONNX-modeller + screenshots kräver mer än Axums default (2 MB)
        .layer(DefaultBodyLimit::max(50 * 1024 * 1024))
}

/// Hämta aktuell minnesstatistik från /proc/self/statm och /proc/self/status
fn get_memory_stats() -> MemoryStats {
    let mut stats = MemoryStats::default();

    // RSS + VSZ från /proc/self/statm (snabbast)
    if let Ok(statm) = std::fs::read_to_string("/proc/self/statm") {
        let parts: Vec<&str> = statm.split_whitespace().collect();
        if parts.len() >= 2 {
            if let Ok(vsz_pages) = parts[0].parse::<u64>() {
                stats.vsz_mb = vsz_pages * 4 / 1024;
            }
            if let Ok(rss_pages) = parts[1].parse::<u64>() {
                stats.rss_mb = rss_pages * 4 / 1024;
            }
        }
    }

    // Detaljerad info från /proc/self/status
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for line in status.lines() {
            if let Some(val) = line.strip_prefix("VmHWM:") {
                stats.peak_rss_mb = parse_kb_value(val) / 1024;
            } else if let Some(val) = line.strip_prefix("VmSwap:") {
                stats.swap_mb = parse_kb_value(val) / 1024;
            } else if let Some(val) = line.strip_prefix("Threads:") {
                stats.threads = val.trim().parse().unwrap_or(0);
            }
        }
    }

    stats
}

/// Parsa "   12345 kB" → 12345
fn parse_kb_value(s: &str) -> u64 {
    s.trim().trim_end_matches("kB").trim().parse().unwrap_or(0)
}

/// Logga aktuell RSS (Resident Set Size) från /proc/self/statm
fn log_rss(label: &str) {
    let stats = get_memory_stats();
    eprintln!(
        "[MEM] {label}: {rss} MB RSS, {peak} MB peak, {vsz} MB VSZ, {swap} MB swap, {threads} threads",
        rss = stats.rss_mb,
        peak = stats.peak_rss_mb,
        vsz = stats.vsz_mb,
        swap = stats.swap_mb,
        threads = stats.threads,
    );
}

/// Minnesstatistik som JSON
#[derive(Default, Serialize)]
struct MemoryStats {
    rss_mb: u64,
    peak_rss_mb: u64,
    vsz_mb: u64,
    swap_mb: u64,
    threads: u64,
}

/// GET /api/memory-stats — returnera aktuell minnesanvändning
async fn memory_stats_handler() -> impl IntoResponse {
    let stats = get_memory_stats();
    (StatusCode::OK, Json(stats))
}

/// Starta bakgrundsloggning av minnesanvändning (var 30:e sekund)
fn spawn_memory_monitor() {
    tokio::spawn(async {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        loop {
            interval.tick().await;
            log_rss("periodic");
        }
    });
}

#[tokio::main]
async fn main() {
    eprintln!("=== AetherAgent Memory Startup Trace ===");
    log_rss("1. process start");

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    log_rss("2. before CDP hook registration");
    // Registrera CDP ready-callback FÖRE warmup (så att global backend uppdateras)
    aether_agent::register_cdp_ready_hook();
    log_rss("3. after CDP hook registration");

    // Starta Chrome i bakgrunden (Tier 2 CDP) — ej blockerande
    aether_agent::vision_backend::warmup_cdp_background();
    log_rss("4. after CDP warmup init");

    // Ladda vision-modell vid startup (om konfigurerad) — Model::load körs EN gång
    #[cfg(feature = "vision")]
    let vision_model = {
        log_rss("5a. before vision model bytes load");
        let vision_bytes = load_vision_model_bytes().await;
        if let Some(ref bytes) = vision_bytes {
            eprintln!(
                "[MEM] 5b. vision model bytes loaded: {:.1} MB on disk",
                bytes.len() as f64 / (1024.0 * 1024.0)
            );
            log_rss("5b. after vision model bytes in memory");
        }
        let model: Option<ort::session::Session> = match vision_bytes {
            Some(bytes) => {
                log_rss("5c. before ORT session create");
                let result = aether_agent::load_vision_model(&bytes);
                log_rss("5d. after ORT session create");
                drop(bytes); // Frigör modell-bytes efter laddning
                log_rss("5e. after dropping model bytes");
                match result {
                    Ok(session) => Some(session),
                    Err(e) => {
                        eprintln!("Kunde inte ladda ORT-modell: {e}");
                        None
                    }
                }
            }
            None => None,
        };
        log_rss("6. after complete vision setup");
        model
    };
    #[cfg(not(feature = "vision"))]
    let vision_model: Option<()> = None;

    log_rss("7. before router build");
    // Starta periodisk minnesmonitor (loggar var 30:e sek till stderr)
    spawn_memory_monitor();

    let (mcp_tx, _) = tokio::sync::broadcast::channel::<String>(128);
    let state = AppState {
        vision_model: Arc::new(std::sync::Mutex::new(vision_model)),
        mcp_events: Arc::new(mcp_tx),
        mcp_event_log: Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::with_capacity(100),
        )),
    };
    log_rss("8. after AppState creation");

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
    println!("  POST /api/fetch/markdown  – Fetch URL → Markdown");
    println!("  POST /api/markdown        – HTML → Markdown");
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
    println!("  POST /api/detect-xhr              – Scan HTML for XHR/fetch/AJAX endpoints");
    println!("  POST /api/search                 – DDG search (pre-fetched HTML)");
    println!("  POST /api/fetch/search           – DDG search (auto-fetch)");
    println!(
        "  POST /api/parse-screenshot       – Analyze screenshot with YOLOv8 vision (client model)"
    );
    println!("  POST /api/vision/parse           – Analyze screenshot with server-loaded model");
    println!("  POST /api/fetch-vision           – URL → screenshot → YOLO vision → images + JSON");
    println!("  POST /api/stream-parse           – Adaptive goal-driven DOM streaming");
    println!("  POST /api/fetch/stream-parse     – Fetch URL → adaptive stream parse");
    println!("  POST /api/directive              – Send directives (expand, stop, etc.)");
    println!("  POST /api/session/*              – Session management (cookies, OAuth 2.0)");
    println!("  POST /api/workflow/*             – Multi-page workflow orchestration");
    println!("  POST /mcp                        – MCP Streamable HTTP endpoint (JSON-RPC)");
    println!(
        "  GET  /mcp                        – MCP SSE stream (server-initiated notifications)"
    );
    println!("  DELETE /mcp                      – Terminate MCP session");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind");
    axum::serve(listener, build_router(state))
        .await
        .expect("Server error");
}
