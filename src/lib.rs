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
mod js_bridge;
mod js_eval;
mod memory;
mod parser;
mod semantic;
mod temporal;
mod trust;
pub mod types;
mod webmcp;

use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use wasm_bindgen::prelude::*;

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

    match serde_json::to_string(&tree) {
        Ok(json) => json,
        Err(e) => format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
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
    let tree_json = match serde_json::to_string(&tree) {
        Ok(j) => j,
        Err(e) => return format!(r#"{{"error": "Serialization failed: {}"}}"#, e),
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
/// * `snapshots_json` - JSON array of [url, node_count, warning_count, key_elements]
/// * `actions_json` - JSON array of action strings between snapshots
///
/// # Returns
/// JSON with CausalGraph: states, edges, current_state_id
#[wasm_bindgen]
pub fn build_causal_graph(snapshots_json: &str, actions_json: &str) -> String {
    let snapshots: Vec<(String, u32, u32, Vec<String>)> = match serde_json::from_str(snapshots_json)
    {
        Ok(s) => s,
        Err(e) => return format!(r#"{{"error": "Invalid snapshots_json: {}"}}"#, e),
    };

    let actions: Vec<String> = match serde_json::from_str(actions_json) {
        Ok(a) => a,
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
    let delta: types::SemanticDelta = match serde_json::from_str(delta_json) {
        Ok(d) => d,
        Err(e) => return format!(r#"{{"error": "Invalid delta_json: {}"}}"#, e),
    };
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
            ["https://shop.se", 5, 0, ["button:Köp"]],
            ["https://shop.se/kassa", 8, 0, ["button:Betala"]]
        ]"#;
        let actions = r#"["click: Köp"]"#;

        let result = build_causal_graph(snapshots, actions);
        let parsed: serde_json::Value = serde_json::from_str(&result).expect("Valid JSON");
        assert!(parsed["states"].is_array(), "Borde ha states");
        assert!(parsed["edges"].is_array(), "Borde ha edges");
    }

    #[test]
    fn test_predict_action_outcome_returns_valid_json() {
        let snapshots = r#"[
            ["https://shop.se", 5, 0, ["button:Köp"]],
            ["https://shop.se/kassa", 8, 0, ["button:Betala"]]
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
            ["https://shop.se", 5, 0, ["button:Köp"]],
            ["https://shop.se/kassa", 8, 0, ["button:Betala"]]
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
}
