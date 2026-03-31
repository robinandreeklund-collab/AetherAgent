# Real-World Validation — Hybrid Scoring Pipeline

**Date:** 2026-03-30
**Mode:** Release build, no embeddings (text similarity only)
**Method:** Fetch → Legacy parse_top_nodes → Hybrid parse_top_nodes_hybrid

## Summary

| Metric | Value |
|--------|-------|
| Sites tested | 20 |
| Successfully fetched | 20 |
| Legacy correctness (keyword in top 3) | 17/20 (85%) |
| Hybrid correctness (keyword in top 3) | 17/20 (85%) |
| Avg legacy parse time | 3.6ms |
| Avg hybrid parse time | 43.0ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Hacker News | 271ms | 33KB | 6ms | 52ms | 10 | 10 | PASS | PASS |
| 2 | HN Newest | 276ms | 40KB | 4ms | 53ms | 10 | 4 | PASS | PASS |
| 3 | Lobsters | 536ms | 57KB | 4ms | 47ms | 10 | 10 | PASS | PASS |
| 4 | CNN Lite | 525ms | 330KB | 2ms | 51ms | 10 | 4 | PASS | PASS |
| 5 | NPR Text | 319ms | 5KB | 0ms | 14ms | 10 | 5 | PASS | PASS |
| 6 | Rust Lang | 858ms | 18KB | 1ms | 29ms | 10 | 10 | PASS | PASS |
| 7 | MDN HTML | 562ms | 173KB | 10ms | 110ms | 10 | 10 | PASS | PASS |
| 8 | Python.org | 345ms | 47KB | 1ms | 1ms | 0 | 0 | MISS | MISS |
| 9 | W3C | 286ms | 50KB | 1ms | 1ms | 0 | 0 | MISS | MISS |
| 10 | GitHub Explore | 1697ms | 392KB | 24ms | 132ms | 10 | 10 | PASS | PASS |
| 11 | NPM | 74ms | 28KB | 0ms | 23ms | 10 | 8 | PASS | PASS |
| 12 | Crates.io | 131ms | 3KB | 0ms | 0ms | 1 | 1 | PASS | PASS |
| 13 | PyPI | 52ms | 21KB | 1ms | 10ms | 10 | 7 | PASS | PASS |
| 14 | docs.rs | 306ms | 17KB | 1ms | 17ms | 10 | 10 | MISS | PASS |
| 15 | pkg.go.dev | 119ms | 32KB | 2ms | 31ms | 10 | 7 | PASS | PASS |
| 16 | Docker Hub | 331ms | 388KB | 7ms | 54ms | 10 | 10 | PASS | PASS |
| 17 | DuckDuckGo | 238ms | 157KB | 7ms | 220ms | 10 | 10 | PASS | MISS |
| 18 | OpenStreetMap | 721ms | 32KB | 2ms | 12ms | 10 | 5 | PASS | PASS |
| 19 | httpbin HTML | 1096ms | 3KB | 0ms | 3ms | 3 | 2 | PASS | PASS |
| 20 | Reuters | 618ms | 0KB | 0ms | 0ms | 1 | 1 | PASS | PASS |

## Hybrid Pipeline Stage Breakdown

| Site | BM25 build | HDC build | BM25 query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Hacker News | 1397µs | 46219µs | 18µs | 244µs | 249µs | 48335µs | 0 | 80 |
| HN Newest | 1433µs | 47123µs | 63µs | 19µs | 26µs | 48857µs | 14 | 5 |
| Lobsters | 1456µs | 40790µs | 10µs | 71µs | 48µs | 42585µs | 21 | 18 |
| CNN Lite | 1549µs | 46916µs | 7µs | 66µs | 15µs | 49079µs | 6 | 4 |
| NPR Text | 427µs | 13696µs | 5µs | 79µs | 18µs | 14249µs | 9 | 5 |
| Rust Lang | 742µs | 26976µs | 14µs | 77µs | 92µs | 27979µs | 38 | 28 |
| MDN HTML | 3001µs | 96956µs | 38µs | 130µs | 120µs | 100917µs | 140 | 60 |
| Python.org | 0µs | 0µs | 2µs | 67µs | 0µs | 142µs | 0 | 0 |
| W3C | 0µs | 0µs | 1µs | 47µs | 0µs | 136µs | 0 | 0 |
| GitHub Explore | 2915µs | 104359µs | 20µs | 53µs | 91µs | 108242µs | 63 | 42 |
| NPM | 415µs | 21801µs | 9µs | 49µs | 59µs | 22389µs | 16 | 16 |
| Crates.io | 7µs | 67µs | 2µs | 72µs | 3µs | 162µs | 1 | 1 |
| PyPI | 252µs | 9217µs | 11µs | 45µs | 36µs | 9602µs | 12 | 12 |
| docs.rs | 572µs | 15837µs | 8µs | 53µs | 42µs | 16571µs | 18 | 18 |
| pkg.go.dev | 806µs | 28012µs | 8µs | 101µs | 40µs | 29124µs | 20 | 10 |
| Docker Hub | 1124µs | 45207µs | 14µs | 54µs | 130µs | 47154µs | 39 | 39 |
| DuckDuckGo | 6498µs | 206393µs | 44µs | 143µs | 102µs | 213853µs | 128 | 60 |
| OpenStreetMap | 352µs | 10179µs | 5µs | 66µs | 22µs | 10718µs | 6 | 6 |
| httpbin HTML | 105µs | 3057µs | 4µs | 46µs | 11µs | 3234µs | 3 | 3 |
| Reuters | 11µs | 143µs | 1µs | 45µs | 5µs | 210µs | 0 | 1 |

## Top-3 Node Quality Comparison

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.148` q
2. `0.130` 
3. `0.130` Hacker News

**Hybrid top 3:**
1. `0.094` 2026-03-31T13:07:33 1774962453
2. `0.080` Hacker News new | past | comments | ask | show | jobs | submit login
3. `0.076` 23 points by vinhnx 4 hours ago | hide | 7 comments

---

### HN Newest — "newest submissions" 

**Legacy top 3:**
1. `0.148` q
2. `0.130` 
3. `0.130` Hacker News

**Hybrid top 3:**
1. `0.375` new
2. `0.281` Hacker News new | past | comments | ask | show | jobs | submit
3. `0.000` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 

---

### Lobsters — "programming stories and discussions" 

**Legacy top 3:**
1. `0.280` 47 Your job isn't programming practices codeandcake.dev authored by nick4 22 hou
2. `0.278` codeandcake.dev
3. `0.268` Your job isn't programming

**Hybrid top 3:**
1. `0.591` Your job isn't programming
2. `0.563` Programming language theory, types, design
3. `0.488` ask programming

---

### CNN Lite — "top news headlines today" 

**Legacy top 3:**
1. `0.268` Behind the scenes and in front of cameras, Hegseth serving as top cheerleader fo
2. `0.268` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94
3. `0.203` Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery

**Hybrid top 3:**
1. `0.563` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94
2. `0.563` Behind the scenes and in front of cameras, Hegseth serving as top cheerleader fo
3. `0.488` Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery

---

### NPR Text — "latest radio news stories" 

**Legacy top 3:**
1. `0.318` News
2. `0.308` Topics News Culture Music
3. `0.268` NPR : National Public Radio

**Hybrid top 3:**
1. `0.591` NPR : National Public Radio
2. `0.443` NPR : National Public Radio Tuesday, March 31, 2026 How Trump's EEOC is attackin
3. `0.398` News

---

### Rust Lang — "latest Rust version download" 

**Legacy top 3:**
1. `0.208` Read Rust
2. `0.208` Watch Rust
3. `0.200` 

**Hybrid top 3:**
1. `0.385` Read Rust
2. `0.385` Watch Rust
3. `0.331` Rust Programming Language Install Learn Playground Tools Governance Community Bl

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
1. `0.825` REPOSITORIES Topics Trending Collections
2. `0.825` COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Main
3. `0.825` Open Source COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Secur

---

### NPM — "search JavaScript packages" 

**Legacy top 3:**
1. `0.470` npm | Home skip to: content package search sign in ❤ Pro Teams Pricing Documenta
2. `0.322` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 
3. `0.243` Take your JavaScript development up a notch

**Hybrid top 3:**
1. `0.603` skip to: content package search sign in ❤ Pro Teams Pricing Documentation npm Se
2. `0.528` npm | Home skip to: content package search sign in ❤ Pro Teams Pricing Documenta
3. `0.458` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 

---

### Crates.io — "Rust package registry search" 

**Legacy top 3:**
1. `0.483` crates.io: Rust Package Registry

**Hybrid top 3:**
1. `0.474` crates.io: Rust Package Registry

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
1. `0.470` Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Down
2. `0.450` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus
3. `0.442` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus

**Hybrid top 3:**
1. `0.748` Rust Rust website The Book Standard Library API Reference Rust by Example The Ca
2. `0.725` Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rus
3. `0.650` Rust website The Book Standard Library API Reference Rust by Example The Cargo G

---

### pkg.go.dev — "Go packages and modules" 

**Legacy top 3:**
1. `0.563` Why Go Why Go Case Studies Use Cases Security Learn Docs Docs Effective Go Go Us
2. `0.458` Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Pa
3. `0.455` About Go Packages

**Hybrid top 3:**
1. `0.675` Packages Standard Library Sub-repositories About Go Packages
2. `0.516` Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Pa
3. `0.423` Packages

---

### Docker Hub — "search container images" 

**Legacy top 3:**
1. `0.470` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.313` Software supply chain Secure Your Supply Chain with Docker Hardened Images Use D
3. `0.313` Seamlessly ship any application, anywhere Push images and make your app accessib

**Hybrid top 3:**
1. `0.600` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.496` Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System st
3. `0.466` Most pulled images

---

### DuckDuckGo — "search engine privacy" 

**Legacy top 3:**
1. `0.715` translations.messages.R2RXjF[0].value: These browser permissions are used to add
2. `0.715` translations.messages.qUwfSi[2].value: . And when you leave our search engine an
3. `0.493` translations.messages.+bs8cY[0].value: Default search engine

**Hybrid top 3:**
1. `0.550` DuckDuckGo - Protection. Privacy. Peace of mind. Duck.ai Main navigation menu cl
2. `0.420` bootstrapTheme: false
3. `0.420` homepageDomain: null

---

### OpenStreetMap — "map navigation and editing" 

**Legacy top 3:**
1. `0.338` OpenStreetMap
2. `0.320` Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! Op
3. `0.320` https://openstreetmap.org/copyright https://openstreetmap.org Copyright OpenStre

**Hybrid top 3:**
1. `0.499` OpenStreetMap is a map of the world, created by people like you and free to use 
2. `0.415` Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people
3. `0.398` Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! Op

---

### httpbin HTML — "Herman Melville story" 

**Legacy top 3:**
1. `0.537` Herman Melville - Moby-Dick
2. `0.287` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.267` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

**Hybrid top 3:**
1. `0.517` Herman Melville - Moby-Dick
2. `0.388` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

---

### Reuters — "business news today" 

**Legacy top 3:**
1. `0.070` reuters.com Please enable JS and disable any ad blocker

**Hybrid top 3:**
1. `0.047` reuters.com Please enable JS and disable any ad blocker

---

