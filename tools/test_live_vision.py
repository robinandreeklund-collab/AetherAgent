#!/usr/bin/env python3
"""
AetherAgent — Live Site Vision Test (post-settle/lazy/nojs changes)
===================================================================
Hämtar live-sajter via fetch-vision endpoint, sparar:
  1. Raw screenshot (PNG)
  2. Annoterad screenshot med ONNX-detektioner (PNG)
  3. Detektions-JSON per sajt
  4. Sammanfattning (summary.json)

Output: tools/results/<timestamp>/

Kräver: Server på localhost:3000 med --features server,vision,blitz

Usage:
    python3 tools/test_live_vision.py [BASE_URL]
"""

import base64
import json
import os
import sys
import time
from datetime import datetime

import requests

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:3000"
TIMEOUT = 90

# Sajter att testa — blandning av statiska, bildtunga, JS-varning, lazy-load
SITES = [
    {
        "name": "example_com",
        "url": "https://example.com",
        "goal": "find main content",
        "type": "static",
    },
    {
        "name": "hacker_news",
        "url": "https://news.ycombinator.com",
        "goal": "find top stories and links",
        "type": "static",
    },
    {
        "name": "books_toscrape",
        "url": "https://books.toscrape.com",
        "goal": "find book prices and titles",
        "type": "ecommerce-images",
    },
    {
        "name": "quotes_toscrape",
        "url": "https://quotes.toscrape.com",
        "goal": "extract quotes and authors",
        "type": "static",
    },
    {
        "name": "httpbin_html",
        "url": "https://httpbin.org/html",
        "goal": "extract text content",
        "type": "static",
    },
    {
        "name": "wikipedia_rust",
        "url": "https://en.wikipedia.org/wiki/Rust_(programming_language)",
        "goal": "find article headings and links",
        "type": "static-images",
    },
]


def post(path, data, timeout=TIMEOUT):
    return requests.post(f"{BASE}{path}", json=data, timeout=timeout)


def get(path, timeout=TIMEOUT):
    return requests.get(f"{BASE}{path}", timeout=timeout)


def save_b64_png(b64_data, filepath):
    """Spara base64-kodad PNG till fil."""
    raw = base64.b64decode(b64_data)
    with open(filepath, "wb") as f:
        f.write(raw)
    return len(raw)


def main():
    # Skapa output-mapp med tidsstämpel
    timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
    out_dir = os.path.join(os.path.dirname(__file__), "results", timestamp)
    os.makedirs(out_dir, exist_ok=True)

    print(f"\n{'═' * 72}")
    print(f"  AetherAgent — Live Vision Test (settle + lazy + nojs)")
    print(f"  Server: {BASE}")
    print(f"  Output: {out_dir}")
    print(f"  Tid:    {timestamp}")
    print(f"{'═' * 72}\n")

    # Health check
    print("▸ Health Check")
    try:
        t0 = time.time()
        resp = get("/health")
        health_ms = (time.time() - t0) * 1000
        data = resp.json()
        print(f"  Status: {data.get('status')}  Version: {data.get('version')}  Latency: {health_ms:.0f}ms")
    except Exception as e:
        print(f"  ERROR: {e}")
        print("  Servern körs inte — starta med: cargo run --release --features server,vision,blitz --bin server")
        sys.exit(1)

    # Kör vision-test per sajt
    summary = {
        "timestamp": timestamp,
        "server": BASE,
        "sites": [],
    }

    print(f"\n{'─' * 72}")
    print("▸ fetch-vision — Live Site Screenshots + ONNX Detection")
    print(f"{'─' * 72}")
    print(f"  {'Sajt':<22} {'Tier':>6} {'Detekt.':>8} {'Raw KB':>8} {'Ann KB':>8} {'Latency':>10} {'Klasser'}")
    print(f"  {'─' * 22} {'─' * 6} {'─' * 8} {'─' * 8} {'─' * 8} {'─' * 10} {'─' * 30}")

    for site in SITES:
        site_start = time.time()
        try:
            resp = post("/api/fetch-vision", {
                "url": site["url"],
                "goal": site["goal"],
                "width": 1280,
                "height": 900,
                "fast_render": False,
            }, timeout=TIMEOUT)
            latency_ms = (time.time() - site_start) * 1000

            if resp.status_code != 200:
                print(f"  {site['name']:<22} ERROR: HTTP {resp.status_code} — {resp.text[:100]}")
                summary["sites"].append({
                    "name": site["name"],
                    "url": site["url"],
                    "error": f"HTTP {resp.status_code}",
                })
                continue

            data = resp.json()

            tier_used = data.get("tier_used", "?")
            original_b64 = data.get("original_screenshot", "")
            annotated_b64 = data.get("annotated_screenshot", "")
            detections_data = data.get("detections", {})

            # Spara raw screenshot
            raw_path = os.path.join(out_dir, f"{site['name']}_raw.png")
            raw_size = 0
            if original_b64:
                raw_size = save_b64_png(original_b64, raw_path)

            # Spara annoterad screenshot (med ONNX bounding boxes)
            ann_path = os.path.join(out_dir, f"{site['name']}_onnx.png")
            ann_size = 0
            if annotated_b64:
                ann_size = save_b64_png(annotated_b64, ann_path)

            # Spara detektions-JSON
            det_path = os.path.join(out_dir, f"{site['name']}_detections.json")
            with open(det_path, "w") as f:
                json.dump(detections_data, f, indent=2)

            # Sammanfatta detektioner
            detections = detections_data.get("detections", [])
            class_counts = {}
            for d in detections:
                cls = d.get("class", "?")
                class_counts[cls] = class_counts.get(cls, 0) + 1
            class_summary = ", ".join(
                f"{v}×{k}" for k, v in sorted(class_counts.items(), key=lambda x: -x[1])
            )

            print(
                f"  {site['name']:<22} {tier_used:>6} {len(detections):>8} "
                f"{raw_size / 1024:>7.1f} {ann_size / 1024:>7.1f} "
                f"{latency_ms:>9.0f}ms {class_summary[:40]}"
            )

            summary["sites"].append({
                "name": site["name"],
                "url": site["url"],
                "type": site["type"],
                "tier_used": tier_used,
                "latency_ms": round(latency_ms, 1),
                "detection_count": len(detections),
                "class_counts": class_counts,
                "raw_screenshot_kb": round(raw_size / 1024, 1),
                "annotated_screenshot_kb": round(ann_size / 1024, 1),
                "inference_time_ms": detections_data.get("inference_time_ms"),
                "preprocess_time_ms": detections_data.get("preprocess_time_ms"),
                "raw_detection_count": detections_data.get("raw_detection_count"),
                "files": {
                    "raw": f"{site['name']}_raw.png",
                    "annotated": f"{site['name']}_onnx.png",
                    "detections": f"{site['name']}_detections.json",
                },
            })

        except requests.exceptions.Timeout:
            print(f"  {site['name']:<22} TIMEOUT ({TIMEOUT}s)")
            summary["sites"].append({
                "name": site["name"], "url": site["url"], "error": "timeout"
            })
        except Exception as e:
            print(f"  {site['name']:<22} ERROR: {e}")
            summary["sites"].append({
                "name": site["name"], "url": site["url"], "error": str(e)
            })

    # Spara sammanfattning
    summary_path = os.path.join(out_dir, "summary.json")
    with open(summary_path, "w") as f:
        json.dump(summary, f, indent=2)

    # Tier stats
    print(f"\n{'─' * 72}")
    print("▸ Tier Stats")
    print(f"{'─' * 72}")
    try:
        resp = get("/api/tier-stats")
        stats = resp.json()
        print(f"  {json.dumps(stats, indent=2)}")
        stats_path = os.path.join(out_dir, "tier_stats.json")
        with open(stats_path, "w") as f:
            json.dump(stats, f, indent=2)
    except Exception as e:
        print(f"  Kunde inte hämta tier stats: {e}")

    # Sammanfattning
    total_dets = sum(s.get("detection_count", 0) for s in summary["sites"] if "error" not in s)
    ok_count = sum(1 for s in summary["sites"] if "error" not in s)
    err_count = sum(1 for s in summary["sites"] if "error" in s)

    print(f"\n{'═' * 72}")
    print(f"  RESULTAT: {ok_count}/{len(SITES)} sajter OK, {err_count} fel")
    print(f"  Totalt {total_dets} detektioner")
    print(f"  Sparade till: {out_dir}")
    print(f"{'═' * 72}\n")

    # Lista alla sparade filer
    files = sorted(os.listdir(out_dir))
    print(f"  Filer ({len(files)}):")
    for f in files:
        size = os.path.getsize(os.path.join(out_dir, f))
        print(f"    {f:<45} {size / 1024:>7.1f} KB")

    return 0 if err_count == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
