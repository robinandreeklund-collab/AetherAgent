"""
AetherAgent Python SDK

Provides both native HTTP client (for deployed server) and WASM runtime.

Usage (HTTP – recommended for production):
    from aether_agent import AetherAgent
    agent = AetherAgent(base_url="https://your-render-url.onrender.com")
    tree = agent.parse(html, goal="buy cheapest flight", url="https://shop.se")

Usage (WASM – local development):
    from aether_agent import AetherAgentWasm
    agent = AetherAgentWasm()
    tree = agent.parse(html, goal="buy cheapest flight", url="https://shop.se")
"""

import json
from typing import Optional
from pathlib import Path

try:
    import requests as _requests
except ImportError:
    _requests = None


class AetherAgent:
    """HTTP client for deployed AetherAgent API server."""

    def __init__(self, base_url: str = "http://localhost:3000"):
        self.base_url = base_url.rstrip("/")
        if _requests is None:
            raise ImportError("requests krävs: pip install requests")

    def _post(self, path: str, data: dict) -> dict:
        resp = _requests.post(
            f"{self.base_url}{path}",
            json=data,
            timeout=30,
        )
        resp.raise_for_status()
        return resp.json()

    def _get(self, path: str) -> dict:
        resp = _requests.get(f"{self.base_url}{path}", timeout=10)
        resp.raise_for_status()
        return resp.json()

    def health(self) -> dict:
        """Health check – verify server is running."""
        return self._get("/health")

    def parse(self, html: str, goal: str, url: str) -> dict:
        """Parse HTML to full semantic tree with goal-relevance scoring."""
        return self._post("/api/parse", {"html": html, "goal": goal, "url": url})

    def parse_top(self, html: str, goal: str, url: str, top_n: int = 10) -> dict:
        """Parse and return only the top-N most relevant nodes."""
        return self._post("/api/parse/top", {"html": html, "goal": goal, "url": url, "top_n": top_n})

    def find_and_click(self, html: str, goal: str, url: str, target_label: str) -> dict:
        """Find the best clickable element matching a target label."""
        return self._post("/api/click", {"html": html, "goal": goal, "url": url, "target_label": target_label})

    def fill_form(self, html: str, goal: str, url: str, fields: dict) -> dict:
        """Map form fields to provided key/value pairs."""
        return self._post("/api/fill-form", {"html": html, "goal": goal, "url": url, "fields": fields})

    def extract_data(self, html: str, goal: str, url: str, keys: list) -> dict:
        """Extract structured data by semantic keys."""
        return self._post("/api/extract", {"html": html, "goal": goal, "url": url, "keys": keys})

    def check_injection(self, text: str) -> dict:
        """Check text for prompt injection patterns."""
        return self._post("/api/check-injection", {"text": text})

    def wrap_untrusted(self, content: str) -> str:
        """Wrap content in untrusted content markers."""
        resp = _requests.post(
            f"{self.base_url}/api/wrap-untrusted",
            json={"content": content},
            timeout=10,
        )
        resp.raise_for_status()
        return resp.text

    def create_memory(self) -> dict:
        """Create a new empty workflow memory."""
        resp = _requests.post(f"{self.base_url}/api/memory/create", timeout=10)
        resp.raise_for_status()
        return resp.json()

    def add_step(self, memory: dict, action: str, url: str, goal: str, summary: str) -> dict:
        """Add a step to workflow memory."""
        memory_json = json.dumps(memory) if isinstance(memory, dict) else memory
        return self._post("/api/memory/step", {
            "memory_json": memory_json, "action": action,
            "url": url, "goal": goal, "summary": summary,
        })

    def set_context(self, memory: dict, key: str, value: str) -> dict:
        """Set a context key/value in workflow memory."""
        memory_json = json.dumps(memory) if isinstance(memory, dict) else memory
        return self._post("/api/memory/context/set", {
            "memory_json": memory_json, "key": key, "value": value,
        })

    def get_context(self, memory: dict, key: str) -> dict:
        """Get a context value from workflow memory."""
        memory_json = json.dumps(memory) if isinstance(memory, dict) else memory
        return self._post("/api/memory/context/get", {
            "memory_json": memory_json, "key": key,
        })


class AetherAgentWasm:
    """Direct WASM runtime via wasmtime (no server needed)."""

    def __init__(self, wasm_path: Optional[str] = None):
        try:
            from wasmtime import Store, Module, Linker, WasiConfig
        except ImportError:
            raise ImportError("wasmtime krävs: pip install wasmtime")

        if wasm_path is None:
            wasm_path = str(Path(__file__).parent.parent.parent / "pkg" / "aether_agent_bg.wasm")

        if not Path(wasm_path).exists():
            raise FileNotFoundError(
                f"WASM-fil saknas: {wasm_path}\n"
                "Bygg först med: wasm-pack build --target web --release"
            )

        self.store = Store()
        wasi = WasiConfig()
        wasi.inherit_stdout()
        wasi.inherit_stderr()
        self.store.set_wasi(wasi)

        linker = Linker(self.store.engine)
        linker.define_wasi()

        module = Module.from_file(self.store.engine, wasm_path)
        self.instance = linker.instantiate(self.store, module)

    def _call(self, fn_name: str, *args) -> str:
        fn = self.instance.exports(self.store)[fn_name]
        return fn(self.store, *args)

    def health(self) -> dict:
        return json.loads(self._call("health_check"))

    def parse(self, html: str, goal: str, url: str) -> dict:
        return json.loads(self._call("parse_to_semantic_tree", html, goal, url))

    def parse_top(self, html: str, goal: str, url: str, top_n: int = 10) -> dict:
        return json.loads(self._call("parse_top_nodes", html, goal, url, top_n))

    def find_and_click(self, html: str, goal: str, url: str, target_label: str) -> dict:
        return json.loads(self._call("find_and_click", html, goal, url, target_label))

    def fill_form(self, html: str, goal: str, url: str, fields: dict) -> dict:
        return json.loads(self._call("fill_form", html, goal, url, json.dumps(fields)))

    def extract_data(self, html: str, goal: str, url: str, keys: list) -> dict:
        return json.loads(self._call("extract_data", html, goal, url, json.dumps(keys)))

    def check_injection(self, text: str) -> dict:
        return json.loads(self._call("check_injection", text))

    def wrap_untrusted(self, content: str) -> str:
        return self._call("wrap_untrusted", content)


if __name__ == "__main__":
    print("AetherAgent Python SDK")
    print("Usage:")
    print("  HTTP:  agent = AetherAgent('https://your-url.onrender.com')")
    print("  WASM:  agent = AetherAgentWasm()")
