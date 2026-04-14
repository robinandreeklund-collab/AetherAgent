#!/usr/bin/env python3
"""
MCP CRFR 50-site debug test.
Tests parse_crfr + crfr_feedback against 50 real websites via local MCP server.

Checks:
1. Data quality: node labels, roles, relevance scores
2. Link quality: URLs resolved, clickable
3. Learning: crfr_feedback boosts causal memory on re-query
"""

import json
import asyncio
import sys
import time

try:
    import websockets
except ImportError:
    print("Installing websockets...")
    import subprocess
    subprocess.check_call([sys.executable, "-m", "pip", "install", "websockets", "-q"])
    import websockets

WS_URL = "ws://localhost:3001/ws"
MSG_ID = 0


def next_id():
    global MSG_ID
    MSG_ID += 1
    return MSG_ID


async def call_tool(ws, tool_name, args):
    """Call an MCP tool and return the result."""
    msg = {
        "jsonrpc": "2.0",
        "id": next_id(),
        "method": "tools/call",
        "params": {
            "name": tool_name,
            "arguments": args,
        },
    }
    await ws.send(json.dumps(msg))
    resp = await asyncio.wait_for(ws.recv(), timeout=60)
    data = json.loads(resp)
    if "error" in data:
        return {"error": data["error"]}
    # MCP result is in data["result"]["content"][0]["text"]
    content = data.get("result", {}).get("content", [])
    if content and content[0].get("type") == "text":
        return json.loads(content[0]["text"])
    return data.get("result", {})


async def test_parse_crfr(ws, url, goal, top_n=15):
    """Test parse_crfr and return analysis."""
    start = time.time()
    result = await call_tool(ws, "parse_crfr", {
        "url": url,
        "goal": goal,
        "top_n": top_n,
        "output_format": "json",
    })
    elapsed = time.time() - start

    if "error" in result:
        return {
            "url": url,
            "status": "ERROR",
            "error": str(result["error"]),
            "elapsed_s": elapsed,
        }

    nodes = result.get("nodes", [])
    total = result.get("total_nodes", 0)
    parse_ms = result.get("parse_time_ms", 0)
    warnings = result.get("injection_warnings", [])
    bot_blocked = result.get("bot_blocked", False)

    # Data quality checks
    issues = []

    # Check 1: Empty results
    if len(nodes) == 0:
        issues.append("EMPTY_RESULTS")

    # Check 2: Duplicate labels
    labels = [n.get("label", "") for n in nodes]
    label_counts = {}
    for l in labels:
        key = l.strip().lower()[:50]
        label_counts[key] = label_counts.get(key, 0) + 1
    dupes = {k: v for k, v in label_counts.items() if v > 2 and k}
    if dupes:
        issues.append(f"DUPLICATE_LABELS({len(dupes)} groups)")

    # Check 3: Metadata nodes in results
    meta_nodes = [n for n in nodes if n.get("role") == "data" and n.get("name", "")
                  and any(p in n.get("name", "").lower() for p in
                         ["jsonld", "meta.", "bootstrap", "viewport", "@type"])]
    if meta_nodes:
        issues.append(f"METADATA_NODES({len(meta_nodes)})")

    # Check 4: HTML entities in labels
    entity_nodes = [n for n in nodes if any(e in n.get("label", "")
                    for e in ["&amp;", "&#x27;", "&lt;", "&gt;", "&nbsp;"])]
    if entity_nodes:
        issues.append(f"HTML_ENTITIES({len(entity_nodes)})")

    # Check 5: HTML tags in labels
    tag_nodes = [n for n in nodes if "<p>" in n.get("label", "")
                 or "<a " in n.get("label", "") or "</p>" in n.get("label", "")]
    if tag_nodes:
        issues.append(f"HTML_TAGS({len(tag_nodes)})")

    # Check 6: Relative URLs in value field
    relative_urls = [n for n in nodes if n.get("value")
                     and n["value"].startswith("/") and not n["value"].startswith("//")]
    if relative_urls:
        issues.append(f"RELATIVE_URLS({len(relative_urls)})")

    # Check 7: Low relevance (possible noise)
    if nodes and nodes[0].get("relevance", 0) < 0.3:
        issues.append("LOW_RELEVANCE")

    # Check 8: False positive injection warnings
    false_pos = [w for w in warnings if "override" in w.get("raw_text", "").lower()
                 and "Override:" not in w.get("raw_text", "")]
    if false_pos:
        issues.append(f"FALSE_INJECTION({len(false_pos)})")

    # Link quality
    links = [n for n in nodes if n.get("value") and n.get("action") == "click"]
    valid_links = [l for l in links if l["value"].startswith("http")]

    return {
        "url": url,
        "status": "OK" if not issues else "ISSUES",
        "issues": issues,
        "node_count": len(nodes),
        "total_dom": total,
        "parse_ms": parse_ms,
        "elapsed_s": round(elapsed, 2),
        "links_total": len(links),
        "links_absolute": len(valid_links),
        "bot_blocked": bot_blocked,
        "top_relevance": round(nodes[0]["relevance"], 3) if nodes else 0,
        "has_causal": any(n.get("causal_boost", 0) > 0 for n in nodes),
        "warnings": len(warnings),
    }


async def test_feedback_loop(ws, url, goal, node_ids):
    """Test the full feedback → re-query loop."""
    # Step 1: Give feedback
    fb_result = await call_tool(ws, "crfr_feedback", {
        "url": url,
        "goal": goal,
        "successful_node_ids": node_ids,
    })
    fb_status = fb_result.get("status", "error")

    # Step 2: Re-query with same goal
    requery = await call_tool(ws, "parse_crfr", {
        "url": url,
        "goal": goal,
        "top_n": 10,
        "output_format": "json",
    })

    nodes = requery.get("nodes", [])
    boosted = [n for n in nodes if n.get("causal_boost", 0) > 0]

    return {
        "url": url,
        "feedback_status": fb_status,
        "feedback_nodes": fb_result.get("nodes_updated", 0),
        "requery_nodes": len(nodes),
        "boosted_nodes": len(boosted),
        "max_boost": max((n.get("causal_boost", 0) for n in nodes), default=0),
        "learning_works": len(boosted) > 0,
    }


# Test sites organized by category
SITES = [
    # News (10)
    ("https://www.bbc.com", "top news headlines today breaking stories articles"),
    ("https://www.cnn.com", "top news headlines today breaking stories articles"),
    ("https://www.svt.se", "nyheter rubriker senaste nyheterna headlines artiklar"),
    ("https://www.theguardian.com", "top news headlines today breaking stories articles"),
    ("https://www.nytimes.com", "top news headlines today breaking stories articles"),
    ("https://www.dn.se", "nyheter rubriker senaste nyheterna dagsnyheter artiklar"),
    ("https://www.aljazeera.com", "top news headlines today breaking stories world"),
    ("https://www.aftonbladet.se", "nyheter rubriker senaste nyheterna sport artiklar"),
    ("https://news.ycombinator.com", "hacker news top stories links articles tech programming"),
    ("https://www.reuters.com", "latest news headlines stories articles breaking"),
    # E-commerce (10)
    ("https://www.amazon.com", "products buy price deal shopping cart order item"),
    ("https://www.ebay.com", "products buy price deal auction bid shopping listing"),
    ("https://www.ikea.com/se/sv/", "möbler products furniture price pris köpa soffa bord"),
    ("https://www.etsy.com", "handmade products buy price gift shop item craft"),
    ("https://www.target.com", "products buy price deal shopping item department sale"),
    ("https://www.walmart.com", "products buy price deal shopping groceries electronics"),
    ("https://www.newegg.com", "electronics buy price deal computer GPU CPU parts"),
    ("https://www.airbnb.com", "rental accommodation stay booking price apartment travel"),
    ("https://www.booking.com", "hotel booking accommodation price rooms travel reservation"),
    ("https://www.hm.com/sv_se/", "kläder mode fashion clothes buy price pris dam herr"),
    # Tech/Docs (10)
    ("https://docs.python.org/3/", "python documentation tutorial reference library modules API"),
    ("https://developer.mozilla.org/en-US/", "web developer documentation HTML CSS JavaScript API MDN"),
    ("https://stackoverflow.com", "questions answers programming code help error debug community"),
    ("https://github.com", "repositories code projects open source developer tools trending"),
    ("https://doc.rust-lang.org/book/", "rust programming language tutorial book guide ownership"),
    ("https://en.wikipedia.org/wiki/Rust_(programming_language)", "rust programming language history features memory safety"),
    ("https://www.w3schools.com", "learn programming tutorials HTML CSS JavaScript Python SQL"),
    ("https://www.reddit.com", "posts community discussion subreddits trending popular threads"),
    ("https://medium.com", "articles blog stories read write publish trending topics"),
    ("https://www.linkedin.com", "jobs career professional network connections companies hiring"),
    # Government (5)
    ("https://www.government.se", "government policy press release ministers departments information"),
    ("https://www.usa.gov", "government services benefits information agencies help citizens"),
    ("https://europa.eu", "EU european union news policy institutions member states"),
    ("https://www.riksdagen.se", "riksdag parlament politik ledamöter debatt beslut lagar"),
    ("https://www.gov.uk", "government services information departments policy guidance"),
    # International (5)
    ("https://www.lemonde.fr", "actualités nouvelles articles politique france monde international"),
    ("https://www3.nhk.or.jp/nhkworld/", "japan news headlines stories NHK world international"),
    ("https://www.spiegel.de", "nachrichten schlagzeilen artikel politik deutschland welt"),
    ("https://www.globo.com", "notícias manchetes artigos brasil mundo esportes política"),
    ("https://www.asahi.com", "ニュース 記事 速報 政治 経済 日本 世界 news headlines"),
    # Misc (10)
    ("https://www.spotify.com", "music listen playlist songs artists albums streaming"),
    ("https://www.imdb.com", "movies films TV shows ratings reviews actors directors"),
    ("https://www.twitch.tv", "live streams gaming channels viewers popular streamers"),
    ("https://www.weather.com", "weather forecast temperature rain wind today tomorrow"),
    ("https://www.tripadvisor.com", "travel reviews hotels restaurants attractions ratings"),
    ("https://www.nasa.gov", "space exploration missions science news research rockets"),
    ("https://www.who.int", "health news disease outbreak WHO information reports global"),
    ("https://www.apple.com", "iPhone Mac iPad products buy technology innovation"),
    ("https://www.microsoft.com", "Windows Office Azure products cloud technology software"),
    ("https://www.bbc.co.uk/sport", "sports football cricket tennis results scores fixtures"),
]

# Sites to test feedback loop on (need initial parse first)
FEEDBACK_SITES = [
    ("https://www.cnn.com", "top news headlines today breaking stories articles"),
    ("https://www.svt.se", "nyheter rubriker senaste nyheterna headlines artiklar"),
    ("https://developer.mozilla.org/en-US/", "web developer documentation HTML CSS JavaScript API MDN"),
    ("https://www.nasa.gov", "space exploration missions science news research rockets"),
    ("https://www.who.int", "health news disease outbreak WHO information reports global"),
    ("https://www.aljazeera.com", "top news headlines today breaking stories world"),
    ("https://www.spiegel.de", "nachrichten schlagzeilen artikel politik deutschland welt"),
    ("https://news.ycombinator.com", "hacker news top stories links articles tech programming"),
    ("https://www.usa.gov", "government services benefits information agencies help citizens"),
    ("https://www.airbnb.com", "rental accommodation stay booking price apartment travel"),
]


async def main():
    print("=" * 80)
    print("MCP CRFR 50-SITE DEBUG TEST")
    print("=" * 80)

    async with websockets.connect(WS_URL, max_size=50_000_000) as ws:
        # Initialize MCP session
        init_msg = {
            "jsonrpc": "2.0",
            "id": next_id(),
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "test", "version": "1.0"},
            },
        }
        await ws.send(json.dumps(init_msg))
        init_resp = await ws.recv()
        print(f"MCP initialized: {json.loads(init_resp).get('result', {}).get('serverInfo', {})}")

        # Send initialized notification
        await ws.send(json.dumps({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
        }))

        # ── Phase 1: Parse all 50 sites ──
        print(f"\n{'─' * 80}")
        print("PHASE 1: Testing parse_crfr on {len(SITES)} sites")
        print(f"{'─' * 80}")

        results = []
        total_ok = 0
        total_issues = 0
        total_error = 0

        for i, (url, goal) in enumerate(SITES):
            try:
                r = await test_parse_crfr(ws, url, goal)
                results.append(r)
                status = r["status"]
                if status == "OK":
                    total_ok += 1
                    icon = "OK"
                elif status == "ISSUES":
                    total_issues += 1
                    icon = "!!"
                else:
                    total_error += 1
                    icon = "XX"

                issues_str = ", ".join(r.get("issues", [])) if r.get("issues") else ""
                print(f"  [{icon}] {i+1:2d}/{len(SITES)} {url:<45s} "
                      f"nodes={r.get('node_count', 0):3d}/{r.get('total_dom', 0):5d} "
                      f"{r.get('parse_ms', 0):4d}ms "
                      f"links={r.get('links_absolute', 0)}/{r.get('links_total', 0)} "
                      f"{'BLOCKED' if r.get('bot_blocked') else ''}"
                      f"{issues_str}")
            except Exception as e:
                results.append({"url": url, "status": "ERROR", "error": str(e)})
                total_error += 1
                print(f"  [XX] {i+1:2d}/{len(SITES)} {url:<45s} ERROR: {e}")

        # ── Phase 2: Feedback learning loop ──
        print(f"\n{'─' * 80}")
        print(f"PHASE 2: Testing crfr_feedback learning on {len(FEEDBACK_SITES)} sites")
        print(f"{'─' * 80}")

        feedback_results = []
        for url, goal in FEEDBACK_SITES:
            # Find node IDs from phase 1 results
            phase1 = next((r for r in results if r.get("url") == url), None)
            if not phase1 or phase1.get("status") == "ERROR" or phase1.get("node_count", 0) == 0:
                print(f"  [SKIP] {url} (no phase 1 data)")
                continue

            # First re-parse to get node IDs (phase 1 didn't store them)
            parse_result = await call_tool(ws, "parse_crfr", {
                "url": url, "goal": goal, "top_n": 10, "output_format": "json"
            })
            nodes = parse_result.get("nodes", [])
            if not nodes:
                print(f"  [SKIP] {url} (no nodes on re-parse)")
                continue

            # Pick top 3 content nodes as "successful"
            top_ids = [n["id"] for n in nodes[:3] if n.get("role") in
                       ("link", "text", "heading", "listitem", "main")]
            if not top_ids:
                top_ids = [nodes[0]["id"]]

            try:
                fb = await test_feedback_loop(ws, url, goal, top_ids)
                feedback_results.append(fb)
                icon = "OK" if fb["learning_works"] else "!!"
                print(f"  [{icon}] {url:<45s} "
                      f"feedback={fb['feedback_status']} "
                      f"boosted={fb['boosted_nodes']}/{fb['requery_nodes']} "
                      f"max_boost={fb['max_boost']:.3f} "
                      f"{'LEARNING_OK' if fb['learning_works'] else 'LEARNING_FAILED'}")
            except Exception as e:
                print(f"  [XX] {url:<45s} ERROR: {e}")

        # ── Summary ──
        print(f"\n{'=' * 80}")
        print("SUMMARY")
        print(f"{'=' * 80}")
        print(f"  Sites tested:    {len(SITES)}")
        print(f"  OK (clean):      {total_ok}")
        print(f"  Issues found:    {total_issues}")
        print(f"  Errors/blocked:  {total_error}")

        # Issue breakdown
        all_issues = {}
        for r in results:
            for iss in r.get("issues", []):
                key = iss.split("(")[0]
                all_issues[key] = all_issues.get(key, 0) + 1
        if all_issues:
            print(f"\n  Issue breakdown:")
            for k, v in sorted(all_issues.items(), key=lambda x: -x[1]):
                print(f"    {k:<30s} {v} sites")

        # Feedback summary
        fb_ok = sum(1 for f in feedback_results if f.get("learning_works"))
        fb_fail = sum(1 for f in feedback_results if not f.get("learning_works"))
        print(f"\n  Feedback learning:")
        print(f"    Working:  {fb_ok}/{len(feedback_results)}")
        print(f"    Failed:   {fb_fail}/{len(feedback_results)}")

        # Save full results
        output = {
            "timestamp": time.strftime("%Y-%m-%d %H:%M:%S"),
            "parse_results": results,
            "feedback_results": feedback_results,
            "summary": {
                "total_sites": len(SITES),
                "ok": total_ok,
                "issues": total_issues,
                "errors": total_error,
                "issue_breakdown": all_issues,
                "feedback_ok": fb_ok,
                "feedback_fail": fb_fail,
            },
        }
        with open("tests/mcp_50site_results.json", "w") as f:
            json.dump(output, f, indent=2, default=str)
        print(f"\n  Full results saved to tests/mcp_50site_results.json")


if __name__ == "__main__":
    asyncio.run(main())
