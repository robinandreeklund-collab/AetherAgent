# Real-World Validation — Hybrid Scoring Pipeline

**Date:** 2026-03-30
**Mode:** Release build, WITH embeddings (all-MiniLM-L6-v2, 384-dim)
**Method:** Fetch → Legacy parse_top_nodes → Hybrid parse_top_nodes_hybrid

## Summary

| Metric | Value |
|--------|-------|
| Sites tested | 20 |
| Successfully fetched | 19 |
| Legacy correctness (keyword in top 3) | 15/19 (79%) |
| Hybrid correctness (keyword in top 3) | 16/19 (84%) |
| Avg legacy parse time | 436.9ms |
| Avg hybrid parse time | 160.9ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Hacker News | 239ms | 33KB | 762ms | 454ms | 10 | 10 | PASS | PASS |
| 2 | HN Newest | 261ms | 39KB | 379ms | 149ms | 10 | 7 | MISS | PASS |
| 3 | Lobsters | 662ms | 57KB | 689ms | 257ms | 10 | 10 | MISS | MISS |
| 4 | CNN Lite | 359ms | 326KB | 814ms | 71ms | 10 | 3 | PASS | PASS |
| 5 | NPR Text | 474ms | 5KB | 768ms | 4ms | 10 | 5 | PASS | PASS |
| 6 | Rust Lang | 1161ms | 18KB | 655ms | 291ms | 10 | 10 | PASS | PASS |
| 7 | MDN HTML | 451ms | 173KB | 650ms | 816ms | 10 | 10 | PASS | PASS |
| 8 | Python.org | 399ms | 47KB | 28ms | 27ms | 0 | 0 | MISS | MISS |
| 9 | W3C | 313ms | 50KB | 30ms | 1ms | 0 | 0 | MISS | MISS |
| 10 | GitHub Explore | 1120ms | 386KB | 731ms | 428ms | 10 | 10 | PASS | PASS |
| 11 | NPM | 149ms | 28KB | 153ms | 29ms | 10 | 7 | PASS | PASS |
| 12 | Crates.io | 134ms | 3KB | 53ms | 27ms | 1 | 1 | PASS | PASS |
| 13 | PyPI | 49ms | 21KB | 354ms | 27ms | 10 | 10 | PASS | PASS |
| 14 | docs.rs | 254ms | 16KB | 638ms | 86ms | 10 | 10 | PASS | PASS |
| 15 | pkg.go.dev | 105ms | 32KB | 564ms | 116ms | 10 | 6 | PASS | PASS |
| 16 | Docker Hub | 228ms | 387KB | 325ms | 169ms | 10 | 10 | PASS | PASS |
| 17 | DuckDuckGo | FAIL | - | - | - | - | - | - | - |
| 18 | OpenStreetMap | 832ms | 32KB | 605ms | 54ms | 10 | 2 | PASS | PASS |
| 19 | httpbin HTML | 158ms | 3KB | 77ms | 27ms | 3 | 3 | PASS | PASS |
| 20 | Reuters | 444ms | 0KB | 26ms | 24ms | 1 | 1 | PASS | PASS |

## Hybrid Pipeline Stage Breakdown

| Site | TF-IDF build | HDC build | TF-IDF query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Hacker News | 1226µs | 15706µs | 14µs | 88µs | 433727µs | 450918µs | 0 | 452 |
| HN Newest | 1254µs | 17245µs | 12µs | 5µs | 126471µs | 145170µs | 18 | 7 |
| Lobsters | 1026µs | 12090µs | 5µs | 33µs | 239598µs | 252984µs | 20 | 18 |
| CNN Lite | 1325µs | 14873µs | 3µs | 29µs | 52593µs | 69279µs | 4 | 3 |
| NPR Text | 241µs | 3368µs | 2µs | 29µs | 6µs | 3670µs | 9 | 5 |
| Rust Lang | 288µs | 3065µs | 8µs | 33µs | 260065µs | 263508µs | 37 | 27 |
| MDN HTML | 1952µs | 18904µs | 21µs | 50µs | 759531µs | 780981µs | 133 | 132 |
| Python.org | 0µs | 0µs | 1µs | 35µs | 0µs | 107µs | 0 | 0 |
| W3C | 0µs | 0µs | 1µs | 24µs | 0µs | 88µs | 0 | 0 |
| GitHub Explore | 1589µs | 18751µs | 7µs | 15µs | 389540µs | 410513µs | 49 | 33 |
| NPM | 127µs | 1978µs | 3µs | 22µs | 11µs | 2188µs | 7 | 7 |
| Crates.io | 6µs | 39µs | 1µs | 32µs | 2µs | 90µs | 0 | 1 |
| PyPI | 106µs | 1033µs | 2µs | 18µs | 10µs | 1203µs | 10 | 10 |
| docs.rs | 319µs | 3442µs | 3µs | 24µs | 53332µs | 57206µs | 17 | 17 |
| pkg.go.dev | 507µs | 5800µs | 2µs | 24µs | 81664µs | 88125µs | 9 | 6 |
| Docker Hub | 606µs | 5616µs | 7µs | 26µs | 157538µs | 164309µs | 39 | 38 |
| OpenStreetMap | 245µs | 2672µs | 2µs | 31µs | 49761µs | 52802µs | 2 | 2 |
| httpbin HTML | 18µs | 184µs | 0µs | 20µs | 4µs | 236µs | 0 | 3 |
| Reuters | 7µs | 65µs | 0µs | 20µs | 24523µs | 24620µs | 0 | 1 |

## Top-3 Node Quality Comparison

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.354` Hacker News
2. `0.297` past
3. `0.297` Do your own writing

**Hybrid top 3:**
1. `0.393` Hacker News
2. `0.347` past
3. `0.347` Do your own writing

---

### HN Newest — "newest submissions" 

**Legacy top 3:**
1. `0.424` submit
2. `0.298` 1 minute ago
3. `0.298` 1 minute ago

**Hybrid top 3:**
1. `0.446` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 
2. `0.417` Hacker News new | past | comments | ask | show | jobs | submit
3. `0.416` Ubuntu MATE Is Seeking a New Primary Maintainer

---

### Lobsters — "programming stories and discussions" 

**Legacy top 3:**
1. `0.375` Your job isn't programming
2. `0.339` Rust programming
3. `0.315` Your job isn't programming

**Hybrid top 3:**
1. `0.689` Stories about particular persons
2. `0.618` Your job isn't programming
3. `0.603` C++ programming

---

### CNN Lite — "top news headlines today" 

**Legacy top 3:**
1. `0.324` Breaking News, Latest News and Videos | CNN CNN 3/30/2026 Latest Stories US oil 
2. `0.274` What we know on Day 31 of the US and Israel’s war with Iran: Trump threatens esc
3. `0.268` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94

**Hybrid top 3:**
1. `0.606` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94
2. `0.558` Breaking News, Latest News and Videos | CNN CNN 3/30/2026 Latest Stories US oil 
3. `0.526` Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery

---

### NPR Text — "latest radio news stories" 

**Legacy top 3:**
1. `0.504` NPR : National Public Radio
2. `0.489` News
3. `0.425` Topics News Culture Music

**Hybrid top 3:**
1. `0.618` News
2. `0.608` NPR : National Public Radio
3. `0.473` Text-Only Version Go To Full Site NPR : National Public Radio Monday, March 30, 

---

### Rust Lang — "latest Rust version download" 

**Legacy top 3:**
1. `0.379` Build it in Rust In 2018, the Rust community decided to improve the programming 
2. `0.371` Rust Programming Language Install Learn Playground Tools Governance Community Bl
3. `0.364` Build it in Rust In 2018, the Rust community decided to improve the programming 

**Hybrid top 3:**
1. `0.476` Read Rust
2. `0.467` Watch Rust
3. `0.444` Build it in Rust In 2018, the Rust community decided to improve the programming 

---

### MDN HTML — "HTML elements reference" 

**Legacy top 3:**
1. `0.700` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu
2. `0.640` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
3. `0.580` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu

**Hybrid top 3:**
1. `0.900` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
2. `0.900` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu
3. `0.881` HTML elements

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
1. `0.505` REPOSITORIES Topics Trending Collections
2. `0.470` REPOSITORIES Topics Trending Collections
3. `0.405` Trending

**Hybrid top 3:**
1. `0.825` REPOSITORIES Topics Trending Collections
2. `0.741` Trending repository
3. `0.741` Trending repository

---

### NPM — "search JavaScript packages" 

**Legacy top 3:**
1. `0.292` Take your JavaScript development up a notch
2. `0.263` npm | Home skip to: content package search sign in ❤ Pro Teams Pricing Documenta
3. `0.176` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 

**Hybrid top 3:**
1. `0.471` Take your JavaScript development up a notch
2. `0.387` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 
3. `0.319` skip to: content package search sign in ❤ Pro Teams Pricing Documentation npm Se

---

### Crates.io — "Rust package registry search" 

**Legacy top 3:**
1. `0.520` crates.io: Rust Package Registry

**Hybrid top 3:**
1. `0.428` crates.io: Rust Package Registry

---

### PyPI — "find Python packages" 

**Legacy top 3:**
1. `0.660` Find, install and publish Python packages with the Python Package Index
2. `0.595` Find, install and publish Python packages with the Python Package Index Search P
3. `0.580` Find, install and publish Python packages with the Python Package Index Search P

**Hybrid top 3:**
1. `0.900` Find, install and publish Python packages with the Python Package Index
2. `0.825` Find, install and publish Python packages with the Python Package Index Search P
3. `0.825` Find, install and publish Python packages with the Python Package Index Search P

---

### docs.rs — "Rust documentation search" 

**Legacy top 3:**
1. `0.496` Rust website
2. `0.462` Rust by Example
3. `0.445` Rustdoc JSON

**Hybrid top 3:**
1. `0.629` Rust Rust website The Book Standard Library API Reference Rust by Example The Ca
2. `0.629` Rust Rust website The Book Standard Library API Reference Rust by Example The Ca
3. `0.628` Rust website

---

### pkg.go.dev — "Go packages and modules" 

**Legacy top 3:**
1. `0.455` About Go Packages
2. `0.438` Packages Standard Library Sub-repositories About Go Packages
3. `0.430` Go Packages - Go Packages Skip to Main Content Why Go Case Studies Common proble

**Hybrid top 3:**
1. `0.806` About Go Packages
2. `0.704` Packages Standard Library Sub-repositories About Go Packages
3. `0.668` Packages

---

### Docker Hub — "search container images" 

**Legacy top 3:**
1. `0.437` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.313` Software supply chain Secure Your Supply Chain with Docker Hardened Images Use D
3. `0.313` Seamlessly ship any application, anywhere Push images and make your app accessib

**Hybrid top 3:**
1. `0.650` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.438` Docker Hardened Images - Now Free
3. `0.435` Seamlessly ship any application, anywhere Push images and make your app accessib

---

### OpenStreetMap — "map navigation and editing" 

**Legacy top 3:**
1. `0.360` OpenStreetMap
2. `0.321` Edit
3. `0.319` GPS Traces

**Hybrid top 3:**
1. `0.544` OpenStreetMap is a map of the world, created by people like you and free to use 
2. `0.476` Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people

---

### httpbin HTML — "Herman Melville story" 

**Legacy top 3:**
1. `0.598` Herman Melville - Moby-Dick
2. `0.476` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.456` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

**Hybrid top 3:**
1. `0.510` Herman Melville - Moby-Dick
2. `0.393` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.393` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

---

### Reuters — "business news today" 

**Legacy top 3:**
1. `0.070` reuters.com Please enable JS and disable any ad blocker

**Hybrid top 3:**
1. `0.186` reuters.com Please enable JS and disable any ad blocker

---

