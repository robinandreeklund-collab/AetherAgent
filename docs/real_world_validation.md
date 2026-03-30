# Real-World Validation — Hybrid Scoring Pipeline

**Date:** 2026-03-30
**Mode:** Release build, WITH embeddings (all-MiniLM-L6-v2, 384-dim)
**Method:** Fetch → Legacy parse_top_nodes → Hybrid parse_top_nodes_hybrid

## Summary

| Metric | Value |
|--------|-------|
| Sites tested | 20 |
| Successfully fetched | 20 |
| Legacy correctness (keyword in top 3) | 16/20 (80%) |
| Hybrid correctness (keyword in top 3) | 18/20 (90%) |
| Avg legacy parse time | 581.0ms |
| Avg hybrid parse time | 319.1ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Hacker News | 273ms | 34KB | 1029ms | 1373ms | 10 | 10 | PASS | PASS |
| 2 | HN Newest | 260ms | 40KB | 611ms | 117ms | 10 | 4 | MISS | PASS |
| 3 | Lobsters | 701ms | 57KB | 903ms | 404ms | 10 | 10 | PASS | PASS |
| 4 | CNN Lite | 294ms | 326KB | 1072ms | 88ms | 10 | 3 | MISS | PASS |
| 5 | NPR Text | 237ms | 5KB | 998ms | 4ms | 10 | 5 | PASS | PASS |
| 6 | Rust Lang | 782ms | 18KB | 910ms | 530ms | 10 | 10 | PASS | PASS |
| 7 | MDN HTML | 379ms | 173KB | 846ms | 615ms | 10 | 10 | PASS | PASS |
| 8 | Python.org | 284ms | 48KB | 39ms | 38ms | 0 | 0 | MISS | MISS |
| 9 | W3C | 335ms | 50KB | 38ms | 1ms | 0 | 0 | MISS | MISS |
| 10 | GitHub Explore | 1128ms | 368KB | 971ms | 617ms | 10 | 10 | PASS | PASS |
| 11 | NPM | 146ms | 28KB | 337ms | 39ms | 10 | 8 | PASS | PASS |
| 12 | Crates.io | 174ms | 3KB | 74ms | 36ms | 1 | 1 | PASS | PASS |
| 13 | PyPI | 47ms | 21KB | 425ms | 38ms | 10 | 7 | PASS | PASS |
| 14 | docs.rs | 149ms | 17KB | 813ms | 174ms | 10 | 10 | PASS | PASS |
| 15 | pkg.go.dev | 90ms | 32KB | 761ms | 183ms | 10 | 5 | PASS | PASS |
| 16 | Docker Hub | 283ms | 387KB | 526ms | 220ms | 10 | 10 | PASS | PASS |
| 17 | DuckDuckGo | 133ms | 157KB | 283ms | 1761ms | 10 | 10 | PASS | PASS |
| 18 | OpenStreetMap | 785ms | 32KB | 842ms | 72ms | 10 | 3 | PASS | PASS |
| 19 | httpbin HTML | 143ms | 3KB | 106ms | 36ms | 3 | 2 | PASS | PASS |
| 20 | Reuters | 217ms | 0KB | 37ms | 36ms | 1 | 1 | PASS | PASS |

## Hybrid Pipeline Stage Breakdown

| Site | BM25 build | HDC build | BM25 query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Hacker News | 1130µs | 14795µs | 16µs | 72µs | 1353621µs | 1369966µs | 0 | 80 |
| HN Newest | 1178µs | 15477µs | 32µs | 9µs | 96756µs | 113601µs | 14 | 5 |
| Lobsters | 1132µs | 14075µs | 9µs | 37µs | 384823µs | 400235µs | 24 | 21 |
| CNN Lite | 1217µs | 15683µs | 5µs | 29µs | 68664µs | 86058µs | 4 | 3 |
| NPR Text | 302µs | 3637µs | 5µs | 27µs | 13µs | 4012µs | 9 | 5 |
| Rust Lang | 603µs | 6925µs | 12µs | 37µs | 487936µs | 495560µs | 42 | 32 |
| MDN HTML | 2422µs | 26817µs | 38µs | 86µs | 543012µs | 573072µs | 140 | 60 |
| Python.org | 1µs | 0µs | 2µs | 31µs | 0µs | 124µs | 0 | 0 |
| W3C | 0µs | 0µs | 2µs | 24µs | 0µs | 94µs | 0 | 0 |
| GitHub Explore | 2327µs | 28710µs | 17µs | 25µs | 566119µs | 597874µs | 55 | 38 |
| NPM | 346µs | 3574µs | 6µs | 24µs | 43µs | 4044µs | 16 | 16 |
| Crates.io | 11µs | 36µs | 3µs | 26µs | 5µs | 92µs | 1 | 1 |
| PyPI | 222µs | 2207µs | 6µs | 20µs | 26µs | 2563µs | 12 | 12 |
| docs.rs | 458µs | 4548µs | 9µs | 24µs | 133979µs | 139071µs | 22 | 22 |
| pkg.go.dev | 679µs | 7954µs | 5µs | 34µs | 137468µs | 146264µs | 14 | 7 |
| Docker Hub | 837µs | 10305µs | 11µs | 28µs | 202246µs | 213948µs | 38 | 37 |
| DuckDuckGo | 5816µs | 82567µs | 36µs | 97µs | 1666799µs | 1755950µs | 135 | 60 |
| OpenStreetMap | 276µs | 3132µs | 5µs | 28µs | 67519µs | 71110µs | 4 | 4 |
| httpbin HTML | 83µs | 471µs | 4µs | 18µs | 10µs | 600µs | 3 | 3 |
| Reuters | 10µs | 54µs | 1µs | 18µs | 35963µs | 36052µs | 0 | 1 |

## Top-3 Node Quality Comparison

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.354` Hacker News
2. `0.297` past
3. `0.297` Do your own writing

**Hybrid top 3:**
1. `0.215` Hacker News new | past | comments | ask | show | jobs | submit login
2. `0.205` 40 points by maurycyz 4 hours ago | hide | 16 comments
3. `0.204` 10 points by bushido 1 hour ago | hide | 5 comments

---

### HN Newest — "newest submissions" 

**Legacy top 3:**
1. `0.424` submit
2. `0.302` 9 minutes ago
3. `0.297` 2 minutes ago

**Hybrid top 3:**
1. `0.445` Hacker News new | past | comments | ask | show | jobs | submit
2. `0.375` new
3. `0.102` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 

---

### Lobsters — "programming stories and discussions" 

**Legacy top 3:**
1. `0.375` Your job isn't programming
2. `0.345` Lobsters Active Recent Comments Search Login Login 105 copilot edited an ad into
3. `0.325` Active Recent Comments Search Login Login 105 copilot edited an ad into my pr vi

**Hybrid top 3:**
1. `0.683` Your job isn't programming
2. `0.639` ask programming
3. `0.555` Stories about particular persons

---

### CNN Lite — "top news headlines today" 

**Legacy top 3:**
1. `0.292` What we know on Day 31 of the US and Israel’s war with Iran: Trump threatens esc
2. `0.272` Trump allowed a Russian oil tanker to reach Cuba, breaking the island’s fuel blo
3. `0.268` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94

**Hybrid top 3:**
1. `0.599` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94
2. `0.514` Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery
3. `0.374` Breaking News, Latest News and Videos | CNN CNN 3/30/2026 Latest Stories As Trum

---

### NPR Text — "latest radio news stories" 

**Legacy top 3:**
1. `0.504` NPR : National Public Radio
2. `0.489` News
3. `0.425` Topics News Culture Music

**Hybrid top 3:**
1. `0.794` NPR : National Public Radio
2. `0.595` NPR : National Public Radio Monday, March 30, 2026 Watch: Who is an American? Th
3. `0.527` News

---

### Rust Lang — "latest Rust version download" 

**Legacy top 3:**
1. `0.372` Rust A language empowering everyone to build reliable and efficient software. Ge
2. `0.328` Rust Logo
3. `0.313` In 2018, the Rust community decided to improve the programming experience
for a 

**Hybrid top 3:**
1. `0.548` Read Rust
2. `0.538` Watch Rust
3. `0.411` Read Rust We love documentation! Take a look at the books available online, as w

---

### MDN HTML — "HTML elements reference" 

**Legacy top 3:**
1. `0.620` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu
2. `0.560` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
3. `0.540` HTML: HyperText Markup Language | MDN Skip to main content Skip to search MDN HT

**Hybrid top 3:**
1. `0.825` Reference for all HTML elements .
2. `0.825` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
3. `0.825` HTML reference Elements Global attributes Attributes See all… HTML guides Respon

---

### Python.org — "download Python latest version" 

**Legacy top 3:**

**Hybrid top 3:**

---

### W3C — "web standards specifications" 

**Legacy top 3:**

**Hybrid top 3:**

---

### GitHub Explore — "trending repositories" 

**Legacy top 3:**
1. `0.505` Open Source COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Secur
2. `0.505` COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Main
3. `0.505` REPOSITORIES Topics Trending Collections

**Hybrid top 3:**
1. `0.887` Trending repository
2. `0.825` REPOSITORIES Topics Trending Collections
3. `0.825` COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Main

---

### NPM — "search JavaScript packages" 

**Legacy top 3:**
1. `0.322` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 
2. `0.322` Relied upon by more than 17 million developers worldwide, npm is committed to ma
3. `0.292` Take your JavaScript development up a notch

**Hybrid top 3:**
1. `0.675` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 
2. `0.647` Relied upon by more than 17 million developers worldwide, npm is committed to ma
3. `0.506` We're GitHub, the company behind the npm Registry and npm CLI. We offer those to

---

### Crates.io — "Rust package registry search" 

**Legacy top 3:**
1. `0.520` crates.io: Rust Package Registry

**Hybrid top 3:**
1. `0.504` crates.io: Rust Package Registry

---

### PyPI — "find Python packages" 

**Legacy top 3:**
1. `0.660` Find, install and publish Python packages with the Python Package Index
2. `0.595` Find, install and publish Python packages with the Python Package Index Search P
3. `0.595` The Python Package Index (PyPI) is a repository of software for the Python progr

**Hybrid top 3:**
1. `0.945` Find, install and publish Python packages with the Python Package Index
2. `0.807` Find, install and publish Python packages with the Python Package Index Search P
3. `0.787` PyPI helps you find and install software developed and shared by the Python comm

---

### docs.rs — "Rust documentation search" 

**Legacy top 3:**
1. `0.620` Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Down
2. `0.600` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus
3. `0.496` Rust website

**Hybrid top 3:**
1. `0.758` Rust Rust website The Book Standard Library API Reference Rust by Example The Ca
2. `0.710` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus
3. `0.684` Rust website

---

### pkg.go.dev — "Go packages and modules" 

**Legacy top 3:**
1. `0.563` Why Go Why Go Case Studies Use Cases Security Learn Docs Docs Effective Go Go Us
2. `0.483` Go Packages - Go Packages Skip to Main Content Why Go Case Studies Common proble
3. `0.458` Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Pa

**Hybrid top 3:**
1. `0.704` Packages Standard Library Sub-repositories About Go Packages
2. `0.621` About Go Packages
3. `0.528` Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Pa

---

### Docker Hub — "search container images" 

**Legacy top 3:**
1. `0.540` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.352` Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System st
3. `0.337` Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System st

**Hybrid top 3:**
1. `0.600` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.575` Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System st
3. `0.514` Docker Hardened Images - Now Free

---

### DuckDuckGo — "search engine privacy" 

**Legacy top 3:**
1. `0.715` translations.messages.R2RXjF[0].value: These browser permissions are used to add
2. `0.715` translations.messages.qUwfSi[2].value: . And when you leave our search engine an
3. `0.493` translations.messages.+bs8cY[0].value: Default search engine

**Hybrid top 3:**
1. `0.960` translations.messages.R2RXjF[0].value: These browser permissions are used to add
2. `0.780` translations.messages.unqgWN[0].value: Search Engine
3. `0.780` translations.messages.bTWzmJ[0].value: Search engine

---

### OpenStreetMap — "map navigation and editing" 

**Legacy top 3:**
1. `0.360` OpenStreetMap
2. `0.340` Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! Op
3. `0.321` Edit

**Hybrid top 3:**
1. `0.512` OpenStreetMap is a map of the world, created by people like you and free to use 
2. `0.421` Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people
3. `0.402` Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! Op

---

### httpbin HTML — "Herman Melville story" 

**Legacy top 3:**
1. `0.598` Herman Melville - Moby-Dick
2. `0.437` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.417` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

**Hybrid top 3:**
1. `0.568` Herman Melville - Moby-Dick
2. `0.426` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

---

### Reuters — "business news today" 

**Legacy top 3:**
1. `0.070` reuters.com Please enable JS and disable any ad blocker

**Hybrid top 3:**
1. `0.171` reuters.com Please enable JS and disable any ad blocker

---

