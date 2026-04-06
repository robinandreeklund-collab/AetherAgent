#!/usr/bin/env python3
"""
Render CRFR 10-questions demo to MP4.

Uses Pillow for frame rendering + imageio-ffmpeg for video encoding.
No external tools needed beyond pip packages.

Usage: python3 demos/render_mp4.py
Output: demos/crfr-10questions.mp4
"""

import math
import time
from pathlib import Path

import imageio.v3 as iio
import numpy as np
from PIL import Image, ImageDraw, ImageFont

# ── Config ───────────────────────────────────────────────────────────────

WIDTH = 1400
HEIGHT = 920
FPS = 30
BG = (13, 17, 23)        # #0d1117 GitHub dark
FG = (230, 237, 243)     # #e6edf3
DIM = (110, 118, 128)    # #6e7681
GREEN = (63, 185, 80)    # #3fb950
CYAN = (88, 166, 255)    # #58a6ff
YELLOW = (210, 153, 34)  # #d29922
WHITE = (230, 237, 243)
BAR_BLUE = (88, 166, 255)
BAR_GREEN = (57, 211, 83)

PADDING = 32
LINE_H = 28
FONT_SIZE = 20
FONT_SIZE_LARGE = 22

OUTPUT = Path(__file__).parent / "crfr-10questions.mp4"

# ── Font ─────────────────────────────────────────────────────────────────

def get_font(size=FONT_SIZE, bold=False):
    """Try to find a monospace font, fall back to default."""
    candidates = [
        "/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf" if bold
        else "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationMono-Bold.ttf" if bold
        else "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
    ]
    for path in candidates:
        if Path(path).exists():
            return ImageFont.truetype(path, size)
    return ImageFont.load_default()

FONT = get_font(FONT_SIZE)
FONT_BOLD = get_font(FONT_SIZE, bold=True)
FONT_LARGE = get_font(FONT_SIZE_LARGE, bold=True)

# ── Frame builder ────────────────────────────────────────────────────────

class FrameBuilder:
    def __init__(self):
        self.lines = []  # list of (y, elements) where elements = [(text, color, font)]

    def add_line(self, elements, y=None):
        """Add a line of styled text segments."""
        if y is None:
            y = PADDING + len(self.lines) * LINE_H
        self.lines.append((y, elements))

    def render(self):
        """Render current state to a numpy frame."""
        img = Image.new("RGB", (WIDTH, HEIGHT), BG)
        draw = ImageDraw.Draw(img)

        for y, elements in self.lines:
            x = PADDING
            for text, color, font in elements:
                draw.text((x, y), text, fill=color, font=font)
                bbox = font.getbbox(text)
                x += bbox[2] - bbox[0]

        return np.array(img)

    def render_with_bar(self, bar_y, bar1_width, bar2_width):
        """Render with colored bars."""
        img = Image.new("RGB", (WIDTH, HEIGHT), BG)
        draw = ImageDraw.Draw(img)

        for y, elements in self.lines:
            x = PADDING
            for text, color, font in elements:
                draw.text((x, y), text, fill=color, font=font)
                bbox = font.getbbox(text)
                x += bbox[2] - bbox[0]

        # Blue bar (raw tokens)
        if bar1_width > 0:
            draw.rectangle(
                [PADDING, bar_y, PADDING + bar1_width, bar_y + 18],
                fill=BAR_BLUE,
            )
            draw.text(
                (PADDING + bar1_width + 10, bar_y),
                "677k tokens  $4.00",
                fill=DIM, font=FONT,
            )

        # Green bar (CRFR tokens)
        if bar2_width > 0:
            draw.rectangle(
                [PADDING, bar_y + 24, PADDING + max(4, bar2_width), bar_y + 42],
                fill=BAR_GREEN,
            )
            draw.text(
                (PADDING + max(4, bar2_width) + 10, bar_y + 24),
                "869 tokens  $0.002",
                fill=DIM, font=FONT,
            )

        return np.array(img)


# ── Data ─────────────────────────────────────────────────────────────────

QUESTIONS = [
    ("Sweden's inflation rate?",     "riksbanken.se", "628,407",   "486",    "7ms",
     '"KPIF feb 2026: 1.7% | Styrränta 1.75%"'),
    ("Latest Python version?",       "python.org",    "335,934",   "270",    "19ms",
     '"3.14 bugfix (2025-10-07)"'),
    ("Population of Gothenburg?",    "Wikipedia SV",  "1,330,344", "388",    "127ms",
     '"674,529 (2023), Greater: 1,090,000"'),
    ("Latest AI/tech news?",         "BBC RSS",       "6,300",     "331",    "3ms",
     '"AI already in use in healthcare..."'),
    ("Stockholm transit pass cost?", "SL.se",         "(SPA)",     "detect", "0ms",
     '"spa_detected → fallback → 1,060 kr/mo"'),
    ("What is ibuprofen?",           "Wikipedia EN",  "1,044,595", "380",    "96ms",
     '"NSAID for pain, fever, inflammation"'),
    ("What is the EU AI Act?",       "Wikipedia EN",  "473,911",   "490",    "52ms",
     None),
    ("Current S&P 500 value?",       "Search",        "24,000",    "280",    "203ms",
     None),
    ("What is the universe made of?","Wikipedia EN",  "1,339,506", "380",    "146ms",
     '"5% matter, 27% dark matter, 68% dark energy"'),
    ("Champions League 2025-26?",    "Wikipedia EN",  "1,210,820", "310",    "97ms",
     None),
]

# ── Build frames ─────────────────────────────────────────────────────────

def build_video():
    frames = []

    def add_frames(frame, duration_s):
        """Add frame repeated for duration at FPS."""
        n = max(1, int(duration_s * FPS))
        for _ in range(n):
            frames.append(frame)

    fb = FrameBuilder()

    # ── Act 1: Command typing (0.0s - 2.0s) ──
    command = "$ aether extract-batch --goals '10 real questions' --live"
    # Blank screen
    add_frames(fb.render(), 0.4)

    # Type command character by character
    for i in range(1, len(command) + 1):
        fb_cmd = FrameBuilder()
        cursor = "█" if i < len(command) else ""
        fb_cmd.add_line([(command[:i] + cursor, GREEN, FONT_BOLD)])
        add_frames(fb_cmd.render(), 0.025)

    # Command complete, brief pause
    fb_typed = FrameBuilder()
    fb_typed.add_line([(command, GREEN, FONT_BOLD)])
    add_frames(fb_typed.render(), 0.5)

    # ── Act 2: Header + results stream (2.0s - 12.5s) ──
    fb = FrameBuilder()
    fb.add_line([(command, DIM, FONT)])
    fb.add_line([("", FG, FONT)])
    fb.add_line([("  CRFR", CYAN, FONT_BOLD), (" — Causal Resonance Field Retrieval", DIM, FONT)])
    fb.add_line([("  10 real questions · 10 live websites · April 2026", DIM, FONT)])
    fb.add_line([("", FG, FONT)])

    # Column headers
    header = f"  {'Question':<42} {'Source':<14} {'Raw':>10} {'CRFR':>7}  {'Time'}"
    fb.add_line([(header, DIM, FONT)])
    sep = "  " + "─" * 42 + " " + "─" * 14 + " " + "─" * 10 + " " + "─" * 7 + " " + "─" * 5
    fb.add_line([(sep, DIM, FONT)])
    add_frames(fb.render(), 0.3)

    # Stream each question
    for q, source, raw, crfr, latency, answer in QUESTIONS:
        # Main row
        line_elements = [
            (f"  {q:<42}", WHITE, FONT),
            (f" {source:<14}", CYAN, FONT),
            (f" {raw:>10}", FG, FONT),
            (f" {crfr:>7}", GREEN, FONT_BOLD),
            (f"  {latency}", YELLOW, FONT),
        ]
        fb.add_line(line_elements)

        if answer:
            fb.add_line([(f"    → {answer}", DIM, FONT)])
            add_frames(fb.render(), 0.7)
        else:
            add_frames(fb.render(), 0.5)

    # ── Act 3: Summary (12.5s - 15.5s) ──
    fb.add_line([("", FG, FONT)])
    fb.add_line([("  " + "─" * 74, DIM, FONT)])
    add_frames(fb.render(), 0.15)

    # Total line
    fb.add_line([
        ("  TOTAL: 6,405,817 chars in → 3,469 chars out", WHITE, FONT_BOLD),
        ("                    ", FG, FONT),
        ("99.9% reduction", GREEN, FONT_BOLD),
    ])
    add_frames(fb.render(), 0.3)

    # Bar chart
    bar_y = PADDING + len(fb.lines) * LINE_H + 6
    fb.add_line([("", FG, FONT)])  # spacer for bar 1
    fb.add_line([("", FG, FONT)])  # spacer for bar 2

    # Animate bar growing
    max_bar = 620
    for pct in [0.2, 0.5, 0.8, 1.0]:
        bar1_w = int(max_bar * pct)
        bar2_w = int(max_bar * (869 / 677000) * pct)
        add_frames(fb.render_with_bar(bar_y, bar1_w, bar2_w), 0.1)

    add_frames(fb.render_with_bar(bar_y, max_bar, int(max_bar * 869 / 677000)), 0.4)

    # Punchline
    fb.add_line([("", FG, FONT)])
    fb.add_line([
        ("  10/10 correct", WHITE, FONT_BOLD),
        (" · ", DIM, FONT),
        ("avg 90ms", WHITE, FONT_BOLD),
        (" · ", DIM, FONT),
        ("no GPU", WHITE, FONT_BOLD),
        (" · ", DIM, FONT),
        ("no embeddings", WHITE, FONT_BOLD),
        (" · ", DIM, FONT),
        ("1.8MB binary", WHITE, FONT_BOLD),
    ])
    fb.add_line([("  pure Rust · sub-ms cached · learns from interaction", DIM, FONT)])

    # Hold final frame
    final = fb.render_with_bar(bar_y, max_bar, int(max_bar * 869 / 677000))
    add_frames(final, 3.0)

    return frames


def main():
    print("Building frames...")
    t0 = time.time()
    frames = build_video()
    t1 = time.time()
    print(f"  {len(frames)} frames in {t1-t0:.1f}s")

    print(f"Encoding MP4 to {OUTPUT}...")
    # Stack frames into a single array for imageio
    iio.imwrite(
        str(OUTPUT),
        frames,
        fps=FPS,
        codec="libx264",
        plugin="pyav",
    )
    t2 = time.time()
    size_mb = OUTPUT.stat().st_size / (1024 * 1024)
    print(f"  Done in {t2-t1:.1f}s — {size_mb:.1f} MB")
    print(f"  Duration: {len(frames)/FPS:.1f}s at {FPS} FPS")
    print(f"\nOutput: {OUTPUT}")


if __name__ == "__main__":
    main()
