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
| Avg legacy parse time | 417.7ms |
| Avg hybrid parse time | 167.8ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Hacker News | 278ms | 33KB | 711ms | 1073ms | 10 | 10 | PASS | PASS |
| 2 | HN Newest | 260ms | 40KB | 407ms | 100ms | 10 | 5 | MISS | PASS |
| 3 | Lobsters | 1199ms | 57KB | 674ms | 267ms | 10 | 10 | MISS | MISS |
| 4 | CNN Lite | 288ms | 326KB | 769ms | 67ms | 10 | 3 | PASS | PASS |
| 5 | NPR Text | 1292ms | 5KB | 739ms | 3ms | 10 | 5 | PASS | PASS |
| 6 | Rust Lang | 600ms | 18KB | 650ms | 285ms | 10 | 10 | PASS | PASS |
| 7 | MDN HTML | 458ms | 173KB | 626ms | 474ms | 10 | 10 | PASS | PASS |
| 8 | Python.org | 347ms | 47KB | 28ms | 27ms | 0 | 0 | MISS | MISS |
| 9 | W3C | 407ms | 50KB | 25ms | 0ms | 0 | 0 | MISS | MISS |
| 10 | GitHub Explore | 1195ms | 386KB | 692ms | 403ms | 10 | 10 | PASS | PASS |
| 11 | NPM | 106ms | 28KB | 147ms | 28ms | 10 | 7 | PASS | PASS |
| 12 | Crates.io | 129ms | 3KB | 50ms | 25ms | 1 | 1 | PASS | PASS |
| 13 | PyPI | 42ms | 21KB | 326ms | 27ms | 10 | 10 | PASS | PASS |
| 14 | docs.rs | 611ms | 16KB | 600ms | 29ms | 10 | 10 | PASS | PASS |
| 15 | pkg.go.dev | 112ms | 32KB | 526ms | 116ms | 10 | 6 | PASS | PASS |
| 16 | Docker Hub | 273ms | 387KB | 290ms | 156ms | 10 | 10 | PASS | PASS |
| 17 | DuckDuckGo | FAIL | - | - | - | - | - | - | - |
| 18 | OpenStreetMap | 820ms | 32KB | 573ms | 55ms | 10 | 2 | PASS | PASS |
| 19 | httpbin HTML | 174ms | 3KB | 73ms | 25ms | 3 | 3 | PASS | PASS |
| 20 | Reuters | 144ms | 0KB | 30ms | 28ms | 1 | 1 | PASS | PASS |

## Hybrid Pipeline Stage Breakdown

| Site | TF-IDF build | HDC build | TF-IDF query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Hacker News | 838µs | 13567µs | 13µs | 57µs | 1055089µs | 1069701µs | 0 | 80 |
| HN Newest | 1213µs | 17474µs | 27µs | 10µs | 76717µs | 95606µs | 14 | 5 |
| Lobsters | 718µs | 10315µs | 7µs | 37µs | 253229µs | 264468µs | 21 | 19 |
| CNN Lite | 1027µs | 13753µs | 5µs | 27µs | 49896µs | 65165µs | 4 | 3 |
| NPR Text | 211µs | 3005µs | 4µs | 30µs | 7µs | 3280µs | 9 | 5 |
| Rust Lang | 253µs | 3341µs | 8µs | 59µs | 253516µs | 257219µs | 37 | 27 |
| MDN HTML | 1559µs | 17574µs | 27µs | 64µs | 421633µs | 441272µs | 133 | 60 |
| Python.org | 0µs | 0µs | 1µs | 26µs | 0µs | 79µs | 0 | 0 |
| W3C | 0µs | 0µs | 1µs | 20µs | 0µs | 86µs | 0 | 0 |
| GitHub Explore | 1554µs | 20079µs | 11µs | 16µs | 361563µs | 383815µs | 49 | 33 |
| NPM | 95µs | 1414µs | 3µs | 23µs | 9µs | 1614µs | 7 | 7 |
| Crates.io | 5µs | 31µs | 2µs | 24µs | 2µs | 72µs | 1 | 1 |
| PyPI | 82µs | 1016µs | 3µs | 18µs | 10µs | 1162µs | 10 | 10 |
| docs.rs | 245µs | 3040µs | 4µs | 21µs | 14µs | 3365µs | 13 | 13 |
| pkg.go.dev | 538µs | 6341µs | 4µs | 32µs | 78290µs | 85315µs | 9 | 6 |
| Docker Hub | 388µs | 4469µs | 9µs | 23µs | 146221µs | 151531µs | 39 | 38 |
| OpenStreetMap | 198µs | 2328µs | 3µs | 30µs | 50707µs | 53351µs | 2 | 2 |
| httpbin HTML | 24µs | 202µs | 2µs | 23µs | 6µs | 268µs | 3 | 3 |
| Reuters | 7µs | 69µs | 1µs | 23µs | 27934µs | 28041µs | 0 | 1 |

## Top-3 Node Quality Comparison

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.354` Hacker News
2. `0.297` past
3. `0.297` Do your own writing

**Hybrid top 3:**
1. `0.215` Hacker News Hacker News new | past | comments | ask | show | jobs | submit login
2. `0.215` Hacker News new | past | comments | ask | show | jobs | submit login
3. `0.214` Hacker News new | past | comments | ask | show | jobs | submit login

---

### HN Newest — "newest submissions" 

**Legacy top 3:**
1. `0.424` submit
2. `0.298` 1 minute ago
3. `0.298` 1 minute ago

**Hybrid top 3:**
1. `0.500` new
2. `0.430` Hacker News new | past | comments | ask | show | jobs | submit
3. `0.422` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 

---

### Lobsters — "programming stories and discussions" 

**Legacy top 3:**
1. `0.375` Your job isn't programming
2. `0.339` Rust programming
3. `0.315` Your job isn't programming

**Hybrid top 3:**
1. `0.701` Your job isn't programming
2. `0.689` Graphics programming
3. `0.689` Stories about particular persons

---

### CNN Lite — "top news headlines today" 

**Legacy top 3:**
1. `0.365` Breaking News, Latest News and Videos | CNN CNN 3/30/2026 Latest Stories Are you
2. `0.274` What we know on Day 31 of the US and Israel’s war with Iran: Trump threatens esc
3. `0.268` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94

**Hybrid top 3:**
1. `0.613` Actor James Tolkan of ‘Top Gun’ and ‘Back to the Future’ fame dies at 94
2. `0.591` Breaking News, Latest News and Videos | CNN CNN 3/30/2026 Latest Stories Are you
3. `0.532` Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery

---

### NPR Text — "latest radio news stories" 

**Legacy top 3:**
1. `0.504` NPR : National Public Radio
2. `0.489` News
3. `0.425` Topics News Culture Music

**Hybrid top 3:**
1. `0.753` News
2. `0.745` NPR : National Public Radio
3. `0.559` NPR : National Public Radio Monday, March 30, 2026 Watch: Who is an American? Th

---

### Rust Lang — "latest Rust version download" 

**Legacy top 3:**
1. `0.379` Build it in Rust In 2018, the Rust community decided to improve the programming 
2. `0.371` Rust Programming Language Install Learn Playground Tools Governance Community Bl
3. `0.364` Build it in Rust In 2018, the Rust community decided to improve the programming 

**Hybrid top 3:**
1. `0.527` Read Rust
2. `0.519` Watch Rust
3. `0.456` Build it in Rust In 2018, the Rust community decided to improve the programming 

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
1. `0.844` Trending repository
2. `0.844` Trending repository
3. `0.844` Trending repository

---

### NPM — "search JavaScript packages" 

**Legacy top 3:**
1. `0.292` Take your JavaScript development up a notch
2. `0.263` npm | Home skip to: content package search sign in ❤ Pro Teams Pricing Documenta
3. `0.176` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 

**Hybrid top 3:**
1. `0.592` Take your JavaScript development up a notch
2. `0.450` Get started today for free, or step up to npm Pro to enjoy a premium JavaScript 
3. `0.358` skip to: content package search sign in ❤ Pro Teams Pricing Documentation npm Se

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
1. `0.799` Rust website
2. `0.771` Rust by Example
3. `0.737` Rust

---

### pkg.go.dev — "Go packages and modules" 

**Legacy top 3:**
1. `0.455` About Go Packages
2. `0.438` Packages Standard Library Sub-repositories About Go Packages
3. `0.430` Go Packages - Go Packages Skip to Main Content Why Go Case Studies Common proble

**Hybrid top 3:**
1. `0.888` About Go Packages
2. `0.750` Packages
3. `0.750` Packages

---

### Docker Hub — "search container images" 

**Legacy top 3:**
1. `0.437` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.313` Software supply chain Secure Your Supply Chain with Docker Hardened Images Use D
3. `0.313` Seamlessly ship any application, anywhere Push images and make your app accessib

**Hybrid top 3:**
1. `0.650` Docker Hub Container Image Library | App Containerization Search Docker Hub K He
2. `0.496` Docker Hardened Images - Now Free
3. `0.476` Most pulled images

---

### OpenStreetMap — "map navigation and editing" 

**Legacy top 3:**
1. `0.360` OpenStreetMap
2. `0.321` Edit
3. `0.319` GPS Traces

**Hybrid top 3:**
1. `0.536` OpenStreetMap is a map of the world, created by people like you and free to use 
2. `0.488` Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people

---

### httpbin HTML — "Herman Melville story" 

**Legacy top 3:**
1. `0.598` Herman Melville - Moby-Dick
2. `0.476` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.456` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

**Hybrid top 3:**
1. `0.534` Herman Melville - Moby-Dick
2. `0.407` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.407` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

---

### Reuters — "business news today" 

**Legacy top 3:**
1. `0.070` reuters.com Please enable JS and disable any ad blocker

**Hybrid top 3:**
1. `0.171` reuters.com Please enable JS and disable any ad blocker

---

