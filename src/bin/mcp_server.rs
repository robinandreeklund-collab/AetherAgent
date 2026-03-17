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
    eprintln!("        check_injection, compile_goal, classify_request, diff_trees, parse_with_js");

    let server = AetherMcpServer::new();

    let transport = rmcp::transport::stdio();
    let service = server.serve(transport).await?;
    service.waiting().await?;

    Ok(())
}
