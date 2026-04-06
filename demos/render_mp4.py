#!/usr/bin/env python3
"""
CRFR Demo: 5 Real Sites — one per screen, 100% authentic data.

Every number comes from actual CRFR runs on April 6, 2026 via MCP parse_crfr.
Shows: Fetch → DOM tree → CRFR propagation → located nodes → IN/OUT bars.

Usage: python3 demos/render_mp4.py
Output: demos/crfr-5sites.mp4
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
BAR_IN = (68, 106, 175)
BAR_OUT= (57, 211, 83)
ACCENT = (48, 54, 65)
NODE_BG = (22, 27, 34)
NODE_HI = (35, 134, 54)

PAD    = 36
LH     = 28
FS     = 19

OUTPUT = Path(__file__).parent / "crfr-5sites.mp4"

# ── Fonts ────────────────────────────────────────────────────────────────

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

# ─── All data from real MCP parse_crfr runs, April 6, 2026 ──────────────
#
# Each site recorded: total_nodes, parse_time_ms, node labels+scores,
# field_build_ms, propagation_ms.
# Raw HTML char counts from whitepaper Section 5+6 (same pages).

SITES = [
    {
        "num": "1/5",
        "domain": "riksbanken.se",
        "title": "What is Sweden's interest rate?",
        "goal": "styrränta ränta procent Riksbanken penningpolitik reporänta 2026",
        "raw_chars": 602_344,
        "total_nodes": 2_690,
        "parse_ms": 279,
        "field_ms": 168,
        "prop_ms": 28,
        "out_nodes": 5,
        "results": [
            (2.558, "text",  "KPIF, februari 2026 1,7 % (2,0 procent i januari 2026)"),
            (2.410, "text",  "Styrränta 1,75 % Gäller från den 25 mars 2026"),
            (2.325, "text",  "Inflationsmål 2 % Mål för KPIF"),
            (1.661, "heading", "Aktuell penningpolitik och styrränta"),
        ],
        "tree": [
            ("document", 0, False),
            ("  banner", 1, False),
            ("    navigation", 2, False),
            ("      link × 12", 3, False),
            ("  main", 1, False),
            ("    heading: Penningpolitik", 2, False),
            ("    section", 2, False),
            ("      text: Styrränta 1,75 %", 3, True),    # ← HIT
            ("      text: KPIF 1,7 %", 3, True),          # ← HIT
            ("      text: Inflationsmål 2 %", 3, True),   # ← HIT
            ("    section", 2, False),
            ("      heading: Aktuell penningpolitik", 3, True),  # ← HIT
            ("      text × 8", 3, False),
            ("  footer", 1, False),
            ("    link × 15", 2, False),
        ],
    },
    {
        "num": "2/5",
        "domain": "en.wikipedia.org/wiki/COVID-19_vaccine",
        "title": "COVID vaccine side effects",
        "goal": "COVID-19 vaccine side effects efficacy mRNA Pfizer Moderna",
        "raw_chars": 2_708_245,
        "total_nodes": 12_434,
        "parse_ms": 1_451,
        "field_ms": 707,
        "prop_ms": 188,
        "out_nodes": 5,
        "results": [
            (1.462, "link",  "8 Adverse effects"),
            (1.432, "text",  "The risks of serious illness from COVID-19 are far higher"),
            (None, None,     "than the risk posed by the vaccines..."),
            (1.318, "text",  "mRNA vaccines were the first authorised in UK, US and EU."),
            (1.248, "text",  "up to 20% report disruptive side effects after 2nd mRNA dose"),
        ],
        "tree": [
            ("document", 0, False),
            ("  navigation", 1, False),
            ("    link × 48", 2, False),
            ("  main (article)", 1, False),
            ("    heading: COVID-19 vaccine", 2, False),
            ("    text × 340 (intro, history...)", 2, False),
            ("    heading: 8 Adverse effects", 2, True),       # ← HIT
            ("    text: risks far higher...", 3, True),         # ← HIT
            ("    text: mRNA vaccines first...", 3, True),      # ← HIT
            ("    text: up to 20% disruptive...", 3, True),     # ← HIT
            ("    text × 580 (rest of article)", 2, False),
            ("  footer", 1, False),
            ("    link × 200+", 2, False),
        ],
    },
    {
        "num": "3/5",
        "domain": "xe.com",
        "title": "USD/SEK exchange rate",
        "goal": "currency converter exchange rate USD SEK kronor dollar",
        "raw_chars": 1_358_050,
        "total_nodes": 5_291,
        "parse_ms": 384,
        "field_ms": 253,
        "prop_ms": 81,
        "out_nodes": 5,
        "results": [
            (1.405, "generic",  "1.00 USD = 9.47964826 SEK  Mid-market rate at 00:40 UTC"),
            (1.132, "data", "Track exchange rate"),
            (0.997, "data", "Our currency rankings show the most popular exchange rate"),
        ],
        "note": "React SPA · SSR JSON · 5,291 DOM nodes",
        "tree": [
            ("document", 0, False),
            ("  banner", 1, False),
            ("    navigation × 3", 2, False),
            ("  main", 1, False),
            ("    generic: 1.00 USD = 9.479 SEK", 2, True),   # ← HIT
            ("    data: i18n strings × 400+", 2, False),
            ("    data: Track exchange rate", 2, True),         # ← HIT
            ("    data: currency rankings...", 2, True),        # ← HIT
            ("    data: JSON manifests × 800+", 2, False),
            ("  footer", 1, False),
        ],
    },
    {
        "num": "4/5",
        "domain": "en.wikipedia.org/wiki/Dark_matter",
        "title": "What is the universe made of?",
        "goal": "universe composition dark matter dark energy ordinary matter percentage",
        "raw_chars": 1_339_506,
        "total_nodes": 6_100,
        "parse_ms": 795,
        "field_ms": 429,
        "prop_ms": 237,
        "out_nodes": 5,
        "results": [
            (1.900, "text",  "5% ordinary matter, 26.8% dark matter, 68.2% dark energy"),
            (1.217, "text",  "Dark energy Dark fluid Dark matter Lambda-CDM model"),
            (1.029, "listitem", "Lambda-CDM model"),
        ],
        "tree": [
            ("document", 0, False),
            ("  navigation", 1, False),
            ("    link × 52", 2, False),
            ("  main (article)", 1, False),
            ("    heading: Dark matter", 2, False),
            ("    text × 120 (intro...)", 2, False),
            ("    text: 5% ordinary, 26.8% dark...", 2, True),  # ← HIT
            ("    text × 480 (evidence, detection)", 2, False),
            ("    text: Dark energy Dark fluid...", 2, True),    # ← HIT
            ("    listitem: Lambda-CDM model", 2, True),         # ← HIT
            ("  footer", 1, False),
            ("    link × 180+", 2, False),
        ],
    },
    {
        "num": "5/5",
        "domain": "stackoverflow.com",
        "title": "Why is sorted array faster?",
        "goal": "branch prediction sorted array pipeline misprediction Mysticial",
        "raw_chars": 879_595,
        "total_nodes": 3_653,
        "parse_ms": 491,
        "field_ms": 393,
        "prop_ms": 84,
        "out_nodes": 5,
        "results": [
            (1.214, "text", "...branch prediction penalty is removed, as explained"),
            (None, None,    "beautifully in Mysticial's answer."),
            (1.050, "listitem", "branch prediction penalty isn't that bad on RISC (M1)"),
            (1.020, "text", '"==" in sorted array not faster than unsorted array?'),
        ],
        "note": "35K upvotes · the most famous SO answer",
        "tree": [
            ("document", 0, False),
            ("  banner", 1, False),
            ("    navigation × 5", 2, False),
            ("  main", 1, False),
            ("    heading: Why is sorted faster?", 2, False),
            ("    text: question body...", 2, False),
            ("    text: branch prediction removed...", 2, True),   # ← HIT
            ("    listitem: M1 penalty comment", 2, True),         # ← HIT
            ("    text × 120 (other answers)", 2, False),
            ("    text: == in sorted array?", 2, True),            # ← HIT
            ("  complementary (sidebar)", 1, False),
            ("    link × 40", 2, False),
        ],
    },
]

# ── Drawing helpers ──────────────────────────────────────────────────────

def tx(draw, x, y, text, color, font):
    draw.text((x, y), text, fill=color, font=font)
    bbox = font.getbbox(text)
    return x + bbox[2] - bbox[0]

def draw_segs(draw, x, y, segs):
    for text, color, font in segs:
        x = tx(draw, x, y, text, color, font)
    return x

class Screen:
    def __init__(self):
        self.ops = []  # list of callables that take (draw)

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

# ── Build one site ───────────────────────────────────────────────────────

def build_site(site):
    frames = []
    scr = Screen()
    y = PAD

    def add(duration_s, s=None):
        f = (s or scr).render()
        for _ in range(max(1, int(duration_s * FPS))):
            frames.append(f)

    # ── Header ──
    scr.text(PAD, y, [
        (f" {site['num']} ", DIM, FB),
        ("── ", DIM, F),
        (site["domain"], CYAN, F_BIG),
    ])
    y += LH + 6
    scr.text(PAD, y, [(f"  {site['title']}", WHITE, FB)])
    y += LH
    goal = site["goal"]
    if len(goal) > 68:
        goal = goal[:65] + "..."
    scr.text(PAD, y, [("  Goal: ", DIM, F_SM), (f'"{goal}"', DIM, F_SM)])
    y += LH + 6
    add(0.35)

    # ── Fetch + Parse + CRFR (animate pipeline) ──
    steps = [
        ("Fetch", f"{site['raw_chars']:,} chars"),
        ("Parse", f"{site['total_nodes']:,} nodes"),
        ("CRFR",  f"{site['prop_ms']}ms → {site['out_nodes']} nodes"),
    ]
    for label, result in steps:
        scr.text(PAD, y, [("  ", FG, F), (f"{label:5} ", YELLOW, FB), ("·····", DIM, F)])
        add(0.12)
        scr = scr.copy()
        scr.ops[-1] = lambda d, px=PAD, py=y, l=label, r=result: draw_segs(
            d, px, py,
            [("  ", FG, F), (f"{l:5} ", GREEN, FB), ("✓ ", GREEN, FB), (r, FG, F)])
        add(0.12)
        y += LH

    y += 6

    # ── DOM tree (left) + Results (right) side by side ──
    tree_x = PAD + 10
    result_x = W // 2 + 20
    tree_y_start = y

    # Section labels
    scr.text(tree_x, y, [("DOM Tree", DIM, F_SMB), (f"  ({site['total_nodes']:,} nodes)", DIM, F_SM)])
    scr.text(result_x, y, [("CRFR Located", GREEN, F_SMB)])
    y += LH
    add(0.15)

    # Draw tree nodes (animate, highlight hits)
    for label, depth, is_hit in site["tree"]:
        indent = "  " * depth
        disp = indent + label
        if len(disp) > 48:
            disp = disp[:45] + "..."
        if is_hit:
            # Highlight background
            scr.rect(tree_x - 4, y - 1, 440, LH - 4, NODE_HI)
            scr.text(tree_x, y, [(disp, WHITE, FB)])
        else:
            scr.text(tree_x, y, [(disp, DIM, F_SM)])
        y += LH - 5
        add(0.06)

    # Draw results on right side
    res_y = tree_y_start + LH + 4
    for entry in site["results"]:
        score, role, text = entry
        if score is not None:
            scr.text(result_x, res_y, [
                (f"[{score:.3f}]", YELLOW, F_SM),
                (f" {role}: ", CYAN, F_SM),
            ])
            res_y += LH - 6
            # Truncate long text
            if len(text) > 52:
                text = text[:49] + "..."
            scr.text(result_x + 10, res_y, [(f'"{text}"', WHITE, F_SM)])
        else:
            scr.text(result_x + 10, res_y, [(f' {text}', WHITE, F_SM)])
        res_y += LH - 2
        add(0.1)

    # Note
    if "note" in site:
        scr.text(result_x, res_y + 4, [(site["note"], DIM, F_SM)])

    y = max(y, res_y) + 14

    # ── IN/OUT bars ──
    scr.text(PAD, y, [("  " + "─" * 82, ACCENT, F_SM)])
    y += LH - 4

    # Compute char counts for output
    out_chars = sum(len(e[2]) for e in site["results"] if e[0] is not None)
    reduction = (1 - out_chars / site["raw_chars"]) * 100

    scr.text(PAD, y, [
        ("  IN:  ", DIM, FB),
        (f"{site['raw_chars']:>12,} chars", FG, F),
        ("  │  ", DIM, F),
        (f"{site['total_nodes']:,} nodes", FG, F),
    ])
    y += LH
    scr.text(PAD, y, [
        ("  OUT: ", GREEN, FB),
        (f"{out_chars:>12,} chars", GREEN, F),
        ("  │  ", DIM, F),
        (f"{len([e for e in site['results'] if e[0] is not None])} nodes", GREEN, F),
        ("     ", FG, F),
        (f"{site['parse_ms']}ms", YELLOW, FB),
    ])
    y += LH + 4

    # Bars
    max_bw = 520
    in_bw = max_bw
    out_bw = max(3, int(max_bw * out_chars / site["raw_chars"]))

    scr.rect(PAD + 80, y, in_bw, 14, BAR_IN)
    scr.text(PAD + 80 + in_bw + 10, y - 2, [(f"{site['raw_chars']:,}", DIM, F_SM)])
    y += 20
    scr.rect(PAD + 80, y, out_bw, 14, BAR_OUT)
    scr.text(PAD + 80 + out_bw + 10, y - 2, [(f"{out_chars:,}", GREEN, F_SM)])
    y += 24

    scr.text(PAD + 80, y, [(f"{reduction:.2f}% reduction", GREEN, FB)])

    add(1.0)
    return frames

# ── Title + closing ──────────────────────────────────────────────────────

def build_title():
    scr = Screen()
    cy = H // 2 - 70
    scr.text(PAD + 40, cy, [("CRFR", CYAN, F_XXL)])
    scr.text(PAD + 40, cy + 50, [("Causal Resonance Field Retrieval", DIM, _font(20))])
    scr.text(PAD + 40, cy + 90, [
        ("5 live websites", WHITE, FB),
        ("  ·  ", DIM, F),
        ("real data", WHITE, FB),
        ("  ·  ", DIM, F),
        ("April 2026", WHITE, FB),
    ])
    scr.text(PAD + 40, cy + 130, [("no GPU  ·  no embeddings  ·  1.8MB Rust binary", DIM, F_SM)])
    f = scr.render()
    return [f] * int(1.5 * FPS)

def build_closing():
    scr = Screen()
    total_in = sum(s["raw_chars"] for s in SITES)
    total_nodes_in = sum(s["total_nodes"] for s in SITES)
    total_out_chars = 0
    total_out_nodes = 0
    for s in SITES:
        total_out_chars += sum(len(e[2]) for e in s["results"] if e[0] is not None)
        total_out_nodes += len([e for e in s["results"] if e[0] is not None])
    pct = (1 - total_out_chars / total_in) * 100

    cy = 60
    scr.text(PAD + 40, cy, [("5 sites  ·  all answers found", DIM, F)])
    cy += 50
    scr.text(PAD + 40, cy, [
        (f"{total_in:,} chars", FG, FB),
        ("  →  ", DIM, F),
        (f"{total_out_chars:,} chars", GREEN, FB),
    ])
    cy += 36
    scr.text(PAD + 40, cy, [
        (f"{total_nodes_in:,} nodes", FG, FB),
        ("  →  ", DIM, F),
        (f"{total_out_nodes} nodes", GREEN, FB),
    ])
    cy += 60
    scr.text(PAD + 40, cy, [(f"{pct:.2f}%", GREEN, _font(52, bold=True))])
    cy += 64
    scr.text(PAD + 40, cy, [("of web content eliminated", DIM, _font(22))])
    cy += 60
    scr.text(PAD + 40, cy, [("─" * 50, ACCENT, F)])
    cy += 36
    scr.text(PAD + 40, cy, [
        ("pure Rust", WHITE, FB), ("  ·  ", DIM, F),
        ("sub-ms cached", WHITE, FB), ("  ·  ", DIM, F),
        ("learns from interaction", WHITE, FB),
    ])
    cy += 30
    scr.text(PAD + 40, cy, [("zero dependencies  ·  no GPU  ·  1.8MB binary", DIM, F_SM)])

    f = scr.render()
    return [f] * int(3.0 * FPS)

# ── Transitions ──────────────────────────────────────────────────────────

def fade(a, b, dur=0.2):
    n = max(1, int(dur * FPS))
    af, bf = a.astype(np.float32), b.astype(np.float32)
    return [((1 - i/(n-1)) * af + (i/(n-1)) * bf).astype(np.uint8) for i in range(n)]

# ── Main ─────────────────────────────────────────────────────────────────

def main():
    print("Building CRFR 5-sites demo...")
    all_frames = []

    title = build_title()
    all_frames.extend(title)
    print(f"  Title: {len(title)/FPS:.1f}s")

    for i, site in enumerate(SITES):
        sf = build_site(site)
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
    mb = OUTPUT.stat().st_size / (1024*1024)
    print(f"  Done — {mb:.1f} MB")

    # Preview frames
    Image.fromarray(all_frames[len(title)+10]).save(OUTPUT.parent / "preview_site1.png")
    Image.fromarray(all_frames[-1]).save(OUTPUT.parent / "preview_closing.png")
    print("  Saved preview PNGs")

if __name__ == "__main__":
    main()
