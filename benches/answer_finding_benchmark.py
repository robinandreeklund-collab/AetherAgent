#!/usr/bin/env python3
"""
Answer-finding benchmark — Does the answer exist in top-10/top-20?
The real question: can an LLM find the answer from our output?
"""
import json, time, urllib.request

AE = "http://127.0.0.1:3000"

def ae_post(ep, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(f"{AE}{ep}", data=data, headers={"Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return json.loads(r.read().decode())

def fetch(url):
    try: return ae_post("/api/fetch", {"url": url}).get("body", "")
    except: return ""

TESTS = [
    {
        "name": "GOV.UK Wage",
        "url": "https://www.gov.uk/national-minimum-wage-rates",
        "goal": "minimum wage 2025 National Living Wage hourly rate per hour £12.21 £12.71 April pay",
        "must_contain": ["£12"],  # THE answer
        "description": "National Living Wage rate",
    },
    {
        "name": "Bank of England",
        "url": "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate",
        "goal": "current interest rate Bank Rate percentage monetary policy MPC February 2025",
        "must_contain": ["%"],
        "description": "Current Bank Rate percentage",
    },
    {
        "name": "Hacker News",
        "url": "https://news.ycombinator.com",
        "goal": "find latest news articles top stories submissions points comments hours ago",
        "must_contain": ["points", "comments"],
        "description": "Actual article rows with points/comments",
    },
    {
        "name": "rust-lang.org",
        "url": "https://www.rust-lang.org",
        "goal": "download install Rust rustup curl sh cargo version stable toolchain rustc",
        "must_contain": ["install", "rustup"],
        "description": "Install link or rustup instructions",
    },
    {
        "name": "PyPI",
        "url": "https://pypi.org",
        "goal": "find Python packages pip install PyPI package index repository 500000 projects",
        "must_contain": ["python", "package"],
        "description": "Python package index description",
    },
    {
        "name": "MDN HTML",
        "url": "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "goal": "HTML elements reference MDN Web Docs element tag attribute global semantic",
        "must_contain": ["element", "html"],
        "description": "HTML elements reference content",
    },
    {
        "name": "lobste.rs",
        "url": "https://lobste.rs",
        "goal": "find technology articles programming stories submissions comments points hours ago",
        "must_contain": ["hours ago", "comments"],
        "description": "Actual article submissions with metadata",
    },
    {
        "name": "Tibro kommun",
        "url": "https://www.tibro.se",
        "goal": "nyheter Tibro kommun senaste mars april 2026 2025 nyhet rubrik",
        "must_contain": ["2026", "2025"],
        "description": "Actual news items with dates",
    },
    {
        "name": "Docker Hub",
        "url": "https://hub.docker.com",
        "goal": "search container images Docker Hub registry pull docker.io official alpine ubuntu nginx",
        "must_contain": ["docker", "image"],
        "description": "Docker container image content",
    },
    {
        "name": "pkg.go.dev",
        "url": "https://pkg.go.dev",
        "goal": "Go packages modules Golang standard library pkg.go.dev import net/http fmt io",
        "must_contain": ["go", "package"],
        "description": "Go package listing or description",
    },
]

print("=" * 100)
print("  ANSWER-FINDING BENCHMARK — Is the answer in top-10 / top-20?")
print("  The POC question: Can we deliver the right information with fewer tokens?")
print("=" * 100)
print()

results = []

for tc in TESTS:
    html = fetch(tc["url"])
    if len(html) < 200:
        print(f"  {tc['name']}: FETCH FAIL")
        results.append({"name": tc["name"], "fetched": False})
        continue

    # Run with top_n=20
    t0 = time.monotonic()
    data = ae_post("/api/parse-hybrid", {
        "html": html, "goal": tc["goal"], "url": tc["url"], "top_n": 20
    })
    ms = (time.monotonic() - t0) * 1000

    nodes = data.get("top_nodes", [])
    pipeline = data.get("pipeline", {})
    total_nodes = data.get("total_nodes", 0)

    # Check at different cutoffs
    def check_at_k(k):
        top_k_text = " ".join(n.get("label", "") for n in nodes[:k]).lower()
        hits = [kw for kw in tc["must_contain"] if kw.lower() in top_k_text]
        return len(hits) == len(tc["must_contain"])

    found_1 = check_at_k(1)
    found_3 = check_at_k(3)
    found_5 = check_at_k(5)
    found_10 = check_at_k(10)
    found_20 = check_at_k(20)

    # Token count
    output_tokens = sum(len(n.get("label", "")) for n in nodes[:20]) // 4
    html_tokens = len(html) // 4
    savings = (1 - output_tokens / html_tokens) * 100 if html_tokens > 0 else 0

    m1 = "✅" if found_1 else "❌"
    m3 = "✅" if found_3 else "❌"
    m5 = "✅" if found_5 else "❌"
    m10 = "✅" if found_10 else "❌"
    m20 = "✅" if found_20 else "❌"

    print(f"  {tc['name']:<20} top1:{m1} top3:{m3} top5:{m5} top10:{m10} top20:{m20}  {ms:.0f}ms  {total_nodes}→{len(nodes)} nodes  {savings:.0f}% saved")
    print(f"    Looking for: {tc['description']} [{', '.join(tc['must_contain'])}]")

    # Show where answer first appears
    for i, n in enumerate(nodes):
        label_lower = n.get("label", "").lower()
        if all(kw.lower() in label_lower for kw in tc["must_contain"]):
            trunc = n.get("label", "")[:80]
            print(f"    → FOUND at rank {i+1}: [{n.get('relevance',0):.3f}] {n.get('role','?'):10} {trunc}")
            break
    else:
        print(f"    → NOT FOUND in top-20")
        # Show what IS in top-3
        for i, n in enumerate(nodes[:3]):
            trunc = n.get("label", "")[:80]
            print(f"    top-{i+1}: [{n.get('relevance',0):.3f}] {n.get('role','?'):10} {trunc}")

    results.append({
        "name": tc["name"],
        "fetched": True,
        "found_1": found_1, "found_3": found_3, "found_5": found_5,
        "found_10": found_10, "found_20": found_20,
        "ms": round(ms), "total_nodes": total_nodes,
        "output_nodes": len(nodes), "savings_pct": round(savings),
    })
    print()

# Summary
print("=" * 100)
print("  SUMMARY")
print("=" * 100)
fetched = [r for r in results if r.get("fetched")]
n = len(fetched)

for k_name, k_key in [("top-1", "found_1"), ("top-3", "found_3"), ("top-5", "found_5"), ("top-10", "found_10"), ("top-20", "found_20")]:
    found = sum(1 for r in fetched if r.get(k_key))
    print(f"  Answer in {k_name}: {found}/{n} ({found/n*100:.0f}%)")

avg_savings = sum(r.get("savings_pct", 0) for r in fetched) / n if n else 0
avg_ms = sum(r.get("ms", 0) for r in fetched) / n if n else 0
print(f"\n  Avg token savings: {avg_savings:.0f}%")
print(f"  Avg latency: {avg_ms:.0f}ms")
print(f"\n  → Can an LLM find the answer from our top-20? {sum(1 for r in fetched if r.get('found_20'))}/{n} sites")

json.dump(results, open("benches/answer_finding_results.json", "w"), indent=2)
