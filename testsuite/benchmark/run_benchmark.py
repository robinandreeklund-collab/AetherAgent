#!/usr/bin/env python3
"""
AetherAgent vs Lightpanda — Head-to-Head Benchmark
====================================================
Compares 10 real websites on:
- Fetch + parse time
- HTML output size
- Semantic tree quality (node count)
- Screenshot capability (AetherAgent only — Lightpanda has no renderer)
- JS execution support
"""

import json, base64, urllib.request, subprocess, time, os, sys

AETHER_SERVER = "http://localhost:3000"
LIGHTPANDA = "/tmp/lightpanda"
BASE_DIR = "/home/user/AetherAgent/testsuite/benchmark"
AETHER_DIR = f"{BASE_DIR}/aether"
LP_DIR = f"{BASE_DIR}/lightpanda"

os.makedirs(AETHER_DIR, exist_ok=True)
os.makedirs(LP_DIR, exist_ok=True)

SITES = [
    ("example.com", "https://example.com"),
    ("news.ycombinator.com", "https://news.ycombinator.com"),
    ("www.aftonbladet.se", "https://www.aftonbladet.se"),
    ("www.di.se", "https://www.di.se"),
    ("www.expressen.se", "https://www.expressen.se"),
    ("www.apple.com", "https://www.apple.com"),
    ("www.bbc.com", "https://www.bbc.com"),
    ("github.com", "https://github.com"),
    ("x.com", "https://x.com"),
    ("books.toscrape.com", "https://books.toscrape.com"),
]

results = []

def aether_api(endpoint, payload, timeout=60):
    data = json.dumps(payload).encode()
    req = urllib.request.Request(
        f"{AETHER_SERVER}{endpoint}",
        data=data,
        headers={"Content-Type": "application/json"}
    )
    with urllib.request.urlopen(req, timeout=timeout) as resp:
        return json.loads(resp.read())

def safe_name(name):
    return name.replace(".", "_")

for name, url in SITES:
    print(f"\n{'='*70}")
    print(f"  {name}")
    print(f"{'='*70}")

    result = {
        "site": name,
        "url": url,
        "aether": {},
        "lightpanda": {},
    }

    sname = safe_name(name)

    # ─── AetherAgent: fetch + CSS inline + full render ────────────────
    print(f"  [AetherAgent] Fetching + rendering...")
    try:
        t0 = time.time()

        # 1. Fetch + render (full with CSS inline + images)
        render_data = aether_api("/api/fetch/render", {
            "url": url, "width": 1280, "height": 900
        })
        aether_total = int((time.time() - t0) * 1000)

        if "error" in render_data:
            print(f"  [AetherAgent] Render error: {render_data['error']}")
            result["aether"]["error"] = render_data["error"]
        else:
            png_b64 = render_data.get("png_base64", "")
            if png_b64:
                png_bytes = base64.b64decode(png_b64)
                png_path = f"{AETHER_DIR}/{sname}.png"
                with open(png_path, "wb") as f:
                    f.write(png_bytes)
                result["aether"]["png_size"] = len(png_bytes)
                result["aether"]["screenshot"] = True
            else:
                result["aether"]["screenshot"] = False

            result["aether"]["html_length"] = render_data.get("html_length", 0)
            result["aether"]["css_inlined"] = render_data.get("css_inlined", False)
            result["aether"]["css_added_bytes"] = render_data.get("css_added_bytes", 0)

        # 2. Semantic tree parse
        t1 = time.time()
        parse_data = aether_api("/api/fetch/parse", {"url": url, "goal": "understand page content"})
        parse_ms = int((time.time() - t1) * 1000)

        if isinstance(parse_data, dict) and "nodes" in parse_data:
            def count_nodes(nodes):
                c = len(nodes)
                for n in nodes:
                    if "children" in n:
                        c += count_nodes(n["children"])
                return c
            node_count = count_nodes(parse_data.get("nodes", []))
            result["aether"]["node_count"] = node_count
            result["aether"]["parse_ms"] = parse_data.get("parse_time_ms", parse_ms)

        result["aether"]["total_ms"] = aether_total
        result["aether"]["has_js"] = True  # Boa JS engine
        result["aether"]["has_renderer"] = True  # Blitz

        print(f"  [AetherAgent] {aether_total}ms total, {result['aether'].get('node_count', '?')} nodes, "
              f"PNG: {result['aether'].get('png_size', 0)} bytes")

    except Exception as e:
        print(f"  [AetherAgent] ERROR: {e}")
        result["aether"]["error"] = str(e)

    # ─── Lightpanda: fetch + dump ─────────────────────────────────────
    print(f"  [Lightpanda] Fetching...")
    try:
        # HTML dump
        t0 = time.time()
        lp_html = subprocess.run(
            [LIGHTPANDA, "fetch", "--dump", "html", "--wait_until", "load", "--wait_ms", "5000", url],
            capture_output=True, text=True, timeout=30
        )
        lp_html_ms = int((time.time() - t0) * 1000)

        html_output = lp_html.stdout
        result["lightpanda"]["html_length"] = len(html_output)
        result["lightpanda"]["html_ms"] = lp_html_ms

        # Save HTML dump
        html_path = f"{LP_DIR}/{sname}.html"
        with open(html_path, "w") as f:
            f.write(html_output[:500000])  # Cap at 500k

        # Semantic tree dump (Lightpanda's built-in)
        t1 = time.time()
        lp_tree = subprocess.run(
            [LIGHTPANDA, "fetch", "--dump", "semantic_tree_text", "--wait_until", "load", "--wait_ms", "5000", url],
            capture_output=True, text=True, timeout=30
        )
        lp_tree_ms = int((time.time() - t1) * 1000)

        tree_output = lp_tree.stdout
        tree_lines = [l for l in tree_output.strip().split("\n") if l.strip()]
        result["lightpanda"]["tree_lines"] = len(tree_lines)
        result["lightpanda"]["tree_ms"] = lp_tree_ms

        # Save tree
        tree_path = f"{LP_DIR}/{sname}_tree.txt"
        with open(tree_path, "w") as f:
            f.write(tree_output[:500000])

        result["lightpanda"]["total_ms"] = lp_html_ms
        result["lightpanda"]["has_js"] = True  # V8 engine
        result["lightpanda"]["has_renderer"] = False  # No rendering
        result["lightpanda"]["screenshot"] = False

        lp_err = lp_html.stderr.strip()
        if lp_err and "error" in lp_err.lower():
            result["lightpanda"]["errors"] = lp_err[:200]

        print(f"  [Lightpanda] {lp_html_ms}ms fetch, {len(html_output)} chars HTML, "
              f"{len(tree_lines)} tree lines")

    except subprocess.TimeoutExpired:
        print(f"  [Lightpanda] TIMEOUT (30s)")
        result["lightpanda"]["error"] = "Timeout 30s"
    except Exception as e:
        print(f"  [Lightpanda] ERROR: {e}")
        result["lightpanda"]["error"] = str(e)

    results.append(result)

# ─── Save results ─────────────────────────────────────────────────────
results_path = f"{BASE_DIR}/results.json"
with open(results_path, "w") as f:
    json.dump(results, f, indent=2)
print(f"\nResults saved to {results_path}")

# ─── Print summary table ─────────────────────────────────────────────
print(f"\n{'='*90}")
print(f"  BENCHMARK RESULTS: AetherAgent vs Lightpanda")
print(f"{'='*90}")
print(f"{'Site':<25} {'AE time':>8} {'LP time':>8} {'AE nodes':>9} {'LP lines':>9} {'AE PNG':>8} {'Winner':>8}")
print(f"{'-'*25} {'-'*8} {'-'*8} {'-'*9} {'-'*9} {'-'*8} {'-'*8}")

ae_wins = 0
lp_wins = 0
ties = 0

for r in results:
    ae = r["aether"]
    lp = r["lightpanda"]

    ae_t = ae.get("total_ms", -1)
    lp_t = lp.get("total_ms", -1)
    ae_n = ae.get("node_count", 0)
    lp_l = lp.get("tree_lines", 0)
    ae_png = ae.get("png_size", 0)

    ae_t_str = f"{ae_t}ms" if ae_t > 0 else "ERR"
    lp_t_str = f"{lp_t}ms" if lp_t > 0 else "ERR"
    ae_n_str = str(ae_n) if ae_n else "ERR"
    lp_l_str = str(lp_l) if lp_l else "ERR"
    ae_png_str = f"{ae_png//1024}KB" if ae_png else "N/A"

    # Speed winner (both must succeed)
    if ae_t > 0 and lp_t > 0:
        if ae_t < lp_t * 0.8:
            winner = "AE"
            ae_wins += 1
        elif lp_t < ae_t * 0.8:
            winner = "LP"
            lp_wins += 1
        else:
            winner = "TIE"
            ties += 1
    elif ae_t > 0:
        winner = "AE"
        ae_wins += 1
    elif lp_t > 0:
        winner = "LP"
        lp_wins += 1
    else:
        winner = "N/A"

    print(f"{r['site']:<25} {ae_t_str:>8} {lp_t_str:>8} {ae_n_str:>9} {lp_l_str:>9} {ae_png_str:>8} {winner:>8}")

print(f"\nSpeed wins: AetherAgent={ae_wins}, Lightpanda={lp_wins}, Ties={ties}")
print(f"Screenshots: AetherAgent=YES (Blitz renderer), Lightpanda=NO (no renderer)")
print(f"JS engine: AetherAgent=Boa (sandboxed), Lightpanda=V8 (full)")
print(f"\nDone!")
