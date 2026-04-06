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

# ── DOM Grid ─────────────────────────────────────────────────────────────

CELL = 5          # pixel size per node cell
GAP  = 1          # gap between cells
COLS = 130        # nodes per row
ROWS = math.ceil(TOTAL_NODES / COLS)  # ~96 rows
GRID_W = COLS * (CELL + GAP)
GRID_H = ROWS * (CELL + GAP)
GRID_X = (W - GRID_W) // 2
GRID_Y = 260

# Pre-compute hit positions in grid coords
HIT_CELLS = set()
for nid in HIT_POSITIONS:
    idx = min(nid, TOTAL_NODES - 1)
    HIT_CELLS.add(idx)

def draw_dom_grid(draw, scan_row=-1, found_so_far=None):
    """Draw the full DOM grid with scan line and highlights."""
    if found_so_far is None:
        found_so_far = set()

    for idx in range(TOTAL_NODES):
        row = idx // COLS
        col = idx % COLS
        x = GRID_X + col * (CELL + GAP)
        y = GRID_Y + row * (CELL + GAP)

        if idx in found_so_far:
            # Hit node — bright green with glow
            draw.rectangle([x-1, y-1, x+CELL+1, y+CELL+1], fill=NODE_HIT_GLOW)
            draw.rectangle([x, y, x+CELL, y+CELL], fill=NODE_HIT)
        elif row == scan_row:
            # Scan line
            draw.rectangle([x, y, x+CELL, y+CELL], fill=NODE_SCAN)
        elif row < scan_row:
            # Already scanned
            draw.rectangle([x, y, x+CELL, y+CELL], fill=ACCENT)
        else:
            # Not yet scanned
            draw.rectangle([x, y, x+CELL, y+CELL], fill=NODE_DIM)

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
    # ════════════════════════════════════════════════════════════════════

    found = set()
    # Sweep through rows
    total_sweep_frames = int(6.0 * FPS)  # 6 seconds of sweep
    rows_per_frame = max(1, ROWS / total_sweep_frames)

    for fi in range(total_sweep_frames):
        scan_row = int(fi * rows_per_frame)
        if scan_row >= ROWS:
            scan_row = ROWS - 1

        # Check if any hit nodes are in scanned rows
        for hit_idx in HIT_CELLS:
            hit_row = hit_idx // COLS
            if hit_row <= scan_row:
                found.add(hit_idx)

        img = Image.new("RGB", (W, H), BG)
        draw = ImageDraw.Draw(img)

        # Header
        draw.text((PAD, 20), "en.wikipedia.org/wiki/COVID-19_vaccine", fill=CYAN, font=F_MD)

        # Stats line
        pct_scanned = min(100, (scan_row + 1) / ROWS * 100)
        draw.text((PAD, 55), f"Scanning {TOTAL_NODES:,} DOM nodes...", fill=DIM2, font=F)
        draw.text((PAD + 400, 55),
                  f"{pct_scanned:.0f}%  ·  {len(found)}/4 found", fill=YELLOW, font=FB)

        # Progress bar
        bar_w = int((W - PAD * 2) * pct_scanned / 100)
        draw.rectangle([PAD, 90, PAD + W - PAD * 2, 94], fill=ACCENT)
        draw.rectangle([PAD, 90, PAD + bar_w, 94], fill=CYAN)

        # Legend
        draw.text((PAD, 105), "■", fill=NODE_DIM, font=F_SM)
        draw.text((PAD + 16, 105), " unscanned", fill=DIM, font=F_SM)
        draw.text((PAD + 130, 105), "■", fill=ACCENT, font=F_SM)
        draw.text((PAD + 146, 105), " scanned", fill=DIM, font=F_SM)
        draw.text((PAD + 250, 105), "■", fill=NODE_HIT, font=F_SM)
        draw.text((PAD + 266, 105), " ANSWER FOUND", fill=GREEN, font=F_SM)

        # Draw grid
        # Shift grid up a bit
        old_grid_y = GRID_Y
        # Use dynamic offset for grid
        draw_dom_grid_offset(draw, scan_row, found, y_offset=140)

        # Found nodes panel (right side, appears as nodes are found)
        panel_x = W - 480
        panel_y = 140
        if found:
            draw.text((panel_x, panel_y), "Located:", fill=GREEN, font=FB)
            panel_y += 30

            for i, (score, role, text) in enumerate(RESULTS):
                nid = HIT_POSITIONS[i]
                if nid not in found:
                    continue
                # Score badge
                draw.text((panel_x, panel_y), f"[{score:.3f}]", fill=YELLOW, font=F_SM)
                draw.text((panel_x + 70, panel_y), f" {role}", fill=CYAN, font=F_SM)
                panel_y += 20
                # Text (truncated)
                short = text[:50] + "..." if len(text) > 50 else text
                draw.text((panel_x + 10, panel_y), f'"{short}"', fill=WHITE, font=F_SM)
                panel_y += 24

        frames.append(np.array(img))

    # Hold final grid for a moment
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


def draw_dom_grid_offset(draw, scan_row, found, y_offset=140):
    """Draw DOM grid with custom y offset."""
    for idx in range(TOTAL_NODES):
        row = idx // COLS
        col = idx % COLS
        x = GRID_X + col * (CELL + GAP)
        y = y_offset + row * (CELL + GAP)

        if y > H - 20:  # clip
            break

        if idx in found:
            draw.rectangle([x-1, y-1, x+CELL+1, y+CELL+1], fill=NODE_HIT_GLOW)
            draw.rectangle([x, y, x+CELL, y+CELL], fill=NODE_HIT)
        elif row == scan_row:
            draw.rectangle([x, y, x+CELL, y+CELL], fill=NODE_SCAN)
        elif row < scan_row:
            draw.rectangle([x, y, x+CELL, y+CELL], fill=ACCENT)
        else:
            draw.rectangle([x, y, x+CELL, y+CELL], fill=NODE_DIM)

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
