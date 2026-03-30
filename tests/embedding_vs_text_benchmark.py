#!/usr/bin/env python3
"""
Embedding vs Text Similarity Benchmark — AetherAgent
=====================================================

Jämför all-MiniLM-L6-v2 (ONNX qint8) mot befintlig text_similarity (word-overlap)
via AetherAgent MCP-server (stdio JSON-RPC).

Testmatris:
  - 20 lokala HTML-tester (svenska + engelska, varierade domäner)
  - 5 riktiga sajter (HTML hämtad via curl, parsad via MCP)
  - Mäter: relevance score, top-N-ranking, semantisk precision

Kör:
  python3 tests/embedding_vs_text_benchmark.py
"""

import json
import os
import subprocess
import sys
import time
import select

# ─── Konfiguration ────────────────────────────────────────────────────────────

ROOT = os.path.dirname(os.path.abspath(os.path.join(__file__, "..")))
MCP_BINARY = os.path.join(ROOT, "target", "server-release", "aether-mcp")
VISION_MODEL = os.path.join(ROOT, "aether-ui-latest.onnx")
EMBEDDING_MODEL = os.path.join(ROOT, "models", "all-MiniLM-L6-v2-qint8.onnx")
EMBEDDING_VOCAB = os.path.join(ROOT, "models", "vocab.txt")


# ─── MCP-klient (Popen med persistent session) ──────────────────────────────

class McpClient:
    """MCP JSON-RPC klient via stdio."""

    def __init__(self, use_embeddings: bool):
        env = os.environ.copy()
        env["AETHER_MODEL_PATH"] = VISION_MODEL
        if use_embeddings:
            env["AETHER_EMBEDDING_MODEL"] = EMBEDDING_MODEL
            env["AETHER_EMBEDDING_VOCAB"] = EMBEDDING_VOCAB
        else:
            env.pop("AETHER_EMBEDDING_MODEL", None)
            env.pop("AETHER_EMBEDDING_VOCAB", None)

        self.proc = subprocess.Popen(
            [MCP_BINARY],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            text=True, env=env
        )
        self.next_id = 1
        self._initialize()

    def _send(self, msg: dict):
        self.proc.stdin.write(json.dumps(msg) + '\n')
        self.proc.stdin.flush()

    def _recv(self, timeout: float = 15.0):
        ready, _, _ = select.select([self.proc.stdout], [], [], timeout)
        if ready:
            line = self.proc.stdout.readline().strip()
            if line:
                return json.loads(line)
        return None

    def _initialize(self):
        self._send({
            "jsonrpc": "2.0", "id": self._next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "embedding-bench", "version": "1.0"}
            }
        })
        resp = self._recv(10)
        if not resp:
            raise RuntimeError("MCP initialize timeout")
        self._send({"jsonrpc": "2.0", "method": "notifications/initialized"})
        time.sleep(0.1)

    def _next_id(self) -> int:
        self.next_id += 1
        return self.next_id - 1

    def call_tool(self, name: str, args: dict, timeout: float = 30.0) -> dict:
        msg_id = self._next_id()
        self._send({
            "jsonrpc": "2.0", "id": msg_id,
            "method": "tools/call",
            "params": {"name": name, "arguments": args}
        })
        resp = self._recv(timeout)
        if not resp:
            return {"error": "MCP timeout"}
        result = resp.get("result", {})
        content = result.get("content", [])
        for item in content:
            if item.get("type") == "text":
                try:
                    return json.loads(item["text"])
                except json.JSONDecodeError:
                    return {"raw": item["text"]}
        return {"error": "Inget text-content i svar"}

    def close(self):
        try:
            self.proc.terminate()
            self.proc.wait(timeout=3)
        except Exception:
            self.proc.kill()


# ─── Hjälpfunktioner ─────────────────────────────────────────────────────────

def find_nodes_recursive(tree, max_depth=10):
    """Hämta alla noder rekursivt ur ett semantic tree."""
    nodes = []
    if isinstance(tree, dict):
        if "role" in tree and "label" in tree:
            nodes.append(tree)
        for child in tree.get("children", []):
            if max_depth > 0:
                nodes.extend(find_nodes_recursive(child, max_depth - 1))
        # parse-resultat har 'nodes' som toppnivå
        for child in tree.get("nodes", []):
            if max_depth > 0:
                nodes.extend(find_nodes_recursive(child, max_depth - 1))
    elif isinstance(tree, list):
        for item in tree:
            nodes.extend(find_nodes_recursive(item, max_depth))
    return nodes


def get_top_relevant(tree_result: dict, n: int = 5) -> list:
    """Returnera top-N noder sorterade efter relevance score."""
    nodes = find_nodes_recursive(tree_result)
    scored = [(node, node.get("relevance", 0.0)) for node in nodes if node.get("label", "").strip()]
    scored.sort(key=lambda x: x[1], reverse=True)
    return scored[:n]


def fetch_html(url: str, timeout: int = 15) -> str:
    """Hämta HTML med curl (pålitligare än reqwest i sandbox)."""
    try:
        result = subprocess.run(
            ["curl", "-sL", "--compressed", "--max-time", str(timeout), "-A",
             "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
             url],
            capture_output=True, text=True, timeout=timeout + 5
        )
        return result.stdout
    except Exception as e:
        return f"<html><body>Fetch error: {e}</body></html>"


# ─── Testfall ────────────────────────────────────────────────────────────────

LOCAL_TESTS = [
    # ─── Synonymer (svenska) ─────────────────────────────────────────────────
    {"name": "SV1: 'pris' → 'kostnad'", "goal": "hitta priset",
     "html": '<div><span class="product-cost">Kostnad: 299 kr</span><span class="titel">Produkt A</span><a href="/about">Om oss</a></div>',
     "expect": "299", "cat": "synonym-sv"},

    {"name": "SV2: 'kontakt' → 'telefonnummer'", "goal": "hitta kontaktinformation",
     "html": '<div><h2>Kundtjänst</h2><p>Telefon: 08-123 456</p><p>Adress: Sveavägen 1</p><a href="/faq">Vanliga frågor</a></div>',
     "expect": "08-123", "cat": "synonym-sv"},

    {"name": "SV3: 'köpa' → 'lägg i varukorg'", "goal": "köpa produkten",
     "html": '<div><h1>Blå Löparskor</h1><button>Lägg i varukorgen</button><button>Spara till önskelista</button><a href="/">Tillbaka</a></div>',
     "expect": "varukorgen", "cat": "synonym-sv"},

    {"name": "SV4: 'logga in' → 'sign in'", "goal": "logga in på kontot",
     "html": '<nav><a href="/login">Sign in</a><a href="/register">Create account</a><a href="/">Home</a></nav>',
     "expect": "Sign in", "cat": "synonym-sv"},

    {"name": "SV5: 'sök' → 'search'", "goal": "sök efter produkter",
     "html": '<div><form><input type="search" placeholder="Search products..."><button type="submit">Go</button></form><nav><a href="/all">Browse All</a></nav></div>',
     "expect": "Search", "cat": "synonym-sv"},

    # ─── Synonymer (engelska) ────────────────────────────────────────────────
    {"name": "EN1: 'purchase' → 'add to cart'", "goal": "purchase this item",
     "html": '<div><h1>Blue Shoes</h1><p class="price">$89.99</p><button class="cta">Add to cart</button><button>Share</button><button>Report</button></div>',
     "expect": "cart", "cat": "synonym-en"},

    {"name": "EN2: 'pricing' → '$29/month'", "goal": "find pricing information",
     "html": '<div><div class="plan"><h3>Pro Plan</h3><p>$29/month — unlimited access</p></div><div class="faq"><h3>FAQ</h3><p>Common questions</p></div></div>',
     "expect": "$29", "cat": "synonym-en"},

    {"name": "EN3: 'employment' → 'job openings'", "goal": "find employment opportunities",
     "html": '<nav><a href="/careers">Job Openings</a><a href="/about">About Us</a><a href="/blog">Blog</a><a href="/contact">Contact</a></nav>',
     "expect": "Job", "cat": "synonym-en"},

    {"name": "EN4: 'feedback' → 'leave a review'", "goal": "give feedback about the product",
     "html": '<div><button>Leave a Review</button><button>Contact Support</button><button>Return Item</button><a href="/warranty">Warranty</a></div>',
     "expect": "Review", "cat": "synonym-en"},

    {"name": "EN5: 'shipping' → 'delivery options'", "goal": "check shipping details",
     "html": '<div><a href="/delivery">Delivery Options & Tracking</a><a href="/returns">Return Policy</a><a href="/faq">FAQ</a></div>',
     "expect": "Delivery", "cat": "synonym-en"},

    # ─── Djupare semantik ────────────────────────────────────────────────────
    {"name": "SEM1: 'nutrition' → 'calories'", "goal": "nutrition facts",
     "html": '<div><h2>Nutrition Information</h2><table><tr><td>Calories</td><td>250 kcal</td></tr><tr><td>Fat</td><td>12g</td></tr></table><p>Ingredients: flour, sugar, eggs</p></div>',
     "expect": "Calorie", "cat": "semantic"},

    {"name": "SEM2: 'author' → 'written by'", "goal": "find the author",
     "html": '<article><h1>Great Article Title</h1><p class="byline">Written by Dr. Jane Doe, 2026-03-15</p><p>Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor...</p></article>',
     "expect": "Jane", "cat": "semantic"},

    {"name": "SEM3: 'open hours' → 'mon-fri'", "goal": "when are they open",
     "html": '<div><h2>Visit Us</h2><p>Mon-Fri 9:00-17:00</p><p>Sat 10:00-14:00</p><p>Address: Main Street 42, Stockholm</p></div>',
     "expect": "Mon", "cat": "semantic"},

    {"name": "SEM4: 'customer service' → 'help center'", "goal": "reach customer service",
     "html": '<nav><a href="/help">Help Center</a><a href="/docs">API Documentation</a><a href="/blog">Company Blog</a><a href="/status">System Status</a></nav>',
     "expect": "Help", "cat": "semantic"},

    {"name": "SEM5: 'unsubscribe' → 'email preferences'", "goal": "unsubscribe from emails",
     "html": '<div><a href="/prefs">Manage Email Preferences</a><a href="/profile">Edit Profile</a><a href="/security">Security Settings</a><a href="/billing">Billing</a></div>',
     "expect": "Email Preferences", "cat": "semantic"},

    # ─── Negativa/precision-tester ───────────────────────────────────────────
    {"name": "NEG1: 'pris' → inte 'nyhet'", "goal": "hitta priset",
     "html": '<div><p>Senaste nyheten: Vi har öppnat nytt kontor i Malmö</p><p>Pris: 199 kr ink. moms</p><p>Kontakt: info@example.com</p></div>',
     "expect": "199", "cat": "negative"},

    {"name": "NEG2: 'login' → inte 'logout'", "goal": "log in to my account",
     "html": '<header><button>Log Out</button><a href="/signin">Sign In</a><a href="/help">Help</a></header>',
     "expect": "Sign In", "cat": "negative"},

    # ─── Komplexa sidor ──────────────────────────────────────────────────────
    {"name": "COMPLEX1: E-handel 15 element", "goal": "buy the blue shoes",
     "html": """<html><body>
        <nav><a href="/">Home</a><a href="/sale">Sale</a><a href="/new">New</a></nav>
        <h1>Blue Running Shoes</h1><div class="price">$89.99</div>
        <p>Lightweight, breathable mesh upper for maximum comfort</p>
        <select><option>Size 8</option><option>Size 9</option><option>Size 10</option></select>
        <button class="add-to-cart">Add to Cart</button><button class="wishlist">Save for Later</button>
        <div class="reviews"><h3>Customer Reviews</h3><p>4.5 stars (230 reviews)</p></div>
        <div class="related"><h3>You might also like</h3><a href="/red-shoes">Red Running Shoes</a></div>
        <footer><a href="/contact">Contact Us</a><a href="/privacy">Privacy Policy</a></footer>
     </body></html>""",
     "expect": "Add to Cart", "cat": "complex"},

    {"name": "COMPLEX2: Nyhetsartikel", "goal": "read the article about climate change",
     "html": """<html><body>
        <header><nav><a href="/">Start</a><a href="/sport">Sport</a><a href="/weather">Weather</a></nav></header>
        <main><article>
            <h1>Climate Change Report 2026: Key Findings</h1>
            <p class="author">By Dr. Anna Lindqvist, March 2026</p>
            <p>Global temperatures have risen by 1.5 degrees C since pre-industrial times. The report highlights several urgent actions needed to mitigate the worst effects of climate change.</p>
        </article></main>
        <aside><h3>Popular</h3><a href="/tech">Tech News</a><a href="/sport">Sports</a></aside>
        <footer><p>Copyright 2026 News Corp</p></footer>
     </body></html>""",
     "expect": "Climate", "cat": "complex"},

    {"name": "COMPLEX3: Registrering", "goal": "register a new account",
     "html": """<html><body>
        <h1>Create Your Account</h1>
        <form><label>Full Name</label><input type="text" name="name">
        <label>Email Address</label><input type="email" name="email">
        <label>Password</label><input type="password" name="password">
        <label><input type="checkbox"> I agree to Terms of Service</label>
        <button type="submit">Create Account</button></form>
        <p>Already have an account? <a href="/login">Sign in instead</a></p>
     </body></html>""",
     "expect": "Create Account", "cat": "complex"},
]

LIVE_TESTS = [
    {"name": "LIVE1: Rust-lang.org — språkbeskrivning",
     "url": "https://www.rust-lang.org/",
     "goal": "learn about the programming language features"},
    {"name": "LIVE2: Hacker News — toppnyheter",
     "url": "https://news.ycombinator.com/",
     "goal": "find the top stories"},
    {"name": "LIVE3: GitHub repo — beskrivning",
     "url": "https://github.com/nickel-org/nickel.rs",
     "goal": "find the project description and star count"},
    {"name": "LIVE4: Python.org — nedladdning",
     "url": "https://www.python.org/downloads/",
     "goal": "download the latest Python version"},
    {"name": "LIVE5: HTTPBin — API endpoints",
     "url": "https://httpbin.org/",
     "goal": "find available API endpoints"},
]


# ─── Benchmark-logik ─────────────────────────────────────────────────────────

def run_local_test(client: McpClient, test: dict) -> dict:
    start = time.time()
    result = client.call_tool("parse", {
        "html": test["html"], "goal": test["goal"], "url": "https://example.com"
    })
    elapsed = time.time() - start

    top = get_top_relevant(result, n=5)
    found = False
    target_score = 0.0
    target_rank = -1

    for i, (node, score) in enumerate(top):
        label = node.get("label", "")
        if test.get("expect") and test["expect"].lower() in label.lower():
            found = True
            target_score = score
            target_rank = i + 1
            break

    best_score = top[0][1] if top else 0.0
    best_label = top[0][0].get("label", "")[:60] if top else ""

    return {
        "found": found,
        "target_score": round(target_score, 4),
        "target_rank": target_rank,
        "best_score": round(best_score, 4),
        "best_label": best_label,
        "elapsed_ms": round(elapsed * 1000, 1),
        "top3": [{"label": n.get("label", "")[:50], "rel": round(s, 4), "role": n.get("role", "")}
                 for n, s in top[:3]],
    }


def run_live_test(client: McpClient, html: str, goal: str, url: str) -> dict:
    start = time.time()
    result = client.call_tool("parse", {"html": html, "goal": goal, "url": url})
    elapsed = time.time() - start

    if "error" in result:
        return {"error": result["error"], "elapsed_ms": round(elapsed * 1000, 1)}

    top = get_top_relevant(result, n=5)
    return {
        "elapsed_ms": round(elapsed * 1000, 1),
        "node_count": len(find_nodes_recursive(result)),
        "best_score": round(top[0][1], 4) if top else 0.0,
        "top5": [{"label": n.get("label", "")[:60], "rel": round(s, 4), "role": n.get("role", "")}
                 for n, s in top[:5]],
    }


def print_separator(title):
    print()
    print("─" * 80)
    print(f"  {title}")
    print("─" * 80)
    print()


def main():
    print("=" * 80)
    print("  EMBEDDING vs TEXT SIMILARITY BENCHMARK — AetherAgent")
    print("  Modell: all-MiniLM-L6-v2 (qint8, 22 MB, 384 dim)")
    print("=" * 80)
    print()

    # Verifiera filer
    for path, name in [(MCP_BINARY, "MCP binary"), (EMBEDDING_MODEL, "Embedding model"),
                       (EMBEDDING_VOCAB, "Vocab")]:
        if os.path.exists(path):
            print(f"  ✓ {name} ({os.path.getsize(path) / 1_048_576:.1f} MB)")
        else:
            print(f"  ✗ SAKNAS: {name} ({path})")
            sys.exit(1)
    print()

    # Starta två MCP-sessioner: med och utan embedding
    print("  Startar MCP (med embedding)...")
    client_emb = McpClient(use_embeddings=True)
    print("  Startar MCP (utan embedding — bara text_similarity)...")
    client_txt = McpClient(use_embeddings=False)
    print("  Båda MCP-sessioner redo.")

    results = {"local": [], "live": [], "summary": {}}
    embed_wins = text_wins = ties = embed_found = text_found = 0

    # ─── DEL 1: Lokala HTML-tester ───────────────────────────────────────────
    print_separator(f"DEL 1: LOKALA HTML-TESTER ({len(LOCAL_TESTS)} testfall)")

    for i, test in enumerate(LOCAL_TESTS, 1):
        print(f"  [{i:2d}/{len(LOCAL_TESTS)}] {test['name']}")
        print(f"         Goal: \"{test['goal']}\"  |  Söker: \"{test.get('expect', '?')}\"")

        r_emb = run_local_test(client_emb, test)
        r_txt = run_local_test(client_txt, test)

        if r_emb["found"]: embed_found += 1
        if r_txt["found"]: text_found += 1

        delta = r_emb["target_score"] - r_txt["target_score"]
        if delta > 0.005:
            winner = "EMB >"
            embed_wins += 1
        elif delta < -0.005:
            winner = "< TXT"
            text_wins += 1
        else:
            winner = "  =  "
            ties += 1

        f_e = "HIT" if r_emb["found"] else "---"
        f_t = "HIT" if r_txt["found"] else "---"

        print(f"         EMB: {r_emb['target_score']:.4f} rank={r_emb['target_rank']:2d} [{f_e}] ({r_emb['elapsed_ms']:5.0f}ms)"
              f"  |  TXT: {r_txt['target_score']:.4f} rank={r_txt['target_rank']:2d} [{f_t}] ({r_txt['elapsed_ms']:5.0f}ms)"
              f"  |  {winner}  Δ={delta:+.4f}")

        if r_emb["top3"] != r_txt["top3"]:
            e_labels = [n["label"][:30] for n in r_emb["top3"]]
            t_labels = [n["label"][:30] for n in r_txt["top3"]]
            if e_labels != t_labels:
                print(f"         EMB top3: {e_labels}")
                print(f"         TXT top3: {t_labels}")

        results["local"].append({
            "test": test["name"], "category": test["cat"],
            "goal": test["goal"], "expect": test.get("expect", ""),
            "embedding": r_emb, "text_sim": r_txt, "delta": round(delta, 4),
        })

    # ─── DEL 2: Live-tester ──────────────────────────────────────────────────
    print_separator(f"DEL 2: LIVE-TESTER MOT RIKTIGA SAJTER ({len(LIVE_TESTS)} sajter)")

    for i, test in enumerate(LIVE_TESTS, 1):
        print(f"  [{i}/{len(LIVE_TESTS)}] {test['name']}")
        print(f"         URL: {test['url']}")
        print(f"         Goal: \"{test['goal']}\"")

        # Hämta HTML via curl (en gång per sajt)
        print(f"         Hämtar HTML...", end=" ", flush=True)
        html = fetch_html(test["url"])
        print(f"{len(html)} bytes")

        if len(html) < 100:
            print(f"         SKIP: Kunde inte hämta HTML")
            results["live"].append({"test": test["name"], "error": "fetch failed"})
            continue

        r_emb = run_live_test(client_emb, html, test["goal"], test["url"])
        r_txt = run_live_test(client_txt, html, test["goal"], test["url"])

        results["live"].append({
            "test": test["name"], "url": test["url"], "goal": test["goal"],
            "html_size": len(html),
            "embedding": r_emb, "text_sim": r_txt,
        })

        for label, r in [("EMB", r_emb), ("TXT", r_txt)]:
            if "error" in r:
                print(f"         {label}: ERROR — {r['error']}")
            else:
                print(f"         {label}: best={r['best_score']:.4f}  nodes={r['node_count']}  ({r['elapsed_ms']:.0f}ms)")
                for n in r.get("top5", [])[:3]:
                    print(f"           [{n['rel']:.3f}] {n['role']:10s} {n['label']}")

        # Jämför: vilka noder hittar bara embedding?
        if "error" not in r_emb and "error" not in r_txt:
            emb_labels = set(n["label"][:40].lower() for n in r_emb.get("top5", []))
            txt_labels = set(n["label"][:40].lower() for n in r_txt.get("top5", []))
            only_emb = emb_labels - txt_labels
            only_txt = txt_labels - emb_labels
            if only_emb:
                print(f"         Bara EMB: {only_emb}")
            if only_txt:
                print(f"         Bara TXT: {only_txt}")
        print()

    # ─── Sammanfattning ──────────────────────────────────────────────────────
    print_separator("SAMMANFATTNING")

    total = len(LOCAL_TESTS)
    print(f"  Lokala tester ({total} st):")
    print(f"    Embedding hittar mål:  {embed_found}/{total} ({100*embed_found/total:.0f}%)")
    print(f"    Text_sim hittar mål:   {text_found}/{total} ({100*text_found/total:.0f}%)")
    print(f"    Embedding vinner:      {embed_wins}")
    print(f"    Text_sim vinner:       {text_wins}")
    print(f"    Lika:                  {ties}")
    print()

    # Genomsnittlig latens
    emb_latencies = [r["embedding"]["elapsed_ms"] for r in results["local"]]
    txt_latencies = [r["text_sim"]["elapsed_ms"] for r in results["local"]]
    print(f"  Genomsnittlig latens (lokal):")
    print(f"    Embedding: {sum(emb_latencies)/len(emb_latencies):.0f} ms/anrop")
    print(f"    Text_sim:  {sum(txt_latencies)/len(txt_latencies):.0f} ms/anrop")
    print(f"    Overhead:  {(sum(emb_latencies)-sum(txt_latencies))/len(emb_latencies):.0f} ms")
    print()

    # Latens per kategori
    cats = {}
    for r in results["local"]:
        c = r["category"]
        if c not in cats: cats[c] = {"emb_scores": [], "txt_scores": [], "emb_found": 0, "txt_found": 0, "total": 0}
        cats[c]["total"] += 1
        cats[c]["emb_scores"].append(r["embedding"]["target_score"])
        cats[c]["txt_scores"].append(r["text_sim"]["target_score"])
        if r["embedding"]["found"]: cats[c]["emb_found"] += 1
        if r["text_sim"]["found"]: cats[c]["txt_found"] += 1

    print(f"  Resultat per kategori:")
    print(f"    {'Kategori':<15s}  {'EMB hitt':>10s}  {'TXT hitt':>10s}  {'EMB avg':>10s}  {'TXT avg':>10s}")
    for c, d in sorted(cats.items()):
        emb_avg = sum(d["emb_scores"]) / d["total"] if d["total"] else 0
        txt_avg = sum(d["txt_scores"]) / d["total"] if d["total"] else 0
        print(f"    {c:<15s}  {d['emb_found']}/{d['total']:>7d}  {d['txt_found']}/{d['total']:>7d}  {emb_avg:>10.4f}  {txt_avg:>10.4f}")
    print()

    results["summary"] = {
        "local_tests": total, "live_tests": len(LIVE_TESTS),
        "embed_found": embed_found, "text_found": text_found,
        "embed_wins": embed_wins, "text_wins": text_wins, "ties": ties,
        "avg_latency_emb_ms": round(sum(emb_latencies)/len(emb_latencies), 1),
        "avg_latency_txt_ms": round(sum(txt_latencies)/len(txt_latencies), 1),
    }

    # Spara
    output_path = os.path.join(os.path.dirname(__file__), "embedding_benchmark_results.json")
    with open(output_path, "w") as f:
        json.dump(results, f, indent=2, ensure_ascii=False)
    print(f"  Resultat sparat: {output_path}")

    # Stäng klienter
    client_emb.close()
    client_txt.close()


if __name__ == "__main__":
    main()
