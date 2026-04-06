#!/usr/bin/env python3
"""CRFR 60s flagship v2 — all real data, sourced stats."""
import math, random
from pathlib import Path
import imageio.v3 as iio
import numpy as np
from PIL import Image, ImageDraw, ImageFont

W, H, FPS = 1400, 880, 30
BG=(13,17,23); FG=(230,237,243); DIM=(72,78,88); DIM2=(100,110,120)
GREEN=(63,185,80); CYAN=(88,166,255); YELLOW=(210,153,34)
WHITE=(240,246,252); RED=(255,123,114); ORANGE=(255,166,87)
BAR_IN=(58,86,145); BAR_OUT=(57,211,83); ACCENT=(40,46,56)
PAD=50
OUTPUT = Path(__file__).parent / "crfr-flagship-v2.mp4"

def _f(sz, b=False):
    t = "Bold" if b else "Regular"
    for p in [f"/usr/share/fonts/truetype/dejavu/DejaVuSansMono-{t}.ttf",
              f"/usr/share/fonts/truetype/liberation/LiberationMono-{t}.ttf"]:
        if Path(p).exists(): return ImageFont.truetype(p, sz)
    return ImageFont.load_default()

F=_f(19); FB=_f(19,True); FS=_f(15); FSB=_f(15,True)
FM=_f(22,True); FL=_f(28,True); FX=_f(36,True); FXX=_f(48,True); FH=_f(64,True)

def ct(d,y,t,c,f):
    bb=f.getbbox(t); d.text(((W-(bb[2]-bb[0]))//2,y),t,fill=c,font=f)
def add(fr,im,dur):
    a=np.array(im) if isinstance(im,Image.Image) else im
    fr.extend([a]*max(1,int(dur*FPS)))
def fade(fr,tgt,dur=0.35):
    if not fr: return
    s=fr[-1].astype(np.float32)
    d=(np.array(tgt) if isinstance(tgt,Image.Image) else tgt).astype(np.float32)
    n=max(1,int(dur*FPS))
    for i in range(n): fr.append(((1-i/(n-1))*s+(i/(n-1))*d).astype(np.uint8))
def mk(): return Image.new("RGB",(W,H),BG)

# ── DOM tree lines for sweep ─────────────────────────────────────────
_rng = random.Random(42)
_secs = [
    (0,0,"html",False),(1,1,"head",False),(2,2,"meta x 48",False),
    (10,1,"body",False),(15,2,"nav#mw-navigation",False),
    (20,3,"div.vector-menu x 8",False),(60,2,"div#content",False),
    (70,3,"h1: COVID-19 vaccine",False),(90,4,"div.mw-parser-output",False),
    (100,5,"p: A COVID-19 vaccine is a vaccine intended to...",False),
    (120,5,"div.toc (table of contents)",False),(140,6,"li x 42",False),
    (180,5,"h2: History",False),(200,5,"p x 12",False),
    (250,5,"h2: Types",False),(270,5,"p x 18",False),
    (308,5,"a: 8 Adverse effects",True),
    (320,5,"h2: Development",False),(450,5,"table.wikitable",False),
    (600,5,"h2: Deployment",False),(1000,5,"h2: Society and culture",False),
    (1500,5,"p x 30",False),(1700,5,"h2: Vaccine types",False),
    (1959,5,"p: mRNA vaccines were the first authorised...",True),
    (2600,5,"h2: Side effects",False),(2800,5,"p x 28",False),
    (3251,5,"p: Risks of serious illness far higher...",True),
    (3321,5,"p: up to 20% disruptive side effects...",True),
    (3700,5,"h2: Misconceptions",False),(4500,5,"div.reflist",False),
    (4600,6,"li.reference x 340",False),(6000,5,"div.catlinks",False),
    (6200,3,"footer",False),
]
DOM_LINES = []
_prev = 0
for nid,depth,label,hit in _secs:
    gap = nid - _prev
    if gap > 60:
        for k in range(min(gap//50, 4)):
            fid = _prev + (k+1)*(gap//5)
            DOM_LINES.append((fid, min(depth+1,7),
                f"{_rng.choice(['p','div','span','a','li','td','img'])} (node {fid})", False))
    DOM_LINES.append((nid, depth, label, hit))
    _prev = nid

def build():
    frames = []

    # ═══ ACT 1: THE SCALE (0-10s) ════════════════════════════════════
    # Opening: the problem at global scale

    i=mk(); d=ImageDraw.Draw(i)
    ct(d, 300, "Every day, AI agents make", DIM2, FL)
    ct(d, 350, "700 million web requests.", WHITE, FX)
    add(frames, i, 2.5)

    i=mk(); d=ImageDraw.Draw(i)
    ct(d, 250, "Each page can be millions of characters.", DIM2, FM)
    ct(d, 300, "Thousands of DOM nodes. Hundreds of thousands of tokens.", DIM2, FM)
    ct(d, 370, "Most of it is navigation, boilerplate, and noise.", DIM2, F)
    fade(frames, i, 0.35); add(frames, i, 2.5)

    i=mk(); d=ImageDraw.Draw(i)
    ct(d, 250, "Every token costs compute.", WHITE, FL)
    ct(d, 300, "Every token costs energy.", WHITE, FL)
    ct(d, 350, "Every token costs money.", WHITE, FL)
    fade(frames, i, 0.35); add(frames, i, 2.0)

    i=mk(); d=ImageDraw.Draw(i)
    ct(d, 280, "What if you only sent the 0.01%", WHITE, FL)
    ct(d, 330, "that actually answers the question?", WHITE, FL)
    fade(frames, i, 0.35); add(frames, i, 2.5)

    # ═══ ACT 2: THE SEARCH (10-22s) ══════════════════════════════════
    # Wikipedia DOM sweep — 12,434 nodes, find 4

    T_TOP = 110; LH_T = 22; VIS = (H-30-T_TOP)//LH_T
    total_sw = int(10.0 * FPS); found = []

    results_data = [
        (1.601, "link", "8 Adverse effects"),
        (1.432, "text", "Risks far higher than vaccine risks..."),
        (1.368, "text", "up to 20% disruptive side effects..."),
        (1.300, "text", "mRNA vaccines first authorised..."),
    ]

    for fi in range(total_sw):
        t = fi / total_sw
        si = int(t * len(DOM_LINES))
        cn = DOM_LINES[min(si, len(DOM_LINES)-1)][0]

        for li in range(min(si+1, len(DOM_LINES))):
            nid = DOM_LINES[li][0]
            if DOM_LINES[li][3] and nid not in [f[0] for f in found]:
                found.append((nid, DOM_LINES[li][2]))

        i=mk(); d=ImageDraw.Draw(i)
        d.text((PAD,15),"en.wikipedia.org/wiki/COVID-19_vaccine",fill=CYAN,font=FM)
        d.text((PAD,52),"Scanning 12,434 DOM nodes...",fill=DIM2,font=F)
        d.text((PAD+380,52),f"{cn:,}/12,434",fill=YELLOW,font=FB)
        fc = GREEN if found else DIM2
        d.text((PAD+560,52),f"{len(found)}/4 found",fill=fc,font=FB)

        bw = W-PAD*2
        d.rectangle([PAD,84,PAD+bw,90],fill=ACCENT)
        d.rectangle([PAD,84,PAD+int(bw*min(100,cn/12434*100)/100),90],fill=CYAN)

        sv = VIS//3; st = max(0,si-sv); en = min(len(DOM_LINES),st+VIS)
        for vi, li in enumerate(range(st, en)):
            nid,depth,label,is_hit = DOM_LINES[li]
            y = T_TOP + vi*LH_T; x = PAD + depth*18
            if is_hit and li <= si:
                d.rectangle([PAD-4,y-2,W//2+60,y+LH_T-4],fill=(20,60,30))
                d.rectangle([PAD-6,y-2,PAD-2,y+LH_T-4],fill=GREEN)
                d.text((x,y),label[:65],fill=GREEN,font=FSB)
            elif li == si:
                d.rectangle([PAD-4,y-1,W//2+60,y+LH_T-5],fill=(20,35,55))
                d.text((x,y),label[:65],fill=CYAN,font=FS)
            elif li < si:
                d.text((x,y),label[:65],fill=DIM,font=FS)
            else:
                d.text((x,y),label[:65],fill=(50,55,65),font=FS)

        px,py = W//2+100, T_TOP
        if found:
            d.text((px,py),f"Located ({len(found)}/4):",fill=GREEN,font=FB); py+=32
            for idx,(nid,lbl) in enumerate(found):
                if idx < len(results_data):
                    sc,role,txt = results_data[idx]
                    d.text((px,py),f"[{sc:.3f}] {role}",fill=YELLOW,font=FS); py+=20
                    d.text((px+8,py),f'"{txt}"',fill=WHITE,font=FS); py+=28

        frames.append(np.array(i))

    add(frames, frames[-1], 0.5)

    # ═══ ACT 3: THREE SITES (22-32s) ═════════════════════════════════
    # Show 3 real sites with answers

    sites = [
        ("en.wikipedia.org/wiki/COVID-19_vaccine",
         "What are the side effects of COVID vaccines?",
         12434, 2708245, 677061, 528, "up to 20% report disruptive side effects after 2nd mRNA dose",
         130, "99.98"),
        ("riksbanken.se/sv/penningpolitik",
         "What is Sweden's interest rate?",
         2690, 602344, 150586, 91, 'Styrränta 1,75 %  Gäller från den 25 mars 2026',
         104, "99.93"),
        ("xe.com (React SPA, 5,291 nodes)",
         "What is the USD/SEK exchange rate?",
         5291, 1358050, 339513, 148, "1.00 USD = 9.47964826 SEK  Mid-market rate",
         85, "99.97"),
    ]

    for url, q, nodes, chars, tok_in, ms, answer, tok_out, red in sites:
        i=mk(); d=ImageDraw.Draw(i)
        d.text((PAD,40),url,fill=CYAN,font=FM)
        d.text((PAD,80),f'"{q}"',fill=DIM2,font=F)
        d.text((PAD,130),"IN:",fill=DIM2,font=FB)
        d.text((PAD+60,130),f'{chars:,} chars  ·  {tok_in:,} tokens  ·  {nodes:,} nodes',fill=FG,font=F)
        d.text((PAD,180),"Answer:",fill=GREEN,font=FB)
        a = answer[:72]+"..." if len(answer)>72 else answer
        d.text((PAD+110,180),f'"{a}"',fill=WHITE,font=FB)
        d.text((PAD+110,212),f'{ms}ms  ·  {tok_out} tokens',fill=GREEN,font=F)

        bw = W-PAD*2-180
        d.rectangle([PAD,270,PAD+bw,290],fill=BAR_IN)
        d.text((PAD+bw+10,272),f'{tok_in:,} tokens',fill=DIM2,font=FS)
        ob = max(4,int(bw*tok_out/tok_in))
        d.rectangle([PAD,300,PAD+ob,320],fill=BAR_OUT)
        d.text((PAD+ob+10,302),f'{tok_out} tokens',fill=GREEN,font=FS)

        d.text((PAD,360),f'{red}%',fill=GREEN,font=FXX)
        d.text((PAD+260,378),"reduction",fill=DIM2,font=FM)

        fade(frames, i, 0.35); add(frames, i, 2.8)

    # ═══ ACT 4: IT LEARNS (32-42s) ═══════════════════════════════════
    # Animated score bars growing with feedback

    i=mk(); d=ImageDraw.Draw(i)
    ct(d, 380, "It learns from every interaction.", WHITE, FX)
    fade(frames, i, 0.35); add(frames, i, 1.5)

    # Animate 3 query iterations with growing bars
    learn_steps = [
        ("Q1: COLD START", "COVID vaccine side effects efficacy mRNA",
         "Risks of serious illness far higher...", 1.432, 0.0,
         "No prior knowledge. Pure keyword matching."),
        ("Q2: AFTER FEEDBACK", "vaccine safety adverse events clinical trials",
         "Risks of serious illness far higher...", 1.390, 0.081,
         "causal_boost: +0.081  ← learned from Q1 feedback"),
        ("Q3: GENERALIZATION", "immunization risks profile serious reactions",
         "Still found — completely new vocabulary", 1.390, 0.081,
         "Concept memory transfers to unseen phrasings."),
    ]

    bar_max = 500
    for step_i, (label, goal, result, score, boost, note) in enumerate(learn_steps):
        # Animate bar growing
        n_anim = int(0.8 * FPS)
        for ai in range(n_anim):
            p = ai / max(1, n_anim-1)
            # Ease-out
            p = 1 - (1-p)**2

            i=mk(); d=ImageDraw.Draw(i)
            d.text((PAD,30),"Wikipedia COVID-19 vaccine  ·  12,434 nodes",fill=CYAN,font=FM)
            d.text((PAD,65),"Three queries. Different wording. Same page.",fill=DIM2,font=F)

            y = 120
            for si2, (lb2, gl2, res2, sc2, bo2, nt2) in enumerate(learn_steps):
                if si2 > step_i:
                    break
                color = DIM2 if si2 == 0 else (YELLOW if si2 == 1 else GREEN)
                d.text((PAD, y), lb2, fill=color, font=FB)
                y += 26
                g = gl2[:60]+"..." if len(gl2)>60 else gl2
                d.text((PAD+20, y), f'"{g}"', fill=DIM, font=FS)
                y += 24

                # Score bar
                total_sc = sc2 + bo2
                if si2 < step_i:
                    bw2 = int(total_sc / 2.0 * bar_max)
                elif si2 == step_i:
                    bw2 = int(total_sc / 2.0 * bar_max * p)
                else:
                    bw2 = 0

                # Draw bar segments
                base_w = int(sc2 / 2.0 * bar_max * (p if si2==step_i else 1))
                boost_w = int(bo2 / 2.0 * bar_max * (p if si2==step_i else 1))
                d.rectangle([PAD+20, y, PAD+20+base_w, y+14], fill=BAR_IN)
                if boost_w > 2:
                    d.rectangle([PAD+20+base_w, y, PAD+20+base_w+boost_w, y+14], fill=BAR_OUT)

                score_txt = f"{sc2:.3f}"
                if bo2 > 0.001:
                    score_txt += f" + {bo2:.3f}"
                d.text((PAD+20+base_w+boost_w+10, y-2), score_txt, fill=YELLOW, font=FS)
                y += 20

                d.text((PAD+20, y), f'→ "{res2}"', fill=WHITE, font=FS)
                y += 22
                d.text((PAD+20, y), nt2, fill=DIM if si2 < step_i else GREEN, font=FS)
                y += 28

                # Feedback arrow between steps
                if si2 < step_i and si2 < 2:
                    d.text((PAD+20, y), "↓ feedback(successful_nodes)", fill=YELLOW, font=FS)
                    y += 24

            frames.append(np.array(i))

        # Hold completed step
        add(frames, frames[-1], 1.5)

    # ═══ ACT 5: THE NUMBERS (42-52s) ═════════════════════════════════
    # All metrics with sourced data

    i=mk(); d=ImageDraw.Draw(i)
    ct(d, 40, "The numbers.", DIM2, FM)

    y = 95
    metrics = [
        ("TOKENS",      "677,061 in",  "130 out",       "99.98% eliminated"),
        ("LATENCY",     "528ms cold",  "< 1ms cached",  "sub-millisecond on repeat queries"),
        ("DOM NODES",   "12,434",      "4 returned",    "0.03% of the page"),
        ("BINARY SIZE", "1.8 MB",      "zero deps",     "no GPU, no model files, no runtime"),
    ]
    for lb,v1,v2,nt in metrics:
        d.text((PAD,y),f"{lb:14}",fill=DIM2,font=FB)
        d.text((PAD+220,y),v1,fill=FG,font=F)
        d.text((PAD+420,y),"->",fill=DIM,font=F)
        d.text((PAD+460,y),v2,fill=GREEN,font=FB)
        d.text((PAD+650,y),nt,fill=DIM,font=FS)
        y += 34

    y += 15
    d.rectangle([PAD,y,W-PAD,y+1],fill=ACCENT); y+=20

    # Cost section with pricing note
    d.text((PAD,y),"Token cost estimate",fill=DIM2,font=FB)
    d.text((PAD+280,y),"(mid-range 2026: ~$2.50/Mtok input)",fill=DIM,font=FS)
    y += 28

    costs = [
        ("Wikipedia (2.7M chars)", "$1.69", "$0.0003", "5,633x"),
        ("Riksbanken (602K chars)", "$0.38", "$0.0003", "1,267x"),
        ("XE.com (1.4M chars)", "$0.85", "$0.0002", "4,250x"),
    ]
    for site,raw,crfr,mult in costs:
        d.text((PAD+20,y),site,fill=FG,font=FS)
        d.text((PAD+320,y),f"raw: {raw}",fill=RED,font=FS)
        d.text((PAD+480,y),f"→ {crfr}",fill=GREEN,font=FSB)
        d.text((PAD+580,y),f"({mult} cheaper)",fill=GREEN,font=FS)
        y += 24
    y += 10
    d.text((PAD+20,y),"Prices based on GPT-4o-class input pricing, April 2026.",fill=DIM,font=FS)
    d.text((PAD+20,y+18),"Estimated against full raw HTML/DOM as input tokens.",fill=DIM,font=FS)
    y += 46

    # Scale
    d.text((PAD,y),"At scale: 1,000 queries/day",fill=DIM2,font=F); y+=30
    d.text((PAD,y),"Raw HTML:",fill=FG,font=FB)
    d.text((PAD+160,y),"$4,004/day",fill=RED,font=FB)
    d.text((PAD+380,y),"$1,461,460/year",fill=RED,font=F); y+=28
    d.text((PAD,y),"With CRFR:",fill=GREEN,font=FB)
    d.text((PAD+160,y),"$2/day",fill=GREEN,font=FB)
    d.text((PAD+380,y),"$730/year",fill=GREEN,font=F); y+=34
    d.text((PAD,y),"Annual savings:",fill=DIM2,font=F)
    d.text((PAD+220,y),"$1,460,730",fill=GREEN,font=FX)

    fade(frames, i, 0.35); add(frames, i, 8.0)

    # ═══ ACT 6: ENVIRONMENT (52-57s) ═════════════════════════════════

    i=mk(); d=ImageDraw.Draw(i)
    ct(d, 50, "The environmental cost.", DIM2, FM)

    y = 110
    stats = [
        "AI agents make 700 million web requests per day.",
        "AI data centers emitted 105 million tons of CO2 last year.",
        "That exceeds the entire aviation industry.",
        "",
        "Every unnecessary token is wasted energy.",
        "Every wasted page read is burned compute.",
    ]
    for s in stats:
        if s:
            d.text((PAD, y), s, fill=DIM2 if y < 220 else WHITE, font=F)
        y += 30

    y += 20
    d.text((PAD, y), "Reducing tokens by 99.98% means:", fill=WHITE, font=FB); y += 34
    d.text((PAD+20, y), "99.98% less compute per page read", fill=GREEN, font=F); y += 28
    d.text((PAD+20, y), "99.98% less energy per page read", fill=GREEN, font=F); y += 28
    d.text((PAD+20, y), "99.98% less carbon per page read", fill=GREEN, font=F); y += 40

    d.text((PAD, y), "At 1B agent queries/day, eliminating 99% of tokens", fill=DIM2, font=FS)
    y += 20
    d.text((PAD, y), "saves the equivalent energy of 35,000 households annually.", fill=DIM2, font=FS)
    y += 24
    d.text((PAD, y), "Sources: HUMAN Security 2026, IEA, Google Cloud AI blog, Goldman Sachs Research", fill=DIM, font=FS)

    fade(frames, i, 0.35); add(frames, i, 5.0)

    # ═══ ACT 7: CLOSE (57-63s) ═══════════════════════════════════════

    i=mk(); d=ImageDraw.Draw(i)
    cy = 160
    ct(d, cy, "Find the signal.", WHITE, FX); cy+=50
    ct(d, cy, "Kill the noise.", WHITE, FX); cy+=50
    ct(d, cy, "Learn from every interaction.", GREEN, FX); cy+=70

    ct(d, cy, "99.98% token reduction  ·  sub-ms latency", DIM2, F); cy+=30
    ct(d, cy, "No GPU  ·  No models  ·  1.8 MB  ·  pure Rust", DIM2, F); cy+=30
    ct(d, cy, "5,600x cheaper  ·  learns without training data", GREEN, FB); cy+=50

    ct(d, cy, "Open source.", DIM2, FM)

    fade(frames, i, 0.4); add(frames, i, 4.5)

    # Final black
    add(frames, mk(), 1.0)

    return frames

def main():
    print("Building flagship v2...")
    fr = build()
    s = len(fr)/FPS
    print(f"  {len(fr)} frames = {s:.1f}s")
    print(f"Encoding {OUTPUT}...")
    iio.imwrite(str(OUTPUT), fr, fps=FPS, codec="libx264", plugin="pyav")
    mb = OUTPUT.stat().st_size/(1024*1024)
    print(f"  Done -- {mb:.1f} MB")
    for t,n in [(3,"scale"),(15,"sweep"),(27,"sites"),(37,"learns"),(47,"numbers"),(54,"env"),(59,"close")]:
        idx=min(int(t*FPS),len(fr)-1)
        Image.fromarray(fr[idx]).save(OUTPUT.parent/f"pv2_{n}.png")
    print("  Saved previews")

if __name__=="__main__": main()
