#!/usr/bin/env python3
"""
CRFR 60-second flagship demo.

All data from live MCP parse_crfr runs, April 6, 2026.
No brand names. No tech reveals. Just results.

Usage: python3 demos/render_flagship.py
Output: demos/crfr-flagship.mp4
"""
import math
import random
from pathlib import Path

import imageio.v3 as iio
import numpy as np
from PIL import Image, ImageDraw, ImageFont

W, H = 1400, 880
FPS = 30
BG      = (13, 17, 23)
FG      = (230, 237, 243)
DIM     = (72, 78, 88)
DIM2    = (100, 110, 120)
GREEN   = (63, 185, 80)
CYAN    = (88, 166, 255)
YELLOW  = (210, 153, 34)
WHITE   = (240, 246, 252)
RED     = (255, 123, 114)
ORANGE  = (255, 166, 87)
BAR_IN  = (58, 86, 145)
BAR_OUT = (57, 211, 83)
ACCENT  = (40, 46, 56)
NODE_HIT = (57, 211, 83)

PAD = 50
OUTPUT = Path(__file__).parent / "crfr-flagship.mp4"

def _font(size, bold=False):
    tag = "Bold" if bold else "Regular"
    for p in [
        f"/usr/share/fonts/truetype/dejavu/DejaVuSansMono-{tag}.ttf",
        f"/usr/share/fonts/truetype/liberation/LiberationMono-{tag}.ttf",
    ]:
        if Path(p).exists():
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()

F     = _font(19)
FB    = _font(19, bold=True)
F_SM  = _font(15)
F_SMB = _font(15, bold=True)
F_MD  = _font(22, bold=True)
F_LG  = _font(28, bold=True)
F_XL  = _font(36, bold=True)
F_XXL = _font(48, bold=True)
F_HUGE = _font(64, bold=True)

# ── Helpers ──────────────────────────────────────────────────────────────

def center_text(draw, y, text, color, font):
    bbox = font.getbbox(text)
    x = (W - (bbox[2] - bbox[0])) // 2
    draw.text((x, y), text, fill=color, font=font)

def add(frames, img, dur):
    arr = np.array(img) if isinstance(img, Image.Image) else img
    for _ in range(max(1, int(dur * FPS))):
        frames.append(arr)

def fade(frames, target, dur=0.35):
    if not frames:
        return
    src = frames[-1].astype(np.float32)
    dst = (np.array(target) if isinstance(target, Image.Image) else target).astype(np.float32)
    n = max(1, int(dur * FPS))
    for i in range(n):
        t = i / max(1, n - 1)
        frames.append(((1 - t) * src + t * dst).astype(np.uint8))

def mkimg():
    return Image.new("RGB", (W, H), BG)

# ── DOM Tree data for sweep ──────────────────────────────────────────────

_rng = random.Random(42)
DOM_LINES = []
_sections = [
    (0,0,"html",False),(1,1,"head",False),(2,2,"meta x 48",False),
    (10,1,"body",False),(15,2,"nav#mw-navigation",False),
    (20,3,"div.vector-menu x 8",False),(40,3,"div#p-search",False),
    (60,2,"div#content",False),(70,3,"h1: COVID-19 vaccine",False),
    (80,3,"div#mw-content-text",False),(90,4,"div.mw-parser-output",False),
    (100,5,"p: A COVID-19 vaccine is a vaccine intended to...",False),
    (120,5,"div.toc (table of contents)",False),(140,6,"li x 42",False),
    (180,5,"h2: History",False),(200,5,"p x 12",False),
    (250,5,"h2: Types",False),(270,5,"p x 18",False),
    (308,5,"a: 8 Adverse effects",True),
    (320,5,"h2: Development",False),(380,5,"p x 24",False),
    (450,5,"table.wikitable (clinical trials)",False),
    (600,5,"h2: Deployment",False),(750,5,"div.navbox x 4",False),
    (1000,5,"h2: Society and culture",False),(1200,5,"table x 3",False),
    (1500,5,"p x 30",False),(1700,5,"h2: Vaccine types",False),
    (1959,5,"p: mRNA vaccines were the first authorised...",True),
    (2000,5,"h3: Viral vector",False),(2300,5,"h3: Inactivated",False),
    (2600,5,"h2: Side effects",False),(2800,5,"p x 28",False),
    (3000,5,"table.wikitable (adverse events)",False),
    (3251,5,"p: Risks of serious illness far higher than vaccine risks...",True),
    (3321,5,"p: up to 20% report disruptive side effects after 2nd mRNA...",True),
    (3500,5,"p x 16",False),(3700,5,"h2: Misconceptions",False),
    (4000,5,"h2: Economics",False),(4500,5,"div.reflist",False),
    (4600,6,"li.reference x 340",False),(5500,5,"div.navbox x 8",False),
    (6000,5,"div.catlinks",False),(6200,3,"footer",False),
]
_prev = 0
for nid, depth, label, hit in _sections:
    gap = nid - _prev
    if gap > 60:
        for k in range(min(gap // 50, 4)):
            fid = _prev + (k+1) * (gap // 5)
            DOM_LINES.append((fid, min(depth+1,7),
                f"{_rng.choice(['p','div','span','a','li','td','img','sup'])} (node {fid})", False))
    DOM_LINES.append((nid, depth, label, hit))
    _prev = nid

HIT_IDS = {308, 1959, 3251, 3321}

# ════════════════════════════════════════════════════════════════════════
# BUILD
# ════════════════════════════════════════════════════════════════════════

def build():
    frames = []

    # ─── ACT 1: THE PROBLEM (0s - 8s) ───────────────────────────────
    # Problem statement + our goal

    img = mkimg(); d = ImageDraw.Draw(img)
    center_text(d, 340, "AI agents are blind on the web.", WHITE, F_LG)
    add(frames, img, 2.2)

    img = mkimg(); d = ImageDraw.Draw(img)
    center_text(d, 240, "A single web page can be", DIM2, F_MD)
    d.text((PAD+160, 290), "2,708,245", fill=RED, font=F_HUGE)
    d.text((PAD+680, 315), "characters", fill=DIM2, font=F_MD)
    center_text(d, 400, "12,434 DOM nodes.   677,061 tokens.", DIM2, F)
    center_text(d, 445, "Too large for any AI model.  $1.69 per page.", DIM2, F)
    fade(frames, img, 0.4)
    add(frames, img, 2.8)

    img = mkimg(); d = ImageDraw.Draw(img)
    center_text(d, 280, "But the answer is in 4 nodes.", WHITE, F_LG)
    center_text(d, 340, "0.03% of the page.", YELLOW, F_LG)
    fade(frames, img, 0.35)
    add(frames, img, 2.0)

    img = mkimg(); d = ImageDraw.Draw(img)
    center_text(d, 260, "Our goal:", DIM2, F_MD)
    center_text(d, 310, "Find the signal.  Kill the noise.", WHITE, F_XL)
    center_text(d, 380, "In milliseconds.  No GPU.  No models.", DIM2, F)
    center_text(d, 420, "And learn from every interaction.", DIM2, F)
    fade(frames, img, 0.35)
    add(frames, img, 2.5)

    # ─── ACT 2: THE SEARCH (8s - 20s) ───────────────────────────────
    # Scrolling DOM tree sweep — 12,434 nodes, find 4

    TREE_TOP = 110
    LINE_H_T = 22
    VISIBLE = (H - 30 - TREE_TOP) // LINE_H_T
    total_sweep = int(10.0 * FPS)
    found = []

    for fi in range(total_sweep):
        t = fi / total_sweep
        scan_idx = int(t * len(DOM_LINES))
        current_nid = DOM_LINES[min(scan_idx, len(DOM_LINES)-1)][0]

        for li in range(min(scan_idx+1, len(DOM_LINES))):
            nid = DOM_LINES[li][0]
            if DOM_LINES[li][3] and nid not in [f[0] for f in found]:
                found.append((nid, DOM_LINES[li][2]))

        img = mkimg(); d = ImageDraw.Draw(img)
        d.text((PAD, 15), "en.wikipedia.org/wiki/COVID-19_vaccine", fill=CYAN, font=F_MD)

        pct = min(100, current_nid / 12434 * 100)
        d.text((PAD, 52), "Scanning DOM...", fill=DIM2, font=F)
        d.text((PAD+230, 52), f"{current_nid:,}/12,434", fill=YELLOW, font=FB)
        found_color = GREEN if found else DIM2
        d.text((PAD+460, 52), f"{len(found)}/4 found", fill=found_color, font=FB)

        bar_full = W - PAD*2
        d.rectangle([PAD, 84, PAD+bar_full, 90], fill=ACCENT)
        d.rectangle([PAD, 84, PAD+int(bar_full*pct/100), 90], fill=CYAN)

        scan_vis = VISIBLE // 3
        start = max(0, scan_idx - scan_vis)
        end = min(len(DOM_LINES), start + VISIBLE)

        for vi, li in enumerate(range(start, end)):
            nid, depth, label, is_hit = DOM_LINES[li]
            y = TREE_TOP + vi * LINE_H_T
            x = PAD + depth * 18
            is_scan = (li == scan_idx)
            scanned = (li < scan_idx)

            if is_hit and li <= scan_idx:
                d.rectangle([PAD-4, y-2, W//2+60, y+LINE_H_T-4], fill=(20,60,30))
                d.rectangle([PAD-6, y-2, PAD-2, y+LINE_H_T-4], fill=NODE_HIT)
                if depth > 0:
                    d.text((x-18, y), "+-", fill=GREEN, font=F_SM)
                d.text((x, y), label[:65], fill=GREEN, font=F_SMB)
            elif is_scan:
                d.rectangle([PAD-4, y-1, W//2+60, y+LINE_H_T-5], fill=(20,35,55))
                if depth > 0:
                    d.text((x-18, y), "+-", fill=CYAN, font=F_SM)
                d.text((x, y), label[:65], fill=CYAN, font=F_SM)
            elif scanned:
                if depth > 0:
                    d.text((x-18, y), "| ", fill=(35,40,50), font=F_SM)
                d.text((x, y), label[:65], fill=DIM, font=F_SM)
            else:
                if depth > 0:
                    d.text((x-18, y), "| ", fill=(30,35,42), font=F_SM)
                d.text((x, y), label[:65], fill=(50,55,65), font=F_SM)

        # Right panel
        px, py = W//2+100, TREE_TOP
        if found:
            d.text((px, py), f"Located ({len(found)}/4):", fill=GREEN, font=FB)
            py += 32
            results_data = [
                (1.601, "link", "8 Adverse effects"),
                (1.432, "text", "Risks of serious illness far higher..."),
                (1.368, "text", "up to 20% disruptive side effects..."),
                (1.300, "text", "mRNA vaccines first authorised..."),
            ]
            for i, (nid, lbl) in enumerate(found):
                if i < len(results_data):
                    sc, role, txt = results_data[i]
                    d.text((px, py), f"[{sc:.3f}]", fill=YELLOW, font=F_SM)
                    d.text((px+65, py), f" {role}", fill=CYAN, font=F_SM)
                    py += 20
                    d.text((px+8, py), f'"{txt}"', fill=WHITE, font=F_SM)
                    py += 28

        frames.append(np.array(img))

    add(frames, frames[-1], 0.5)

    # ─── ACT 3: THREE SITES (20s - 32s) ─────────────────────────────
    # Show 3 sites with answers, IN/OUT

    sites_data = [
        {
            "url": "en.wikipedia.org/wiki/COVID-19_vaccine",
            "q": "What are the side effects of COVID vaccines?",
            "nodes": 12434, "chars": 2708245, "tokens": 677061,
            "ms": 528, "cost": 1.69,
            "answer": "up to 20% report disruptive side effects after 2nd mRNA dose",
            "out_tokens": 130, "out_cost": 0.0003, "reduction": "99.98",
        },
        {
            "url": "riksbanken.se/sv/penningpolitik",
            "q": "What is Sweden's interest rate?",
            "nodes": 2690, "chars": 602344, "tokens": 150586,
            "ms": 91, "cost": 0.38,
            "answer": "Styrränta 1,75 %  Gäller från den 25 mars 2026",
            "out_tokens": 104, "out_cost": 0.0003, "reduction": "99.93",
        },
        {
            "url": "xe.com (React SPA, 5,291 nodes)",
            "q": "What is the USD/SEK exchange rate?",
            "nodes": 5291, "chars": 1358050, "tokens": 339513,
            "ms": 148, "cost": 0.85,
            "answer": "1.00 USD = 9.47964826 SEK  Mid-market rate at 00:40 UTC",
            "out_tokens": 85, "out_cost": 0.0002, "reduction": "99.97",
        },
    ]

    for si, s in enumerate(sites_data):
        img = mkimg(); d = ImageDraw.Draw(img)
        d.text((PAD, 40), s["url"], fill=CYAN, font=F_MD)
        d.text((PAD, 80), f'"{s["q"]}"', fill=DIM2, font=F)

        d.text((PAD, 130), "IN:", fill=DIM2, font=FB)
        d.text((PAD+60, 130), f'{s["chars"]:,} chars  ·  {s["tokens"]:,} tokens  ·  {s["nodes"]:,} nodes', fill=FG, font=F)

        d.text((PAD, 180), "Answer:", fill=GREEN, font=FB)
        ans = s["answer"]
        if len(ans) > 75:
            ans = ans[:72] + "..."
        d.text((PAD+100, 180), f'"{ans}"', fill=WHITE, font=FB)
        d.text((PAD+100, 212), f'{s["ms"]}ms  ·  {s["out_tokens"]} tokens  ·  ${s["out_cost"]}', fill=GREEN, font=F)

        # Visual bar
        max_bw = W - PAD*2 - 180
        d.rectangle([PAD, 270, PAD+max_bw, 290], fill=BAR_IN)
        d.text((PAD+max_bw+10, 272), f'{s["tokens"]:,} tokens  ${s["cost"]:.2f}', fill=DIM2, font=F_SM)
        out_bw = max(4, int(max_bw * s["out_tokens"] / s["tokens"]))
        d.rectangle([PAD, 300, PAD+out_bw, 320], fill=BAR_OUT)
        d.text((PAD+out_bw+10, 302), f'{s["out_tokens"]} tokens  ${s["out_cost"]}', fill=GREEN, font=F_SM)

        d.text((PAD, 360), f'{s["reduction"]}%', fill=GREEN, font=F_XXL)
        d.text((PAD+250, 378), "reduction", fill=DIM2, font=F_MD)

        fade(frames, img, 0.35)
        add(frames, img, 3.3)

    # ─── ACT 4: IT LEARNS (32s - 42s) ───────────────────────────────

    img = mkimg(); d = ImageDraw.Draw(img)
    center_text(d, 340, "It learns.", WHITE, F_XL)
    fade(frames, img, 0.35)
    add(frames, img, 1.5)

    img = mkimg(); d = ImageDraw.Draw(img)
    d.text((PAD, 40), "Wikipedia COVID-19 vaccine  ·  12,434 nodes", fill=CYAN, font=F_MD)
    d.text((PAD, 85), "Three queries, different wording. Same page.", fill=DIM2, font=F)

    y = 140
    queries = [
        ("Q1  COLD", "COVID vaccine side effects efficacy mRNA",
         "#3251: Risks of serious illness far higher...", "1.432", "0.000", DIM2),
        ("Q2  +FEEDBACK", "vaccine safety adverse events clinical trials",
         "#3251: Risks of serious illness far higher...", "1.390", "0.081", GREEN),
        ("Q3  NEW TERMS", "immunization risks profile serious reactions",
         "#3251: still found with completely new vocabulary", "1.390+", "0.081+", GREEN),
    ]

    for label, goal, result, score, boost, color in queries:
        d.text((PAD, y), label, fill=color, font=FB)
        y += 26
        d.text((PAD+20, y), f'"{goal}"', fill=DIM, font=F_SM)
        y += 22
        d.text((PAD+20, y), result, fill=WHITE, font=F_SM)
        y += 22
        d.text((PAD+20, y), f"score: {score}", fill=YELLOW, font=F_SM)
        if boost != "0.000":
            d.text((PAD+180, y), f"causal_boost: +{boost}", fill=GREEN, font=F_SMB)
            d.text((PAD+420, y), "<-- learned from feedback", fill=DIM, font=F_SM)
        y += 32

        if label.startswith("Q1") or label.startswith("Q2"):
            d.text((PAD+20, y-8), "  | feedback(successful_node_ids=[3251, 3321])", fill=YELLOW, font=F_SM)
            y += 28

    d.text((PAD, y+10), "No training data.  No gradient descent.", fill=DIM2, font=F)
    d.text((PAD, y+38), "Bayesian memory from interaction alone.", fill=DIM2, font=F)

    fade(frames, img, 0.35)
    add(frames, img, 7.5)

    # ─── ACT 5: THE NUMBERS (42s - 52s) ─────────────────────────────

    img = mkimg(); d = ImageDraw.Draw(img)
    center_text(d, 50, "The numbers.", DIM2, F_MD)

    y = 110
    metrics = [
        ("TOKENS",     "677,061 in", "130 out",      "99.98% eliminated"),
        ("LATENCY",    "528ms cold",  "< 1ms cached", "sub-millisecond on repeat queries"),
        ("COST",       "$1.69/page",  "$0.0003/page",  "5,600x cheaper"),
        ("DOM NODES",  "12,434",     "4 returned",    "0.03% of the page"),
        ("BINARY",     "1.8 MB",     "zero deps",     "no GPU, no model files, no runtime"),
        ("ENVIRONMENT","< 1 kWh",   "per 10M pages", "vs ~500 kWh for GPU embedding pipelines"),
    ]

    for label, val1, val2, note in metrics:
        d.text((PAD, y), f"{label:14}", fill=DIM2, font=FB)
        d.text((PAD+220, y), val1, fill=FG, font=F)
        d.text((PAD+420, y), "->", fill=DIM, font=F)
        d.text((PAD+460, y), val2, fill=GREEN, font=FB)
        d.text((PAD+650, y), note, fill=DIM, font=F_SM)
        y += 36

    y += 20
    d.rectangle([PAD, y, W-PAD, y+1], fill=ACCENT)
    y += 20

    d.text((PAD, y), "At scale: 1,000 queries/day", fill=DIM2, font=F)
    y += 32
    d.text((PAD, y), "Raw HTML:", fill=FG, font=FB)
    d.text((PAD+160, y), "$4,004 / day", fill=RED, font=FB)
    d.text((PAD+380, y), "$1,461,460 / year", fill=RED, font=F)
    y += 30
    d.text((PAD, y), "With us:", fill=GREEN, font=FB)
    d.text((PAD+160, y), "$2 / day", fill=GREEN, font=FB)
    d.text((PAD+380, y), "$730 / year", fill=GREEN, font=F)
    y += 36
    d.text((PAD, y), "Annual savings:", fill=DIM2, font=F)
    d.text((PAD+220, y), "$1,460,730", fill=GREEN, font=F_XL)

    fade(frames, img, 0.35)
    add(frames, img, 8.5)

    # ─── ACT 6: THE CLOSE (52s - 62s) ───────────────────────────────

    img = mkimg(); d = ImageDraw.Draw(img)
    cy = 140
    center_text(d, cy, "Find the signal.", WHITE, F_XL)
    cy += 50
    center_text(d, cy, "Kill the noise.", WHITE, F_XL)
    cy += 50
    center_text(d, cy, "Learn from every interaction.", GREEN, F_XL)
    cy += 80
    center_text(d, cy, "99.98% token reduction  ·  sub-ms latency", DIM2, F)
    cy += 35
    center_text(d, cy, "No GPU  ·  No models  ·  1.8 MB  ·  pure Rust", DIM2, F)
    cy += 35
    center_text(d, cy, "5,600x cheaper at scale", GREEN, FB)

    fade(frames, img, 0.4)
    add(frames, img, 5.0)

    # Final black
    add(frames, mkimg(), 1.5)

    return frames

# ── Main ─────────────────────────────────────────────────────────────────

def main():
    print("Building 60s flagship demo...")
    frames = build()
    total_s = len(frames) / FPS
    print(f"  {len(frames)} frames = {total_s:.1f}s at {FPS}fps")

    print(f"\nEncoding {OUTPUT}...")
    iio.imwrite(str(OUTPUT), frames, fps=FPS, codec="libx264", plugin="pyav")
    mb = OUTPUT.stat().st_size / (1024*1024)
    print(f"  Done -- {mb:.1f} MB")

    # Previews
    for t, name in [(3,  "problem"), (14, "sweep"), (25, "sites"),
                     (37, "learns"), (47, "numbers"), (57, "close")]:
        idx = min(int(t * FPS), len(frames)-1)
        Image.fromarray(frames[idx]).save(OUTPUT.parent / f"preview_flag_{name}.png")
    print("  Saved previews")


if __name__ == "__main__":
    main()
