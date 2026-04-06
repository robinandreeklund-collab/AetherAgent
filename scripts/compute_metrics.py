#!/usr/bin/env python3
"""Compute nDCG@5, MRR, P@5 from the 20-site baseline Q1 results (2026-04-06)."""
import math, json

def ndcg5(rels):
    rels = rels[:5]
    dcg = sum(r / math.log2(i+2) for i, r in enumerate(rels))
    ideal = sorted(rels, reverse=True)
    idcg = sum(r / math.log2(i+2) for i, r in enumerate(ideal))
    return dcg / idcg if idcg > 0 else 0.0

def mrr(rels):
    for i, r in enumerate(rels):
        if r > 0: return 1.0 / (i+1)
    return 0.0

def p5(rels):
    return sum(rels[:5]) / 5.0

# Results from MCP baseline queries (Q1 cold-start, 2026-04-06)
# Each entry: (name, nodes_returned, total_nodes, relevances_top5)
# Relevance = 1 if node contains actual content keywords, 0 if nav/boilerplate
results = [
    # BBC News: 3 nodes. OG title (meta=0), measles article (1), whale article (1)
    ("BBC News", 3, 689, [0, 1, 1]),
    # NPR: 0 nodes returned
    ("NPR", 0, 1266, []),
    # Guardian: 0 nodes returned
    ("The Guardian", 0, 1752, []),
    # GitHub: 2 nodes. hermes-agent heading (1), LiteRT-LM star count (1)
    ("GitHub Trending", 2, 561, [1, 1]),
    # HN: 5 nodes. GovAuctions (0-irrelevant to AI), "Hacker News" (0), login (0), hide (0), hide (0)
    ("Hacker News", 5, 486, [0, 0, 0, 0, 0]),
    # SO: 2 nodes. Hebrew question (0), boot RAM (0) — completely irrelevant
    ("Stack Overflow", 2, 688, [0, 0]),
    # Einstein: 4 nodes. "Born 1879-03-14 Ulm" (1), same again (1), full bio (1), "1953: Zernike" (0)
    ("Wikipedia Einstein", 4, 8330, [1, 1, 1, 0]),
    # Rust: 5 nodes. "created in 2015" (1), category link (0), "Graydon Hoare created" (1), origin story (1), survey note (0)
    ("Wikipedia Rust", 5, 4462, [1, 0, 1, 1, 0]),
    # Python: 5 nodes. "Guido van Rossum" (1), "Programming languages" (0), "Designed by" (1), "History of Python" (1), "Functional programming" (0)
    ("Wikipedia Python", 5, 5043, [1, 0, 1, 1, 0]),
    # Linux: 5 nodes. Full intro with Torvalds+1991 (1), Minix/Tanenbaum (1), "Kernel" link (0), FTP server naming (1), Torvalds 1991 cite (1)
    ("Wikipedia Linux", 5, 5713, [1, 1, 0, 1, 1]),
    # Amazon: 0 nodes (SPA, 4 total nodes)
    ("Amazon", 0, 4, []),
    # ESPN: 5 nodes. "scores" listitem (1), "scores" list (0-duplicate), NFL trade article (1), "scores" link (0-dup), "Mock Draft" (0)
    ("ESPN", 5, 644, [1, 0, 1, 0, 0]),
    # Weather: 4 nodes. "Today Forecast" link (1), "Today Forecast" listitem (0-dup), Artemis article (0), nav dump (0)
    ("Weather.com", 4, 338, [1, 0, 0, 0]),
    # Yahoo Finance: FETCH ERROR
    ("Yahoo Finance", 0, 0, []),
    # Allrecipes: 2 nodes. "Leftover Matzo" (0-not pasta), "Strawberry Cobblers" (0-not pasta)
    ("Allrecipes", 2, 808, [0, 0]),
    # Khan Academy: 1 node, just title (0)
    ("Khan Academy", 1, 1, [0]),
    # USA.gov: 5 nodes. "Gov benefits" heading (1), description (1), expanded (1), "Disability" (1), "Disasters" (1)
    ("USA.gov", 5, 294, [1, 1, 1, 1, 1]),
    # Nature: 5 nodes. "Latest Research" (1), meta-research paper (1), title again (1), replication games list (1), replication briefing (1)
    ("Nature", 5, 426, [1, 1, 1, 1, 1]),
    # TripAdvisor: 1 node, JS-required message (0)
    ("TripAdvisor", 1, 1, [0]),
    # WebMD: 2 nodes. "Cold, Flu, & Cough" category (1), "Cold and Flu Map" (1)
    ("WebMD", 2, 763, [1, 1]),
]

print("=" * 85)
print(f"{'Site':<22} {'Nodes':>5} {'Total':>6} {'nDCG@5':>8} {'MRR':>8} {'P@5':>8} {'Old BL':>8}")
print("-" * 85)

old_baselines = {
    "BBC News": 0.000, "NPR": 0.000, "The Guardian": 0.000,
    "GitHub Trending": 0.684, "Hacker News": 0.000, "Stack Overflow": 0.000,
    "Wikipedia Einstein": 0.786, "Wikipedia Rust": 1.000, "Wikipedia Python": 1.000,
    "Wikipedia Linux": 0.613, "Amazon": 0.631, "ESPN": 0.509,
    "Weather.com": 0.830, "Yahoo Finance": 0.869, "Allrecipes": 0.661,
    "Khan Academy": 0.000, "USA.gov": 0.655, "Nature": 0.655,
    "TripAdvisor": 0.000, "WebMD": 0.947,
}

sum_ndcg = 0
sum_mrr = 0
sum_p5 = 0
count = 0
improved = 0
regressed = 0
same = 0

for name, nodes, total, rels in results:
    n = ndcg5(rels)
    m = mrr(rels)
    p = p5(rels)
    old = old_baselines.get(name, 0)
    delta = n - old
    marker = ""
    if delta > 0.05: marker = " ↑"
    elif delta < -0.05: marker = " ↓"

    if delta > 0.01: improved += 1
    elif delta < -0.01: regressed += 1
    else: same += 1

    print(f"{name:<22} {nodes:>5} {total:>6} {n:>8.3f} {m:>8.3f} {p:>8.3f} {old:>8.3f}{marker}")
    sum_ndcg += n
    sum_mrr += m
    sum_p5 += p
    count += 1

print("-" * 85)
print(f"{'AVERAGE':<22} {'':>5} {'':>6} {sum_ndcg/count:>8.3f} {sum_mrr/count:>8.3f} {sum_p5/count:>8.3f} {sum(old_baselines.values())/len(old_baselines):>8.3f}")
print(f"\nImproved: {improved}, Regressed: {regressed}, Same: {same}")
print(f"Old avg nDCG@5: {sum(old_baselines.values())/len(old_baselines):.3f}")
print(f"New avg nDCG@5: {sum_ndcg/count:.3f}")
print(f"Delta: {sum_ndcg/count - sum(old_baselines.values())/len(old_baselines):+.3f}")
