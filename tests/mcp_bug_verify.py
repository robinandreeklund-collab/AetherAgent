#!/usr/bin/env python3
"""Local MCP bug verification tests with inline HTML."""
import json, asyncio, websockets

WS = "ws://localhost:3001/ws"
MID = 0

def nid():
    global MID; MID += 1; return MID

async def call(ws, tool, args):
    await ws.send(json.dumps({"jsonrpc":"2.0","id":nid(),"method":"tools/call","params":{"name":tool,"arguments":args}}))
    r = await asyncio.wait_for(ws.recv(), timeout=30)
    d = json.loads(r)
    return json.loads(d.get("result",{}).get("content",[{}])[0].get("text","{}"))

async def main():
    print("="*70)
    print("LOCAL MCP BUG VERIFICATION TEST")
    print("="*70)

    async with websockets.connect(WS, max_size=50_000_000) as ws:
        await ws.send(json.dumps({"jsonrpc":"2.0","id":nid(),"method":"initialize",
            "params":{"protocolVersion":"2024-11-05","capabilities":{},
                      "clientInfo":{"name":"test","version":"1.0"}}}))
        await ws.recv()
        await ws.send(json.dumps({"jsonrpc":"2.0","method":"notifications/initialized"}))

        passed = failed = 0

        # BUG-3: Dedup short labels
        r = await call(ws, "parse_crfr", {"html":"""<html><body>
            <a href="/h1">hide</a><a href="/h2">hide</a><a href="/h3">hide</a>
            <a href="/h4">hide</a><a href="/h5">hide</a><a href="/h6">hide</a>
            <a href="/h7">hide</a>
            <p>US news</p><p>US news</p><p>US news</p><p>US news</p><p>US news</p>
            <a href="/art">Minnesota county investigates federal agents</a>
            <a href="/art2">Trump threatens Iranian ships as blockade begins</a>
        </body></html>""", "goal":"news headlines stories articles", "top_n":15, "output_format":"json", "url":"https://test.example.com"})
        hide_count = sum(1 for n in r.get("nodes",[]) if n["label"].strip() == "hide")
        usnews_count = sum(1 for n in r.get("nodes",[]) if n["label"].strip() == "US news")
        ok = hide_count <= 2 and usnews_count <= 2
        print(f"  [{'PASS' if ok else 'FAIL'}] BUG-3: Dedup short labels (hide={hide_count}<=2, US news={usnews_count}<=2)")
        passed += ok; failed += not ok

        # BUG-4: Filter metadata
        r = await call(ws, "parse_crfr", {"html":"""<html><head>
            <meta name="viewport" content="width=device-width">
            <script type="application/ld+json">{"@context":"https://schema.org","@type":"WebSite","potentialAction":{"@type":"SearchAction"}}</script>
        </head><body><h1>Real Product</h1><p>Price: $49.99</p><a href="/buy">Buy Now</a></body></html>""",
            "goal":"product price buy purchase cost", "top_n":15, "output_format":"json", "url":"https://test.example.com"})
        meta_nodes = [n for n in r.get("nodes",[]) if n.get("name","") and
                      any(p in n.get("name","").lower() for p in ["jsonld.","meta.viewport","@type","@context","potentialaction"])]
        ok = len(meta_nodes) == 0
        print(f"  [{'PASS' if ok else 'FAIL'}] BUG-4: Filter metadata ({len(meta_nodes)} meta nodes in results)")
        if not ok:
            for n in meta_nodes: print(f"         -> {n.get('name','')}: {n.get('label','')[:50]}")
        passed += ok; failed += not ok

        # BUG-6: HTML entity cleaning
        r = await call(ws, "parse_crfr", {"html":"""<html><body>
            <h1>Johnson &amp; Johnson&#x27;s Product</h1>
            <p>Price: &lt;special&gt; only &#39;great&#39; deal</p>
        </body></html>""", "goal":"product price offer deal", "top_n":15, "output_format":"json", "url":"https://test.example.com"})
        has_entities = any("&amp;" in n.get("label","") or "&#x27;" in n.get("label","") or "&#39;" in n.get("label","")
                          for n in r.get("nodes",[]))
        ok = not has_entities
        print(f"  [{'PASS' if ok else 'FAIL'}] BUG-6: HTML entity cleaning ({'entities found' if has_entities else 'clean'})")
        if not ok:
            for n in r.get("nodes",[])[:3]: print(f"         -> {n.get('label','')[:80]}")
        passed += ok; failed += not ok

        # BUG-7: No false injection on override
        r = await call(ws, "parse_crfr", {"html":"""<html><body>
            <p>bootstrapData.cv.shared._all_.unitPriceDisplayOverride: false</p>
            <p>enableAmendFulfillmentGroupOverride: true</p>
            <h1>Welcome to our store</h1><p>Buy products at great prices</p>
        </body></html>""", "goal":"store products buy price", "top_n":15, "output_format":"json", "url":"https://test.example.com"})
        false_inj = [w for w in r.get("injection_warnings",[]) if w.get("severity") == "High" and "override" in w.get("raw_text","").lower()]
        ok = len(false_inj) == 0
        print(f"  [{'PASS' if ok else 'FAIL'}] BUG-7: No false injection ({len(false_inj)} false positives)")
        if not ok:
            for w in false_inj[:2]: print(f"         -> {w.get('severity')}: {w.get('raw_text','')[:60]}")
        passed += ok; failed += not ok

        # BUG-L4: Relative URL resolution
        r = await call(ws, "parse_crfr", {"html":"""<html><body>
            <a href="/nyheter/inrikes/article1">Swedish domestic news</a>
            <a href="//cdn.example.com/img.png">Protocol relative</a>
            <a href="https://full.example.com/page">Already absolute</a>
        </body></html>""", "goal":"news article domestic Swedish nyheter", "top_n":15, "output_format":"json", "url":"https://www.svt.se"})
        bare_relative = [n for n in r.get("nodes",[]) if (n.get("value") or "").startswith("/") and not (n.get("value") or "").startswith("//")]
        ok = len(bare_relative) == 0
        vals = [n.get("value","") for n in r.get("nodes",[]) if n.get("value")]
        print(f"  [{'PASS' if ok else 'FAIL'}] BUG-L4: URL resolution ({len(bare_relative)} bare relative URLs)")
        for v in vals[:3]: print(f"         -> {v}")
        passed += ok; failed += not ok

        # BUG-8: Wikipedia nav filter
        r = await call(ws, "parse_crfr", {"html":"""<html><body>
            <a href="/wiki/Help:Category">Help:Category</a>
            <a href="/wiki/Category:Programming_languages">Category:Programming languages</a>
            <h2>Memory management</h2>
            <p>Rust uses ownership and borrowing to manage memory safely</p>
        </body></html>""", "goal":"rust programming memory safety ownership",
            "top_n":15, "output_format":"json", "url":"https://en.wikipedia.org/wiki/Rust"})
        wiki_nav = [n for n in r.get("nodes",[]) if "/wiki/Help:" in (n.get("value","") or "") or "/wiki/Category:" in (n.get("value","") or "")]
        ok = len(wiki_nav) == 0
        print(f"  [{'PASS' if ok else 'FAIL'}] BUG-8: Wiki nav filter ({len(wiki_nav)} nav nodes)")
        passed += ok; failed += not ok

        # BUG-9: Language picker filter
        r = await call(ws, "parse_crfr", {"html":"""<html><body>
            <a href="https://europa.eu/index_bg">bg български</a>
            <a href="https://europa.eu/index_fr">fr français</a>
            <a href="https://europa.eu/index_de">de Deutsch</a>
            <a href="https://europa.eu/index_nl">nl Nederlands</a>
            <h1>European Union Policy</h1>
            <p>The EU promotes peace and security across member states.</p>
        </body></html>""", "goal":"EU european union policy institutions member states",
            "top_n":15, "output_format":"json", "url":"https://europa.eu"})
        lang_links = [n for n in r.get("nodes",[]) if "/index_" in (n.get("value","") or "") and n.get("role") == "link" and len(n.get("label","")) < 20]
        ok = len(lang_links) == 0
        print(f"  [{'PASS' if ok else 'FAIL'}] BUG-9: Language picker filter ({len(lang_links)} lang links)")
        passed += ok; failed += not ok

        # BUG-1: Feedback learning loop
        test_html = """<html><body>
            <h1>NASA Space News</h1>
            <p>Artemis II crew returned after 10-day record-setting mission</p>
            <p>Hubble spies an active spiral galaxy in deep space</p>
            <a href="/missions">All Missions</a>
            <a href="/artemis">Artemis Program</a>
        </body></html>"""
        test_url = "https://test-feedback-bug1.example.com"
        test_goal = "space mission artemis astronaut crew"

        r1 = await call(ws, "parse_crfr", {"html":test_html, "url":test_url, "goal":test_goal, "top_n":10, "output_format":"json"})
        nodes1 = r1.get("nodes",[])
        mission_ids = [n["id"] for n in nodes1 if "artemis" in n.get("label","").lower() or "mission" in n.get("label","").lower()][:3]

        if mission_ids:
            fb = await call(ws, "crfr_feedback", {"url":test_url, "goal":test_goal, "successful_node_ids":mission_ids})
            r2 = await call(ws, "parse_crfr", {"html":test_html, "url":test_url, "goal":test_goal, "top_n":10, "output_format":"json"})
            nodes2 = r2.get("nodes",[])
            boosted = [n for n in nodes2 if n.get("causal_boost",0) > 0]
            ok = len(boosted) > 0
            max_b = max((n.get("causal_boost",0) for n in nodes2), default=0)
            print(f"  [{'PASS' if ok else 'FAIL'}] BUG-1: Feedback learning ({len(boosted)} boosted, max_boost={max_b:.3f})")
            if not ok:
                for n in nodes2[:3]: print(f"         -> [{n['role']}] {n['label'][:50]} boost={n.get('causal_boost',0)}")
        else:
            ok = False
            print(f"  [FAIL] BUG-1: No mission nodes found in initial parse")
        passed += ok; failed += not ok

        print(f"\n{'='*70}")
        print(f"RESULTS: {passed} passed, {failed} failed out of {passed+failed} tests")
        print(f"{'='*70}")

asyncio.run(main())
