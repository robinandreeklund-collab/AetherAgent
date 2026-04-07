#!/usr/bin/env python3
"""
Sweep BM25_WEIGHT from 0.10 to 0.80 and measure CRFR quality at each level.
Modifies the constant in resonance.rs, rebuilds the server, runs the test.
"""
import subprocess
import re
import time
import json
import sys
import os
import signal
import requests

RESONANCE_RS = "/home/user/AetherAgent/src/resonance.rs"
BASE = "http://localhost:3000"

# BM25 weights to test
WEIGHTS = [0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.75]

# Test sites (same as convergence protocol)
SITES = [
    {
        "name": "ESPN",
        "url": "https://www.espn.com/",
        "goals": [
            "latest sports scores today",
            "todays game results",
            "live sports scores and updates",
            "major sports results today",
            "current game scores",
            "todays match results",
            "sports scores and highlights",
            "what are todays sports results",
            "live game updates and scores",
            "current sports standings and scores",
        ],
        "keywords": ["score", "game", "win", "loss", "team", "match", "nba", "nfl", "mlb"],
    },
    {
        "name": "USA.gov",
        "url": "https://www.usa.gov/",
        "goals": [
            "government benefits and services",
            "how to apply for government benefits",
            "federal services for citizens",
            "government assistance programs",
            "public benefits information",
            "citizen services overview",
            "federal government help",
            "what government services are available",
            "how to get government assistance",
            "public services and benefits guide",
        ],
        "keywords": ["benefit", "service", "government", "federal", "apply", "assistance"],
    },
    {
        "name": "NPR",
        "url": "https://www.npr.org/",
        "goals": [
            "latest news stories today",
            "breaking news headlines now",
            "top articles published today",
            "important current events",
            "major stories happening right now",
            "key news developments today",
            "most notable news stories",
            "what are todays biggest news stories",
            "current affairs and global events",
            "recent notable world happenings",
        ],
        "keywords": ["news", "article", "story", "report", "headline"],
    },
]

NAV_SIGNALS = ["cookie", "privacy", "sign in", "log in", "subscribe", "skip to",
               "menu", "footer", "copyright", "terms of use"]

def is_relevant(label, keywords):
    lower = label.lower()
    if len(lower) < 5:
        return False
    for nav in NAV_SIGNALS:
        if nav in lower and len(lower) < 100:
            return False
    return any(k.lower() in lower for k in keywords)


def set_bm25_weight(weight):
    """Modify BM25_WEIGHT constant in resonance.rs"""
    with open(RESONANCE_RS, 'r') as f:
        content = f.read()
    # Replace the constant
    content = re.sub(
        r'const BM25_WEIGHT: f32 = [\d.]+;',
        f'const BM25_WEIGHT: f32 = {weight:.2f};',
        content
    )
    with open(RESONANCE_RS, 'w') as f:
        f.write(content)


def build_server():
    """Build the server binary"""
    result = subprocess.run(
        ["cargo", "build", "--bin", "aether-server", "--features", "server"],
        capture_output=True, text=True, timeout=600
    )
    if result.returncode != 0:
        print(f"  BUILD FAILED: {result.stderr[-200:]}")
        return False
    return True


def start_server():
    """Start server in background, return process"""
    proc = subprocess.Popen(
        ["cargo", "run", "--bin", "aether-server", "--features", "server"],
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
        preexec_fn=os.setsid
    )
    # Wait for server to be ready
    for _ in range(30):
        time.sleep(1)
        try:
            r = requests.get(f"{BASE}/health", timeout=2)
            if r.status_code == 200:
                return proc
        except:
            pass
    print("  SERVER FAILED TO START")
    return proc


def stop_server(proc):
    """Stop server process"""
    try:
        os.killpg(os.getpgid(proc.pid), signal.SIGTERM)
        proc.wait(timeout=5)
    except:
        try:
            os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
        except:
            pass


def fetch_html(url):
    """Fetch HTML via server"""
    try:
        r = requests.post(f"{BASE}/api/fetch", json={"url": url}, timeout=30)
        return r.json().get("body", "")
    except:
        return ""


def run_site_test(site, html):
    """Run 10-iteration protocol, return metrics"""
    url = site["url"]
    keywords = site["keywords"]
    goals = site["goals"]

    results = {
        "baseline_rel": 0,
        "baseline_total": 0,
        "train_rel": 0,
        "train_total": 0,
        "train_causal": 0,
        "test_rel": 0,
        "test_total": 0,
        "test_causal": 0,
        "test_max_cb": 0.0,
        "iterations": [],
    }

    for i, goal in enumerate(goals):
        phase = "BL" if i == 0 else ("TR" if i <= 6 else "TE")

        try:
            r = requests.post(f"{BASE}/api/parse-crfr", json={
                "html": html, "url": url, "goal": goal, "top_n": 10, "run_js": False,
            }, timeout=30)
            data = r.json()
        except:
            results["iterations"].append({"phase": phase, "rel": 0, "total": 0, "causal": 0})
            continue

        nodes = data.get("nodes", [])
        rel = sum(1 for n in nodes if is_relevant(n.get("label", ""), keywords))
        total = len(nodes)
        causal = sum(1 for n in nodes if n.get("causal_boost", 0) > 0.001)
        max_cb = max((n.get("causal_boost", 0) for n in nodes), default=0)

        results["iterations"].append({
            "phase": phase, "rel": rel, "total": total, "causal": causal, "max_cb": max_cb
        })

        if phase == "BL":
            results["baseline_rel"] = rel
            results["baseline_total"] = total
        elif phase == "TR":
            results["train_rel"] += rel
            results["train_total"] += total
            results["train_causal"] += causal
        else:
            results["test_rel"] += rel
            results["test_total"] += total
            results["test_causal"] += causal
            results["test_max_cb"] = max(results["test_max_cb"], max_cb)

        # Feedback in TRAIN phase
        if phase == "TR":
            fb_ids = [n["id"] for n in nodes if is_relevant(n.get("label", ""), keywords)][:5]
            if fb_ids:
                try:
                    requests.post(f"{BASE}/api/crfr-feedback", json={
                        "url": url, "goal": goal, "successful_node_ids": fb_ids,
                    }, timeout=10)
                except:
                    pass

    return results


def main():
    all_results = {}

    # Pre-fetch HTML for all sites (do this once before server restarts)
    print("=== Pre-fetching HTML ===")

    # First, set original weight and build to fetch
    set_bm25_weight(0.75)
    if not build_server():
        sys.exit(1)

    server = start_server()
    html_cache = {}
    for site in SITES:
        html = fetch_html(site["url"])
        html_cache[site["url"]] = html
        print(f"  {site['name']}: {len(html)} chars")
    stop_server(server)
    time.sleep(2)

    # Now sweep BM25 weights
    for weight in WEIGHTS:
        print(f"\n{'='*60}")
        print(f"  BM25_WEIGHT = {weight:.2f}")
        print(f"{'='*60}")

        set_bm25_weight(weight)
        if not build_server():
            continue

        server = start_server()

        weight_results = {}
        for site in SITES:
            html = html_cache.get(site["url"], "")
            if len(html) < 200:
                print(f"  {site['name']}: SKIP (no HTML)")
                continue

            result = run_site_test(site, html)
            weight_results[site["name"]] = result

            bl_r = result["baseline_rel"]
            bl_t = result["baseline_total"]
            te_r = result["test_rel"]
            te_t = result["test_total"]
            te_c = result["test_causal"]
            te_cb = result["test_max_cb"]
            print(f"  {site['name']:10s}: BL={bl_r}/{bl_t}  TEST={te_r}/{te_t}  causal={te_c}  max_cb={te_cb:.4f}")

        all_results[f"{weight:.2f}"] = weight_results
        stop_server(server)
        time.sleep(2)

    # Restore original weight
    set_bm25_weight(0.75)

    # Summary table
    print(f"\n{'='*80}")
    print(f"  SUMMARY: BM25 Weight Sweep")
    print(f"{'='*80}")
    print(f"{'Weight':>8} | {'ESPN BL':>8} {'ESPN TE':>8} {'ESPN CB':>8} | {'USA BL':>8} {'USA TE':>8} {'USA CB':>8} | {'NPR BL':>8} {'NPR TE':>8} {'NPR CB':>8}")
    print("-" * 100)
    for w in WEIGHTS:
        key = f"{w:.2f}"
        wr = all_results.get(key, {})
        cols = []
        for site_name in ["ESPN", "USA.gov", "NPR"]:
            r = wr.get(site_name, {})
            bl = f"{r.get('baseline_rel',0)}/{r.get('baseline_total',0)}"
            te = f"{r.get('test_rel',0)}/{r.get('test_total',0)}"
            cb = f"{r.get('test_max_cb',0):.3f}"
            cols.extend([bl, te, cb])
        print(f"  {w:.2f}   | {cols[0]:>8} {cols[1]:>8} {cols[2]:>8} | {cols[3]:>8} {cols[4]:>8} {cols[5]:>8} | {cols[6]:>8} {cols[7]:>8} {cols[8]:>8}")

    # Save JSON
    with open("docs/bm25-weight-sweep.json", "w") as f:
        json.dump(all_results, f, indent=2)
    print(f"\nResults saved to docs/bm25-weight-sweep.json")


if __name__ == "__main__":
    main()
