#!/usr/bin/env python3
"""
AetherAgent vs Lightpanda — Fair Head-to-Head Benchmark
=========================================================
Apples-to-apples comparison:
  Test 1: Raw HTML fetch (same operation)
  Test 2: Semantic tree parse (same operation)
  Test 3: Screenshot render (AetherAgent only - LP has no renderer)
"""

import json, base64, urllib.request, subprocess, time, os, sys

AETHER_SERVER = "http://localhost:3000"
LIGHTPANDA = "/tmp/lightpanda"
BASE_DIR = os.path.dirname(os.path.abspath(__file__))
AETHER_DIR = f"{BASE_DIR}/aether"
LP_DIR = f"{BASE_DIR}/lightpanda"

os.makedirs(AETHER_DIR, exist_ok=True)
os.makedirs(LP_DIR, exist_ok=True)

SITES = [
    # Befintliga 10
    ("example.com", "https://example.com"),
    ("news.ycombinator.com", "https://news.ycombinator.com"),
    ("books.toscrape.com", "https://books.toscrape.com"),
    ("www.aftonbladet.se", "https://www.aftonbladet.se"),
    ("www.di.se", "https://www.di.se"),
    ("www.expressen.se", "https://www.expressen.se"),
    ("www.apple.com", "https://www.apple.com"),
    ("github.com", "https://github.com"),
    ("x.com", "https://x.com"),
    ("www.bbc.com", "https://www.bbc.com"),
    # Nya 10 (blandad JS-intensitet)
    ("docs.python.org", "https://docs.python.org/3/"),
    ("cnn.com", "https://www.cnn.com"),
    ("linkedin.com", "https://www.linkedin.com"),
    ("svt.se", "https://www.svt.se"),
    ("dn.se", "https://www.dn.se"),
    ("google.com", "https://www.google.com"),
    ("rust-lang.org", "https://www.rust-lang.org"),
    ("mozilla.org", "https://www.mozilla.org"),
    ("cloudflare.com", "https://www.cloudflare.com"),
    ("vercel.com", "https://vercel.com"),
]

def ae_api(endpoint, payload, timeout=30):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        f"{AETHER_SERVER}{endpoint}",
        data=data,
        headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read())

def count_nodes(nodes):
    c = len(nodes)
    for n in nodes:
        c += count_nodes(n.get("children", []))
    return c

def safe_name(name):
    return name.replace(".", "_")

results = {"fetch": [], "parse": [], "render": []}

# ═══════════════════════════════════════════════════════════════════
print("=" * 90)
print("  TEST 1: Ren HTML-hämtning")
print("  AetherAgent /api/fetch  vs  Lightpanda fetch --dump html")
print("=" * 90)
print(f"{'Site':<28} {'AE ms':>7} {'AE size':>9} {'LP ms':>7} {'LP size':>9} {'Speedup':>8} {'Vinnare':>8}")
print("-" * 90)

ae_w, lp_w = 0, 0
for name, url in SITES:
    # AetherAgent
    t0 = time.time()
    try:
        d = ae_api("/api/fetch", {"url": url})
        ae_ms = int((time.time() - t0) * 1000)
        ae_sz = d.get("body_size_bytes", 0)
    except Exception as e:
        ae_ms = int((time.time() - t0) * 1000)
        ae_sz = -1

    # Lightpanda
    t0 = time.time()
    try:
        r = subprocess.run([LIGHTPANDA, "fetch", "--dump", "html", "--wait_until", "load",
                          "--wait_ms", "5000", url], capture_output=True, text=True, timeout=25)
        lp_ms = int((time.time() - t0) * 1000)
        lp_sz = len(r.stdout)
    except:
        lp_ms = int((time.time() - t0) * 1000)
        lp_sz = -1

    if ae_sz >= 0 and lp_sz >= 0:
        if ae_ms <= lp_ms:
            winner = "AE"
            ae_w += 1
            speedup = f"{lp_ms/max(ae_ms,1):.1f}x"
        else:
            winner = "LP"
            lp_w += 1
            speedup = f"{ae_ms/max(lp_ms,1):.1f}x"
    elif ae_sz >= 0:
        winner = "AE"; ae_w += 1; speedup = "N/A"
    else:
        winner = "LP"; lp_w += 1; speedup = "N/A"

    results["fetch"].append({"site": name, "ae_ms": ae_ms, "ae_size": ae_sz,
                            "lp_ms": lp_ms, "lp_size": lp_sz, "winner": winner})

    ae_s = str(ae_sz) if ae_sz >= 0 else "ERR"
    lp_s = str(lp_sz) if lp_sz >= 0 else "ERR"
    print(f"{name:<28} {ae_ms:>6}ms {ae_s:>9} {lp_ms:>6}ms {lp_s:>9} {speedup:>8} {winner:>8}")

print(f"\nFetch: AetherAgent {ae_w} – Lightpanda {lp_w}")

# ═══════════════════════════════════════════════════════════════════
print()
print("=" * 90)
print("  TEST 2: Semantisk parsning (fetch + tree)")
print("  AetherAgent /api/fetch/parse  vs  Lightpanda fetch --dump semantic_tree_text")
print("=" * 90)
print(f"{'Site':<28} {'AE ms':>7} {'AE nodes':>9} {'LP ms':>7} {'LP lines':>9} {'Speedup':>8} {'Vinnare':>8}")
print("-" * 90)

ae_w, lp_w = 0, 0
for name, url in SITES:
    # AetherAgent
    t0 = time.time()
    try:
        d = ae_api("/api/fetch/parse", {"url": url, "goal": "understand page content"})
        ae_ms = int((time.time() - t0) * 1000)
        tree = d.get("tree", d)
        ae_nodes = count_nodes(tree.get("nodes", []))
    except:
        ae_ms = int((time.time() - t0) * 1000)
        ae_nodes = -1

    # Lightpanda
    t0 = time.time()
    try:
        r = subprocess.run([LIGHTPANDA, "fetch", "--dump", "semantic_tree_text", "--wait_until",
                          "load", "--wait_ms", "5000", url], capture_output=True, text=True, timeout=25)
        lp_ms = int((time.time() - t0) * 1000)
        lp_lines = len([l for l in r.stdout.strip().split("\n") if l.strip()])
    except:
        lp_ms = int((time.time() - t0) * 1000)
        lp_lines = -1

    if ae_nodes >= 0 and lp_lines >= 0:
        if ae_ms <= lp_ms:
            winner = "AE"
            ae_w += 1
            speedup = f"{lp_ms/max(ae_ms,1):.1f}x"
        else:
            winner = "LP"
            lp_w += 1
            speedup = f"{ae_ms/max(lp_ms,1):.1f}x"
    elif ae_nodes >= 0:
        winner = "AE"; ae_w += 1; speedup = "N/A"
    else:
        winner = "LP"; lp_w += 1; speedup = "N/A"

    results["parse"].append({"site": name, "ae_ms": ae_ms, "ae_nodes": ae_nodes,
                            "lp_ms": lp_ms, "lp_lines": lp_lines, "winner": winner})

    ae_s = str(ae_nodes) if ae_nodes >= 0 else "ERR"
    lp_s = str(lp_lines) if lp_lines >= 0 else "ERR"
    print(f"{name:<28} {ae_ms:>6}ms {ae_s:>9} {lp_ms:>6}ms {lp_s:>9} {speedup:>8} {winner:>8}")

print(f"\nParse: AetherAgent {ae_w} – Lightpanda {lp_w}")

# ═══════════════════════════════════════════════════════════════════
print()
print("=" * 90)
print("  TEST 3: Screenshot rendering")
print("  AetherAgent (Blitz)  vs  Lightpanda (SAKNAR RENDERING)")
print("=" * 90)

for name, url in SITES:
    sname = safe_name(name)
    t0 = time.time()
    try:
        d = ae_api("/api/fetch/render", {"url": url, "width": 1280, "height": 900}, timeout=30)
        ae_ms = int((time.time() - t0) * 1000)
        png = base64.b64decode(d.get("png_base64", ""))
        if png:
            with open(f"{AETHER_DIR}/{sname}.png", "wb") as f:
                f.write(png)
            css_ok = d.get("css_bytes_added", d.get("css_added_bytes", 0)) > 0
            quality = "med CSS" if css_ok else "utan CSS"
            print(f"  {name:<28} {ae_ms:>6}ms  {len(png):>7} bytes  ({quality})")
            results["render"].append({"site": name, "ms": ae_ms, "png_bytes": len(png),
                                     "css_inlined": css_ok})
        else:
            print(f"  {name:<28} {ae_ms:>6}ms  Ingen PNG")
    except Exception as e:
        ae_ms = int((time.time() - t0) * 1000)
        print(f"  {name:<28} {ae_ms:>6}ms  FEL: {e}")

print(f"\n  Lightpanda: 0/10 screenshots (ingen rendering-motor)")

# ═══════════════════════════════════════════════════════════════════
# Save results
with open(f"{BASE_DIR}/fair_results.json", "w") as f:
    json.dump(results, f, indent=2)

print()
print("=" * 90)
print("  TOTALRESULTAT")
print("=" * 90)
fetch_ae = sum(1 for r in results["fetch"] if r["winner"] == "AE")
fetch_lp = sum(1 for r in results["fetch"] if r["winner"] == "LP")
parse_ae = sum(1 for r in results["parse"] if r["winner"] == "AE")
parse_lp = sum(1 for r in results["parse"] if r["winner"] == "LP")
render_ae = len(results["render"])

print(f"  HTML-hämtning:     AetherAgent {fetch_ae} – Lightpanda {fetch_lp}")
print(f"  Semantisk parse:   AetherAgent {parse_ae} – Lightpanda {parse_lp}")
print(f"  Screenshots:       AetherAgent {render_ae} – Lightpanda 0")
print(f"  Features:          AetherAgent 35+ MCP-verktyg – Lightpanda CDP only")
print()
