/**
 * AetherAgent Node.js SDK
 *
 * Wraps the WASM module with a friendly API.
 * Build WASM first: wasm-pack build --target nodejs --out-dir bindings/node/pkg
 */

let wasmModule = null;

function loadWasm() {
  if (wasmModule) return wasmModule;
  try {
    wasmModule = require("./pkg/aether_agent");
    return wasmModule;
  } catch {
    throw new Error(
      "WASM module not found. Build first:\n" +
        "  wasm-pack build --target nodejs --out-dir bindings/node/pkg"
    );
  }
}

/**
 * AetherAgent – LLM-native browser engine for Node.js
 *
 * @example
 * const { AetherAgent } = require('@aether-agent/node');
 * const agent = new AetherAgent();
 * const tree = agent.parse(html, 'buy cheapest flight', url);
 */
class AetherAgent {
  constructor() {
    this.wasm = loadWasm();
  }

  /** Health check – verify WASM module loaded correctly */
  health() {
    return JSON.parse(this.wasm.health_check());
  }

  /** Parse HTML to full semantic tree with goal-relevance scoring */
  parse(html, goal, url) {
    return JSON.parse(this.wasm.parse_to_semantic_tree(html, goal, url));
  }

  /** Parse and return only the top-N most relevant nodes */
  parseTop(html, goal, url, topN = 10) {
    return JSON.parse(this.wasm.parse_top_nodes(html, goal, url, topN));
  }

  /** Find the best clickable element matching a target label */
  findAndClick(html, goal, url, targetLabel) {
    return JSON.parse(
      this.wasm.find_and_click(html, goal, url, targetLabel)
    );
  }

  /** Map form fields to provided key/value pairs */
  fillForm(html, goal, url, fields) {
    return JSON.parse(
      this.wasm.fill_form(html, goal, url, JSON.stringify(fields))
    );
  }

  /** Extract structured data by semantic keys */
  extractData(html, goal, url, keys) {
    return JSON.parse(
      this.wasm.extract_data(html, goal, url, JSON.stringify(keys))
    );
  }

  /** Check text for prompt injection patterns */
  checkInjection(text) {
    return JSON.parse(this.wasm.check_injection(text));
  }

  /** Compare two semantic trees and return only the changes (delta) */
  diffTrees(oldTreeJson, newTreeJson) {
    const oldJson = typeof oldTreeJson === "string" ? oldTreeJson : JSON.stringify(oldTreeJson);
    const newJson = typeof newTreeJson === "string" ? newTreeJson : JSON.stringify(newTreeJson);
    return JSON.parse(this.wasm.diff_semantic_trees(oldJson, newJson));
  }

  /** Detect JavaScript snippets in HTML that may affect page content */
  detectJs(html) {
    return JSON.parse(this.wasm.detect_js(html));
  }

  /** Evaluate a JavaScript expression in a sandboxed environment */
  evalJs(code) {
    return JSON.parse(this.wasm.eval_js(code));
  }

  /** Evaluate multiple JS expressions in sequence */
  evalJsBatch(snippets) {
    return JSON.parse(this.wasm.eval_js_batch(JSON.stringify(snippets)));
  }

  /** Parse HTML with automatic JS detection, evaluation, and application to semantic tree */
  parseWithJs(html, goal, url) {
    return JSON.parse(this.wasm.parse_with_js(html, goal, url));
  }

  /** Wrap content in untrusted content markers for LLM safety */
  wrapUntrusted(content) {
    return this.wasm.wrap_untrusted(content);
  }

  // ─── Fas 8: Semantic Firewall ───────────────────────────────────────

  /** Classify a URL against the semantic firewall (L1/L2/L3) */
  classifyRequest(url, goal, configJson = "{}") {
    const config = typeof configJson === "string" ? configJson : JSON.stringify(configJson);
    return JSON.parse(this.wasm.classify_request(url, goal, config));
  }

  /** Classify a batch of URLs against the semantic firewall */
  classifyRequestBatch(urls, goal, configJson = "{}") {
    const urlsJson = JSON.stringify(urls);
    const config = typeof configJson === "string" ? configJson : JSON.stringify(configJson);
    return JSON.parse(this.wasm.classify_request_batch(urlsJson, goal, config));
  }

  // ─── Fas 5: Temporal Memory ─────────────────────────────────────────

  /** Create a new empty temporal memory for tracking page state over time */
  createTemporalMemory() {
    return JSON.parse(this.wasm.create_temporal_memory());
  }

  /** Add a snapshot of the current page state to temporal memory */
  addTemporalSnapshot(memoryJson, html, goal, url, timestampMs) {
    const mem = typeof memoryJson === "string" ? memoryJson : JSON.stringify(memoryJson);
    return JSON.parse(this.wasm.add_temporal_snapshot(mem, html, goal, url, timestampMs));
  }

  /** Analyze temporal memory for adversarial patterns and volatility */
  analyzeTemporal(memoryJson) {
    const mem = typeof memoryJson === "string" ? memoryJson : JSON.stringify(memoryJson);
    return JSON.parse(this.wasm.analyze_temporal(mem));
  }

  /** Predict next page state based on temporal history */
  predictTemporal(memoryJson) {
    const mem = typeof memoryJson === "string" ? memoryJson : JSON.stringify(memoryJson);
    return JSON.parse(this.wasm.predict_temporal(mem));
  }

  // ─── Fas 6: Intent Compiler ─────────────────────────────────────────

  /** Compile a goal into an optimized action plan */
  compileGoal(goal) {
    return JSON.parse(this.wasm.compile_goal(goal));
  }

  /** Execute an action plan against current page state */
  executePlan(planJson, html, goal, url, completedSteps = []) {
    const plan = typeof planJson === "string" ? planJson : JSON.stringify(planJson);
    return JSON.parse(
      this.wasm.execute_plan(plan, html, goal, url, JSON.stringify(completedSteps))
    );
  }

  /** Create a new empty workflow memory */
  createMemory() {
    return JSON.parse(this.wasm.create_workflow_memory());
  }

  /** Add a step to workflow memory, returns updated memory */
  addStep(memoryJson, action, url, goal, summary) {
    const mem =
      typeof memoryJson === "string"
        ? memoryJson
        : JSON.stringify(memoryJson);
    return JSON.parse(this.wasm.add_workflow_step(mem, action, url, goal, summary));
  }

  /** Set a context key/value in workflow memory */
  setContext(memoryJson, key, value) {
    const mem =
      typeof memoryJson === "string"
        ? memoryJson
        : JSON.stringify(memoryJson);
    return JSON.parse(this.wasm.set_workflow_context(mem, key, value));
  }

  /** Get a context value from workflow memory */
  getContext(memoryJson, key) {
    const mem =
      typeof memoryJson === "string"
        ? memoryJson
        : JSON.stringify(memoryJson);
    return JSON.parse(this.wasm.get_workflow_context(mem, key));
  }
}

/**
 * AetherAgent HTTP Client – connects to deployed server.
 * Supports all endpoints including Fas 7 fetch operations.
 *
 * @example
 * const { AetherAgentHTTP } = require('@aether-agent/node');
 * const agent = new AetherAgentHTTP('https://your-url.onrender.com');
 * const result = await agent.fetchParse('https://example.com', 'buy product');
 */
class AetherAgentHTTP {
  constructor(baseUrl = "http://localhost:3000") {
    this.baseUrl = baseUrl.replace(/\/$/, "");
  }

  async _post(path, data) {
    const resp = await fetch(`${this.baseUrl}${path}`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });
    if (!resp.ok) throw new Error(`HTTP ${resp.status}: ${await resp.text()}`);
    return resp.json();
  }

  async health() {
    const resp = await fetch(`${this.baseUrl}/health`);
    return resp.json();
  }

  async parse(html, goal, url) {
    return this._post("/api/parse", { html, goal, url });
  }

  async findAndClick(html, goal, url, targetLabel) {
    return this._post("/api/click", {
      html, goal, url, target_label: targetLabel,
    });
  }

  async compileGoal(goal) {
    return this._post("/api/compile", { goal });
  }

  async executePlan(planJson, html, goal, url, completedSteps = []) {
    const plan = typeof planJson === "string" ? planJson : JSON.stringify(planJson);
    return this._post("/api/execute-plan", {
      plan_json: plan, html, goal, url, completed_steps: completedSteps,
    });
  }

  // ─── Fas 8: Semantic Firewall ────────────────────────────────────

  async classifyRequest(url, goal, config) {
    const data = { url, goal };
    if (config) data.config = config;
    return this._post("/api/firewall/classify", data);
  }

  async classifyRequestBatch(urls, goal, config) {
    const data = { urls, goal };
    if (config) data.config = config;
    return this._post("/api/firewall/classify-batch", data);
  }

  // ─── Fas 7: HTTP Fetch ──────────────────────────────────────────

  async fetchRaw(url, config) {
    const data = { url };
    if (config) data.config = config;
    return this._post("/api/fetch", data);
  }

  async fetchParse(url, goal, config) {
    const data = { url, goal };
    if (config) data.config = config;
    return this._post("/api/fetch/parse", data);
  }

  async fetchClick(url, goal, targetLabel, config) {
    const data = { url, goal, target_label: targetLabel };
    if (config) data.config = config;
    return this._post("/api/fetch/click", data);
  }

  async fetchExtract(url, goal, keys, config) {
    const data = { url, goal, keys };
    if (config) data.config = config;
    return this._post("/api/fetch/extract", data);
  }

  async fetchPlan(url, goal, completedSteps = [], config) {
    const data = { url, goal, completed_steps: completedSteps };
    if (config) data.config = config;
    return this._post("/api/fetch/plan", data);
  }
}

module.exports = { AetherAgent, AetherAgentHTTP };
