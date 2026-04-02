#!/usr/bin/env python3
"""
Bottom-up + penalties combined ablation — 30 sites.
Tests: full vs no_bottomup vs no_bottomup_no_penalties vs minilm
Shows whether bottom-up AND penalties together prevent wrapper-bias.
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

FACT_ROLES = {"text", "data", "cell", "row", "listitem"}

TESTS = [
    ("GOV.UK Wage", "https://www.gov.uk/national-minimum-wage-rates", "minimum wage 2025 £12.21 £12.71 hourly rate"),
    ("Bank of England", "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate", "current interest rate Bank Rate 3.75 monetary policy"),
    ("Hacker News", "https://news.ycombinator.com", "latest news articles stories points comments hours ago"),
    ("lobste.rs", "https://lobste.rs", "technology articles programming stories comments hours ago"),
    ("CNN Lite", "https://lite.cnn.com", "top news headlines today breaking CNN"),
    ("NPR Text", "https://text.npr.org", "latest radio news NPR today headlines"),
    ("Al Jazeera", "https://www.aljazeera.com", "international news world headlines breaking"),
    ("Tibro kommun", "https://www.tibro.se", "nyheter Tibro kommun 2026 2025"),
    ("rust-lang", "https://www.rust-lang.org", "Rust programming language fast safe memory install"),
    ("Go Dev", "https://go.dev", "Go programming language Golang download install"),
    ("Node.js", "https://nodejs.org", "Node.js JavaScript runtime download LTS npm"),
    ("Ruby Lang", "https://www.ruby-lang.org/en/", "Ruby programming language download install"),
    ("Kotlin", "https://kotlinlang.org", "Kotlin programming language JVM Android"),
    ("Elixir", "https://elixir-lang.org", "Elixir programming language functional concurrent"),
    ("Zig", "https://ziglang.org", "Zig programming language systems low-level"),
    ("Svelte", "https://svelte.dev", "Svelte web framework JavaScript compiler reactive"),
    ("PyPI", "https://pypi.org", "find Python packages pip install PyPI repository"),
    ("pkg.go.dev", "https://pkg.go.dev", "Go packages modules standard library import"),
    ("RubyGems", "https://rubygems.org", "Ruby gem packages RubyGems download install"),
    ("NuGet", "https://www.nuget.org", "NuGet .NET package manager C# library"),
    ("Docker Hub", "https://hub.docker.com", "search container images Docker Hub pull official"),
    ("Terraform", "https://www.terraform.io", "Terraform infrastructure code IaC HashiCorp"),
    ("GitHub Explore", "https://github.com/explore", "trending repositories GitHub open source"),
    ("Tailwind CSS", "https://tailwindcss.com", "Tailwind CSS utility-first framework responsive"),
    ("NASA", "https://www.nasa.gov", "NASA space exploration missions Artemis moon"),
    ("WHO", "https://www.who.int", "World Health Organization global health disease"),
    ("CoinGecko", "https://www.coingecko.com", "cryptocurrency Bitcoin price market cap BTC USD"),
    ("ECB", "https://www.ecb.europa.eu", "European Central Bank ECB interest rate monetary"),
    ("W3Schools", "https://www.w3schools.com/html/", "HTML tutorial learn web development elements"),
    ("Goodreads", "https://www.goodreads.com", "books reading reviews Goodreads popular bestseller"),
]

CONFIGS = [
    ("Full (bottom-up + penalties)", {}),
    ("No bottom-up (penalties ON)", {"ablation": "no_bottomup"}),
    ("No bottom-up + no penalties", {"ablation": "no_bottomup_no_penalties"}),
    ("MiniLM only", {"reranker": "minilm"}),
]

print("=" * 115)
print(f"  BOTTOM-UP + PENALTIES ABLATION — {len(TESTS)} sites × {len(CONFIGS)} configs")
print("=" * 115)

print("\n  Fetching...", end="", flush=True)
html_cache = {}
for name, url, _ in TESTS:
    html = fetch(url)
    if len(html) > 200:
        html_cache[name] = html
        sys.stdout.write(".")
        sys.stdout.flush()
print(f" {len(html_cache)} sites.\n")

summary = {}

for config_name, cfg in CONFIGS:
    fact_count = 0
    heading_count = 0
    total = 0

    for name, url, goal in TESTS:
        if name not in html_cache: continue
        total += 1
        body = {"html": html_cache[name], "goal": goal, "url": url, "top_n": 10}
        body.update(cfg)
        try:
            data = ae_post("/api/parse-hybrid", body)
        except:
            continue
        nodes = data.get("top_nodes", [])
        if not nodes: continue
        t1_role = nodes[0].get("role", "?")
        if t1_role in FACT_ROLES:
            fact_count += 1
        elif t1_role in ("heading", "navigation", "link", "generic", "banner", "complementary", "cta"):
            heading_count += 1

    summary[config_name] = {"fact": fact_count, "heading": heading_count, "total": total}
    print(f"  {config_name:<40} Top-1 fact: {fact_count:>2}/{total} ({fact_count/total*100:.0f}%)  Top-1 heading/nav: {heading_count:>2}/{total}")

print(f"\n{'='*115}")
print("  RESULT TABLE")
print(f"{'='*115}\n")
print(f"  {'Config':<40} {'Top-1=fact':>12} {'Top-1=heading':>14} {'Fact %':>8}")
print("  " + "-" * 76)
for config_name, _ in CONFIGS:
    s = summary[config_name]
    pct = s["fact"]/s["total"]*100 if s["total"] else 0
    print(f"  {config_name:<40} {s['fact']:>5}/{s['total']} {s['heading']:>7}/{s['total']} {pct:>7.1f}%")

json.dump(summary, open("benches/bottomup_penalties_results.json", "w"), indent=2)
print(f"\n  Results → benches/bottomup_penalties_results.json")
