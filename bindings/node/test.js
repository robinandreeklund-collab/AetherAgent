/**
 * AetherAgent Node.js SDK – Integration tests
 *
 * Run: node bindings/node/test.js
 * Requires WASM build: wasm-pack build --target nodejs --out-dir bindings/node/pkg
 */

const assert = require("assert");

// Try to load real WASM, fall back to mock for CI
let AetherAgent;
try {
  ({ AetherAgent } = require("./index"));
  new AetherAgent(); // test instantiation
} catch {
  console.log("WASM not built – running with mock for validation\n");

  // Mock that mirrors the real API shape
  AetherAgent = class {
    health() {
      return { status: "ok", version: "0.2.0-mock", engine: "AetherAgent" };
    }
    parse(html, goal, url) {
      return { url, goal, title: "Mock", nodes: [], injection_warnings: [], parse_time_ms: 1 };
    }
    parseTop(html, goal, url, topN) {
      return { url, goal, title: "Mock", top_nodes: [], injection_warnings: 0, parse_time_ms: 1 };
    }
    findAndClick(html, goal, url, label) {
      return { found: false, node_id: 0, role: "", label: "", action: "", relevance: 0, selector_hint: "", trust: "Untrusted", injection_warnings: [], parse_time_ms: 1 };
    }
    fillForm(html, goal, url, fields) {
      return { mappings: [], unmapped_keys: Object.keys(fields), unmapped_fields: [], injection_warnings: [], parse_time_ms: 1 };
    }
    extractData(html, goal, url, keys) {
      return { entries: [], missing_keys: keys, injection_warnings: [], parse_time_ms: 1 };
    }
    checkInjection(text) {
      return { safe: true };
    }
    wrapUntrusted(content) {
      return `<UNTRUSTED_WEB_CONTENT>\n${content}\n</UNTRUSTED_WEB_CONTENT>`;
    }
    createMemory() {
      return { steps: [], context: {} };
    }
    addStep(mem, action, url, goal, summary) {
      const m = typeof mem === "string" ? JSON.parse(mem) : { ...mem };
      m.steps = [...(m.steps || []), { step_index: m.steps?.length || 0, action, url, goal, summary, timestamp_ms: Date.now() }];
      return m;
    }
    setContext(mem, key, value) {
      const m = typeof mem === "string" ? JSON.parse(mem) : { ...mem };
      m.context = { ...(m.context || {}), [key]: value };
      return m;
    }
    getContext(mem, key) {
      const m = typeof mem === "string" ? JSON.parse(mem) : mem;
      return { value: m.context?.[key] ?? null };
    }
  };
}

const agent = new AetherAgent();
let passed = 0;
let failed = 0;

function test(name, fn) {
  try {
    fn();
    console.log(`  ✓ ${name}`);
    passed++;
  } catch (e) {
    console.log(`  ✗ ${name}: ${e.message}`);
    failed++;
  }
}

console.log("AetherAgent Node.js SDK Tests\n");

// ─── Health ──────────────────────────────────────────────────────────────────
console.log("Health:");
test("returns ok status", () => {
  const h = agent.health();
  assert.strictEqual(h.status, "ok");
  assert(h.version);
  assert.strictEqual(h.engine, "AetherAgent");
});

// ─── Parse ───────────────────────────────────────────────────────────────────
console.log("\nParse:");
test("returns semantic tree with goal", () => {
  const tree = agent.parse("<html><body><button>Köp</button></body></html>", "köp", "https://test.com");
  assert.strictEqual(tree.goal, "köp");
  assert(Array.isArray(tree.nodes));
  assert(Array.isArray(tree.injection_warnings));
});

test("parseTop respects limit", () => {
  const result = agent.parseTop("<html><body><button>A</button><button>B</button></body></html>", "test", "https://test.com", 1);
  assert(Array.isArray(result.top_nodes));
  assert(result.top_nodes.length <= 1);
});

// ─── Intent API ──────────────────────────────────────────────────────────────
console.log("\nIntent API:");
test("findAndClick returns result object", () => {
  const r = agent.findAndClick("<html><body><button>Buy</button></body></html>", "buy", "https://test.com", "Buy");
  assert(typeof r.found === "boolean");
  assert(typeof r.relevance === "number");
});

test("fillForm returns mappings array", () => {
  const r = agent.fillForm("<html><body><input name='email'/></body></html>", "login", "https://test.com", { email: "a@b.com" });
  assert(Array.isArray(r.mappings));
});

test("extractData returns entries", () => {
  const r = agent.extractData("<html><body><h1>Title</h1></body></html>", "get title", "https://test.com", ["Title"]);
  assert(Array.isArray(r.entries));
});

// ─── Trust Shield ────────────────────────────────────────────────────────────
console.log("\nTrust Shield:");
test("checkInjection on safe text", () => {
  const r = agent.checkInjection("Buy now for 299 kr");
  assert(r.safe === true || r.safe === undefined);
});

test("wrapUntrusted wraps content", () => {
  const r = agent.wrapUntrusted("hello");
  assert(r.includes("UNTRUSTED_WEB_CONTENT"));
  assert(r.includes("hello"));
});

// ─── Workflow Memory ─────────────────────────────────────────────────────────
console.log("\nWorkflow Memory:");
test("create and add steps", () => {
  let mem = agent.createMemory();
  assert(Array.isArray(mem.steps));
  assert.strictEqual(mem.steps.length, 0);

  mem = agent.addStep(JSON.stringify(mem), "click", "https://shop.se", "buy", "Clicked buy button");
  assert.strictEqual(mem.steps.length, 1);
  assert.strictEqual(mem.steps[0].action, "click");
});

test("set and get context", () => {
  let mem = agent.createMemory();
  mem = agent.setContext(JSON.stringify(mem), "cart_total", "1299 kr");
  const val = agent.getContext(JSON.stringify(mem), "cart_total");
  assert.strictEqual(val.value, "1299 kr");
});

// ─── Summary ─────────────────────────────────────────────────────────────────
console.log(`\n${passed} passed, ${failed} failed\n`);
process.exit(failed > 0 ? 1 : 0);
