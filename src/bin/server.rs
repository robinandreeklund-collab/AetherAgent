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
const RENDER_TIMEOUT_SECS: u64 = 45;

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
    /// Global request counter for live stats
    request_count: Arc<std::sync::atomic::AtomicU64>,
    /// Server start time
    started_at: std::time::Instant,
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
    /// Stage 3 reranker: "minilm", "colbert", "hybrid"
    #[serde(default)]
    reranker: Option<String>,
}

#[derive(Deserialize)]
struct ParseCrfrRequest {
    html: Option<String>,
    goal: String,
    url: String,
    #[serde(default = "default_crfr_top_n")]
    top_n: u32,
    #[serde(default)]
    run_js: bool,
    #[serde(default = "default_json")]
    output_format: String,
    /// Auto-follow high-relevance links: fetch targets, run CRFR, replace link nodes
    /// with extracted content. Default: true. Set false to disable.
    #[serde(default = "default_true")]
    follow_links: bool,
}

fn default_crfr_top_n() -> u32 {
    20
}
fn default_true() -> bool {
    true
}
fn default_json() -> String {
    "json".to_string()
}

#[derive(Deserialize)]
struct CrfrFeedbackRequest {
    url: String,
    goal: String,
    successful_node_ids: Vec<u32>,
}

#[derive(Deserialize)]
struct ParseCrfrMultiRequest {
    goals: Vec<String>,
    html: String,
    url: String,
    #[serde(default = "default_crfr_top_n")]
    top_n: u32,
}

#[derive(Deserialize)]
struct CrfrSaveRequest {
    url: String,
}

#[derive(Deserialize)]
struct CrfrLoadRequest {
    json: String,
}

#[derive(Deserialize)]
struct CrfrUpdateRequest {
    url: String,
    node_id: u32,
    new_label: String,
    new_role: String,
    #[serde(default)]
    new_value: String,
}

#[derive(Deserialize)]
struct CrfrTransferRequest {
    donor_url: String,
    recipient_url: String,
    #[serde(default = "default_min_similarity")]
    min_similarity: f32,
}

fn default_min_similarity() -> f32 {
    0.3
}

#[derive(Deserialize)]
struct ExtractMultiRequest {
    html: String,
    goal: String,
    url: String,
    keys: Vec<String>,
    #[serde(default = "default_max_per_key")]
    max_per_key: u32,
}

fn default_max_per_key() -> u32 {
    5
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
struct AdaptiveCrawlRequest {
    url: String,
    goal: String,
    #[serde(default = "default_max_crawl_pages")]
    max_pages: usize,
    #[serde(default = "default_max_crawl_depth")]
    max_depth: u32,
    #[serde(default = "default_top_k_links")]
    top_k_links: usize,
    #[serde(default = "default_min_gain")]
    min_gain: f32,
    #[serde(default = "default_crawl_top_n")]
    top_n_per_page: u32,
}

fn default_max_crawl_pages() -> usize {
    20
}
fn default_max_crawl_depth() -> u32 {
    3
}
fn default_top_k_links() -> usize {
    5
}
fn default_min_gain() -> f32 {
    0.02
}
fn default_crawl_top_n() -> u32 {
    15
}

#[derive(Deserialize)]
struct ExtractLinksRequest {
    #[serde(default)]
    html: Option<String>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default = "default_max_links")]
    max_links: u32,
    #[serde(default)]
    filter_navigation: bool,
    #[serde(default)]
    include_head_data: bool,
}

fn default_max_links() -> u32 {
    50
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

/// MCP Tool Explorer – Interactive frontend for testing all tools
async fn tool_explorer() -> impl IntoResponse {
    let html = include_str!("tool_explorer.html");
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
            "POST /api/parse-hybrid": "Parse with hybrid BM25+HDC+Neural pipeline. Set reranker=colbert for 2.8x faster + 41% better quality.",
            "POST /api/parse-crfr": "CRFR: Causal Resonance Field Retrieval — BM25+HDC wave propagation, 10-15x faster, learns over time. Params: top_n, run_js, output_format (json/markdown).",
            "POST /api/crfr-feedback": "Teach CRFR which nodes had the answer — improves future queries on same URL.",
            "POST /api/parse-crfr-multi": "Run multiple goal variants through CRFR and merge results.",
            "POST /api/crfr-save": "Save CRFR field (causal memory) for a URL to JSON.",
            "POST /api/crfr-load": "Load a previously saved CRFR field from JSON.",
            "POST /api/crfr-update": "Update a specific node in the CRFR field by ID.",
            "POST /api/crfr-transfer": "Transfer causal learning from one URL to another.",
            "POST /api/extract-multi": "Extract structured data for multiple keys with per-key limits.",
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
        serde_json::to_string(&body).unwrap_or_default(),
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

async fn parse_hybrid(Json(req): Json<ParseTopRequest>) -> impl IntoResponse {
    let top_n = if req.top_n > 0 { req.top_n } else { 20 };
    let html = req.html.clone();
    let goal = req.goal.clone();
    let url = req.url.clone();

    // Bygg träd med JS-eval (synk)
    let mut tree = tokio::task::spawn_blocking(move || {
        aether_agent::tools::build_tree_with_js(&html, &goal, &url)
    })
    .await
    .unwrap_or_default();

    // Resolve pending fetch-URLs (async — BUGG J)
    #[cfg(feature = "fetch")]
    aether_agent::tools::resolve_pending_fetches(&mut tree, &req.goal).await;

    // Kör hybrid scoring (synk)
    let goal2 = req.goal.clone();
    let url2 = req.url.clone();
    let html2 = req.html.clone();
    let reranker = req.reranker.clone();
    let result_json = tokio::task::spawn_blocking(move || {
        let goal_embedding = aether_agent::embedding::embed(&goal2);
        let config = aether_agent::tools::parse_hybrid_tool::build_config(reranker.as_deref());
        let pipeline = aether_agent::scoring::ScoringPipeline::run_cached(
            &html2,
            &tree.nodes,
            &goal2,
            goal_embedding.as_deref(),
            &config,
        );
        let score_map = aether_agent::scoring::pipeline::scores_to_map(&pipeline.scored_nodes);
        aether_agent::scoring::pipeline::apply_scores_to_tree(&mut tree.nodes, &score_map);

        let top_scored = aether_agent::scoring::ScoringPipeline::apply_top_n(
            pipeline.scored_nodes,
            Some(top_n as usize),
        );

        serde_json::json!({
            "url": url2,
            "goal": tree.goal,
            "title": tree.title,
            "top_nodes": top_scored.iter().map(|s| serde_json::json!({
                "id": s.id, "role": s.role, "label": s.label, "relevance": s.relevance,
            })).collect::<Vec<_>>(),
            "node_count": top_scored.len(),
            "total_nodes": aether_agent::tools::count_all_nodes(&tree.nodes),
            "injection_warnings": tree.injection_warnings.len(),
            "xhr_intercepted": tree.xhr_intercepted,
            "pipeline": {
                "method": match config.stage3_reranker {
                    aether_agent::scoring::Stage3Reranker::MiniLM => "hybrid_bm25_hdc_minilm",
                    #[cfg(feature = "colbert")]
                    aether_agent::scoring::Stage3Reranker::ColBert => "hybrid_bm25_hdc_colbert",
                    #[cfg(feature = "colbert")]
                    aether_agent::scoring::Stage3Reranker::Hybrid { .. } => "hybrid_bm25_hdc_colbert_minilm",
                },
                "bm25_candidates": pipeline.timings.tfidf_candidates,
                "hdc_survivors": pipeline.timings.hdc_survivors,
                "total_pipeline_us": pipeline.timings.total_us,
                "cache_hit": pipeline.timings.cache_hit,
            }
        })
        .to_string()
    })
    .await
    .unwrap_or_else(|_| "{}".to_string());

    (StatusCode::OK, result_json)
}

async fn parse_crfr_handler(Json(req): Json<ParseCrfrRequest>) -> impl IntoResponse {
    let follow_links = req.follow_links;
    let goal = req.goal;
    let url = req.url;
    let top_n = req.top_n;
    let run_js = req.run_js;
    let output_format = req.output_format;

    // Resolve HTML: use provided html, or fetch from url
    let html = if let Some(h) = req.html {
        if !h.is_empty() {
            h
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    #[allow(unused_mut)]
    let mut html = html;
    #[allow(unused_mut)]
    let mut fetch_ms: u64 = 0;
    #[cfg(feature = "fetch")]
    if html.is_empty() && !url.is_empty() {
        let fetch_start = std::time::Instant::now();
        match aether_agent::fetch::fetch_page(&url, &aether_agent::types::FetchConfig::default())
            .await
        {
            Ok(fetched) => {
                fetch_ms = fetch_start.elapsed().as_millis() as u64;
                html = fetched.body;
            }
            Err(e) => {
                let err = serde_json::json!({"error": format!("Fetch failed: {e}")});
                return (axum::http::StatusCode::BAD_GATEWAY, axum::Json(err)).into_response();
            }
        }
    }

    let raw_html_chars = html.len();

    if html.is_empty() {
        let err = serde_json::json!({"error": "Provide either 'html' or 'url'"});
        return (axum::http::StatusCode::BAD_REQUEST, axum::Json(err)).into_response();
    }

    // Inline externa scripts om run_js=true (React/Next.js bundles)
    #[cfg(feature = "fetch")]
    if run_js && !html.is_empty() {
        let js_inline = aether_agent::fetch::fetch_and_inline_external_scripts(&html, &url).await;
        if js_inline.scripts_loaded > 0 {
            html = js_inline.html;
        }
    }

    // Pass 1: Statisk CRFR (+ JS eval om run_js=true)
    let goal_clone = goal.clone();
    let url_clone = url.clone();
    let fmt_clone = output_format.clone();
    let mut tree = tokio::task::spawn_blocking({
        let g = goal.clone();
        let u = url.clone();
        let h = html.clone();
        move || aether_agent::build_tree_for_crfr(&h, &g, &u, run_js)
    })
    .await
    .unwrap_or_default();

    // Pass 2: Bot challenge re-fetch — om JS satte cookies och DOM är minimal
    #[cfg(feature = "fetch")]
    {
        let is_challenge = {
            let t = &tree;
            let c = &tree.js_cookies;
            aether_agent::is_likely_bot_challenge(t, c)
        };
        if is_challenge && !tree.js_cookies.is_empty() {
            eprintln!(
                "[COOKIE-BRIDGE] Bot challenge on {} — re-fetching with cookies",
                url
            );
            let rc = aether_agent::types::FetchConfig {
                cookies: tree.js_cookies.clone(),
                ..Default::default()
            };
            if let Ok(rr) = aether_agent::fetch::fetch_page(&url, &rc).await {
                eprintln!(
                    "[COOKIE-BRIDGE] Re-fetch: {} bytes, status {}",
                    rr.body.len(),
                    rr.status_code
                );
                let g = goal.clone();
                let u = rr.final_url.clone();
                let sc = rr.set_cookie_headers.clone();
                let b = rr.body;
                tree = tokio::task::spawn_blocking(move || {
                    aether_agent::build_tree_with_cookies(&b, &g, &u, &sc)
                })
                .await
                .unwrap_or_default();
            }
        }
    }

    // Pass 3: SPA-enrichment — om pending_fetch_urls finns, hämta och merge
    #[cfg(feature = "fetch")]
    if !tree.pending_fetch_urls.is_empty() {
        aether_agent::tools::resolve_pending_fetches(&mut tree, &goal).await;
    }

    // Pass 4: Kör CRFR på (potentiellt berikad) tree
    let result = tokio::task::spawn_blocking(move || {
        aether_agent::parse_crfr_from_tree_js(
            &tree,
            &goal_clone,
            &url_clone,
            top_n,
            &fmt_clone,
            run_js,
        )
    })
    .await
    .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());

    // Follow high-relevance links if requested
    #[cfg(feature = "fetch")]
    let result = if follow_links {
        follow_relevant_links_http(&result, &goal, top_n).await
    } else {
        result
    };
    #[cfg(not(feature = "fetch"))]
    let _ = follow_links;

    // Inject raw_html_chars and fetch_ms into the JSON response
    let result = if let Ok(mut json) = serde_json::from_str::<serde_json::Value>(&result) {
        json["raw_html_chars"] = serde_json::json!(raw_html_chars);
        json["fetch_ms"] = serde_json::json!(fetch_ms);
        json.to_string()
    } else {
        result
    };

    (StatusCode::OK, result).into_response()
}

/// Auto-follow relevant link nodes: fetch targets, run CRFR, REPLACE link nodes
/// with extracted content from the target page.
///
/// Smart link selection:
/// - Only follows links with amplitude > adaptive threshold (50% of top node, min 0.5)
/// - Skips nav-links (single words like "Product", "Home", "About")
/// - Skips ALL CAPS promotional links ("REGISTER FOR PYCON US!")
/// - Skips anchor-only links (#fragment), mailto:, javascript:
/// - Max 3 links followed per request
/// - URL dedup: same page never fetched twice (normalized + post-redirect)
///
/// Replacement: each followed link node is replaced by up to 2 content nodes
/// from the target page, marked with source="followed_link" + source_url.
#[cfg(feature = "fetch")]
async fn follow_relevant_links_http(original_result: &str, goal: &str, top_n: u32) -> String {
    const MAX_FOLLOW: usize = 3;
    const MIN_AMP_FLOOR: f64 = 0.5;

    let parsed: serde_json::Value = match serde_json::from_str(original_result) {
        Ok(v) => v,
        Err(_) => return original_result.to_string(),
    };

    let nodes = match parsed.get("nodes").and_then(|n| n.as_array()) {
        Some(n) => n.clone(),
        None => return original_result.to_string(),
    };

    let top_amp = nodes
        .iter()
        .filter_map(|n| n.get("relevance").and_then(|v| v.as_f64()))
        .fold(0.0_f64, f64::max);
    let min_amp = (top_amp * 0.5).max(MIN_AMP_FLOOR);

    let original_url = parsed
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    let normalize = |u: &str| -> String {
        let s = u.split('#').next().unwrap_or(u);
        let s = s.split('?').next().unwrap_or(s);
        s.trim_end_matches('/').to_lowercase()
    };

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    seen.insert(normalize(&original_url));

    // Identify which link nodes to follow + collect their indices
    let mut follow_targets: Vec<(usize, String, String)> = Vec::new(); // (index, label, url)

    for (idx, node) in nodes.iter().enumerate() {
        if follow_targets.len() >= MAX_FOLLOW {
            break;
        }
        let role = node.get("role").and_then(|v| v.as_str()).unwrap_or("");
        let action = node.get("action").and_then(|v| v.as_str()).unwrap_or("");
        let amp = node
            .get("relevance")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);
        let label = node.get("label").and_then(|v| v.as_str()).unwrap_or("");

        if role != "link" || action != "click" || amp <= min_amp {
            continue;
        }

        // Smart link filtering
        if !should_follow_link(label) {
            continue;
        }

        let href = match node.get("value").and_then(|v| v.as_str()) {
            Some(v) => v.split_whitespace().next().unwrap_or(v),
            None => continue,
        };

        if !href.starts_with("http") && !href.starts_with('/') {
            continue;
        }

        let resolved = if href.starts_with("http") {
            href.to_string()
        } else {
            let parts: Vec<&str> = original_url.splitn(4, '/').collect();
            if parts.len() >= 3 {
                format!(
                    "{}//{}/{}",
                    parts[0],
                    parts[2],
                    href.trim_start_matches('/')
                )
            } else {
                continue;
            }
        };

        if !seen.insert(normalize(&resolved)) {
            continue;
        }

        follow_targets.push((idx, label.to_string(), resolved));
    }

    if follow_targets.is_empty() {
        return original_result.to_string();
    }

    // Fetch + CRFR each link, collect replacement nodes per index
    let mut replacements: std::collections::HashMap<usize, Vec<serde_json::Value>> =
        std::collections::HashMap::new();
    let mut followed_urls: Vec<String> = Vec::new();

    for (idx, label, fetch_url) in &follow_targets {
        if aether_agent::fetch::validate_url(fetch_url).is_err() {
            continue;
        }
        let config = aether_agent::types::FetchConfig::default();
        let fetched = match aether_agent::fetch::fetch_page(fetch_url, &config).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Post-redirect dedup
        let final_norm = normalize(&fetched.final_url);
        let fetch_norm = normalize(fetch_url);
        if final_norm != fetch_norm && !seen.insert(final_norm) {
            continue;
        }

        let g = goal.to_string();
        let u = fetched.final_url.clone();
        let h = fetched.body;
        let n = top_n.min(5);
        let link_result = aether_agent::parse_crfr(&h, &g, &u, n, false, "json");

        if let Ok(link_parsed) = serde_json::from_str::<serde_json::Value>(&link_result) {
            if let Some(link_nodes) = link_parsed.get("nodes").and_then(|n| n.as_array()) {
                let mut content_nodes: Vec<serde_json::Value> = Vec::new();
                for node in link_nodes.iter().take(2) {
                    let nl = node.get("label").and_then(|v| v.as_str()).unwrap_or("");
                    let nr = node.get("role").and_then(|v| v.as_str()).unwrap_or("");
                    let ll = nl.to_lowercase();

                    // Filter boilerplate
                    if nl.len() < 10
                        || ll.starts_with("annons")
                        || ll.starts_with("advertisement")
                        || ll.contains("cookie")
                        || ll.contains("sponsored")
                    {
                        continue;
                    }
                    // Filter raw SSR data
                    if nr == "data"
                        && node
                            .get("name")
                            .and_then(|v| v.as_str())
                            .is_some_and(|n| n.contains('[') || n.contains("page.@"))
                    {
                        continue;
                    }

                    let mut n = node.clone();
                    if let Some(obj) = n.as_object_mut() {
                        obj.insert("source".to_string(), serde_json::json!("followed_link"));
                        obj.insert("source_url".to_string(), serde_json::json!(fetch_url));
                    }
                    content_nodes.push(n);
                }
                if !content_nodes.is_empty() {
                    replacements.insert(*idx, content_nodes);
                    followed_urls.push(format!("{}: {}", label, fetch_url));
                }
            }
        }
    }

    if replacements.is_empty() {
        return original_result.to_string();
    }

    // Build new nodes array: replace link nodes with their content
    let mut new_nodes: Vec<serde_json::Value> = Vec::new();
    for (idx, node) in nodes.iter().enumerate() {
        if let Some(replacement) = replacements.get(&idx) {
            // Replace link node with content from followed page
            for r in replacement {
                new_nodes.push(r.clone());
            }
        } else {
            new_nodes.push(node.clone());
        }
    }

    // Build final result
    let mut merged = parsed;
    if let Some(obj) = merged.as_object_mut() {
        obj.insert("nodes".to_string(), serde_json::json!(new_nodes));
        obj.insert("node_count".to_string(), serde_json::json!(new_nodes.len()));
        obj.insert(
            "followed_links".to_string(),
            serde_json::json!({
                "count": followed_urls.len(),
                "urls": followed_urls,
                "replaced_nodes": replacements.values().map(|v| v.len()).sum::<usize>(),
            }),
        );
    }

    serde_json::to_string(&merged).unwrap_or_else(|_| original_result.to_string())
}

/// Decide if a link label looks like content worth following.
/// Rejects navigation, promotional, and structural links.
#[cfg(feature = "fetch")]
fn should_follow_link(label: &str) -> bool {
    let trimmed = label.trim();

    // Too short = navigation ("Home", "About", "Product")
    if trimmed.len() < 8 {
        return false;
    }

    // ALL CAPS promotional ("REGISTER FOR PYCON US!", "BUY NOW")
    let upper_count = trimmed.chars().filter(|c| c.is_uppercase()).count();
    let alpha_count = trimmed.chars().filter(|c| c.is_alphabetic()).count();
    if alpha_count > 3 && upper_count as f32 / alpha_count as f32 > 0.7 {
        return false;
    }

    let lower = trimmed.to_lowercase();

    // Common nav/action patterns
    if lower.starts_with("log in")
        || lower.starts_with("sign up")
        || lower.starts_with("register")
        || lower.starts_with("download")
        || lower.starts_with("subscribe")
        || lower.starts_with("contact")
        || lower.starts_with("privacy")
        || lower.starts_with("terms of")
        || lower.starts_with("cookie")
        || lower.starts_with("tipsa ")
        || lower == "fler artiklar"
        || lower == "read more"
        || lower == "see more"
        || lower == "show more"
        || lower == "visa mer"
        || lower == "läs mer"
    {
        return false;
    }

    true
}

async fn crfr_feedback_handler(Json(req): Json<CrfrFeedbackRequest>) -> impl IntoResponse {
    let url = req.url;
    let goal = req.goal;
    let ids_json =
        serde_json::to_string(&req.successful_node_ids).unwrap_or_else(|_| "[]".to_string());

    let result =
        tokio::task::spawn_blocking(move || aether_agent::crfr_feedback(&url, &goal, &ids_json))
            .await
            .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());

    (StatusCode::OK, result)
}

async fn parse_crfr_multi_handler(Json(req): Json<ParseCrfrMultiRequest>) -> impl IntoResponse {
    let goals_json = serde_json::to_string(&req.goals).unwrap_or_else(|_| "[]".to_string());
    let url = req.url.clone();
    let top_n = req.top_n;

    // Om html är tom men url finns → fetcha
    let html = if req.html.is_empty() && !req.url.is_empty() {
        let config = aether_agent::types::FetchConfig::default();
        match aether_agent::fetch::fetch_page(&req.url, &config).await {
            Ok(r) => r.body,
            Err(e) => {
                return (
                    StatusCode::BAD_GATEWAY,
                    serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
                );
            }
        }
    } else {
        req.html
    };

    let result = tokio::task::spawn_blocking(move || {
        aether_agent::parse_crfr_multi(&html, &goals_json, &url, top_n)
    })
    .await
    .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());
    (StatusCode::OK, result)
}

async fn crfr_save_handler(Json(req): Json<CrfrSaveRequest>) -> impl IntoResponse {
    let url = req.url;
    let result = tokio::task::spawn_blocking(move || aether_agent::crfr_save_field(&url))
        .await
        .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());
    (StatusCode::OK, result)
}

async fn crfr_load_handler(Json(req): Json<CrfrLoadRequest>) -> impl IntoResponse {
    let json = req.json;
    let result = tokio::task::spawn_blocking(move || aether_agent::crfr_load_field(&json))
        .await
        .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());
    (StatusCode::OK, result)
}

async fn crfr_update_handler(Json(req): Json<CrfrUpdateRequest>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || {
        aether_agent::crfr_update_node(
            &req.url,
            req.node_id,
            &req.new_label,
            &req.new_role,
            &req.new_value,
        )
    })
    .await
    .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());
    (StatusCode::OK, result)
}

async fn crfr_transfer_handler(Json(req): Json<CrfrTransferRequest>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || {
        aether_agent::crfr_transfer(&req.donor_url, &req.recipient_url, req.min_similarity)
    })
    .await
    .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());
    (StatusCode::OK, result)
}

async fn extract_multi_handler(Json(req): Json<ExtractMultiRequest>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || {
        aether_agent::extract_data_multi(
            &req.html,
            &req.goal,
            &req.url,
            &serde_json::to_string(&req.keys).unwrap_or_else(|_| "[]".to_string()),
            req.max_per_key,
        )
    })
    .await
    .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());
    (StatusCode::OK, result)
}

async fn parse_extract_handler(Json(req): Json<ParseTopRequest>) -> impl IntoResponse {
    let max_items = if req.top_n > 0 { req.top_n } else { 20 };
    let result = aether_agent::parse_extract(&req.html, &req.goal, &req.url, max_items);
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

    // Pre-fetcha API-URLs från inline scripts (SPA-stöd)
    let prefetch_start = std::time::Instant::now();
    let api_responses = aether_agent::prefetch_api_urls(
        &fetch_result.body,
        &fetch_result.final_url,
        10,   // max 10 API-anrop
        3000, // 3s timeout per anrop
    )
    .await;
    let prefetch_ms = prefetch_start.elapsed().as_millis() as u64;
    let prefetched_count = api_responses.len();

    // Adaptiv parse med pre-fetched API-data injicerad i JS-sandlådan
    let parse_start = std::time::Instant::now();
    let adaptive_json = if api_responses.is_empty() {
        aether_agent::parse_adaptive(&fetch_result.body, &req.goal, &fetch_result.final_url)
    } else {
        aether_agent::parse_adaptive_with_fetch(
            &fetch_result.body,
            &req.goal,
            &fetch_result.final_url,
            api_responses,
        )
    };
    let parse_ms = parse_start.elapsed().as_millis() as u64;

    // Fas C.13: Inline XHR-URLs i svaret (om de finns i HTML:en)
    let xhr_urls = aether_agent::detect_xhr_urls(&fetch_result.body);

    let total_time_ms = total_start.elapsed().as_millis() as u64;

    log_rss(&format!(
        "fetch_parse after parse: {} (body={}KB)",
        req.url,
        fetch_result.body_size_bytes / 1024
    ));

    // Extrahera tree och tier_used från adaptive-resultatet
    let adaptive_value: serde_json::Value =
        serde_json::from_str(&adaptive_json).unwrap_or_default();

    let mut result_value = serde_json::json!({
        "fetch": fetch_result,
        "tree": adaptive_value.get("tree").cloned().unwrap_or_default(),
        "tier_used": adaptive_value.get("tier_used").cloned().unwrap_or_default(),
        "total_time_ms": total_time_ms,
        "prefetched_api_count": prefetched_count,
        "timing": {
            "fetch_ms": fetch_ms,
            "prefetch_api_ms": prefetch_ms,
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

// ─── Extract endpoint (compact, ranked, deduped) ───────────────────────────

async fn fetch_extract_smart(Json(req): Json<FetchParseRequest>) -> impl IntoResponse {
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
            )
        }
    };

    let result =
        aether_agent::parse_extract(&fetch_result.body, &req.goal, &fetch_result.final_url, 20);
    (StatusCode::OK, result)
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
                        // CRFR deep-parse: använd resonansfält istf stream_parse
                        let top_n = (mnpr * 4).max(20) as u32;
                        let crfr_json =
                            aether_agent::parse_crfr(&result.body, &g, &url, top_n, false, "json");
                        let nodes = deep_extract_crfr_nodes(&crfr_json, mnpr);
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

/// Extract page nodes from CRFR or stream_parse JSON (same nodes array format)
fn deep_extract_crfr_nodes(json: &str, max: usize) -> Vec<aether_agent::search::PageNode> {
    deep_extract_page_nodes(json, max)
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

// ─── WebSocket: Universal API Gateway (/ws/api) ─────────────────────────────

/// Universell WebSocket-gateway som multiplexar alla API-anrop med realtids-progress.
async fn ws_api_handler(ws: axum::extract::WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws_api)
}

async fn handle_ws_api(mut socket: axum::extract::ws::WebSocket) {
    use axum::extract::ws::Message;
    use tokio::sync::mpsc;

    // Kanal för att skicka svar tillbaka från spawnade tasks
    let (tx, mut rx) = mpsc::channel::<String>(64);

    loop {
        tokio::select! {
            // Skicka svar från spawnade tasks till klienten
            Some(msg) = rx.recv() => {
                if socket.send(Message::Text(msg)).await.is_err() {
                    return;
                }
            }
            // Ta emot meddelanden från klienten
            result = tokio::time::timeout(std::time::Duration::from_secs(300), socket.recv()) => {
                let msg_text = match result {
                    Ok(Some(Ok(Message::Text(text)))) => text,
                    Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return,
                    Ok(Some(Ok(Message::Ping(d)))) => {
                        let _ = socket.send(Message::Pong(d)).await;
                        continue;
                    }
                    Ok(Some(Ok(_))) => continue,
                    Ok(Some(Err(_))) | Err(_) => return,
                };

                let parsed: serde_json::Value = match serde_json::from_str(&msg_text) {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = socket.send(Message::Text(
                            serde_json::json!({"type":"error","message":format!("JSON parse error: {e}")}).to_string()
                        )).await;
                        continue;
                    }
                };

                let req_id = parsed.get("id").cloned().unwrap_or(serde_json::json!(null));
                let method = parsed.get("method").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let params = parsed.get("params").cloned().unwrap_or(serde_json::json!({}));

                if method.is_empty() {
                    let _ = socket.send(Message::Text(
                        serde_json::json!({"id": req_id, "type":"error","message":"Missing 'method' field"}).to_string()
                    )).await;
                    continue;
                }

                let tx2 = tx.clone();
                tokio::spawn(async move {
                    ws_api_dispatch(req_id, method, params, tx2).await;
                });
            }
        }
    }
}

/// Dispatchar ett WS API-anrop till rätt aether_agent-funktion
async fn ws_api_dispatch(
    id: serde_json::Value,
    method: String,
    params: serde_json::Value,
    tx: tokio::sync::mpsc::Sender<String>,
) {
    // Skicka progress
    let send = |msg: serde_json::Value| {
        let tx = tx.clone();
        async move {
            let _ = tx.send(msg.to_string()).await;
        }
    };

    let _ = send(serde_json::json!({
        "id": id, "type": "progress", "stage": "processing", "method": method
    }))
    .await;

    let result: Result<serde_json::Value, String> = match method.as_str() {
        "parse" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::parse_to_semantic_tree(&html, &goal, &url);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "parse_top" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            let top_n = params["top_n"].as_u64().unwrap_or(10) as u32;
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::parse_top_nodes(&html, &goal, &url, top_n);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "parse_hybrid" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            let top_n = params["top_n"].as_u64().unwrap_or(100) as u32;
            let reranker = params["reranker"].as_str().map(|s| s.to_string());
            tokio::task::spawn_blocking(move || {
                let config =
                    aether_agent::tools::parse_hybrid_tool::build_config(reranker.as_deref());
                let r =
                    aether_agent::parse_top_nodes_with_config(&html, &goal, &url, top_n, &config);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "find_and_click" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            let target = params["target_label"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::find_and_click(&html, &goal, &url, &target);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "fill_form" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            let fields_json = serde_json::to_string(&params["fields"]).unwrap_or_default();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::fill_form(&html, &goal, &url, &fields_json);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "extract_data" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            let keys_json = serde_json::to_string(&params["keys"]).unwrap_or_default();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::extract_data(&html, &goal, &url, &keys_json);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "check_injection" => {
            let text = params["text"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::check_injection(&text);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "compile_goal" => {
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::compile_goal(&goal);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "classify_request" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let config = aether_agent::firewall::FirewallConfig::default();
                let verdict = aether_agent::firewall::classify_request(&url, &goal, &config);
                serde_json::to_value(&verdict).unwrap_or(serde_json::json!(null))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "diff_trees" => {
            let old = params["old_tree_json"].as_str().unwrap_or("").to_string();
            let new = params["new_tree_json"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::diff_semantic_trees(&old, &new);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "parse_with_js" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::parse_with_js(&html, &goal, &url);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "stream_parse" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            let top_n = params["top_n"].as_u64().unwrap_or(10) as u32;
            let threshold = params["threshold"].as_f64().unwrap_or(0.0) as f32;
            let max_nodes = params["max_nodes"].as_u64().unwrap_or(50) as u32;
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::stream_parse_adaptive(
                    &html, &goal, &url, top_n, threshold, max_nodes,
                );
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "detect_xhr_urls" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::detect_xhr_urls(&html);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "build_causal_graph" => {
            let snapshots = params["snapshots_json"].as_str().unwrap_or("").to_string();
            let actions = params["actions_json"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::build_causal_graph(&snapshots, &actions);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "predict_action_outcome" => {
            let graph = params["graph_json"].as_str().unwrap_or("").to_string();
            let action = params["action"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::predict_action_outcome(&graph, &action);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "find_safest_path" => {
            let graph = params["graph_json"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::find_safest_path(&graph, &goal);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "discover_webmcp" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::discover_webmcp(&html, &url);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "tiered_screenshot" => {
            let html = params["html"].as_str().unwrap_or("").to_string();
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let width = params["width"].as_u64().unwrap_or(1280) as u32;
            let height = params["height"].as_u64().unwrap_or(800) as u32;
            let fast_render = params["fast_render"].as_bool().unwrap_or(false);
            let xhr_json = params["xhr_captures_json"]
                .as_str()
                .unwrap_or("[]")
                .to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::tiered_screenshot(
                    &html,
                    &url,
                    &goal,
                    width,
                    height,
                    fast_render,
                    &xhr_json,
                );
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "tier_stats" => tokio::task::spawn_blocking(|| {
            let r = aether_agent::tier_stats();
            serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
        })
        .await
        .map_err(|e| e.to_string()),
        // Fetch-operationer — flerstegsprogress
        "fetch_parse" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            ws_api_fetch_op(&id, &url, &goal, "parse", &params, &tx).await
        }
        "fetch_click" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            ws_api_fetch_op(&id, &url, &goal, "click", &params, &tx).await
        }
        "fetch_extract" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            ws_api_fetch_op(&id, &url, &goal, "extract", &params, &tx).await
        }
        "fetch_stream_parse" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            ws_api_fetch_op(&id, &url, &goal, "stream_parse", &params, &tx).await
        }
        // ─── Adaptive Crawl via WS ──────────────────────────
        "adaptive_crawl" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let _ = tx
                .send(
                    serde_json::json!({"id": id, "type":"progress","stage":"crawling","url":url})
                        .to_string(),
                )
                .await;
            let max_pages = params["max_pages"].as_u64().unwrap_or(20) as usize;
            let max_depth = params["max_depth"].as_u64().unwrap_or(3) as u32;
            let top_k = params["top_k_links"].as_u64().unwrap_or(5) as usize;
            let top_n = params["top_n_per_page"].as_u64().unwrap_or(10) as u32;
            let config = aether_agent::adaptive::AdaptiveConfig {
                max_pages,
                max_depth,
                top_k_links: top_k,
                top_n_per_page: top_n,
                ..Default::default()
            };
            let result = aether_agent::adaptive::adaptive_crawl(&url, &goal, config).await;
            serde_json::to_value(&result).map_err(|e| e.to_string())
        }
        // ─── Extract Links via WS ────────────────────────────
        "fetch_extract_links" | "extract_links" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or("").to_string();
            let _ = tx
                .send(
                    serde_json::json!({"id": id, "type":"progress","stage":"fetching","url":url})
                        .to_string(),
                )
                .await;
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(&url, &config).await {
                Ok(fetch_result) => {
                    let _ = tx.send(serde_json::json!({"id": id, "type":"progress","stage":"extracting","bytes_fetched":fetch_result.body.len()}).to_string()).await;
                    let max_links = params["max_links"].as_u64().unwrap_or(50) as u32;
                    let json = aether_agent::extract_links(
                        &fetch_result.body,
                        &goal,
                        &fetch_result.final_url,
                        max_links,
                    );
                    serde_json::from_str(&json).map_err(|e| e.to_string())
                }
                Err(e) => Err(format!("Fetch: {e}")),
            }
        }
        // ─── Search via WS ───────────────────────────────────
        "fetch_search" | "search" => {
            let query = params["query"].as_str().unwrap_or("").to_string();
            let goal = params["goal"].as_str().unwrap_or(&query).to_string();
            let _ = tx.send(serde_json::json!({"id": id, "type":"progress","stage":"searching","query":query}).to_string()).await;
            let top_n = params["top_n"].as_u64().unwrap_or(8) as usize;
            let ddg_url = aether_agent::build_search_url(&query);
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(&ddg_url, &config).await {
                Ok(ddg_html) => {
                    let _ = tx
                        .send(
                            serde_json::json!({"id": id, "type":"progress","stage":"parsing"})
                                .to_string(),
                        )
                        .await;
                    let search_json =
                        aether_agent::search_from_html(&query, &ddg_html.body, top_n, &goal);
                    serde_json::from_str(&search_json).map_err(|e| e.to_string())
                }
                Err(e) => Err(format!("Search fetch: {e}")),
            }
        }
        // ─── Render via WS ───────────────────────────────────
        "fetch_render" | "render" => {
            let url = params["url"].as_str().unwrap_or("").to_string();
            let _ = tx
                .send(
                    serde_json::json!({"id": id, "type":"progress","stage":"fetching","url":url})
                        .to_string(),
                )
                .await;
            let w = params["width"].as_u64().unwrap_or(1280) as u32;
            let h = params["height"].as_u64().unwrap_or(800) as u32;
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(&url, &config).await {
                Ok(fetch_result) => {
                    let _ = tx
                        .send(
                            serde_json::json!({"id": id, "type":"progress","stage":"rendering"})
                                .to_string(),
                        )
                        .await;
                    let body = fetch_result.body;
                    let final_url = fetch_result.final_url;
                    match tokio::task::spawn_blocking(move || {
                        aether_agent::screenshot_with_tier(&body, &final_url, w, h, false)
                    })
                    .await
                    {
                        Ok(Ok((png_bytes, tier))) => {
                            use base64::Engine;
                            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
                            Ok(serde_json::json!({
                                "png_base64": b64,
                                "png_size_bytes": png_bytes.len(),
                                "tier_used": format!("{:?}", tier),
                            }))
                        }
                        Ok(Err(e)) => Err(format!("Render failed: {e}")),
                        Err(e) => Err(format!("Render task: {e}")),
                    }
                }
                Err(e) => Err(format!("Fetch: {e}")),
            }
        }
        other => Err(format!("Unknown method: {other}")),
    };

    match result {
        Ok(data) => {
            let _ = send(serde_json::json!({"id": id, "type": "result", "data": data})).await;
        }
        Err(e) => {
            let _ = send(serde_json::json!({"id": id, "type": "error", "message": e})).await;
        }
    }
}

/// Hjälpfunktion för fetch_*-operationer med flerstegsprogress
async fn ws_api_fetch_op(
    id: &serde_json::Value,
    url: &str,
    goal: &str,
    op: &str,
    params: &serde_json::Value,
    tx: &tokio::sync::mpsc::Sender<String>,
) -> Result<serde_json::Value, String> {
    aether_agent::fetch::validate_url(url)?;

    // Progress: fetching
    let _ = tx
        .send(
            serde_json::json!({
                "id": id, "type": "progress", "stage": "fetching", "url": url
            })
            .to_string(),
        )
        .await;

    let config = aether_agent::types::FetchConfig::default();
    let fetch_result = aether_agent::fetch::fetch_page(url, &config)
        .await
        .map_err(|e| format!("Fetch failed: {e}"))?;

    let bytes_fetched = fetch_result.body.len();
    let final_url = fetch_result.final_url.clone();
    let body = fetch_result.body.clone();

    // Progress: parsing
    let _ = tx
        .send(
            serde_json::json!({
                "id": id, "type": "progress", "stage": "parsing", "bytes_fetched": bytes_fetched
            })
            .to_string(),
        )
        .await;

    let goal_owned = goal.to_string();
    let params_clone = params.clone();

    match op {
        "parse" => tokio::task::spawn_blocking(move || {
            let r = aether_agent::parse_to_semantic_tree(&body, &goal_owned, &final_url);
            serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
        })
        .await
        .map_err(|e| e.to_string()),
        "click" => {
            let target = params_clone["target_label"]
                .as_str()
                .unwrap_or("")
                .to_string();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::find_and_click(&body, &goal_owned, &final_url, &target);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "extract" => {
            let keys_json = serde_json::to_string(&params_clone["keys"]).unwrap_or_default();
            tokio::task::spawn_blocking(move || {
                let r = aether_agent::extract_data(&body, &goal_owned, &final_url, &keys_json);
                serde_json::from_str(&r).unwrap_or(serde_json::json!({"raw": r}))
            })
            .await
            .map_err(|e| e.to_string())
        }
        "stream_parse" => {
            let top_n = params_clone["top_n"].as_u64().unwrap_or(10) as u32;
            let threshold = params_clone["threshold"].as_f64().unwrap_or(0.0) as f32;
            let max_nodes = params_clone["max_nodes"].as_u64().unwrap_or(50) as u32;
            let tx_stream = tx.clone();
            let id_stream = id.clone();
            let result = tokio::task::spawn_blocking(move || {
                aether_agent::stream_parse_adaptive(
                    &body,
                    &goal_owned,
                    &final_url,
                    top_n,
                    threshold,
                    max_nodes,
                )
            })
            .await
            .map_err(|e| e.to_string())?;

            // Parse result and stream nodes one by one
            let parsed: serde_json::Value =
                serde_json::from_str(&result).unwrap_or(serde_json::json!({"nodes":[]}));
            let nodes = parsed["nodes"].as_array().cloned().unwrap_or_default();
            let total = parsed["total_dom_nodes"].as_u64().unwrap_or(0);
            let savings = parsed["token_savings_ratio"].as_f64().unwrap_or(0.0);

            // Send header
            let _ = tx_stream
                .send(
                    serde_json::json!({
                        "id": id_stream, "type": "stream_start",
                        "total_dom_nodes": total,
                        "total_nodes": nodes.len(),
                        "token_savings_ratio": savings,
                    })
                    .to_string(),
                )
                .await;

            // Stream each node individually
            for (i, node) in nodes.iter().enumerate() {
                let _ = tx_stream
                    .send(
                        serde_json::json!({
                            "id": id_stream, "type": "stream_node",
                            "index": i,
                            "node": node,
                        })
                        .to_string(),
                    )
                    .await;
                // Small delay for visual streaming effect (10ms per node)
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }

            // Send completion
            Ok(serde_json::json!({
                "type": "stream_complete",
                "total_nodes": nodes.len(),
                "total_dom_nodes": total,
                "token_savings_ratio": savings,
            }))
        }
        _ => Err(format!("Unknown fetch op: {op}")),
    }
}

// ─── WebSocket: MCP JSON-RPC (/ws/mcp) ──────────────────────────────────────

/// MCP JSON-RPC via WebSocket — initialize, tools/list, tools/call, ping
async fn ws_mcp_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_mcp(socket, state))
}

async fn handle_ws_mcp(mut socket: axum::extract::ws::WebSocket, state: AppState) {
    use axum::extract::ws::Message;

    loop {
        let msg_text =
            match tokio::time::timeout(std::time::Duration::from_secs(300), socket.recv()).await {
                Ok(Some(Ok(Message::Text(text)))) => text,
                Ok(Some(Ok(Message::Close(_)))) | Ok(None) => return,
                Ok(Some(Ok(Message::Ping(d)))) => {
                    let _ = socket.send(Message::Pong(d)).await;
                    continue;
                }
                Ok(Some(Ok(_))) => continue,
                Ok(Some(Err(_))) | Err(_) => return,
            };

        let msg: serde_json::Value = match serde_json::from_str(&msg_text) {
            Ok(v) => v,
            Err(e) => {
                let err = jsonrpc_error(
                    &serde_json::json!(null),
                    -32700,
                    &format!("Parse error: {e}"),
                );
                let _ = socket.send(Message::Text(err.to_string())).await;
                continue;
            }
        };

        let method = msg["method"].as_str().unwrap_or("");
        let id = &msg["id"];
        let params = &msg["params"];

        // Notification (inget id) — ignorera tyst
        if id.is_null() {
            continue;
        }

        let response = match method {
            "initialize" => jsonrpc_result(
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
            ),
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
                if let Ok(mut log) = state.mcp_event_log.lock() {
                    if log.len() >= 100 {
                        log.pop_front();
                    }
                    log.push_back(event_str);
                }

                match result {
                    Ok(content) => jsonrpc_result(
                        id,
                        serde_json::json!({"content": content, "isError": false}),
                    ),
                    Err(e) => jsonrpc_result(
                        id,
                        serde_json::json!({
                            "content": [{"type": "text", "text": e}],
                            "isError": true
                        }),
                    ),
                }
            }
            "ping" => jsonrpc_result(id, serde_json::json!({})),
            _ => jsonrpc_error(id, -32601, &format!("Method not found: {method}")),
        };

        if socket
            .send(Message::Text(response.to_string()))
            .await
            .is_err()
        {
            return;
        }
    }
}

// ─── WebSocket: Streaming Search (/ws/search) ───────────────────────────────

/// WebSocket-baserad streaming-sökning som skickar resultat ett-i-taget
async fn ws_search_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_search(socket, state))
}

async fn handle_ws_search(mut socket: axum::extract::ws::WebSocket, _state: AppState) {
    use axum::extract::ws::Message;

    // Vänta på sökförfrågan
    let req_text = match tokio::time::timeout(std::time::Duration::from_secs(30), socket.recv())
        .await
    {
        Ok(Some(Ok(Message::Text(text)))) => text,
        _ => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type":"error","message":"Timeout or invalid start message"}).to_string(),
                ))
                .await;
            return;
        }
    };

    let req: serde_json::Value = match serde_json::from_str(&req_text) {
        Ok(v) => v,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type":"error","message":format!("JSON parse error: {e}")})
                        .to_string(),
                ))
                .await;
            return;
        }
    };

    let query = req["query"].as_str().unwrap_or("");
    let goal = req["goal"].as_str().unwrap_or("");
    let top_n = req["top_n"].as_u64().unwrap_or(5) as usize;
    let deep = req["deep"].as_bool().unwrap_or(true);
    let max_nodes_per_result = req["max_nodes_per_result"].as_u64().unwrap_or(5) as usize;

    if query.is_empty() {
        let _ = socket
            .send(Message::Text(
                serde_json::json!({"type":"error","message":"Missing 'query' field"}).to_string(),
            ))
            .await;
        return;
    }

    let search_start = std::time::Instant::now();
    let ddg_url = aether_agent::build_search_url(query);

    // Progress: söker
    let _ = socket
        .send(Message::Text(
            serde_json::json!({"type":"searching","ddg_url": ddg_url}).to_string(),
        ))
        .await;

    // Hämta DDG-sida
    let config = aether_agent::types::FetchConfig::default();
    let html = match aether_agent::fetch::fetch_page(&ddg_url, &config).await {
        Ok(r) => r.body,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type":"error","message":format!("DDG fetch failed: {e}")})
                        .to_string(),
                ))
                .await;
            return;
        }
    };

    // Parsa sökresultat
    let search_json = aether_agent::search_from_html(query, &html, top_n, goal);
    let mut search_result: aether_agent::search::SearchResult =
        match serde_json::from_str(&search_json) {
            Ok(r) => r,
            Err(_) => {
                let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type":"error","message":"Failed to parse search results"})
                        .to_string(),
                ))
                .await;
                return;
            }
        };

    let effective_goal = if goal.is_empty() {
        format!("hitta svar på: {}", query)
    } else {
        goal.to_string()
    };

    // Streama resultat ett-i-taget
    for (idx, entry) in search_result.results.iter_mut().enumerate() {
        let mut entry_json = serde_json::json!({
            "title": entry.title,
            "url": entry.url,
            "snippet": entry.snippet,
        });

        // Deep fetch om begärt
        if deep {
            let fetch_cfg = aether_agent::types::FetchConfig::default();
            let g = effective_goal.clone();
            let url = entry.url.clone();
            let mnpr = max_nodes_per_result;
            if let Ok(Ok(result)) = tokio::time::timeout(
                std::time::Duration::from_secs(8),
                aether_agent::fetch::fetch_page(&url, &fetch_cfg),
            )
            .await
            {
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
                if !nodes.is_empty() {
                    entry.page_content = Some(nodes.clone());
                    entry_json["page_content"] = serde_json::to_value(&nodes).unwrap_or_default();

                    // Berika snippet
                    let best_text: Vec<&str> = nodes
                        .iter()
                        .filter(|n| n.label.len() > 30)
                        .take(2)
                        .map(|n| n.label.as_str())
                        .collect();
                    if !best_text.is_empty()
                        && (entry.snippet.len() < 30 || entry.snippet.contains("www."))
                    {
                        let enriched = best_text.join(" | ");
                        entry.snippet = enriched.clone();
                        entry_json["snippet"] = serde_json::json!(enriched);
                    }
                }
            }
        }

        let msg = serde_json::json!({
            "type": "result",
            "index": idx,
            "entry": entry_json,
        });
        if socket.send(Message::Text(msg.to_string())).await.is_err() {
            return;
        }
    }

    let elapsed_ms = search_start.elapsed().as_millis() as u64;
    let total = search_result.results.len();

    let done_msg = serde_json::json!({
        "type": "done",
        "total": total,
        "elapsed_ms": elapsed_ms,
        "direct_answer": search_result.direct_answer,
    });
    let _ = socket.send(Message::Text(done_msg.to_string())).await;
}

// ─── Fas 16: WebSocket Stream ────────────────────────────────────────────────

/// WebSocket-baserad realtidsstreaming av semantiska noder.
///
/// Protokoll:
/// Client → Server:
///   {"type":"start", "html":"...", "goal":"...", "url":"...", "config":{"top_n":10,...}}
///   {"type":"directive", "action":"expand", "node_id": 5}
///   {"type":"directive", "action":"stop"}
///   {"type":"directive", "action":"next_branch"}
///   {"type":"directive", "action":"lower_threshold", "value": 0.1}
///
/// Server → Client:
///   {"type":"meta", "total_dom_nodes":372, "goal":"...", "url":"..."}
///   {"type":"chunk", "chunk_id":0, "nodes":[...], "nodes_emitted":10, "nodes_seen":372}
///   {"type":"warning", "warning":{...}}
///   {"type":"done", "nodes_emitted":10, "total_dom_nodes":372, ...}
///   {"type":"error", "message":"..."}
async fn ws_stream_handler(ws: axum::extract::WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_ws_stream)
}

async fn handle_ws_stream(mut socket: axum::extract::ws::WebSocket) {
    use axum::extract::ws::Message;

    // Vänta på start-meddelande
    let start_msg =
        match tokio::time::timeout(std::time::Duration::from_secs(30), socket.recv()).await {
            Ok(Some(Ok(Message::Text(text)))) => text,
            _ => {
                let _ = socket
                    .send(Message::Text(
                        r#"{"type":"error","message":"Timeout eller ogiltigt start-meddelande"}"#
                            .to_string(),
                    ))
                    .await;
                return;
            }
        };

    // Parsa start-meddelande
    let start: serde_json::Value = match serde_json::from_str(&start_msg) {
        Ok(v) => v,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({"type":"error","message":format!("JSON-parse error: {}", e)})
                        .to_string(),
                ))
                .await;
            return;
        }
    };

    let msg_type = start.get("type").and_then(|v| v.as_str()).unwrap_or("");
    if msg_type != "start" {
        let _ = socket
            .send(Message::Text(
                r#"{"type":"error","message":"Förväntat type: start"}"#.to_string(),
            ))
            .await;
        return;
    }

    let html = start.get("html").and_then(|v| v.as_str()).unwrap_or("");
    let goal = start.get("goal").and_then(|v| v.as_str()).unwrap_or("");
    let url = start.get("url").and_then(|v| v.as_str()).unwrap_or("");

    if html.is_empty() || goal.is_empty() {
        let _ = socket
            .send(Message::Text(
                r#"{"type":"error","message":"html och goal krävs"}"#.to_string(),
            ))
            .await;
        return;
    }

    let config_val = start.get("config");
    let top_n = config_val
        .and_then(|c| c.get("top_n"))
        .and_then(|v| v.as_u64())
        .unwrap_or(10) as usize;
    let min_relevance = config_val
        .and_then(|c| c.get("min_relevance"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3) as f32;
    let max_nodes = config_val
        .and_then(|c| c.get("max_nodes"))
        .and_then(|v| v.as_u64())
        .unwrap_or(50) as usize;

    let config = aether_agent::stream_engine::StreamParseConfig {
        chunk_size: top_n,
        min_relevance,
        max_nodes,
    };

    // Skapa engine och kör initial parse i blocking thread
    let html_owned = html.to_string();
    let goal_owned = goal.to_string();
    let url_owned = url.to_string();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<aether_agent::stream_engine::StreamChunk>(32);

    // Kör parsning + initial emission i en blocking thread
    let parse_handle = tokio::task::spawn_blocking(move || {
        let mut engine = aether_agent::stream_engine::StreamEngine::new(&goal_owned, config);
        engine.run_streaming(&html_owned, &url_owned, |chunk| {
            // Skicka via channel (blockerande om buffert full)
            let _ = tx.blocking_send(chunk);
        });
        // Returnera engine för att kunna hantera directives senare
        engine
    });

    // Skicka chunks till WebSocket medan parsning pågår
    while let Some(c) = rx.recv().await {
        let json = match serde_json::to_string(&c) {
            Ok(j) => j,
            Err(_) => continue,
        };
        if socket.send(Message::Text(json)).await.is_err() {
            return; // Klient disconnectade
        }
    }

    // Parsern är klar (tx droppades) — hämta engine för directives
    let mut engine = match parse_handle.await {
        Ok(eng) => eng,
        Err(_) => return,
    };

    // Directive-loop: vänta på expand/stop/next_branch/lower_threshold
    loop {
        let msg = match tokio::time::timeout(
            std::time::Duration::from_secs(300), // 5 min timeout
            socket.recv(),
        )
        .await
        {
            Ok(Some(Ok(Message::Text(text)))) => text,
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Err(_) => break,
            _ => continue,
        };

        let directive_val: serde_json::Value = match serde_json::from_str(&msg) {
            Ok(v) => v,
            Err(e) => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::json!({"type":"error","message":format!("JSON error: {}", e)})
                            .to_string(),
                    ))
                    .await;
                continue;
            }
        };

        let action = directive_val
            .get("action")
            .or_else(|| directive_val.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let directive = match action {
            "expand" => {
                let node_id = directive_val
                    .get("node_id")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                aether_agent::stream_state::Directive::Expand { node_id }
            }
            "stop" => aether_agent::stream_state::Directive::Stop,
            "next_branch" => aether_agent::stream_state::Directive::NextBranch,
            "lower_threshold" => {
                let value = directive_val
                    .get("value")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.1) as f32;
                aether_agent::stream_state::Directive::LowerThreshold { value }
            }
            _ => {
                let _ = socket
                    .send(Message::Text(
                        serde_json::json!({"type":"error","message":format!("Okänd directive: {}", action)})
                            .to_string(),
                    ))
                    .await;
                continue;
            }
        };

        // Kör directive och samla chunks
        let mut chunks = Vec::new();
        engine.handle_directive_streaming(directive, &mut |chunk| {
            chunks.push(chunk);
        });

        // Skicka alla genererade chunks
        for chunk in chunks {
            let json = match serde_json::to_string(&chunk) {
                Ok(j) => j,
                Err(_) => continue,
            };
            if socket.send(Message::Text(json)).await.is_err() {
                return;
            }
        }

        if engine.is_done() {
            break;
        }
    }
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

async fn adaptive_crawl_handler(Json(req): Json<AdaptiveCrawlRequest>) -> impl IntoResponse {
    let config = aether_agent::adaptive::AdaptiveConfig {
        max_pages: req.max_pages,
        max_depth: req.max_depth,
        top_k_links: req.top_k_links,
        min_gain_threshold: req.min_gain,
        top_n_per_page: req.top_n_per_page,
        ..Default::default()
    };

    let result = aether_agent::adaptive::adaptive_crawl(&req.url, &req.goal, config).await;
    let json = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
    (StatusCode::OK, json)
}

async fn extract_links_handler(Json(req): Json<ExtractLinksRequest>) -> impl IntoResponse {
    let html = req.html.as_deref().unwrap_or("");
    let url = req.url.as_deref().unwrap_or("");
    let goal = req.goal.as_deref().unwrap_or("");
    let json = aether_agent::extract_links(html, goal, url, req.max_links);
    (StatusCode::OK, json)
}

async fn fetch_extract_links_handler(Json(req): Json<ExtractLinksRequest>) -> impl IntoResponse {
    let url = req.url.as_deref().unwrap_or("");
    if url.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            r#"{"error":"url required"}"#.to_string(),
        );
    }

    let config = aether_agent::types::FetchConfig::default();
    let fetch_result = match aether_agent::fetch::fetch_page(url, &config).await {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                serde_json::to_string(&ErrorResponse { error: e }).unwrap_or_default(),
            );
        }
    };

    let goal_str = req.goal.as_deref().unwrap_or("").to_string();
    let final_url = fetch_result.final_url.clone();
    let body = fetch_result.body;
    let max_links = req.max_links as usize;
    let filter_nav = req.filter_navigation;
    let include_head = req.include_head_data;

    // Sync: parse + extract links
    let link_config = aether_agent::link_extract::LinkExtractionConfig {
        goal: if goal_str.is_empty() {
            None
        } else {
            Some(goal_str.clone())
        },
        max_links,
        include_context: true,
        include_structural_role: true,
        filter_navigation: filter_nav,
        min_relevance: 0.0,
        include_head_data: include_head,
        head_concurrency: 8,
    };

    let tree = aether_agent::build_tree_for_crfr(&body, &goal_str, &final_url, false);
    let mut result = aether_agent::link_extract::extract_links_from_tree(
        &tree.nodes,
        &final_url,
        &link_config,
        None,
    );

    // Async: HEAD-fetch metadata om begärt
    if include_head {
        let goal_words: Vec<String> = goal_str
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| s.len() > 2)
            .map(String::from)
            .collect();
        aether_agent::link_extract::enrich_links_with_head(&mut result.links, &goal_words, 8).await;
    }

    let json = serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error":"{e}"}}"#));
    (StatusCode::OK, json)
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
    false
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

    // Steg 1.5: Kör QuickJS på original HTML (INNAN CSS-inlining).
    // Original-HTML är liten (typiskt 50–500 KB) → ArenaDom + QuickJS är snabb.
    // CSS-inlining blåser upp HTML till 2–10 MB → JS-eval fungerar aldrig där.
    // Genom att köra JS här applicerar vi DOMContentLoaded-mutationer (klassbyta,
    // nav-toggle, visa/dölj element) INNAN CSS kompileras — Blitz renderar rätt state.
    #[cfg(feature = "js-eval")]
    let html_after_js: String = aether_agent::apply_js_mutations(html, req.width, req.height);
    #[cfg(not(feature = "js-eval"))]
    let html_after_js: String = html.to_string();
    let html_for_css: &str = &html_after_js;

    // Steg 2: Inline extern CSS med detaljerad felrapportering
    let css_result = fetch::inline_external_css_detailed(html_for_css, final_url).await;
    let html_with_css = css_result.html.clone();

    // Steg 2b: Hämta och inlina externa scripts (SPA-stöd)
    // Skippa JS-inlining helt i render-pipelinen — CSS-inlining redan blåser upp HTML
    // och JS-inlining ovanpå det kraschar Blitz (github 569KB → 3.8MB med CSS+JS)
    let js_result = fetch::JsInlineResult {
        html: html_with_css.clone(),
        scripts_found: 0,
        scripts_loaded: 0,
        scripts_failed: 0,
        js_bytes_added: 0,
    };
    let html_with_js = html_with_css;

    // Steg 3: Rendera med TieredBackend (Blitz → CDP-fallback) — med timeout-skydd
    // Auto-detektera fast_render baserat på original HTML-storlek (före CSS/JS-inlining)
    // Tröskeln 500KB matchar riktigt tunga sidor (github 569KB) som kraschar Blitz
    const FAST_RENDER_THRESHOLD: usize = 500 * 1024;
    let fast_render = req
        .fast_render
        .unwrap_or(html.len() > FAST_RENDER_THRESHOLD);
    let html_for_render = html_with_js;
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
    let js_scripts_found = js_result.scripts_found;
    let js_scripts_loaded = js_result.scripts_loaded;
    let js_bytes_added = js_result.js_bytes_added;

    let render_future = tokio::task::spawn_blocking(move || {
        #[cfg(feature = "js-eval")]
        {
            if js_code.is_empty() {
                // catch_unwind: Vello/GradientLut kraschar ibland på edge-case gradienter
                let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    aether_agent::screenshot_with_tier(
                        &html_for_render,
                        &url_for_render,
                        render_width,
                        render_height,
                        fast_render,
                    )
                }));
                let render_result = match render_result {
                    Ok(r) => r,
                    Err(_) => Err("Blitz/Vello panic (gradient edge case)".to_string()),
                };
                match render_result {
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
                            "js_scripts_found": js_scripts_found,
                            "js_scripts_loaded": js_scripts_loaded,
                            "js_bytes_added": js_bytes_added,
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
                        "js_scripts_found": js_scripts_found,
                        "js_scripts_loaded": js_scripts_loaded,
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
            let render_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                aether_agent::screenshot_with_tier(
                    &html_for_render,
                    &url_for_render,
                    render_width,
                    render_height,
                    fast_render,
                )
            }));
            let render_result = match render_result {
                Ok(r) => r,
                Err(_) => Err("Blitz/Vello panic (gradient edge case)".to_string()),
            };
            match render_result {
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

/// Gamla MCP tool-definitioner (pre-konsolidering) — behålls för bakåtkompatibilitet
#[allow(dead_code)]
fn mcp_tool_definitions_legacy() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "parse",
            "description": "Parse HTML to a semantic accessibility tree with goal-relevance scoring. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Parse HTML and return only the top-N most relevant nodes. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "name": "parse_hybrid",
            "description": "RECOMMENDED: Parse HTML using the hybrid BM25+HDC+Neural pipeline. Stage 3 supports 'reranker' param: 'minilm' (default, ~1.2s), 'colbert' (MaxSim, ~0.4s, 2.8x faster + 41% better node quality), or 'hybrid' (adaptive blend). ColBERT ranks fact-bearing nodes (prices, stats, rates) above headings/nav. Use reranker='colbert' for best results.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "goal": {"type": "string", "description": "The agent's current goal"},
                    "url": {"type": "string", "description": "The page URL"},
                    "top_n": {"type": "integer", "description": "Max nodes to return (default: 100)", "default": 100},
                    "reranker": {"type": "string", "enum": ["minilm", "colbert", "hybrid"], "description": "Stage 3 reranker: 'colbert' (default), 'colbert' (recommended: faster + better), 'hybrid'", "default": "colbert"}
                },
                "required": ["html", "goal", "url"]
            }
        },
        {
            "name": "fetch_parse",
            "description": "Fetch a URL and parse it into a semantic tree in one call. REAL-TIME: Via WebSocket /ws/api, receive multi-stage progress (fetching → parsing → result).",
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
            "description": "Find the best clickable element matching a target label. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Map form fields to provided key/value pairs. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Extract structured data from a page by semantic keys. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Check text for prompt injection patterns. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Compile a complex goal into an optimized action plan. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Classify URL against semantic firewall (L1/L2/L3). REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Compare two semantic trees and return only the delta. 70-99% token savings. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Fetch a URL and extract structured data in one call. REAL-TIME: Via WebSocket /ws/api, receive multi-stage progress (fetching → parsing → result).",
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
            "description": "Fetch a URL and find a clickable element in one call. REAL-TIME: Via WebSocket /ws/api, receive multi-stage progress (fetching → parsing → result).",
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
            "description": "Parse HTML with automatic JS evaluation in sandboxed QuickJS engine. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Build a causal action graph from temporal page snapshots and actions. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Predict the outcome of an action using the causal graph. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Find the safest path to a goal state through the causal graph. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Discover WebMCP tool definitions embedded in an HTML page. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Ground semantic tree with visual bounding box annotations. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Match a bounding box against semantic tree nodes using IoU. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Create a shared diff store for cross-agent collaboration. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "register_collab_agent",
            "description": "Register an agent in a collaboration store. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Publish a semantic delta to the collaboration store. Pass the FULL output from diff_trees as delta_json. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Fetch new semantic deltas from the collaboration store. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Scan HTML for hidden XHR/fetch/AJAX network calls in inline scripts and event handlers. Discovers fetch(), XMLHttpRequest.open(), $.ajax(), $.get(), $.post() patterns. Returns array of {url, method, headers} objects. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Take a screenshot using the intelligent TieredBackend. Tier 1 (Blitz, pure Rust) renders static HTML/CSS in ~10-50ms without Chrome. If Blitz fails or JavaScript rendering is needed, Tier 2 (CDP/Chrome) takes over automatically. Returns: tier_used, latency_ms, size_bytes, and escalation_reason if tier switching occurred. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Get rendering tier statistics: how many screenshots were rendered by Blitz (Tier 1) vs CDP/Chrome (Tier 2), escalation count, and average latency per tier. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "parse_screenshot",
            "description": "Analyze a screenshot using YOLOv8-nano object detection to find UI elements (buttons, inputs, links, icons, text, images, checkboxes, selects, headings). Returns detected elements with bounding boxes, confidence scores, and a semantic tree. Requires vision feature flag. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Analyze a screenshot using the server's pre-loaded YOLOv8-nano model. Detects UI elements (buttons, inputs, links, icons, text, images, checkboxes, selects, headings) and returns bounding boxes, confidence scores, and a semantic tree. No model upload needed — uses the model configured via AETHER_MODEL_URL/AETHER_MODEL_PATH. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "ALL-IN-ONE: Fetch a URL, render it to a pixel-perfect screenshot with Blitz (pure Rust browser engine), then analyze with YOLOv8 vision. Returns: 1) the actual screenshot as image/png, 2) an annotated image with color-coded bounding boxes around detected UI elements, 3) JSON with all detections (class, confidence, bbox) and semantic tree. USE THIS TOOL WHEN: you want to visually analyze any web page — just provide the URL and goal. No external browser needed. REAL-TIME: Via WebSocket /ws/api, receive multi-stage progress (fetching → rendering → vision → result).",
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
            "description": "Build a DuckDuckGo search URL for a query. Returns the URL to fetch. For auto-fetch, use fetch_search instead. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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
            "description": "Search the web via DuckDuckGo: fetches DDG HTML, parses results, and returns structured search results with title, URL, snippet, domain, confidence, and optional direct_answer. Use this when you don't know which URL to visit. REAL-TIME: Via WebSocket /ws/api, receive multi-stage progress (searching → fetching → parsing → result).",
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
            "description": "Goal-driven adaptive DOM streaming. Parses HTML and emits only the most relevant nodes for the given goal, with 90-99% token savings. Use instead of parse/parse_top when you want minimal output focused on what matters. Returns ranked nodes, token savings ratio, and chunk metadata. REAL-TIME: For interactive streaming, connect to WebSocket /ws/stream. Also via /ws/api gateway.",
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
            "description": "ALL-IN-ONE: Fetch a URL and run goal-driven adaptive DOM streaming. Combines fetch + stream_parse in one call. Returns only the most relevant nodes for the given goal with 90-99% token savings. Use this instead of fetch_parse when you want minimal, goal-focused output. REAL-TIME: Via WebSocket /ws/api, receive multi-stage progress (fetching → parsing → result).",
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
            "description": "Goal-driven adaptive DOM streaming with LLM directives. Like stream_parse but accepts directives to control traversal: expand(node_id) to get children, next_branch to jump to next top-ranked unsent nodes, lower_threshold(value) to reduce min_relevance, stop to halt immediately. Use for interactive multi-step exploration. REAL-TIME: For interactive streaming, connect to WebSocket /ws/stream. Also via /ws/api gateway.",
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
            "description": "Render HTML with JavaScript execution: evaluates JS code against the DOM (via QuickJS sandbox), then renders the modified DOM to a PNG screenshot (via Blitz). Returns: base64-encoded PNG, mutation count, JS eval stats, timing. REAL-TIME: Available via WebSocket /ws/api for streaming progress.",
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

/// 12 konsoliderade MCP tools — ersätter 35+ gamla verktyg
fn mcp_tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "parse",
            "description": "Unified parsing: HTML/URL/screenshot → semantic tree or markdown. Auto-detects input type. Includes JS evaluation when needed, top-N filtering, and hydration extraction. Set hybrid=true for BM25+HDC+Neural scoring (recommended). With hybrid=true, set reranker='colbert' for best quality (2.8x faster, 41% better node ranking). Replaces: parse, parse_top, parse_with_js, fetch_parse, html_to_markdown, parse_screenshot.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch and parse (auto-detected)"},
                    "html": {"type": "string", "description": "Raw HTML to parse directly"},
                    "screenshot_b64": {"type": "string", "description": "Base64-encoded PNG for YOLO vision analysis"},
                    "goal": {"type": "string", "description": "Agent goal for relevance scoring"},
                    "top_n": {"type": "integer", "description": "Limit to N most relevant nodes (default: all)"},
                    "format": {"type": "string", "enum": ["tree", "markdown"], "description": "Output format (default: tree)", "default": "tree"},
                    "js": {"type": "boolean", "description": "Force JS evaluation (true/false/omit for auto)"},
                    "hybrid": {"type": "boolean", "description": "Use hybrid BM25+HDC+Neural scoring pipeline (default: false). Recommended when using top_n.", "default": false},
                    "reranker": {"type": "string", "enum": ["minilm", "colbert", "hybrid"], "description": "Stage 3 reranker when hybrid=true: 'colbert' (recommended, 2.8x faster + 41% better), 'colbert' (default), 'hybrid' (blend)", "default": "colbert"}
                },
                "required": ["goal"]
            }
        },
        {
            "name": "parse_hybrid",
            "description": "Parse HTML/URL using the hybrid BM25+HDC+Neural scoring pipeline. Supports 'reranker' param: 'minilm' (default, ~1.2s), 'colbert' (MaxSim late interaction, ~0.4s, 2.8x faster + 41% higher node quality), or 'hybrid' (adaptive blend). ColBERT excels at finding specific facts in long mixed-content nodes. Use reranker='colbert' for best speed and quality.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch and parse"},
                    "html": {"type": "string", "description": "Raw HTML to parse directly"},
                    "goal": {"type": "string", "description": "Agent goal for relevance scoring"},
                    "top_n": {"type": "integer", "description": "Max nodes to return (default: 100)", "default": 100},
                    "reranker": {"type": "string", "enum": ["minilm", "colbert", "hybrid"], "description": "Stage 3 reranker: 'colbert' recommended (faster + better quality)", "default": "colbert"}
                },
                "required": ["goal"]
            }
        },
        {
            "name": "parse_crfr",
            "description": "CRFR (Causal Resonance Field Retrieval) — next-gen DOM parsing that treats the page as a living resonance field. 10-15x faster than parse_hybrid (no ONNX inference). LEARNS from feedback: call crfr_feedback after finding answers to improve future queries on the same URL.\n\nIMPORTANT — GOAL EXPANSION: Expand the 'goal' with synonyms, translations, and expected values before calling.\nExample: 'what is the price?' → goal: 'price pris cost £ $ kr amount total fee belopp'\nExample: 'who wrote this?' → goal: 'author författare writer journalist by name publicerad'\n\nOutput includes resonance_type per node: Direct (keyword match), Propagated (wave from neighbor), CausalMemory (learned from past queries).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch, or page URL for caching (same URL reuses causal memory)"},
                    "html": {"type": "string", "description": "Raw HTML to parse directly (if omitted, fetches from url)"},
                    "goal": {"type": "string", "description": "EXPAND THIS: Include user's question + 5-10 synonyms, translations, expected values. NO generic words."},
                    "top_n": {"type": "integer", "description": "Max nodes to return (default: 20). Gap-detection often returns fewer.", "default": 20},
                    "run_js": {"type": "boolean", "description": "Evaluate inline JavaScript via QuickJS sandbox before parsing. Use for SPA/dynamic pages.", "default": false},
                    "output_format": {"type": "string", "enum": ["json", "markdown"], "description": "Output format: 'json' (structured nodes) or 'markdown' (token-efficient text for LLM consumption)", "default": "json"}
                },
                "required": ["goal"]
            }
        },
        {
            "name": "crfr_feedback",
            "description": "Teach CRFR which nodes contained the correct answer. Call AFTER parse_crfr when you find useful information. This builds causal memory so future similar queries on the same URL rank those nodes higher.\n\nWorkflow:\n1. parse_crfr(goal='price pris cost') → nodes [id:5, id:12, id:23]\n2. Node 12 has the answer → crfr_feedback(url, goal, [12])\n3. Next query: node 12 gets causal boost → ranked higher automatically",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "Page URL (must match a previous parse_crfr call)"},
                    "goal": {"type": "string", "description": "The goal that was used when parsing"},
                    "successful_node_ids": {"type": "array", "items": {"type": "integer"}, "description": "Array of node IDs that contained the correct answer"}
                },
                "required": ["url", "goal", "successful_node_ids"]
            }
        },
        {
            "name": "parse_crfr_multi",
            "description": "Run multiple goal variants through CRFR and merge results. Exploits sub-ms cache to try synonyms/translations. Pass goals as array.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "Raw HTML string"},
                    "url": {"type": "string", "description": "Source URL"},
                    "goals": {"type": "array", "items": {"type": "string"}, "description": "Array of goal variant strings"},
                    "top_n": {"type": "integer", "default": 20, "description": "Max nodes to return"}
                },
                "required": ["goals"]
            }
        },
        {
            "name": "crfr_save",
            "description": "Save CRFR field (causal memory) for a URL to JSON. Use for persistent learning across sessions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL whose CRFR field to save"}
                },
                "required": ["url"]
            }
        },
        {
            "name": "crfr_load",
            "description": "Load a previously saved CRFR field from JSON. Restores causal memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "json": {"type": "string", "description": "JSON string from a previous crfr_save call"}
                },
                "required": ["json"]
            }
        },
        {
            "name": "crfr_transfer",
            "description": "Transfer causal learning from one URL to another (cross-site learning). Use when visiting similar sites.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "donor_url": {"type": "string", "description": "URL to copy causal memory from"},
                    "recipient_url": {"type": "string", "description": "URL to apply causal memory to"},
                    "min_similarity": {"type": "number", "default": 0.3, "description": "Minimum similarity threshold for transfer"}
                },
                "required": ["donor_url", "recipient_url"]
            }
        },
        {
            "name": "crfr_update",
            "description": "Update a specific node in the CRFR field. Change label, role, or value for a cached node by ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL whose CRFR field contains the node"},
                    "node_id": {"type": "integer", "description": "Node ID to update"},
                    "new_label": {"type": "string", "description": "New label for the node"},
                    "new_role": {"type": "string", "description": "New role for the node"},
                    "new_value": {"type": "string", "default": "", "description": "New value for the node"}
                },
                "required": ["url", "node_id", "new_label", "new_role"]
            }
        },
        {
            "name": "extract_multi",
            "description": "Extract structured data for multiple keys with per-key limits. Returns up to max_per_key results per key.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "html": {"type": "string", "description": "HTML content to extract from"},
                    "goal": {"type": "string", "description": "Agent goal context"},
                    "url": {"type": "string", "description": "Page URL"},
                    "keys": {"type": "array", "items": {"type": "string"}, "description": "Data keys to extract"},
                    "max_per_key": {"type": "integer", "default": 5, "description": "Maximum results per key"}
                },
                "required": ["html", "goal", "url", "keys"]
            }
        },
        {
            "name": "act",
            "description": "Interact with page elements: click buttons, fill forms, or extract data. Provide HTML or URL + an action type. Security (injection scan + firewall) runs automatically. Replaces: find_and_click, fill_form, extract_data, fetch_click, fetch_extract.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch (auto-detected)"},
                    "html": {"type": "string", "description": "Raw HTML"},
                    "goal": {"type": "string", "description": "Agent goal context"},
                    "action": {"type": "string", "enum": ["click", "fill", "extract"], "description": "Action type"},
                    "target": {"type": "string", "description": "Label/text to click (for action=click)"},
                    "fields": {"type": "object", "additionalProperties": {"type": "string"}, "description": "Form fields as key→value (for action=fill)"},
                    "keys": {"type": "array", "items": {"type": "string"}, "description": "Data keys to extract (for action=extract)"}
                },
                "required": ["goal", "action"]
            }
        },
        {
            "name": "stream",
            "description": "Adaptive DOM streaming for large pages. Emits only the most goal-relevant nodes with 90-99% token savings. Supports LLM-driven directives to expand nodes, change thresholds, or stop. Replaces: stream_parse, stream_parse_directive, fetch_stream_parse.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch"},
                    "html": {"type": "string", "description": "Raw HTML"},
                    "goal": {"type": "string", "description": "Agent goal for relevance filtering"},
                    "max_nodes": {"type": "integer", "description": "Max nodes in output (default: 50)", "default": 50},
                    "min_relevance": {"type": "number", "description": "Minimum relevance threshold 0.0-1.0 (default: 0.3)", "default": 0.3},
                    "directives": {"type": "array", "items": {"type": "string"}, "description": "LLM directives: 'expand(node_id)', 'next_branch', 'stop', 'lower_threshold(0.1)'"}
                },
                "required": ["goal"]
            }
        },
        {
            "name": "plan",
            "description": "Goal decomposition + causal reasoning. Compiles a high-level goal into step-by-step action plan with dependencies, or analyzes causal graphs for safest path. Replaces: compile_goal, build_causal_graph, predict_action_outcome, find_safest_path, execute_plan.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": {"type": "string", "description": "Goal to decompose or analyze"},
                    "action": {"type": "string", "enum": ["compile", "predict", "safest_path", "execute"], "description": "Action (default: compile)", "default": "compile"},
                    "graph_json": {"type": "string", "description": "Causal graph JSON (for predict/safest_path)"},
                    "html": {"type": "string", "description": "HTML page (for execute)"},
                    "url": {"type": "string", "description": "Page URL (for execute)"},
                    "max_steps": {"type": "integer", "description": "Max steps in plan (default: 10)", "default": 10}
                },
                "required": ["goal"]
            }
        },
        {
            "name": "diff",
            "description": "Semantic tree diffing — compare two page snapshots and return only the changes. Achieves 70-99% token savings by sending deltas instead of full trees. Replaces: diff_trees, diff_semantic_trees.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "old_tree": {"type": "string", "description": "Previous semantic tree JSON"},
                    "new_tree": {"type": "string", "description": "Current semantic tree JSON"}
                },
                "required": ["old_tree", "new_tree"]
            }
        },
        {
            "name": "search",
            "description": "Web search via DuckDuckGo. Returns structured results with title, URL, snippet, and confidence. With deep=true (default), fetches and parses each result page. Replaces: search, fetch_search.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Search query"},
                    "goal": {"type": "string", "description": "Agent goal for relevance filtering"},
                    "top_n": {"type": "integer", "description": "Number of results (default: 5)", "default": 5},
                    "deep": {"type": "boolean", "description": "Fetch+parse each result (default: true)", "default": true}
                },
                "required": ["query"]
            }
        },
        {
            "name": "secure",
            "description": "Explicit security check. Auto-detects: text → injection scan, url → firewall classify, urls → batch classify. NOTE: Security runs automatically in all other tools — use this only for explicit pre-checks. Replaces: check_injection, classify_request, classify_request_batch.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": {"type": "string", "description": "Text to scan for prompt injection"},
                    "url": {"type": "string", "description": "Single URL to classify via firewall"},
                    "urls": {"type": "array", "items": {"type": "string"}, "description": "Batch of URLs to classify"},
                    "goal": {"type": "string", "description": "Agent goal (for firewall relevance scoring)"}
                }
            }
        },
        {
            "name": "vision",
            "description": "Visual analysis: screenshots, YOLO UI-element detection, grounding, bbox matching. Auto-selects Blitz (pure Rust) or Chrome CDP for rendering. Replaces: tiered_screenshot, parse_screenshot, vision_parse, fetch_vision, ground_semantic_tree, match_bbox_iou.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to render and analyze"},
                    "html": {"type": "string", "description": "HTML to render"},
                    "screenshot_b64": {"type": "string", "description": "Pre-captured screenshot as base64 PNG"},
                    "goal": {"type": "string", "description": "Agent goal for relevance scoring"},
                    "mode": {"type": "string", "enum": ["detect", "screenshot", "ground", "match"], "description": "Vision mode (default: detect)", "default": "detect"},
                    "annotations": {"type": "array", "description": "Bbox annotations for grounding (mode=ground)"},
                    "bbox": {"type": "object", "description": "Bounding box to match (mode=match)"},
                    "tree_json": {"type": "string", "description": "Semantic tree JSON (mode=match)"},
                    "width": {"type": "integer", "description": "Viewport width (default: 1280)", "default": 1280},
                    "height": {"type": "integer", "description": "Viewport height (default: 720)", "default": 720}
                }
            }
        },
        {
            "name": "discover",
            "description": "Discover hidden resources in web pages: WebMCP tool registrations and XHR/fetch API endpoints. Scans inline scripts for navigator.modelContext.registerTool(), <script type='application/mcp+json'>, window.mcpTools, fetch(), XMLHttpRequest. Replaces: discover_webmcp, detect_xhr_urls.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "url": {"type": "string", "description": "URL to fetch and scan"},
                    "html": {"type": "string", "description": "Raw HTML to scan"},
                    "mode": {"type": "string", "enum": ["all", "webmcp", "xhr"], "description": "What to discover (default: all)", "default": "all"}
                }
            }
        },
        {
            "name": "session",
            "description": "Session lifecycle: cookies, OAuth tokens, login detection. Expired cookies auto-cleaned. Token refresh auto-triggered. Replaces: all 11 session endpoints + detect_login_form.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["create", "status", "cookies", "token", "oauth", "detect_login", "evict", "mark_logged_in", "refresh"], "description": "Session action"},
                    "session_json": {"type": "string", "description": "Session state JSON"},
                    "domain": {"type": "string", "description": "Cookie domain"},
                    "path": {"type": "string", "description": "Cookie path (default: /)"},
                    "cookies": {"type": "array", "items": {"type": "string"}, "description": "Set-Cookie headers to add"},
                    "access_token": {"type": "string", "description": "OAuth access token"},
                    "html": {"type": "string", "description": "HTML for login detection"},
                    "goal": {"type": "string", "description": "Goal for login detection"}
                },
                "required": ["action"]
            }
        },
        {
            "name": "workflow",
            "description": "Multi-page workflow orchestration with automatic planning, rollback, and step tracking. Create a workflow from a goal, feed it pages, report action results. Replaces: all 8 orchestrator endpoints + workflow memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["create", "page", "report", "complete", "rollback", "status"], "description": "Workflow action"},
                    "workflow_json": {"type": "string", "description": "Workflow state JSON"},
                    "goal": {"type": "string", "description": "Workflow goal (for create)"},
                    "start_url": {"type": "string", "description": "Starting URL (for create)"},
                    "html": {"type": "string", "description": "Page HTML (for page action)"},
                    "url": {"type": "string", "description": "Page URL (for page action)"},
                    "result_json": {"type": "string", "description": "Action result JSON (for report)"},
                    "report_type": {"type": "string", "enum": ["click", "fill", "extract"], "description": "Report type (for report)"},
                    "step_index": {"type": "integer", "description": "Step index (for complete/rollback)"}
                },
                "required": ["action"]
            }
        },
        {
            "name": "collab",
            "description": "Multi-agent collaboration: shared diff stores for cross-agent knowledge sharing. Register agents, publish semantic deltas, fetch updates. Auto-cleanup of inactive agents. Replaces: all collab endpoints + tier_stats.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action": {"type": "string", "enum": ["create", "register", "publish", "fetch", "stats"], "description": "Collab action"},
                    "store_json": {"type": "string", "description": "Collab store state JSON"},
                    "agent_id": {"type": "string", "description": "Agent identifier"},
                    "goal": {"type": "string", "description": "Agent goal (for register)"},
                    "url": {"type": "string", "description": "URL for delta (for publish)"},
                    "delta_json": {"type": "string", "description": "Semantic delta JSON (for publish)"}
                },
                "required": ["action"]
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
/// Gammal MCP dispatch — behålls för bakåtkompatibilitet med äldre klienter
#[allow(dead_code)]
async fn mcp_dispatch_tool_legacy(
    name: &str,
    args: &serde_json::Value,
    state: &AppState,
) -> Result<serde_json::Value, String> {
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
        "parse_hybrid" => {
            let html = args["html"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let url = args["url"].as_str().unwrap_or("");
            let top_n = args["top_n"].as_u64().unwrap_or(100) as u32;
            let reranker = args["reranker"].as_str();
            let config = aether_agent::tools::parse_hybrid_tool::build_config(reranker);
            text_ok(aether_agent::parse_top_nodes_with_config(
                html, goal, url, top_n, &config,
            ))
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
        _ => Err(format!("Unknown legacy tool: {name}")),
    }
}

/// 12 konsoliderade MCP tools — dispatch
async fn mcp_dispatch_tool(
    name: &str,
    args: &serde_json::Value,
    _state: &AppState,
) -> Result<serde_json::Value, String> {
    let text_ok = |s: String| -> Result<serde_json::Value, String> {
        Ok(serde_json::json!([{"type": "text", "text": s}]))
    };

    match name {
        // ─── Tool 1: parse ──────────────────────────────────────────
        "parse" => {
            let req = serde_json::from_value::<aether_agent::tools::parse_tool::ParseRequest>(
                args.clone(),
            )
            .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;

            // Async fetch om URL angetts
            if let Some(ref url) = req.url {
                if !url.is_empty() && req.html.is_none() && req.screenshot_b64.is_none() {
                    if let Some(reason) = aether_agent::tools::firewall_check(url, &req.goal) {
                        return text_ok(
                            serde_json::json!({"error": "Firewall blocked", "reason": reason})
                                .to_string(),
                        );
                    }
                    aether_agent::fetch::validate_url(url)?;
                    let config = aether_agent::types::FetchConfig::default();
                    let fetched = aether_agent::fetch::fetch_page(url, &config).await?;
                    // Async variant: resolvar pending fetch-URLs (BUGG J)
                    let result = aether_agent::tools::parse_tool::execute_with_html_async(
                        &fetched.body,
                        &req,
                        &fetched.final_url,
                    )
                    .await;
                    return text_ok(result.to_json());
                }
            }
            let result = aether_agent::tools::parse_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 1b: parse_hybrid ─────────────────────────────────
        "parse_hybrid" => {
            let req = serde_json::from_value::<
                aether_agent::tools::parse_hybrid_tool::ParseHybridRequest,
            >(args.clone())
            .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;

            if let Some(ref url) = req.url {
                if !url.is_empty() && req.html.is_none() {
                    if let Some(reason) = aether_agent::tools::firewall_check(url, &req.goal) {
                        return text_ok(
                            serde_json::json!({"error": "Firewall blocked", "reason": reason})
                                .to_string(),
                        );
                    }
                    #[cfg(feature = "fetch")]
                    {
                        let config = aether_agent::types::FetchConfig::default();
                        match aether_agent::fetch::fetch_page(url, &config).await {
                            Ok(r) => {
                                // Async variant: resolvar pending fetch-URLs (BUGG J)
                                let result =
                                    aether_agent::tools::parse_hybrid_tool::execute_with_html_async(
                                        &r.body, &req, url,
                                    )
                                    .await;
                                return text_ok(result.to_json());
                            }
                            Err(e) => return Err(format!("Fetch failed: {e}")),
                        }
                    }
                    #[cfg(not(feature = "fetch"))]
                    return Err("URL input requires fetch feature".to_string());
                }
            }

            // HTML-input: kör async variant för att resolve pending fetch-URLs
            if let Some(ref html) = req.html {
                if !html.is_empty() {
                    let url = req.url.as_deref().unwrap_or("");
                    #[cfg(feature = "fetch")]
                    {
                        let result =
                            aether_agent::tools::parse_hybrid_tool::execute_with_html_async(
                                html, &req, url,
                            )
                            .await;
                        return text_ok(result.to_json());
                    }
                    #[cfg(not(feature = "fetch"))]
                    {
                        let result = aether_agent::tools::parse_hybrid_tool::execute_with_html(
                            html, &req, url,
                        );
                        return text_ok(result.to_json());
                    }
                }
            }
            let result = aether_agent::tools::parse_hybrid_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 2: act ────────────────────────────────────────────
        "act" => {
            let req =
                serde_json::from_value::<aether_agent::tools::act_tool::ActRequest>(args.clone())
                    .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;

            if let Some(ref url) = req.url {
                if !url.is_empty() && req.html.is_none() {
                    if let Some(reason) = aether_agent::tools::firewall_check(url, &req.goal) {
                        return text_ok(
                            serde_json::json!({"error": "Firewall blocked", "reason": reason})
                                .to_string(),
                        );
                    }
                    aether_agent::fetch::validate_url(url)?;
                    let config = aether_agent::types::FetchConfig::default();
                    let fetched = aether_agent::fetch::fetch_page(url, &config).await?;
                    let result = aether_agent::tools::act_tool::execute_with_html(
                        &fetched.body,
                        &req,
                        &fetched.final_url,
                    );
                    return text_ok(result.to_json());
                }
            }
            let result = aether_agent::tools::act_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 3: stream ─────────────────────────────────────────
        "stream" => {
            let req = serde_json::from_value::<aether_agent::tools::stream_tool::StreamRequest>(
                args.clone(),
            )
            .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;

            if let Some(ref url) = req.url {
                if !url.is_empty() && req.html.is_none() {
                    if let Some(reason) = aether_agent::tools::firewall_check(url, &req.goal) {
                        return text_ok(
                            serde_json::json!({"error": "Firewall blocked", "reason": reason})
                                .to_string(),
                        );
                    }
                    aether_agent::fetch::validate_url(url)?;
                    let config = aether_agent::types::FetchConfig::default();
                    let fetched = aether_agent::fetch::fetch_page(url, &config).await?;
                    let result = aether_agent::tools::stream_tool::execute_with_html(
                        &fetched.body,
                        &req,
                        &fetched.final_url,
                    );
                    return text_ok(result.to_json());
                }
            }
            let result = aether_agent::tools::stream_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 4: plan ───────────────────────────────────────────
        "plan" => {
            let req =
                serde_json::from_value::<aether_agent::tools::plan_tool::PlanRequest>(args.clone())
                    .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;
            let result = aether_agent::tools::plan_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 5: diff ───────────────────────────────────────────
        "diff" => {
            let req =
                serde_json::from_value::<aether_agent::tools::diff_tool::DiffRequest>(args.clone())
                    .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;
            let result = aether_agent::tools::diff_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 6: search ────────────────────────────────────────
        "search" => {
            let req = serde_json::from_value::<aether_agent::tools::search_tool::SearchRequest>(
                args.clone(),
            )
            .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;

            // Auto-fetch DDG + deep hybrid_parse på top-3
            let ddg_url = aether_agent::search::build_ddg_url(&req.query);
            let config = aether_agent::types::FetchConfig::default();
            match aether_agent::fetch::fetch_page(&ddg_url, &config).await {
                Ok(fetched) => {
                    // Async: deep-fetch top-3 resultat med hybrid_parse
                    let result = aether_agent::tools::search_tool::execute_with_html_async(
                        &fetched.body,
                        &req,
                    )
                    .await;
                    text_ok(result.to_json())
                }
                Err(_) => {
                    // Fallback: returnera URL att fetcha manuellt
                    let result = aether_agent::tools::search_tool::execute(&req);
                    text_ok(result.to_json())
                }
            }
        }

        // ─── Tool 7: secure ────────────────────────────────────────
        "secure" => {
            let req = serde_json::from_value::<aether_agent::tools::secure_tool::SecureRequest>(
                args.clone(),
            )
            .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;
            let result = aether_agent::tools::secure_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 8: vision ────────────────────────────────────────
        "vision" => {
            let req = serde_json::from_value::<aether_agent::tools::vision_tool::VisionRequest>(
                args.clone(),
            )
            .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;
            let result = aether_agent::tools::vision_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 9: discover ──────────────────────────────────────
        "discover" => {
            let req =
                serde_json::from_value::<aether_agent::tools::discover_tool::DiscoverRequest>(
                    args.clone(),
                )
                .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;

            if let Some(ref url) = req.url {
                if !url.is_empty() && req.html.is_none() {
                    aether_agent::fetch::validate_url(url)?;
                    let config = aether_agent::types::FetchConfig::default();
                    let fetched = aether_agent::fetch::fetch_page(url, &config).await?;
                    let result =
                        aether_agent::tools::discover_tool::execute_with_html(&fetched.body, &req);
                    return text_ok(result.to_json());
                }
            }
            let result = aether_agent::tools::discover_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 10: session ──────────────────────────────────────
        "session" => {
            let req =
                serde_json::from_value::<aether_agent::tools::session_tool::SessionRequest>(
                    args.clone(),
                )
                .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;
            let result = aether_agent::tools::session_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 11: workflow ─────────────────────────────────────
        "workflow" => {
            let req =
                serde_json::from_value::<aether_agent::tools::workflow_tool::WorkflowRequest>(
                    args.clone(),
                )
                .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;
            let result = aether_agent::tools::workflow_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool 12: collab ──────────────────────────────────────
        "collab" => {
            let req = serde_json::from_value::<aether_agent::tools::collab_tool::CollabRequest>(
                args.clone(),
            )
            .map_err(|e| format!("Ogiltiga parametrar: {e}"))?;
            let result = aether_agent::tools::collab_tool::execute(&req);
            text_ok(result.to_json())
        }

        // ─── Tool: parse_crfr ──────────────────────────────────
        "parse_crfr" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let top_n = args["top_n"].as_u64().unwrap_or(20) as u32;
            let run_js = args["run_js"].as_bool().unwrap_or(false);
            let output_format = args["output_format"].as_str().unwrap_or("json");

            // Hämta HTML: antingen direkt eller via URL
            let html = if let Some(h) = args["html"].as_str() {
                h.to_string()
            } else if !url.is_empty() {
                #[cfg(feature = "fetch")]
                {
                    if let Some(reason) = aether_agent::tools::firewall_check(url, goal) {
                        return text_ok(
                            serde_json::json!({"error": "Firewall blocked", "reason": reason})
                                .to_string(),
                        );
                    }
                    aether_agent::fetch::validate_url(url)?;
                    let config = aether_agent::types::FetchConfig::default();
                    let fetched = aether_agent::fetch::fetch_page(url, &config).await?;
                    fetched.body
                }
                #[cfg(not(feature = "fetch"))]
                {
                    return Err("fetch feature not enabled — provide html directly".to_string());
                }
            } else {
                return Err("Provide either 'html' or 'url'".to_string());
            };

            // 2-pass: build tree → XHR enrich → CRFR propagation
            let goal_str = goal.to_string();
            let page_url = url.to_string();
            let fmt = output_format.to_string();

            let mut tree = tokio::task::spawn_blocking({
                let h = html.clone();
                let g = goal_str.clone();
                let u = page_url.clone();
                move || aether_agent::build_tree_for_crfr(&h, &g, &u, run_js)
            })
            .await
            .unwrap_or_default();

            // SPA enrichment: fetch detected XHR URLs
            #[cfg(feature = "fetch")]
            if !tree.pending_fetch_urls.is_empty() {
                aether_agent::tools::resolve_pending_fetches(&mut tree, &goal_str).await;
            }

            let result = tokio::task::spawn_blocking(move || {
                aether_agent::parse_crfr_from_tree_js(&tree, &goal_str, &page_url, top_n, &fmt, run_js)
            })
            .await
            .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());

            text_ok(result)
        }

        // ─── Tool: crfr_feedback ───────────────────────────────
        "crfr_feedback" => {
            let url = args["url"].as_str().unwrap_or("");
            let goal = args["goal"].as_str().unwrap_or("");
            let ids: Vec<u32> = args["successful_node_ids"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u32))
                        .collect()
                })
                .unwrap_or_default();

            let ids_json = serde_json::to_string(&ids).unwrap_or_else(|_| "[]".to_string());
            let url_str = url.to_string();
            let goal_str = goal.to_string();
            let result = tokio::task::spawn_blocking(move || {
                aether_agent::crfr_feedback(&url_str, &goal_str, &ids_json)
            })
            .await
            .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());

            text_ok(result)
        }

        // ─── Tool: parse_crfr_multi ──────────────────────────────
        "parse_crfr_multi" => {
            let goals: Vec<String> = args["goals"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let goals_json = serde_json::to_string(&goals).unwrap_or_else(|_| "[]".to_string());
            let mut html = args["html"].as_str().unwrap_or("").to_string();
            let url = args["url"].as_str().unwrap_or("").to_string();
            let top_n = args["top_n"].as_u64().unwrap_or(20) as u32;
            // Om html tom men url finns → fetcha
            if html.is_empty() && !url.is_empty() {
                let config = aether_agent::types::FetchConfig::default();
                match aether_agent::fetch::fetch_page(&url, &config).await {
                    Ok(r) => html = r.body,
                    Err(e) => return text_ok(format!(r#"{{"error":"fetch failed: {e}"}}"#)),
                }
            }
            let result = tokio::task::spawn_blocking(move || {
                aether_agent::parse_crfr_multi(&html, &goals_json, &url, top_n)
            })
            .await
            .unwrap_or_else(|_| r#"{"error":"panicked"}"#.to_string());
            text_ok(result)
        }

        // ─── Tool: crfr_save ─────────────────────────────────────
        "crfr_save" => {
            let url = args["url"].as_str().unwrap_or("").to_string();
            let result = tokio::task::spawn_blocking(move || aether_agent::crfr_save_field(&url))
                .await
                .unwrap_or_else(|_| r#"{"error":"panicked"}"#.to_string());
            text_ok(result)
        }

        // ─── Tool: crfr_load ─────────────────────────────────────
        "crfr_load" => {
            let json = args["json"].as_str().unwrap_or("").to_string();
            let result = tokio::task::spawn_blocking(move || aether_agent::crfr_load_field(&json))
                .await
                .unwrap_or_else(|_| r#"{"error":"panicked"}"#.to_string());
            text_ok(result)
        }

        // ─── Tool: crfr_transfer ─────────────────────────────────
        "crfr_transfer" => {
            let donor = args["donor_url"].as_str().unwrap_or("").to_string();
            let recipient = args["recipient_url"].as_str().unwrap_or("").to_string();
            let min_sim = args["min_similarity"].as_f64().unwrap_or(0.3) as f32;
            let result = tokio::task::spawn_blocking(move || {
                aether_agent::crfr_transfer(&donor, &recipient, min_sim)
            })
            .await
            .unwrap_or_else(|_| r#"{"error":"panicked"}"#.to_string());
            text_ok(result)
        }

        // ─── Tool: crfr_update ──────────────────────────────────
        "crfr_update" => {
            let url = args["url"].as_str().unwrap_or("").to_string();
            let node_id = args["node_id"].as_u64().unwrap_or(0) as u32;
            let new_label = args["new_label"].as_str().unwrap_or("").to_string();
            let new_role = args["new_role"].as_str().unwrap_or("").to_string();
            let new_value = args["new_value"].as_str().unwrap_or("").to_string();
            let result = tokio::task::spawn_blocking(move || {
                aether_agent::crfr_update_node(&url, node_id, &new_label, &new_role, &new_value)
            })
            .await
            .unwrap_or_else(|_| r#"{"error":"panicked"}"#.to_string());
            text_ok(result)
        }

        // ─── Tool: extract_multi ────────────────────────────────
        "extract_multi" => {
            let html = args["html"].as_str().unwrap_or("").to_string();
            let goal = args["goal"].as_str().unwrap_or("").to_string();
            let url = args["url"].as_str().unwrap_or("").to_string();
            let keys: Vec<String> = args["keys"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            let keys_json = serde_json::to_string(&keys).unwrap_or_else(|_| "[]".to_string());
            let max_per_key = args["max_per_key"].as_u64().unwrap_or(5) as u32;
            let result = tokio::task::spawn_blocking(move || {
                aether_agent::extract_data_multi(&html, &goal, &url, &keys_json, max_per_key)
            })
            .await
            .unwrap_or_else(|_| r#"{"error":"panicked"}"#.to_string());
            text_ok(result)
        }

        _ => Err(format!(
            "Unknown tool: '{name}'. Available: parse, parse_hybrid, parse_crfr, crfr_feedback, parse_crfr_multi, crfr_save, crfr_load, crfr_transfer, crfr_update, extract_multi, act, stream, plan, diff, search, secure, vision, discover, session, workflow, collab"
        )),
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

// ─── Dashboard Endpoints ────────────────────────────────────────────────────

#[derive(Deserialize)]
struct DashboardWeightsRequest {
    /// Accepterar både string ("12345") och number (12345) — JS Number förlorar precision >2^53
    #[serde(deserialize_with = "deser_u64_from_string_or_number")]
    url_hash: u64,
}

fn deser_u64_from_string_or_number<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<u64, D::Error> {
    use serde::de::{self, Visitor};
    struct U64Visitor;
    impl<'de> Visitor<'de> for U64Visitor {
        type Value = u64;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("u64 as string or number")
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> {
            Ok(v as u64)
        }
        fn visit_f64<E: de::Error>(self, v: f64) -> Result<u64, E> {
            Ok(v as u64)
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.parse().map_err(de::Error::custom)
        }
    }
    d.deserialize_any(U64Visitor)
}

#[derive(Deserialize)]
struct DashboardExploreRequest {
    html: String,
    goal: String,
    url: String,
    #[serde(default = "default_crfr_top_n")]
    top_n: u32,
    /// Set-Cookie headers from HTTP response (for JS cookie bridge)
    #[serde(default)]
    set_cookie_headers: Vec<String>,
}

async fn dashboard_snapshot_handler() -> impl IntoResponse {
    let vision_available = cfg!(feature = "vision");
    let result = aether_agent::dashboard_snapshot(vision_available, 76);
    (StatusCode::OK, result)
}

async fn dashboard_crfr_cache_handler() -> impl IntoResponse {
    let result = aether_agent::dashboard_crfr_cache();
    (StatusCode::OK, result)
}

async fn dashboard_weights_handler(Json(req): Json<DashboardWeightsRequest>) -> impl IntoResponse {
    let result = aether_agent::dashboard_propagation_weights(req.url_hash);
    (StatusCode::OK, result)
}

/// GET /api/dashboard/domain-detail?domain_hash=12345
async fn dashboard_domain_detail_handler(
    axum::extract::Query(params): axum::extract::Query<DomainDetailQuery>,
) -> impl IntoResponse {
    let result = aether_agent::dashboard_domain_detail(params.domain_hash);
    (
        StatusCode::OK,
        [(axum::http::header::CACHE_CONTROL, "no-cache")],
        result,
    )
}

#[derive(Deserialize)]
struct DomainDetailQuery {
    #[serde(deserialize_with = "deser_u64_from_string_or_number")]
    domain_hash: u64,
}

async fn dashboard_wpt_handler() -> impl IntoResponse {
    let result = aether_agent::dashboard_wpt();
    (StatusCode::OK, result)
}

async fn dashboard_persistence_handler() -> impl IntoResponse {
    #[cfg(feature = "persist")]
    let result = aether_agent::dashboard_persistence();
    #[cfg(not(feature = "persist"))]
    let result = r#"{"enabled":false}"#.to_string();
    (StatusCode::OK, result)
}

async fn dashboard_explore_handler(Json(req): Json<DashboardExploreRequest>) -> impl IntoResponse {
    let html = req.html;
    let goal = req.goal;
    let url = req.url;
    let top_n = req.top_n;
    let set_cookies = req.set_cookie_headers;

    // Bygg träd med JS-eval + cookie-bridge (HTTP Set-Cookie → JS document.cookie)
    let goal_clone = goal.clone();
    let url_clone = url.clone();
    let mut tree = tokio::task::spawn_blocking({
        let h = html.clone();
        let g = goal.clone();
        let u = url.clone();
        let sc = set_cookies;
        move || {
            if sc.is_empty() {
                aether_agent::build_tree_for_crfr(&h, &g, &u, true)
            } else {
                aether_agent::build_tree_with_cookies(&h, &g, &u, &sc)
            }
        }
    })
    .await
    .unwrap_or_default();

    // Bot challenge re-fetch: om JS satte cookies och sidan ser ut som en challenge,
    // re-fetcha med JS-cookies och parsa igen
    #[cfg(feature = "fetch")]
    {
        let is_challenge = tokio::task::spawn_blocking({
            let t = tree.clone();
            let c = tree.js_cookies.clone();
            move || aether_agent::is_likely_bot_challenge(&t, &c)
        })
        .await
        .unwrap_or(false);

        if is_challenge && !tree.js_cookies.is_empty() {
            eprintln!(
                "[COOKIE-BRIDGE] Bot challenge detected on {}. Re-fetching with JS cookies...",
                url
            );
            let refetch_config = aether_agent::types::FetchConfig {
                cookies: tree.js_cookies.clone(),
                ..Default::default()
            };

            if let Ok(refetch_result) = aether_agent::fetch::fetch_page(&url, &refetch_config).await
            {
                eprintln!(
                    "[COOKIE-BRIDGE] Re-fetch: {} bytes, status {}",
                    refetch_result.body.len(),
                    refetch_result.status_code
                );
                // Re-parse med nya cookies
                let g2 = goal.clone();
                let u2 = refetch_result.final_url.clone();
                let sc2 = refetch_result.set_cookie_headers.clone();
                let body2 = refetch_result.body;
                tree = tokio::task::spawn_blocking(move || {
                    aether_agent::build_tree_with_cookies(&body2, &g2, &u2, &sc2)
                })
                .await
                .unwrap_or_default();
            }
        }
    }

    // Resolve pending fetch-URLs (SPA enrichment)
    #[cfg(feature = "fetch")]
    if !tree.pending_fetch_urls.is_empty() {
        aether_agent::tools::resolve_pending_fetches(&mut tree, &goal).await;
    }

    // Kör CRFR explorer med traced propagation
    let result = tokio::task::spawn_blocking(move || {
        aether_agent::dashboard_crfr_explore(&tree, &goal_clone, &url_clone, top_n, true)
    })
    .await
    .unwrap_or_else(|_| r#"{"error":"task panicked"}"#.to_string());
    (StatusCode::OK, result)
}

/// Serve a static HTML file from disk at runtime.
/// Files are read from /app/static/ in Docker, or relative paths for local dev.
/// This decouples HTML from the Rust binary — HTML-only changes skip recompilation.
async fn serve_html_file(paths: &[&str]) -> impl IntoResponse {
    for path in paths {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            return (
                StatusCode::OK,
                [("content-type", "text/html; charset=utf-8")],
                content,
            );
        }
    }
    (
        StatusCode::NOT_FOUND,
        [("content-type", "text/html; charset=utf-8")],
        "<h1>404 — HTML file not found</h1>".to_string(),
    )
}

async fn dashboard_html() -> impl IntoResponse {
    serve_html_file(&["/app/static/dashboard.html", "dashboard.html"]).await
}

async fn landing_index() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/index.html",
        "landing-pages/index.html",
    ])
    .await
}

async fn landing_concept_1() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/concept-1-the-reduction.html",
        "landing-pages/concept-1-the-reduction.html",
    ])
    .await
}

async fn landing_concept_2() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/concept-2-the-signal.html",
        "landing-pages/concept-2-the-signal.html",
    ])
    .await
}

async fn landing_try() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/try.html",
        "landing-pages/try.html",
    ])
    .await
}

async fn landing_mission() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/mission.html",
        "landing-pages/mission.html",
    ])
    .await
}

async fn landing_timeline() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/timeline.html",
        "landing-pages/timeline.html",
    ])
    .await
}

async fn landing_live() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/dashboard.html",
        "landing-pages/dashboard.html",
    ])
    .await
}

async fn landing_docs() -> impl IntoResponse {
    serve_html_file(&[
        "/app/static/landing-pages/docs.html",
        "landing-pages/docs.html",
    ])
    .await
}

async fn favicon_svg() -> impl IntoResponse {
    let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 32 32\"><text x=\"8\" y=\"26\" font-family=\"Inter,system-ui,sans-serif\" font-weight=\"800\" font-size=\"28\" fill=\"#3b82f6\">/</text></svg>";
    (StatusCode::OK, [("content-type", "image/svg+xml")], svg)
}

async fn components_js() -> impl IntoResponse {
    let js = match tokio::fs::read_to_string("/app/static/landing-pages/components.js").await {
        Ok(c) => c,
        Err(_) => match tokio::fs::read_to_string("landing-pages/components.js").await {
            Ok(c) => c,
            Err(_) => String::from("// components.js not found"),
        },
    };
    (
        StatusCode::OK,
        [
            ("content-type", "application/javascript; charset=utf-8"),
            ("cache-control", "no-cache, must-revalidate"),
        ],
        js,
    )
}

fn build_router(state: AppState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    // Request counter middleware
    let counter = state.request_count.clone();
    let count_layer = axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let c = counter.clone();
            async move {
                c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                next.run(req).await
            }
        },
    );

    Router::new()
        // Root = landing page
        .route("/", get(landing_concept_1))
        .route("/favicon.svg", get(favicon_svg))
        .route("/components.js", get(components_js))
        .route("/api-info", get(root))
        .route("/tools", get(tool_explorer))
        .route("/api/endpoints", get(api_endpoints))
        .route("/health", get(health))
        .route("/api/memory-stats", get(memory_stats_handler))
        .route("/api/live-stats", get(live_stats_handler))
        // Landing pages
        .route("/landing", get(landing_index))
        .route("/landing/1", get(landing_concept_1))
        .route("/landing/2", get(landing_concept_2))
        .route("/try", get(landing_try))
        .route("/mission", get(landing_mission))
        .route("/timeline", get(landing_timeline))
        .route("/live", get(landing_live))
        .route("/docs", get(landing_docs))
        // Dashboard
        .route("/dashboard", get(dashboard_html))
        .route("/api/dashboard/snapshot", get(dashboard_snapshot_handler))
        .route(
            "/api/dashboard/crfr-cache",
            get(dashboard_crfr_cache_handler),
        )
        .route("/api/dashboard/weights", post(dashboard_weights_handler))
        .route("/api/dashboard/wpt", get(dashboard_wpt_handler))
        .route("/api/dashboard/explore", post(dashboard_explore_handler))
        .route(
            "/api/dashboard/persistence",
            get(dashboard_persistence_handler),
        )
        .route(
            "/api/dashboard/domain-detail",
            get(dashboard_domain_detail_handler),
        )
        // Fas 1: Semantic parsing
        .route("/api/parse", post(parse))
        .route("/api/parse-top", post(parse_top))
        .route("/api/parse-hybrid", post(parse_hybrid))
        .route("/api/parse-crfr", post(parse_crfr_handler))
        .route("/api/crfr-feedback", post(crfr_feedback_handler))
        .route("/api/parse-crfr-multi", post(parse_crfr_multi_handler))
        .route("/api/crfr-save", post(crfr_save_handler))
        .route("/api/crfr-load", post(crfr_load_handler))
        .route("/api/crfr-update", post(crfr_update_handler))
        .route("/api/crfr-transfer", post(crfr_transfer_handler))
        .route("/api/extract-multi", post(extract_multi_handler))
        .route("/api/extract-smart", post(parse_extract_handler))
        .route("/api/fetch/extract-smart", post(fetch_extract_smart))
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
        // Fas 19: Adaptive Crawl + Link Extraction
        .route("/api/adaptive-crawl", post(adaptive_crawl_handler))
        .route("/api/extract-links", post(extract_links_handler))
        .route(
            "/api/fetch/extract-links",
            post(fetch_extract_links_handler),
        )
        .route("/api/directive", post(directive_handler))
        .route("/ws/stream", get(ws_stream_handler))
        .route("/ws/api", get(ws_api_handler))
        .route("/ws/mcp", get(ws_mcp_handler))
        .route("/ws/search", get(ws_search_handler))
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
        .layer(count_layer)
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

/// GET /api/live-stats — aggregated stats for landing page live counters
async fn live_stats_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
) -> impl IntoResponse {
    let fields = aether_agent::resonance::list_cached_fields();
    let (cache_entries, cache_capacity) = aether_agent::resonance::cache_stats();

    let sites_profiled = fields.len();
    let total_queries: u32 = fields.iter().map(|f| f.total_queries).sum();
    let total_nodes: usize = fields.iter().map(|f| f.node_count).sum();
    let total_edges: usize = fields.iter().map(|f| f.edge_count).sum();
    let total_causal_weights: usize = fields.iter().map(|f| f.propagation_weight_count).sum();
    let total_concepts: usize = fields.iter().map(|f| f.concept_memory_count).sum();
    let total_feedback: u32 = fields.iter().map(|f| f.total_feedback).sum();
    let total_successful: u32 = fields.iter().map(|f| f.total_successful_nodes).sum();
    let total_learned: usize = fields.iter().map(|f| f.learned_nodes).sum();
    let total_chars_in: u64 = fields.iter().map(|f| f.total_chars_in).sum();
    let total_chars_out: u64 = fields.iter().map(|f| f.total_chars_out).sum();
    let token_savings_pct = if total_chars_in > 0 {
        ((1.0 - total_chars_out as f64 / total_chars_in as f64) * 100.0).max(0.0)
    } else {
        0.0
    };
    // p95 across all sites
    let all_p95: Vec<u64> = fields
        .iter()
        .filter(|f| f.p95_latency_us > 0)
        .map(|f| f.p95_latency_us)
        .collect();
    let global_p95_us = if all_p95.is_empty() {
        0
    } else {
        let mut sorted = all_p95;
        sorted.sort_unstable();
        let idx = ((sorted.len() as f64) * 0.95).ceil() as usize;
        sorted[idx.min(sorted.len() - 1)]
    };
    let requests = state
        .request_count
        .load(std::sync::atomic::Ordering::Relaxed);
    let uptime_secs = state.started_at.elapsed().as_secs();

    // Use SQLite totals as the authoritative numbers (survive deploys)
    #[cfg(feature = "persist")]
    let (db_fields, _db_domains, _db_size) = aether_agent::persist::db_stats();
    #[cfg(not(feature = "persist"))]
    let db_fields: usize = 0;
    // Show the larger of cache vs db (db has historical, cache has current session)
    let sites_total = sites_profiled.max(db_fields);

    // Per-site leaderboard (sorted by total_queries desc)
    let mut site_list: Vec<serde_json::Value> = fields
        .iter()
        .map(|f| {
            // Extract short domain from URL
            let domain = f
                .url
                .split("//")
                .nth(1)
                .unwrap_or(&f.url)
                .split('/')
                .next()
                .unwrap_or(&f.url)
                .replace("www.", "");
            serde_json::json!({
                "url": f.url,
                "domain": domain,
                "domain_hash": f.domain_hash.to_string(),
                "nodes": f.node_count,
                "queries": f.total_queries,
                "feedback": f.total_feedback,
                "successful_nodes": f.total_successful_nodes,
                "learned_nodes": f.learned_nodes,
                "causal_weights": f.propagation_weight_count,
                "concepts": f.concept_memory_count,
                "last_propagation_us": f.last_propagation_us,
                "last_result_count": f.last_result_count,
                "max_depth": f.max_depth,
                "edges": f.edge_count,
                "chars_in": f.total_chars_in,
                "chars_out": f.total_chars_out,
                "p95_us": f.p95_latency_us,
            })
        })
        .collect();
    site_list.sort_by(|a, b| {
        let aq = a["queries"].as_u64().unwrap_or(0);
        let bq = b["queries"].as_u64().unwrap_or(0);
        bq.cmp(&aq)
    });

    // Avg propagation latency (from sites with data)
    let sites_with_timing: Vec<u64> = fields
        .iter()
        .filter(|f| f.last_propagation_us > 0)
        .map(|f| f.last_propagation_us)
        .collect();
    let avg_propagation_us = if sites_with_timing.is_empty() {
        0
    } else {
        sites_with_timing.iter().sum::<u64>() / sites_with_timing.len() as u64
    };

    #[cfg(feature = "persist")]
    let db_info = {
        let (db_fields, db_domains, db_size) = aether_agent::persist::db_stats();
        serde_json::json!({
            "fields_stored": db_fields,
            "domains_stored": db_domains,
            "size_bytes": db_size,
            "persistent": true,
        })
    };
    #[cfg(not(feature = "persist"))]
    let db_info = serde_json::json!({"persistent": false});

    let json = serde_json::json!({
        "sites_profiled": sites_total,
        "sites_in_cache": sites_profiled,
        "total_queries": total_queries,
        "total_nodes_indexed": total_nodes,
        "total_edges": total_edges,
        "causal_weights_learned": total_causal_weights,
        "concepts_memorized": total_concepts,
        "total_feedback": total_feedback,
        "total_successful_nodes": total_successful,
        "total_learned_nodes": total_learned,
        "avg_propagation_us": avg_propagation_us,
        "p95_latency_us": global_p95_us,
        "total_chars_in": total_chars_in,
        "total_chars_out": total_chars_out,
        "token_savings_pct": format!("{:.2}", token_savings_pct),
        "cache_entries": cache_entries,
        "cache_capacity": cache_capacity,
        "total_requests": requests,
        "uptime_secs": uptime_secs,
        "db": db_info,
        "sites": site_list,
    });
    (
        StatusCode::OK,
        [(axum::http::header::CACHE_CONTROL, "no-cache")],
        Json(json),
    )
}

/// Starta bakgrundsloggning av minnesanvändning (var 30:e sekund)
/// + periodisk persist-checkpoint (var 60:e sekund)
fn spawn_memory_monitor(request_counter: Arc<std::sync::atomic::AtomicU64>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(30));
        let mut tick_count: u32 = 0;
        loop {
            interval.tick().await;
            log_rss("periodic");
            tick_count += 1;
            // Persist checkpoint varannan tick (var 60:e sekund)
            #[cfg(feature = "persist")]
            if tick_count.is_multiple_of(2) {
                let reqs = request_counter.load(std::sync::atomic::Ordering::Relaxed);
                aether_agent::persist::save_global_stat("total_requests", reqs);
                aether_agent::persist::checkpoint();
            }
        }
    });
}

#[tokio::main]
async fn main() {
    eprintln!("=== AetherAgent Memory Startup Trace ===");
    log_rss("1. process start");

    // Persistence: init SQLite + restore CRFR state
    #[cfg(feature = "persist")]
    {
        let db_path =
            std::env::var("AETHER_DB_PATH").unwrap_or_else(|_| "/data/aether.db".to_string());
        // Skapa katalog om den inte finns
        if let Some(parent) = std::path::Path::new(&db_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match aether_agent::persist::init(&db_path) {
            Ok(()) => {
                eprintln!("[PERSIST] SQLite initialized at {db_path}");
                let (fields_before, domains_before, size) = aether_agent::persist::db_stats();
                eprintln!(
                    "[PERSIST] DB contains: {fields_before} fields, {domains_before} domains, {:.1} KB",
                    size as f64 / 1024.0
                );
                aether_agent::persist::restore();
                let (ce, _) = aether_agent::resonance::cache_stats();
                eprintln!("[PERSIST] Restored to cache: {ce} fields loaded");
            }
            Err(e) => {
                eprintln!("[PERSIST] WARNING: Failed to init DB: {e} — running in-memory only")
            }
        }
        log_rss("1b. after persistence init");
    }

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

    // Embedding + ColBERT modell-laddning
    //
    // Laddningsordning:
    // 1. Om AETHER_EMBEDDING_MODEL finns → ladda som bi-encoder
    // 2. Om AETHER_COLBERT_MODEL finns → ladda som ColBERT
    // 3. Om bi-encoder INTE laddades → använd ColBERT-modellen som bi-encoder också
    //    (samma ONNX-modell fungerar för båda — bi-encoder gör mean pooling,
    //     ColBERT skippar det och använder per-token embeddings)
    #[cfg(feature = "embeddings")]
    {
        // Vocab (delas av bi-encoder och ColBERT)
        let vocab_path = std::env::var("AETHER_EMBEDDING_VOCAB")
            .unwrap_or_else(|_| "models/vocab.txt".to_string());
        let vocab_text = std::fs::read_to_string(&vocab_path).ok();
        if vocab_text.is_none() {
            eprintln!("[Embedding] WARN: Kan inte läsa vocab '{vocab_path}'");
        }

        // Bi-encoder (FP32, optional)
        let mut _embedding_loaded = false;
        if let Ok(model_path) = std::env::var("AETHER_EMBEDDING_MODEL") {
            if let (Ok(model_bytes), Some(ref vt)) = (std::fs::read(&model_path), &vocab_text) {
                eprintln!(
                    "[Embedding] Laddar bi-encoder: {} ({:.1} MB)",
                    model_path,
                    model_bytes.len() as f64 / 1_048_576.0,
                );
                match aether_agent::embedding::init_global(&model_bytes, vt) {
                    Ok(()) => {
                        eprintln!("[Embedding] Bi-encoder redo");
                        _embedding_loaded = true;
                    }
                    Err(e) => eprintln!("[Embedding] WARN: {e}"),
                }
            } else {
                eprintln!("[Embedding] WARN: Kan inte läsa '{model_path}'");
            }
        }

        // ColBERT (int8, default: models/colbert-small-int8.onnx)
        #[cfg(feature = "colbert")]
        {
            let colbert_model = std::env::var("AETHER_COLBERT_MODEL")
                .unwrap_or_else(|_| "models/colbert-small-int8.onnx".to_string());
            if let (Ok(model_bytes), Some(ref vt)) = (std::fs::read(&colbert_model), &vocab_text) {
                eprintln!(
                    "[ColBERT] Laddar modell: {} ({:.1} MB)",
                    colbert_model,
                    model_bytes.len() as f64 / 1_048_576.0,
                );

                // Ladda som ColBERT
                match aether_agent::embedding::init_colbert(&model_bytes, vt) {
                    Ok(()) => eprintln!("[ColBERT] Modell redo (MaxSim late interaction)"),
                    Err(e) => eprintln!("[ColBERT] WARN: {e}"),
                }

                // Om bi-encoder INTE laddades → använd samma modell som fallback
                if !embedding_loaded {
                    eprintln!("[Embedding] Använder ColBERT-modellen som bi-encoder-fallback");
                    match aether_agent::embedding::init_global(&model_bytes, vt) {
                        Ok(()) => {
                            eprintln!("[Embedding] Bi-encoder redo (via ColBERT-modell)");
                        }
                        Err(e) => eprintln!("[Embedding] WARN: {e}"),
                    }
                }
            } else {
                eprintln!(
                    "[ColBERT] Modell saknas: {} (kör utan ColBERT, Stage 3 = MiniLM)",
                    colbert_model
                );
            }
        }
    }

    log_rss("7. before router build");
    // Starta periodisk minnesmonitor (loggar var 30:e sek till stderr)
    let (mcp_tx, _) = tokio::sync::broadcast::channel::<String>(128);
    let state = AppState {
        vision_model: Arc::new(std::sync::Mutex::new(vision_model)),
        mcp_events: Arc::new(mcp_tx),
        mcp_event_log: Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::with_capacity(100),
        )),
        request_count: Arc::new(std::sync::atomic::AtomicU64::new({
            #[cfg(feature = "persist")]
            {
                aether_agent::persist::load_global_stat("total_requests")
            }
            #[cfg(not(feature = "persist"))]
            {
                0
            }
        })),
        started_at: std::time::Instant::now(),
    };
    log_rss("8. after AppState creation");
    spawn_memory_monitor(state.request_count.clone());

    println!("AetherAgent API server starting on http://{}", addr);
    println!("Endpoints:");
    println!("  GET  /health              – Health check");
    println!("  GET  /                    – API documentation");
    println!("  POST /api/parse           – Parse HTML to semantic tree");
    println!("  POST /api/parse-top       – Parse top-N relevant nodes");
    println!("  POST /api/parse-hybrid    – Hybrid BM25+HDC+Neural pipeline (reranker=colbert recommended)");
    println!("  POST /api/parse-crfr      – CRFR: Causal Resonance Field (BM25+HDC wave, 13x faster, learns)");
    println!("  POST /api/crfr-feedback   – Teach CRFR which nodes had the answer");
    println!("  POST /api/parse-crfr-multi – Multi-goal CRFR (synonyms/translations)");
    println!("  POST /api/crfr-save       – Save CRFR field to JSON");
    println!("  POST /api/crfr-load       – Load CRFR field from JSON");
    println!("  POST /api/crfr-update     – Update node in CRFR field");
    println!("  POST /api/crfr-transfer   – Transfer causal learning across URLs");
    println!("  POST /api/extract-multi   – Extract structured data (multi-key)");
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
    println!("  GET  /ws/stream                  – WebSocket real-time streaming parse");
    println!("  GET  /ws/api                    – Universal WebSocket gateway (all tools)");
    println!("  GET  /ws/mcp                    – MCP JSON-RPC over WebSocket");
    println!("  GET  /ws/search                 – WebSocket streaming search");
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
