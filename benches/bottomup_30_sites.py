#!/usr/bin/env python3
"""
Bottom-up vs Flat ColBERT — 30 sites.
Measures: top-1 node role (fact vs structural), top-1 label, score.
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
    ("GOV.UK Wage", "https://www.gov.uk/national-minimum-wage-rates", "minimum wage 2025 £12.21 £12.71 hourly rate per hour April pay"),
    ("Bank of England", "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate", "current interest rate Bank Rate 3.75 percentage monetary policy MPC"),
    ("Hacker News", "https://news.ycombinator.com", "latest news articles stories points comments hours ago"),
    ("lobste.rs", "https://lobste.rs", "technology articles programming stories comments hours ago"),
    ("CNN Lite", "https://lite.cnn.com", "top news headlines today breaking stories CNN"),
    ("NPR Text", "https://text.npr.org", "latest radio news NPR today headlines"),
    ("Reuters", "https://www.reuters.com", "business news today Reuters financial markets"),
    ("Al Jazeera", "https://www.aljazeera.com", "international news world headlines breaking"),
    ("Tibro kommun", "https://www.tibro.se", "nyheter Tibro kommun 2026 2025 nyhet"),
    ("rust-lang", "https://www.rust-lang.org", "Rust programming language fast safe memory install cargo"),
    ("Go Dev", "https://go.dev", "Go programming language Golang download install build"),
    ("Node.js", "https://nodejs.org", "Node.js JavaScript runtime download LTS npm"),
    ("Ruby Lang", "https://www.ruby-lang.org/en/", "Ruby programming language download install"),
    ("Kotlin", "https://kotlinlang.org", "Kotlin programming language JVM Android multiplatform"),
    ("Elixir", "https://elixir-lang.org", "Elixir programming language functional concurrent"),
    ("Zig", "https://ziglang.org", "Zig programming language systems low-level comptime"),
    ("Svelte", "https://svelte.dev", "Svelte web framework JavaScript compiler reactive"),
    ("PyPI", "https://pypi.org", "find Python packages pip install PyPI repository"),
    ("pkg.go.dev", "https://pkg.go.dev", "Go packages modules standard library import"),
    ("RubyGems", "https://rubygems.org", "Ruby gem packages RubyGems download install"),
    ("NuGet", "https://www.nuget.org", "NuGet .NET package manager C# library install"),
    ("Docker Hub", "https://hub.docker.com", "search container images Docker Hub pull official"),
    ("Terraform", "https://www.terraform.io", "Terraform infrastructure code IaC HashiCorp provision"),
    ("GitHub Explore", "https://github.com/explore", "trending repositories GitHub open source popular"),
    ("Tailwind CSS", "https://tailwindcss.com", "Tailwind CSS utility-first framework responsive"),
    ("NASA", "https://www.nasa.gov", "NASA space exploration missions Artemis moon Mars"),
    ("WHO", "https://www.who.int", "World Health Organization global health disease"),
    ("CoinGecko", "https://www.coingecko.com", "cryptocurrency Bitcoin price market cap BTC USD"),
    ("ECB", "https://www.ecb.europa.eu", "European Central Bank ECB interest rate monetary policy"),
    ("W3Schools", "https://www.w3schools.com/html/", "HTML tutorial learn web development elements tags"),
]

print("=" * 115)
print(f"  BOTTOM-UP vs FLAT ColBERT — {len(TESTS)} sites")
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

hlir_facts, flat_facts, minilm_facts = 0, 0, 0
hlir_data, flat_data, minilm_data = [], [], []
n = 0

print(f"  {'Site':<20} {'HLIR top-1':>12} {'Flat top-1':>12} {'MiniLM top-1':>13}  HLIR label")
print("  " + "-" * 110)

for name, url, goal in TESTS:
    if name not in html_cache:
        continue
    html = html_cache[name]
    n += 1

    results = {}
    for label, params in [("hlir", {}), ("flat", {"ablation": "no_bottomup"}), ("minilm", {"reranker": "minilm"})]:
        body = {"html": html, "goal": goal, "url": url, "top_n": 10}
        body.update(params)
        try:
            data = ae_post("/api/parse-hybrid", body)
        except:
            results[label] = {"role": "?", "score": 0, "label": "ERROR"}
            continue
        nodes = data.get("top_nodes", [])
        if nodes:
            t1 = nodes[0]
            results[label] = {"role": t1.get("role","?"), "score": t1.get("relevance",0), "label": t1.get("label","")[:60]}
        else:
            results[label] = {"role": "?", "score": 0, "label": "(empty)"}

    h = results["hlir"]
    f = results["flat"]
    m = results["minilm"]

    h_fact = h["role"] in FACT_ROLES
    f_fact = f["role"] in FACT_ROLES
    m_fact = m["role"] in FACT_ROLES

    if h_fact: hlir_facts += 1
    if f_fact: flat_facts += 1
    if m_fact: minilm_facts += 1

    hm = "📄" if h_fact else "⚠️"
    fm = "📄" if f_fact else "⚠️"
    mm = "📄" if m_fact else "⚠️"

    diff = ""
    if h["role"] != f["role"]:
        diff = " ◄ DIFFERENT"

    print(f"  {name:<20} {hm}{h['role']:<10} {fm}{f['role']:<10} {mm}{m['role']:<11} {h['label'][:55]}{diff}")

    hlir_data.append(h)
    flat_data.append(f)
    minilm_data.append(m)

print(f"\n{'='*115}")
print(f"  SUMMARY ({n} sites)")
print(f"{'='*115}")
print(f"  {'Method':<25} {'Top-1 = fact node':>20} {'Rate':>8}")
print(f"  {'-'*55}")
print(f"  {'HLIR (bottom-up)':<25} {hlir_facts:>3}/{n}{' ':>14} {hlir_facts/n*100:>5.1f}%")
print(f"  {'ColBERT flat':<25} {flat_facts:>3}/{n}{' ':>14} {flat_facts/n*100:>5.1f}%")
print(f"  {'MiniLM bi-encoder':<25} {minilm_facts:>3}/{n}{' ':>14} {minilm_facts/n*100:>5.1f}%")

# Count where they differ
diffs = sum(1 for h,f in zip(hlir_data, flat_data) if h["role"] != f["role"])
print(f"\n  Sites where HLIR and flat differ in top-1 role: {diffs}/{n}")

json.dump({"hlir": hlir_data, "flat": flat_data, "minilm": minilm_data, "n": n,
           "hlir_facts": hlir_facts, "flat_facts": flat_facts, "minilm_facts": minilm_facts},
          open("benches/bottomup_30_results.json", "w"), indent=2)
print(f"  Results → benches/bottomup_30_results.json")
