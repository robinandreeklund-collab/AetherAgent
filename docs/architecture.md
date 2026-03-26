# Architecture

> System architecture and design overview for AetherAgent.
> For a summary, see the [main README](../README.md).

---

## System Diagram

```
┌───────────────────────────────────────────────────────────────────┐
│               LLM Agent (Claude / GPT / Llama / Gemini)           │
│            Receives semantic JSON → reasons → acts                │
└──────────────────────────────┬────────────────────────────────────┘
                               │ goal-aware JSON (200 tokens)
┌──────────────────────────────▼────────────────────────────────────┐
│                    AetherAgent Core (Rust → WASM)                 │
│                                                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ Progressive Escalation — auto-select Tier 0→4 per page  │   │
│  └──────────────────────────────────────────────────────────┘   │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │ Parser   │ │ Arena DOM │ │  Trust   │ │   Intent API     │   │
│  │ html5ever│ │ SlotMap   │ │  Shield  │ │ click/fill/      │   │
│  │ →ArenaDom│ │ semantic  │ │ 20+      │ │ extract          │   │
│  │          │ │ builder   │ │ patterns │ │                  │   │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │ Diff     │ │ JS Sandbox│ │ Temporal │ │   Compiler       │   │
│  │ 80-95%   │ │ QuickJS   │ │ Memory & │ │ goal → plan →    │   │
│  │ token    │ │ bridge    │ │ Adversar.│ │ execute          │   │
│  │ savings  │ │           │ │ Detection│ │                  │   │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │ Hydration│ │ Firewall  │ │ Causal   │ │   Grounding      │   │
│  │ Tier 0   │ │ L1/L2/L3  │ │ Action   │ │ BBox + IoU +     │   │
│  │ 10 SSR   │ │ goal-aware│ │ Graph    │ │ Set-of-Mark      │   │
│  │ frameworks│ │ filtering │ │          │ │                  │   │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────┐ ┌───────────┐ ┌──────────┐ ┌──────────────────┐   │
│  │ Fetch    │ │ Collab    │ │ XHR      │ │   Vision         │   │
│  │ HTTP     │ │ Cross-    │ │ Intercept│ │ YOLOv8-nano      │   │
│  │ cookies  │ │ Agent     │ │ fetch/xhr│ │ ONNX Runtime     │   │
│  │ SSRF prot│ │           │ │          │ │                  │   │
│  └──────────┘ └───────────┘ └──────────┘ └──────────────────┘   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │ Blitz Renderer — pure Rust CSS layout → PNG screenshots  │   │
│  └──────────────────────────────────────────────────────────┘   │
│                                                                   │
│              28 modules · 62 WASM functions                       │
│              65 HTTP endpoints · 30 MCP tools                     │
└──────────────────────────────┬────────────────────────────────────┘
                               │
┌──────────────────────────────▼────────────────────────────────────┐
│                   Runtime (zero vendor lock-in)                   │
│  WASM (any host)  │  HTTP API (Axum)  │  MCP (stdio)  │  Python  │
│  Node.js          │  Cloudflare Workers│  Claude Desktop│  SDK    │
└───────────────────────────────────────────────────────────────────┘
```

---

## Web Coverage Tiers

AetherAgent uses a tiered architecture that automatically selects the fastest technique per page:

| Tier | Technique | Coverage | Latency |
|------|-----------|----------|---------|
| T0 | SSR hydration extraction | ~25% | ~0 ms |
| T1 | Static HTML parsing | ~30% | ~1 ms |
| T2 | QuickJS sandbox + DOM | ~25% | ~10–50 ms |
| T3 | Blitz render (pure Rust) | included above | ~10–50 ms |
| T4 | CDP fallback (Chrome) | ~15% | ~2–5 s |

**~80% of the web** is handled in under 50 ms with no browser process. The remaining ~15% falls back to CDP. ~5% requires full rendering engines (WebGL, Canvas, WASM-heavy apps).

---

## Honest Positioning

AetherAgent is **not** a Chrome replacement. It fetches pages and builds semantic trees, renders via Blitz (pure Rust), but does not run V8. For JS-heavy SPAs, pair it with a headless browser for rendering, then feed HTML to AetherAgent.

**When to use AetherAgent:** Fast, end-to-end browser engine — fetch, parse, plan, detect injection — with no browser overhead.

**When to use Playwright/Browser Use:** Full JavaScript execution: SPAs, visual rendering, CDP automation.

**Best of both:** Fetch with a browser, perceive with AetherAgent.

---

## Comparison

| Capability | AetherAgent | Playwright | Browser Use | Scrapy |
|-----------|:-----------:|:----------:|:-----------:|:------:|
| Semantic tree with goal scoring | **Yes** | No | Partial | No |
| Prompt injection protection | **Yes** | No | No | No |
| Startup time | <1 ms | ~2,000 ms | ~3,000 ms | ~50 ms |
| Memory per instance | ~27 MB | ~150 MB | ~200 MB | ~30 MB |
| Full JavaScript (V8) | No | Yes | Yes | No |
| Embeddable in WASM | **Yes** | No | No | No |
| MCP server built-in | **Yes** | No | No | No |
