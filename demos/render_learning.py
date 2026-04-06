#!/usr/bin/env python3
"""
CRFR "It Learns" Demo — 3 sites × 3 queries showing causal memory growth.

All data from live MCP parse_crfr + crfr_feedback runs, April 6, 2026.
Shows: Query 1 (cold) → feedback → Query 2 (improved) → feedback → Query 3 (generalization).

Usage: python3 demos/render_learning.py
Output: demos/crfr-it-learns.mp4
"""
from pathlib import Path

import imageio.v3 as iio
import numpy as np
from PIL import Image, ImageDraw, ImageFont

# ── Config ───────────────────────────────────────────────────────────────

W, H = 1400, 880
FPS = 30
BG     = (13, 17, 23)
FG     = (230, 237, 243)
DIM    = (100, 110, 120)
GREEN  = (63, 185, 80)
CYAN   = (88, 166, 255)
YELLOW = (210, 153, 34)
WHITE  = (240, 246, 252)
RED    = (255, 123, 114)
ORANGE = (255, 166, 87)
BAR_COLD = (68, 106, 175)
BAR_WARM = (57, 211, 83)
ACCENT = (48, 54, 65)

PAD = 36
LH  = 28
FS  = 19
OUTPUT = Path(__file__).parent / "crfr-it-learns.mp4"

def _font(size, bold=False):
    tag = "Bold" if bold else "Regular"
    for p in [
        f"/usr/share/fonts/truetype/dejavu/DejaVuSansMono-{tag}.ttf",
        f"/usr/share/fonts/truetype/liberation/LiberationMono-{tag}.ttf",
    ]:
        if Path(p).exists():
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()

F     = _font(FS)
FB    = _font(FS, bold=True)
F_BIG = _font(24, bold=True)
F_SM  = _font(15)
F_SMB = _font(15, bold=True)
F_XL  = _font(38, bold=True)
F_XXL = _font(42, bold=True)
F_HUGE = _font(52, bold=True)

# ── Authentic data: live MCP runs April 6, 2026 ─────────────────────────

SITES = [
    {
        "domain": "stackoverflow.com",
        "title": "Why is sorted array faster?",
        "total_nodes": 3_653,
        "queries": [
            {
                "label": "Q1: COLD",
                "goal": "why sorted array faster than unsorted branch prediction CPU pipeline",
                "top_result": "openGraph.url: stackoverflow.com/questions/11227809/...",
                "top_role": "data",
                "top_score": 1.448,
                "causal_boost": 0.0,
                "answer_rank": None,
                "answer_text": "branch prediction penalty is removed, as explained by Mysticial",
                "answer_score": 1.214,
                "answer_id": 1153,
                "note": "OG metadata dominates top-1",
            },
            {
                "label": "Q2: +FEEDBACK",
                "goal": "performance difference sorting data processor branch misprediction",
                "top_result": "branch prediction penalty is removed, as explained by Mysticial",
                "top_role": "text",
                "top_score": 1.723,
                "causal_boost": 0.078,
                "answer_rank": 1,
                "answer_text": "branch prediction penalty is removed, as explained by Mysticial",
                "answer_score": 1.723,
                "answer_id": 1153,
                "note": "Answer jumps to #1 · metadata eliminated · causal_boost: 0.078",
            },
            {
                "label": "Q3: GENERALIZATION",
                "goal": "railroad junction analogy instruction prefetch speculation wrong guess",
                "top_result": "You are a victim of branch prediction fail. Consider a railroad junction...",
                "top_role": "text",
                "top_score": 1.432,
                "causal_boost": 0.0,
                "answer_rank": 1,
                "answer_text": "You are a victim of branch prediction fail. Consider a railroad junction...",
                "answer_score": 1.432,
                "answer_id": 421,
                "note": "Completely different terms → still finds the answer (35K upvotes)",
            },
        ],
    },
    {
        "domain": "en.wikipedia.org/wiki/Dark_matter",
        "title": "What is the universe made of?",
        "total_nodes": 6_100,
        "queries": [
            {
                "label": "Q1: COLD",
                "goal": "what percentage of universe is dark matter dark energy composition",
                "top_result": "5% ordinary matter, 26.8% dark matter, 68.2% dark energy",
                "top_role": "text",
                "top_score": 1.777,
                "causal_boost": 0.0,
                "answer_rank": 1,
                "answer_text": "5% ordinary matter, 26.8% dark matter, 68.2% dark energy",
                "answer_score": 1.777,
                "answer_id": 1185,
                "note": "Good cold start — strong BM25 keyword overlap",
            },
            {
                "label": "Q2: +FEEDBACK",
                "goal": "cosmological composition ordinary matter exotic matter Lambda CDM model",
                "top_result": "5% ordinary matter, 26.8% dark matter, 68.2% dark energy",
                "top_role": "text",
                "top_score": 2.233,
                "causal_boost": 0.078,
                "answer_rank": 1,
                "answer_text": "5% ordinary matter, 26.8% dark matter, 68.2% dark energy",
                "answer_score": 2.233,
                "answer_id": 1185,
                "note": "Score +26% with causal_boost: 0.078 — same node, stronger signal",
            },
            {
                "label": "Q3: GENERALIZATION",
                "goal": "how much cosmos is invisible non-baryonic substance ratio observable",
                "top_result": "5.2 Non-baryonic matter (section heading)",
                "top_role": "link",
                "top_score": 2.096,
                "causal_boost": 0.0,
                "answer_rank": None,
                "answer_text": "Non-baryonic matter (relevant section, different angle)",
                "answer_score": 2.096,
                "answer_id": 278,
                "note": "Completely new vocabulary — finds relevant section of 6,100 nodes",
            },
        ],
    },
    {
        "domain": "bbc.com/news",
        "title": "Latest breaking news",
        "total_nodes": 658,
        "queries": [
            {
                "label": "Q1: COLD",
                "goal": "latest news headlines today breaking stories BBC world",
                "top_result": "openGraph.title: BBC News - Breaking news, video and the latest...",
                "top_role": "data",
                "top_score": 2.089,
                "causal_boost": 0.0,
                "answer_rank": None,
                "answer_text": "Artemis II crew see first glimpse of far side of Moon",
                "answer_score": 0.552,
                "answer_id": 742,
                "note": "OG metadata at #1 — real article buried at 0.552",
            },
            {
                "label": "Q2: +FEEDBACK",
                "goal": "current world news stories important events happening now",
                "top_result": "openGraph.title: BBC News - Breaking news... (still #1)",
                "top_role": "data",
                "top_score": 2.088,
                "causal_boost": 0.0,
                "answer_rank": 3,
                "answer_text": "Artemis II toilet trouble on their way towards the Moon",
                "answer_score": 0.830,
                "answer_id": 633,
                "note": "Article rises 0.552 → 0.830 (+50%) — approaching metadata",
            },
            {
                "label": "Q3: +FEEDBACK",
                "goal": "major headlines notable happenings across the globe right now",
                "top_result": "Hungary alleges plot to blow up gas pipeline ahead of election",
                "top_role": "heading",
                "top_score": 1.652,
                "causal_boost": 0.0,
                "answer_rank": 1,
                "answer_text": "Hungary alleges plot to blow up gas pipeline",
                "answer_score": 1.652,
                "answer_id": 876,
                "note": "Real news article at #1 — metadata gone from top results",
            },
        ],
    },
]

# ── Drawing ──────────────────────────────────────────────────────────────

def draw_segs(draw, x, y, segs):
    for text, color, font in segs:
        draw.text((x, y), text, fill=color, font=font)
        bbox = font.getbbox(text)
        x += bbox[2] - bbox[0]
    return x

class Screen:
    def __init__(self):
        self.ops = []

    def text(self, x, y, segs):
        self.ops.append(lambda d, x=x, y=y, s=segs: draw_segs(d, x, y, s))

    def rect(self, x, y, w, h, color):
        self.ops.append(lambda d, x=x, y=y, w=w, h=h, c=color:
                        d.rectangle([x, y, x+max(2,w), y+h], fill=c))

    def render(self):
        img = Image.new("RGB", (W, H), BG)
        draw = ImageDraw.Draw(img)
        for op in self.ops:
            op(draw)
        return np.array(img)

    def copy(self):
        s = Screen()
        s.ops = list(self.ops)
        return s

# ── Build site screen ────────────────────────────────────────────────────

def build_site(site, site_idx):
    frames = []
    scr = Screen()
    y = PAD

    def add(dur, s=None):
        f = (s or scr).render()
        for _ in range(max(1, int(dur * FPS))):
            frames.append(f)

    # Header
    scr.text(PAD, y, [
        (f"  {site_idx}/3", DIM, FB),
        ("  ── ", DIM, F),
        (site["domain"], CYAN, F_BIG),
        (f"  ({site['total_nodes']:,} nodes)", DIM, F),
    ])
    y += LH + 4
    scr.text(PAD, y, [(f"  {site['title']}", WHITE, FB)])
    y += LH + 10
    add(0.4)

    # Each query iteration
    for qi, q in enumerate(site["queries"]):
        # Query label + goal
        label_color = DIM if qi == 0 else (YELLOW if qi == 1 else GREEN)
        scr.text(PAD, y, [
            ("  ", FG, F),
            (f"{q['label']}", label_color, FB),
        ])
        y += LH
        goal_short = q["goal"]
        if len(goal_short) > 70:
            goal_short = goal_short[:67] + "..."
        scr.text(PAD, y, [("    ", FG, F), (f'"{goal_short}"', DIM, F_SM)])
        y += LH + 2
        add(0.3)

        # Top result
        result_short = q["top_result"]
        if len(result_short) > 62:
            result_short = result_short[:59] + "..."

        is_metadata = q["top_role"] == "data"
        result_color = RED if is_metadata else GREEN
        role_label = "METADATA" if is_metadata else q["top_role"]

        scr.text(PAD + 20, y, [
            ("→ #1 ", DIM, F_SM),
            (f"[{q['top_score']:.3f}]", YELLOW, F_SM),
            (f" {role_label}: ", result_color, F_SMB),
        ])
        y += LH - 4
        scr.text(PAD + 30, y, [
            (f'"{result_short}"', result_color, F_SM),
        ])
        y += LH

        # Causal boost
        if q["causal_boost"] > 0.001:
            scr.text(PAD + 20, y, [
                ("  causal_boost: ", DIM, F_SM),
                (f"+{q['causal_boost']:.3f}", GREEN, FB),
                ("  ← learned from previous feedback", DIM, F_SM),
            ])
            y += LH - 2

        # Score bar
        bar_w = int(q["top_score"] / 2.5 * 400)
        bar_color = BAR_COLD if qi == 0 else BAR_WARM
        scr.rect(PAD + 20, y + 2, bar_w, 12, bar_color)
        y += 18

        # Note
        scr.text(PAD + 20, y, [(q["note"], DIM, F_SM)])
        y += LH + 2
        add(0.5)

        # Feedback arrow (between iterations, not after last)
        if qi < len(site["queries"]) - 1:
            scr.text(PAD + 20, y, [
                ("  ↓ ", YELLOW, FB),
                ("crfr_feedback(", DIM, F_SM),
                (f"node_ids=[{q['answer_id']}]", YELLOW, F_SM),
                (")", DIM, F_SM),
            ])
            y += LH + 4
            add(0.3)

    # Summary for this site
    y += 6
    scr.text(PAD, y, [("  " + "─" * 78, ACCENT, F_SM)])
    y += LH

    q1 = site["queries"][0]
    q2 = site["queries"][1]
    q3 = site["queries"][2]

    if q1["answer_rank"] is None and q3["answer_rank"] == 1:
        scr.text(PAD, y, [
            ("  Answer: ", DIM, FB),
            ("not in top-5", RED, F),
            (" → ", DIM, F),
            ("#1", GREEN, FB),
            ("  (metadata eliminated)", GREEN, F),
        ])
    elif q1["answer_rank"] == 1:
        delta = ((q2["top_score"] - q1["top_score"]) / q1["top_score"]) * 100
        scr.text(PAD, y, [
            ("  Score: ", DIM, FB),
            (f"{q1['top_score']:.3f}", FG, F),
            (" → ", DIM, F),
            (f"{q2['top_score']:.3f}", GREEN, FB),
            (f"  (+{delta:.0f}%)", GREEN, F),
            ("  with causal memory", DIM, F),
        ])

    y += LH
    add(0.7)

    return frames

# ── Title + Closing ──────────────────────────────────────────────────────

def build_title():
    scr = Screen()
    cy = H // 2 - 90
    scr.text(PAD + 40, cy, [("IT LEARNS", GREEN, F_XXL)])
    cy += 56
    scr.text(PAD + 40, cy, [("CRFR Causal Feedback Demo", DIM, _font(20))])
    cy += 44
    scr.text(PAD + 40, cy, [
        ("3 websites", WHITE, FB),
        ("  ·  ", DIM, F),
        ("3 queries each", WHITE, FB),
        ("  ·  ", DIM, F),
        ("live data April 2026", WHITE, FB),
    ])
    cy += 36
    scr.text(PAD + 40, cy, [("Query → Feedback → Query → Feedback → Query", DIM, F)])
    cy += 28
    scr.text(PAD + 40, cy, [("Watch the system learn from interaction.", DIM, F_SM)])
    cy += 28
    scr.text(PAD + 40, cy, [("No training data. No gradient descent. No GPU.", DIM, F_SM)])
    f = scr.render()
    return [f] * int(2.0 * FPS)

def build_closing():
    scr = Screen()
    cy = 60

    scr.text(PAD + 40, cy, [("What happened:", DIM, _font(20))])
    cy += 44

    scr.text(PAD + 40, cy, [
        ("Stack Overflow", CYAN, FB),
        (":  metadata at #1 → ", DIM, F),
        ("real answer at #1", GREEN, FB),
    ])
    cy += 32
    scr.text(PAD + 40, cy, [
        ("Wikipedia", CYAN, FB),
        (":       score 1.777 → ", DIM, F),
        ("2.233 (+26%)", GREEN, FB),
        (" with causal memory", DIM, F),
    ])
    cy += 32
    scr.text(PAD + 40, cy, [
        ("BBC News", CYAN, FB),
        (":        metadata → ", DIM, F),
        ("real news at #1", GREEN, FB),
    ])
    cy += 50

    scr.text(PAD + 40, cy, [("─" * 55, ACCENT, F)])
    cy += 36

    scr.text(PAD + 40, cy, [("How:", DIM, _font(20))])
    cy += 36
    scr.text(PAD + 60, cy, [
        ("1. ", DIM, F),
        ("Causal memory", WHITE, FB),
        (" — nodes that answered correctly get boosted", DIM, F),
    ])
    cy += 30
    scr.text(PAD + 60, cy, [
        ("2. ", DIM, F),
        ("Suppression", WHITE, FB),
        (" — metadata nodes that fail get penalized", DIM, F),
    ])
    cy += 30
    scr.text(PAD + 60, cy, [
        ("3. ", DIM, F),
        ("Concept memory", WHITE, FB),
        (" — goal-tokens generalize to new phrasings", DIM, F),
    ])
    cy += 50

    scr.text(PAD + 40, cy, [
        ("No training data", GREEN, FB),
        ("  ·  ", DIM, F),
        ("No gradient descent", GREEN, FB),
        ("  ·  ", DIM, F),
        ("No GPU", GREEN, FB),
    ])
    cy += 30
    scr.text(PAD + 40, cy, [("pure Rust  ·  Bayesian Beta distributions  ·  hypervector algebra", DIM, F_SM)])

    f = scr.render()
    return [f] * int(3.5 * FPS)

# ── Transitions ──────────────────────────────────────────────────────────

def fade(a, b, dur=0.25):
    n = max(1, int(dur * FPS))
    af, bf = a.astype(np.float32), b.astype(np.float32)
    return [((1 - i/(n-1)) * af + (i/(n-1)) * bf).astype(np.uint8) for i in range(n)]

# ── Main ─────────────────────────────────────────────────────────────────

def main():
    print("Building CRFR 'It Learns' demo...")
    all_frames = []

    title = build_title()
    all_frames.extend(title)
    print(f"  Title: {len(title)/FPS:.1f}s")

    for i, site in enumerate(SITES):
        sf = build_site(site, i + 1)
        if all_frames:
            all_frames.extend(fade(all_frames[-1], sf[0]))
        all_frames.extend(sf)
        print(f"  Site {i+1} ({site['domain']}): {len(sf)/FPS:.1f}s")

    closing = build_closing()
    all_frames.extend(fade(all_frames[-1], closing[0], 0.3))
    all_frames.extend(closing)
    print(f"  Closing: {len(closing)/FPS:.1f}s")

    total_s = len(all_frames) / FPS
    print(f"\n  Total: {len(all_frames)} frames = {total_s:.1f}s at {FPS}fps")

    print(f"\nEncoding {OUTPUT}...")
    iio.imwrite(str(OUTPUT), all_frames, fps=FPS, codec="libx264", plugin="pyav")
    mb = OUTPUT.stat().st_size / (1024 * 1024)
    print(f"  Done — {mb:.1f} MB")

    # Previews
    Image.fromarray(all_frames[len(title) + 10]).save(OUTPUT.parent / "preview_learn_site1.png")
    Image.fromarray(all_frames[-1]).save(OUTPUT.parent / "preview_learn_closing.png")
    print("  Saved previews")


if __name__ == "__main__":
    main()
