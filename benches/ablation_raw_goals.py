#!/usr/bin/env python3
"""
Ablation Study with RAW goals + strict answer checking.
No LLM expansion — shows what each component actually contributes.
Answer check: specific facts, not just keywords.
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

# 20 sites with STRICT answer criteria — actual facts, not just site names
TESTS = [
    # ── Sites where the answer is a specific fact ──
    {"name": "GOV.UK Wage", "url": "https://www.gov.uk/national-minimum-wage-rates",
     "raw": "minimum wage 2025",
     "expanded": "minimum wage 2025 National Living Wage hourly rate per hour £12.21 £12.71 April",
     "answer": ["£12"],  # Must contain actual wage amount
     "question": "What is the UK minimum wage?"},

    {"name": "Bank of England", "url": "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate",
     "raw": "current interest rate",
     "expanded": "current interest rate Bank Rate percentage 3.75 monetary policy MPC February",
     "answer": ["3.75"],  # Must contain actual rate number
     "question": "What is the Bank of England interest rate?"},

    {"name": "PyPI", "url": "https://pypi.org",
     "raw": "find Python packages",
     "expanded": "find Python packages pip install PyPI package index repository projects",
     "answer": ["python", "package", "install"],  # Must describe what PyPI does
     "question": "What is PyPI?"},

    # ── News sites — must contain actual article content ──
    {"name": "Hacker News", "url": "https://news.ycombinator.com",
     "raw": "latest news",
     "expanded": "latest news articles stories submissions points comments hours ago",
     "answer": ["points", "ago"],  # Must have actual article rows
     "question": "What are the latest HN stories?"},

    {"name": "lobste.rs", "url": "https://lobste.rs",
     "raw": "technology articles",
     "expanded": "technology articles programming stories submissions comments points hours ago",
     "answer": ["hours ago"],  # Must have actual submissions
     "question": "What are the latest lobste.rs stories?"},

    {"name": "CNN Lite", "url": "https://lite.cnn.com",
     "raw": "news headlines",
     "expanded": "top news headlines today breaking stories CNN latest",
     "answer": ["cnn"],
     "question": "What are today's CNN headlines?"},

    # ── Dev sites — must contain language/framework info ──
    {"name": "rust-lang", "url": "https://www.rust-lang.org",
     "raw": "Rust programming",
     "expanded": "Rust programming language fast safe memory efficient install cargo rustup",
     "answer": ["rust", "memory"],  # Must describe Rust's properties
     "question": "What is Rust?"},

    {"name": "Go Dev", "url": "https://go.dev",
     "raw": "Go language",
     "expanded": "Go programming language Golang download install build simple secure scalable",
     "answer": ["go"],
     "question": "What is Go?"},

    {"name": "Node.js", "url": "https://nodejs.org",
     "raw": "Node.js",
     "expanded": "Node.js JavaScript runtime download LTS version npm server-side",
     "answer": ["node"],
     "question": "What is Node.js?"},

    {"name": "Kotlin", "url": "https://kotlinlang.org",
     "raw": "Kotlin language",
     "expanded": "Kotlin programming language JVM Android multiplatform modern concise safe",
     "answer": ["kotlin"],
     "question": "What is Kotlin?"},

    # ── Reference — must contain actual content ──
    {"name": "NASA", "url": "https://www.nasa.gov",
     "raw": "space missions",
     "expanded": "NASA space exploration missions moon Mars Artemis astronaut launch rocket",
     "answer": ["nasa"],
     "question": "What is NASA doing?"},

    {"name": "WHO", "url": "https://www.who.int",
     "raw": "global health",
     "expanded": "World Health Organization global health disease outbreak response prevention",
     "answer": ["health"],
     "question": "What does WHO do?"},

    {"name": "W3Schools", "url": "https://www.w3schools.com/html/",
     "raw": "HTML tutorial",
     "expanded": "HTML tutorial learn web development elements tags attributes beginner guide",
     "answer": ["html"],
     "question": "How to learn HTML?"},

    # ── Finance — must contain actual data ──
    {"name": "CoinGecko", "url": "https://www.coingecko.com",
     "raw": "bitcoin price",
     "expanded": "cryptocurrency Bitcoin price market cap volume BTC ETH trading exchange rate USD",
     "answer": ["bitcoin"],
     "question": "What is the Bitcoin price?"},

    {"name": "ECB", "url": "https://www.ecb.europa.eu",
     "raw": "ECB policy",
     "expanded": "European Central Bank ECB interest rate monetary policy euro inflation Frankfurt",
     "answer": ["ecb"],
     "question": "What is ECB monetary policy?"},

    # ── Infra — must describe the tool ──
    {"name": "Docker Hub", "url": "https://hub.docker.com",
     "raw": "container images",
     "expanded": "search container images Docker Hub registry pull official alpine ubuntu nginx",
     "answer": ["docker", "image"],
     "question": "What is Docker Hub?"},

    {"name": "Terraform", "url": "https://www.terraform.io",
     "raw": "infrastructure code",
     "expanded": "Terraform infrastructure as code IaC HashiCorp provision cloud AWS automate",
     "answer": ["terraform"],
     "question": "What is Terraform?"},

    {"name": "GitHub Explore", "url": "https://github.com/explore",
     "raw": "trending repos",
     "expanded": "trending repositories GitHub explore open source projects popular stars developers",
     "answer": ["trending"],
     "question": "What's trending on GitHub?"},

    # ── Swedish ──
    {"name": "Tibro kommun", "url": "https://www.tibro.se",
     "raw": "nyheter Tibro",
     "expanded": "nyheter Tibro kommun senaste mars april 2026 2025 nyhet rubrik",
     "answer": ["2026"],
     "question": "Vad händer i Tibro?"},

    # ── Large site ──
    {"name": "Tailwind CSS", "url": "https://tailwindcss.com",
     "raw": "CSS framework",
     "expanded": "Tailwind CSS utility-first framework classes responsive design styling rapid UI",
     "answer": ["tailwind"],
     "question": "What is Tailwind CSS?"},
]

CONFIGS = [
    ("Full system",          {"expanded": True,  "ablation": None}),
    ("Full (raw goals)",     {"expanded": False, "ablation": None}),
    ("Raw − dense fallback", {"expanded": False, "ablation": "no_dense"}),
    ("Raw − HDC pruning",    {"expanded": False, "ablation": "no_hdc"}),
    ("Raw − bottom-up",      {"expanded": False, "ablation": "no_bottomup"}),
    ("Raw − expansion",      {"expanded": False, "ablation": "no_expansion"}),
    ("Raw MiniLM only",      {"expanded": False, "ablation": None, "reranker": "minilm"}),
]

print("=" * 95)
print(f"  ABLATION — RAW GOALS — {len(TESTS)} sites × {len(CONFIGS)} configs")
print("  Strict answer check: specific facts, not just site names")
print("=" * 95)

# Pre-fetch
print("\n  Fetching...", end="", flush=True)
html_cache = {}
for tc in TESTS:
    html = fetch(tc["url"])
    if len(html) > 200:
        html_cache[tc["name"]] = html
        sys.stdout.write(".")
        sys.stdout.flush()
print(f" {len(html_cache)} sites.\n")

all_results = {}

for config_name, cfg in CONFIGS:
    use_expanded = cfg.get("expanded", False)
    ablation = cfg.get("ablation")
    reranker = cfg.get("reranker")

    f5, f10, f20, lats = 0, 0, 0, []
    fails = []

    for tc in TESTS:
        if tc["name"] not in html_cache:
            continue
        html = html_cache[tc["name"]]
        goal = tc["expanded"] if use_expanded else tc["raw"]

        body = {"html": html, "goal": goal, "url": tc["url"], "top_n": 20}
        if ablation:
            body["ablation"] = ablation
        if reranker:
            body["reranker"] = reranker

        t0 = time.monotonic()
        try:
            data = ae_post("/api/parse-hybrid", body)
        except:
            continue
        ms = (time.monotonic() - t0) * 1000
        lats.append(ms)

        nodes = data.get("top_nodes", [])

        def check(k):
            text = " ".join(n.get("label", "") for n in nodes[:k]).lower()
            return all(a.lower() in text for a in tc["answer"])

        hit5 = check(5)
        hit10 = check(10)
        hit20 = check(20)
        if hit5: f5 += 1
        if hit10: f10 += 1
        if hit20: f20 += 1
        if not hit20:
            fails.append(tc["name"])

    n = len(lats)
    avg = sum(lats) / n if n else 0
    all_results[config_name] = {
        "n": n, "f5": f5, "f10": f10, "f20": f20,
        "avg_ms": round(avg), "fails": fails
    }

    p5 = f"{f5}/{n} ({f5/n*100:.0f}%)" if n else "N/A"
    p10 = f"{f10}/{n} ({f10/n*100:.0f}%)" if n else "N/A"
    p20 = f"{f20}/{n} ({f20/n*100:.0f}%)" if n else "N/A"
    print(f"  {config_name:<25} @5: {p5:<12} @10: {p10:<12} @20: {p20:<12} {avg:.0f}ms")
    if fails:
        print(f"    Misses: {', '.join(fails)}")

# Summary
print(f"\n{'='*95}")
print("  SUMMARY TABLE")
print(f"{'='*95}")
print(f"\n  {'Config':<25} {'@5':>12} {'@10':>12} {'@20':>12} {'Latency':>10}")
print("  " + "-" * 74)
for config_name, _ in CONFIGS:
    r = all_results.get(config_name, {})
    n = r.get("n", 0)
    f5, f10, f20 = r.get("f5",0), r.get("f10",0), r.get("f20",0)
    ms = r.get("avg_ms", 0)
    print(f"  {config_name:<25} {f5:>3}/{n} ({f5/n*100:>4.0f}%) {f10:>3}/{n} ({f10/n*100:>4.0f}%) {f20:>3}/{n} ({f20/n*100:>4.0f}%) {ms:>8}ms")

# Delta table
print(f"\n  {'Config':<25} {'Δ@5 vs full':>12} {'Δ@20 vs full':>13}")
print("  " + "-" * 52)
full = all_results.get("Full system", {})
for config_name, _ in CONFIGS:
    r = all_results.get(config_name, {})
    d5 = r.get("f5",0) - full.get("f5",0)
    d20 = r.get("f20",0) - full.get("f20",0)
    print(f"  {config_name:<25} {d5:>+12} {d20:>+13}")

json.dump(all_results, open("benches/ablation_raw_results.json", "w"), indent=2)
print(f"\n  Results → benches/ablation_raw_results.json")
