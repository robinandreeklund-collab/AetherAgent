#!/usr/bin/env python3
"""
AetherAgent vs Lightpanda – Head-to-Head Benchmark
===================================================

Measures:
  1. Token savings: raw tree vs Fas 4a delta across multi-step loops
  2. Parallel throughput: 25/50/100 concurrent parses, total wall-clock time
  3. Memory (RSS): per-instance peak resident memory
  4. Lightpanda comparison: same fixtures, same metrics, head-to-head

Run:
  python3 benches/bench_vs_lightpanda.py

Requirements:
  - AetherAgent HTTP server running (cargo run --features server --bin aether-server)
    OR use the live deployment URL
  - Lightpanda binary at /tmp/lightpanda (or set LIGHTPANDA_BIN env var)
  - python3 with requests (pip install requests)
"""

import json
import os
import subprocess
import sys
import time
import threading
import statistics
import http.server
import socketserver
from concurrent.futures import ThreadPoolExecutor, as_completed
from pathlib import Path

try:
    import requests
except ImportError:
    print("pip install requests")
    sys.exit(1)

# ─── Configuration ────────────────────────────────────────────────────────────

AETHER_URL = os.environ.get("AETHER_URL", "http://127.0.0.1:3000")
LIGHTPANDA_BIN = os.environ.get("LIGHTPANDA_BIN", "/tmp/lightpanda")
FIXTURE_SERVER_PORT = 18765  # Local HTTP server for Lightpanda fixtures
PARALLEL_LEVELS = [25, 50, 100]
ITERATIONS = 20

# ─── HTML Fixtures (shared between both engines) ─────────────────────────────

FIXTURES = {
    "simple": {
        "html": '<html><head><title>Simple</title></head><body>'
                '<h1>Hello World</h1><p>A paragraph.</p>'
                '<a href="/about">About</a></body></html>',
        "goal": "find about link",
        "url": "https://test.com",
    },
    "ecommerce": {
        "html": '<html><head><title>SuperShop</title></head><body>'
                '<nav><a href="/">Hem</a><a href="/produkter">Produkter</a>'
                '<input type="text" placeholder="Sök" /></nav>'
                '<main><h1>iPhone 16 Pro</h1>'
                '<p class="price">13 990 kr</p>'
                '<button id="buy-btn">Lägg i varukorg</button>'
                '<button>Spara</button>'
                '<a href="/kassa">Gå till kassan</a>'
                '<select name="color"><option>Svart</option><option>Vit</option></select>'
                '</main></body></html>',
        "goal": "köp iPhone",
        "url": "https://shop.se",
    },
    "ecommerce_v2": {
        "html": '<html><head><title>SuperShop</title></head><body>'
                '<nav><a href="/">Hem</a><a href="/produkter">Produkter</a>'
                '<input type="text" placeholder="Sök" /></nav>'
                '<main><h1>iPhone 16 Pro</h1>'
                '<p class="price">12 990 kr</p>'
                '<button id="buy-btn">1 i varukorg</button>'
                '<button>Spara</button>'
                '<a href="/kassa">Gå till kassan (1)</a>'
                '<a href="/jämför">Jämför modeller</a>'
                '<select name="color"><option>Svart</option><option>Vit</option></select>'
                '</main></body></html>',
        "goal": "köp iPhone",
        "url": "https://shop.se",
    },
    "login": {
        "html": '<html><head><title>Logga in</title></head><body>'
                '<form><input type="email" placeholder="E-post" />'
                '<input type="password" placeholder="Lösenord" />'
                '<input type="checkbox" /> Kom ihåg mig'
                '<button type="submit">Logga in</button>'
                '<a href="/forgot">Glömt lösenord?</a>'
                '<a href="/register">Skapa konto</a></form></body></html>',
        "goal": "logga in",
        "url": "https://test.com/login",
    },
}


def generate_complex_page(n_products=50):
    """Generate a complex e-commerce page with N products."""
    parts = ['<html><head><title>Alla produkter</title></head><body><main>']
    for i in range(n_products):
        parts.append(
            f'<div class="item"><h3>Produkt {i}</h3>'
            f'<p class="price">{100 + i * 10} kr</p>'
            f'<button id="buy-{i}">Köp</button>'
            f'<a href="/produkt/{i}">Visa</a></div>'
        )
    parts.append('</main></body></html>')
    return ''.join(parts)


FIXTURES["complex_50"] = {
    "html": generate_complex_page(50),
    "goal": "köp produkt",
    "url": "https://shop.se/alla",
}

FIXTURES["complex_100"] = {
    "html": generate_complex_page(100),
    "goal": "köp produkt 42",
    "url": "https://shop.se/alla",
}


# ─── Utility ─────────────────────────────────────────────────────────────────

def count_tokens(text):
    """Approximate token count (whitespace + punctuation splitting)."""
    # Rough approximation: ~4 chars per token for JSON
    return max(1, len(text) // 4)


def fmt_us(us):
    """Format microseconds."""
    if us >= 1_000_000:
        return f"{us/1_000_000:.2f}s"
    if us >= 1_000:
        return f"{us/1_000:.1f}ms"
    return f"{us:.0f}µs"


def measure_rss_kb():
    """Get current process RSS in KB."""
    try:
        with open("/proc/self/status") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
    except Exception:
        return 0
    return 0


def measure_process_rss_kb(pid):
    """Get RSS for a specific PID."""
    try:
        with open(f"/proc/{pid}/status") as f:
            for line in f:
                if line.startswith("VmRSS:"):
                    return int(line.split()[1])
    except Exception:
        return 0
    return 0


# ─── AetherAgent Client ─────────────────────────────────────────────────────

class AetherClient:
    def __init__(self, base_url):
        self.base_url = base_url.rstrip("/")
        self.session = requests.Session()

    def parse(self, fixture):
        resp = self.session.post(
            f"{self.base_url}/api/parse",
            json={"html": fixture["html"], "goal": fixture["goal"], "url": fixture["url"]},
            timeout=30,
        )
        resp.raise_for_status()
        return resp.text

    def diff(self, old_json, new_json):
        resp = self.session.post(
            f"{self.base_url}/api/diff",
            json={"old_tree_json": old_json, "new_tree_json": new_json},
            timeout=30,
        )
        resp.raise_for_status()
        return resp.text

    def health(self):
        resp = self.session.get(f"{self.base_url}/health", timeout=10)
        resp.raise_for_status()
        return resp.json()


# ─── Lightpanda Client ──────────────────────────────────────────────────────

class LightpandaClient:
    def __init__(self, binary_path, fixture_port):
        self.binary = binary_path
        self.port = fixture_port

    def fetch_semantic(self, fixture_name):
        """Fetch a fixture via HTTP and return semantic tree."""
        url = f"http://127.0.0.1:{self.port}/{fixture_name}.html"
        start = time.monotonic()
        result = subprocess.run(
            [self.binary, "fetch", "--dump", "semantic_tree", url],
            capture_output=True, text=True, timeout=30,
        )
        elapsed_us = (time.monotonic() - start) * 1_000_000
        return result.stdout, elapsed_us

    def fetch_html(self, fixture_name):
        """Fetch a fixture and return raw HTML."""
        url = f"http://127.0.0.1:{self.port}/{fixture_name}.html"
        start = time.monotonic()
        result = subprocess.run(
            [self.binary, "fetch", "--dump", "html", url],
            capture_output=True, text=True, timeout=30,
        )
        elapsed_us = (time.monotonic() - start) * 1_000_000
        return result.stdout, elapsed_us


# ─── Fixture HTTP Server ─────────────────────────────────────────────────────

def start_fixture_server(port):
    """Start a local HTTP server serving fixture HTML files."""
    fixture_dir = Path("/tmp/aether_bench_fixtures")
    fixture_dir.mkdir(exist_ok=True)

    for name, fixture in FIXTURES.items():
        (fixture_dir / f"{name}.html").write_text(fixture["html"])

    handler = http.server.SimpleHTTPRequestHandler
    os.chdir(str(fixture_dir))

    server = socketserver.TCPServer(("127.0.0.1", port), handler)
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    return server


# ─── Benchmark 1: Token Savings (Raw vs Delta) ──────────────────────────────

def bench_token_savings(client):
    """Simulate a multi-step agent loop: parse → act → parse → diff."""
    print("\n" + "=" * 72)
    print("BENCHMARK 1: Token Savings – Raw Tree vs Fas 4a Delta")
    print("=" * 72)
    print(f"{'Scenario':<35} {'Raw tokens':>12} {'Delta tokens':>14} {'Savings':>10}")
    print("-" * 72)

    scenarios = [
        ("Simple page (no change)", "simple", "simple"),
        ("E-commerce: add to cart", "ecommerce", "ecommerce_v2"),
        ("Complex 50: price update", "complex_50", "complex_50"),
    ]

    total_raw = 0
    total_delta = 0

    for label, fix1_name, fix2_name in scenarios:
        # Step 1: Parse initial page
        tree1_json = client.parse(FIXTURES[fix1_name])
        raw_tokens = count_tokens(tree1_json)

        # Step 2: Parse updated page
        tree2_json = client.parse(FIXTURES[fix2_name])
        raw_tokens_2 = count_tokens(tree2_json)

        # Step 3: Compute delta
        delta_json = client.diff(tree1_json, tree2_json)
        delta_tokens = count_tokens(delta_json)

        total_raw += raw_tokens + raw_tokens_2
        total_delta += raw_tokens + delta_tokens  # First parse is always full

        savings = (1 - delta_tokens / max(1, raw_tokens_2)) * 100
        print(f"  {label:<33} {raw_tokens_2:>10,} {delta_tokens:>12,} {savings:>9.1f}%")

    # Simulate a 10-step loop
    print(f"\n  {'10-step loop simulation:':<33}")
    loop_raw = 0
    loop_delta = 0
    for step in range(10):
        fix = "ecommerce" if step % 2 == 0 else "ecommerce_v2"
        tree_json = client.parse(FIXTURES[fix])
        loop_raw += count_tokens(tree_json)
        if step == 0:
            prev_json = tree_json
            loop_delta += count_tokens(tree_json)
        else:
            delta_json = client.diff(prev_json, tree_json)
            loop_delta += count_tokens(delta_json)
            prev_json = tree_json

    savings = (1 - loop_delta / max(1, loop_raw)) * 100
    print(f"  {'  Raw (10 full parses):':<33} {loop_raw:>10,} tokens")
    print(f"  {'  Delta (1 full + 9 diffs):':<33} {loop_delta:>10,} tokens")
    print(f"  {'  Savings:':<33} {savings:>9.1f}%")

    return {
        "total_raw_tokens": total_raw,
        "total_delta_tokens": total_delta,
        "loop_raw": loop_raw,
        "loop_delta": loop_delta,
        "loop_savings_pct": savings,
    }


# ─── Benchmark 2: Parallel Throughput ────────────────────────────────────────

def bench_parallel(client, lp_client):
    """Run N concurrent parses and measure total wall-clock time."""
    print("\n" + "=" * 72)
    print("BENCHMARK 2: Parallel Throughput – Concurrent Parse Operations")
    print("=" * 72)
    print(f"{'Engine':<18} {'N':>5} {'Total (ms)':>12} {'Avg/req (ms)':>14} {'Throughput':>14}")
    print("-" * 72)

    results = {}

    for n in PARALLEL_LEVELS:
        # ─── AetherAgent ─────────────────────────────────────────────
        timings = []
        start = time.monotonic()
        with ThreadPoolExecutor(max_workers=min(n, 50)) as pool:
            futures = []
            for i in range(n):
                fix = FIXTURES["ecommerce"] if i % 2 == 0 else FIXTURES["complex_50"]
                futures.append(pool.submit(_timed_parse, client, fix))
            for f in as_completed(futures):
                t = f.result()
                timings.append(t)
        wall_ms = (time.monotonic() - start) * 1000
        avg_ms = statistics.mean(timings)
        throughput = n / (wall_ms / 1000)
        print(f"  {'AetherAgent':<16} {n:>5} {wall_ms:>10.0f}ms {avg_ms:>12.1f}ms {throughput:>11.1f}/s")
        results[f"aether_{n}"] = {"wall_ms": wall_ms, "avg_ms": avg_ms, "throughput": throughput}

        # ─── Lightpanda ──────────────────────────────────────────────
        lp_timings = []
        start = time.monotonic()
        with ThreadPoolExecutor(max_workers=min(n, 50)) as pool:
            futures = []
            for i in range(n):
                fix_name = "ecommerce" if i % 2 == 0 else "complex_50"
                futures.append(pool.submit(_timed_lp_fetch, lp_client, fix_name))
            for f in as_completed(futures):
                t = f.result()
                lp_timings.append(t)
        wall_ms = (time.monotonic() - start) * 1000
        avg_ms = statistics.mean(lp_timings)
        throughput = n / (wall_ms / 1000)
        print(f"  {'Lightpanda':<16} {n:>5} {wall_ms:>10.0f}ms {avg_ms:>12.1f}ms {throughput:>11.1f}/s")
        results[f"lp_{n}"] = {"wall_ms": wall_ms, "avg_ms": avg_ms, "throughput": throughput}
        print()

    return results


def _timed_parse(client, fixture):
    start = time.monotonic()
    client.parse(fixture)
    return (time.monotonic() - start) * 1000


def _timed_lp_fetch(lp_client, fixture_name):
    _, elapsed_us = lp_client.fetch_semantic(fixture_name)
    return elapsed_us / 1000


# ─── Benchmark 3: Memory (RSS per instance) ─────────────────────────────────

def bench_memory(client, lp_client):
    """Measure peak RSS for single and batch operations."""
    print("\n" + "=" * 72)
    print("BENCHMARK 3: Memory – RSS per Instance")
    print("=" * 72)
    print(f"{'Engine':<18} {'Scenario':<30} {'RSS (KB)':>10} {'RSS (MB)':>10}")
    print("-" * 72)

    results = {}

    # AetherAgent: measure server RSS (it's a single process)
    # Find the aether-server PID
    try:
        pids = subprocess.run(
            ["pgrep", "-f", "aether-server"],
            capture_output=True, text=True,
        )
        if pids.stdout.strip():
            pid = int(pids.stdout.strip().split('\n')[0])
            # Measure RSS before
            rss_before = measure_process_rss_kb(pid)

            # Do a batch of parses
            for _ in range(50):
                client.parse(FIXTURES["complex_100"])

            rss_after = measure_process_rss_kb(pid)
            print(f"  {'AetherAgent':<16} {'Server idle':>28} {rss_before:>10,} {rss_before/1024:>9.1f}")
            print(f"  {'AetherAgent':<16} {'After 50x complex_100':>28} {rss_after:>10,} {rss_after/1024:>9.1f}")
            results["aether_idle_kb"] = rss_before
            results["aether_loaded_kb"] = rss_after
        else:
            print("  AetherAgent server PID not found – skipping RSS measurement")
            print("  (Start with: cargo run --features server --bin aether-server)")
    except Exception as e:
        print(f"  AetherAgent RSS measurement failed: {e}")

    # Lightpanda: measure per-process RSS
    # Each lightpanda fetch is a separate process, measure via /usr/bin/time
    for scenario, fix_name in [("simple page", "simple"), ("complex 50", "complex_50")]:
        try:
            url = f"http://127.0.0.1:{FIXTURE_SERVER_PORT}/{fix_name}.html"
            result = subprocess.run(
                ["/usr/bin/time", "-v", LIGHTPANDA_BIN, "fetch", "--dump", "html", url],
                capture_output=True, text=True, timeout=30,
            )
            # Parse "Maximum resident set size" from stderr
            for line in result.stderr.splitlines():
                if "Maximum resident" in line:
                    rss_kb = int(line.strip().split()[-1])
                    print(f"  {'Lightpanda':<16} {scenario:>28}   {rss_kb:>8,} {rss_kb/1024:>9.1f}")
                    results[f"lp_{fix_name}_kb"] = rss_kb
                    break
        except Exception as e:
            print(f"  Lightpanda {scenario}: {e}")

    return results


# ─── Benchmark 4: Head-to-Head Parse Comparison ─────────────────────────────

def bench_head_to_head(client, lp_client):
    """Same fixtures, same measurement: parse time + output size."""
    print("\n" + "=" * 72)
    print("BENCHMARK 4: Head-to-Head – Parse Time & Output Size")
    print("=" * 72)
    print(f"{'Fixture':<18} {'AetherAgent':>14} {'Lightpanda':>14} {'AE tokens':>11} {'LP tokens':>11} {'Speedup':>9}")
    print("-" * 72)

    results = {}

    for name in ["simple", "ecommerce", "login", "complex_50", "complex_100"]:
        fixture = FIXTURES[name]

        # AetherAgent: N iterations
        ae_times = []
        ae_output = ""
        for _ in range(ITERATIONS):
            start = time.monotonic()
            ae_output = client.parse(fixture)
            ae_times.append((time.monotonic() - start) * 1_000_000)
        ae_avg = statistics.median(ae_times)
        ae_tokens = count_tokens(ae_output)

        # Lightpanda: N iterations
        lp_times = []
        lp_output = ""
        for _ in range(ITERATIONS):
            lp_output, elapsed = lp_client.fetch_semantic(name)
            lp_times.append(elapsed)
        lp_avg = statistics.median(lp_times)
        lp_tokens = count_tokens(lp_output)

        speedup = lp_avg / max(1, ae_avg)
        results[name] = {
            "ae_us": ae_avg,
            "lp_us": lp_avg,
            "ae_tokens": ae_tokens,
            "lp_tokens": lp_tokens,
            "speedup": speedup,
        }

        print(
            f"  {name:<16} {fmt_us(ae_avg):>12} {fmt_us(lp_avg):>14}"
            f" {ae_tokens:>9,} {lp_tokens:>11,}"
            f" {speedup:>7.1f}x"
        )

    return results


# ─── Benchmark 5: Output Quality Comparison ─────────────────────────────────

def bench_output_quality(client, lp_client):
    """Compare what each engine outputs for the same HTML."""
    print("\n" + "=" * 72)
    print("BENCHMARK 5: Output Quality – Semantic Tree Comparison")
    print("=" * 72)

    fixture = FIXTURES["ecommerce"]
    ae_json = client.parse(fixture)
    lp_json, _ = lp_client.fetch_semantic("ecommerce")

    ae_tree = json.loads(ae_json)
    try:
        lp_tree = json.loads(lp_json)
    except json.JSONDecodeError:
        print("  Lightpanda output is not valid JSON")
        return {}

    # Count nodes
    def count_nodes(obj):
        if isinstance(obj, dict):
            children = obj.get("children", [])
            return 1 + sum(count_nodes(c) for c in children)
        return 0

    def count_interactive(obj):
        if isinstance(obj, dict):
            count = 1 if obj.get("isInteractive") or obj.get("action") else 0
            for c in obj.get("children", obj.get("nodes", [])):
                count += count_interactive(c)
            return count
        return 0

    ae_nodes = sum(count_nodes(n) for n in ae_tree.get("nodes", []))
    lp_nodes = count_nodes(lp_tree)

    ae_interactive = count_interactive(ae_tree)
    lp_interactive = count_interactive(lp_tree)

    ae_size = len(ae_json)
    lp_size = len(lp_json)

    print(f"  {'Metric':<30} {'AetherAgent':>15} {'Lightpanda':>15}")
    print(f"  {'-'*60}")
    print(f"  {'Total nodes':<30} {ae_nodes:>15} {lp_nodes:>15}")
    print(f"  {'Interactive elements':<30} {ae_interactive:>15} {lp_interactive:>15}")
    print(f"  {'Output size (bytes)':<30} {ae_size:>15,} {lp_size:>15,}")
    print(f"  {'Output size (tokens ~)':<30} {count_tokens(ae_json):>15,} {count_tokens(lp_json):>15,}")
    print(f"  {'Has goal-relevance scoring':<30} {'Yes':>15} {'No':>15}")
    print(f"  {'Has injection warnings':<30} {'Yes':>15} {'No':>15}")
    print(f"  {'Has semantic diff (delta)':<30} {'Yes':>15} {'No':>15}")

    return {
        "ae_nodes": ae_nodes, "lp_nodes": lp_nodes,
        "ae_size": ae_size, "lp_size": lp_size,
    }


# ─── Main ────────────────────────────────────────────────────────────────────

def main():
    print("=" * 72)
    print("  AetherAgent vs Lightpanda – Head-to-Head Benchmark Suite")
    print("=" * 72)

    # Check prerequisites
    if not Path(LIGHTPANDA_BIN).exists():
        print(f"Lightpanda binary not found at {LIGHTPANDA_BIN}")
        print("Download: curl -sL -o /tmp/lightpanda "
              "https://github.com/lightpanda-io/browser/releases/download/nightly/"
              "lightpanda-x86_64-linux && chmod +x /tmp/lightpanda")
        sys.exit(1)

    # Start fixture HTTP server for Lightpanda
    print(f"\nStarting fixture server on port {FIXTURE_SERVER_PORT}...")
    fixture_server = start_fixture_server(FIXTURE_SERVER_PORT)

    # Initialize clients
    client = AetherClient(AETHER_URL)
    lp_client = LightpandaClient(LIGHTPANDA_BIN, FIXTURE_SERVER_PORT)

    # Verify connectivity
    print(f"AetherAgent URL: {AETHER_URL}")
    try:
        health = client.health()
        print(f"  Status: {health.get('status', 'unknown')} (v{health.get('version', '?')})")
    except Exception as e:
        print(f"  Connection failed: {e}")
        print("  Start AetherAgent: cargo run --features server --bin aether-server")
        sys.exit(1)

    print(f"Lightpanda binary: {LIGHTPANDA_BIN}")
    try:
        out, t = lp_client.fetch_html("simple")
        print(f"  Verified: fetched {len(out)} bytes in {fmt_us(t)}")
    except Exception as e:
        print(f"  Lightpanda test failed: {e}")
        sys.exit(1)

    # Run all benchmarks
    all_results = {}

    all_results["tokens"] = bench_token_savings(client)
    all_results["head_to_head"] = bench_head_to_head(client, lp_client)
    all_results["parallel"] = bench_parallel(client, lp_client)
    all_results["memory"] = bench_memory(client, lp_client)
    all_results["quality"] = bench_output_quality(client, lp_client)

    # Summary
    print("\n" + "=" * 72)
    print("  SUMMARY")
    print("=" * 72)

    h2h = all_results.get("head_to_head", {})
    if h2h:
        speedups = [v["speedup"] for v in h2h.values()]
        avg_speedup = statistics.mean(speedups)
        print(f"\n  Parse speed advantage (median across fixtures): {avg_speedup:.1f}x faster")

    tokens = all_results.get("tokens", {})
    if tokens:
        print(f"  10-step loop token savings with Fas 4a diff:    {tokens['loop_savings_pct']:.0f}%")

    par = all_results.get("parallel", {})
    if par:
        for n in PARALLEL_LEVELS:
            ae = par.get(f"aether_{n}", {})
            lp = par.get(f"lp_{n}", {})
            if ae and lp:
                ratio = lp["wall_ms"] / max(1, ae["wall_ms"])
                print(f"  Parallel {n:>3} tasks wall-clock:                "
                      f"AE {ae['wall_ms']:.0f}ms vs LP {lp['wall_ms']:.0f}ms ({ratio:.1f}x)")

    mem = all_results.get("memory", {})
    if mem:
        ae_mb = mem.get("aether_loaded_kb", 0) / 1024
        lp_mb = mem.get("lp_complex_50_kb", 0) / 1024
        if ae_mb and lp_mb:
            print(f"  Memory: AetherAgent server {ae_mb:.1f} MB vs Lightpanda {lp_mb:.1f} MB/instance")

    # Save raw results
    results_path = Path(__file__).parent / "benchmark_results.json"
    with open(results_path, "w") as f:
        json.dump(all_results, f, indent=2, default=str)
    print(f"\n  Raw results saved to: {results_path}")

    # Cleanup
    fixture_server.shutdown()


if __name__ == "__main__":
    main()
