#!/usr/bin/env python3
"""
Ablation Study — 44 sites × 5 configurations.
Removes one component at a time. Academic-grade.
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

# Same 50 sites as poc_50_sites.py
TESTS = [
    {"name":"Hacker News","url":"https://news.ycombinator.com","goal":"latest news articles stories submissions points comments hours ago top","must":["points","comments"]},
    {"name":"lobste.rs","url":"https://lobste.rs","goal":"technology articles programming stories submissions comments points hours ago tags","must":["hours ago"]},
    {"name":"CNN Lite","url":"https://lite.cnn.com","goal":"top news headlines today breaking stories CNN latest","must":["cnn"]},
    {"name":"NPR Text","url":"https://text.npr.org","goal":"latest radio news stories NPR today national public radio headlines","must":["npr"]},
    {"name":"Reuters","url":"https://www.reuters.com","goal":"business news today Reuters financial markets world headlines breaking","must":["reuters"]},
    {"name":"Tibro kommun","url":"https://www.tibro.se","goal":"nyheter Tibro kommun senaste mars april 2026 2025 nyhet rubrik","must":["2026"]},
    {"name":"Al Jazeera","url":"https://www.aljazeera.com","goal":"international news coverage world headlines stories breaking latest today","must":["news"]},
    {"name":"GOV.UK Wage","url":"https://www.gov.uk/national-minimum-wage-rates","goal":"minimum wage 2025 National Living Wage hourly rate per hour £12.21 £12.71 April pay","must":["£12"]},
    {"name":"Bank of England","url":"https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate","goal":"current interest rate Bank Rate percentage 3.75 monetary policy MPC","must":["%"]},
    {"name":"WHO","url":"https://www.who.int","goal":"World Health Organization global health disease outbreak response prevention","must":["health"]},
    {"name":"EU Europa","url":"https://european-union.europa.eu/index_en","goal":"European Union EU member states policy institutions Brussels parliament","must":["european"]},
    {"name":"NASA","url":"https://www.nasa.gov","goal":"NASA space exploration missions moon Mars Artemis astronaut launch rocket","must":["nasa"]},
    {"name":"rust-lang.org","url":"https://www.rust-lang.org","goal":"Rust programming language fast safe memory efficient systems install cargo","must":["rust"]},
    {"name":"MDN HTML","url":"https://developer.mozilla.org/en-US/docs/Web/HTML","goal":"HTML elements reference MDN Web Docs element tag attribute semantic","must":["html","element"]},
    {"name":"Go Dev","url":"https://go.dev","goal":"Go programming language Golang download install build simple secure scalable","must":["go"]},
    {"name":"Node.js","url":"https://nodejs.org","goal":"Node.js JavaScript runtime download LTS version npm server-side","must":["node"]},
    {"name":"Ruby Lang","url":"https://www.ruby-lang.org/en/","goal":"Ruby programming language download install elegant productive dynamic","must":["ruby"]},
    {"name":"docs.rs","url":"https://docs.rs","goal":"Rust documentation crates packages API reference search library","must":["rust"]},
    {"name":"Kotlin","url":"https://kotlinlang.org","goal":"Kotlin programming language JVM Android multiplatform modern concise safe","must":["kotlin"]},
    {"name":"Elixir","url":"https://elixir-lang.org","goal":"Elixir programming language functional concurrent distributed fault-tolerant","must":["elixir"]},
    {"name":"Zig","url":"https://ziglang.org","goal":"Zig programming language systems low-level comptime safety performance","must":["zig"]},
    {"name":"Svelte","url":"https://svelte.dev","goal":"Svelte web framework JavaScript compiler reactive UI components SvelteKit","must":["svelte"]},
    {"name":"PyPI","url":"https://pypi.org","goal":"find Python packages pip install PyPI package index repository","must":["python","package"]},
    {"name":"pkg.go.dev","url":"https://pkg.go.dev","goal":"Go packages modules Golang standard library import net/http fmt io","must":["go","package"]},
    {"name":"RubyGems","url":"https://rubygems.org","goal":"Ruby gem packages RubyGems download install community library","must":["ruby"]},
    {"name":"NuGet","url":"https://www.nuget.org","goal":"NuGet .NET package manager C# library download install packages","must":["nuget"]},
    {"name":"Docker Hub","url":"https://hub.docker.com","goal":"search container images Docker Hub registry pull official alpine ubuntu","must":["docker","image"]},
    {"name":"Terraform","url":"https://www.terraform.io","goal":"Terraform infrastructure as code IaC HashiCorp provision cloud AWS automate","must":["terraform"]},
    {"name":"GitHub Explore","url":"https://github.com/explore","goal":"trending repositories GitHub explore open source projects popular stars","must":["trending"]},
    {"name":"Tailwind CSS","url":"https://tailwindcss.com","goal":"Tailwind CSS utility-first framework classes responsive design styling","must":["tailwind"]},
    {"name":"OpenStreetMap","url":"https://www.openstreetmap.org","goal":"OpenStreetMap map world navigation editing free community-driven OSM","must":["map"]},
    {"name":"httpbin","url":"https://httpbin.org/html","goal":"Herman Melville Moby Dick story novel whale captain Ahab literary","must":["melville"]},
    {"name":"JSON PH","url":"https://jsonplaceholder.typicode.com","goal":"free fake API testing prototyping REST JSON placeholder endpoints","must":["api"]},
    {"name":"Haskell","url":"https://www.haskell.org","goal":"Haskell programming language functional purely typed lazy evaluation","must":["haskell"]},
    {"name":"W3Schools","url":"https://www.w3schools.com/html/","goal":"HTML tutorial learn web development elements tags attributes beginner","must":["html"]},
    {"name":"CoinGecko","url":"https://www.coingecko.com","goal":"cryptocurrency Bitcoin price market cap volume BTC ETH trading USD","must":["bitcoin","price"]},
    {"name":"ECB","url":"https://www.ecb.europa.eu","goal":"European Central Bank ECB interest rate monetary policy euro inflation","must":["ecb"]},
    {"name":"Investing","url":"https://www.investing.com","goal":"stock market indices S&P 500 Dow Jones NASDAQ trading financial live","must":["market"]},
    {"name":"XE Currency","url":"https://www.xe.com","goal":"currency converter exchange rate EUR USD GBP real-time conversion","must":["currency"]},
    {"name":"Goodreads","url":"https://www.goodreads.com","goal":"books reading reviews Goodreads popular bestseller fiction nonfiction","must":["book"]},
    {"name":"Spotify","url":"https://open.spotify.com","goal":"Spotify music streaming playlists songs artists albums podcast listen","must":["spotify"]},
    {"name":"Product Hunt","url":"https://www.producthunt.com","goal":"Product Hunt new products startup launch tech tools apps upvote","must":["product"]},
]

CONFIGS = [
    ("Full system", {}),
    ("− Dense fallback", {"ablation": "no_dense"}),
    ("− HDC pruning", {"ablation": "no_hdc"}),
    ("− Bottom-up scoring", {"ablation": "no_bottomup"}),
    ("− Query expansion", {"ablation": "no_expansion"}),
    ("MiniLM only (no ColBERT)", {"reranker": "minilm"}),
]

print("=" * 90)
print(f"  ABLATION STUDY — {len(TESTS)} sites × {len(CONFIGS)} configurations")
print("=" * 90)

# Pre-fetch
print("\n  Fetching HTML...", end="", flush=True)
html_cache = {}
for tc in TESTS:
    html = fetch(tc["url"])
    if len(html) > 200:
        html_cache[tc["name"]] = html
        sys.stdout.write(".")
        sys.stdout.flush()
print(f" {len(html_cache)} sites.\n")

results = {}

for config_name, config_params in CONFIGS:
    print(f"━━━ {config_name} ━━━")
    found_5, found_20, latencies = 0, 0, []

    for tc in TESTS:
        if tc["name"] not in html_cache:
            continue
        html = html_cache[tc["name"]]

        body = {"html": html, "goal": tc["goal"], "url": tc["url"], "top_n": 20}
        body.update(config_params)

        t0 = time.monotonic()
        try:
            data = ae_post("/api/parse-hybrid", body)
        except:
            continue
        ms = (time.monotonic() - t0) * 1000
        latencies.append(ms)

        nodes = data.get("top_nodes", [])
        t5 = " ".join(n.get("label","") for n in nodes[:5]).lower()
        t20 = " ".join(n.get("label","") for n in nodes[:20]).lower()

        if all(kw.lower() in t5 for kw in tc["must"]):
            found_5 += 1
        if all(kw.lower() in t20 for kw in tc["must"]):
            found_20 += 1

    n = len(latencies)
    avg_ms = sum(latencies) / n if n else 0
    r5 = f"{found_5}/{n}"
    r20 = f"{found_20}/{n}"
    pct5 = found_5/n*100 if n else 0
    pct20 = found_20/n*100 if n else 0
    print(f"  Recall@5: {r5} ({pct5:.1f}%)  Recall@20: {r20} ({pct20:.1f}%)  Avg: {avg_ms:.0f}ms  Sites: {n}")
    results[config_name] = {"recall5": found_5, "recall20": found_20, "total": n, "avg_ms": round(avg_ms)}
    print()

# Summary table
print("=" * 90)
print("  ABLATION SUMMARY")
print("=" * 90)
print(f"\n  {'Configuration':<30} {'Recall@5':>10} {'Recall@20':>11} {'Avg ms':>8}")
print("  " + "-" * 62)
for config_name, _ in CONFIGS:
    r = results.get(config_name, {})
    n = r.get("total", 0)
    r5 = r.get("recall5", 0)
    r20 = r.get("recall20", 0)
    ms = r.get("avg_ms", 0)
    pct5 = f"{r5/n*100:.1f}%" if n else "N/A"
    pct20 = f"{r20/n*100:.1f}%" if n else "N/A"
    print(f"  {config_name:<30} {r5:>3}/{n} ({pct5:>5}) {r20:>3}/{n} ({pct20:>5}) {ms:>7}ms")

json.dump(results, open("benches/ablation_44_results.json", "w"), indent=2)
print(f"\n  Results → benches/ablation_44_results.json")
