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
| Avg legacy parse time | 601.3ms |
| Avg hybrid parse time | 333.1ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Hacker News | 340ms | 34KB | 1015ms | 1426ms | 10 | 10 | PASS | PASS |
| 2 | HN Newest | 302ms | 40KB | 587ms | 78ms | 10 | 4 | MISS | PASS |
| 3 | Lobsters | 472ms | 57KB | 940ms | 451ms | 10 | 10 | PASS | PASS |
| 4 | CNN Lite | 752ms | 330KB | 1113ms | 82ms | 10 | 3 | MISS | PASS |
| 5 | NPR Text | 434ms | 5KB | 1047ms | 2ms | 10 | 5 | PASS | PASS |
| 6 | Rust Lang | 884ms | 18KB | 949ms | 566ms | 10 | 10 | PASS | PASS |
| 7 | MDN HTML | 1864ms | 173KB | 876ms | 634ms | 10 | 10 | PASS | PASS |
| 8 | Python.org | 386ms | 47KB | 39ms | 39ms | 0 | 0 | MISS | MISS |
| 9 | W3C | 289ms | 50KB | 37ms | 1ms | 0 | 0 | MISS | MISS |
| 10 | GitHub Explore | 1113ms | 368KB | 1025ms | 659ms | 10 | 10 | PASS | PASS |
| 11 | NPM | 154ms | 28KB | 355ms | 39ms | 10 | 8 | PASS | PASS |
| 12 | Crates.io | 113ms | 3KB | 81ms | 37ms | 1 | 1 | PASS | PASS |
| 13 | PyPI | 51ms | 21KB | 440ms | 39ms | 10 | 7 | PASS | PASS |
| 14 | docs.rs | 265ms | 16KB | 843ms | 220ms | 10 | 10 | PASS | PASS |
| 15 | pkg.go.dev | 82ms | 32KB | 794ms | 184ms | 10 | 5 | PASS | PASS |
| 16 | Docker Hub | 275ms | 388KB | 536ms | 221ms | 10 | 10 | PASS | PASS |
| 17 | DuckDuckGo | 255ms | 157KB | 298ms | 1839ms | 10 | 10 | PASS | PASS |
| 18 | OpenStreetMap | 897ms | 32KB | 906ms | 74ms | 10 | 3 | PASS | PASS |
| 19 | httpbin HTML | 369ms | 3KB | 107ms | 36ms | 3 | 2 | PASS | PASS |
| 20 | Reuters | 239ms | 0KB | 38ms | 36ms | 1 | 1 | PASS | PASS |

## Hybrid Pipeline Stage Breakdown

| Site | BM25 build | HDC build | BM25 query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Hacker News | 1180µs | 7331µs | 13µs | 39µs | 1414115µs | 1422827µs | 0 | 80 |
| HN Newest | 1117µs | 7215µs | 72µs | 10µs | 65936µs | 74491µs | 14 | 5 |
| Lobsters | 1190µs | 7046µs | 10µs | 21µs | 438659µs | 447091µs | 25 | 22 |
| CNN Lite | 1198µs | 7431µs | 4µs | 15µs | 70850µs | 79951µs | 4 | 3 |
| NPR Text | 307µs | 1765µs | 5µs | 16µs | 13µs | 2131µs | 9 | 5 |
| Rust Lang | 632µs | 3299µs | 12µs | 20µs | 524694µs | 528706µs | 42 | 32 |
| MDN HTML | 2364µs | 12996µs | 32µs | 82µs | 573923µs | 590068µs | 140 | 60 |
| Python.org | 1µs | 0µs | 2µs | 16µs | 0µs | 91µs | 0 | 0 |
| W3C | 0µs | 0µs | 1µs | 13µs | 0µs | 83µs | 0 | 0 |
| GitHub Explore | 2329µs | 13896µs | 16µs | 23µs | 622882µs | 639930µs | 55 | 38 |
| NPM | 300µs | 1732µs | 7µs | 13µs | 74µs | 2181µs | 16 | 16 |
| Crates.io | 12µs | 20µs | 4µs | 14µs | 6µs | 67µs | 1 | 1 |
| PyPI | 230µs | 1040µs | 7µs | 12µs | 26µs | 1403µs | 12 | 12 |
| docs.rs | 491µs | 2189µs | 10µs | 16µs | 179901µs | 182666µs | 24 | 24 |
| pkg.go.dev | 712µs | 3917µs | 5µs | 19µs | 141572µs | 146463µs | 14 | 7 |
| Docker Hub | 784µs | 4869µs | 11µs | 17µs | 208833µs | 215052µs | 38 | 37 |
| DuckDuckGo | 5582µs | 38728µs | 42µs | 78µs | 1788427µs | 1833485µs | 135 | 60 |
| OpenStreetMap | 278µs | 1469µs | 4µs | 20µs | 70156µs | 72029µs | 4 | 4 |
| httpbin HTML | 79µs | 234µs | 4µs | 11µs | 11µs | 351µs | 3 | 3 |
| Reuters | 11µs | 32µs | 2µs | 11µs | 36331µs | 36394µs | 0 | 1 |

## Top-3 Node Quality Comparison

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.354` Hacker News
2. `0.313` Show HN: Raincast – Describe an app, get a native desktop app (open source)
3. `0.313` historytoday.com

**Hybrid top 3:**
1. `0.212` Hacker News new | past | comments | ask | show | jobs | submit login
2. `0.201` 25 points by DavidCanHelp 4 hours ago | hide | 1 comment
3. `0.199` 9 points by samizdis 2 hours ago | hide | discuss

---

### HN Newest — "newest submissions" 

**Legacy top 3:**
1. `0.424` submit
2. `0.298` 1 minute ago
3. `0.297` 2 minutes ago

**Hybrid top 3:**
1. `0.443` Hacker News new | past | comments | ask | show | jobs | submit
2. `0.375` new
3. `0.083` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 

---

### Lobsters — "programming stories and discussions" 

**Legacy top 3:**
1. `0.375` Your job isn't programming
2. `0.345` Lobsters Active Recent Comments Search Login Login 123 copilot edited an ad into
3. `0.325` Active Recent Comments Search Login Login 123 copilot edited an ad into my pr vi

**Hybrid top 3:**
1. `0.704` Programming language theory, types, design
2. `0.683` Your job isn't programming
3. `0.639` ask programming

---

### CNN Lite — "top news headlines today" 

**Legacy top 3:**
1. `0.305` Trump ramps up threats, oil tanker struck: What we know on Day 32 of the US and 
2. `0.272` Trump allowed a Russian oil tanker to reach Cuba, breaking the island’s fuel blo
3. `0.268` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94

**Hybrid top 3:**
1. `0.599` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94
2. `0.514` Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery
3. `0.361` Breaking News, Latest News and Videos | CNN CNN 3/31/2026 Latest Stories Why the

---

### NPR Text — "latest radio news stories" 

**Legacy top 3:**
1. `0.504` NPR : National Public Radio
2. `0.489` News
3. `0.425` Topics News Culture Music

**Hybrid top 3:**
1. `0.794` NPR : National Public Radio
2. `0.595` NPR : National Public Radio Tuesday, March 31, 2026 Iran's strike wounded over a
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
2. `0.684` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus
3. `0.650` Rust website The Book Standard Library API Reference Rust by Example The Cargo G

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
1. `0.172` reuters.com Please enable JS and disable any ad blocker

---

