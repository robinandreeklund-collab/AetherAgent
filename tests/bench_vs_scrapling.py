#!/usr/bin/env python3
"""
Benchmark: AetherAgent (CRFR via local MCP) vs Scrapling vs Parsel/lxml
Tests: Parse speed, text extraction, element finding, similarity tracking.
"""
import json, time, statistics, asyncio

# ── Generate test HTML ──
def gen_html(n_elements=5000):
    """Generate realistic HTML with n nested elements."""
    parts = ['<!DOCTYPE html><html><head><title>Benchmark Page</title></head><body>']
    parts.append('<nav><ul>')
    for i in range(20):
        parts.append(f'<li><a href="/page-{i}">Nav Link {i}</a></li>')
    parts.append('</ul></nav><main>')
    for i in range(n_elements):
        depth = i % 5
        tag = ['div', 'section', 'article', 'p', 'span'][depth]
        cls = f'item-{i % 10}'
        parts.append(f'<{tag} class="{cls}" data-id="{i}">')
        if i % 7 == 0:
            parts.append(f'<h2>Heading {i}</h2>')
        if i % 3 == 0:
            parts.append(f'<a href="/product/{i}">Product {i} - Buy now for ${i * 1.5:.2f}</a>')
        if i % 5 == 0:
            parts.append(f'<p>Description for item {i}. This is a longer text that contains '
                        f'information about the product, including pricing at ${i * 2:.2f} '
                        f'and availability status. Ships within {i % 7 + 1} days.</p>')
        parts.append(f'<span class="price">${i * 1.99:.2f}</span>')
        parts.append(f'</{tag}>')
    parts.append('</main><footer><p>Footer content</p></footer></body></html>')
    return ''.join(parts)

def gen_small_html():
    """Small realistic page for semantic tests."""
    return '''<!DOCTYPE html><html><head><title>E-Shop</title></head><body>
    <nav><a href="/">Home</a> <a href="/products">Products</a> <a href="/cart">Cart (3)</a></nav>
    <main>
        <h1>Summer Sale - Up to 50% Off</h1>
        <article class="product" data-id="p1">
            <h2>Wireless Headphones</h2>
            <p class="price">$79.99 <span class="original">$159.99</span></p>
            <p class="desc">Premium noise-cancelling headphones with 30h battery life.</p>
            <button data-action="add-to-cart">Add to Cart</button>
            <a href="/product/wireless-headphones">View Details</a>
        </article>
        <article class="product" data-id="p2">
            <h2>Smart Watch Pro</h2>
            <p class="price">$249.00</p>
            <p class="desc">Advanced fitness tracking with GPS and heart rate monitor.</p>
            <button data-action="add-to-cart">Add to Cart</button>
            <a href="/product/smart-watch-pro">View Details</a>
        </article>
        <article class="product" data-id="p3">
            <h2>USB-C Hub Adapter</h2>
            <p class="price">$34.99</p>
            <p class="desc">7-in-1 hub with HDMI, USB 3.0, SD card reader, and PD charging.</p>
            <button data-action="add-to-cart">Add to Cart</button>
            <a href="/product/usb-c-hub">View Details</a>
        </article>
    </main>
    <footer><p>&copy; 2026 E-Shop. All rights reserved.</p></footer>
    </body></html>'''

# ── Scrapling benchmarks ──
def bench_scrapling_parse(html, runs=50):
    from scrapling import Selector as Adaptor
    times = []
    for _ in range(runs):
        t0 = time.perf_counter()
        page = Adaptor(html, auto_match=False)
        _ = page.css('a')  # force parse
        times.append((time.perf_counter() - t0) * 1000)
    return times

def bench_scrapling_text(html, runs=50):
    from scrapling import Selector as Adaptor
    page = Adaptor(html, auto_match=False)
    times = []
    for _ in range(runs):
        t0 = time.perf_counter()
        texts = page.css('p::text')
        times.append((time.perf_counter() - t0) * 1000)
    return times

def bench_scrapling_find(html, runs=50):
    from scrapling import Selector as Adaptor
    page = Adaptor(html, auto_match=False)
    times = []
    for _ in range(runs):
        t0 = time.perf_counter()
        prices = page.css('.price')
        links = page.css('a[href*="product"]')
        times.append((time.perf_counter() - t0) * 1000)
    return times

# ── Parsel/lxml benchmarks ──
def bench_parsel_parse(html, runs=50):
    from parsel import Selector
    times = []
    for _ in range(runs):
        t0 = time.perf_counter()
        sel = Selector(text=html)
        _ = sel.css('a').getall()
        times.append((time.perf_counter() - t0) * 1000)
    return times

def bench_parsel_text(html, runs=50):
    from parsel import Selector
    sel = Selector(text=html)
    times = []
    for _ in range(runs):
        t0 = time.perf_counter()
        texts = sel.css('p::text').getall()
        times.append((time.perf_counter() - t0) * 1000)
    return times

# ── AetherAgent MCP benchmarks ──
async def bench_aether_parse(html, url, runs=20):
    import websockets
    times = []
    async with websockets.connect("ws://localhost:3001/ws", max_size=50_000_000) as ws:
        await ws.send(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2024-11-05","capabilities":{},
                      "clientInfo":{"name":"bench","version":"1.0"}}}))
        await ws.recv()
        await ws.send(json.dumps({"jsonrpc":"2.0","method":"notifications/initialized"}))

        for i in range(runs):
            t0 = time.perf_counter()
            await ws.send(json.dumps({"jsonrpc":"2.0","id":i+10,"method":"tools/call",
                "params":{"name":"parse_crfr","arguments":{
                    "html": html, "url": url,
                    "goal": "product price buy headphones watch",
                    "top_n": 20, "output_format": "json"
                }}}))
            resp = await asyncio.wait_for(ws.recv(), timeout=30)
            elapsed = (time.perf_counter() - t0) * 1000
            times.append(elapsed)

            # Extract internal parse_time_ms for comparison
            try:
                data = json.loads(resp)
                text = data["result"]["content"][0]["text"]
                parsed = json.loads(text)
                if i == 0:
                    node_count = parsed.get("node_count", 0)
                    total_nodes = parsed.get("total_nodes", 0)
                    parse_ms = parsed.get("parse_time_ms", 0)
                    print(f"    AetherAgent first run: {node_count} nodes / {total_nodes} total, internal {parse_ms}ms")
            except Exception:
                pass
    return times

async def bench_aether_semantic(html, url):
    """Test semantic understanding — find products, prices, actions."""
    import websockets
    async with websockets.connect("ws://localhost:3001/ws", max_size=50_000_000) as ws:
        await ws.send(json.dumps({"jsonrpc":"2.0","id":1,"method":"initialize",
            "params":{"protocolVersion":"2024-11-05","capabilities":{},
                      "clientInfo":{"name":"bench","version":"1.0"}}}))
        await ws.recv()
        await ws.send(json.dumps({"jsonrpc":"2.0","method":"notifications/initialized"}))

        results = {}
        for goal, label in [
            ("product price cost amount $", "price_extraction"),
            ("add to cart buy purchase button", "action_finding"),
            ("product name title heading description", "content_finding"),
        ]:
            t0 = time.perf_counter()
            await ws.send(json.dumps({"jsonrpc":"2.0","id":100,"method":"tools/call",
                "params":{"name":"parse_crfr","arguments":{
                    "html": html, "url": url, "goal": goal,
                    "top_n": 10, "output_format": "json"
                }}}))
            resp = await asyncio.wait_for(ws.recv(), timeout=15)
            elapsed_ms = (time.perf_counter() - t0) * 1000
            data = json.loads(json.loads(resp)["result"]["content"][0]["text"])
            nodes = data.get("nodes", [])
            results[label] = {
                "time_ms": round(elapsed_ms, 2),
                "nodes_found": len(nodes),
                "top_labels": [n["label"][:60] for n in nodes[:3]],
                "top_relevance": [round(n["relevance"], 3) for n in nodes[:3]],
            }
    return results


def main():
    print("=" * 80)
    print("BENCHMARK: AetherAgent vs Scrapling vs Parsel/lxml")
    print("=" * 80)

    html_5k = gen_html(5000)
    html_small = gen_small_html()
    print(f"\nTest HTML sizes: large={len(html_5k)//1024}KB, small={len(html_small)//1024}KB")

    # ── Test 1: Large page parse speed ──
    print(f"\n{'─'*80}")
    print("TEST 1: Parse speed (5000 elements, 50 runs)")
    print(f"{'─'*80}")

    scr_times = bench_scrapling_parse(html_5k, 50)
    par_times = bench_parsel_parse(html_5k, 50)
    aether_times = asyncio.run(bench_aether_parse(html_5k, "https://bench.example.com", 20))

    print(f"\n  {'Library':<25s} {'Median':>10s} {'P95':>10s} {'Min':>10s} {'Max':>10s}")
    print(f"  {'─'*65}")
    for name, t in [("Scrapling", scr_times), ("Parsel/lxml", par_times), ("AetherAgent (CRFR)", aether_times)]:
        med = statistics.median(t)
        p95 = sorted(t)[int(len(t)*0.95)]
        print(f"  {name:<25s} {med:>8.2f}ms {p95:>8.2f}ms {min(t):>8.2f}ms {max(t):>8.2f}ms")

    # ── Test 2: Text extraction ──
    print(f"\n{'─'*80}")
    print("TEST 2: Text extraction (CSS .price, 50 runs)")
    print(f"{'─'*80}")

    scr_text = bench_scrapling_text(html_5k, 50)
    par_text = bench_parsel_text(html_5k, 50)

    print(f"\n  {'Library':<25s} {'Median':>10s} {'P95':>10s}")
    print(f"  {'─'*50}")
    for name, t in [("Scrapling", scr_text), ("Parsel/lxml", par_text)]:
        med = statistics.median(t)
        p95 = sorted(t)[int(len(t)*0.95)]
        print(f"  {name:<25s} {med:>8.2f}ms {p95:>8.2f}ms")

    # ── Test 3: Element finding ──
    print(f"\n{'─'*80}")
    print("TEST 3: Element finding (.price + a[href*=product], 50 runs)")
    print(f"{'─'*80}")

    scr_find = bench_scrapling_find(html_5k, 50)

    print(f"\n  {'Library':<25s} {'Median':>10s} {'P95':>10s}")
    print(f"  {'─'*50}")
    for name, t in [("Scrapling", scr_find)]:
        med = statistics.median(t)
        p95 = sorted(t)[int(len(t)*0.95)]
        print(f"  {name:<25s} {med:>8.2f}ms {p95:>8.2f}ms")

    # ── Test 4: Semantic understanding (AetherAgent only) ──
    print(f"\n{'─'*80}")
    print("TEST 4: Semantic understanding (AetherAgent CRFR, small page)")
    print(f"{'─'*80}")

    semantic = asyncio.run(bench_aether_semantic(html_small, "https://eshop.example.com"))
    for label, data in semantic.items():
        print(f"\n  Goal: {label}")
        print(f"    Time: {data['time_ms']}ms, Nodes: {data['nodes_found']}")
        for i, (lbl, rel) in enumerate(zip(data['top_labels'], data['top_relevance'])):
            print(f"    #{i+1}: [{rel:.3f}] {lbl}")

    # ── Test 5: Scrapling has no semantic equivalent ──
    print(f"\n{'─'*80}")
    print("TEST 5: Feature comparison matrix")
    print(f"{'─'*80}")

    features = [
        ("HTML parsing", "YES (lxml)", "YES (html5ever + arena DOM)"),
        ("CSS selectors", "YES", "YES (in dom_bridge)"),
        ("XPath", "YES", "NO (not needed for semantic approach)"),
        ("Semantic roles", "NO", "YES (button, link, heading, price, etc.)"),
        ("Goal-driven ranking", "NO", "YES (CRFR resonance field)"),
        ("Causal learning", "NO", "YES (crfr_feedback)"),
        ("Prompt injection detection", "NO", "YES (trust shield)"),
        ("JS evaluation", "Playwright (external)", "QuickJS (embedded, sandboxed)"),
        ("Anti-bot/TLS fingerprint", "YES (curl_cffi)", "NO"),
        ("Adaptive element tracking", "YES (SequenceMatcher)", "YES (semantic diff + HV matching)"),
        ("Streaming DOM", "NO", "YES (95-99% token savings)"),
        ("Vision/YOLO", "NO", "YES (22 UI element classes)"),
        ("WASM compilation", "NO", "YES (browser-embeddable)"),
        ("MCP server", "YES (10 tools)", "YES (35+ tools)"),
        ("Robots.txt", "YES (protego)", "YES (RFC 9309 robotstxt)"),
    ]

    print(f"\n  {'Feature':<35s} {'Scrapling':<30s} {'AetherAgent':<35s}")
    print(f"  {'─'*100}")
    for feat, scr, ae in features:
        print(f"  {feat:<35s} {scr:<30s} {ae:<35s}")

    # ── Summary stats for report ──
    results = {
        "parse_speed": {
            "scrapling_median_ms": round(statistics.median(scr_times), 2),
            "parsel_median_ms": round(statistics.median(par_times), 2),
            "aether_median_ms": round(statistics.median(aether_times), 2),
        },
        "text_extraction": {
            "scrapling_median_ms": round(statistics.median(scr_text), 2),
            "parsel_median_ms": round(statistics.median(par_text), 2),
        },
        "semantic": semantic,
    }
    with open("tests/bench_scrapling_results.json", "w") as f:
        json.dump(results, f, indent=2)
    print(f"\n  Results saved to tests/bench_scrapling_results.json")


if __name__ == "__main__":
    main()
