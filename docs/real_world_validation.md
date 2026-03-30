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
| Avg legacy parse time | 596.0ms |
| Avg hybrid parse time | 365.3ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Hacker News | 553ms | 34KB | 1043ms | 1442ms | 10 | 10 | PASS | PASS |
| 2 | HN Newest | 274ms | 40KB | 571ms | 288ms | 10 | 8 | MISS | PASS |
| 3 | Lobsters | 278ms | 57KB | 955ms | 455ms | 10 | 10 | PASS | PASS |
| 4 | CNN Lite | 324ms | 326KB | 1093ms | 115ms | 10 | 3 | MISS | PASS |
| 5 | NPR Text | 407ms | 5KB | 1036ms | 11ms | 10 | 5 | PASS | PASS |
| 6 | Rust Lang | 1178ms | 18KB | 927ms | 568ms | 10 | 10 | PASS | PASS |
| 7 | MDN HTML | 240ms | 173KB | 850ms | 712ms | 10 | 10 | PASS | PASS |
| 8 | Python.org | 120ms | 48KB | 38ms | 37ms | 0 | 0 | MISS | MISS |
| 9 | W3C | 1488ms | 50KB | 38ms | 1ms | 0 | 0 | MISS | MISS |
| 10 | GitHub Explore | 1157ms | 366KB | 1000ms | 736ms | 10 | 10 | PASS | PASS |
| 11 | NPM | 168ms | 28KB | 374ms | 52ms | 10 | 8 | PASS | PASS |
| 12 | Crates.io | 160ms | 3KB | 70ms | 37ms | 1 | 1 | PASS | PASS |
| 13 | PyPI | 54ms | 21KB | 431ms | 43ms | 10 | 7 | PASS | PASS |
| 14 | docs.rs | 341ms | 16KB | 839ms | 155ms | 10 | 10 | PASS | PASS |
| 15 | pkg.go.dev | 126ms | 32KB | 841ms | 213ms | 10 | 5 | PASS | PASS |
| 16 | Docker Hub | 300ms | 387KB | 535ms | 254ms | 10 | 10 | PASS | PASS |
| 17 | DuckDuckGo | 661ms | 157KB | 319ms | 2035ms | 10 | 10 | PASS | PASS |
| 18 | OpenStreetMap | 794ms | 32KB | 820ms | 80ms | 10 | 3 | PASS | PASS |
| 19 | httpbin HTML | 215ms | 3KB | 105ms | 38ms | 3 | 2 | PASS | PASS |
| 20 | Reuters | 377ms | 0KB | 36ms | 34ms | 1 | 1 | PASS | PASS |

## Hybrid Pipeline Stage Breakdown

| Site | BM25 build | HDC build | BM25 query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Hacker News | 1343µs | 39977µs | 19µs | 123µs | 1396337µs | 1437996µs | 0 | 80 |
| HN Newest | 1352µs | 42376µs | 45µs | 17µs | 239139µs | 283120µs | 22 | 9 |
| Lobsters | 1437µs | 39773µs | 15µs | 82µs | 409288µs | 450852µs | 24 | 21 |
| CNN Lite | 1578µs | 41127µs | 10µs | 79µs | 69676µs | 113071µs | 4 | 3 |
| NPR Text | 404µs | 10557µs | 6µs | 67µs | 18µs | 11080µs | 9 | 5 |
| Rust Lang | 760µs | 21695µs | 15µs | 78µs | 509976µs | 532605µs | 42 | 32 |
| MDN HTML | 3016µs | 82545µs | 44µs | 332µs | 576035µs | 662694µs | 140 | 60 |
| Python.org | 1µs | 0µs | 2µs | 72µs | 0µs | 149µs | 0 | 0 |
| W3C | 1µs | 0µs | 3µs | 83µs | 1µs | 177µs | 0 | 0 |
| GitHub Explore | 3053µs | 81824µs | 34µs | 93µs | 626053µs | 712033µs | 55 | 38 |
| NPM | 354µs | 14736µs | 7µs | 48µs | 52µs | 15256µs | 16 | 16 |
| Crates.io | 13µs | 82µs | 3µs | 65µs | 6µs | 180µs | 1 | 1 |
| PyPI | 231µs | 7057µs | 7µs | 49µs | 32µs | 7423µs | 12 | 12 |
| docs.rs | 665µs | 12405µs | 8µs | 50µs | 105555µs | 118745µs | 20 | 20 |
| pkg.go.dev | 840µs | 24477µs | 11µs | 106µs | 147506µs | 173090µs | 14 | 7 |
| Docker Hub | 938µs | 32083µs | 13µs | 63µs | 213290µs | 246983µs | 38 | 37 |
| DuckDuckGo | 7694µs | 223886µs | 58µs | 180µs | 1795418µs | 2028258µs | 135 | 60 |
| OpenStreetMap | 293µs | 8609µs | 4µs | 66µs | 69388µs | 78450µs | 4 | 4 |
| httpbin HTML | 80µs | 1813µs | 4µs | 44µs | 16µs | 1970µs | 3 | 3 |
| Reuters | 11µs | 143µs | 1µs | 45µs | 33918µs | 34125µs | 0 | 1 |

## Top-3 Node Quality Comparison

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.354` Hacker News
2. `0.297` past
3. `0.297` Do your own writing

**Hybrid top 3:**
1. `0.214` Hacker News new | past | comments | ask | show | jobs | submit login
2. `0.209` 43 points by maurycyz 5 hours ago | hide | 16 comments
3. `0.205` 103 points by taubek 3 hours ago | hide | 53 comments

---

### HN Newest — "newest submissions" 

**Legacy top 3:**
1. `0.424` submit
2. `0.298` 1 minute ago
3. `0.297` 2 minutes ago

**Hybrid top 3:**
1. `0.449` David Sacks' new role shaping Trump's AI agenda
2. `0.423` Hacker News new | past | comments | ask | show | jobs | submit
3. `0.387` What's New in Flutter 3.41

---

### Lobsters — "programming stories and discussions" 

**Legacy top 3:**
1. `0.375` Your job isn't programming
2. `0.345` Lobsters Active Recent Comments Search Login Login 111 copilot edited an ad into
3. `0.325` Active Recent Comments Search Login Login 111 copilot edited an ad into my pr vi

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
3. `0.265` Breaking News, Latest News and Videos | CNN CNN 3/30/2026 Latest Stories Student

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
2. `0.707` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus
3. `0.686` Rust website

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
1. `0.173` reuters.com Please enable JS and disable any ad blocker

---

