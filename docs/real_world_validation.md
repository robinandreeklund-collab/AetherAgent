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
| Hybrid correctness (keyword in top 3) | 16/20 (80%) |
| Avg legacy parse time | 524.5ms |
| Avg hybrid parse time | 246.2ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Hacker News | 332ms | 33KB | 933ms | 1233ms | 10 | 10 | PASS | PASS |
| 2 | HN Newest | 286ms | 40KB | 480ms | 206ms | 10 | 6 | PASS | PASS |
| 3 | Lobsters | 2621ms | 57KB | 848ms | 266ms | 10 | 10 | PASS | PASS |
| 4 | CNN Lite | 355ms | 330KB | 960ms | 112ms | 10 | 4 | MISS | PASS |
| 5 | NPR Text | 1162ms | 5KB | 934ms | 15ms | 10 | 5 | PASS | PASS |
| 6 | Rust Lang | 840ms | 18KB | 828ms | 516ms | 10 | 10 | PASS | PASS |
| 7 | MDN HTML | 278ms | 173KB | 781ms | 652ms | 10 | 10 | PASS | PASS |
| 8 | Python.org | 421ms | 47KB | 37ms | 36ms | 0 | 0 | MISS | MISS |
| 9 | W3C | 474ms | 50KB | 36ms | 1ms | 0 | 0 | MISS | MISS |
| 10 | GitHub Explore | 1653ms | 395KB | 901ms | 651ms | 10 | 10 | PASS | PASS |
| 11 | NPM | 159ms | 6KB | 34ms | 67ms | 1 | 1 | MISS | MISS |
| 12 | Crates.io | 267ms | 3KB | 78ms | 36ms | 1 | 1 | PASS | PASS |
| 13 | PyPI | 50ms | 21KB | 403ms | 43ms | 10 | 7 | PASS | PASS |
| 14 | docs.rs | 283ms | 17KB | 744ms | 151ms | 10 | 10 | PASS | PASS |
| 15 | pkg.go.dev | 118ms | 32KB | 765ms | 194ms | 10 | 7 | PASS | PASS |
| 16 | Docker Hub | 419ms | 387KB | 541ms | 320ms | 10 | 10 | PASS | PASS |
| 17 | DuckDuckGo | 201ms | 157KB | 266ms | 282ms | 10 | 10 | PASS | MISS |
| 18 | OpenStreetMap | 1209ms | 32KB | 784ms | 72ms | 10 | 5 | PASS | PASS |
| 19 | httpbin HTML | 203ms | 3KB | 100ms | 36ms | 3 | 2 | PASS | PASS |
| 20 | Reuters | 510ms | 0KB | 37ms | 35ms | 1 | 1 | PASS | PASS |

## Hybrid Pipeline Stage Breakdown

| Site | BM25 build | HDC build | BM25 query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Hacker News | 1371µs | 44184µs | 17µs | 116µs | 1183803µs | 1229624µs | 0 | 80 |
| HN Newest | 1449µs | 48115µs | 31µs | 16µs | 151598µs | 201371µs | 18 | 7 |
| Lobsters | 1397µs | 37670µs | 8µs | 86µs | 222538µs | 261870µs | 21 | 18 |
| CNN Lite | 1534µs | 45028µs | 7µs | 65µs | 62697µs | 109877µs | 6 | 4 |
| NPR Text | 432µs | 13840µs | 6µs | 78µs | 27µs | 14408µs | 9 | 5 |
| Rust Lang | 826µs | 28161µs | 9µs | 67µs | 454522µs | 483634µs | 42 | 32 |
| MDN HTML | 3006µs | 96131µs | 25µs | 116µs | 511091µs | 610823µs | 140 | 60 |
| Python.org | 1µs | 0µs | 2µs | 67µs | 1µs | 143µs | 0 | 0 |
| W3C | 1µs | 0µs | 2µs | 46µs | 1µs | 125µs | 0 | 0 |
| GitHub Explore | 3006µs | 98418µs | 13µs | 35µs | 524843µs | 627009µs | 63 | 42 |
| NPM | 8µs | 9µs | 2µs | 47µs | 33658µs | 33759µs | 0 | 1 |
| Crates.io | 11µs | 80µs | 3µs | 67µs | 8µs | 179µs | 1 | 1 |
| PyPI | 259µs | 9165µs | 5µs | 48µs | 45µs | 9568µs | 12 | 12 |
| docs.rs | 542µs | 16545µs | 8µs | 61µs | 99502µs | 116725µs | 20 | 20 |
| pkg.go.dev | 891µs | 27890µs | 7µs | 67µs | 128361µs | 157328µs | 20 | 10 |
| Docker Hub | 1161µs | 46216µs | 13µs | 54µs | 265190µs | 313243µs | 43 | 42 |
| DuckDuckGo | 6308µs | 200302µs | 25µs | 138µs | 69089µs | 276389µs | 135 | 60 |
| OpenStreetMap | 297µs | 9957µs | 5µs | 66µs | 59690µs | 70098µs | 6 | 6 |
| httpbin HTML | 120µs | 3201µs | 4µs | 45µs | 18µs | 3400µs | 3 | 3 |
| Reuters | 12µs | 147µs | 1µs | 46µs | 34746µs | 34959µs | 0 | 1 |

## Top-3 Node Quality Comparison

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.354` Hacker News
2. `0.297` past
3. `0.282` jobs

**Hybrid top 3:**
1. `0.214` Hacker News new | past | comments | ask | show | jobs | submit login
2. `0.207` 72 points by bookofjoe 8 hours ago | hide | 41 comments
3. `0.195` 111 points by radimm 9 hours ago | hide | 28 comments

---

### HN Newest — "newest submissions" 

**Legacy top 3:**
1. `0.424` submit
2. `0.296` Hacker News
3. `0.293` 3 minutes ago

**Hybrid top 3:**
1. `0.441` Hacker News new | past | comments | ask | show | jobs | submit
2. `0.380` New fibre optic record allows 50M movies to be streamed at once
3. `0.375` new

---

### Lobsters — "programming stories and discussions" 

**Legacy top 3:**
1. `0.375` Your job isn't programming
2. `0.315` Your job isn't programming
3. `0.296` Development and team practices

**Hybrid top 3:**
1. `0.683` Your job isn't programming
2. `0.663` Programming language theory, types, design
3. `0.639` ask programming

---

### CNN Lite — "top news headlines today" 

**Legacy top 3:**
1. `0.283` What we know on Day 32 of the US and Israel’s war with Iran: Gas prices skyrocke
2. `0.276` It’s been a volatile month on Wall Street
3. `0.276` The pace of hiring just fell to the lowest since 2011, outside of the pandemic

**Hybrid top 3:**
1. `0.563` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94
2. `0.563` Behind the scenes and in front of cameras, Hegseth serving as top cheerleader fo
3. `0.514` Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery

---

### NPR Text — "latest radio news stories" 

**Legacy top 3:**
1. `0.504` NPR : National Public Radio
2. `0.489` News
3. `0.425` Topics News Culture Music

**Hybrid top 3:**
1. `0.794` NPR : National Public Radio
2. `0.595` NPR : National Public Radio Tuesday, March 31, 2026 How Trump's EEOC is attackin
3. `0.489` News

---

### Rust Lang — "latest Rust version download" 

**Legacy top 3:**
1. `0.328` Rust Logo
2. `0.313` In 2018, the Rust community decided to improve the programming experience
for a 
3. `0.306` Install

**Hybrid top 3:**
1. `0.550` Read Rust
2. `0.540` Watch Rust
3. `0.414` Rust in production Hundreds of companies around the world are using Rust in prod

---

### MDN HTML — "HTML elements reference" 

**Legacy top 3:**
1. `0.550` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu
2. `0.515` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
3. `0.515` HTML consists of elements , each of which may be modified by some number of attr

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
1. `0.070` Just a moment...

**Hybrid top 3:**
1. `0.087` Just a moment...

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
3. `0.575` Find, install and publish Python packages with the Python Package Index Search P

**Hybrid top 3:**
1. `0.945` Find, install and publish Python packages with the Python Package Index
2. `0.825` Find, install and publish Python packages with the Python Package Index Search P
3. `0.807` PyPI helps you find and install software developed and shared by the Python comm

---

### docs.rs — "Rust documentation search" 

**Legacy top 3:**
1. `0.496` Rust website
2. `0.470` Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Down
3. `0.462` Rust by Example

**Hybrid top 3:**
1. `0.758` Rust Rust website The Book Standard Library API Reference Rust by Example The Ca
2. `0.725` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus
3. `0.691` Rust website

---

### pkg.go.dev — "Go packages and modules" 

**Legacy top 3:**
1. `0.563` Why Go Why Go Case Studies Use Cases Security Learn Docs Docs Effective Go Go Us
2. `0.458` Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Pa
3. `0.455` About Go Packages

**Hybrid top 3:**
1. `0.704` Packages Standard Library Sub-repositories About Go Packages
2. `0.577` About Go Packages
3. `0.528` Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Pa

---

### Docker Hub — "search container images" 

**Legacy top 3:**
1. `0.470` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.313` Software supply chain Secure Your Supply Chain with Docker Hardened Images Use D
3. `0.313` Seamlessly ship any application, anywhere Push images and make your app accessib

**Hybrid top 3:**
1. `0.600` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.504` Docker Hardened Images - Now Free
3. `0.488` Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System st

---

### DuckDuckGo — "search engine privacy" 

**Legacy top 3:**
1. `0.715` translations.messages.R2RXjF[0].value: These browser permissions are used to add
2. `0.715` translations.messages.qUwfSi[2].value: . And when you leave our search engine an
3. `0.493` translations.messages.+bs8cY[0].value: Default search engine

**Hybrid top 3:**
1. `0.550` DuckDuckGo - Protection. Privacy. Peace of mind. Duck.ai Main navigation menu cl
2. `0.440` bootstrapTheme: false
3. `0.439` homepageDomain: null

---

### OpenStreetMap — "map navigation and editing" 

**Legacy top 3:**
1. `0.360` OpenStreetMap
2. `0.321` Edit
3. `0.320` Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! Op

**Hybrid top 3:**
1. `0.499` OpenStreetMap is a map of the world, created by people like you and free to use 
2. `0.415` Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people
3. `0.398` Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! Op

---

### httpbin HTML — "Herman Melville story" 

**Legacy top 3:**
1. `0.598` Herman Melville - Moby-Dick
2. `0.287` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.267` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

**Hybrid top 3:**
1. `0.570` Herman Melville - Moby-Dick
2. `0.427` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

---

### Reuters — "business news today" 

**Legacy top 3:**
1. `0.070` reuters.com Please enable JS and disable any ad blocker

**Hybrid top 3:**
1. `0.173` reuters.com Please enable JS and disable any ad blocker

---

