#!/usr/bin/env python3
"""
CRFR Live Verification — 20 riktiga sajter via parse_crfr HTTP endpoint.
Verifierar: finns svaret på frågan i CRFR:s output?
"""
import json, time, urllib.request, sys

AE = "http://127.0.0.1:3000"

def fetch_html(url):
    """Hämta HTML via AetherAgent fetch endpoint"""
    body = json.dumps({"url": url}).encode()
    req = urllib.request.Request(
        f"{AE}/api/fetch", body,
        headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            data = json.loads(r.read().decode())
            return data.get("body", "")
    except Exception as e:
        return None

def crfr(html, url, goal, top_n=20):
    """Anropa parse_crfr med HTML"""
    body = json.dumps({"html": html, "url": url, "goal": goal, "top_n": top_n}).encode()
    req = urllib.request.Request(
        f"{AE}/api/parse-crfr", body,
        headers={"Content-Type": "application/json"}
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return json.loads(r.read().decode())
    except Exception as e:
        return {"error": str(e), "nodes": []}

TESTS = [
    {
        "name": "Wikipedia — Malmö invånare",
        "url": "https://sv.wikipedia.org/wiki/Malm%C3%B6",
        "goal": "Malmö invånare befolkning folkmängd population antal kommun 2024 2025",
        "must_contain": ["invånare"],
        "question": "Hur många invånare har Malmö?",
    },
    {
        "name": "Wikipedia — Sveriges huvudstad",
        "url": "https://en.wikipedia.org/wiki/Sweden",
        "goal": "Sweden capital city Stockholm huvudstad country Scandinavia",
        "must_contain": ["stockholm"],
        "question": "Vad är Sveriges huvudstad?",
    },
    {
        "name": "Hacker News — toppnyheter",
        "url": "https://news.ycombinator.com",
        "goal": "top stories articles submissions Hacker News points comments hours ago",
        "must_contain": ["point"],
        "question": "Vilka är de senaste nyheterna på HN?",
    },
    {
        "name": "rust-lang.org — installation",
        "url": "https://www.rust-lang.org",
        "goal": "install Rust rustup curl cargo toolchain download get started",
        "must_contain": ["rustup"],
        "question": "Hur installerar man Rust?",
    },
    {
        "name": "python.org — version",
        "url": "https://www.python.org",
        "goal": "Python latest version download 3.12 3.13 release programming language",
        "must_contain": ["python"],
        "question": "Vilken är senaste Python-versionen?",
    },
    {
        "name": "MDN — HTML element",
        "url": "https://developer.mozilla.org/en-US/docs/Web/HTML",
        "goal": "HTML HyperText Markup Language elements tags web standard reference",
        "must_contain": ["html"],
        "question": "Vad är HTML?",
    },
    {
        "name": "GitHub — trending",
        "url": "https://github.com/trending",
        "goal": "trending repositories stars today popular open source projects GitHub",
        "must_contain": ["star"],
        "question": "Vilka repos trendar på GitHub?",
    },
    {
        "name": "lobste.rs — tekniknyheter",
        "url": "https://lobste.rs",
        "goal": "latest technology articles programming submissions comments points lobsters",
        "must_contain": ["comment"],
        "question": "Senaste artiklarna på Lobsters?",
    },
    {
        "name": "PyPI — paketindex",
        "url": "https://pypi.org",
        "goal": "Python Package Index PyPI pip install packages projects repository",
        "must_contain": ["package"],
        "question": "Vad är PyPI?",
    },
    {
        "name": "crates.io — Rust paket",
        "url": "https://crates.io",
        "goal": "Rust crates packages registry cargo downloads community",
        "must_contain": ["crate"],
        "question": "Vad är crates.io?",
    },
    {
        "name": "BBC News — senaste",
        "url": "https://www.bbc.com/news",
        "goal": "latest news headlines BBC breaking world UK stories today",
        "must_contain": ["news"],
        "question": "Senaste nyheterna på BBC?",
    },
    {
        "name": "SVT Nyheter — Sverige",
        "url": "https://www.svt.se/nyheter",
        "goal": "senaste nyheter Sverige SVT rubriker idag aktuellt",
        "must_contain": ["svt"],
        "question": "Senaste nyheterna på SVT?",
    },
    {
        "name": "Stack Overflow — frågor",
        "url": "https://stackoverflow.com",
        "goal": "programming questions answers Stack Overflow developer community ask",
        "must_contain": ["question"],
        "question": "Vad är Stack Overflow?",
    },
    {
        "name": "Expressen — nyheter",
        "url": "https://www.expressen.se",
        "goal": "senaste nyheter rubriker Expressen Sverige idag",
        "must_contain": ["expressen"],
        "question": "Senaste nyheterna på Expressen?",
    },
    {
        "name": "DN — nyheter",
        "url": "https://www.dn.se",
        "goal": "Dagens Nyheter DN senaste nyheter rubriker Stockholm Sverige",
        "must_contain": ["nyheter"],
        "question": "Senaste nyheterna på DN?",
    },
    {
        "name": "Arch Wiki — pacman",
        "url": "https://wiki.archlinux.org/title/Pacman",
        "goal": "pacman package manager Arch Linux install update sync command",
        "must_contain": ["pacman"],
        "question": "Vad är pacman?",
    },
    {
        "name": "Docker Docs — vad är Docker",
        "url": "https://docs.docker.com/get-started/overview/",
        "goal": "Docker container platform overview what is Docker images containers",
        "must_contain": ["container"],
        "question": "Vad är Docker?",
    },
    {
        "name": "Node.js — version",
        "url": "https://nodejs.org",
        "goal": "Node.js download latest version LTS JavaScript runtime server",
        "must_contain": ["node"],
        "question": "Vilken är senaste Node.js-versionen?",
    },
    {
        "name": "Go.dev — paketsök",
        "url": "https://go.dev",
        "goal": "Go programming language Google download build reliable software",
        "must_contain": ["go"],
        "question": "Vad är Go?",
    },
    {
        "name": "Aftonbladet — nyheter",
        "url": "https://www.aftonbladet.se",
        "goal": "senaste nyheter rubriker Aftonbladet Sverige idag sport nöje",
        "must_contain": ["aftonbladet"],
        "question": "Senaste nyheterna på Aftonbladet?",
    },
]

print("=" * 100)
print("  CRFR LIVE VERIFICATION — 20 sajter via parse_crfr")
print("  Fråga: Finns svaret i CRFR:s top-20 output?")
print("=" * 100)
print()

results = []

for i, tc in enumerate(TESTS):
    t0 = time.monotonic()
    html = fetch_html(tc["url"])
    if not html or len(html) < 100:
        ms = (time.monotonic() - t0) * 1000
        print(f"  [{i+1:>2}/20] {tc['name']:<35} FETCH FAIL ({ms:.0f}ms)")
        results.append({"name": tc["name"], "status": "FAIL"})
        print()
        continue
    fetch_ms = (time.monotonic() - t0) * 1000
    data = crfr(html, tc["url"], tc["goal"])
    ms = (time.monotonic() - t0) * 1000

    nodes = data.get("nodes", [])
    error = data.get("error")
    crfr_meta = data.get("crfr", {})
    total_nodes = data.get("total_nodes", 0)

    if error and not nodes:
        print(f"  [{i+1:>2}/20] {tc['name']:<35} FETCH FAIL: {str(error)[:60]}")
        results.append({"name": tc["name"], "status": "FAIL", "error": str(error)[:80]})
        continue

    # Sammanfoga alla nod-labels
    all_text = " ".join(n.get("label", "") for n in nodes).lower()

    # Kolla alla must_contain
    hits = [kw for kw in tc["must_contain"] if kw.lower() in all_text]
    found = len(hits) == len(tc["must_contain"])

    # Hitta vilken nod som matchar
    answer_rank = None
    answer_text = None
    for j, n in enumerate(nodes):
        label_lower = n.get("label", "").lower()
        if all(kw.lower() in label_lower for kw in tc["must_contain"]):
            answer_rank = j + 1
            answer_text = n.get("label", "")[:100]
            break

    status = "OK" if found else "MISS"
    cache = "C" if crfr_meta.get("cache_hit") else " "

    print(f"  [{i+1:>2}/20] {tc['name']:<35} [{status:>4}] {ms:>6.0f}ms  {len(nodes):>2}n/{total_nodes}  [{cache}]")
    print(f"         Q: {tc['question']}")

    if found and answer_rank:
        trunc = answer_text[:90] if answer_text else ""
        print(f"         A: (rank {answer_rank}) {trunc}")
    elif found:
        # Svaret finns spritt över flera noder
        print(f"         A: (spritt i top-{len(nodes)} noder)")
    else:
        # Visa top-1 vid miss
        if nodes:
            top1 = nodes[0].get("label", "")[:90]
            print(f"         top-1: {top1}")
        print(f"         Sökte: {tc['must_contain']}")

    print()

    results.append({
        "name": tc["name"],
        "status": status,
        "ms": round(ms),
        "nodes": len(nodes),
        "total_nodes": total_nodes,
        "found_rank": answer_rank,
    })

# Sammanfattning
print("=" * 100)
print("  SAMMANFATTNING")
print("=" * 100)

ok = sum(1 for r in results if r.get("status") == "OK")
fail = sum(1 for r in results if r.get("status") == "FAIL")
miss = sum(1 for r in results if r.get("status") == "MISS")
total = len(results)

print(f"\n  Svar hittat:    {ok}/{total} ({ok/total*100:.0f}%)")
print(f"  Missar:         {miss}/{total}")
print(f"  Fetch failures: {fail}/{total}")

if [r for r in results if r.get("status") == "OK"]:
    avg_ms = sum(r["ms"] for r in results if r.get("status") == "OK") / ok
    avg_rank = sum(r.get("found_rank", 0) or 0 for r in results if r.get("status") == "OK" and r.get("found_rank"))
    ranked = sum(1 for r in results if r.get("status") == "OK" and r.get("found_rank"))
    if ranked:
        avg_rank /= ranked
    print(f"  Avg latens:     {avg_ms:.0f}ms")
    if ranked:
        print(f"  Avg rank:       {avg_rank:.1f}")

print()

# Spara resultat
with open("benches/crfr_live_results.json", "w") as f:
    json.dump(results, f, indent=2, ensure_ascii=False)
print("  Resultat sparade i benches/crfr_live_results.json")
