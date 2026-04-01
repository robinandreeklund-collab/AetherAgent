#!/usr/bin/env python3
"""
REAL Ablation — measures what actually matters:
1. Is the CORRECT node top-1? (not just "keyword exists somewhere")
2. What TYPE of node is top-1? (text/fact vs heading/nav/wrapper)
3. Latency on large DOMs (where HDC matters)
4. Score separation (top-1 vs top-5 gap)
"""
import json, time, urllib.request, sys

AE = "http://127.0.0.1:3000"

def ae_post(ep, body):
    data = json.dumps(body).encode()
    req = urllib.request.Request(f"{AE}{ep}", data=data, headers={"Content-Type":"application/json"})
    with urllib.request.urlopen(req, timeout=120) as r:
        return json.loads(r.read().decode())

def fetch(url):
    try: return ae_post("/api/fetch", {"url": url}).get("body", "")
    except: return ""

# Sites chosen for specific reasons — each tests a component
TESTS = [
    # ── HDC matters: large DOMs ──
    {"name": "Investing.com", "url": "https://www.investing.com",
     "goal": "stock market indices S&P 500 Dow Jones NASDAQ trading live",
     "fact_keywords": ["market", "s&p", "dow", "nasdaq"],
     "why": "27K nodes — without HDC, ColBERT must score thousands"},

    {"name": "Tailwind CSS", "url": "https://tailwindcss.com",
     "goal": "Tailwind CSS utility-first framework classes responsive design",
     "fact_keywords": ["tailwind", "utility", "css"],
     "why": "9K nodes — HDC critical for pruning"},

    {"name": "CoinGecko", "url": "https://www.coingecko.com",
     "goal": "cryptocurrency Bitcoin price market cap volume BTC USD",
     "fact_keywords": ["bitcoin", "price", "$"],
     "why": "1.5K nodes — needs fact extraction from data-heavy page"},

    # ── Bottom-up matters: wrapper-heavy DOMs ──
    {"name": "GOV.UK Wage", "url": "https://www.gov.uk/national-minimum-wage-rates",
     "goal": "minimum wage 2025 National Living Wage hourly rate £12.21 £12.71",
     "fact_keywords": ["£12", "per hour", "wage"],
     "why": "Deep wrappers — without bottom-up, nav/footer dominates"},

    {"name": "Bank of England", "url": "https://www.bankofengland.co.uk/monetary-policy/the-interest-rate-bank-rate",
     "goal": "current interest rate Bank Rate percentage 3.75 monetary policy",
     "fact_keywords": ["3.75%", "bank rate"],
     "why": "Footer address used to rank #1 without bottom-up"},

    # ── Dense fallback matters: weak BM25 match ──
    {"name": "Hacker News", "url": "https://news.ycombinator.com",
     "goal": "latest news",  # Deliberately SHORT — BM25 will struggle
     "fact_keywords": ["points", "ago", "comments"],
     "why": "Short goal — BM25 finds <20 candidates, dense fallback needed"},

    {"name": "lobste.rs", "url": "https://lobste.rs",
     "goal": "technology articles",  # Deliberately SHORT
     "fact_keywords": ["hours ago", "comments"],
     "why": "Short goal — BM25 finds ~1 candidate without dense fallback"},

    # ── Expansion matters: synonym gap ──
    {"name": "PyPI", "url": "https://pypi.org",
     "goal": "find Python packages",
     "fact_keywords": ["python", "package", "install"],
     "why": "Simple goal — expansion adds pip/install/repository"},

    {"name": "rust-lang", "url": "https://www.rust-lang.org",
     "goal": "Rust programming",
     "fact_keywords": ["rust", "memory", "fast"],
     "why": "Short goal — expansion adds cargo/rustup/safe"},

    # ── General quality ──
    {"name": "NASA", "url": "https://www.nasa.gov",
     "goal": "NASA space missions moon Mars Artemis",
     "fact_keywords": ["nasa", "artemis"],
     "why": "Government site with mixed content"},

    {"name": "Docker Hub", "url": "https://hub.docker.com",
     "goal": "search container images Docker Hub pull official",
     "fact_keywords": ["docker", "image", "container"],
     "why": "Commercial site with marketing + data mix"},

    {"name": "Tibro kommun", "url": "https://www.tibro.se",
     "goal": "nyheter Tibro kommun 2026",
     "fact_keywords": ["2026", "tibro"],
     "why": "Swedish municipal site — language diversity"},
]

CONFIGS = [
    ("Full system",       {}),
    ("− Dense fallback",  {"ablation": "no_dense"}),
    ("− HDC pruning",     {"ablation": "no_hdc"}),
    ("− Bottom-up",       {"ablation": "no_bottomup"}),
    ("− Expansion",       {"ablation": "no_expansion"}),
    ("MiniLM only",       {"reranker": "minilm"}),
]

print("=" * 110)
print(f"  REAL ABLATION — {len(TESTS)} sites × {len(CONFIGS)} configs")
print("  Measures: top-1 node type, fact presence, latency, score gap")
print("=" * 110)

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

all_data = {}

for config_name, cfg in CONFIGS:
    print(f"\n{'━'*110}")
    print(f"  {config_name}")
    print(f"{'━'*110}")

    config_data = []

    for tc in TESTS:
        if tc["name"] not in html_cache:
            continue
        html = html_cache[tc["name"]]

        body = {"html": html, "goal": tc["goal"], "url": tc["url"], "top_n": 20}
        body.update(cfg)

        t0 = time.monotonic()
        try:
            data = ae_post("/api/parse-hybrid", body)
        except Exception as e:
            print(f"  {tc['name']:<20} ERROR: {e}")
            continue
        ms = (time.monotonic() - t0) * 1000

        nodes = data.get("top_nodes", [])
        total = data.get("total_nodes", 0)
        pipeline = data.get("pipeline", {})
        surv = pipeline.get("hdc_survivors", 0)
        cand = pipeline.get("bm25_candidates", 0)

        # Top-1 analysis
        top1 = nodes[0] if nodes else {}
        top1_role = top1.get("role", "?")
        top1_score = top1.get("relevance", 0)
        top1_label = top1.get("label", "")[:70]
        top1_is_fact = top1_role in ("text", "data", "cell", "row", "listitem")

        # Fact in top-5?
        top5_text = " ".join(n.get("label", "") for n in nodes[:5]).lower()
        fact_found = all(kw.lower() in top5_text for kw in tc["fact_keywords"])

        # Score gap (top-1 vs top-5)
        scores = [n.get("relevance", 0) for n in nodes[:5]]
        score_gap = (scores[0] - scores[-1]) if len(scores) >= 5 else 0

        role_mark = "📄" if top1_is_fact else "⚠️"
        fact_mark = "✅" if fact_found else "❌"

        print(f"  {tc['name']:<20} {role_mark}{top1_role:<10} [{top1_score:.3f}] gap:{score_gap:.3f} {fact_mark} {ms:>6.0f}ms cand:{cand:>3} surv:{surv:>3} dom:{total:>5}  {top1_label}")

        config_data.append({
            "name": tc["name"],
            "top1_role": top1_role,
            "top1_score": top1_score,
            "top1_is_fact": top1_is_fact,
            "fact_found": fact_found,
            "score_gap": round(score_gap, 3),
            "ms": round(ms),
            "candidates": cand,
            "survivors": surv,
            "total_nodes": total,
        })

    all_data[config_name] = config_data

# ── Summary ──
print(f"\n\n{'='*110}")
print("  SUMMARY")
print(f"{'='*110}\n")

print(f"  {'Config':<22} {'Fact@5':>8} {'Top1=fact':>10} {'Avg score':>10} {'Avg gap':>9} {'Avg ms':>8}")
print("  " + "-" * 72)
for config_name, _ in CONFIGS:
    cd = all_data.get(config_name, [])
    n = len(cd)
    if n == 0: continue
    fact5 = sum(1 for d in cd if d["fact_found"])
    top1fact = sum(1 for d in cd if d["top1_is_fact"])
    avg_score = sum(d["top1_score"] for d in cd) / n
    avg_gap = sum(d["score_gap"] for d in cd) / n
    avg_ms = sum(d["ms"] for d in cd) / n
    print(f"  {config_name:<22} {fact5:>3}/{n} ({fact5/n*100:>3.0f}%) {top1fact:>4}/{n} ({top1fact/n*100:>3.0f}%) {avg_score:>9.3f} {avg_gap:>9.3f} {avg_ms:>7.0f}ms")

# Component impact
print(f"\n  COMPONENT IMPACT (delta vs full system):")
print(f"  {'Config':<22} {'ΔFact@5':>8} {'ΔTop1=fact':>11} {'ΔLatency':>10}")
print("  " + "-" * 54)
full = all_data.get("Full system", [])
full_f5 = sum(1 for d in full if d["fact_found"])
full_t1 = sum(1 for d in full if d["top1_is_fact"])
full_ms = sum(d["ms"] for d in full) / len(full) if full else 0
for config_name, _ in CONFIGS:
    cd = all_data.get(config_name, [])
    if not cd: continue
    f5 = sum(1 for d in cd if d["fact_found"])
    t1 = sum(1 for d in cd if d["top1_is_fact"])
    ms = sum(d["ms"] for d in cd) / len(cd)
    print(f"  {config_name:<22} {f5-full_f5:>+8} {t1-full_t1:>+11} {ms-full_ms:>+9.0f}ms")

json.dump(all_data, open("benches/ablation_real_results.json", "w"), indent=2)
print(f"\n  Results → benches/ablation_real_results.json")
