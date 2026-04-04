#!/usr/bin/env python3
"""
POC 50 Sites — CRFR vs Pipeline head-to-head.
Kör parse-crfr och parse-hybrid på samma 50 sajter, jämför allt.
"""
import json, time, urllib.request, sys

AE = "http://127.0.0.1:3000"

def ae_post(ep, body, timeout=90):
    data = json.dumps(body).encode()
    req = urllib.request.Request(f"{AE}{ep}", data=data, headers={"Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return json.loads(r.read().decode())

def fetch(url):
    try: return ae_post("/api/fetch", {"url": url}).get("body", "")
    except: return ""

def crfr_parse(html, goal, url):
    return ae_post("/api/parse-crfr", {"html": html, "goal": goal, "url": url, "top_n": 20})

def hybrid_parse(html, goal, url):
    return ae_post("/api/parse-hybrid", {"html": html, "goal": goal, "url": url, "top_n": 20})

# Importera testcases från poc_50_sites.py
TESTS = [
    # ── Nyheter (8) ──
    {"cat": "news", "name": "Hacker News", "url": "https://news.ycombinator.com", "goal": "latest news articles stories submissions points comments hours ago top", "must_contain": ["points", "comments"]},
    {"cat": "news", "name": "lobste.rs", "url": "https://lobste.rs", "goal": "technology articles programming stories submissions comments points hours ago tags", "must_contain": ["hours ago"]},
    {"cat": "news", "name": "CNN Lite", "url": "https://lite.cnn.com", "goal": "top news headlines today breaking stories CNN latest", "must_contain": ["cnn"]},
    {"cat": "news", "name": "NPR Text", "url": "https://text.npr.org", "goal": "latest radio news stories NPR today national public radio headlines", "must_contain": ["npr"]},
    {"cat": "news", "name": "Reuters", "url": "https://www.reuters.com", "goal": "business news today Reuters financial markets world headlines breaking", "must_contain": ["reuters"]},
    {"cat": "news", "name": "Tibro kommun", "url": "https://www.tibro.se", "goal": "nyheter Tibro kommun senaste mars april 2026 2025 nyhet rubrik händelse", "must_contain": ["2026"]},
    {"cat": "news", "name": "Al Jazeera", "url": "https://www.aljazeera.com", "goal": "international news coverage world headlines stories breaking latest today", "must_contain": ["news"]},
    {"cat": "news", "name": "The Guardian", "url": "https://www.theguardian.com", "goal": "latest news UK world headlines opinion sport culture today", "must_contain": ["guardian"]},
    # ── Myndigheter (5) ──
    {"cat": "gov", "name": "GOV.UK Wage", "url": "https://www.gov.uk/national-minimum-wage-rates", "goal": "minimum wage 2025 National Living Wage hourly rate per hour £12.21 £12.71 April pay workers", "must_contain": ["£12"]},
    {"cat": "gov", "name": "Bank of England", "url": "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate", "goal": "current interest rate Bank Rate percentage 3.75 monetary policy MPC decision", "must_contain": ["%"]},
    {"cat": "gov", "name": "WHO", "url": "https://www.who.int", "goal": "World Health Organization global health disease outbreak response prevention", "must_contain": ["health"]},
    {"cat": "gov", "name": "EU Europa", "url": "https://european-union.europa.eu/index_en", "goal": "European Union EU member states policy institutions Brussels parliament commission", "must_contain": ["european"]},
    {"cat": "gov", "name": "NASA", "url": "https://www.nasa.gov", "goal": "NASA space exploration missions moon Mars Artemis astronaut launch rocket science", "must_contain": ["nasa"]},
    # ── Utveckling/Docs (10) ──
    {"cat": "dev", "name": "rust-lang.org", "url": "https://www.rust-lang.org", "goal": "Rust programming language fast safe memory efficient systems install cargo", "must_contain": ["rust"]},
    {"cat": "dev", "name": "MDN HTML", "url": "https://developer.mozilla.org/en-US/docs/Web/HTML", "goal": "HTML elements reference MDN Web Docs element tag attribute semantic heading paragraph", "must_contain": ["html", "element"]},
    {"cat": "dev", "name": "Go Dev", "url": "https://go.dev", "goal": "Go programming language Golang download install build simple secure scalable systems", "must_contain": ["go"]},
    {"cat": "dev", "name": "Node.js", "url": "https://nodejs.org", "goal": "Node.js JavaScript runtime download LTS version npm server-side event-driven", "must_contain": ["node"]},
    {"cat": "dev", "name": "Ruby Lang", "url": "https://www.ruby-lang.org/en/", "goal": "Ruby programming language download install elegant productive dynamic object-oriented", "must_contain": ["ruby"]},
    {"cat": "dev", "name": "docs.rs", "url": "https://docs.rs", "goal": "Rust documentation crates packages API reference search library docs.rs", "must_contain": ["rust"]},
    {"cat": "dev", "name": "Kotlin", "url": "https://kotlinlang.org", "goal": "Kotlin programming language JVM Android multiplatform modern concise safe", "must_contain": ["kotlin"]},
    {"cat": "dev", "name": "Elixir Lang", "url": "https://elixir-lang.org", "goal": "Elixir programming language functional concurrent distributed fault-tolerant scalable Erlang", "must_contain": ["elixir"]},
    {"cat": "dev", "name": "Zig Lang", "url": "https://ziglang.org", "goal": "Zig programming language systems low-level comptime safety performance", "must_contain": ["zig"]},
    {"cat": "dev", "name": "Svelte", "url": "https://svelte.dev", "goal": "Svelte web framework JavaScript compiler reactive UI components SvelteKit", "must_contain": ["svelte"]},
    # ── Paketregister (5) ──
    {"cat": "pkg", "name": "PyPI", "url": "https://pypi.org", "goal": "find Python packages pip install PyPI package index repository projects download", "must_contain": ["python", "package"]},
    {"cat": "pkg", "name": "pkg.go.dev", "url": "https://pkg.go.dev", "goal": "Go packages modules Golang standard library pkg.go.dev import net/http fmt io", "must_contain": ["go", "package"]},
    {"cat": "pkg", "name": "RubyGems", "url": "https://rubygems.org", "goal": "Ruby gem packages RubyGems download install community library repository", "must_contain": ["ruby"]},
    {"cat": "pkg", "name": "NuGet", "url": "https://www.nuget.org", "goal": "NuGet .NET package manager C# library download install packages Microsoft", "must_contain": ["nuget"]},
    {"cat": "pkg", "name": "DevDocs", "url": "https://devdocs.io", "goal": "DevDocs API documentation browser offline fast search multiple programming languages", "must_contain": ["documentation"]},
    # ── Infra/DevOps (4) ──
    {"cat": "infra", "name": "Docker Hub", "url": "https://hub.docker.com", "goal": "search container images Docker Hub registry pull docker.io official alpine ubuntu nginx", "must_contain": ["docker", "image"]},
    {"cat": "infra", "name": "Terraform", "url": "https://www.terraform.io", "goal": "Terraform infrastructure as code IaC HashiCorp provision cloud AWS Azure GCP automate", "must_contain": ["terraform"]},
    {"cat": "infra", "name": "GitHub Explore", "url": "https://github.com/explore", "goal": "trending repositories GitHub explore open source projects popular stars developers", "must_contain": ["trending"]},
    {"cat": "infra", "name": "Tailwind CSS", "url": "https://tailwindcss.com", "goal": "Tailwind CSS utility-first framework classes responsive design styling rapid UI", "must_contain": ["tailwind"]},
    # ── Referens/Utbildning (6) ──
    {"cat": "ref", "name": "OpenStreetMap", "url": "https://www.openstreetmap.org", "goal": "OpenStreetMap map world navigation editing free community-driven cartography OSM", "must_contain": ["map"]},
    {"cat": "ref", "name": "httpbin HTML", "url": "https://httpbin.org/html", "goal": "Herman Melville Moby Dick story novel whale captain Ahab literary classic", "must_contain": ["melville"]},
    {"cat": "ref", "name": "JSON Placeholder", "url": "https://jsonplaceholder.typicode.com", "goal": "free fake API testing prototyping REST JSON placeholder endpoints posts users", "must_contain": ["api"]},
    {"cat": "ref", "name": "Haskell.org", "url": "https://www.haskell.org", "goal": "Haskell programming language functional purely typed lazy evaluation advanced type system", "must_contain": ["haskell"]},
    {"cat": "ref", "name": "W3Schools HTML", "url": "https://www.w3schools.com/html/", "goal": "HTML tutorial learn web development elements tags attributes beginner guide examples", "must_contain": ["html"]},
    {"cat": "ref", "name": "Stack Overflow", "url": "https://stackoverflow.com/questions", "goal": "programming questions answers Stack Overflow developers code help debugging solutions", "must_contain": ["question"]},
    # ── Finans/Ekonomi (4) ──
    {"cat": "finance", "name": "CoinGecko", "url": "https://www.coingecko.com", "goal": "cryptocurrency Bitcoin price market cap volume BTC ETH trading exchange rate USD", "must_contain": ["bitcoin", "price"]},
    {"cat": "finance", "name": "ECB", "url": "https://www.ecb.europa.eu", "goal": "European Central Bank ECB interest rate monetary policy euro inflation Frankfurt", "must_contain": ["ecb"]},
    {"cat": "finance", "name": "Investing.com", "url": "https://www.investing.com", "goal": "stock market indices S&P 500 Dow Jones NASDAQ trading financial markets live quotes", "must_contain": ["market"]},
    {"cat": "finance", "name": "XE Currency", "url": "https://www.xe.com", "goal": "currency converter exchange rate EUR USD GBP real-time conversion foreign exchange", "must_contain": ["currency"]},
    # ── Kultur/Övrigt (8) ──
    {"cat": "other", "name": "IMDB Top", "url": "https://www.imdb.com/chart/top/", "goal": "top rated movies best films all time IMDB rating 250 Shawshank Godfather Schindler", "must_contain": ["rating"]},
    {"cat": "other", "name": "Goodreads", "url": "https://www.goodreads.com", "goal": "books reading reviews Goodreads popular bestseller fiction nonfiction author recommendations", "must_contain": ["book"]},
    {"cat": "other", "name": "Weather.com", "url": "https://weather.com", "goal": "weather forecast today temperature rain sun wind humidity degrees Celsius Fahrenheit", "must_contain": ["weather"]},
    {"cat": "other", "name": "Spotify Web", "url": "https://open.spotify.com", "goal": "Spotify music streaming playlists songs artists albums podcast listen discover", "must_contain": ["spotify"]},
    {"cat": "other", "name": "Reddit", "url": "https://old.reddit.com", "goal": "Reddit posts comments upvotes subreddit community discussion trending popular front page", "must_contain": ["reddit"]},
    {"cat": "other", "name": "Archive.org", "url": "https://archive.org", "goal": "Internet Archive digital library books movies music web Wayback Machine free access", "must_contain": ["archive"]},
    {"cat": "other", "name": "Product Hunt", "url": "https://www.producthunt.com", "goal": "Product Hunt new products startup launch tech tools apps upvote maker community", "must_contain": ["product"]},
    {"cat": "other", "name": "Hjo kommun", "url": "https://www.hjo.se", "goal": "Hjo kommun invånare befolkning folkmängd population 9000 14000 centralort Vättern", "must_contain": ["hjo"]},
]

def check_nodes(nodes, must_contain, label_key="label"):
    """Kolla vid varje cutoff"""
    def at_k(k):
        text = " ".join(n.get(label_key, "") for n in nodes[:k]).lower()
        return all(kw.lower() in text for kw in must_contain)
    return at_k(1), at_k(3), at_k(5), at_k(10), at_k(20)

def find_rank(nodes, must_contain, label_key="label"):
    for j, n in enumerate(nodes):
        if all(kw.lower() in n.get(label_key, "").lower() for kw in must_contain):
            return j + 1
    return None

print("=" * 120)
print(f"  CRFR vs Pipeline — {len(TESTS)} Sites Head-to-Head")
print("=" * 120)
print()

results = []
crfr_totals = {"f1":0,"f3":0,"f5":0,"f10":0,"f20":0,"ms":0,"n":0}
pipe_totals = {"f1":0,"f3":0,"f5":0,"f10":0,"f20":0,"ms":0,"n":0}
fetch_fails = 0

for i, tc in enumerate(TESTS):
    sys.stdout.write(f"\r  [{i+1:2}/{len(TESTS)}] {tc['name']:<25} fetching...")
    sys.stdout.flush()

    html = fetch(tc["url"])
    if len(html) < 200:
        sys.stdout.write(f" FETCH FAIL\n")
        fetch_fails += 1
        results.append({"name": tc["name"], "cat": tc["cat"], "status": "FAIL"})
        continue

    html_tokens = len(html) // 4

    # ── CRFR ──
    t0 = time.monotonic()
    try:
        crfr_data = crfr_parse(html, tc["goal"], tc["url"])
        crfr_ms = (time.monotonic() - t0) * 1000
        crfr_nodes = crfr_data.get("nodes", [])
    except Exception as e:
        crfr_ms = 0
        crfr_nodes = []

    # ── Pipeline ──
    t1 = time.monotonic()
    try:
        pipe_data = hybrid_parse(html, tc["goal"], tc["url"])
        pipe_ms = (time.monotonic() - t1) * 1000
        pipe_nodes = pipe_data.get("top_nodes", [])
    except Exception as e:
        pipe_ms = 0
        pipe_nodes = []

    # Mät recall
    c1, c3, c5, c10, c20 = check_nodes(crfr_nodes, tc["must_contain"])
    p1, p3, p5, p10, p20 = check_nodes(pipe_nodes, tc["must_contain"])

    crfr_rank = find_rank(crfr_nodes, tc["must_contain"])
    pipe_rank = find_rank(pipe_nodes, tc["must_contain"])

    crfr_tokens = sum(len(n.get("label","")) for n in crfr_nodes[:20]) // 4
    pipe_tokens = sum(len(n.get("label","")) for n in pipe_nodes[:20]) // 4

    # Ackumulera
    for found, totals in [(c1, crfr_totals), (c3, crfr_totals), (c5, crfr_totals), (c10, crfr_totals), (c20, crfr_totals)]:
        pass
    crfr_totals["f1"] += c1; crfr_totals["f3"] += c3; crfr_totals["f5"] += c5
    crfr_totals["f10"] += c10; crfr_totals["f20"] += c20
    crfr_totals["ms"] += crfr_ms; crfr_totals["n"] += 1

    pipe_totals["f1"] += p1; pipe_totals["f3"] += p3; pipe_totals["f5"] += p5
    pipe_totals["f10"] += p10; pipe_totals["f20"] += p20
    pipe_totals["ms"] += pipe_ms; pipe_totals["n"] += 1

    # Vinnare
    crfr_win = "=" if c20 == p20 else ("C" if c20 and not p20 else ("P" if p20 and not c20 else "="))
    cm = "OK" if c20 else "--"
    pm = "OK" if p20 else "--"
    cr = f"{crfr_rank}" if crfr_rank else "-"
    pr = f"{pipe_rank}" if pipe_rank else "-"

    sys.stdout.write(f"\r  [{i+1:2}/{len(TESTS)}] {tc['name']:<25} CRFR:{cm} r:{cr:>3} {crfr_ms:>5.0f}ms {len(crfr_nodes):>2}n | Pipe:{pm} r:{pr:>3} {pipe_ms:>5.0f}ms {len(pipe_nodes):>2}n | {crfr_win}\n")

    results.append({
        "name": tc["name"], "cat": tc["cat"], "status": "OK",
        "crfr": {"f1":c1,"f3":c3,"f5":c5,"f10":c10,"f20":c20,"ms":round(crfr_ms),"rank":crfr_rank,"nodes":len(crfr_nodes),"tokens":crfr_tokens},
        "pipe": {"f1":p1,"f3":p3,"f5":p5,"f10":p10,"f20":p20,"ms":round(pipe_ms),"rank":pipe_rank,"nodes":len(pipe_nodes),"tokens":pipe_tokens},
        "html_tokens": html_tokens,
    })

# ── Summary ──
n = crfr_totals["n"]
print()
print("=" * 120)
print(f"  SAMMANFATTNING ({n} sajter testade, {fetch_fails} fetch failures)")
print("=" * 120)
print()
print(f"  {'Metod':<30} {'@1':>6} {'@3':>6} {'@5':>6} {'@10':>6} {'@20':>6} {'Avg ms':>8} {'Avg nod':>8}")
print(f"  {'-'*85}")
print(f"  {'CRFR v5':<30} {crfr_totals['f1']:>3}/{n} {crfr_totals['f3']:>3}/{n} {crfr_totals['f5']:>3}/{n} {crfr_totals['f10']:>3}/{n} {crfr_totals['f20']:>3}/{n} {crfr_totals['ms']/n:>7.0f} {sum(r['crfr']['nodes'] for r in results if r.get('status')=='OK')/n:>7.1f}")
print(f"  {'Pipeline (BM25+HDC+Embed)':<30} {pipe_totals['f1']:>3}/{n} {pipe_totals['f3']:>3}/{n} {pipe_totals['f5']:>3}/{n} {pipe_totals['f10']:>3}/{n} {pipe_totals['f20']:>3}/{n} {pipe_totals['ms']/n:>7.0f} {sum(r['pipe']['nodes'] for r in results if r.get('status')=='OK')/n:>7.1f}")

if crfr_totals["ms"] > 0:
    print(f"\n  Speedup CRFR vs Pipeline: {pipe_totals['ms']/crfr_totals['ms']:.1f}x")

# Per category
print(f"\n  {'Category':<12} {'n':>3} {'CRFR @3':>8} {'CRFR @20':>9} {'Pipe @3':>8} {'Pipe @20':>9}")
print(f"  {'-'*55}")
for cat_name, cat_label in [("news","News"),("gov","Gov"),("dev","Dev"),("pkg","Pkg"),("infra","Infra"),("ref","Ref"),("finance","Finance"),("other","Other")]:
    cat = [r for r in results if r.get("status")=="OK" and r.get("cat")==cat_name]
    if not cat: continue
    cn = len(cat)
    c3 = sum(1 for r in cat if r["crfr"]["f3"]); c20 = sum(1 for r in cat if r["crfr"]["f20"])
    p3 = sum(1 for r in cat if r["pipe"]["f3"]); p20 = sum(1 for r in cat if r["pipe"]["f20"])
    print(f"  {cat_label:<12} {cn:>3} {c3:>4}/{cn}    {c20:>4}/{cn}    {p3:>4}/{cn}    {p20:>4}/{cn}")

# CRFR-only wins / Pipeline-only wins
crfr_only = [r for r in results if r.get("status")=="OK" and r["crfr"]["f20"] and not r["pipe"]["f20"]]
pipe_only = [r for r in results if r.get("status")=="OK" and r["pipe"]["f20"] and not r["crfr"]["f20"]]
print(f"\n  CRFR-only wins (@20): {len(crfr_only)}")
for r in crfr_only:
    print(f"    {r['name']}: CRFR rank {r['crfr']['rank']}")
print(f"  Pipeline-only wins (@20): {len(pipe_only)}")
for r in pipe_only:
    print(f"    {r['name']}: Pipeline rank {r['pipe']['rank']}")

json.dump(results, open("benches/poc_50_crfr_results.json", "w"), indent=2, ensure_ascii=False)
print(f"\n  Results → benches/poc_50_crfr_results.json")
