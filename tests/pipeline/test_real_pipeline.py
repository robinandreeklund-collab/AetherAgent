#!/usr/bin/env python3
import requests
import json
import time
from datetime import datetime
from pathlib import Path

# ================== KONFIG ==================
BASE_URL = "http://127.0.0.1:3000"
ENDPOINT = "/api/fetch/parse"
DEFAULT_GOAL = "understand the main content and key information on this page"

TEST_SITES = [
    "https://www.rust-lang.org",
    "https://quotes.toscrape.com",
    "https://github.com",
    "https://www.amazon.se/s?k=macbook",
    "https://example.com",
]

# Skapa mapp för resultat
RESULTS_DIR = Path("pipeline_results")
RESULTS_DIR.mkdir(exist_ok=True)
# ===========================================

def run_test(url: str):
    payload = {
        "url": url,
        "goal": DEFAULT_GOAL,
        "options": {
            "full_semantic": True,
            "goal_relevance": True
        }
    }

    print(f"🔄 Testar → {url}")

    start = time.time()
    resp = requests.post(BASE_URL + ENDPOINT, json=payload, timeout=30)
    duration_ms = round((time.time() - start) * 1000, 1)

    # Spara allt i en fil
    filename = RESULTS_DIR / f"result_{url.split('//')[1].split('/')[0]}_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
    
    data = {
        "timestamp": datetime.now().isoformat(),
        "url": url,
        "status": resp.status_code,
        "duration_ms": duration_ms,
        "raw_response": resp.json() if resp.status_code == 200 else resp.text
    }

    with open(filename, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)

    # Kort sammanfattning i terminalen
    if resp.status_code == 200:
        result = resp.json()
        tokens = result.get("tokens") or result.get("token_count") or result.get("tokenUsage") or "N/A"
        relevance = result.get("avg_relevance_score") or result.get("relevance") or "N/A"
        print(f"   ✅ Tokens: {tokens} | Tid: {duration_ms} ms | Relevance: {relevance}%")
    else:
        print(f"   ❌ Fel (status {resp.status_code}): {resp.text[:200]}...")

    print(f"   📁 Sparad: {filename.name}\n")


if __name__ == "__main__":
    print("🚀 AetherAgent Local Pipeline Test")
    print(f"Endpoint: {BASE_URL}{ENDPOINT}")
    print(f"Sparar filer till mappen: {RESULTS_DIR}\n")

    for url in TEST_SITES:
        run_test(url)

    print("✅ Alla tester klara! Resultaten ligger i mappen 'pipeline_results/'")