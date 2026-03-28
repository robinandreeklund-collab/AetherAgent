#!/usr/bin/env python3
"""
Definitive Embedding Benchmark — AetherAgent Embedding vs LightPanda
=====================================================================

50 lokala HTML-tester + 20 live sajter + prestanda-benchmark.
Kör varje motor separat (ingen resurskapplöpning).

Kör:
  python3 tests/embedding_vs_text_benchmark.py
"""

import json
import os
import re
import subprocess
import sys
import time
import select
import statistics
import threading
import http.server
import socketserver
from pathlib import Path

# ─── Konfiguration ────────────────────────────────────────────────────────────

ROOT = os.path.dirname(os.path.abspath(os.path.join(__file__, "..")))
FIXTURE_DIR = os.path.join(ROOT, "tests", "fixtures")
MCP_BINARY = os.path.join(ROOT, "target", "server-release", "aether-mcp")
# Fallback till release-katalogen om server-release inte finns
if not os.path.exists(MCP_BINARY):
    MCP_BINARY = os.path.join(ROOT, "target", "release", "aether-mcp")
VISION_MODEL = os.path.join(ROOT, "aether-ui-latest.onnx")
EMBEDDING_MODEL = os.path.join(ROOT, "models", "all-MiniLM-L6-v2-qint8.onnx")
EMBEDDING_VOCAB = os.path.join(ROOT, "models", "vocab.txt")
LIGHTPANDA_BIN = os.environ.get("LIGHTPANDA_BIN", "/tmp/lightpanda")
FIXTURE_PORT = 18767
PERF_RUNS = 100


# ─── MCP-klient ───────────────────────────────────────────────────────────────

class McpClient:
    """MCP JSON-RPC klient via stdio."""

    def __init__(self):
        env = os.environ.copy()
        env["AETHER_MODEL_PATH"] = VISION_MODEL
        env["AETHER_EMBEDDING_MODEL"] = EMBEDDING_MODEL
        env["AETHER_EMBEDDING_VOCAB"] = EMBEDDING_VOCAB

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
                "clientInfo": {"name": "embedding-bench", "version": "2.0"}
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


# ─── LightPanda-klient ────────────────────────────────────────────────────────

class LightPandaClient:
    """Wrapper kring LightPanda CLI subprocess."""

    def __init__(self, binary_path: str):
        self.binary = binary_path
        self.available = os.path.exists(binary_path)

    def fetch_semantic_tree(self, url: str, timeout: int = 30):
        """Kör: lightpanda fetch --dump semantic_tree_text <url>
        Returnerar (stdout, elapsed_ms) eller (None, elapsed_ms)."""
        start = time.monotonic()
        try:
            result = subprocess.run(
                [self.binary, "fetch", "--dump", "semantic_tree_text", url],
                capture_output=True, text=True, timeout=timeout,
            )
            elapsed = (time.monotonic() - start) * 1000
            if result.returncode == 0 and result.stdout.strip():
                return result.stdout, elapsed
            return None, elapsed
        except (subprocess.TimeoutExpired, Exception):
            return None, (time.monotonic() - start) * 1000


# ─── Fixture-server ───────────────────────────────────────────────────────────

class _QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, fmt, *args):
        pass


def start_fixture_server(directory: str, port: int):
    """Starta lokal HTTP-server som serverar HTML-filer."""
    original = os.getcwd()
    os.chdir(directory)
    server = socketserver.TCPServer(("127.0.0.1", port), _QuietHandler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    os.chdir(original)
    return server


# ─── Hjälpfunktioner ─────────────────────────────────────────────────────────

def find_nodes_recursive(tree, max_depth=10):
    """Hämta alla noder rekursivt ur ett AetherAgent semantic tree."""
    nodes = []
    if isinstance(tree, dict):
        if "role" in tree and "label" in tree:
            nodes.append(tree)
        for child in tree.get("children", []):
            if max_depth > 0:
                nodes.extend(find_nodes_recursive(child, max_depth - 1))
        for child in tree.get("nodes", []):
            if max_depth > 0:
                nodes.extend(find_nodes_recursive(child, max_depth - 1))
    elif isinstance(tree, list):
        for item in tree:
            nodes.extend(find_nodes_recursive(item, max_depth))
    return nodes


def get_top_relevant(tree_result: dict, n: int = 5) -> list:
    """Top-N noder sorterade efter relevance score."""
    nodes = find_nodes_recursive(tree_result)
    scored = [(node, node.get("relevance", 0.0)) for node in nodes if node.get("label", "").strip()]
    scored.sort(key=lambda x: x[1], reverse=True)
    return scored[:n]


def parse_lp_tree(output: str) -> list:
    """Parsa LightPandas semantic_tree_text till en lista av noder."""
    nodes = []
    for line in output.strip().split("\n"):
        line = line.strip()
        if not line:
            continue
        # Format: ID ROLE 'label' eller bara ID ROLE
        m = re.match(r"(\d+)\s+(\S+)\s*'?(.*?)'?\s*$", line)
        if m:
            nodes.append({"role": m.group(2), "label": m.group(3)})
        else:
            # Ren text-nod: 'text content'
            label = line.strip("' ")
            if label:
                nodes.append({"role": "text", "label": label})
    return nodes


def fetch_html(url: str, timeout: int = 15) -> str:
    """Hämta HTML med curl."""
    try:
        result = subprocess.run(
            ["curl", "-sL", "--compressed", "--max-time", str(timeout), "-A",
             "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36",
             url],
            capture_output=True, text=True, timeout=timeout + 5
        )
        return result.stdout
    except Exception as e:
        return f"<html><body>Fetch error: {e}</body></html>"


def load_fixture(filename: str) -> str:
    """Läs en HTML-fixture från disk."""
    path = os.path.join(FIXTURE_DIR, filename)
    with open(path, "r") as f:
        return f.read()


def fmt_ms(ms):
    if ms >= 1000:
        return f"{ms/1000:.2f}s"
    return f"{ms:.1f}ms"


# ─── 50 Lokala testfall ──────────────────────────────────────────────────────

LOCAL_TESTS = [
    # ── Befintliga fixtures (01-20) ──────────────────────────────────────────
    {"id": 1, "name": "E-handel: Köp iPhone", "file": "01_ecommerce_product.html",
     "goal": "köpa produkten", "expect": "varukorg", "cat": "ecommerce"},
    {"id": 2, "name": "Login-formulär", "file": "02_login_form.html",
     "goal": "logga in på mitt konto", "expect": "Sign In", "cat": "form"},
    {"id": 3, "name": "Sökresultat", "file": "03_search_results.html",
     "goal": "hitta sökresultaten", "expect": "result", "cat": "navigation"},
    {"id": 4, "name": "Registrering", "file": "04_registration.html",
     "goal": "skapa ett nytt konto", "expect": "Create", "cat": "form"},
    {"id": 5, "name": "Checkout", "file": "05_checkout.html",
     "goal": "slutföra mitt köp", "expect": "checkout", "cat": "ecommerce"},
    {"id": 6, "name": "Nyhetsartikel", "file": "06_news_article.html",
     "goal": "läsa artikeln", "expect": "article", "cat": "content"},
    {"id": 7, "name": "Boka flygresa", "file": "07_booking_flight.html",
     "goal": "boka billigaste flyget", "expect": "Boka", "cat": "ecommerce"},
    {"id": 8, "name": "Restaurangmeny", "file": "08_restaurant_menu.html",
     "goal": "beställa mat", "expect": "order", "cat": "ecommerce"},
    {"id": 9, "name": "Dashboard", "file": "09_dashboard.html",
     "goal": "se min kontostatus", "expect": "status", "cat": "navigation"},
    {"id": 10, "name": "Injection (dold)", "file": "10_injection_hidden.html",
     "goal": "köpa produkten", "expect": "cart", "cat": "negative"},
    {"id": 11, "name": "Injection (social)", "file": "11_injection_social.html",
     "goal": "hitta kontaktinfo", "expect": "contact", "cat": "negative"},
    {"id": 12, "name": "Banking överföring", "file": "12_banking.html",
     "goal": "överföra pengar", "expect": "verforing", "cat": "form"},
    {"id": 13, "name": "Fastighetsannons", "file": "13_real_estate.html",
     "goal": "se bostadens detaljer", "expect": "kvm", "cat": "content"},
    {"id": 14, "name": "Jobbannons", "file": "14_job_listing.html",
     "goal": "ansöka om jobbet", "expect": "ansok", "cat": "form"},
    {"id": 15, "name": "Matbutik", "file": "15_grocery_store.html",
     "goal": "lägga till matvaror i korgen", "expect": "cart", "cat": "ecommerce"},
    {"id": 16, "name": "Inställningar", "file": "16_settings_page.html",
     "goal": "ändra mitt lösenord", "expect": "password", "cat": "form"},
    {"id": 17, "name": "Wiki-artikel", "file": "17_wiki_article.html",
     "goal": "lära om Stockholms historia", "expect": "histori", "cat": "content"},
    {"id": 18, "name": "Social media", "file": "18_social_media.html",
     "goal": "posta ett meddelande", "expect": "post", "cat": "form"},
    {"id": 19, "name": "Kontaktformulär", "file": "19_contact_form.html",
     "goal": "skicka ett meddelande", "expect": "send", "cat": "form"},
    {"id": 20, "name": "Stor katalog", "file": "20_large_catalog.html",
     "goal": "köpa billigaste laptopen", "expect": "price", "cat": "complex"},

    # ── Nya fixtures (21-50) ─────────────────────────────────────────────────
    {"id": 21, "name": "SV: pris → kostnad", "file": "21_sv_price_synonym.html",
     "goal": "hitta priset", "expect": "2 499", "cat": "synonym-sv"},
    {"id": 22, "name": "SV: kontakt → telefon", "file": "22_sv_contact_synonym.html",
     "goal": "hitta kontaktuppgifter", "expect": "0771", "cat": "synonym-sv"},
    {"id": 23, "name": "SV: köpa → lägg i korgen", "file": "23_sv_buy_synonym.html",
     "goal": "köpa produkten", "expect": "korgen", "cat": "synonym-sv"},
    {"id": 24, "name": "EN: purchase → add to basket", "file": "24_en_purchase_synonym.html",
     "goal": "purchase this item", "expect": "basket", "cat": "synonym-en"},
    {"id": 25, "name": "EN: cost → $49/month", "file": "25_en_pricing_synonym.html",
     "goal": "find how much it costs", "expect": "$49", "cat": "synonym-en"},
    {"id": 26, "name": "EN: employment → careers", "file": "26_en_employment_synonym.html",
     "goal": "find employment opportunities", "expect": "Careers", "cat": "synonym-en"},
    {"id": 27, "name": "Semantic: nutrition → kalorier", "file": "27_semantic_nutrition.html",
     "goal": "check nutrition facts", "expect": "Calorie", "cat": "semantic"},
    {"id": 28, "name": "Semantic: author → byline", "file": "28_semantic_author.html",
     "goal": "who wrote this article", "expect": "Maria Chen", "cat": "semantic"},
    {"id": 29, "name": "Semantic: öppettider", "file": "29_semantic_hours.html",
     "goal": "when are they open", "expect": "Monday", "cat": "semantic"},
    {"id": 30, "name": "Semantic: unsubscribe → email prefs", "file": "30_semantic_unsubscribe.html",
     "goal": "unsubscribe from emails", "expect": "Email Preferences", "cat": "semantic"},
    {"id": 31, "name": "NEG: login ≠ logout", "file": "31_negative_login_logout.html",
     "goal": "log in to my account", "expect": "Sign In", "cat": "negative"},
    {"id": 32, "name": "NEG: pris ≠ nyhet", "file": "32_negative_price_news.html",
     "goal": "hitta priset", "expect": "14 995", "cat": "negative"},
    {"id": 33, "name": "NEG: save ≠ delete", "file": "33_negative_delete_save.html",
     "goal": "save my changes", "expect": "Save", "cat": "negative"},
    {"id": 34, "name": "E-handel: checkout i kundvagn", "file": "34_ecommerce_cart_review.html",
     "goal": "proceed to checkout", "expect": "checkout", "cat": "ecommerce"},
    {"id": 35, "name": "E-handel: jämför produkter", "file": "35_ecommerce_product_compare.html",
     "goal": "compare the two laptops", "expect": "MacBook", "cat": "ecommerce"},
    {"id": 36, "name": "Formulär: flersteg", "file": "36_form_multi_step.html",
     "goal": "go to next step", "expect": "Continue", "cat": "form"},
    {"id": 37, "name": "Content: batterikapacitet", "file": "37_content_table_specs.html",
     "goal": "find the battery capacity", "expect": "5,000 mAh", "cat": "content"},
    {"id": 38, "name": "Content: returpolicy FAQ", "file": "38_content_faq_accordion.html",
     "goal": "how do I return an item", "expect": "30 days", "cat": "content"},
    {"id": 39, "name": "SV: boka läkartid", "file": "39_sv_medical_booking.html",
     "goal": "boka läkartid", "expect": "Boka", "cat": "swedish"},
    {"id": 40, "name": "SV: deklarera skatt", "file": "40_sv_government_form.html",
     "goal": "deklarera mina skatter", "expect": "deklarera", "cat": "swedish"},
    {"id": 41, "name": "Edge: nästan tom sida", "file": "41_edge_empty_page.html",
     "goal": "find content", "expect": "Coming Soon", "cat": "edge"},
    {"id": 42, "name": "Edge: 30+ produkter", "file": "42_edge_huge_page.html",
     "goal": "find the contact form", "expect": "Send message", "cat": "edge"},
    {"id": 43, "name": "Edge: 15 nivåer djup", "file": "43_edge_deep_nesting.html",
     "goal": "click the button", "expect": "Activate", "cat": "edge"},
    {"id": 44, "name": "Edge: ingen semantisk HTML", "file": "44_edge_no_semantics.html",
     "goal": "find the main content", "expect": "web hosting", "cat": "edge"},
    {"id": 45, "name": "Complex: analytics dashboard", "file": "45_complex_dashboard.html",
     "goal": "export the report", "expect": "Export", "cat": "complex"},
    {"id": 46, "name": "Complex: email inbox", "file": "46_complex_email_inbox.html",
     "goal": "compose new email", "expect": "Compose", "cat": "complex"},
    {"id": 47, "name": "Complex: social feed", "file": "47_complex_social_feed.html",
     "goal": "create a new post", "expect": "New Post", "cat": "complex"},
    {"id": 48, "name": "Complex: code editor", "file": "48_complex_code_editor.html",
     "goal": "run the code", "expect": "Run", "cat": "complex"},
    {"id": 49, "name": "SV: hitta ingredienser", "file": "49_sv_recipe_page.html",
     "goal": "hitta ingredienserna", "expect": "blandfars", "cat": "swedish"},
    {"id": 50, "name": "Accessibility: ARIA-formulär", "file": "50_accessibility_aria.html",
     "goal": "submit the form", "expect": "Save", "cat": "accessibility"},
]

# ─── 20 Live-tester ──────────────────────────────────────────────────────────

LIVE_TESTS = [
    {"id": 1, "name": "Wikipedia: Stockholm",
     "url": "https://en.wikipedia.org/wiki/Stockholm",
     "goal": "find the population of Stockholm"},
    {"id": 2, "name": "Hacker News",
     "url": "https://news.ycombinator.com/",
     "goal": "find the top story"},
    {"id": 3, "name": "GitHub: nickel.rs",
     "url": "https://github.com/nickel-org/nickel.rs",
     "goal": "find the project description"},
    {"id": 4, "name": "Python.org downloads",
     "url": "https://www.python.org/downloads/",
     "goal": "download the latest Python version"},
    {"id": 5, "name": "Rust-lang.org",
     "url": "https://www.rust-lang.org/",
     "goal": "learn about the language features"},
    {"id": 6, "name": "SVT.se",
     "url": "https://www.svt.se/",
     "goal": "hitta senaste nyheterna"},
    {"id": 7, "name": "HTTPBin",
     "url": "https://httpbin.org/",
     "goal": "find available API endpoints"},
    {"id": 8, "name": "BBC News",
     "url": "https://www.bbc.com/",
     "goal": "read the top news story"},
    {"id": 9, "name": "Stack Overflow",
     "url": "https://stackoverflow.com/questions",
     "goal": "find questions about Python"},
    {"id": 10, "name": "Books to Scrape",
     "url": "https://books.toscrape.com/",
     "goal": "find the cheapest book"},
    {"id": 11, "name": "Quotes to Scrape",
     "url": "https://quotes.toscrape.com/",
     "goal": "find a quote about life"},
    {"id": 12, "name": "W3Schools",
     "url": "https://www.w3schools.com/",
     "goal": "learn HTML basics"},
    {"id": 13, "name": "Example.com",
     "url": "https://example.com/",
     "goal": "find the main content"},
    {"id": 14, "name": "Python docs",
     "url": "https://docs.python.org/3/",
     "goal": "find the tutorial"},
    {"id": 15, "name": "Mozilla.org",
     "url": "https://www.mozilla.org/",
     "goal": "download Firefox"},
    {"id": 16, "name": "Apple.com",
     "url": "https://www.apple.com/",
     "goal": "find the latest product"},
    {"id": 17, "name": "DN.se",
     "url": "https://www.dn.se/",
     "goal": "läsa huvudnyheten"},
    {"id": 18, "name": "Dev.to",
     "url": "https://dev.to/",
     "goal": "find articles about Rust"},
    {"id": 19, "name": "MDN Web Docs",
     "url": "https://developer.mozilla.org/en-US/docs/Web/HTML",
     "goal": "find HTML elements reference"},
    {"id": 20, "name": "arXiv.org",
     "url": "https://arxiv.org/",
     "goal": "find latest papers"},
]


# ─── Campfire Commerce HTML (för prestandatest) ─────────────────────────────

CAMPFIRE_HTML = r"""<!DOCTYPE html>
<html><head><title>Outdoor Odyssey Nomad Backpack</title></head>
<body>
<nav><ul><li><a href="#">Home</a></li><li><a href="#">Products</a></li>
<li><a href="#">About</a></li><li><a href="#">Contact</a></li></ul></nav>
<div class="single-product">
  <h1 id="product-name">Outdoor Odyssey Nomad Backpack 60 liters</h1>
  <h4 id="product-price">$244.99</h4>
  <input type="number" value="1" />
  <a href="#" class="btn">Add To Cart</a>
  <p id="product-description">The Outdoor Odyssey Nomad Backpack is a spacious 60-liter
  backpack for multi-day outdoor adventures. Padded shoulder straps, adjustable hip belt,
  multiple compartments, water-resistant materials.</p>
  <ul id="product-features">
    <li>Large 60-liter capacity</li><li>Padded shoulder straps</li>
    <li>Multiple compartments</li><li>Water-resistant materials</li>
  </ul>
</div>
<div id="product-related">
  <h4>Outdoor Odyssey Hiking Poles</h4><p>$79.99</p>
  <h4>Outdoor Odyssey Sleeping Bag</h4><p>$129.99</p>
  <h4>Outdoor Odyssey Water Bottle</h4><p>$19.99</p>
</div>
<div id="product-reviews">
  <p>I recently used the Nomad Backpack on a week-long camping trip and was thoroughly impressed.</p>
  <p>The 60-liter capacity provides plenty of room for all of my essentials.</p>
  <p>I purchased the Nomad Backpack for a two-week trip through Europe and was blown away.</p>
</div>
<footer><p>Gear up for your next adventure</p></footer>
</body></html>"""


# ─── Test-runners ────────────────────────────────────────────────────────────

def run_aether_test(client: McpClient, html: str, goal: str, expect: str) -> dict:
    """Kör ett test via AetherAgent MCP. Returnerar resultat-dict."""
    start = time.monotonic()
    result = client.call_tool("parse", {
        "html": html, "goal": goal, "url": "https://example.com"
    })
    elapsed = (time.monotonic() - start) * 1000

    if "error" in result:
        return {"found": False, "target_rank": -1, "target_score": 0.0,
                "node_count": 0, "token_count": 0, "elapsed_ms": round(elapsed, 1),
                "top5": [], "error": result.get("error", "unknown")}

    all_nodes = find_nodes_recursive(result)
    top = get_top_relevant(result, n=5)

    found = False
    target_rank = -1
    target_score = 0.0
    if expect:
        for i, (node, score) in enumerate(top):
            if expect.lower() in node.get("label", "").lower():
                found = True
                target_rank = i + 1
                target_score = score
                break

    raw_json = json.dumps(result, ensure_ascii=False)
    return {
        "found": found,
        "target_rank": target_rank,
        "target_score": round(target_score, 4),
        "node_count": len(all_nodes),
        "token_count": len(raw_json) // 4,
        "elapsed_ms": round(elapsed, 1),
        "top5": [{"label": n.get("label", "")[:60], "rel": round(s, 4),
                  "role": n.get("role", "")} for n, s in top],
        "error": None,
    }


def run_lightpanda_test(lp: LightPandaClient, url: str, expect: str) -> dict:
    """Kör ett test via LightPanda CLI. Returnerar resultat-dict."""
    output, elapsed = lp.fetch_semantic_tree(url)
    if output is None:
        return {"found": False, "target_rank": -1, "node_count": 0,
                "token_count": 0, "elapsed_ms": round(elapsed, 1),
                "top5": [], "error": "fetch_failed"}

    nodes = parse_lp_tree(output)
    found = False
    target_rank = -1
    if expect:
        for i, node in enumerate(nodes):
            if expect.lower() in node.get("label", "").lower():
                found = True
                target_rank = i + 1
                break

    return {
        "found": found,
        "target_rank": target_rank,
        "node_count": len(nodes),
        "token_count": len(output) // 4,
        "elapsed_ms": round(elapsed, 1),
        "top5": [{"label": n.get("label", "")[:60], "role": n.get("role", "")}
                 for n in nodes[:5]],
        "error": None,
    }


# ─── Utskriftsfunktioner ────────────────────────────────────────────────────

def print_header(title):
    print()
    print("=" * 80)
    print(f"  {title}")
    print("=" * 80)
    print()


def print_section(title):
    print()
    print("-" * 80)
    print(f"  {title}")
    print("-" * 80)
    print()


# ─── Main ────────────────────────────────────────────────────────────────────

def main():
    print_header("DEFINITIVE BENCHMARK: AetherAgent Embedding vs LightPanda")
    print(f"  Datum: {time.strftime('%Y-%m-%d %H:%M')}")
    print(f"  50 lokala tester | 20 live sajter | prestanda-benchmark")
    print()

    # ── Verifiera filer ──────────────────────────────────────────────────────
    missing = False
    for path, name in [(MCP_BINARY, "MCP binary"), (EMBEDDING_MODEL, "Embedding model"),
                       (EMBEDDING_VOCAB, "Vocab")]:
        if os.path.exists(path):
            sz = os.path.getsize(path) / 1_048_576
            print(f"  OK  {name} ({sz:.1f} MB)")
        else:
            print(f"  SAKNAS  {name} ({path})")
            missing = True
    if missing:
        print("\n  Kan inte köra utan MCP-binär och embedding-modell.")
        sys.exit(1)

    # ── LightPanda ───────────────────────────────────────────────────────────
    lp = LightPandaClient(LIGHTPANDA_BIN)
    if lp.available:
        print(f"  OK  LightPanda ({LIGHTPANDA_BIN})")
    else:
        print(f"  INFO  LightPanda ej installerad — kör bara AetherAgent")
        print(f"         Sätt LIGHTPANDA_BIN för att aktivera jämförelse")
    print()

    # ── Starta fixture-server (för LightPanda) ───────────────────────────────
    fixture_server = None
    if lp.available:
        fixture_server = start_fixture_server(FIXTURE_DIR, FIXTURE_PORT)
        print(f"  Fixture-server startad på port {FIXTURE_PORT}")

    results = {"meta": {"date": time.strftime("%Y-%m-%d"), "local_tests": 50,
                        "live_tests": 20, "lightpanda_available": lp.available},
               "local": [], "live": [], "performance": {}, "summary": {}}

    # ── Ladda alla fixture-filer ─────────────────────────────────────────────
    fixtures = {}
    for test in LOCAL_TESTS:
        try:
            fixtures[test["file"]] = load_fixture(test["file"])
        except FileNotFoundError:
            print(f"  VARNING: Fixture saknas: {test['file']}")
            fixtures[test["file"]] = "<html><body>Missing fixture</body></html>"

    # ══════════════════════════════════════════════════════════════════════════
    #  FAS 1: LOKALA TESTER — AetherAgent
    # ══════════════════════════════════════════════════════════════════════════
    print_header(f"FAS 1: LOKALA TESTER — AetherAgent Embedding ({len(LOCAL_TESTS)} st)")

    print("  Startar MCP-session (embedding)...", flush=True)
    client = McpClient()
    print("  MCP redo.\n")

    ae_local = {}
    for test in LOCAL_TESTS:
        html = fixtures[test["file"]]
        r = run_aether_test(client, html, test["goal"], test["expect"])
        ae_local[test["id"]] = r
        f = "HIT" if r["found"] else "---"
        rank_str = f"rank={r['target_rank']}" if r["found"] else "miss"
        print(f"  [{test['id']:2d}/50] {test['name']:<35s}  {f}  {rank_str:<8s}  "
              f"rel={r['target_score']:.3f}  {r['elapsed_ms']:5.0f}ms  "
              f"nodes={r['node_count']}  tokens={r['token_count']}")

    client.close()

    # ══════════════════════════════════════════════════════════════════════════
    #  FAS 2: LOKALA TESTER — LightPanda
    # ══════════════════════════════════════════════════════════════════════════
    lp_local = {}
    if lp.available:
        print_header(f"FAS 2: LOKALA TESTER — LightPanda ({len(LOCAL_TESTS)} st)")

        for test in LOCAL_TESTS:
            url = f"http://127.0.0.1:{FIXTURE_PORT}/{test['file']}"
            r = run_lightpanda_test(lp, url, test["expect"])
            lp_local[test["id"]] = r
            f = "HIT" if r["found"] else "---"
            rank_str = f"pos={r['target_rank']}" if r["found"] else "miss"
            print(f"  [{test['id']:2d}/50] {test['name']:<35s}  {f}  {rank_str:<8s}  "
                  f"{r['elapsed_ms']:5.0f}ms  nodes={r['node_count']}  tokens={r['token_count']}")
    else:
        print_section("FAS 2: LightPanda — HOPPAS ÖVER (ej installerad)")

    # ── Samla lokala resultat ────────────────────────────────────────────────
    for test in LOCAL_TESTS:
        entry = {"id": test["id"], "name": test["name"], "category": test["cat"],
                 "goal": test["goal"], "expect": test["expect"],
                 "aether": ae_local[test["id"]]}
        if test["id"] in lp_local:
            entry["lightpanda"] = lp_local[test["id"]]
        results["local"].append(entry)

    # ══════════════════════════════════════════════════════════════════════════
    #  FAS 3: LIVE-TESTER — AetherAgent
    # ══════════════════════════════════════════════════════════════════════════
    print_header(f"FAS 3: LIVE-TESTER — AetherAgent Embedding ({len(LIVE_TESTS)} sajter)")

    # Hämta HTML för alla sajter först
    live_html = {}
    for test in LIVE_TESTS:
        print(f"  Hämtar {test['url'][:50]}...", end=" ", flush=True)
        html = fetch_html(test["url"])
        live_html[test["id"]] = html
        print(f"{len(html)} bytes")

    print()
    print("  Startar MCP-session (embedding)...", flush=True)
    client = McpClient()
    print("  MCP redo.\n")

    ae_live = {}
    for test in LIVE_TESTS:
        html = live_html[test["id"]]
        if len(html) < 100:
            ae_live[test["id"]] = {"found": False, "target_rank": -1,
                                    "node_count": 0, "token_count": 0,
                                    "elapsed_ms": 0, "top5": [],
                                    "error": "fetch_failed"}
            print(f"  [{test['id']:2d}/20] {test['name']:<30s}  SKIP — fetch misslyckades")
            continue

        r = run_aether_test(client, html, test["goal"], "")
        ae_live[test["id"]] = r
        best = r["top5"][0]["label"][:40] if r["top5"] else "—"
        print(f"  [{test['id']:2d}/20] {test['name']:<30s}  "
              f"nodes={r['node_count']:4d}  tokens={r['token_count']:5d}  "
              f"{r['elapsed_ms']:5.0f}ms  best: {best}")

    client.close()

    # ══════════════════════════════════════════════════════════════════════════
    #  FAS 4: LIVE-TESTER — LightPanda
    # ══════════════════════════════════════════════════════════════════════════
    lp_live = {}
    if lp.available:
        print_header(f"FAS 4: LIVE-TESTER — LightPanda ({len(LIVE_TESTS)} sajter)")

        for test in LIVE_TESTS:
            r = run_lightpanda_test(lp, test["url"], "")
            lp_live[test["id"]] = r
            if r["error"]:
                print(f"  [{test['id']:2d}/20] {test['name']:<30s}  ERROR: {r['error']}")
            else:
                print(f"  [{test['id']:2d}/20] {test['name']:<30s}  "
                      f"nodes={r['node_count']:4d}  tokens={r['token_count']:5d}  "
                      f"{r['elapsed_ms']:5.0f}ms")
    else:
        print_section("FAS 4: LightPanda — HOPPAS ÖVER (ej installerad)")

    # ── Samla live-resultat ──────────────────────────────────────────────────
    for test in LIVE_TESTS:
        entry = {"id": test["id"], "name": test["name"], "url": test["url"],
                 "goal": test["goal"], "html_size": len(live_html.get(test["id"], "")),
                 "aether": ae_live.get(test["id"], {})}
        if test["id"] in lp_live:
            entry["lightpanda"] = lp_live[test["id"]]
        results["live"].append(entry)

    # ══════════════════════════════════════════════════════════════════════════
    #  FAS 5: PRESTANDA — 100 sekventiella parseringar
    # ══════════════════════════════════════════════════════════════════════════
    print_header(f"FAS 5: PRESTANDA — Campfire Commerce ({PERF_RUNS} körningar)")

    # AetherAgent
    print("  AetherAgent: Startar MCP-session...", flush=True)
    client = McpClient()
    ae_perf_times = []
    for i in range(PERF_RUNS):
        start = time.monotonic()
        client.call_tool("parse", {"html": CAMPFIRE_HTML, "goal": "buy the backpack",
                                   "url": "https://demo.lightpanda.io/"})
        ae_perf_times.append((time.monotonic() - start) * 1000)
        if (i + 1) % 25 == 0:
            print(f"    AetherAgent: {i+1}/{PERF_RUNS} done ({fmt_ms(ae_perf_times[-1])} last)")
    client.close()

    ae_perf = {
        "total_ms": round(sum(ae_perf_times), 1),
        "avg_ms": round(statistics.mean(ae_perf_times), 1),
        "median_ms": round(statistics.median(ae_perf_times), 1),
        "p99_ms": round(sorted(ae_perf_times)[int(PERF_RUNS * 0.99)], 1),
    }
    results["performance"]["aether"] = ae_perf

    print(f"\n  AetherAgent ({PERF_RUNS} runs):")
    print(f"    Total:   {fmt_ms(ae_perf['total_ms'])}")
    print(f"    Avg:     {fmt_ms(ae_perf['avg_ms'])}")
    print(f"    Median:  {fmt_ms(ae_perf['median_ms'])}")
    print(f"    P99:     {fmt_ms(ae_perf['p99_ms'])}")

    # LightPanda prestanda
    if lp.available:
        # Skriv campfire till fixture-server
        campfire_path = os.path.join(FIXTURE_DIR, "_campfire_bench.html")
        with open(campfire_path, "w") as f:
            f.write(CAMPFIRE_HTML)

        lp_perf_times = []
        campfire_url = f"http://127.0.0.1:{FIXTURE_PORT}/_campfire_bench.html"
        for i in range(PERF_RUNS):
            _, elapsed = lp.fetch_semantic_tree(campfire_url)
            lp_perf_times.append(elapsed)
            if (i + 1) % 25 == 0:
                print(f"    LightPanda: {i+1}/{PERF_RUNS} done ({fmt_ms(lp_perf_times[-1])} last)")

        lp_perf = {
            "total_ms": round(sum(lp_perf_times), 1),
            "avg_ms": round(statistics.mean(lp_perf_times), 1),
            "median_ms": round(statistics.median(lp_perf_times), 1),
            "p99_ms": round(sorted(lp_perf_times)[int(PERF_RUNS * 0.99)], 1),
        }
        results["performance"]["lightpanda"] = lp_perf

        print(f"\n  LightPanda ({PERF_RUNS} runs):")
        print(f"    Total:   {fmt_ms(lp_perf['total_ms'])}")
        print(f"    Avg:     {fmt_ms(lp_perf['avg_ms'])}")
        print(f"    Median:  {fmt_ms(lp_perf['median_ms'])}")
        print(f"    P99:     {fmt_ms(lp_perf['p99_ms'])}")

        speedup = lp_perf["total_ms"] / max(1, ae_perf["total_ms"])
        print(f"\n  Speedup: AetherAgent är {speedup:.1f}x snabbare")

        # Rensa temp-fil
        try:
            os.remove(campfire_path)
        except OSError:
            pass

    # ══════════════════════════════════════════════════════════════════════════
    #  FAS 6: SAMMANFATTNING
    # ══════════════════════════════════════════════════════════════════════════
    print_header("SAMMANFATTNING — TOTAL ÄRLIGHET")

    # Lokala resultat
    ae_local_found = sum(1 for r in ae_local.values() if r["found"])
    ae_local_latencies = [r["elapsed_ms"] for r in ae_local.values()]
    ae_local_tokens = [r["token_count"] for r in ae_local.values()]

    print(f"  LOKALA TESTER ({len(LOCAL_TESTS)} st):")
    print(f"  {'Motor':<20s}  {'Hittar mål':>12s}  {'Avg latens':>12s}  {'Avg tokens':>12s}")
    print(f"  {'-'*60}")
    print(f"  {'AetherAgent':<20s}  {ae_local_found:>5d}/50     "
          f"  {statistics.mean(ae_local_latencies):>8.0f}ms"
          f"  {statistics.mean(ae_local_tokens):>10.0f}")

    if lp_local:
        lp_local_found = sum(1 for r in lp_local.values() if r["found"])
        lp_local_latencies = [r["elapsed_ms"] for r in lp_local.values()]
        lp_local_tokens = [r["token_count"] for r in lp_local.values()]
        print(f"  {'LightPanda':<20s}  {lp_local_found:>5d}/50     "
              f"  {statistics.mean(lp_local_latencies):>8.0f}ms"
              f"  {statistics.mean(lp_local_tokens):>10.0f}")
    print()

    # Per kategori
    print(f"  RESULTAT PER KATEGORI:")
    cats = {}
    for test in LOCAL_TESTS:
        c = test["cat"]
        if c not in cats:
            cats[c] = {"total": 0, "ae_found": 0, "lp_found": 0}
        cats[c]["total"] += 1
        if ae_local.get(test["id"], {}).get("found"):
            cats[c]["ae_found"] += 1
        if lp_local.get(test["id"], {}).get("found"):
            cats[c]["lp_found"] += 1

    header = f"  {'Kategori':<15s}  {'AE hittar':>10s}"
    if lp_local:
        header += f"  {'LP hittar':>10s}"
    print(header)
    print(f"  {'-'*40}")
    for c in sorted(cats):
        d = cats[c]
        line = f"  {c:<15s}  {d['ae_found']:>3d}/{d['total']:<5d}"
        if lp_local:
            line += f"  {d['lp_found']:>3d}/{d['total']:<5d}"
        print(line)
    print()

    # Failures — total ärlighet
    print(f"  FAILURES (alla missade tester dokumenterade):")
    ae_misses = [t for t in LOCAL_TESTS if not ae_local.get(t["id"], {}).get("found")]
    if ae_misses:
        print(f"  AetherAgent missade ({len(ae_misses)} st):")
        for t in ae_misses:
            r = ae_local.get(t["id"], {})
            top_label = r.get("top5", [{}])[0].get("label", "—")[:40] if r.get("top5") else "—"
            print(f"    [{t['id']:2d}] {t['name']} — sökte '{t['expect']}', "
                  f"fick '{top_label}'")
    else:
        print(f"  AetherAgent: ALLA 50 HITTADE!")

    if lp_local:
        lp_misses = [t for t in LOCAL_TESTS if not lp_local.get(t["id"], {}).get("found")]
        if lp_misses:
            print(f"  LightPanda missade ({len(lp_misses)} st):")
            for t in lp_misses:
                print(f"    [{t['id']:2d}] {t['name']} — sökte '{t['expect']}'")
        else:
            print(f"  LightPanda: ALLA 50 HITTADE!")
    print()

    # Sammanfattning i results
    summary = {
        "aether": {
            "local_found": ae_local_found,
            "local_total": len(LOCAL_TESTS),
            "avg_latency_ms": round(statistics.mean(ae_local_latencies), 1),
            "avg_tokens": round(statistics.mean(ae_local_tokens)),
        }
    }
    if lp_local:
        summary["lightpanda"] = {
            "local_found": lp_local_found,
            "local_total": len(LOCAL_TESTS),
            "avg_latency_ms": round(statistics.mean(lp_local_latencies), 1),
            "avg_tokens": round(statistics.mean(lp_local_tokens)),
        }
    results["summary"] = summary

    # ── Spara JSON ───────────────────────────────────────────────────────────
    output_path = os.path.join(os.path.dirname(__file__), "embedding_benchmark_results.json")
    with open(output_path, "w") as f:
        json.dump(results, f, indent=2, ensure_ascii=False)
    print(f"  Resultat sparat: {output_path}")

    # ── Cleanup ──────────────────────────────────────────────────────────────
    if fixture_server:
        fixture_server.shutdown()

    print()
    print("  Benchmark klar.")
    print()


if __name__ == "__main__":
    main()
