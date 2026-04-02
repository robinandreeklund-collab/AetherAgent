#!/usr/bin/env python3
"""
Ablation Study — Remove one component at a time and measure impact.
Required for academic credibility.
"""
import json, time, urllib.request, sys

AE = "http://127.0.0.1:3000"

def ae_post(ep, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(f"{AE}{ep}", data=data, headers={"Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=90) as r:
        return json.loads(r.read().decode())

def fetch(url):
    try: return ae_post("/api/fetch", {"url": url}).get("body", "")
    except: return ""

# Same 10 showcase sites from whitepaper
TESTS = [
    {"name": "GOV.UK Wage", "url": "https://www.gov.uk/national-minimum-wage-rates",
     "goal": "minimum wage 2025 National Living Wage hourly rate per hour £12.21 £12.71 April pay",
     "must_contain": ["£12"]},
    {"name": "Bank of England", "url": "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate",
     "goal": "current interest rate Bank Rate percentage 3.75 monetary policy MPC",
     "must_contain": ["%"]},
    {"name": "Hacker News", "url": "https://news.ycombinator.com",
     "goal": "latest news articles stories submissions points comments hours ago",
     "must_contain": ["points", "comments"]},
    {"name": "PyPI", "url": "https://pypi.org",
     "goal": "find Python packages pip install PyPI package index repository",
     "must_contain": ["python", "package"]},
    {"name": "NASA", "url": "https://www.nasa.gov",
     "goal": "NASA space exploration missions moon Mars Artemis astronaut launch",
     "must_contain": ["nasa"]},
    {"name": "lobste.rs", "url": "https://lobste.rs",
     "goal": "technology articles programming stories submissions comments points hours ago",
     "must_contain": ["hours ago"]},
    {"name": "Tibro kommun", "url": "https://www.tibro.se",
     "goal": "nyheter Tibro kommun senaste mars april 2026 2025 nyhet",
     "must_contain": ["2026"]},
    {"name": "CoinGecko", "url": "https://www.coingecko.com",
     "goal": "cryptocurrency Bitcoin price market cap volume BTC ETH trading USD",
     "must_contain": ["bitcoin", "price"]},
    {"name": "W3Schools", "url": "https://www.w3schools.com/html/",
     "goal": "HTML tutorial learn web development elements tags attributes",
     "must_contain": ["html"]},
    {"name": "Docker Hub", "url": "https://hub.docker.com",
     "goal": "search container images Docker Hub registry pull official alpine ubuntu",
     "must_contain": ["docker", "image"]},
]

# Configurations to test (ablation)
CONFIGS = [
    ("Full system (ColBERT)", None),                    # Default = ColBERT with all opts
    ("MiniLM only (no ColBERT)", "minilm"),             # Baseline: bi-encoder
    ("ColBERT no expansion", None),                      # Will use raw goal
]

print("=" * 90)
print("  ABLATION STUDY — Component-by-component impact")
print("=" * 90)
print()

# Pre-fetch all HTML
print("  Fetching HTML...")
html_cache = {}
for tc in TESTS:
    html = fetch(tc["url"])
    if len(html) > 200:
        html_cache[tc["name"]] = html
        sys.stdout.write(f"    {tc['name']}: {len(html)//1024}KB\n")
    else:
        sys.stdout.write(f"    {tc['name']}: FAIL\n")

print(f"\n  {len(html_cache)} sites fetched.\n")

results = {}

# Test 1: Full system (ColBERT + expansion)
print("━━━ Config: Full system (ColBERT + all optimizations) ━━━")
config_results = []
for tc in TESTS:
    if tc["name"] not in html_cache: continue
    html = html_cache[tc["name"]]
    t0 = time.monotonic()
    data = ae_post("/api/parse-hybrid", {"html": html, "goal": tc["goal"], "url": tc["url"], "top_n": 20})
    ms = (time.monotonic() - t0) * 1000
    nodes = data.get("top_nodes", [])
    top5_text = " ".join(n.get("label","") for n in nodes[:5]).lower()
    found = all(kw.lower() in top5_text for kw in tc["must_contain"])
    config_results.append({"name": tc["name"], "found5": found, "ms": ms, "nodes": len(nodes)})
    mark = "✅" if found else "❌"
    print(f"  {mark} {tc['name']:<20} {ms:>6.0f}ms")
found_count = sum(1 for r in config_results if r["found5"])
avg_ms = sum(r["ms"] for r in config_results) / len(config_results) if config_results else 0
results["full"] = {"recall5": found_count, "total": len(config_results), "avg_ms": avg_ms}
print(f"  Recall@5: {found_count}/{len(config_results)}  Avg: {avg_ms:.0f}ms\n")

# Test 2: MiniLM only (no ColBERT)
print("━━━ Config: MiniLM bi-encoder only (reranker=minilm) ━━━")
config_results = []
for tc in TESTS:
    if tc["name"] not in html_cache: continue
    html = html_cache[tc["name"]]
    t0 = time.monotonic()
    data = ae_post("/api/parse-hybrid", {"html": html, "goal": tc["goal"], "url": tc["url"], "top_n": 20, "reranker": "minilm"})
    ms = (time.monotonic() - t0) * 1000
    nodes = data.get("top_nodes", [])
    top5_text = " ".join(n.get("label","") for n in nodes[:5]).lower()
    found = all(kw.lower() in top5_text for kw in tc["must_contain"])
    config_results.append({"name": tc["name"], "found5": found, "ms": ms})
    mark = "✅" if found else "❌"
    print(f"  {mark} {tc['name']:<20} {ms:>6.0f}ms")
found_count = sum(1 for r in config_results if r["found5"])
avg_ms = sum(r["ms"] for r in config_results) / len(config_results) if config_results else 0
results["minilm"] = {"recall5": found_count, "total": len(config_results), "avg_ms": avg_ms}
print(f"  Recall@5: {found_count}/{len(config_results)}  Avg: {avg_ms:.0f}ms\n")

# Test 3: ColBERT without goal expansion (raw goals)
print("━━━ Config: ColBERT with RAW goals (no LLM expansion) ━━━")
RAW_GOALS = {
    "GOV.UK Wage": "minimum wage 2025",
    "Bank of England": "current interest rate",
    "Hacker News": "find latest news articles",
    "PyPI": "find Python packages",
    "NASA": "NASA space missions",
    "lobste.rs": "find technology articles",
    "Tibro kommun": "nyheter Tibro",
    "CoinGecko": "bitcoin price",
    "W3Schools": "HTML tutorial",
    "Docker Hub": "search container images",
}
config_results = []
for tc in TESTS:
    if tc["name"] not in html_cache: continue
    html = html_cache[tc["name"]]
    raw_goal = RAW_GOALS.get(tc["name"], tc["goal"])
    t0 = time.monotonic()
    data = ae_post("/api/parse-hybrid", {"html": html, "goal": raw_goal, "url": tc["url"], "top_n": 20})
    ms = (time.monotonic() - t0) * 1000
    nodes = data.get("top_nodes", [])
    top5_text = " ".join(n.get("label","") for n in nodes[:5]).lower()
    found = all(kw.lower() in top5_text for kw in tc["must_contain"])
    config_results.append({"name": tc["name"], "found5": found, "ms": ms})
    mark = "✅" if found else "❌"
    print(f"  {mark} {tc['name']:<20} {ms:>6.0f}ms  goal='{raw_goal}'")
found_count = sum(1 for r in config_results if r["found5"])
avg_ms = sum(r["ms"] for r in config_results) / len(config_results) if config_results else 0
results["no_expansion"] = {"recall5": found_count, "total": len(config_results), "avg_ms": avg_ms}
print(f"  Recall@5: {found_count}/{len(config_results)}  Avg: {avg_ms:.0f}ms\n")

# Test 4: Accessibility tree baseline (parse without hybrid scoring)
print("━━━ Config: Accessibility tree only (no scoring pipeline) ━━━")
config_results = []
for tc in TESTS:
    if tc["name"] not in html_cache: continue
    html = html_cache[tc["name"]]
    t0 = time.monotonic()
    data = ae_post("/api/parse", {"html": html, "goal": tc["goal"], "url": tc["url"], "top_n": 20})
    ms = (time.monotonic() - t0) * 1000
    # parse returns tree, check top-level nodes
    nodes = data.get("nodes", [])[:20] if isinstance(data.get("nodes"), list) else []
    top5_text = " ".join(n.get("label","") for n in nodes[:5]).lower()
    found = all(kw.lower() in top5_text for kw in tc["must_contain"])
    config_results.append({"name": tc["name"], "found5": found, "ms": ms})
    mark = "✅" if found else "❌"
    print(f"  {mark} {tc['name']:<20} {ms:>6.0f}ms")
found_count = sum(1 for r in config_results if r["found5"])
avg_ms = sum(r["ms"] for r in config_results) / len(config_results) if config_results else 0
results["a11y_tree"] = {"recall5": found_count, "total": len(config_results), "avg_ms": avg_ms}
print(f"  Recall@5: {found_count}/{len(config_results)}  Avg: {avg_ms:.0f}ms\n")

# Summary
print("=" * 90)
print("  ABLATION SUMMARY")
print("=" * 90)
print(f"\n  {'Configuration':<45} {'Recall@5':>10} {'Avg ms':>10}")
print("  " + "-" * 65)
for key, label in [("full", "Full system (ColBERT + expansion + all)"),
                    ("minilm", "MiniLM bi-encoder only"),
                    ("no_expansion", "ColBERT without goal expansion"),
                    ("a11y_tree", "Accessibility tree (no scoring)")]:
    r = results.get(key, {})
    total = r.get("total", 0)
    recall = r.get("recall5", 0)
    pct = f"{recall}/{total}" if total else "N/A"
    ms = r.get("avg_ms", 0)
    print(f"  {label:<45} {pct:>10} {ms:>9.0f}ms")

json.dump(results, open("benches/ablation_results.json", "w"), indent=2)
print(f"\n  Results → benches/ablation_results.json")
