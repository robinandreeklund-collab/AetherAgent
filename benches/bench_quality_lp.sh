#!/bin/bash
# LightPanda Quality Benchmark — same 5 pre-fetched sites
# Served via local HTTP, LP fetch --dump html
set -e

LP_BIN="${LIGHTPANDA_BIN:-/root/.config/lightpanda-gomcp/lightpanda}"
PORT=18905
SERVE_DIR="/tmp/live_bench"

echo "═══════════════════════════════════════════════════════════"
echo "  LightPanda Quality Benchmark — 5 Real Sites"
echo "═══════════════════════════════════════════════════════════"

# Start local server
python3 -c "
import http.server, socketserver, threading, os
os.chdir('$SERVE_DIR')
handler = http.server.SimpleHTTPRequestHandler
srv = socketserver.TCPServer(('127.0.0.1', $PORT), handler)
t = threading.Thread(target=srv.serve_forever, daemon=True)
t.start()
import time; time.sleep(999999)
" &>/dev/null &
SERVER_PID=$!
sleep 1

cleanup() { kill $SERVER_PID 2>/dev/null; }
trap cleanup EXIT

echo "  Server: http://127.0.0.1:$PORT"
echo "  LP: $LP_BIN"
echo ""

# Measure LP startup overhead
echo "  Measuring startup overhead..."
OVERHEAD=$(python3 -c "
import subprocess, time, statistics
times = []
for i in range(5):
    start = time.monotonic()
    subprocess.run(['$LP_BIN', 'fetch', '--dump', 'html', '--log-level', 'fatal', '--wait-until', 'load', '--wait-ms', '500', 'http://127.0.0.1:$PORT/rustlang.html'], capture_output=True, timeout=15)
    times.append((time.monotonic() - start) * 1000)
times.sort()
print(f'{times[2]:.1f}')
")
echo "  Startup overhead: ~${OVERHEAD}ms"
echo ""

# Run quality benchmark
python3 << PYEOF
import subprocess, time, json, sys

LP_BIN = "${LP_BIN}"
PORT = ${PORT}
OVERHEAD = float("${OVERHEAD}")

sites = [
    ("apple.com", "apple.html", "find iPhone price"),
    ("Hacker News", "hackernews.html", "find latest news articles"),
    ("books.toscrape", "books.html", "find book titles and prices"),
    ("lobste.rs", "lobsters.html", "find technology articles"),
    ("rust-lang.org", "rustlang.html", "download and install Rust"),
]

results = []
for name, fname, goal in sites:
    url = f"http://127.0.0.1:{PORT}/{fname}"

    # Read raw HTML for token count
    with open(f"/tmp/live_bench/{fname}") as f:
        html = f.read()
    html_tokens = len(html) // 4

    # LP fetch --dump html (3 runs, take median)
    times = []
    last_output = ""
    for _ in range(3):
        start = time.monotonic()
        try:
            proc = subprocess.run(
                [LP_BIN, "fetch", "--dump", "html", "--log-level", "fatal",
                 "--wait-until", "load", "--wait-ms", "500", url],
                capture_output=True, text=True, timeout=15
            )
            elapsed = (time.monotonic() - start) * 1000
            times.append(elapsed)
            if proc.stdout:
                last_output = proc.stdout
        except:
            times.append(15000)

    times.sort()
    gross = times[1]
    net = max(0, gross - OVERHEAD)
    out_tokens = len(last_output) // 4

    # LP fetch --dump semantic_tree
    try:
        proc = subprocess.run(
            [LP_BIN, "fetch", "--dump", "semantic_tree", "--log-level", "fatal",
             "--wait-until", "load", "--wait-ms", "500", url],
            capture_output=True, text=True, timeout=15
        )
        st_output = proc.stdout
        st_tokens = len(st_output) // 4
        # Count nodes
        def count_nodes(j):
            c = 1
            for ch in j.get("children", []):
                c += count_nodes(ch)
            return c
        try:
            nodes = count_nodes(json.loads(st_output))
        except:
            nodes = 0
    except:
        st_tokens = 0
        nodes = 0

    results.append({
        "site": name, "goal": goal,
        "html_tokens": html_tokens,
        "lp_html_tokens": out_tokens,
        "lp_st_tokens": st_tokens,
        "lp_nodes": nodes,
        "gross_ms": gross,
        "net_ms": net,
    })

    savings = (1 - out_tokens / html_tokens) * 100 if html_tokens > 0 else 0
    print(f"  {name:<16} gross={gross:>8.1f}ms  net={net:>8.1f}ms  html_out={out_tokens:>6}tok  nodes={nodes:>4}  savings={savings:.1f}%")

print()
print(f"{'Site':<16} {'HTML':>7} {'LP out':>7} {'LP ST':>7} {'Nodes':>6} {'Gross':>9} {'Net':>9} {'Savings':>8}")
print("-" * 76)
total_html = total_out = 0
for r in results:
    total_html += r["html_tokens"]
    total_out += r["lp_html_tokens"]
    sav = (1 - r["lp_html_tokens"] / r["html_tokens"]) * 100 if r["html_tokens"] > 0 else 0
    print(f"{r['site']:<16} {r['html_tokens']:>7} {r['lp_html_tokens']:>7} {r['lp_st_tokens']:>7} {r['lp_nodes']:>6} {r['gross_ms']:>8.1f}ms {r['net_ms']:>8.1f}ms {sav:>7.1f}%")

overall = (1 - total_out / total_html) * 100 if total_html > 0 else 0
print("-" * 76)
print(f"{'TOTAL':<16} {total_html:>7} {total_out:>7} {'':>7} {'':>6} {'':>9} {'':>9} {overall:>7.1f}%")

with open("/home/user/AetherAgent/benches/quality_lp_results.json", "w") as f:
    json.dump(results, f, indent=2)
PYEOF
