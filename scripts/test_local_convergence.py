#!/usr/bin/env python3
"""Test CRFR convergence against local server with real feedback loop."""
import json
import requests
import sys

BASE = "http://localhost:3000"

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

def run_site(site):
    name = site["name"]
    url = site["url"]
    keywords = site["keywords"]
    goals = site["goals"]

    print(f"\n{'='*60}")
    print(f"  {name} ({url})")
    print(f"{'='*60}")

    # Fetch HTML once
    try:
        r = requests.post(f"{BASE}/api/fetch", json={"url": url}, timeout=30)
        fetch = r.json()
        html = fetch.get("body", "")
    except Exception as e:
        print(f"  FETCH ERROR: {e}")
        return

    if len(html) < 200:
        print(f"  SKIP: body too small ({len(html)} chars)")
        return

    print(f"  Fetched {len(html)} chars")

    for i, goal in enumerate(goals):
        phase = "BASELINE" if i == 0 else ("TRAIN" if i <= 6 else "TEST")

        # Parse
        try:
            r = requests.post(f"{BASE}/api/parse-crfr", json={
                "html": html,
                "url": url,
                "goal": goal,
                "top_n": 10,
                "run_js": False,
            }, timeout=30)
            result = r.json()
        except Exception as e:
            print(f"  {phase} Q{i+1}: ERROR {e}")
            continue

        nodes = result.get("nodes", [])
        crfr = result.get("crfr", {})
        fq = crfr.get("field_queries", 0)
        cache = crfr.get("cache_hit", False)

        # Judge relevance
        rel_count = sum(1 for n in nodes if is_relevant(n.get("label", ""), keywords))
        causal_count = sum(1 for n in nodes if n.get("causal_boost", 0) > 0.001)
        max_cb = max((n.get("causal_boost", 0) for n in nodes), default=0)

        caus_str = f" causal={causal_count} max_cb={max_cb:.4f}" if causal_count > 0 else ""
        print(f"  {phase:8s} Q{i+1:2d}: fq={fq:3d} rel={rel_count}/{len(nodes)}{caus_str}  \"{goal[:45]}\"")

        # Show top-3 nodes
        for j, n in enumerate(nodes[:3]):
            label = n.get("label", "")[:75]
            rel_mark = "REL" if is_relevant(n.get("label", ""), keywords) else "---"
            cb = n.get("causal_boost", 0)
            cb_str = f" cb={cb:.4f}" if cb > 0.001 else ""
            amp = n.get('amplitude', n.get('relevance', 0))
            print(f"           #{j+1} [{rel_mark}] amp={amp:.3f}{cb_str} {n['role']}: \"{label}\"")

        # Feedback in TRAIN phase
        if phase == "TRAIN":
            feedback_ids = [n["id"] for n in nodes if is_relevant(n.get("label", ""), keywords)][:5]
            if feedback_ids:
                try:
                    fb = requests.post(f"{BASE}/api/crfr-feedback", json={
                        "url": url,
                        "goal": goal,
                        "successful_node_ids": feedback_ids,
                    }, timeout=10)
                    fb_result = fb.json()
                    print(f"           FEEDBACK: {feedback_ids} → {fb_result.get('status', '?')} fq={fb_result.get('field_queries', '?')}")
                except Exception as e:
                    print(f"           FEEDBACK ERROR: {e}")
            else:
                print(f"           FEEDBACK: no relevant nodes found")

    print()

if __name__ == "__main__":
    # Health check
    try:
        r = requests.get(f"{BASE}/health", timeout=5)
        print(f"Server: {r.json()}")
    except:
        print("ERROR: Local server not running on port 3000")
        sys.exit(1)

    for site in SITES:
        run_site(site)
