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

  /** Wrap content in untrusted content markers for LLM safety */
  wrapUntrusted(content) {
    return this.wasm.wrap_untrusted(content);
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

module.exports = { AetherAgent };
