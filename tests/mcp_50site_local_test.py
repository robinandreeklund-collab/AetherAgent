#!/usr/bin/env python3
"""
50-site CRFR test against local MCP server.
Uses inline HTML fixtures that reproduce real-world page patterns.
Tests: data quality, link quality, feedback learning, dedup, bot detection.
"""
import json, asyncio, time, sys

try:
    import websockets
except ImportError:
    import subprocess
    subprocess.check_call([sys.executable, "-m", "pip", "install", "websockets", "-q"])
    import websockets

WS_URL = "ws://localhost:3002/ws"
MSG_ID = 0

def next_id():
    global MSG_ID
    MSG_ID += 1
    return MSG_ID

async def call_tool(ws, name, args, timeout=30):
    await ws.send(json.dumps({"jsonrpc":"2.0","id":next_id(),"method":"tools/call",
        "params":{"name":name,"arguments":args}}))
    resp = await asyncio.wait_for(ws.recv(), timeout=timeout)
    data = json.loads(resp)
    if "error" in data:
        return {"_error": str(data["error"])}
    content = data.get("result",{}).get("content",[])
    if content and content[0].get("type") == "text":
        return json.loads(content[0]["text"])
    return {}

# ── HTML generators for each site category ──

def news_page(title, headlines, lang="en"):
    lis = "\n".join(f'<article><h2><a href="/article/{i}">{h}</a></h2><p>Published today. Full story about {h.lower()} with details and quotes from sources.</p></article>' for i,h in enumerate(headlines))
    return f'<html><head><title>{title}</title><meta name="description" content="Latest news from {title}"></head><body><nav><a href="/">Home</a><a href="/sport">Sport</a><a href="/login">Login</a></nav><main><h1>{title}</h1>{lis}</main><footer><p>Copyright 2026</p></footer></body></html>'

def ecommerce_page(store, products):
    items = "\n".join(f'<article class="product"><h2>{p["name"]}</h2><p class="price">${p["price"]}</p><p>{p["desc"]}</p><a href="/product/{i}">View Details</a><button>Add to Cart</button></article>' for i,p in enumerate(products))
    return f'<html><head><title>{store}</title><meta name="description" content="Shop {store} for the best deals"></head><body><nav><a href="/">Home</a><a href="/cart">Cart (0)</a><a href="/login">Sign in</a></nav><main><h1>Featured Products</h1>{items}</main><footer><p>Copyright 2026 {store}</p></footer></body></html>'

def spa_page(title):
    return f'<html><head><title>{title}</title><meta name="description" content="{title} - Interactive application"></head><body><div id="root"></div><script>document.getElementById("root").innerHTML="<h1>Welcome to {title}</h1><p>Dynamic content loaded via JavaScript framework. This has real product data and pricing.</p><a href=\\"/products\\">Browse Products</a>";</script></body></html>'

def ssr_json_page(title, articles):
    data = {"props":{"pageProps":{"sections":[{"content":[{"title":a["title"],"href":f"/articles/{i}","summary":a["summary"]} for i,a in enumerate(articles)]}]}}}
    return f'<html><head><title>{title}</title></head><body><div id="__next"></div><script id="__NEXT_DATA__" type="application/json">{json.dumps(data)}</script></body></html>'

def gov_page(title, services):
    svcs = "\n".join(f'<section><h2>{s["name"]}</h2><p>{s["desc"]}</p><a href="{s["url"]}">Learn more</a></section>' for s in services)
    return f'<html><head><title>{title}</title><meta name="description" content="Official government services"></head><body><header><h1>{title}</h1></header><main>{svcs}</main><footer><p>Official government website</p></footer></body></html>'

def blocked_page():
    return '<html><head><title>Access Denied</title></head><body><h1>Access Denied</h1><p>You don\'t have permission to access this resource. Reference#18.abc123</p></body></html>'

def cf_challenge():
    return '<html><head><title>Just a moment...</title></head><body><div class="cf-browser-verification"><h2>Checking your browser</h2></div><script src="/cdn-cgi/challenge-platform/main.js"></script></body></html>'

def cookie_consent_page(title, content):
    return f'<html><head><title>{title}</title></head><body><div class="cookie-banner"><p>We use cookies to improve your experience. Accept all cookies or reject all cookies.</p><button>Accept all</button></div><main><h1>{title}</h1><p>{content}</p></main></body></html>'

def blob_text_page(title, text):
    return f'<html><head><title>{title}</title></head><body><main><h1>{title}</h1><p>{text}</p></main></body></html>'

def nav_heavy_page():
    navs = "".join(f'<li><a href="/p{i}">Nav {i}</a></li>' for i in range(20))
    return f'<html><body><nav><ul>{navs}</ul></nav><main><h1>Important Content</h1><p>This is the actual article content about quantum computing breakthroughs in 2026.</p></main></body></html>'

def duplicate_labels_page():
    hides = "".join(f'<tr><td><a href="/story/{i}">Tech Story {i}: AI advances in healthcare</a></td><td><a href="/hide?id={i}">hide</a></td></tr>' for i in range(20))
    return f'<html><body><table>{hides}</table></body></html>'

def intl_page(title, headlines, lang_code):
    lis = "\n".join(f'<article><h2><a href="/artikel/{i}">{h}</a></h2><p>Fullständig artikel om ämnet med detaljer.</p></article>' for i,h in enumerate(headlines))
    return f'<html lang="{lang_code}"><head><title>{title}</title></head><body><main>{lis}</main></body></html>'

def wiki_page(title, sections):
    secs = "\n".join(f'<section><h2 id="{s["id"]}">{s["title"]}</h2><p>{s["text"]}</p></section>' for s in sections)
    cats = '<div id="catlinks"><a href="/wiki/Help:Category">Help:Category</a> <a href="/wiki/Category:Programming">Category:Programming</a></div>'
    return f'<html><head><title>{title} - Wikipedia</title></head><body><main>{secs}</main>{cats}<footer><a href="/wiki/JavaScript">JavaScript</a><a href="/wiki/Python">Python</a></footer></body></html>'

def entity_page():
    return '<html><body><p>The company&#x27;s Q3 earnings beat expectations &amp; analysts raised targets. Stock rose &gt;5% in after-hours trading.</p><p>CEO said: &quot;We&#39;re excited about the future.&quot;</p></body></html>'

# ── 50 Test Sites ──
SITES = []

# News (10)
for i, (title, headlines) in enumerate([
    ("BBC News", ["UK Parliament votes on trade deal", "Scientists discover high-temp superconductor", "Mars mission launch delayed"]),
    ("CNN", ["Iran blockade escalates tensions", "Minnesota investigates federal agents", "Tech giants report earnings"]),
    ("SVT Nyheter", ["Trumps blockad har inletts", "Regeringen presenterar vårbudgeten", "Explosion i Södertälje"]),
    ("The Guardian", ["Hungary election results", "Climate summit agreement reached", "AI regulation framework proposed"]),
    ("Al Jazeera", ["Naval blockade begins in Strait of Hormuz", "Hungary elects new leader", "Ceasefire violations tracked"]),
    ("Le Monde", ["French lawmakers debate colonial art", "Bardella book signing crowds", "EU reform talks continue"]),
    ("Der Spiegel", ["Deutschland braucht Agenda-Moment", "Feminismus-Debatte", "Koalition beschließt Entlastung"]),
    ("Asahi Shimbun", ["速報ニュース: 政治改革法案", "経済指標発表", "国際会議開催"]),
    ("Globo", ["Política: votação no Congresso", "Esportes: Copa do Mundo", "Economia: taxa de juros"]),
    ("Reuters", ["Global markets rally on trade optimism", "Central banks signal rate cuts", "Tech sector leads gains"]),
]):
    SITES.append(("news", title, news_page(title, headlines), "top news headlines today breaking stories articles"))

# E-commerce (10)
for title, products in [
    ("TechShop", [{"name":"MacBook Pro M4","price":"1999","desc":"16-inch laptop for professionals"},{"name":"iPhone 17","price":"1099","desc":"Latest smartphone"},{"name":"AirPods Pro","price":"249","desc":"Noise cancelling"}]),
    ("FashionStore", [{"name":"Summer Dress","price":"79","desc":"Floral print cotton"},{"name":"Denim Jacket","price":"129","desc":"Classic fit"},{"name":"Sneakers","price":"159","desc":"Running shoes"}]),
    ("HomeGoods", [{"name":"Standing Desk","price":"599","desc":"Electric adjustable"},{"name":"Office Chair","price":"449","desc":"Ergonomic mesh"},{"name":"Monitor Arm","price":"89","desc":"Dual mount"}]),
    ("BookStore", [{"name":"AI Revolution","price":"29","desc":"The future of artificial intelligence"},{"name":"Rust Programming","price":"45","desc":"Systems programming guide"},{"name":"Design Patterns","price":"55","desc":"Software architecture"}]),
    ("SportShop", [{"name":"Running Watch","price":"399","desc":"GPS fitness tracker"},{"name":"Yoga Mat","price":"49","desc":"Non-slip surface"},{"name":"Dumbbells","price":"79","desc":"Adjustable weight set"}]),
]:
    SITES.append(("ecommerce", title, ecommerce_page(title, products), "product price buy cost deal"))

# SPA with JS content (5)
for title in ["React Dashboard", "Vue Analytics", "Angular CRM", "Svelte Portfolio", "Next.js Blog"]:
    SITES.append(("spa", title, spa_page(title), "content products data application"))

# SSR JSON (5)
for title, articles in [
    ("BBC SSR", [{"title":"UK trade deal vote","summary":"Parliament debates implications"},{"title":"Superconductor breakthrough","summary":"Could revolutionize energy"}]),
    ("Next.js News", [{"title":"Tech layoffs continue","summary":"Major companies restructure"},{"title":"AI regulation proposed","summary":"EU framework for artificial intelligence"}]),
    ("Nuxt Portal", [{"title":"Climate summit results","summary":"Nations agree on emissions targets"},{"title":"Space exploration update","summary":"Mars colony plans announced"}]),
    ("Remix App", [{"title":"Economy forecast 2027","summary":"Analysts predict moderate growth"},{"title":"Healthcare reform bill","summary":"Universal coverage debate continues"}]),
    ("Gatsby Blog", [{"title":"Open source trends","summary":"Community-driven development grows"},{"title":"Cybersecurity alert","summary":"New vulnerability discovered"}]),
]:
    SITES.append(("ssr_json", title, ssr_json_page(title, articles), "news articles headlines"))

# Government (5)
for title, services in [
    ("USA.gov", [{"name":"Benefits","desc":"Find government benefits you may be eligible for","url":"/benefits"},{"name":"Taxes","desc":"File your tax return and check refund status","url":"/taxes"}]),
    ("GOV.UK", [{"name":"Services","desc":"Access government services and information","url":"/services"},{"name":"Departments","desc":"Find government departments and agencies","url":"/departments"}]),
    ("Riksdagen", [{"name":"Debatter","desc":"Se riksdagens debatter och beslut","url":"/debatter"},{"name":"Ledamöter","desc":"Hitta riksdagens ledamöter och partier","url":"/ledamoter"}]),
    ("Europa.eu", [{"name":"EU Policy","desc":"European Union policies and regulations","url":"/policy"},{"name":"Institutions","desc":"EU institutions and bodies","url":"/institutions"}]),
    ("Government.se", [{"name":"Press releases","desc":"Latest government press releases","url":"/press"},{"name":"Policy areas","desc":"Government policy priorities","url":"/policy"}]),
]:
    SITES.append(("gov", title, gov_page(title, services), "government services information policy"))

# Edge cases (15)
SITES.append(("blocked", "H&M Blocked", blocked_page(), "fashion clothes"))
SITES.append(("blocked", "CF Challenge", cf_challenge(), "products"))
SITES.append(("cookie", "Cookie News", cookie_consent_page("Tech Daily", "AI breakthrough: quantum computer solves previously impossible problems in minutes."), "AI quantum breakthrough"))
SITES.append(("blob", "Blob Text", blob_text_page("Economy Report", "The central bank raised interest rates by 0.5 percentage points today. Markets reacted strongly with a 3 percent drop. The prime minister will address the nation tonight. Opposition called for emergency debate. Consumer prices rose 4.2 percent year-over-year. Housing market shows signs of cooling. The currency fell 2 percent against the dollar. International observers expressed concern about stability. Analysts expect further tightening."), "interest rates economy central bank"))
SITES.append(("nav", "Nav Heavy", nav_heavy_page(), "quantum computing breakthroughs"))
SITES.append(("dedup", "Dedup Test", duplicate_labels_page(), "AI healthcare tech stories"))
SITES.append(("intl_sv", "SVT Test", intl_page("SVT Nyheter", ["Blockaden inledd i Hormuzsundet", "Val 2026: Nya siffror", "Ekonomisk kris i Europa"], "sv"), "nyheter politik ekonomi val"))
SITES.append(("intl_de", "Spiegel Test", intl_page("Der Spiegel", ["Reformpaket beschlossen", "Wahlen in Europa", "Klimagipfel-Ergebnis"], "de"), "politik reform wahlen nachrichten"))
SITES.append(("intl_jp", "Asahi Test", intl_page("朝日新聞", ["政治改革法案が可決", "経済成長率発表", "国際サミット開催"], "ja"), "政治 経済 ニュース"))
SITES.append(("wiki", "Rust Wiki", wiki_page("Rust (programming language)", [{"id":"overview","title":"Overview","text":"Rust is a multi-paradigm systems programming language focused on safety and performance."},{"id":"memory","title":"Memory Safety","text":"Rust enforces memory safety through ownership and borrowing rules without garbage collection."},{"id":"concurrency","title":"Concurrency","text":"Rust prevents data races at compile time through its type system."}]), "rust programming memory safety ownership"))
SITES.append(("entity", "Entity Test", entity_page(), "earnings stock company"))
SITES.append(("empty_spa", "Empty SPA", '<html><head><title>MyApp</title><meta name="description" content="Interactive dashboard with real-time data"></head><body><div id="root"></div><script type="module" src="/app.js"></script></body></html>', "dashboard data"))
SITES.append(("doc", "Python Docs", '<html><head><title>Python 3.14 Documentation</title></head><body><main><h1>Python 3.14</h1><p>Library reference: Standard library and builtins</p><p>Language reference: Syntax and language elements</p><a href="/library/">Library Reference</a><a href="/tutorial/">Tutorial</a></main></body></html>', "python documentation tutorial library"))
SITES.append(("doc", "MDN Web Docs", '<html><head><title>MDN Web Docs</title></head><body><main><h2>References</h2><a href="/docs/Web/HTML">HTML Reference</a><a href="/docs/Web/CSS">CSS Reference</a><a href="/docs/Web/JavaScript">JavaScript Reference</a><p>Documenting CSS, HTML, and JavaScript since 2005.</p></main></body></html>', "javascript css html reference documentation"))
SITES.append(("travel", "Airbnb", '<html><head><title>Airbnb</title></head><body><main><h1>Vacation Rentals</h1><article><h2>Cottage in Portland</h2><p class="price">$244 for 2 nights</p><p>4.9 out of 5 rating. Guest favorite.</p></article><article><h2>Apartment in San Diego</h2><p class="price">$383 for 2 nights</p><p>4.88 rating.</p></article></main></body></html>', "rental accommodation price stay booking"))

async def main():
    print("=" * 80)
    print("50-SITE LOCAL MCP TEST — Phase 1-4 Verification")
    print("=" * 80)

    async with websockets.connect(WS_URL, max_size=50_000_000) as ws:
        # Init MCP
        await ws.send(json.dumps({"jsonrpc":"2.0","id":next_id(),"method":"initialize",
            "params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test50","version":"1.0"}}}))
        await ws.recv()
        await ws.send(json.dumps({"jsonrpc":"2.0","method":"notifications/initialized"}))

        results = []
        ok = issues = errors = 0

        for i, (cat, title, html, goal) in enumerate(SITES):
            url = f"https://{title.lower().replace(' ','-')}.example.com"
            try:
                r = await call_tool(ws, "parse_crfr", {"html": html, "url": url, "goal": goal, "top_n": 15, "output_format": "json"})
                nodes = r.get("nodes", [])
                total = r.get("total_nodes", 0)
                ms = r.get("parse_time_ms", 0)
                blocked = r.get("bot_blocked", False)

                # Quality checks
                iss = []
                if len(nodes) == 0 and not blocked: iss.append("EMPTY")
                labels = [n.get("label","").strip().lower()[:50] for n in nodes]
                dupes = {l: labels.count(l) for l in set(labels) if labels.count(l) > 2 and l}
                if dupes: iss.append(f"DUPES({len(dupes)})")
                meta = [n for n in nodes if n.get("role")=="data" and n.get("name","") and ("jsonld" in n.get("name","").lower() or "meta.viewport" in n.get("name","").lower())]
                if meta: iss.append(f"META({len(meta)})")
                ent = [n for n in nodes if any(e in n.get("label","") for e in ["&amp;","&#x27;","&lt;"])]
                if ent: iss.append(f"ENTITIES({len(ent)})")
                rel_urls = [n for n in nodes if n.get("value","") and n["value"].startswith("/") and not n["value"].startswith("//")]
                if rel_urls: iss.append(f"REL_URL({len(rel_urls)})")

                status = "BLOCK" if blocked else ("OK" if not iss else "ISSUE")
                if status == "OK": ok += 1
                elif status == "BLOCK": errors += 1
                else: issues += 1

                icon = {"OK":"✓","ISSUE":"!","BLOCK":"✗"}[status]
                iss_str = " ".join(iss) if iss else ""
                print(f"  [{icon}] {i+1:2d}/50 [{cat:8s}] {title:<22s} nodes={len(nodes):2d}/{total:4d} {ms:3d}ms {iss_str}")
                results.append({"cat":cat,"title":title,"status":status,"nodes":len(nodes),"total":total,"ms":ms,"issues":iss,"blocked":blocked})
            except Exception as e:
                errors += 1
                print(f"  [✗] {i+1:2d}/50 [{cat:8s}] {title:<22s} ERROR: {e}")
                results.append({"cat":cat,"title":title,"status":"ERROR","error":str(e)})

        # ── Feedback learning test ──
        print(f"\n{'─'*80}")
        print("FEEDBACK LEARNING TEST (5 sites)")
        print(f"{'─'*80}")
        fb_ok = 0
        for cat, title, html, goal in SITES[:5]:
            url = f"https://{title.lower().replace(' ','-')}.example.com"
            r1 = await call_tool(ws, "parse_crfr", {"html":html,"url":url,"goal":goal,"top_n":5,"output_format":"json"})
            nodes = r1.get("nodes",[])
            if not nodes: continue
            top_ids = [n["id"] for n in nodes[:2] if n.get("id") is not None]
            if not top_ids: continue
            fb = await call_tool(ws, "crfr_feedback", {"url":url,"goal":goal,"successful_node_ids":top_ids})
            r2 = await call_tool(ws, "parse_crfr", {"html":html,"url":url,"goal":goal,"top_n":5,"output_format":"json"})
            nodes2 = r2.get("nodes",[])
            boosted = [n for n in nodes2 if n.get("causal_boost",0) > 0]
            ok_flag = len(boosted) > 0
            if ok_flag: fb_ok += 1
            icon = "✓" if ok_flag else "✗"
            max_b = max((n.get("causal_boost",0) for n in nodes2), default=0)
            print(f"  [{icon}] {title:<22s} boosted={len(boosted)}/{len(nodes2)} max_boost={max_b:.3f}")

        # ── Summary ──
        print(f"\n{'='*80}")
        print("RESULTS")
        print(f"{'='*80}")
        print(f"  Sites tested:        50")
        print(f"  Clean (OK):          {ok}")
        print(f"  Issues:              {issues}")
        print(f"  Blocked/Error:       {errors}")
        print(f"  Success rate:        {ok*100//50}%")
        print(f"  Feedback learning:   {fb_ok}/5")

        all_iss = {}
        for r in results:
            for iss in r.get("issues",[]):
                k = iss.split("(")[0]
                all_iss[k] = all_iss.get(k,0) + 1
        if all_iss:
            print(f"\n  Remaining issues:")
            for k,v in sorted(all_iss.items(), key=lambda x:-x[1]):
                print(f"    {k:<20s} {v} sites")

        by_cat = {}
        for r in results:
            c = r["cat"]
            by_cat.setdefault(c, {"ok":0,"total":0})
            by_cat[c]["total"] += 1
            if r["status"] == "OK": by_cat[c]["ok"] += 1
        print(f"\n  By category:")
        for c, s in sorted(by_cat.items()):
            print(f"    {c:<12s} {s['ok']}/{s['total']} OK")

        with open("tests/mcp_50site_local_results.json","w") as f:
            json.dump({"results":results,"summary":{"ok":ok,"issues":issues,"errors":errors,"feedback_ok":fb_ok}}, f, indent=2)
        print(f"\n  Saved to tests/mcp_50site_local_results.json")

if __name__ == "__main__":
    asyncio.run(main())
