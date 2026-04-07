# CRFR Honest Manual Evaluation — BBC News & NPR

**Date**: 2026-04-06  
**Method**: Manual MCP tool calls (parse_crfr + crfr_feedback), 10 iterations per site  
**Protocol**: Q1=baseline, Q2-Q7=train+feedback, Q8-Q10=test (no feedback)  
**Server**: Production MCP (pre-v18, does NOT include today's code changes)

---

## BBC News (https://www.bbc.com/news)

### Per-iteration results

| Iter | Phase | Goal | #1 Node | #2 Node | Articles in top-5 | Causal boost |
|------|-------|------|---------|---------|--------------------|-------------|
| 1 | BL | latest news headlines today | OG metadata tag | **Measles Bangladesh** | **3/6** | 0.0 |
| 2 | TR | breaking news stories right now | OG metadata tag | "More Top Stories" | **3/6** | 0.0 |
| 3 | TR | top news articles today | **Measles Bangladesh** | OG metadata tag | **3/5** | 0.0 |
| 4 | TR | current world news updates | OG metadata tag | "Middle East" link | **1/4** | 0.0 |
| 5 | TR | major news events happening now | "More Top Stories" | **Measles Bangladesh** | **1/3** | 0.0 |
| 6 | TR | important world headlines today | Nav region list | OG metadata tag | **2/5** | 0.0 |
| 7 | TR | todays top news stories | OG metadata tag | **Measles Bangladesh** | **2/3** | 0.0 |
| **8** | **TE** | what is happening in the world right now | Empty node | **Hormuz/Iran war** | **1/5** | **0.0** |
| **9** | **TE** | global news and current events | **Measles Bangladesh** | OG metadata tag | **2/3** | **0.0** |
| **10** | **TE** | recent major world developments | Nav region list | OG metadata tag | **2/5** | **0.0** |

### Actual articles found across all iterations

| Node ID | Content | Times appeared | First rank |
|---------|---------|---------------|------------|
| 742 | "Emergency jabs after 100 children die of suspected measles in Bangladesh" | 7/10 | #1 (iter 3) |
| 734 | "Young gray whale dies after swimming up river in Washington state" | 4/10 | #3 (iter 1) |
| 774 | "Seven Eritrean players fail to return home after international match" | 3/10 | #4 (iter 1) |
| 498 | "Artemis II astronauts have toilet trouble on their way towards Moon" | 2/10 | #3 (iter 6) |
| 692 | "Why the Strait of Hormuz matters so much in the Iran war" | 1/10 | #2 (iter 8) |
| 271 | "'See you on the other side': Artemis II astronauts lose contact" | 1/10 | #4 (iter 10) |

### Critical observation

**causal_boost = 0.0 on ALL 10 iterations** despite 5 feedback calls with 11 total node IDs.

Reason: Each goal variation lands in a different goal cluster:
- "latest news headlines" → cluster `headlines+latest+news`
- "breaking news stories" → cluster `breaking+news+stories`
- "top news articles" → cluster `articles+news+top`

Feedback in one cluster does NOT transfer to another. Suppression learning requires 3+ iterations in the SAME cluster — but we switch goals every iteration.

### What actually drives ranking

Pure BM25 keyword matching. When the goal contains words that happen to appear in article text ("articles", "top", "stories"), real content ranks higher. When the goal uses abstract words ("happening", "world", "developments"), nav elements and metadata win.

---

## NPR (https://www.npr.org/)

### Per-iteration results

| Iter | Phase | Goal | #1 Node | Articles in top-5 | Causal |
|------|-------|------|---------|--------------------|--------|
| 1 | BL | latest news stories today | "Listen · 5:18" (play button) | **0/2** | 0.0 |
| 2 | TR | breaking news headlines now | **"Christians under siege" (article!)** | **1/3** | 0.0 |
| 3 | TR | top articles published today | **"Christians under siege"** | **1/2** | **0.078** |
| 4 | TR | important current affairs | "Listen · 5:18" | **0/5** | 0.0 |
| 5 | TR | major stories happening right now | Embed iframe code | **0/2** | 0.0 |
| 6 | TR | key news developments today | "Listen · 5:18" | **0/2** | 0.0 |
| 7 | TR | Trump tariffs Congress Lebanon Israel... | "Listen · 3:50" | **1/6** (Trump photo #4) | 0.0 |
| **8** | **TE** | what are todays biggest news stories | "Listen · 5:18" | **1/6** ("Blind students" heading #4) | 0.0 |
| **9** | **TE** | current affairs and global events | "Life Kit" (podcast section) | **0/1** | 0.0 |
| **10** | **TE** | recent notable world happenings | "Life Kit" ×5 (all duplicates) | **0/5** | 0.0 |

### Why NPR fails

1. **Audio-dominant DOM**: NPR's page structure is built around audio players. "Listen", "Download", "Transcript", "Embed" buttons have high BM25 scores for many generic query words.

2. **1229 nodes, ~5 real articles**: The article headings exist deep in the DOM but score below the BM25 pre-filter threshold. They only surface when goal contains specific topic words (iter 7: "Trump", "Lebanon").

3. **Causal learning fired once** (iter 3, boost 0.078) — then the article disappeared from results because the next goal's BM25 profile didn't match it.

---

## Honest Assessment

### What works
- **BM25 cold-start on BBC**: When query words match article text, articles rank in top-5
- **BBC article diversity**: 6 different articles surfaced across 10 iterations
- **Structural content**: BBC article nodes have rich text (100+ chars) that BM25 matches well

### What's broken

1. **Causal learning doesn't generalize across goal phrasings** — goal clustering prevents it
2. **Suppression learning can't fire** — needs 3 iterations in same cluster, but protocol varies goals
3. **NPR's DOM defeats BM25** — audio player UI elements outscore article headings
4. **OG metadata tags consistently outrank content** — `openGraph.title` has the highest BM25 match for any news query

### What v15-v18 changes would fix (once deployed)

| Change | Effect |
|--------|--------|
| Structural cascade bypass | Would include NPR article headings in scoring (currently excluded by depth) |
| DCFR/LCFR discounting | Would help IF same cluster queried repeatedly |
| RBP pruning | Would skip "Listen"/"Download" subtrees after learning |
| MCCFR sampling | Limits cascade to 300, reducing noise on NPR (1229 nodes) |

### What v15-v18 would NOT fix

- Goal clustering fragmentation (each phrasing = different cluster)
- OG metadata dominance (BM25 weight 0.75 means metadata always wins)
- Single-shot cold-start performance (no feedback → no learning)
