#!/usr/bin/env python3
"""
Goal Expansion Benchmark — LLM-expanded goals vs raw goals on live sites.
Tests 12 sites × 2 modes (raw vs expanded) = 24 measurements.
"""
import json, time, urllib.request, sys

AE = "http://127.0.0.1:3000"

def ae_post(ep, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(f"{AE}{ep}", data=data, headers={"Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read().decode())

def fetch(url):
    try:
        r = ae_post("/api/fetch", {"url": url})
        return r.get("body", "")
    except:
        return ""

def parse_hybrid(html, goal, url, top_n=8):
    try:
        return ae_post("/api/parse-hybrid", {"html": html, "goal": goal, "url": url, "top_n": top_n})
    except Exception as e:
        return {"error": str(e)}

# ── Test cases with LLM-expanded goals ──

TESTS = [
    {
        "name": "Hjo kommun",
        "url": "https://www.hjo.se",
        "raw_goal": "hur många bor i Hjo",
        "expanded_goal": "hur många bor i Hjo invånare befolkning folkmängd 14000 9000 Hjo kommun centralort",
        "answer_keywords": ["invånare", "befolkning", "14"],
    },
    {
        "name": "GOV.UK Wage",
        "url": "https://www.gov.uk/national-minimum-wage-rates",
        "raw_goal": "minimum wage 2025",
        "expanded_goal": "minimum wage 2025 National Living Wage hourly rate per hour £12.21 £12.71 April 2025 pay workers apprentice aged",
        "answer_keywords": ["£12", "per hour", "hourly"],
    },
    {
        "name": "Bank of England",
        "url": "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate",
        "raw_goal": "current interest rate",
        "expanded_goal": "current interest rate Bank Rate percentage monetary policy MPC base rate decision February 2025 inflation target",
        "answer_keywords": ["Bank Rate", "%", "3.75"],
    },
    {
        "name": "Hacker News",
        "url": "https://news.ycombinator.com",
        "raw_goal": "find latest news articles",
        "expanded_goal": "find latest news articles top stories submissions points comments hours ago hide Hacker News",
        "answer_keywords": ["points", "comments", "ago"],
    },
    {
        "name": "rust-lang.org",
        "url": "https://www.rust-lang.org",
        "raw_goal": "download and install Rust",
        "expanded_goal": "download install Rust rustup curl sh cargo version 1.78 stable toolchain rustc compiler",
        "answer_keywords": ["install", "rustup", "cargo", "getting started"],
    },
    {
        "name": "lobste.rs",
        "url": "https://lobste.rs",
        "raw_goal": "find technology articles",
        "expanded_goal": "find technology articles programming stories submissions comments points hours ago lobste.rs tags",
        "answer_keywords": ["programming", "article", "stories"],
    },
    {
        "name": "PyPI",
        "url": "https://pypi.org",
        "raw_goal": "find Python packages",
        "expanded_goal": "find Python packages pip install PyPI package index repository 500000 projects download",
        "answer_keywords": ["python", "package", "install", "pip"],
    },
    {
        "name": "MDN HTML",
        "url": "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "raw_goal": "HTML elements reference",
        "expanded_goal": "HTML elements reference MDN Web Docs element tag attribute global semantic heading paragraph div span a img",
        "answer_keywords": ["element", "html", "reference", "tag"],
    },
    {
        "name": "Docker Hub",
        "url": "https://hub.docker.com",
        "raw_goal": "search container images",
        "expanded_goal": "search container images Docker Hub registry pull docker.io official image alpine ubuntu nginx postgres",
        "answer_keywords": ["docker", "image", "container", "pull"],
    },
    {
        "name": "Wikipedia Light Speed",
        "url": "https://en.m.wikipedia.org/wiki/Speed_of_light",
        "raw_goal": "speed of light value",
        "expanded_goal": "speed of light value 299792458 meters per second m/s c constant vacuum exactly defined",
        "answer_keywords": ["299", "meters", "second", "m/s"],
    },
    {
        "name": "Tibro kommun",
        "url": "https://www.tibro.se",
        "raw_goal": "nyheter Tibro",
        "expanded_goal": "nyheter Tibro kommun senaste mars april 2026 2025 nyhet rubrik händelse",
        "answer_keywords": ["nyheter", "nyhet", "mars", "april", "2026", "2025"],
    },
    {
        "name": "pkg.go.dev",
        "url": "https://pkg.go.dev",
        "raw_goal": "Go packages and modules",
        "expanded_goal": "Go packages modules Golang standard library pkg.go.dev import net/http fmt io module",
        "answer_keywords": ["go", "package", "module", "library"],
    },
]

print("=" * 90)
print("  GOAL EXPANSION BENCHMARK — Raw vs LLM-Expanded Goals")
print("  12 sites × 2 modes = 24 measurements")
print("=" * 90)
print()

results = []

for tc in TESTS:
    name = tc["name"]
    print(f"━━━ {name} ({tc['url']}) ━━━")

    html = fetch(tc["url"])
    if len(html) < 200:
        print(f"  FETCH FAIL ({len(html)} bytes)\n")
        results.append({"name": name, "fetched": False})
        continue

    html_tokens = len(html) // 4

    for mode in ["raw", "expanded"]:
        goal = tc["raw_goal"] if mode == "raw" else tc["expanded_goal"]
        label = "RAW     " if mode == "raw" else "EXPANDED"

        t0 = time.monotonic()
        data = parse_hybrid(html, goal, tc["url"], 8)
        ms = (time.monotonic() - t0) * 1000

        if "error" in data:
            print(f"  {label}: ERROR {data['error']}")
            continue

        pipeline = data.get("pipeline", {})
        method = pipeline.get("method", "?")
        bm25_cand = pipeline.get("bm25_candidates", 0)
        hdc_surv = pipeline.get("hdc_survivors", 0)
        total_nodes = data.get("total_nodes", 0)
        nodes = data.get("top_nodes", [])

        # Check if any answer keyword appears in top-3
        top3_text = " ".join(n.get("label", "") for n in nodes[:3]).lower()
        hits = [kw for kw in tc["answer_keywords"] if kw.lower() in top3_text]
        p_at_3 = len(hits) / len(tc["answer_keywords"]) if tc["answer_keywords"] else 0

        print(f"  {label}: {ms:.0f}ms | BM25:{bm25_cand} HDC:{hdc_surv} DOM:{total_nodes} | P@3:{p_at_3:.2f} [{method}]")
        for i, n in enumerate(nodes[:3]):
            lbl = n.get("label", "")[:85]
            print(f"    {i+1}. [{n.get('relevance',0):.3f}] {n.get('role','?'):10} {lbl}")

        results.append({
            "name": name,
            "mode": mode,
            "ms": round(ms),
            "bm25_candidates": bm25_cand,
            "hdc_survivors": hdc_surv,
            "total_nodes": total_nodes,
            "p_at_3": round(p_at_3, 2),
            "top1_score": round(nodes[0]["relevance"], 3) if nodes else 0,
            "top1_role": nodes[0].get("role", "?") if nodes else "?",
            "top1_label": nodes[0].get("label", "")[:60] if nodes else "",
            "method": method,
            "fetched": True,
        })

    print()

# ── Summary ──
print("=" * 90)
print("  SUMMARY")
print("=" * 90)
print()

raw_results = [r for r in results if r.get("fetched") and r.get("mode") == "raw"]
exp_results = [r for r in results if r.get("fetched") and r.get("mode") == "expanded"]

if raw_results and exp_results:
    raw_p3 = sum(r["p_at_3"] for r in raw_results) / len(raw_results)
    exp_p3 = sum(r["p_at_3"] for r in exp_results) / len(exp_results)
    raw_ms = sum(r["ms"] for r in raw_results) / len(raw_results)
    exp_ms = sum(r["ms"] for r in exp_results) / len(exp_results)
    raw_cand = sum(r["bm25_candidates"] for r in raw_results) / len(raw_results)
    exp_cand = sum(r["bm25_candidates"] for r in exp_results) / len(exp_results)
    raw_surv = sum(r["hdc_survivors"] for r in raw_results) / len(raw_results)
    exp_surv = sum(r["hdc_survivors"] for r in exp_results) / len(exp_results)
    raw_top1 = sum(r["top1_score"] for r in raw_results) / len(raw_results)
    exp_top1 = sum(r["top1_score"] for r in exp_results) / len(exp_results)

    print(f"{'Metric':<25} {'Raw':>10} {'Expanded':>10} {'Delta':>10}")
    print("-" * 58)
    print(f"{'Avg P@3':<25} {raw_p3:>10.2f} {exp_p3:>10.2f} {exp_p3-raw_p3:>+10.2f}")
    print(f"{'Avg latency (ms)':<25} {raw_ms:>10.0f} {exp_ms:>10.0f} {exp_ms-raw_ms:>+10.0f}")
    print(f"{'Avg BM25 candidates':<25} {raw_cand:>10.0f} {exp_cand:>10.0f} {exp_cand-raw_cand:>+10.0f}")
    print(f"{'Avg HDC survivors':<25} {raw_surv:>10.0f} {exp_surv:>10.0f} {exp_surv-raw_surv:>+10.0f}")
    print(f"{'Avg top-1 score':<25} {raw_top1:>10.3f} {exp_top1:>10.3f} {exp_top1-raw_top1:>+10.3f}")
    print()

    # Per-site comparison
    print(f"{'Site':<20} {'Raw P@3':>8} {'Exp P@3':>8} {'Delta':>8} {'Raw BM25':>9} {'Exp BM25':>9} {'Δ BM25':>7}")
    print("-" * 72)
    for raw, exp in zip(raw_results, exp_results):
        if raw["name"] == exp["name"]:
            d_p3 = exp["p_at_3"] - raw["p_at_3"]
            d_bm25 = exp["bm25_candidates"] - raw["bm25_candidates"]
            marker = " ✓" if d_p3 > 0 else " =" if d_p3 == 0 else " ✗"
            print(f"{raw['name']:<20} {raw['p_at_3']:>8.2f} {exp['p_at_3']:>8.2f} {d_p3:>+8.2f}{marker} {raw['bm25_candidates']:>9} {exp['bm25_candidates']:>9} {d_bm25:>+7}")

json.dump(results, open("benches/expansion_benchmark_results.json", "w"), indent=2)
print(f"\nResults saved: benches/expansion_benchmark_results.json")
