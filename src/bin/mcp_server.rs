// Justification: rmcp's #[tool_router] proc macro uses the parameter structs
// and tool_router field via generated code that rustc can't detect.
#![allow(dead_code)]

/// AetherAgent MCP Server
///
/// Exposes AetherAgent as MCP (Model Context Protocol) tools for AI agents.
/// Runs on stdio transport – compatible with Claude Desktop, Cursor, VS Code, etc.
///
/// Run: cargo run --features mcp --bin aether-mcp
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::ServerInfo;
use rmcp::schemars;
use rmcp::{tool, tool_router, ServerHandler, ServiceExt};

use serde::Deserialize;
use std::collections::HashMap;

// ─── Parameter types ────────────────────────────────────────────────────────
#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct ParseParams {
    /// Raw HTML string from the web page
    html: String,
    /// The agent's current goal (e.g. "buy cheapest flight")
    goal: String,
    /// The page URL
    url: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct ParseTopParams {
    /// Raw HTML string
    html: String,
    /// The agent's current goal
    goal: String,
    /// The page URL
    url: String,
    /// Max number of nodes to return (recommended: 10-20)
    top_n: u32,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct ClickParams {
    /// Raw HTML string
    html: String,
    /// The agent's current goal
    goal: String,
    /// The page URL
    url: String,
    /// What to click (e.g. "Add to cart", "Log in")
    target_label: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct FillFormParams {
    /// Raw HTML string
    html: String,
    /// The agent's current goal
    goal: String,
    /// The page URL
    url: String,
    /// Form fields as key-value map
    fields: HashMap<String, String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct ExtractParams {
    /// Raw HTML string
    html: String,
    /// The agent's current goal
    goal: String,
    /// The page URL
    url: String,
    /// Keys to extract (e.g. ["price", "title"])
    keys: Vec<String>,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CheckInjectionParams {
    /// Text to check for injection
    text: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CompileGoalParams {
    /// The agent's goal (e.g. "buy iPhone 16 Pro", "logga in")
    goal: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct FirewallParams {
    /// URL to classify
    url: String,
    /// The agent's current goal
    goal: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct DiffParams {
    /// Previous semantic tree JSON
    old_tree_json: String,
    /// Current semantic tree JSON
    new_tree_json: String,
}

// ─── Fas 9 parameter types ──────────────────────────────────────────────────

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct BuildCausalGraphParams {
    /// JSON array of temporal snapshots (from temporal memory)
    snapshots_json: String,
    /// JSON array of action labels between snapshots
    actions_json: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct PredictOutcomeParams {
    /// Causal graph JSON (from build_causal_graph)
    graph_json: String,
    /// Action to predict outcome for
    action: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct SafestPathParams {
    /// Causal graph JSON (from build_causal_graph)
    graph_json: String,
    /// Target goal state label
    goal: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct WebMcpDiscoverParams {
    /// Raw HTML string from the web page
    html: String,
    /// The page URL
    url: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct GroundTreeParams {
    /// Raw HTML string
    html: String,
    /// The agent's current goal
    goal: String,
    /// The page URL
    url: String,
    /// JSON array of bbox annotations to match against nodes
    annotations_json: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct MatchBboxParams {
    /// Semantic tree JSON (from parse)
    tree_json: String,
    /// Bounding box JSON ({x, y, width, height})
    bbox_json: String,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CollabCreateParams {}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CollabRegisterParams {
    /// Collab store JSON
    store_json: String,
    /// Unique agent ID
    agent_id: String,
    /// Agent's goal
    goal: String,
    /// Timestamp in milliseconds since epoch
    timestamp_ms: u64,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CollabPublishParams {
    /// Collab store JSON
    store_json: String,
    /// Publishing agent's ID
    agent_id: String,
    /// URL the delta applies to
    url: String,
    /// Semantic delta JSON (from diff_trees)
    delta_json: String,
    /// Timestamp in milliseconds since epoch
    timestamp_ms: u64,
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
struct CollabFetchParams {
    /// Collab store JSON
    store_json: String,
    /// Requesting agent's ID
    agent_id: String,
}

// ─── Server ─────────────────────────────────────────────────────────────────

struct AetherMcpServer {
    tool_router: ToolRouter<Self>,
}

impl AetherMcpServer {
    fn new() -> Self {
        Self {
            tool_router: ToolRouter::new(),
        }
    }
}

#[tool_router]
impl AetherMcpServer {
    #[tool(
        name = "parse",
        description = "Parse HTML to a semantic accessibility tree with goal-relevance scoring. Returns structured JSON with roles, labels, actions, and relevance scores."
    )]
    fn parse(&self, Parameters(params): Parameters<ParseParams>) -> String {
        aether_agent::parse_to_semantic_tree(&params.html, &params.goal, &params.url)
    }

    #[tool(
        name = "parse_top",
        description = "Parse HTML and return only the top-N most relevant nodes. Reduces token usage."
    )]
    fn parse_top(&self, Parameters(params): Parameters<ParseTopParams>) -> String {
        aether_agent::parse_top_nodes(&params.html, &params.goal, &params.url, params.top_n)
    }

    #[tool(
        name = "find_and_click",
        description = "Find the best clickable element (button, link) matching a target label on the page."
    )]
    fn find_and_click(&self, Parameters(params): Parameters<ClickParams>) -> String {
        aether_agent::find_and_click(
            &params.html,
            &params.goal,
            &params.url,
            &params.target_label,
        )
    }

    #[tool(
        name = "fill_form",
        description = "Map form fields to provided key/value pairs. Returns selector hints for filling each field."
    )]
    fn fill_form(&self, Parameters(params): Parameters<FillFormParams>) -> String {
        let fields_json =
            serde_json::to_string(&params.fields).unwrap_or_else(|_| "{}".to_string());
        aether_agent::fill_form(&params.html, &params.goal, &params.url, &fields_json)
    }

    #[tool(
        name = "extract_data",
        description = "Extract structured data from a page by semantic keys (e.g. 'price', 'title')."
    )]
    fn extract_data(&self, Parameters(params): Parameters<ExtractParams>) -> String {
        let keys_json = serde_json::to_string(&params.keys).unwrap_or_else(|_| "[]".to_string());
        aether_agent::extract_data(&params.html, &params.goal, &params.url, &keys_json)
    }

    #[tool(
        name = "check_injection",
        description = "Check text for prompt injection patterns. Returns safe:true or injection warning."
    )]
    fn check_injection(&self, Parameters(params): Parameters<CheckInjectionParams>) -> String {
        aether_agent::check_injection(&params.text)
    }

    #[tool(
        name = "compile_goal",
        description = "Compile a complex goal into an optimized action plan with sub-goals and execution order."
    )]
    fn compile_goal(&self, Parameters(params): Parameters<CompileGoalParams>) -> String {
        aether_agent::compile_goal(&params.goal)
    }

    #[tool(
        name = "classify_request",
        description = "Classify URL against the semantic firewall. Returns allowed/blocked and reason (L1: tracking, L2: file type, L3: semantic relevance)."
    )]
    fn classify_request(&self, Parameters(params): Parameters<FirewallParams>) -> String {
        aether_agent::classify_request(&params.url, &params.goal, "{}")
    }

    #[tool(
        name = "diff_trees",
        description = "Compare two semantic trees and return only the changes (delta). 80-95% token savings."
    )]
    fn diff_trees(&self, Parameters(params): Parameters<DiffParams>) -> String {
        aether_agent::diff_semantic_trees(&params.old_tree_json, &params.new_tree_json)
    }

    #[tool(
        name = "parse_with_js",
        description = "Parse HTML with automatic JavaScript detection, evaluation, and application to the semantic tree."
    )]
    fn parse_with_js(&self, Parameters(params): Parameters<ParseParams>) -> String {
        aether_agent::parse_with_js(&params.html, &params.goal, &params.url)
    }

    // ─── Fas 9a: Causal Action Graph ─────────────────────────────────────────

    #[tool(
        name = "build_causal_graph",
        description = "Build a causal action graph from temporal snapshots. Models page state transitions as a directed graph with probabilities and risk scores."
    )]
    fn build_causal_graph(&self, Parameters(params): Parameters<BuildCausalGraphParams>) -> String {
        aether_agent::build_causal_graph(&params.snapshots_json, &params.actions_json)
    }

    #[tool(
        name = "predict_action_outcome",
        description = "Predict the outcome of an action using the causal graph. Returns probability, risk, and expected state changes."
    )]
    fn predict_action_outcome(
        &self,
        Parameters(params): Parameters<PredictOutcomeParams>,
    ) -> String {
        aether_agent::predict_action_outcome(&params.graph_json, &params.action)
    }

    #[tool(
        name = "find_safest_path",
        description = "Find the safest path to a goal state in the causal graph. Uses BFS with risk-weighting."
    )]
    fn find_safest_path(&self, Parameters(params): Parameters<SafestPathParams>) -> String {
        aether_agent::find_safest_path(&params.graph_json, &params.goal)
    }

    // ─── Fas 9b: WebMCP Discovery ───────────────────────────────────────────

    #[tool(
        name = "discover_webmcp",
        description = "Discover WebMCP tools registered on a web page via navigator.modelContext.registerTool(). Returns tool names, descriptions, and input schemas."
    )]
    fn discover_webmcp(&self, Parameters(params): Parameters<WebMcpDiscoverParams>) -> String {
        aether_agent::discover_webmcp(&params.html, &params.url)
    }

    // ─── Fas 9c: Multimodal Grounding ───────────────────────────────────────

    #[tool(
        name = "ground_semantic_tree",
        description = "Ground a semantic tree with bounding box annotations. Matches visual coordinates to DOM elements and generates Set-of-Mark annotations."
    )]
    fn ground_semantic_tree(&self, Parameters(params): Parameters<GroundTreeParams>) -> String {
        aether_agent::ground_semantic_tree(
            &params.html,
            &params.goal,
            &params.url,
            &params.annotations_json,
        )
    }

    #[tool(
        name = "match_bbox_iou",
        description = "Match a predicted bounding box against all nodes in a semantic tree using IoU (Intersection over Union)."
    )]
    fn match_bbox_iou(&self, Parameters(params): Parameters<MatchBboxParams>) -> String {
        aether_agent::match_bbox_iou(&params.tree_json, &params.bbox_json)
    }

    // ─── Fas 9d: Cross-Agent Diffing ────────────────────────────────────────

    #[tool(
        name = "create_collab_store",
        description = "Create an empty shared diff store for cross-agent collaboration."
    )]
    fn create_collab_store(&self, Parameters(_params): Parameters<CollabCreateParams>) -> String {
        aether_agent::create_collab_store()
    }

    #[tool(
        name = "register_collab_agent",
        description = "Register an agent in the collab store. Agents can then publish and consume semantic deltas."
    )]
    fn register_collab_agent(
        &self,
        Parameters(params): Parameters<CollabRegisterParams>,
    ) -> String {
        aether_agent::register_collab_agent(
            &params.store_json,
            &params.agent_id,
            &params.goal,
            params.timestamp_ms,
        )
    }

    #[tool(
        name = "publish_collab_delta",
        description = "Publish a semantic delta to the collab store for other agents to consume."
    )]
    fn publish_collab_delta(&self, Parameters(params): Parameters<CollabPublishParams>) -> String {
        aether_agent::publish_collab_delta(
            &params.store_json,
            &params.agent_id,
            &params.url,
            &params.delta_json,
            params.timestamp_ms,
        )
    }

    #[tool(
        name = "fetch_collab_deltas",
        description = "Fetch new semantic deltas for an agent from the collab store. Returns only deltas not yet consumed."
    )]
    fn fetch_collab_deltas(&self, Parameters(params): Parameters<CollabFetchParams>) -> String {
        aether_agent::fetch_collab_deltas(&params.store_json, &params.agent_id)
    }
}

impl ServerHandler for AetherMcpServer {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.instructions = Some(
            "AetherAgent – LLM-native browser engine. Parse HTML into semantic trees \
             with goal-relevance scoring, prompt injection protection, and intent-aware \
             actions. Use 'parse' for full trees, 'parse_top' for token-efficient results, \
             'find_and_click' to locate clickable elements, 'extract_data' for structured \
             data extraction."
                .to_string(),
        );
        info
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("AetherAgent MCP Server starting on stdio...");
    eprintln!("Tools: parse, parse_top, find_and_click, fill_form, extract_data,");
    eprintln!(
        "        check_injection, compile_goal, classify_request, diff_trees, parse_with_js,"
    );
    eprintln!("        build_causal_graph, predict_action_outcome, find_safest_path,");
    eprintln!("        discover_webmcp, ground_semantic_tree, match_bbox_iou,");
    eprintln!("        create_collab_store, register_collab_agent, publish_collab_delta, fetch_collab_deltas");

    let server = AetherMcpServer::new();

    let transport = rmcp::transport::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}
