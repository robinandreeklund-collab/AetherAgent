/// AetherAgent – LLM-native browser engine
///
/// Publik WASM-API som exponeras till Python, Node.js och edge-runtimes.
mod causal;
mod collab;
mod compiler;
mod diff;
#[cfg(feature = "fetch")]
pub mod fetch;
pub mod firewall;
mod grounding;
mod intent;
pub mod intercept;
mod js_bridge;
mod js_eval;
mod memory;
mod orchestrator;
mod parser;
pub mod search;
mod semantic;
mod session;
mod stream_engine;
mod stream_state;
mod streaming;
mod temporal;
mod trust;
pub mod types;
pub mod vision;
pub mod vision_backend;
mod webmcp;

use std::collections::HashMap;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use wasm_bindgen::prelude::*;

// Global delad TieredBackend så att tier-statistik ackumuleras över anrop
static GLOBAL_TIERED_BACKEND: OnceLock<vision_backend::TieredBackend> = OnceLock::new();

fn global_tiered_backend() -> &'static vision_backend::TieredBackend {
    GLOBAL_TIERED_BACKEND.get_or_init(vision_backend::TieredBackend::default)
}

/// Registrera CDP ready-callback som uppdaterar global TieredBackend.
/// Anropas före warmup_cdp_background() i server-main.
pub fn register_cdp_ready_hook() {
    vision_backend::on_cdp_ready(|| {
        // Force-initiera backend om den inte finns ännu, sedan sätt cdp_available.
        // Vid detta läge har CDP_BROWSER precis satts → default() ser is_some()==true,
        // men vi kör set_cdp_available(true) som säkerhetsnät ändå.
        let backend = global_tiered_backend();
        backend.set_cdp_available(true);
        eprintln!("CDP: global_tiered_backend updated — cdp_available=true");
    });
}

use parser::parse_html;
use semantic::{extract_title, SemanticBuilder};
use types::{SemanticTree, WorkflowMemory};

// ─── Intern hjälpfunktion ────────────────────────────────────────────────────

/// Gemensam parse-pipeline: HTML -> DOM -> SemanticTree
fn build_tree(html: &str, goal: &str, url: &str) -> SemanticTree {
    let dom = parse_html(html);
    let title = extract_title(&dom);
    let mut builder = SemanticBuilder::new(goal);
    let mut tree = builder.build(&dom, url, &title);
    tree.parse_time_ms = 0; // sätts av anroparen
    tree
}

/// Pre-allokerad JSON-serialisering via serde_json::to_writer.
/// Undviker intern String-allokering genom att skriva direkt till Vec<u8>.
/// Estimerar buffert-storlek baserat på antal noder (~200 bytes/nod).
fn serialize_json<T: serde::Serialize>(
    value: &T,
    estimated_nodes: usize,
) -> Result<String, String> {
    let capacity = (estimated_nodes * 200).max(1024);
    let mut buf = Vec::with_capacity(capacity);
    serde_json::to_writer(&mut buf, value)
        .map_err(|e| format!(r#"{{"error": "Serialization failed: {e}"}}"#))?;
    // SAFETY: serde_json::to_writer producerar alltid giltig UTF-8
    Ok(unsafe { String::from_utf8_unchecked(buf) })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn collect_all_nodes(nodes: &[types::SemanticNode]) -> Vec<&types::SemanticNode> {
    let mut result = vec![];
    for node in nodes {
        result.push(node);
        result.extend(collect_all_nodes(&node.children));
    }
    result
}

// ─── Fas 1: Publik API ──────────────────────────────────────────────────────

/// Parsa HTML till ett semantiskt träd med goal-relevance scoring
///
/// # Arguments
/// * `html` - Raw HTML string from the web page
/// * `goal` - The agent's current goal (e.g. "buy cheapest flight")
/// * `url` - The page URL (for context)
///
/// # Returns
/// JSON string with SemanticTree, ready to send to the LLM
#[wasm_bindgen]
pub fn parse_to_semantic_tree(html: &str, goal: &str, url: &str) -> String {
    let start = now_ms();
    let mut tree = build_tree(html, goal, url);
    tree.parse_time_ms = now_ms() - start;

    // Sortera noder efter relevance (högst först) för LLM-effektivitet
    tree.nodes
        .sort_by(|a, b| b.relevance.total_cmp(&a.relevance));

    match serialize_json(&tree, tree.nodes.len()) {
        Ok(json) => json,
        Err(e) => e,
    }
}

/// Snabbversion – returnerar bara de mest relevanta noderna
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's goal
/// * `url` - The page URL
/// * `top_n` - Max number of nodes to return (recommended: 10-20)
#[wasm_bindgen]
pub fn parse_top_nodes(html: &str, goal: &str, url: &str, top_n: u32) -> String {
    let start = now_ms();
    let mut tree = build_tree(html, goal, url);
    tree.parse_time_ms = now_ms() - start;

    // Samla alla noder platt och sortera
    let mut all_nodes = collect_all_nodes(&tree.nodes);
    all_nodes.sort_by(|a, b| b.relevance.total_cmp(&a.relevance));

    // Ta topp-N
    let top: Vec<_> = all_nodes
        .into_iter()
        .take(top_n as usize)
        .cloned()
        .collect();

    let result = serde_json::json!({
        "url": tree.url,
        "title": tree.title,
        "goal": tree.goal,
        "top_nodes": top,
        "injection_warnings": tree.injection_warnings.len(),
        "parse_time_ms": tree.parse_time_ms,
    });

    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Parse HTML with streaming and early-stopping
///
/// Builds semantic nodes incrementally and stops when max_nodes is reached.
/// More memory-efficient than full parse for large pages (1000+ elements).
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's current goal
/// * `url` - The page URL
/// * `max_nodes` - Maximum number of semantic nodes to build (0 = default 300)
#[wasm_bindgen]
pub fn parse_streaming(html: &str, goal: &str, url: &str, max_nodes: u32) -> String {
    let start = now_ms();
    let limit = if max_nodes == 0 {
        300
    } else {
        max_nodes as usize
    };
    let mut tree = streaming::stream_parse_limited(html, goal, url, limit);
    tree.parse_time_ms = now_ms() - start;

    tree.nodes
        .sort_by(|a, b| b.relevance.total_cmp(&a.relevance));

    match serialize_json(&tree, tree.nodes.len()) {
        Ok(json) => json,
        Err(e) => e,
    }
}

// ─── Fas 16: Goal-Driven Adaptive DOM Streaming ─────────────────────────────

/// Adaptive goal-driven DOM streaming – emits only the most relevant nodes
/// in ranked chunks, with support for LLM-directed expansion.
///
/// Returns JSON with `nodes`, `total_dom_nodes`, `nodes_emitted`,
/// `token_savings_ratio`, `parse_ms`, `injection_warnings`, and `chunks`.
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's current goal for relevance scoring
/// * `url` - The page URL
/// * `top_n` - Nodes per chunk (default: 10)
/// * `min_relevance` - Minimum relevance for emission (default: 0.3)
/// * `max_nodes` - Hard limit on total emitted nodes (default: 50)
#[wasm_bindgen]
pub fn stream_parse_adaptive(
    html: &str,
    goal: &str,
    url: &str,
    top_n: u32,
    min_relevance: f32,
    max_nodes: u32,
) -> String {
    let config = stream_engine::StreamParseConfig {
        chunk_size: if top_n == 0 { 10 } else { top_n as usize },
        min_relevance: if min_relevance <= 0.0 {
            0.3
        } else {
            min_relevance
        },
        max_nodes: if max_nodes == 0 {
            50
        } else {
            max_nodes as usize
        },
    };
    let result = stream_engine::stream_parse(html, goal, url, config);
    match serialize_json(&result, result.nodes.len()) {
        Ok(json) => json,
        Err(e) => e,
    }
}

/// Adaptive stream parse with pre-loaded directives (expand, stop, etc.)
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's current goal
/// * `url` - The page URL
/// * `config_json` - JSON config: `{"top_n": 10, "min_relevance": 0.3, "max_nodes": 50}`
/// * `directives_json` - JSON array of directives: `[{"action": "expand", "node_id": 56}]`
#[wasm_bindgen]
pub fn stream_parse_with_directives(
    html: &str,
    goal: &str,
    url: &str,
    config_json: &str,
    directives_json: &str,
) -> String {
    #[derive(serde::Deserialize)]
    struct ConfigInput {
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

    let cfg: ConfigInput = serde_json::from_str(config_json).unwrap_or(ConfigInput {
        top_n: 10,
        min_relevance: 0.3,
        max_nodes: 50,
    });

    let directives: Vec<stream_state::Directive> =
        serde_json::from_str(directives_json).unwrap_or_default();

    let config = stream_engine::StreamParseConfig {
        chunk_size: cfg.top_n,
        min_relevance: cfg.min_relevance,
        max_nodes: cfg.max_nodes,
    };

    let result = stream_engine::stream_parse_with_directives(html, goal, url, config, directives);
    match serialize_json(&result, result.nodes.len()) {
        Ok(json) => json,
        Err(e) => e,
    }
}

/// Analysera ett textstycke för prompt injection
#[wasm_bindgen]
pub fn check_injection(text: &str) -> String {
    let (_, warning) = trust::analyze_text(0, text);
    if let Some(w) = warning {
        serde_json::to_string_pretty(&w).unwrap_or_else(|_| "{}".to_string())
    } else {
        r#"{"safe": true}"#.to_string()
    }
}

/// Wrappa text i content-boundary markers för säker LLM-konsumption
#[wasm_bindgen]
pub fn wrap_untrusted(content: &str) -> String {
    trust::wrap_untrusted(content)
}

/// Sanitetskontroll – verifiera att WASM-modulen laddats korrekt
#[wasm_bindgen]
pub fn health_check() -> String {
    r#"{"status": "ok", "version": "0.2.0", "engine": "AetherAgent"}"#.to_string()
}

// ─── Fas 2: Intent API ──────────────────────────────────────────────────────

/// Hitta det bäst matchande klickbara elementet
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's current goal
/// * `url` - The page URL
/// * `target_label` - What to click (e.g. "Add to cart", "Log in")
///
/// # Returns
/// JSON with ClickResult: found, node_id, role, label, selector_hint, relevance
#[wasm_bindgen]
pub fn find_and_click(html: &str, goal: &str, url: &str, target_label: &str) -> String {
    let start = now_ms();
    let mut tree = build_tree(html, goal, url);
    tree.parse_time_ms = now_ms() - start;

    let result = intent::find_best_clickable(&tree, target_label);
    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Matcha formulärfält med angivna nycklar och värden
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's current goal
/// * `url` - The page URL
/// * `fields_json` - JSON object: {"email": "user@test.com", "password": "secret"}
///
/// # Returns
/// JSON with FillFormResult: mappings, unmapped_keys, unmapped_fields
#[wasm_bindgen]
pub fn fill_form(html: &str, goal: &str, url: &str, fields_json: &str) -> String {
    let start = now_ms();
    let mut tree = build_tree(html, goal, url);
    tree.parse_time_ms = now_ms() - start;

    let fields: HashMap<String, String> = match serde_json::from_str(fields_json) {
        Ok(f) => f,
        Err(e) => {
            return format!(r#"{{"error": "Invalid fields_json: {}"}}"#, e);
        }
    };

    let result = intent::map_form_fields(&tree, &fields);
    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Extrahera strukturerad data från en sida baserat på nycklar
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's current goal
/// * `url` - The page URL
/// * `data_keys_json` - JSON array: ["price", "title", "description"]
///
/// # Returns
/// JSON with ExtractDataResult: entries, missing_keys
#[wasm_bindgen]
pub fn extract_data(html: &str, goal: &str, url: &str, data_keys_json: &str) -> String {
    let start = now_ms();
    let mut tree = build_tree(html, goal, url);
    tree.parse_time_ms = now_ms() - start;

    let keys: Vec<String> = match serde_json::from_str(data_keys_json) {
        Ok(k) => k,
        Err(e) => {
            return format!(r#"{{"error": "Invalid data_keys_json: {}"}}"#, e);
        }
    };

    let result = intent::extract_by_keys(&tree, &keys);
    serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".to_string())
}

// ─── Fas 2: Workflow Memory ──────────────────────────────────────────────────

/// Skapa ett nytt tomt workflow-minne
///
/// # Returns
/// JSON string with empty WorkflowMemory
#[wasm_bindgen]
pub fn create_workflow_memory() -> String {
    WorkflowMemory::new().to_json()
}

/// Lägg till ett steg i workflow-minnet
///
/// # Arguments
/// * `memory_json` - Existing workflow memory (from create_workflow_memory or previous call)
/// * `action` - Action type: "click", "fill_form", "extract_data", "parse"
/// * `url` - The URL where the action took place
/// * `goal` - The agent's goal for this step
/// * `summary` - Short description of what happened
///
/// # Returns
/// Updated workflow memory JSON
#[wasm_bindgen]
pub fn add_workflow_step(
    memory_json: &str,
    action: &str,
    url: &str,
    goal: &str,
    summary: &str,
) -> String {
    let mut mem = match WorkflowMemory::from_json(memory_json) {
        Ok(m) => m,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    mem.add_step(action, url, goal, summary, now_ms());
    mem.to_json()
}

/// Spara ett nyckel-värde-par i workflow-kontexten
///
/// # Returns
/// Updated workflow memory JSON
#[wasm_bindgen]
pub fn set_workflow_context(memory_json: &str, key: &str, value: &str) -> String {
    let mut mem = match WorkflowMemory::from_json(memory_json) {
        Ok(m) => m,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };
    mem.set_context(key, value);
    mem.to_json()
}

/// Hämta ett värde från workflow-kontexten
///
/// # Returns
/// JSON: {"value": "..."} or {"value": null}
#[wasm_bindgen]
pub fn get_workflow_context(memory_json: &str, key: &str) -> String {
    let mem = match WorkflowMemory::from_json(memory_json) {
        Ok(m) => m,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };
    match mem.get_context(key) {
        Some(v) => format!(r#"{{"value": "{}"}}"#, v.replace('"', "\\\"")),
        None => r#"{"value": null}"#.to_string(),
    }
}

// ─── Fas 4a: Semantic Diff ───────────────────────────────────────────────────

/// Compare two semantic trees and return only the changes (delta)
///
/// This dramatically reduces token usage for multi-step agent flows.
/// Instead of sending the full tree after every action, the agent sends
/// the initial tree once and then only the delta for subsequent steps.
///
/// # Arguments
/// * `old_tree_json` - Previous SemanticTree JSON (from parse_to_semantic_tree)
/// * `new_tree_json` - Current SemanticTree JSON (from parse_to_semantic_tree)
///
/// # Returns
/// JSON with SemanticDelta: changes, token_savings_ratio, summary
#[wasm_bindgen]
pub fn diff_semantic_trees(old_tree_json: &str, new_tree_json: &str) -> String {
    let start = now_ms();

    let old_tree: SemanticTree = match serde_json::from_str(old_tree_json) {
        Ok(t) => t,
        Err(e) => return format!(r#"{{"error": "Invalid old_tree_json: {}"}}"#, e),
    };

    let new_tree: SemanticTree = match serde_json::from_str(new_tree_json) {
        Ok(t) => t,
        Err(e) => return format!(r#"{{"error": "Invalid new_tree_json: {}"}}"#, e),
    };

    let mut delta = diff::diff_trees(&old_tree, &new_tree);
    delta.diff_time_ms = now_ms() - start;

    match serde_json::to_string_pretty(&delta) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 4b: JS Sandbox ─────────────────────────────────────────────────────

/// Detect JavaScript snippets in HTML that may affect page content
///
/// Scans for inline scripts, event handlers, and framework markers.
/// Use this to determine if a page needs JS evaluation for complete parsing.
///
/// # Returns
/// JSON with JsDetectionResult: snippets, has_framework, framework_hint
#[wasm_bindgen]
pub fn detect_js(html: &str) -> String {
    let result = js_eval::detect_js_snippets(html);
    match serde_json::to_string_pretty(&result) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Evaluate a JavaScript expression in a sandboxed environment
///
/// Supports: math, strings, arrays, objects, ternary, template literals.
/// Blocks: DOM access, fetch, timers, eval, import, require.
///
/// # Arguments
/// * `code` - JavaScript expression to evaluate
///
/// # Returns
/// JSON with JsEvalResult: value, error, timed_out, eval_time_us
#[wasm_bindgen]
pub fn eval_js(code: &str) -> String {
    let result = js_eval::eval_js(code);
    match serde_json::to_string_pretty(&result) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Evaluate multiple JavaScript expressions in sequence
///
/// # Arguments
/// * `snippets_json` - JSON array of code strings: ["1+1", "'a'+'b'"]
///
/// # Returns
/// JSON with JsBatchResult: results[], total_eval_time_us
#[wasm_bindgen]
pub fn eval_js_batch(snippets_json: &str) -> String {
    let snippets: Vec<String> = match serde_json::from_str(snippets_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "Invalid snippets_json: {}"}}"#, e),
    };

    let result = js_eval::eval_js_batch(&snippets);
    match serde_json::to_string_pretty(&result) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 4c: Selective JS Execution ──────────────────────────────────────────

/// Parse HTML with automatic JS evaluation for dynamic content
///
/// Combines the full pipeline: HTML parsing → JS detection → sandbox eval →
/// enhanced semantic tree. Use this instead of parse_to_semantic_tree when
/// you suspect the page has JavaScript-computed content (prices, totals, etc.)
///
/// # Arguments
/// * `html` - Raw HTML string (including inline scripts)
/// * `goal` - The agent's current goal
/// * `url` - The page URL
///
/// # Returns
/// JSON with SelectiveExecResult: enhanced tree, JS bindings, analysis
#[wasm_bindgen]
pub fn parse_with_js(html: &str, goal: &str, url: &str) -> String {
    let start = now_ms();
    let tree = build_tree(html, goal, url);

    let mut result = js_bridge::selective_exec(&tree, html);
    result.exec_time_ms = now_ms() - start;
    result.tree.parse_time_ms = result.exec_time_ms;

    match serde_json::to_string_pretty(&result) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 10: XHR Network Interception ────────────────────────────────────────

/// Detect fetch()/XHR URLs in HTML page's JavaScript
///
/// Scans inline scripts and event handlers for network calls.
/// Returns JSON array of XhrCapture objects.
///
/// # Arguments
/// * `html` - HTML source to scan
///
/// # Returns
/// JSON string with array of `{url, method, headers}` objects
#[wasm_bindgen]
pub fn detect_xhr_urls(html: &str) -> String {
    let captures = js_bridge::extract_xhr_from_snippets(html);
    match serde_json::to_string_pretty(&captures) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 5: Temporal Memory & Adversarial Modeling ──────────────────────────

/// Create a new empty temporal memory
///
/// # Returns
/// JSON string with empty TemporalMemory
#[wasm_bindgen]
pub fn create_temporal_memory() -> String {
    temporal::TemporalMemory::new().to_json()
}

/// Add a snapshot to temporal memory (track page state over time)
///
/// # Arguments
/// * `memory_json` - Existing temporal memory JSON
/// * `html` - Raw HTML of the current page
/// * `goal` - The agent's current goal
/// * `url` - The page URL
/// * `timestamp_ms` - Current timestamp in milliseconds
///
/// # Returns
/// Updated temporal memory JSON
#[wasm_bindgen]
pub fn add_temporal_snapshot(
    memory_json: &str,
    html: &str,
    goal: &str,
    url: &str,
    timestamp_ms: u64,
) -> String {
    let mut mem = match temporal::TemporalMemory::from_json(memory_json) {
        Ok(m) => m,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let tree = build_tree(html, goal, url);
    let tree_json = match serialize_json(&tree, tree.nodes.len()) {
        Ok(j) => j,
        Err(e) => return e,
    };

    mem.add_snapshot(&tree, &tree_json, timestamp_ms);
    mem.to_json()
}

/// Analyze temporal memory for adversarial patterns and volatility
///
/// # Arguments
/// * `memory_json` - Temporal memory JSON with at least 1 snapshot
///
/// # Returns
/// JSON with TemporalAnalysis: snapshots, volatility, adversarial patterns, risk score
#[wasm_bindgen]
pub fn analyze_temporal(memory_json: &str) -> String {
    let start = now_ms();

    let mem = match temporal::TemporalMemory::from_json(memory_json) {
        Ok(m) => m,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let mut analysis = mem.analyze();
    analysis.analysis_time_ms = now_ms() - start;

    match serde_json::to_string_pretty(&analysis) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Predict next page state based on temporal history
///
/// # Arguments
/// * `memory_json` - Temporal memory JSON
///
/// # Returns
/// JSON with PredictedState: expected_node_count, expected_warning_count, likely_changed_nodes
#[wasm_bindgen]
pub fn predict_temporal(memory_json: &str) -> String {
    let mem = match temporal::TemporalMemory::from_json(memory_json) {
        Ok(m) => m,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let prediction = temporal::predict_next_state(&mem);
    match serde_json::to_string_pretty(&prediction) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 6: Intent Compiler ─────────────────────────────────────────────────

/// Compile a goal into an optimized action plan
///
/// Decomposes complex goals into sub-goals with dependencies,
/// computes execution order with parallel groups, and estimates cost.
///
/// # Arguments
/// * `goal` - The agent's goal (e.g. "buy iPhone 16 Pro", "logga in")
///
/// # Returns
/// JSON with ActionPlan: sub_goals, execution_order, estimated_cost
#[wasm_bindgen]
pub fn compile_goal(goal: &str) -> String {
    let start = now_ms();
    let mut plan = compiler::compile_goal(goal);
    plan.compile_time_ms = now_ms() - start;

    match serde_json::to_string_pretty(&plan) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Execute an action plan against current page state
///
/// Determines which steps are ready, recommends the next action,
/// and generates prefetch suggestions.
///
/// # Arguments
/// * `plan_json` - ActionPlan JSON (from compile_goal)
/// * `html` - Current page HTML
/// * `goal` - The agent's goal
/// * `url` - Current page URL
/// * `completed_steps_json` - JSON array of completed step indices: [0, 1]
///
/// # Returns
/// JSON with PlanExecutionResult: next_action, prefetch_suggestions, summary
#[wasm_bindgen]
pub fn execute_plan(
    plan_json: &str,
    html: &str,
    goal: &str,
    url: &str,
    completed_steps_json: &str,
) -> String {
    let plan: compiler::ActionPlan = match serde_json::from_str(plan_json) {
        Ok(p) => p,
        Err(e) => return format!(r#"{{"error": "Invalid plan_json: {}"}}"#, e),
    };

    let completed: Vec<u32> = match serde_json::from_str(completed_steps_json) {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "Invalid completed_steps_json: {}"}}"#, e),
    };

    let tree = build_tree(html, goal, url);
    let result = compiler::execute_plan(&plan, &tree, &completed);

    match serde_json::to_string_pretty(&result) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 8: Semantic Firewall ────────────────────────────────────────────────

/// Klassificera en URL mot firewallens tre nivåer (L1/L2/L3)
#[wasm_bindgen]
pub fn classify_request(url: &str, goal: &str, config_json: &str) -> String {
    let config: firewall::FirewallConfig = serde_json::from_str(config_json).unwrap_or_default();
    let verdict = firewall::classify_request(url, goal, &config);
    serde_json::to_string(&verdict)
        .unwrap_or_else(|e| format!(r#"{{"error": "Serialization failed: {e}"}}"#))
}

/// Klassificera en batch av URLs mot firewallen
#[wasm_bindgen]
pub fn classify_request_batch(urls_json: &str, goal: &str, config_json: &str) -> String {
    let urls: Vec<String> = match serde_json::from_str(urls_json) {
        Ok(u) => u,
        Err(e) => return format!(r#"{{"error": "Invalid urls_json: {e}"}}"#),
    };
    let config: firewall::FirewallConfig = serde_json::from_str(config_json).unwrap_or_default();
    let (verdicts, summary) = firewall::classify_batch(&urls, goal, &config);
    let result = serde_json::json!({
        "verdicts": verdicts,
        "summary": summary,
    });
    serde_json::to_string(&result)
        .unwrap_or_else(|e| format!(r#"{{"error": "Serialization failed: {e}"}}"#))
}

// ─── Fas 9a: Causal Action Graph ─────────────────────────────────────────────

/// Build a causal graph from navigation history
///
/// Models page state transitions as a directed graph.
/// The agent can reason about action consequences before executing them.
///
/// # Arguments
/// * `snapshots_json` - JSON array of snapshot objects: `[{"url": "...", "node_count": 5, "warning_count": 0, "key_elements": ["button:Buy"]}]`
/// * `actions_json` - JSON array of action strings between snapshots
///
/// # Returns
/// JSON with CausalGraph: states, edges, current_state_id
#[wasm_bindgen]
pub fn build_causal_graph(snapshots_json: &str, actions_json: &str) -> String {
    let snapshots: Vec<causal::CausalSnapshotInput> = match serde_json::from_str(snapshots_json) {
        Ok(s) => s,
        Err(e) => {
            return format!(
                r#"{{"error": "Invalid snapshots_json: {}. Expected format: [{{\"url\": \"...\", \"node_count\": 5, \"warning_count\": 0, \"key_elements\": [\"button:Buy\"]}}]"}}"#,
                e
            );
        }
    };

    // BUG-011 fix: Acceptera både array och objekt (extrahera values som strängar)
    let actions: Vec<String> = match serde_json::from_str::<serde_json::Value>(actions_json) {
        Ok(serde_json::Value::Array(arr)) => arr
            .into_iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .collect(),
        Ok(serde_json::Value::Object(obj)) => obj
            .into_iter()
            .map(|(_, v)| match v {
                serde_json::Value::String(s) => s,
                other => other.to_string(),
            })
            .collect(),
        Ok(serde_json::Value::String(s)) => vec![s],
        Ok(_) => {
            return r#"{"error": "Invalid actions_json: expected array or object"}"#.to_string();
        }
        Err(e) => return format!(r#"{{"error": "Invalid actions_json: {}"}}"#, e),
    };

    let graph = causal::CausalGraph::build_from_history(&snapshots, &actions);
    graph.to_json()
}

/// Predict the outcome of an action from current state
///
/// # Arguments
/// * `graph_json` - CausalGraph JSON
/// * `action` - Action to predict (e.g. "click: Buy")
///
/// # Returns
/// JSON with PredictedOutcome: outcomes, aggregate_risk, recommendation
#[wasm_bindgen]
pub fn predict_action_outcome(graph_json: &str, action: &str) -> String {
    let graph = match causal::CausalGraph::from_json(graph_json) {
        Ok(g) => g,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let prediction = graph.predict_outcome(action, None);
    match serde_json::to_string_pretty(&prediction) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Find the safest path to a goal in the causal graph
///
/// # Arguments
/// * `graph_json` - CausalGraph JSON
/// * `goal` - Target goal description
///
/// # Returns
/// JSON with SafePath: path, actions, total_risk, success_probability
#[wasm_bindgen]
pub fn find_safest_path(graph_json: &str, goal: &str) -> String {
    let graph = match causal::CausalGraph::from_json(graph_json) {
        Ok(g) => g,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let path = graph.find_safest_path(goal, 20);
    match serde_json::to_string_pretty(&path) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 9b: WebMCP Discovery ───────────────────────────────────────────────

/// Discover WebMCP tools registered on a web page
///
/// Scans inline scripts for navigator.modelContext.registerTool() calls
/// and extracts tool definitions (name, description, inputSchema).
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `url` - The page URL
///
/// # Returns
/// JSON with WebMcpDiscoveryResult: tools, has_webmcp, scripts_scanned
#[wasm_bindgen]
pub fn discover_webmcp(html: &str, url: &str) -> String {
    let start = now_ms();
    let mut result = webmcp::discover_webmcp_tools(html, url);
    result.scan_time_ms = now_ms() - start;

    match serde_json::to_string_pretty(&result) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 9c: Multimodal Grounding ───────────────────────────────────────────

/// Ground a semantic tree with bounding box annotations
///
/// Enriches SemanticNode with spatial data from getBoundingClientRect()
/// or vision model predictions. Generates Set-of-Mark annotations.
///
/// # Arguments
/// * `html` - Raw HTML string
/// * `goal` - The agent's goal
/// * `url` - The page URL
/// * `annotations_json` - JSON array of BboxAnnotation objects
///
/// # Returns
/// JSON with GroundingResult: grounded tree, matched/unmatched counts, set_of_marks
#[wasm_bindgen]
pub fn ground_semantic_tree(html: &str, goal: &str, url: &str, annotations_json: &str) -> String {
    let start = now_ms();
    let tree = build_tree(html, goal, url);

    let annotations: Vec<grounding::BboxAnnotation> = match serde_json::from_str(annotations_json) {
        Ok(a) => a,
        Err(e) => return format!(r#"{{"error": "Invalid annotations_json: {}"}}"#, e),
    };

    let mut result = grounding::ground_tree(&tree, &annotations);
    result.grounding_time_ms = now_ms() - start;

    match serde_json::to_string_pretty(&result) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Match a predicted bounding box against grounded nodes using IoU
///
/// # Arguments
/// * `tree_json` - SemanticTree JSON (with bbox data from ground_semantic_tree)
/// * `bbox_json` - Predicted BoundingBox JSON: {"x": 100, "y": 200, "width": 80, "height": 30}
///
/// # Returns
/// JSON array of IoUMatch objects sorted by IoU (highest first)
#[wasm_bindgen]
pub fn match_bbox_iou(tree_json: &str, bbox_json: &str) -> String {
    let tree: types::SemanticTree = match serde_json::from_str(tree_json) {
        Ok(t) => t,
        Err(e) => return format!(r#"{{"error": "Invalid tree_json: {}"}}"#, e),
    };

    let bbox: types::BoundingBox = match serde_json::from_str(bbox_json) {
        Ok(b) => b,
        Err(e) => return format!(r#"{{"error": "Invalid bbox_json: {}"}}"#, e),
    };

    let matches = grounding::match_by_iou(&tree.nodes, &bbox);
    match serde_json::to_string_pretty(&matches) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 11: Vision – YOLOv8 inference ──────────────────────────────────────

/// Parse a screenshot into a semantic tree using YOLOv8-nano vision model
///
/// Requires the `vision` feature flag. Input: PNG bytes + ONNX model bytes + goal string.
#[wasm_bindgen]
pub fn parse_screenshot(png_bytes: &[u8], model_bytes: &[u8], goal: &str) -> String {
    let config = vision::VisionConfig::default();
    match vision::detect_ui_elements(png_bytes, model_bytes, goal, &config) {
        Ok(result) => serde_json::to_string(&result)
            .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}).to_string()),
        Err(e) => serde_json::json!({"error": e}).to_string(),
    }
}

/// Parse a screenshot with a pre-loaded ORT session (fast path — no model reload).
///
/// Use `load_vision_model` to load the model once, then pass it here for each request.
// [RTEN-ROLLBACK-ID:lib-parse-model] Gamla: pub fn parse_screenshot_with_model(... model: &rten::Model ...)
#[cfg(feature = "vision")]
pub fn parse_screenshot_with_model(
    png_bytes: &[u8],
    session: &mut ort::session::Session,
    goal: &str,
) -> String {
    let config = vision::VisionConfig::default();
    match vision::detect_ui_elements_with_model(png_bytes, session, goal, &config) {
        Ok(result) => serde_json::to_string(&result)
            .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}).to_string()),
        Err(e) => serde_json::json!({"error": e}).to_string(),
    }
}

/// Load a vision model from ONNX bytes into an ORT session.
/// Call once at startup, reuse for all requests.
// [RTEN-ROLLBACK-ID:lib-load-model] Gamla: pub fn load_vision_model(... ) -> Result<rten::Model, String>
#[cfg(feature = "vision")]
pub fn load_vision_model(model_bytes: &[u8]) -> Result<ort::session::Session, String> {
    vision::load_model(model_bytes)
}

// ─── Blitz rendering (testbar via library) ──────────────────────────────────

/// Render HTML to PNG using Blitz (pure Rust).
///
/// `fast_render=true`: skip external resources (~50ms). Good enough for YOLO.
/// `fast_render=false`: load CSS/fonts/images with 5s timeout cap.
///
/// Max resolution: 4096×8192 (134 MB buffer). Larger values are clamped.
#[cfg(feature = "blitz")]
pub fn render_html_to_png(
    html: &str,
    base_url: &str,
    width: u32,
    height: u32,
    fast_render: bool,
) -> Result<Vec<u8>, String> {
    // Säkerhetsgräns: förhindra OOM vid orimliga dimensioner
    const MAX_WIDTH: u32 = 4096;
    const MAX_HEIGHT: u32 = 8192;
    const MAX_PIXELS: u64 = 4096 * 8192; // ~134 MB RGBA-buffer
    let width = width.min(MAX_WIDTH);
    let height = height.min(MAX_HEIGHT);
    let total_pixels = width as u64 * height as u64;
    if total_pixels > MAX_PIXELS {
        return Err(format!(
            "Screenshot för stor: {width}×{height} = {total_pixels} pixlar (max {MAX_PIXELS})"
        ));
    }
    use anyrender::{ImageRenderer, PaintScene as _};
    use blitz_dom::DocumentConfig;
    use blitz_html::HtmlDocument;
    use blitz_traits::shell::{ColorScheme, Viewport};

    let scale: f32 = 1.0;

    let mut document = if fast_render {
        HtmlDocument::from_html(
            html,
            DocumentConfig {
                viewport: Some(Viewport::new(width, height, scale, ColorScheme::Light)),
                base_url: Some(base_url.to_string()),
                ..Default::default()
            },
        )
    } else {
        let (mut rx, callback) = blitz_net::MpscCallback::<blitz_dom::net::Resource>::new();
        let callback: std::sync::Arc<dyn blitz_traits::net::NetCallback<blitz_dom::net::Resource>> =
            std::sync::Arc::new(callback);
        let net = std::sync::Arc::new(blitz_net::Provider::new(callback));

        let mut doc = HtmlDocument::from_html(
            html,
            DocumentConfig {
                viewport: Some(Viewport::new(width, height, scale, ColorScheme::Light)),
                base_url: Some(base_url.to_string()),
                net_provider: Some(std::sync::Arc::clone(&net)
                    as std::sync::Arc<
                        dyn blitz_traits::net::NetProvider<blitz_dom::net::Resource>,
                    >),
                ..Default::default()
            },
        );

        // Vänta kort så att Blitz hinner starta alla resurshämtningar
        std::thread::sleep(std::time::Duration::from_millis(50));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut idle_rounds = 0u32;
        loop {
            let mut loaded_any = false;
            while let Ok((_doc_id, resource)) = rx.try_recv() {
                doc.as_mut().load_resource(resource);
                loaded_any = true;
            }
            doc.as_mut().resolve(0.0);

            // Kräv minst 3 tomma rundor i rad innan vi avslutar
            if net.is_empty() && !loaded_any {
                idle_rounds += 1;
                if idle_rounds >= 3 {
                    break;
                }
            } else {
                idle_rounds = 0;
            }

            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        while let Ok((_doc_id, resource)) = rx.try_recv() {
            doc.as_mut().load_resource(resource);
        }
        doc.as_mut().resolve(0.0);
        doc
    };

    if fast_render {
        document.as_mut().resolve(0.0);
    }

    let white = peniko::Color::new([1.0, 1.0, 1.0, 1.0]);
    let mut renderer = anyrender_vello_cpu::VelloCpuImageRenderer::new(width, height);
    let mut buffer = Vec::with_capacity((width * height * 4) as usize);
    renderer.render_to_vec(
        |scene| {
            scene.fill(
                peniko::Fill::NonZero,
                peniko::kurbo::Affine::IDENTITY,
                white,
                None,
                &peniko::kurbo::Rect::new(0.0, 0.0, width as f64, height as f64),
            );
            blitz_paint::paint_scene(scene, document.as_ref(), scale as f64, width, height);
        },
        &mut buffer,
    );

    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| format!("PNG header: {e}"))?;
        writer
            .write_image_data(&buffer)
            .map_err(|e| format!("PNG data: {e}"))?;
        writer.finish().map_err(|e| format!("PNG finish: {e}"))?;
    }

    Ok(png_bytes)
}

// ─── Fas 12: TieredBackend – Blitz/CDP tier-val ─────────────────────────────

/// Screenshot with intelligent tier selection (Blitz → CDP fallback)
///
/// Uses TieredBackend to render a page:
/// - Tier 1 (Blitz): Pure Rust, ~10-50ms, no Chrome needed
/// - Tier 2 (CDP): Chrome DevTools Protocol, ~60-80ms, JS-capable
///
/// # Arguments
/// * `html` - Page HTML
/// * `url` - Page URL
/// * `goal` - Agent's goal
/// * `width` - Viewport width (default 1280)
/// * `height` - Viewport height (default 800)
/// * `fast_render` - Skip external resources (default true)
/// * `xhr_captures_json` - Optional: XHR captures JSON for tier hint
///
/// # Returns
/// JSON with ScreenshotResult: tier_used, latency_ms, size_bytes
pub fn tiered_screenshot(
    html: &str,
    url: &str,
    goal: &str,
    width: u32,
    height: u32,
    fast_render: bool,
    xhr_captures_json: &str,
) -> String {
    let backend = global_tiered_backend();

    // Bestäm tier-hint från URL + XHR-captures + HTML
    let tier_hint = if xhr_captures_json.is_empty() || xhr_captures_json == "[]" {
        // Kolla URL + HTML direkt (BUG-2 fix: URL-heuristiker + script-src-analys)
        vision_backend::determine_tier_hint_with_url(html, &[], url)
    } else {
        // Parsa XHR-captures och analysera
        let captures: Vec<intercept::XhrCapture> =
            serde_json::from_str(xhr_captures_json).unwrap_or_default();
        let hint = intercept::tier_hint_from_captures(&captures);
        if matches!(hint, vision_backend::TierHint::TryBlitzFirst) {
            // Fallback: analysera URL + HTML
            vision_backend::determine_tier_hint_with_url(html, &[], url)
        } else {
            hint
        }
    };

    let req = vision_backend::ScreenshotRequest {
        url: url.to_string(),
        html: Some(html.to_string()),
        width,
        height,
        fast_render,
        tier_hint,
        goal: goal.to_string(),
    };

    match backend.screenshot(&req) {
        Ok(result) => {
            // Returnera metadata (inte PNG-bytes — de hanteras separat)
            let stats = backend.stats();
            serde_json::json!({
                "tier_used": result.tier_used,
                "latency_ms": result.latency_ms,
                "size_bytes": result.size_bytes,
                "width": result.width,
                "height": result.height,
                "escalation_reason": result.escalation_reason,
                "stats": stats,
            })
            .to_string()
        }
        Err(e) => serde_json::json!({"error": e}).to_string(),
    }
}

/// Take a screenshot using TieredBackend with tier-hint analysis.
///
/// Returns (png_bytes, tier_used) — use this when you need the actual PNG bytes
/// (unlike `tiered_screenshot` which returns metadata JSON only).
pub fn screenshot_with_tier(
    html: &str,
    url: &str,
    width: u32,
    height: u32,
    fast_render: bool,
) -> Result<(Vec<u8>, vision_backend::ScreenshotTier), String> {
    let backend = global_tiered_backend();

    let tier_hint = vision_backend::determine_tier_hint_with_url(html, &[], url);

    let req = vision_backend::ScreenshotRequest {
        url: url.to_string(),
        html: Some(html.to_string()),
        width,
        height,
        fast_render,
        tier_hint,
        goal: String::new(),
    };

    let result = backend.screenshot(&req)?;
    Ok((result.png_bytes, result.tier_used))
}

/// Get tier statistics for monitoring
///
/// Returns JSON with TierStats: blitz_count, cdp_count, escalation_count, etc.
pub fn tier_stats() -> String {
    let stats = global_tiered_backend().stats();
    serde_json::to_string_pretty(&stats).unwrap_or_else(|_| "{}".to_string())
}

// ─── Fas 9d: Cross-Agent Semantic Diffing ───────────────────────────────────

/// Create a new shared diff store for cross-agent collaboration
///
/// # Returns
/// JSON with empty SharedDiffStore
#[wasm_bindgen]
pub fn create_collab_store() -> String {
    collab::SharedDiffStore::new().to_json()
}

/// Register an agent in the collaboration store
///
/// # Arguments
/// * `store_json` - SharedDiffStore JSON
/// * `agent_id` - Unique agent identifier
/// * `goal` - The agent's goal
/// * `timestamp_ms` - Current timestamp
///
/// # Returns
/// Updated SharedDiffStore JSON
#[wasm_bindgen]
pub fn register_collab_agent(
    store_json: &str,
    agent_id: &str,
    goal: &str,
    timestamp_ms: u64,
) -> String {
    let mut store = match collab::SharedDiffStore::from_json(store_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };
    store.register_agent(agent_id, goal, timestamp_ms);
    store.to_json()
}

/// Publish a semantic delta to the collaboration store
///
/// # Arguments
/// * `store_json` - SharedDiffStore JSON
/// * `agent_id` - Publishing agent's ID
/// * `url` - URL the delta applies to
/// * `delta_json` - SemanticDelta JSON (from diff_semantic_trees)
/// * `timestamp_ms` - Current timestamp
///
/// # Returns
/// Updated SharedDiffStore JSON
#[wasm_bindgen]
pub fn publish_collab_delta(
    store_json: &str,
    agent_id: &str,
    url: &str,
    delta_json: &str,
    timestamp_ms: u64,
) -> String {
    let mut store = match collab::SharedDiffStore::from_json(store_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };
    let mut delta: types::SemanticDelta = match serde_json::from_str(delta_json) {
        Ok(d) => d,
        Err(e) => return format!(r#"{{"error": "Invalid delta_json: {}"}}"#, e),
    };
    // Fyll i url från yttre parameter om den saknas i delta
    if delta.url.is_empty() {
        delta.url = url.to_string();
    }
    store.publish_delta(agent_id, url, delta, timestamp_ms);
    store.to_json()
}

/// Get collaboration store statistics
///
/// # Arguments
/// * `store_json` - SharedDiffStore JSON
///
/// # Returns
/// JSON with CollabStats: active_agents, cached_deltas, total_publishes, total_consumes
#[wasm_bindgen]
pub fn collab_stats(store_json: &str) -> String {
    let store = match collab::SharedDiffStore::from_json(store_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };
    let stats = store.stats();
    match serde_json::to_string_pretty(&stats) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

/// Clean up inactive agents from the collaboration store
///
/// # Arguments
/// * `store_json` - SharedDiffStore JSON
/// * `now_ms` - Current timestamp in milliseconds
/// * `max_age_ms` - Max inactivity before removal (milliseconds)
///
/// # Returns
/// Updated SharedDiffStore JSON
#[wasm_bindgen]
pub fn cleanup_collab_store(store_json: &str, now_ms: u64, max_age_ms: u64) -> String {
    let mut store = match collab::SharedDiffStore::from_json(store_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };
    store.cleanup_inactive(now_ms, max_age_ms);
    store.to_json()
}

/// Get the cached delta for a specific URL
///
/// # Arguments
/// * `store_json` - SharedDiffStore JSON
/// * `url` - URL to look up
///
/// # Returns
/// JSON with CachedDelta or {"found": false}
#[wasm_bindgen]
pub fn get_collab_delta_for_url(store_json: &str, url: &str) -> String {
    let store = match collab::SharedDiffStore::from_json(store_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };
    match store.get_delta_for_url(url) {
        Some(delta) => match serde_json::to_string_pretty(delta) {
            Ok(json) => json,
            Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
        },
        None => r#"{"found": false}"#.to_string(),
    }
}

/// Fetch new deltas from other agents
///
/// # Arguments
/// * `store_json` - SharedDiffStore JSON
/// * `agent_id` - Requesting agent's ID
///
/// # Returns
/// JSON with DeltaFetchResult: deltas, saved_parse_count, summary
/// Also returns updated store (consumed deltas are tracked)
#[wasm_bindgen]
pub fn fetch_collab_deltas(store_json: &str, agent_id: &str) -> String {
    let mut store = match collab::SharedDiffStore::from_json(store_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let result = store.fetch_deltas(agent_id);
    let response = serde_json::json!({
        "result": result,
        "store": store,
    });

    match serde_json::to_string_pretty(&response) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
    }
}

// ─── Fas 13: Session Management API ─────────────────────────────────────────

/// Create a new empty session manager
///
/// Returns JSON with empty cookie jar, no auth state.
/// Host persists the JSON and sends it back with each request.
#[wasm_bindgen]
pub fn create_session() -> String {
    session::SessionManager::new().to_json()
}

/// Add cookies from HTTP response Set-Cookie headers
///
/// # Arguments
/// * `session_json` - Current session state
/// * `domain` - Cookie domain (e.g., "example.com")
/// * `cookies_json` - JSON array of Set-Cookie header strings
#[wasm_bindgen]
pub fn session_add_cookies(session_json: &str, domain: &str, cookies_json: &str) -> String {
    let mut session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let headers: Vec<String> = match serde_json::from_str(cookies_json) {
        Ok(h) => h,
        Err(e) => return format!(r#"{{"error": "Invalid cookies JSON: {}"}}"#, e),
    };

    session.add_cookies_from_headers(domain, &headers);
    session.to_json()
}

/// Get Cookie header string for a URL
///
/// Returns JSON with `cookie_header` field (or null if no cookies).
#[wasm_bindgen]
pub fn session_get_cookies(session_json: &str, domain: &str, path: &str) -> String {
    let session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let now = now_ms();
    let header = session.get_cookie_header(domain, path, now);

    serde_json::to_string(&serde_json::json!({
        "cookie_header": header,
    }))
    .unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Set OAuth token on session
///
/// # Arguments
/// * `session_json` - Current session state
/// * `access_token` - OAuth access token
/// * `refresh_token` - OAuth refresh token (empty string for none)
/// * `expires_in_secs` - Token validity in seconds
/// * `scopes_json` - JSON array of scope strings
#[wasm_bindgen]
pub fn session_set_token(
    session_json: &str,
    access_token: &str,
    refresh_token: &str,
    expires_in_secs: u64,
    scopes_json: &str,
) -> String {
    let mut session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let scopes: Vec<String> = serde_json::from_str(scopes_json).unwrap_or_default();
    let refresh = if refresh_token.is_empty() {
        None
    } else {
        Some(refresh_token)
    };

    session.set_oauth_token(access_token, refresh, expires_in_secs, now_ms(), scopes);
    session.to_json()
}

/// Build OAuth 2.0 authorize URL
///
/// # Arguments
/// * `session_json` - Current session state
/// * `config_json` - OAuthConfig JSON
///
/// # Returns
/// JSON with `authorize_url`, `state`, and updated `session_json`
#[wasm_bindgen]
pub fn session_oauth_authorize(session_json: &str, config_json: &str) -> String {
    let mut session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let config: session::OAuthConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "Invalid OAuth config: {}"}}"#, e),
    };

    let result = session.build_authorize_url(&config);
    serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Prepare OAuth token exchange parameters
///
/// Returns JSON map of POST parameters for the token endpoint.
#[wasm_bindgen]
pub fn session_prepare_token_exchange(
    session_json: &str,
    config_json: &str,
    authorization_code: &str,
) -> String {
    let session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let config: session::OAuthConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "Invalid OAuth config: {}"}}"#, e),
    };

    let params = session.prepare_token_exchange(&config, authorization_code);
    serde_json::to_string(&params).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Detect login form in HTML
///
/// Returns JSON with login form info (username/password/submit node IDs) or null.
#[wasm_bindgen]
pub fn detect_login_form(html: &str, goal: &str, url: &str) -> String {
    let tree = build_tree(html, goal, url);
    let form = session::SessionManager::detect_login_form(&tree);

    serde_json::to_string(&serde_json::json!({
        "found": form.is_some(),
        "form": form,
    }))
    .unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Check session auth status
///
/// Returns JSON with `authenticated`, `auth_state`, `cookie_count`, `needs_refresh`.
#[wasm_bindgen]
pub fn session_status(session_json: &str) -> String {
    let session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let now = now_ms();
    serde_json::to_string(&serde_json::json!({
        "authenticated": session.is_authenticated(now),
        "auth_state": session.auth_state,
        "cookie_count": session.cookie_count(),
        "needs_refresh": session.needs_token_refresh(now),
        "authenticated_requests": session.authenticated_requests,
    }))
    .unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Evict expired cookies and mark expired tokens
///
/// Returns updated session JSON with expired cookies removed.
#[wasm_bindgen]
pub fn session_evict_expired(session_json: &str) -> String {
    let mut session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let now = now_ms();
    session.evict_expired(now);

    // Kolla om token behöver refresh
    if session.needs_token_refresh(now) {
        session.mark_token_expired();
    }

    session.to_json()
}

/// Mark session as logged in via form submission
///
/// Call this after successful form login (cookies set via session_add_cookies).
#[wasm_bindgen]
pub fn session_mark_logged_in(session_json: &str) -> String {
    let mut session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    session.mark_logged_in();
    session.record_authenticated_request();
    session.to_json()
}

/// Prepare OAuth token refresh parameters
///
/// Returns JSON map of POST parameters for the token endpoint,
/// or null if no refresh token is available.
#[wasm_bindgen]
pub fn session_prepare_refresh(session_json: &str, config_json: &str) -> String {
    let session = match session::SessionManager::from_json(session_json) {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let config: session::OAuthConfig = match serde_json::from_str(config_json) {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "Invalid OAuth config: {}"}}"#, e),
    };

    match session.prepare_token_refresh(&config) {
        Some(params) => {
            serde_json::to_string(&params).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
        }
        None => r#"{"error": "No refresh token available"}"#.to_string(),
    }
}

// ─── Fas 14: Workflow Orchestration API ─────────────────────────────────────

/// Create a new workflow orchestrator
///
/// Compiles the goal into an ActionPlan and initializes workflow state.
///
/// # Arguments
/// * `goal` - What to achieve (e.g., "köp billigaste flyg")
/// * `start_url` - Starting URL for the workflow
/// * `config_json` - OrchestratorConfig JSON (or "{}" for defaults)
///
/// # Returns
/// JSON with orchestrator state and first action
#[wasm_bindgen]
pub fn create_workflow(goal: &str, start_url: &str, config_json: &str) -> String {
    let config: orchestrator::OrchestratorConfig =
        serde_json::from_str(config_json).unwrap_or_default();
    let mut orch = orchestrator::WorkflowOrchestrator::new(goal, start_url, config);
    let result = orch.start(now_ms());
    serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Provide a fetched page to the workflow orchestrator
///
/// Call this after the host fetches a page requested by a FetchPage action.
///
/// # Arguments
/// * `orchestrator_json` - Current orchestrator state
/// * `html` - Fetched HTML content
/// * `url` - Final URL (after redirects)
///
/// # Returns
/// JSON with StepResult: next action, status, progress, extracted data
#[wasm_bindgen]
pub fn workflow_provide_page(orchestrator_json: &str, html: &str, url: &str) -> String {
    let mut orch = match orchestrator::WorkflowOrchestrator::from_json(orchestrator_json) {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let result = orch.provide_page(html, url, now_ms());
    serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Report a click result to the workflow orchestrator
///
/// Call this after the host executes a Click action.
#[wasm_bindgen]
pub fn workflow_report_click(orchestrator_json: &str, click_result_json: &str) -> String {
    let mut orch = match orchestrator::WorkflowOrchestrator::from_json(orchestrator_json) {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let click: types::ClickResult = match serde_json::from_str(click_result_json) {
        Ok(c) => c,
        Err(e) => return format!(r#"{{"error": "Invalid click result: {}"}}"#, e),
    };

    let result = orch.report_click_result(&click, now_ms());
    serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Report a fill form result to the workflow orchestrator
#[wasm_bindgen]
pub fn workflow_report_fill(orchestrator_json: &str, fill_result_json: &str) -> String {
    let mut orch = match orchestrator::WorkflowOrchestrator::from_json(orchestrator_json) {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let fill: types::FillFormResult = match serde_json::from_str(fill_result_json) {
        Ok(f) => f,
        Err(e) => return format!(r#"{{"error": "Invalid fill result: {}"}}"#, e),
    };

    let result = orch.report_fill_result(&fill, now_ms());
    serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Report an extract result to the workflow orchestrator
#[wasm_bindgen]
pub fn workflow_report_extract(orchestrator_json: &str, extract_result_json: &str) -> String {
    let mut orch = match orchestrator::WorkflowOrchestrator::from_json(orchestrator_json) {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let extract: types::ExtractDataResult = match serde_json::from_str(extract_result_json) {
        Ok(e) => e,
        Err(e) => return format!(r#"{{"error": "Invalid extract result: {}"}}"#, e),
    };

    let result = orch.report_extract_result(&extract, now_ms());
    serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Report a step as manually completed (for Verify/Wait actions)
#[wasm_bindgen]
pub fn workflow_complete_step(orchestrator_json: &str, step_index: u32) -> String {
    let mut orch = match orchestrator::WorkflowOrchestrator::from_json(orchestrator_json) {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    let result = orch.report_step_completed(step_index, now_ms());
    serde_json::to_string(&result).unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

/// Rollback a completed step and retry it
#[wasm_bindgen]
pub fn workflow_rollback_step(orchestrator_json: &str, step_index: u32) -> String {
    let mut orch = match orchestrator::WorkflowOrchestrator::from_json(orchestrator_json) {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    orch.rollback_step(step_index);
    orch.to_json()
}

/// Get workflow status summary
#[wasm_bindgen]
pub fn workflow_status(orchestrator_json: &str) -> String {
    let orch = match orchestrator::WorkflowOrchestrator::from_json(orchestrator_json) {
        Ok(o) => o,
        Err(e) => return format!(r#"{{"error": "{}"}}"#, e),
    };

    serde_json::to_string(&serde_json::json!({
        "status": orch.status,
        "goal": orch.goal,
        "progress": format!("{}/{}", orch.completed_steps.len(), orch.plan.total_steps),
        "current_url": orch.current_url,
        "pages_visited": orch.pages_visited,
        "extracted_data": orch.extracted_data,
        "completed_steps": orch.completed_steps,
        "page_history": orch.page_history,
    }))
    .unwrap_or_else(|e| format!(r#"{{"error": "{}"}}"#, e))
}

// ─── Fas 17: DDG Search Layer ────────────────────────────────────────────────

/// Search the web via DuckDuckGo HTML and return structured results.
///
/// Combines DDG HTML fetch + stream_parse pipeline to extract
/// title, URL, snippet, domain and optional direct answer.
///
/// # Arguments
/// * `query` - Free-text search query
/// * `top_n` - Number of results to return (1-10, default 3)
/// * `goal` - Agent goal for relevance scoring (default: same as query)
/// * `html` - Pre-fetched DDG HTML (if empty, returns error asking caller to fetch)
pub fn search_from_html(query: &str, html: &str, top_n: usize, goal: &str) -> String {
    let start = now_ms();
    let effective_goal = if goal.is_empty() {
        format!("hitta svar på: {}", query)
    } else {
        goal.to_string()
    };
    let effective_top_n = if top_n == 0 { 3 } else { top_n.min(10) };

    let ddg_url = search::build_ddg_url(query);

    // Full parse — DDG HTML är ~250 noder, stream_parse filtrerar bort snippets
    let tree = build_tree(html, &effective_goal, &ddg_url);
    let total_nodes = collect_all_nodes(&tree.nodes).len();

    // Extrahera strukturerade sökresultat från hela trädet
    let results = search::extract_results(&tree.nodes, effective_top_n);

    // Försök hitta direktsvar
    let (direct_answer, direct_answer_confidence) = search::detect_direct_answer(&results)
        .map(|(a, c)| (Some(a), c))
        .unwrap_or((None, 0.0));

    let search_result = search::SearchResult {
        query: query.to_string(),
        results,
        direct_answer,
        direct_answer_confidence,
        source_url: ddg_url,
        parse_ms: now_ms() - start,
        nodes_seen: total_nodes,
        nodes_emitted: total_nodes,
        deep: None,
        deep_fetch_ms: None,
    };

    match serialize_json(&search_result, 10) {
        Ok(json) => json,
        Err(e) => e,
    }
}

/// Convenience: build the DDG URL for a query so callers can fetch it
pub fn build_search_url(query: &str) -> String {
    search::build_ddg_url(query)
}

// ─── Tester ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_check() {
        let result = health_check();
        assert!(result.contains("ok"));
        assert!(result.contains("0.2.0"));
    }

    #[test]
    fn test_parse_returns_valid_json() {
        let html = r#"<html>
            <head><title>Test Shop</title></head>
            <body>
                <button>Lägg i varukorg</button>
                <a href="/checkout">Till kassan</a>
                <input type="text" placeholder="Sök produkter..." />
            </body>
        </html>"#;

        let result = parse_to_semantic_tree(html, "lägg i varukorg", "https://test.com");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Ska vara valid JSON");

        assert!(parsed["nodes"].is_array());
        assert_eq!(parsed["goal"], "lägg i varukorg");
    }

    #[test]
    fn test_top_nodes_respects_limit() {
        let html = r#"<html><body>
            <button>Knapp 1</button>
            <button>Knapp 2</button>
            <button>Knapp 3</button>
            <button>Knapp 4</button>
            <button>Knapp 5</button>
        </body></html>"#;

        let result = parse_top_nodes(html, "klicka knapp", "https://test.com", 3);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Ska vara valid JSON");

        assert!(parsed["top_nodes"].as_array().unwrap().len() <= 3);
    }

    // ─── Fas 2: Intent API smoke tests ───────────────────────────────────────

    #[test]
    fn test_find_and_click_returns_valid_json() {
        let html = r#"<html><body><button>Köp nu</button></body></html>"#;
        let result = find_and_click(html, "köp", "https://test.com", "Köp nu");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert_eq!(parsed["found"], true);
        assert_eq!(parsed["action"], "click");
    }

    #[test]
    fn test_fill_form_returns_valid_json() {
        let html = r#"<html><body><form>
            <input name="email" placeholder="E-post" />
        </form></body></html>"#;
        let result = fill_form(
            html,
            "logga in",
            "https://test.com",
            r#"{"email": "test@test.se"}"#,
        );
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["mappings"].is_array());
    }

    #[test]
    fn test_fill_form_invalid_json() {
        let html = r#"<html><body></body></html>"#;
        let result = fill_form(html, "test", "https://test.com", "not json");
        assert!(result.contains("error"));
    }

    #[test]
    fn test_extract_data_returns_valid_json() {
        let html = r#"<html><body>
            <h1>Produkt</h1>
            <p>999 kr</p>
        </body></html>"#;
        let result = extract_data(html, "hämta pris", "https://test.com", r#"["Produkt"]"#);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["entries"].is_array());
    }

    #[test]
    fn test_extract_data_invalid_json() {
        let html = r#"<html><body></body></html>"#;
        let result = extract_data(html, "test", "https://test.com", "not json");
        assert!(result.contains("error"));
    }

    #[test]
    fn test_create_workflow_memory_returns_valid_json() {
        let json = create_workflow_memory();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Valid JSON");
        assert!(parsed["steps"].is_array());
        assert_eq!(parsed["steps"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_add_workflow_step_returns_updated_memory() {
        let mem = create_workflow_memory();
        let updated = add_workflow_step(
            &mem,
            "click",
            "https://shop.se",
            "köp produkt",
            "Klickade på Köp-knappen",
        );
        let parsed: serde_json::Value = serde_json::from_str(&updated).expect("Valid JSON");
        assert_eq!(parsed["steps"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["steps"][0]["action"], "click");
    }

    #[test]
    fn test_add_workflow_step_invalid_memory() {
        let result = add_workflow_step("bad json", "click", "url", "goal", "summary");
        assert!(result.contains("error"));
    }

    #[test]
    fn test_workflow_context_set_and_get() {
        let mem = create_workflow_memory();
        let updated = set_workflow_context(&mem, "user_email", "test@test.se");
        let parsed: serde_json::Value = serde_json::from_str(&updated).expect("Valid JSON");
        assert_eq!(parsed["context"]["user_email"], "test@test.se");

        let value = get_workflow_context(&updated, "user_email");
        let val_parsed: serde_json::Value = serde_json::from_str(&value).expect("Valid JSON");
        assert_eq!(val_parsed["value"], "test@test.se");
    }

    // ─── Fas 4a: Semantic Diff smoke tests ────────────────────────────────

    #[test]
    fn test_diff_semantic_trees_returns_valid_json() {
        let html1 = r#"<html><body><button>Köp</button></body></html>"#;
        let html2 = r#"<html><body><button>Köp</button><a href="/ny">Ny länk</a></body></html>"#;

        let tree1 = parse_to_semantic_tree(html1, "köp", "https://test.com");
        let tree2 = parse_to_semantic_tree(html2, "köp", "https://test.com");

        let result = diff_semantic_trees(&tree1, &tree2);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");

        assert!(parsed["changes"].is_array(), "Borde ha changes-array");
        assert!(parsed["summary"].is_string(), "Borde ha summary");
        assert!(
            parsed["token_savings_ratio"].is_number(),
            "Borde ha token_savings_ratio"
        );
    }

    #[test]
    fn test_diff_semantic_trees_invalid_json() {
        let result = diff_semantic_trees("bad json", "{}");
        assert!(
            result.contains("error"),
            "Borde returnera error vid ogiltig JSON"
        );
    }

    #[test]
    fn test_diff_identical_trees_zero_changes() {
        let html = r#"<html><body><button>Köp</button></body></html>"#;
        let tree = parse_to_semantic_tree(html, "köp", "https://test.com");

        let result = diff_semantic_trees(&tree, &tree);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");

        assert_eq!(
            parsed["changes"].as_array().unwrap().len(),
            0,
            "Identiska träd borde ge 0 förändringar"
        );
    }

    // ─── Fas 4b: JS Sandbox smoke tests ───────────────────────────────

    #[test]
    fn test_detect_js_returns_valid_json() {
        let html = r#"<html><body>
            <script>document.getElementById('x').textContent = 'hi';</script>
            <button onclick="alert('clicked')">Click</button>
        </body></html>"#;

        let result = detect_js(html);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["snippets"].is_array(), "Borde ha snippets-array");
        assert_eq!(parsed["total_inline_scripts"], 1);
        assert_eq!(parsed["total_event_handlers"], 1);
    }

    #[test]
    fn test_detect_js_no_scripts() {
        let html = r#"<html><body><p>Statisk sida</p></body></html>"#;
        let result = detect_js(html);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert_eq!(parsed["total_inline_scripts"], 0);
        assert_eq!(parsed["total_event_handlers"], 0);
        assert_eq!(parsed["has_framework"], false);
    }

    #[test]
    fn test_eval_js_returns_valid_json() {
        let result = eval_js("1 + 1");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        // Med js-eval feature: value = "2"
        // Utan: error = "JS evaluation not available..."
        assert!(
            parsed["value"].is_string() || parsed["error"].is_string(),
            "Borde ha antingen value eller error"
        );
    }

    #[test]
    fn test_eval_js_batch_returns_valid_json() {
        let result = eval_js_batch(r#"["1+1", "'a'+'b'"]"#);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["results"].is_array(), "Borde ha results-array");
        assert_eq!(parsed["results"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_eval_js_batch_invalid_json() {
        let result = eval_js_batch("not json");
        assert!(result.contains("error"));
    }

    // ─── Fas 4c: Selective Execution smoke tests ──────────────────────

    #[test]
    fn test_parse_with_js_returns_valid_json() {
        let html = r#"<html><body>
            <script>document.getElementById('price').textContent = (29.99 * 2).toFixed(2);</script>
            <p id="price"></p>
            <button>Köp</button>
        </body></html>"#;

        let result = parse_with_js(html, "köp", "https://shop.se");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");

        assert!(parsed["tree"].is_object(), "Borde ha tree-objekt");
        assert!(
            parsed["js_bindings"].is_array(),
            "Borde ha js_bindings-array"
        );
        assert!(parsed["js_analysis"].is_object(), "Borde ha js_analysis");
    }

    #[test]
    fn test_parse_with_js_static_page() {
        let html = r#"<html><body><p>Statisk</p><button>Köp</button></body></html>"#;
        let result = parse_with_js(html, "köp", "https://shop.se");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");

        assert_eq!(parsed["total_evals"], 0);
        assert_eq!(parsed["js_bindings"].as_array().unwrap().len(), 0);
    }

    // ─── Fas 5: Temporal Memory smoke tests ─────────────────────────────

    #[test]
    fn test_create_temporal_memory_returns_valid_json() {
        let json = create_temporal_memory();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Valid JSON");
        assert!(parsed["snapshots"].is_array());
        assert_eq!(parsed["snapshots"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn test_add_temporal_snapshot_returns_updated_memory() {
        let mem = create_temporal_memory();
        let html = r#"<html><body><button>Köp</button></body></html>"#;
        let updated = add_temporal_snapshot(&mem, html, "köp", "https://shop.se", 1000);
        let parsed: serde_json::Value = serde_json::from_str(&updated).expect("Valid JSON");
        assert_eq!(parsed["snapshots"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_add_temporal_snapshot_invalid_memory() {
        let html = r#"<html><body></body></html>"#;
        let result = add_temporal_snapshot("bad json", html, "test", "url", 0);
        assert!(result.contains("error"));
    }

    #[test]
    fn test_analyze_temporal_returns_valid_json() {
        let mem = create_temporal_memory();
        let html = r#"<html><body><button>Köp</button></body></html>"#;
        let updated = add_temporal_snapshot(&mem, html, "köp", "https://shop.se", 1000);

        let result = analyze_temporal(&updated);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["snapshots"].is_array(), "Borde ha snapshots");
        assert!(parsed["risk_score"].is_number(), "Borde ha risk_score");
        assert!(parsed["summary"].is_string(), "Borde ha summary");
    }

    #[test]
    fn test_predict_temporal_returns_valid_json() {
        let mem = create_temporal_memory();
        let html = r#"<html><body><button>Köp</button></body></html>"#;
        let updated = add_temporal_snapshot(&mem, html, "köp", "https://shop.se", 1000);

        let result = predict_temporal(&updated);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["expected_node_count"].is_number(),
            "Borde ha expected_node_count"
        );
        assert!(parsed["confidence"].is_number(), "Borde ha confidence");
    }

    // ─── Fas 6: Intent Compiler smoke tests ─────────────────────────────

    #[test]
    fn test_compile_goal_returns_valid_json() {
        let result = compile_goal("köp iPhone 16 Pro");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["sub_goals"].is_array(), "Borde ha sub_goals");
        assert!(
            parsed["execution_order"].is_array(),
            "Borde ha execution_order"
        );
        assert_eq!(parsed["goal"], "köp iPhone 16 Pro");
    }

    #[test]
    fn test_compile_goal_unknown() {
        let result = compile_goal("gör något konstigt");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["sub_goals"].as_array().unwrap().len() >= 3,
            "Generisk plan borde ha minst 3 steg"
        );
    }

    #[test]
    fn test_execute_plan_returns_valid_json() {
        let plan_json = compile_goal("logga in");
        let html = r##"<html><body>
            <input placeholder="E-post" />
            <input type="password" placeholder="Lösenord" />
            <button>Logga in</button>
        </body></html>"##;

        let result = execute_plan(&plan_json, html, "logga in", "https://test.com", "[]");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["plan"].is_object(), "Borde ha plan");
        assert!(parsed["summary"].is_string(), "Borde ha summary");
    }

    #[test]
    fn test_execute_plan_invalid_json() {
        let result = execute_plan("bad", "<html></html>", "test", "url", "[]");
        assert!(result.contains("error"));
    }

    // ─── Fas 9a: Causal Action Graph smoke tests ──────────────────────

    #[test]
    fn test_build_causal_graph_returns_valid_json() {
        let snapshots = r#"[
            {"url": "https://shop.se", "node_count": 5, "warning_count": 0, "key_elements": ["button:Köp"]},
            {"url": "https://shop.se/kassa", "node_count": 8, "warning_count": 0, "key_elements": ["button:Betala"]}
        ]"#;
        let actions = r#"["click: Köp"]"#;

        let result = build_causal_graph(snapshots, actions);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["states"].is_array(), "Borde ha states");
        assert!(parsed["edges"].is_array(), "Borde ha edges");
    }

    #[test]
    fn test_build_causal_graph_minimal_fields() {
        // Endast url krävs — övriga fält har defaults
        let snapshots = r#"[{"url": "https://example.com"}, {"url": "https://example.com/page2"}]"#;
        let actions = r#"["click: Link"]"#;

        let result = build_causal_graph(snapshots, actions);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["states"].is_array(),
            "Borde ha states med minimal input"
        );
        assert_eq!(
            parsed["states"].as_array().unwrap().len(),
            2,
            "Borde ha 2 states"
        );
    }

    #[test]
    fn test_predict_action_outcome_returns_valid_json() {
        let snapshots = r#"[
            {"url": "https://shop.se", "node_count": 5, "warning_count": 0, "key_elements": ["button:Köp"]},
            {"url": "https://shop.se/kassa", "node_count": 8, "warning_count": 0, "key_elements": ["button:Betala"]}
        ]"#;
        let graph_json = build_causal_graph(snapshots, r#"["click: Köp"]"#);

        let result = predict_action_outcome(&graph_json, "click: Köp");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["recommendation"].is_string(),
            "Borde ha recommendation"
        );
    }

    #[test]
    fn test_find_safest_path_returns_valid_json() {
        let snapshots = r#"[
            {"url": "https://shop.se", "node_count": 5, "warning_count": 0, "key_elements": ["button:Köp"]},
            {"url": "https://shop.se/kassa", "node_count": 8, "warning_count": 0, "key_elements": ["button:Betala"]}
        ]"#;
        let graph_json = build_causal_graph(snapshots, r#"["click: Köp"]"#);

        let result = find_safest_path(&graph_json, "kassa");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["path"].is_array(), "Borde ha path");
        assert!(parsed["summary"].is_string(), "Borde ha summary");
    }

    // ─── Fas 9b: WebMCP smoke tests ─────────────────────────────────

    #[test]
    fn test_discover_webmcp_no_tools() {
        let html = r#"<html><body><p>Normal sida</p></body></html>"#;
        let result = discover_webmcp(html, "https://test.com");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert_eq!(parsed["has_webmcp"], false);
        assert!(parsed["tools"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_discover_webmcp_with_tools() {
        let html = r##"<html><body>
        <script>
            navigator.modelContext.registerTool({
                name: "search",
                description: "Search products"
            });
        </script>
        </body></html>"##;
        let result = discover_webmcp(html, "https://shop.com");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert_eq!(parsed["has_webmcp"], true);
        assert_eq!(parsed["tools"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["tools"][0]["name"], "search");
    }

    // ─── Fas 9c: Multimodal Grounding smoke tests ───────────────────

    #[test]
    fn test_ground_semantic_tree_returns_valid_json() {
        let html = r#"<html><body><button id="buy">Köp</button></body></html>"#;
        let annotations =
            r#"[{"html_id": "buy", "bbox": {"x": 10, "y": 20, "width": 80, "height": 30}}]"#;
        let result = ground_semantic_tree(html, "köp", "https://test.com", annotations);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["matched_count"].is_number(),
            "Borde ha matched_count"
        );
        assert!(parsed["set_of_marks"].is_array(), "Borde ha set_of_marks");
    }

    #[test]
    fn test_match_bbox_iou_returns_valid_json() {
        let html = r#"<html><body><button id="buy">Köp</button></body></html>"#;
        let annotations =
            r#"[{"html_id": "buy", "bbox": {"x": 10, "y": 20, "width": 80, "height": 30}}]"#;
        let grounded = ground_semantic_tree(html, "köp", "https://test.com", annotations);
        let parsed: serde_json::Value = serde_json::from_str(&grounded).expect("Valid JSON");
        let tree_json = serde_json::to_string(&parsed["tree"]).unwrap_or_default();

        let bbox = r#"{"x": 15, "y": 25, "width": 70, "height": 25}"#;
        let result = match_bbox_iou(&tree_json, bbox);
        // Should parse without error
        let _: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
    }

    // ─── Fas 9d: Cross-Agent Diffing smoke tests ────────────────────

    #[test]
    fn test_create_collab_store_returns_valid_json() {
        let json = create_collab_store();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Valid JSON");
        assert!(parsed["agents"].is_array());
        assert!(parsed["agents"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_register_collab_agent_returns_updated_store() {
        let store = create_collab_store();
        let updated = register_collab_agent(&store, "agent-1", "buy shoes", 1000);
        let parsed: serde_json::Value = serde_json::from_str(&updated).expect("Valid JSON");
        assert_eq!(parsed["agents"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["agents"][0]["agent_id"], "agent-1");
    }

    #[test]
    fn test_collab_publish_and_fetch() {
        let store = create_collab_store();
        let store = register_collab_agent(&store, "a", "test", 1000);
        let store = register_collab_agent(&store, "b", "test", 1000);

        // Skapa en enkel delta
        let html1 = r#"<html><body><button>Köp</button></body></html>"#;
        let html2 = r#"<html><body><button>Köp nu</button></body></html>"#;
        let tree1 = parse_to_semantic_tree(html1, "köp", "https://shop.se");
        let tree2 = parse_to_semantic_tree(html2, "köp", "https://shop.se");
        let delta = diff_semantic_trees(&tree1, &tree2);

        let store = publish_collab_delta(&store, "a", "https://shop.se", &delta, 2000);
        let result = fetch_collab_deltas(&store, "b");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["result"]["deltas"].is_array(),
            "Borde ha deltas i result"
        );
    }

    // ─── Fas 13: Session Management smoke tests ─────────────────────

    #[test]
    fn test_create_session_returns_valid_json() {
        let json = create_session();
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("Valid JSON");
        assert_eq!(parsed["auth_state"], "Unauthenticated");
    }

    #[test]
    fn test_session_add_and_get_cookies() {
        let session = create_session();
        let session = session_add_cookies(
            &session,
            "example.com",
            r#"["session=abc123; Path=/; HttpOnly"]"#,
        );
        let result = session_get_cookies(&session, "example.com", "/");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["cookie_header"].is_string(),
            "Borde ha cookie_header"
        );
        assert!(
            parsed["cookie_header"]
                .as_str()
                .unwrap()
                .contains("session=abc123"),
            "Borde innehålla session cookie"
        );
    }

    #[test]
    fn test_session_set_token_and_status() {
        let session = create_session();
        let session = session_set_token(
            &session,
            "my_token",
            "my_refresh",
            3600,
            r#"["read", "write"]"#,
        );

        let status = session_status(&session);
        let parsed: serde_json::Value = serde_json::from_str(&status).expect("Valid JSON");
        assert_eq!(parsed["authenticated"], true);
        assert_eq!(parsed["auth_state"], "OAuthAuthenticated");
    }

    #[test]
    fn test_session_oauth_authorize() {
        let session = create_session();
        let config = r#"{
            "authorize_url": "https://auth.example.com/authorize",
            "token_url": "https://auth.example.com/token",
            "client_id": "my_client",
            "redirect_uri": "https://app.example.com/callback",
            "scopes": ["read"]
        }"#;

        let result = session_oauth_authorize(&session, config);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(
            parsed["authorize_url"].is_string(),
            "Borde ha authorize_url"
        );
        assert!(
            parsed["authorize_url"]
                .as_str()
                .unwrap()
                .contains("client_id=my_client"),
            "Borde innehålla client_id"
        );
    }

    #[test]
    fn test_detect_login_form_found() {
        let html = r##"<html><body>
            <input name="email" placeholder="E-post" />
            <input name="password" type="password" placeholder="Lösenord" />
            <button>Logga in</button>
        </body></html>"##;
        let result = detect_login_form(html, "logga in", "https://test.com");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert_eq!(parsed["found"], true, "Borde hitta login-formulär");
    }

    #[test]
    fn test_detect_login_form_not_found() {
        let html = r#"<html><body><p>Normal sida</p></body></html>"#;
        let result = detect_login_form(html, "browse", "https://test.com");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert_eq!(parsed["found"], false, "Borde inte hitta login-formulär");
    }

    #[test]
    fn test_session_add_cookies_invalid_json() {
        let result = session_add_cookies("bad json", "domain", "[]");
        assert!(result.contains("error"));
    }

    // ─── Fas 14: Workflow Orchestration smoke tests ─────────────────

    #[test]
    fn test_create_workflow_returns_valid_json() {
        let result = create_workflow("köp flyg", "https://flights.se", "{}");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["next_action"].is_object(), "Borde ha next_action");
        assert!(parsed["progress"].is_string(), "Borde ha progress");
    }

    #[test]
    fn test_workflow_provide_page() {
        let result = create_workflow("hitta priser", "https://shop.se", "{}");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        let orch_json = parsed["orchestrator_json"].as_str().unwrap();

        let html = r#"<html><body><h1>Produkter</h1><button>Köp</button></body></html>"#;
        let result = workflow_provide_page(orch_json, html, "https://shop.se");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["summary"].is_string(), "Borde ha summary");
        assert!(parsed["progress"].is_string(), "Borde ha progress");
    }

    #[test]
    fn test_workflow_complete_step() {
        let result = create_workflow("test", "https://test.se", "{}");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        let orch_json = parsed["orchestrator_json"].as_str().unwrap();

        let result = workflow_complete_step(orch_json, 0);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["progress"].is_string(), "Borde ha progress");
    }

    #[test]
    fn test_workflow_rollback_step() {
        let result = create_workflow("test", "https://test.se", "{}");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        let orch_json = parsed["orchestrator_json"].as_str().unwrap();

        // Complete then rollback
        let step_result = workflow_complete_step(orch_json, 0);
        let parsed: serde_json::Value = serde_json::from_str(&step_result).expect("Valid JSON");
        let orch_json = parsed["orchestrator_json"].as_str().unwrap();

        let result = workflow_rollback_step(orch_json, 0);
        let _: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
    }

    #[test]
    fn test_workflow_status() {
        let result = create_workflow("test", "https://test.se", "{}");
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        let orch_json = parsed["orchestrator_json"].as_str().unwrap();

        let status = workflow_status(orch_json);
        let parsed: serde_json::Value = serde_json::from_str(&status).expect("Valid JSON");
        assert!(parsed["status"].is_string(), "Borde ha status");
        assert!(parsed["goal"].is_string(), "Borde ha goal");
        assert!(parsed["progress"].is_string(), "Borde ha progress");
    }

    #[test]
    fn test_workflow_invalid_json() {
        let result = workflow_provide_page("bad json", "<html></html>", "url");
        assert!(result.contains("error"));
    }
}
