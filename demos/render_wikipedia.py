#!/usr/bin/env python3
"""
CRFR Flagship Demo: Wikipedia COVID-19 Vaccine
"2.7 million characters. 4 nodes. 1.4 seconds."

Extended demo with DOM sweep animation, pitch, and dramatic payoff.
All data from live MCP parse_crfr on April 6, 2026.

Usage: python3 demos/render_wikipedia.py
Output: demos/crfr-wikipedia.mp4
"""
import math
import random
from pathlib import Path

import imageio.v3 as iio
import numpy as np
from PIL import Image, ImageDraw, ImageFont

# ── Config ───────────────────────────────────────────────────────────────

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
BAR_IN  = (58, 86, 145)
BAR_OUT = (57, 211, 83)
ACCENT  = (40, 46, 56)
NODE_DIM = (28, 33, 42)
NODE_SCAN = (40, 50, 65)
NODE_HIT = (57, 211, 83)
NODE_HIT_GLOW = (80, 230, 110)

PAD = 50
OUTPUT = Path(__file__).parent / "crfr-wikipedia.mp4"

def _font(size, bold=False):
    tag = "Bold" if bold else "Regular"
    for p in [
        f"/usr/share/fonts/truetype/dejavu/DejaVuSansMono-{tag}.ttf",
        f"/usr/share/fonts/truetype/liberation/LiberationMono-{tag}.ttf",
    ]:
        if Path(p).exists():
            return ImageFont.truetype(p, size)
    return ImageFont.load_default()

F    = _font(19)
FB   = _font(19, bold=True)
F_SM = _font(15)
F_SMB = _font(15, bold=True)
F_MD = _font(22, bold=True)
F_LG = _font(28, bold=True)
F_XL = _font(36, bold=True)
F_XXL = _font(48, bold=True)
F_HUGE = _font(72, bold=True)
F_NUM = _font(56, bold=True)

# ── Real data from MCP parse_crfr, April 6, 2026 ────────────────────────

TOTAL_NODES = 12_434
RAW_CHARS = 2_708_245
TOKENS_IN = 677_061
TOKENS_OUT = 130
PARSE_MS = 1_451
COST_IN = 1.69
COST_OUT = 0.0003

# Hit node IDs (positions in the 12,434 node array)
HIT_POSITIONS = [308, 1959, 3251, 3321]  # actual node IDs from CRFR

RESULTS = [
    (1.462, "link",  "8 Adverse effects"),
    (1.432, "text",  "The risks of serious illness from COVID-19 are far higher than the risk posed by the vaccines..."),
    (1.318, "text",  "mRNA vaccines were the first authorised in UK, US and EU. Pfizer-BioNTech, Moderna."),
    (1.248, "text",  "up to 20% report disruptive side effects after 2nd mRNA dose. Fever, fatigue, headache."),
]

# ── Helpers ──────────────────────────────────────────────────────────────

def tx(draw, x, y, text, color, font):
    draw.text((x, y), text, fill=color, font=font)
    bbox = font.getbbox(text)
    return x + bbox[2] - bbox[0]

def center_text(draw, y, text, color, font):
    bbox = font.getbbox(text)
    tw = bbox[2] - bbox[0]
    x = (W - tw) // 2
    draw.text((x, y), text, fill=color, font=font)

def add_frames(frames, img_or_arr, dur):
    if isinstance(img_or_arr, Image.Image):
        arr = np.array(img_or_arr)
    else:
        arr = img_or_arr
    for _ in range(max(1, int(dur * FPS))):
        frames.append(arr)

def fade_to(frames, target_arr, dur=0.3):
    if not frames:
        return
    src = frames[-1].astype(np.float32)
    dst = target_arr.astype(np.float32)
    n = max(1, int(dur * FPS))
    for i in range(n):
        t = i / max(1, n - 1)
        frames.append(((1 - t) * src + t * dst).astype(np.uint8))

# ── DOM Tree Lines (scrolling representation) ────────────────────────────
# Represent 12,434 nodes as labeled rows that scroll past a viewport.
# Hit nodes glow green. A cyan scan-line sweeps down.

# Build a representative DOM tree listing (abbreviated — shows structure)
DOM_LINES = []
_rng = random.Random(42)

# Generate ~200 representative lines covering the 12,434 node range
# Each line = (node_id_approx, depth, label, is_hit)
_sections = [
    (0,    0, "html", False),
    (1,    1, "head", False),
    (2,    2, "meta × 48", False),
    (3,    2, "link × 22 (stylesheets)", False),
    (10,   1, "body", False),
    (15,   2, "nav#mw-navigation", False),
    (20,   3, "div.vector-menu × 8", False),
    (30,   3, "a.mw-wiki-logo", False),
    (40,   3, "div#p-search", False),
    (60,   2, "div#content", False),
    (70,   3, "h1: COVID-19 vaccine", False),
    (80,   3, "div#mw-content-text", False),
    (90,   4, "div.mw-parser-output", False),
    (100,  5, "p: A COVID-19 vaccine is a vaccine intended to...", False),
    (120,  5, "div.toc (table of contents)", False),
    (140,  6, "li × 42 (TOC entries)", False),
    (180,  5, "h2: History", False),
    (200,  5, "p × 12 (history paragraphs)", False),
    (250,  5, "h2: Types", False),
    (270,  5, "p × 18 (vaccine types)", False),
    (308,  5, "a: 8 Adverse effects", True),         # ← HIT
    (320,  5, "h2: Development", False),
    (380,  5, "p × 24 (development)", False),
    (450,  5, "table.wikitable (clinical trials)", False),
    (500,  6, "tr × 35", False),
    (600,  5, "h2: Deployment", False),
    (650,  5, "p × 20 (deployment)", False),
    (750,  5, "div.navbox × 4", False),
    (850,  5, "h3: Authorization", False),
    (900,  5, "p × 15", False),
    (1000, 5, "h2: Society and culture", False),
    (1100, 5, "p × 22", False),
    (1200, 5, "table × 3 (vaccination rates)", False),
    (1400, 5, "h2: Research", False),
    (1500, 5, "p × 30", False),
    (1700, 5, "h2: COVID-19 vaccine types", False),
    (1800, 5, "p × 18", False),
    (1959, 5, "p: mRNA vaccines were the first authorised in UK, US, EU. Pfizer-BioNTech, Moderna.", True),  # ← HIT
    (2000, 5, "h3: Viral vector", False),
    (2100, 5, "p × 14", False),
    (2300, 5, "h3: Inactivated", False),
    (2400, 5, "p × 12", False),
    (2600, 5, "h2: Side effects", False),
    (2800, 5, "p × 28", False),
    (3000, 5, "table.wikitable (adverse events)", False),
    (3100, 6, "tr × 45", False),
    (3251, 5, "p: The risks of serious illness from COVID-19 are far higher than the risk posed by vaccines...", True),  # ← HIT
    (3321, 5, "p: up to 20% report disruptive side effects after 2nd mRNA dose. Fever, fatigue, headache.", True),  # ← HIT
    (3400, 5, "h3: Serious adverse events", False),
    (3500, 5, "p × 16", False),
    (3700, 5, "h2: Misconceptions", False),
    (3800, 5, "p × 22", False),
    (4000, 5, "h2: Economics", False),
    (4200, 5, "p × 18", False),
    (4500, 5, "div.reflist", False),
    (4600, 6, "li.reference × 340", False),
    (5500, 5, "div.navbox × 8", False),
    (6000, 5, "div.catlinks", False),
    (6100, 4, "div#mw-navigation-footer", False),
    (6200, 3, "footer", False),
    (6300, 4, "ul × 6 (footer links)", False),
]

for nid, depth, label, is_hit in _sections:
    DOM_LINES.append((nid, depth, label, is_hit))

# Fill gaps with generic nodes
_filled = []
_prev_id = 0
for nid, depth, label, is_hit in DOM_LINES:
    # Add filler lines between sections
    gap = nid - _prev_id
    if gap > 50:
        n_fill = min(gap // 40, 6)
        for k in range(n_fill):
            fid = _prev_id + (k + 1) * (gap // (n_fill + 1))
            ftag = _rng.choice(["p", "div", "span", "a", "li", "td", "th", "img", "sup", "cite"])
            flabel = f"{ftag} (node {fid})"
            _filled.append((fid, min(depth + 1, 7), flabel, False))
    _filled.append((nid, depth, label, is_hit))
    _prev_id = nid

DOM_LINES = _filled
TOTAL_DOM_LINES = len(DOM_LINES)

# ── Build Frames ─────────────────────────────────────────────────────────

def build():
    frames = []
    rng = random.Random(42)

    # ════════════════════════════════════════════════════════════════════
    # ACT 1: THE PITCH (0s - 4s)
    # ════════════════════════════════════════════════════════════════════

    # Frame 1: Setup
    img = Image.new("RGB", (W, H), BG)
    draw = ImageDraw.Draw(img)
    center_text(draw, 320, "LLMs need web data.", DIM2, F_LG)
    center_text(draw, 370, "Web pages are massive.", DIM2, F_LG)
    add_frames(frames, img, 2.0)

    # Frame 2: The question
    img2 = Image.new("RGB", (W, H), BG)
    draw2 = ImageDraw.Draw(img2)
    center_text(draw2, 290, "What if you could reduce a web page", WHITE, F_LG)
    center_text(draw2, 340, "to the 0.01% that actually answers the question?", WHITE, F_LG)
    fade_to(frames, np.array(img2), 0.4)
    add_frames(frames, img2, 2.2)

    # ════════════════════════════════════════════════════════════════════
    # ACT 2: THE CHALLENGE (4s - 7s)
    # ════════════════════════════════════════════════════════════════════

    img3 = Image.new("RGB", (W, H), BG)
    draw3 = ImageDraw.Draw(img3)
    draw3.text((PAD, 40), "en.wikipedia.org/wiki/COVID-19_vaccine", fill=CYAN, font=F_LG)
    draw3.text((PAD, 85), '"What are the side effects and efficacy of COVID vaccines?"', fill=DIM2, font=F)

    # Big scary numbers
    draw3.text((PAD, 150), "2,708,245", fill=RED, font=F_HUGE)
    draw3.text((PAD + 520, 175), "characters of raw HTML", fill=DIM2, font=F_MD)

    draw3.text((PAD, 240), "677,061 tokens", fill=RED, font=F_XL)
    draw3.text((PAD + 420, 248), "·  $1.69 per read  ·  exceeds GPT-3.5 context", fill=DIM2, font=F)

    draw3.text((PAD, 310), "12,434", fill=YELLOW, font=F_XL)
    draw3.text((PAD + 240, 318), "DOM nodes", fill=DIM2, font=F)

    draw3.text((PAD, 380), "The answer is in 4 of them.", fill=WHITE, font=F_MD)

    fade_to(frames, np.array(img3), 0.4)
    add_frames(frames, img3, 3.0)

    # ════════════════════════════════════════════════════════════════════
    # ACT 3: THE DOM SWEEP (7s - 14s)
    # Scrolling DOM tree with cyan scan-line and green hit highlights
    # ════════════════════════════════════════════════════════════════════

    TREE_TOP = 110      # viewport top
    TREE_BOT = H - 30   # viewport bottom
    LINE_H_TREE = 22    # pixels per tree line
    VISIBLE = (TREE_BOT - TREE_TOP) // LINE_H_TREE  # ~34 visible lines
    INDENT_PX = 18      # pixels per indent level

    found_results = []   # accumulated found results
    total_sweep_frames = int(6.0 * FPS)

    for fi in range(total_sweep_frames):
        # Current scan position in the DOM_LINES array
        t = fi / total_sweep_frames
        scan_idx = int(t * TOTAL_DOM_LINES)
        # Corresponding node ID for the counter
        if scan_idx < TOTAL_DOM_LINES:
            current_node_id = DOM_LINES[scan_idx][0]
        else:
            current_node_id = TOTAL_NODES

        # Check for new hits
        for li in range(min(scan_idx + 1, TOTAL_DOM_LINES)):
            nid, depth, label, is_hit = DOM_LINES[li]
            if is_hit and nid not in [r[0] for r in found_results]:
                # Find matching result
                for hi, hid in enumerate(HIT_POSITIONS):
                    if hid == nid:
                        found_results.append((nid, RESULTS[hi]))
                        break

        img = Image.new("RGB", (W, H), BG)
        draw = ImageDraw.Draw(img)

        # Header
        draw.text((PAD, 15), "en.wikipedia.org/wiki/COVID-19_vaccine", fill=CYAN, font=F_MD)

        # Progress counter
        pct = min(100, current_node_id / TOTAL_NODES * 100)
        draw.text((PAD, 52), f"Scanning DOM...", fill=DIM2, font=F)
        node_str = f"{current_node_id:,}/{TOTAL_NODES:,}"
        draw.text((PAD + 230, 52), node_str, fill=YELLOW, font=FB)
        draw.text((PAD + 230 + len(node_str) * 11 + 10, 52),
                  f"  {len(found_results)}/4 found", fill=GREEN if found_results else DIM2, font=FB)

        # Progress bar
        bar_full = W - PAD * 2
        draw.rectangle([PAD, 84, PAD + bar_full, 90], fill=ACCENT)
        draw.rectangle([PAD, 84, PAD + int(bar_full * pct / 100), 90], fill=CYAN)

        # Scrolling tree viewport
        # Center the scan line in the viewport
        scan_line_in_view = VISIBLE // 3  # scan line at 1/3 from top
        start_line = max(0, scan_idx - scan_line_in_view)
        end_line = min(TOTAL_DOM_LINES, start_line + VISIBLE)

        for vi, li in enumerate(range(start_line, end_line)):
            nid, depth, label, is_hit = DOM_LINES[li]
            y = TREE_TOP + vi * LINE_H_TREE
            x = PAD + depth * INDENT_PX

            is_scan_line = (li == scan_idx)
            is_scanned = (li < scan_idx)

            if is_hit and li <= scan_idx:
                # HIT — green highlight bar
                draw.rectangle([PAD - 4, y - 2, W // 2 + 60, y + LINE_H_TREE - 4], fill=(20, 60, 30))
                draw.rectangle([PAD - 6, y - 2, PAD - 2, y + LINE_H_TREE - 4], fill=NODE_HIT)
                # Tree connector
                if depth > 0:
                    draw.text((x - INDENT_PX, y), "├─", fill=GREEN, font=F_SM)
                draw.text((x, y), label[:65], fill=NODE_HIT_GLOW, font=F_SMB)
                draw.text((x, y), label[:65], fill=GREEN, font=F_SMB)  # double for brightness
            elif is_scan_line:
                # Scan line — cyan highlight
                draw.rectangle([PAD - 4, y - 1, W // 2 + 60, y + LINE_H_TREE - 5], fill=(20, 35, 55))
                if depth > 0:
                    draw.text((x - INDENT_PX, y), "├─", fill=CYAN, font=F_SM)
                draw.text((x, y), label[:65], fill=CYAN, font=F_SM)
            elif is_scanned:
                # Already passed — dim
                if depth > 0:
                    draw.text((x - INDENT_PX, y), "│ ", fill=(35, 40, 50), font=F_SM)
                draw.text((x, y), label[:65], fill=DIM, font=F_SM)
            else:
                # Not yet reached
                if depth > 0:
                    draw.text((x - INDENT_PX, y), "│ ", fill=(30, 35, 42), font=F_SM)
                draw.text((x, y), label[:65], fill=(50, 55, 65), font=F_SM)

        # Right panel: found results
        panel_x = W // 2 + 100
        panel_y = TREE_TOP
        if found_results:
            draw.text((panel_x, panel_y), f"CRFR Located ({len(found_results)}/4):", fill=GREEN, font=FB)
            panel_y += 32

            for nid, (score, role, text) in found_results:
                draw.text((panel_x, panel_y), f"[{score:.3f}]", fill=YELLOW, font=F_SM)
                draw.text((panel_x + 65, panel_y), f" {role}", fill=CYAN, font=F_SM)
                panel_y += 20
                short = text[:42] + "..." if len(text) > 42 else text
                draw.text((panel_x + 8, panel_y), f'"{short}"', fill=WHITE, font=F_SM)
                panel_y += 28

        frames.append(np.array(img))

    # Hold final sweep frame
    add_frames(frames, frames[-1], 0.8)

    # ════════════════════════════════════════════════════════════════════
    # ACT 4: THE ANSWER (14s - 18s)
    # ════════════════════════════════════════════════════════════════════

    img4 = Image.new("RGB", (W, H), BG)
    draw4 = ImageDraw.Draw(img4)

    draw4.text((PAD, 40), "en.wikipedia.org/wiki/COVID-19_vaccine", fill=CYAN, font=F_MD)
    draw4.text((PAD, 75), "CRFR found 4 nodes in 1.4 seconds. No GPU. No embeddings.", fill=DIM2, font=F)

    y = 130
    for score, role, text in RESULTS:
        # Score bar
        bar_w = int(score / 1.5 * 300)
        draw4.rectangle([PAD, y + 6, PAD + bar_w, y + 20], fill=(35, 134, 54))

        draw4.text((PAD + bar_w + 10, y + 2), f"[{score:.3f}] {role}", fill=YELLOW, font=F_SM)
        y += 28

        # Full text
        lines = [text[i:i+85] for i in range(0, len(text), 85)]
        for line in lines:
            draw4.text((PAD + 20, y), f'"{line}"' if line == lines[0] else f' {line}', fill=WHITE, font=F)
            y += 24
        y += 12

    # Separator
    y += 10
    draw4.rectangle([PAD, y, W - PAD, y + 1], fill=ACCENT)
    y += 20

    # IN/OUT comparison
    draw4.text((PAD, y), "INPUT:", fill=DIM2, font=FB)
    draw4.text((PAD + 100, y), f"{RAW_CHARS:,} chars  ·  {TOKENS_IN:,} tokens  ·  ${COST_IN:.2f}", fill=FG, font=F)
    y += 30
    draw4.text((PAD, y), "OUTPUT:", fill=GREEN, font=FB)
    draw4.text((PAD + 100, y), f"521 chars  ·  {TOKENS_OUT} tokens  ·  ${COST_OUT}", fill=GREEN, font=F)
    y += 40

    # Big bars
    max_bw = W - PAD * 2 - 200
    draw4.rectangle([PAD, y, PAD + max_bw, y + 22], fill=BAR_IN)
    draw4.text((PAD + max_bw + 10, y + 2), f"{TOKENS_IN:,} tokens", fill=DIM2, font=F_SM)
    y += 28
    out_bw = max(4, int(max_bw * TOKENS_OUT / TOKENS_IN))
    draw4.rectangle([PAD, y, PAD + out_bw, y + 22], fill=BAR_OUT)
    draw4.text((PAD + out_bw + 10, y + 2), f"{TOKENS_OUT} tokens", fill=GREEN, font=F_SM)
    y += 40

    # Reduction
    draw4.text((PAD, y), "99.98%", fill=GREEN, font=F_XXL)
    draw4.text((PAD + 280, y + 14), "of tokens eliminated", fill=DIM2, font=F_MD)

    fade_to(frames, np.array(img4), 0.4)
    add_frames(frames, img4, 3.5)

    # ════════════════════════════════════════════════════════════════════
    # ACT 5: THE CLOSE (18s - 22s)
    # ════════════════════════════════════════════════════════════════════

    img5 = Image.new("RGB", (W, H), BG)
    draw5 = ImageDraw.Draw(img5)

    cy = 160
    center_text(draw5, cy, "CRFR", GREEN, _font(64, bold=True))
    cy += 80
    center_text(draw5, cy, "Causal Resonance Field Retrieval", DIM2, F_MD)
    cy += 50
    center_text(draw5, cy, "2.7M characters  →  521 characters  ·  1.4 seconds", WHITE, F_LG)
    cy += 50
    center_text(draw5, cy, "$1.69  →  $0.0003", GREEN, F_XL)
    cy += 60
    center_text(draw5, cy, "No GPU  ·  No embeddings  ·  1.8MB binary  ·  pure Rust", DIM2, F)
    cy += 35
    center_text(draw5, cy, "Learns from interaction  ·  sub-ms cached  ·  zero dependencies", DIM2, F_SM)

    fade_to(frames, np.array(img5), 0.4)
    add_frames(frames, img5, 3.5)

    return frames



# ── Main ─────────────────────────────────────────────────────────────────

def main():
    print("Building CRFR Wikipedia flagship demo...")
    frames = build()
    total_s = len(frames) / FPS
    print(f"  {len(frames)} frames = {total_s:.1f}s at {FPS}fps")

    print(f"\nEncoding {OUTPUT}...")
    iio.imwrite(str(OUTPUT), frames, fps=FPS, codec="libx264", plugin="pyav")
    mb = OUTPUT.stat().st_size / (1024 * 1024)
    print(f"  Done — {mb:.1f} MB")

    # Previews
    Image.fromarray(frames[int(2.0*FPS)]).save(OUTPUT.parent / "preview_wp_pitch.png")
    Image.fromarray(frames[int(5.5*FPS)]).save(OUTPUT.parent / "preview_wp_challenge.png")
    Image.fromarray(frames[int(11.0*FPS)]).save(OUTPUT.parent / "preview_wp_sweep.png")
    Image.fromarray(frames[int(16.0*FPS)]).save(OUTPUT.parent / "preview_wp_answer.png")
    Image.fromarray(frames[-1]).save(OUTPUT.parent / "preview_wp_close.png")
    print("  Saved preview PNGs")


if __name__ == "__main__":
    main()
