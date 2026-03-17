#!/usr/bin/env python3
"""
AetherAgent MCP Proxy – Connect Claude Desktop to a remote AetherAgent server.

This lightweight MCP server proxies tool calls over stdio to a remote
AetherAgent HTTP API (e.g. on Render, Docker, or localhost).

Usage:
  AETHER_URL=https://your-app.onrender.com python3 mcp_proxy.py

Claude Desktop config (~/.config/Claude/claude_desktop_config.json on Linux,
~/Library/Application Support/Claude/claude_desktop_config.json on macOS):

  {
    "mcpServers": {
      "aether-agent": {
        "command": "python3",
        "args": ["/path/to/AetherAgent/mcp_proxy.py"],
        "env": {
          "AETHER_URL": "https://your-app.onrender.com"
        }
      }
    }
  }

Requirements: Python 3.8+, requests (pip install requests)
"""

import json
import os
import sys
from typing import Any

try:
    import requests
except ImportError:
    sys.stderr.write("Error: pip install requests\n")
    sys.exit(1)

AETHER_URL = os.environ.get("AETHER_URL", "https://aether-agent-api.onrender.com").rstrip("/")

# ─── Tool definitions ─────────────────────────────────────────────────────────

TOOLS = [
    {
        "name": "parse",
        "description": "Parse HTML to a semantic accessibility tree with goal-relevance scoring. Returns structured JSON with roles, labels, actions, and relevance scores.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string from the web page"},
                "goal": {"type": "string", "description": "The agent's current goal (e.g. 'buy cheapest flight')"},
                "url": {"type": "string", "description": "The page URL"},
            },
            "required": ["html", "goal", "url"],
        },
    },
    {
        "name": "parse_top",
        "description": "Parse HTML and return only the top-N most relevant nodes. Reduces token usage.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "url": {"type": "string", "description": "The page URL"},
                "top_n": {"type": "integer", "description": "Max nodes to return (recommended: 10-20)"},
            },
            "required": ["html", "goal", "url", "top_n"],
        },
    },
    {
        "name": "fetch_parse",
        "description": "Fetch a URL and parse it into a semantic tree in one call. AetherAgent fetches the page with cookies, redirects, robots.txt compliance, and SSRF protection.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to fetch and parse"},
                "goal": {"type": "string", "description": "The agent's current goal"},
            },
            "required": ["url", "goal"],
        },
    },
    {
        "name": "find_and_click",
        "description": "Find the best clickable element (button, link) matching a target label on the page.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "url": {"type": "string", "description": "The page URL"},
                "target_label": {"type": "string", "description": "What to click (e.g. 'Add to cart', 'Log in')"},
            },
            "required": ["html", "goal", "url", "target_label"],
        },
    },
    {
        "name": "fill_form",
        "description": "Map form fields to provided key/value pairs. Returns selector hints for filling each field.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "url": {"type": "string", "description": "The page URL"},
                "fields": {"type": "object", "description": "Form fields as key-value map", "additionalProperties": {"type": "string"}},
            },
            "required": ["html", "goal", "url", "fields"],
        },
    },
    {
        "name": "extract_data",
        "description": "Extract structured data from a page by semantic keys (e.g. 'price', 'title').",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "url": {"type": "string", "description": "The page URL"},
                "keys": {"type": "array", "items": {"type": "string"}, "description": "Keys to extract (e.g. ['price', 'title'])"},
            },
            "required": ["html", "goal", "url", "keys"],
        },
    },
    {
        "name": "check_injection",
        "description": "Check text for prompt injection patterns. Returns safe:true or injection warning.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "text": {"type": "string", "description": "Text to check for injection"},
            },
            "required": ["text"],
        },
    },
    {
        "name": "compile_goal",
        "description": "Compile a complex goal into an optimized action plan with sub-goals and execution order.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "goal": {"type": "string", "description": "The agent's goal (e.g. 'buy iPhone 16 Pro')"},
            },
            "required": ["goal"],
        },
    },
    {
        "name": "classify_request",
        "description": "Classify URL against the semantic firewall. Returns allowed/blocked and reason (L1: tracking, L2: file type, L3: semantic relevance).",
        "inputSchema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to classify"},
                "goal": {"type": "string", "description": "The agent's current goal"},
            },
            "required": ["url", "goal"],
        },
    },
    {
        "name": "diff_trees",
        "description": "Compare two semantic trees and return only the changes (delta). 70-99% token savings.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "old_tree_json": {"type": "string", "description": "Previous semantic tree JSON"},
                "new_tree_json": {"type": "string", "description": "Current semantic tree JSON"},
            },
            "required": ["old_tree_json", "new_tree_json"],
        },
    },
    {
        "name": "fetch_extract",
        "description": "Fetch a URL and extract structured data in one call.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to fetch"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "keys": {"type": "array", "items": {"type": "string"}, "description": "Keys to extract"},
            },
            "required": ["url", "goal", "keys"],
        },
    },
    {
        "name": "fetch_click",
        "description": "Fetch a URL and find a clickable element in one call.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "url": {"type": "string", "description": "URL to fetch"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "target_label": {"type": "string", "description": "What to click"},
            },
            "required": ["url", "goal", "target_label"],
        },
    },
    # ─── Fas 4c: JS Evaluation ───────────────────────────────────────────────
    {
        "name": "parse_with_js",
        "description": "Parse HTML with automatic JS evaluation. Detects inline scripts, evaluates them in a sandboxed Boa engine, and applies DOM mutations to the semantic tree.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "url": {"type": "string", "description": "The page URL"},
            },
            "required": ["html", "goal", "url"],
        },
    },
    # ─── Fas 9a: Causal Action Graph ────────────────────────────────────────
    {
        "name": "build_causal_graph",
        "description": "Build a causal action graph from temporal page snapshots and actions. Enables outcome prediction and safe path planning.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "snapshots_json": {"type": "string", "description": "JSON array of temporal page snapshots"},
                "actions_json": {"type": "string", "description": "JSON array of actions taken between snapshots"},
            },
            "required": ["snapshots_json", "actions_json"],
        },
    },
    {
        "name": "predict_action_outcome",
        "description": "Predict the outcome of an action using the causal graph. Returns expected state changes and confidence.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "graph_json": {"type": "string", "description": "Causal graph JSON (from build_causal_graph)"},
                "action": {"type": "string", "description": "Action to predict outcome for"},
            },
            "required": ["graph_json", "action"],
        },
    },
    {
        "name": "find_safest_path",
        "description": "Find the safest path to a goal state through the causal graph. Avoids high-risk or irreversible actions.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "graph_json": {"type": "string", "description": "Causal graph JSON (from build_causal_graph)"},
                "goal": {"type": "string", "description": "Target goal state"},
            },
            "required": ["graph_json", "goal"],
        },
    },
    # ─── Fas 9b: WebMCP Discovery ───────────────────────────────────────────
    {
        "name": "discover_webmcp",
        "description": "Discover WebMCP tool definitions embedded in an HTML page. Returns structured tool schemas for any MCP-compatible widgets.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string"},
                "url": {"type": "string", "description": "The page URL"},
            },
            "required": ["html", "url"],
        },
    },
    # ─── Fas 9c: Multimodal Grounding ───────────────────────────────────────
    {
        "name": "ground_semantic_tree",
        "description": "Ground a semantic tree with visual bounding box annotations. Maps accessibility nodes to screen coordinates for visual agents.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "html": {"type": "string", "description": "Raw HTML string"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "url": {"type": "string", "description": "The page URL"},
                "annotations": {"type": "array", "description": "Array of bounding box annotations [{x, y, width, height, label}]"},
            },
            "required": ["html", "goal", "url", "annotations"],
        },
    },
    {
        "name": "match_bbox_iou",
        "description": "Match a bounding box against semantic tree nodes using IoU (Intersection over Union). Returns best matching node.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "tree_json": {"type": "string", "description": "Semantic tree JSON"},
                "bbox": {"type": "object", "description": "Bounding box {x, y, width, height}", "properties": {"x": {"type": "number"}, "y": {"type": "number"}, "width": {"type": "number"}, "height": {"type": "number"}}},
            },
            "required": ["tree_json", "bbox"],
        },
    },
    # ─── Fas 9d: Cross-Agent Collaboration ──────────────────────────────────
    {
        "name": "create_collab_store",
        "description": "Create a shared diff store for cross-agent collaboration. Multiple agents can publish and subscribe to semantic deltas.",
        "inputSchema": {
            "type": "object",
            "properties": {},
        },
    },
    {
        "name": "register_collab_agent",
        "description": "Register an agent in a collaboration store. Required before publishing or fetching deltas.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "store_json": {"type": "string", "description": "Collab store JSON (from create_collab_store)"},
                "agent_id": {"type": "string", "description": "Unique agent identifier"},
                "goal": {"type": "string", "description": "The agent's current goal"},
                "timestamp_ms": {"type": "integer", "description": "Current timestamp in milliseconds"},
            },
            "required": ["store_json", "agent_id", "goal", "timestamp_ms"],
        },
    },
    {
        "name": "publish_collab_delta",
        "description": "Publish a semantic delta to the collaboration store for other agents to consume.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "store_json": {"type": "string", "description": "Collab store JSON"},
                "agent_id": {"type": "string", "description": "Publishing agent's ID"},
                "url": {"type": "string", "description": "URL the delta applies to"},
                "delta_json": {"type": "string", "description": "Semantic delta JSON (from diff_trees)"},
                "timestamp_ms": {"type": "integer", "description": "Current timestamp in milliseconds"},
            },
            "required": ["store_json", "agent_id", "url", "delta_json", "timestamp_ms"],
        },
    },
    {
        "name": "fetch_collab_deltas",
        "description": "Fetch new semantic deltas from the collaboration store since this agent's last fetch.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "store_json": {"type": "string", "description": "Collab store JSON"},
                "agent_id": {"type": "string", "description": "Fetching agent's ID"},
            },
            "required": ["store_json", "agent_id"],
        },
    },
]

# Map tool names to HTTP API endpoints and parameter transforms
TOOL_ROUTES = {
    "parse":            ("POST", "/api/parse",            lambda p: p),
    "parse_top":        ("POST", "/api/parse-top",        lambda p: p),
    "fetch_parse":      ("POST", "/api/fetch/parse",      lambda p: p),
    "find_and_click":   ("POST", "/api/click",            lambda p: p),
    "fill_form":        ("POST", "/api/fill-form",        lambda p: {"html": p["html"], "goal": p["goal"], "url": p["url"], "fields_json": json.dumps(p["fields"])}),
    "extract_data":     ("POST", "/api/extract",          lambda p: {"html": p["html"], "goal": p["goal"], "url": p["url"], "keys_json": json.dumps(p["keys"])}),
    "check_injection":  ("POST", "/api/check-injection",  lambda p: p),
    "compile_goal":     ("POST", "/api/compile",          lambda p: p),
    "classify_request": ("POST", "/api/firewall/classify", lambda p: p),
    "diff_trees":       ("POST", "/api/diff",             lambda p: p),
    "fetch_extract":    ("POST", "/api/fetch/extract",    lambda p: {"url": p["url"], "goal": p["goal"], "keys_json": json.dumps(p["keys"])}),
    "fetch_click":      ("POST", "/api/fetch/click",      lambda p: p),
    # Fas 4c
    "parse_with_js":    ("POST", "/api/parse-js",         lambda p: p),
    # Fas 9a
    "build_causal_graph":      ("POST", "/api/causal/build",        lambda p: p),
    "predict_action_outcome":  ("POST", "/api/causal/predict",      lambda p: p),
    "find_safest_path":        ("POST", "/api/causal/safest-path",  lambda p: p),
    # Fas 9b
    "discover_webmcp":         ("POST", "/api/webmcp/discover",     lambda p: p),
    # Fas 9c
    "ground_semantic_tree":    ("POST", "/api/ground",              lambda p: p),
    "match_bbox_iou":          ("POST", "/api/ground/match-bbox",   lambda p: p),
    # Fas 9d
    "create_collab_store":     ("POST", "/api/collab/create",       lambda p: p),
    "register_collab_agent":   ("POST", "/api/collab/register",     lambda p: p),
    "publish_collab_delta":    ("POST", "/api/collab/publish",      lambda p: p),
    "fetch_collab_deltas":     ("POST", "/api/collab/fetch",        lambda p: p),
}

# ─── MCP Protocol (JSON-RPC over stdio) ───────────────────────────────────────

session = requests.Session()
session.headers["Content-Type"] = "application/json"


def send_response(id: Any, result: Any) -> None:
    msg = {"jsonrpc": "2.0", "id": id, "result": result}
    data = json.dumps(msg)
    sys.stdout.write(f"Content-Length: {len(data)}\r\n\r\n{data}")
    sys.stdout.flush()


def send_error(id: Any, code: int, message: str) -> None:
    msg = {"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}
    data = json.dumps(msg)
    sys.stdout.write(f"Content-Length: {len(data)}\r\n\r\n{data}")
    sys.stdout.flush()


def handle_initialize(id: Any, _params: dict) -> None:
    send_response(id, {
        "protocolVersion": "2024-11-05",
        "capabilities": {"tools": {"listChanged": False}},
        "serverInfo": {
            "name": "aether-agent",
            "version": "0.3.0",
        },
    })


def handle_tools_list(id: Any, _params: dict) -> None:
    send_response(id, {"tools": TOOLS})


def handle_tools_call(id: Any, params: dict) -> None:
    tool_name = params.get("name", "")
    arguments = params.get("arguments", {})

    route = TOOL_ROUTES.get(tool_name)
    if not route:
        send_error(id, -32602, f"Unknown tool: {tool_name}")
        return

    method, path, transform = route
    try:
        body = transform(arguments)
        resp = session.request(method, f"{AETHER_URL}{path}", json=body, timeout=30)
        result_text = resp.text
        if resp.status_code >= 400:
            send_response(id, {
                "content": [{"type": "text", "text": f"Error {resp.status_code}: {result_text}"}],
                "isError": True,
            })
        else:
            send_response(id, {
                "content": [{"type": "text", "text": result_text}],
            })
    except requests.exceptions.ConnectionError:
        send_response(id, {
            "content": [{"type": "text", "text": f"Connection error: cannot reach {AETHER_URL}. Is the server running?"}],
            "isError": True,
        })
    except Exception as e:
        send_response(id, {
            "content": [{"type": "text", "text": f"Error: {e}"}],
            "isError": True,
        })


def read_message() -> dict | None:
    """Read a JSON-RPC message from stdin (Content-Length framing)."""
    headers = {}
    while True:
        line = sys.stdin.readline()
        if not line:
            return None
        line = line.strip()
        if line == "":
            break
        if ":" in line:
            key, value = line.split(":", 1)
            headers[key.strip().lower()] = value.strip()

    length = int(headers.get("content-length", 0))
    if length == 0:
        return None
    body = sys.stdin.read(length)
    return json.loads(body)


HANDLERS = {
    "initialize": handle_initialize,
    "tools/list": handle_tools_list,
    "tools/call": handle_tools_call,
}


def main() -> None:
    sys.stderr.write(f"AetherAgent MCP Proxy starting (server: {AETHER_URL})\n")

    # Verify server is reachable
    try:
        health = session.get(f"{AETHER_URL}/health", timeout=10).json()
        sys.stderr.write(f"Connected to AetherAgent v{health.get('version', '?')}\n")
    except Exception as e:
        sys.stderr.write(f"Warning: cannot reach {AETHER_URL}: {e}\n")

    while True:
        msg = read_message()
        if msg is None:
            break

        method = msg.get("method", "")
        msg_id = msg.get("id")
        params = msg.get("params", {})

        # Notifications (no id) — just acknowledge
        if msg_id is None:
            if method == "notifications/initialized":
                sys.stderr.write("Client initialized\n")
            continue

        handler = HANDLERS.get(method)
        if handler:
            handler(msg_id, params)
        else:
            send_error(msg_id, -32601, f"Method not found: {method}")


if __name__ == "__main__":
    main()
