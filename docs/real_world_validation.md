# Real-World Validation — Hybrid Scoring Pipeline

**Date:** 2026-03-30
**Mode:** Release build, WITH embeddings (all-MiniLM-L6-v2, 384-dim)
**Method:** Fetch → Legacy parse_top_nodes → Hybrid parse_top_nodes_hybrid

## Summary

| Metric | Value |
|--------|-------|
| Sites tested | 20 |
| Successfully fetched | 20 |
| Legacy correctness (keyword in top 3) | 9/20 (45%) |
| Hybrid correctness (keyword in top 3) | 10/20 (50%) |
| Avg legacy parse time | 204.4ms |
| Avg hybrid parse time | 184.9ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Wikipedia Stockholm | 65ms | 0KB | 28ms | 52ms | 1 | 1 | MISS | MISS |
| 2 | Hacker News | 322ms | 33KB | 690ms | 436ms | 10 | 10 | PASS | PASS |
| 3 | Rust Lang | 898ms | 18KB | 650ms | 300ms | 10 | 10 | PASS | PASS |
| 4 | httpbin HTML | 193ms | 3KB | 79ms | 26ms | 3 | 3 | PASS | PASS |
| 5 | Python.org | 588ms | 47KB | 29ms | 29ms | 0 | 0 | MISS | MISS |
| 6 | MDN Web Docs | 299ms | 173KB | 638ms | 847ms | 10 | 10 | PASS | PASS |
| 7 | W3C | 146ms | 50KB | 28ms | 0ms | 0 | 0 | MISS | MISS |
| 8 | Example.com | 145ms | 0KB | 158ms | 0ms | 7 | 5 | PASS | PASS |
| 9 | BBC News | 379ms | 0KB | 26ms | 0ms | 1 | 1 | MISS | MISS |
| 10 | Crates.io | 118ms | 3KB | 58ms | 27ms | 1 | 1 | PASS | PASS |
| 11 | StackOverflow | 260ms | 0KB | 26ms | 0ms | 1 | 1 | MISS | MISS |
| 12 | Reuters | 202ms | 0KB | 26ms | 25ms | 1 | 1 | PASS | PASS |
| 13 | GitHub Explore | 1305ms | 386KB | 728ms | 425ms | 10 | 10 | MISS | MISS |
| 14 | Wikipedia Rust PL | 64ms | 0KB | 25ms | 25ms | 1 | 1 | MISS | MISS |
| 15 | NPM | 117ms | 28KB | 159ms | 27ms | 10 | 7 | PASS | PASS |
| 16 | Wikipedia AI | 44ms | 0KB | 25ms | 0ms | 1 | 1 | MISS | MISS |
| 17 | DuckDuckGo | 203ms | 157KB | 222ms | 1330ms | 10 | 10 | PASS | PASS |
| 18 | Hacker News New | 287ms | 39KB | 414ms | 149ms | 10 | 7 | MISS | PASS |
| 19 | Wikipedia ML | 54ms | 0KB | 27ms | 0ms | 1 | 1 | MISS | MISS |
| 20 | curl httpbin | 173ms | 0KB | 53ms | 0ms | 2 | 2 | MISS | MISS |

## Hybrid Pipeline Stage Breakdown

| Site | TF-IDF build | HDC build | TF-IDF query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Wikipedia Stockholm | 15µs | 30µs | 0µs | 20µs | 26213µs | 26283µs | 0 | 1 |
| Hacker News | 1131µs | 15068µs | 14µs | 85µs | 416860µs | 433299µs | 0 | 453 |
| Rust Lang | 295µs | 3324µs | 7µs | 33µs | 269735µs | 273436µs | 37 | 27 |
| httpbin HTML | 19µs | 189µs | 0µs | 20µs | 5µs | 242µs | 0 | 3 |
| Python.org | 0µs | 0µs | 1µs | 29µs | 0µs | 92µs | 0 | 0 |
| MDN Web Docs | 1814µs | 18958µs | 19µs | 56µs | 792531µs | 813868µs | 133 | 132 |
| W3C | 0µs | 0µs | 1µs | 23µs | 0µs | 89µs | 0 | 0 |
| Example.com | 31µs | 264µs | 1µs | 2µs | 5µs | 310µs | 5 | 5 |
| BBC News | 15µs | 30µs | 1µs | 19µs | 2µs | 24µs | 0 | 1 |
| Crates.io | 9µs | 37µs | 1µs | 33µs | 2µs | 91µs | 0 | 1 |
| StackOverflow | 15µs | 30µs | 1µs | 23µs | 2µs | 29µs | 0 | 1 |
| Reuters | 8µs | 63µs | 0µs | 20µs | 25724µs | 25820µs | 0 | 1 |
| GitHub Explore | 1799µs | 20574µs | 7µs | 16µs | 383546µs | 406586µs | 49 | 33 |
| Wikipedia Rust PL | 15µs | 30µs | 1µs | 31µs | 3µs | 38µs | 0 | 1 |
| NPM | 116µs | 1410µs | 2µs | 21µs | 9µs | 1602µs | 7 | 7 |
| Wikipedia AI | 15µs | 30µs | 1µs | 27µs | 2µs | 33µs | 0 | 1 |
| DuckDuckGo | 6590µs | 90684µs | 21µs | 61µs | 1227048µs | 1324990µs | 135 | 135 |
| Hacker News New | 1196µs | 16629µs | 12µs | 6µs | 127282µs | 145289µs | 18 | 7 |
| Wikipedia ML | 15µs | 30µs | 1µs | 29µs | 2µs | 34µs | 0 | 1 |
| curl httpbin | 6µs | 49µs | 0µs | 17µs | 1µs | 77µs | 0 | 2 |

## Top-3 Node Quality Comparison

### Wikipedia Stockholm — "population of Stockholm" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

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

### Python.org — "download Python latest version" 

**Legacy top 3:**

**Hybrid top 3:**

---

### MDN Web Docs — "HTML elements reference" 

**Legacy top 3:**
1. `0.700` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu
2. `0.640` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
3. `0.580` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu

**Hybrid top 3:**
1. `0.900` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu
2. `0.900` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
3. `0.881` HTML elements

---

### W3C — "web standards specifications" 

**Legacy top 3:**

**Hybrid top 3:**

---

### Example.com — "domain information" 

**Legacy top 3:**
1. `0.535` Example Domain
2. `0.411` Example Domain This domain is for use in documentation examples without needing 
3. `0.400` Example Domain Example Domain This domain is for use in documentation examples w

**Hybrid top 3:**
1. `0.475` Example Domain
2. `0.375` Example Domain This domain is for use in documentation examples without needing 
3. `0.350` Example Domain This domain is for use in documentation examples without needing 

---

### BBC News — "breaking news headlines" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.067` Blocked by egress policy

---

### Crates.io — "most downloaded Rust crates" 

**Legacy top 3:**
1. `0.450` crates.io: Rust Package Registry

**Hybrid top 3:**
1. `0.371` crates.io: Rust Package Registry

---

### StackOverflow — "popular programming questions" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

### Reuters — "business news today" 

**Legacy top 3:**
1. `0.070` reuters.com Please enable JS and disable any ad blocker

**Hybrid top 3:**
1. `0.186` reuters.com Please enable JS and disable any ad blocker

---

### GitHub Explore — "trending repositories" 

**Legacy top 3:**
1. `0.505` REPOSITORIES Topics Trending Collections
2. `0.470` REPOSITORIES Topics Trending Collections
3. `0.405` Trending

**Hybrid top 3:**
1. `0.825` REPOSITORIES Topics Trending Collections
2. `0.742` Trending repository
3. `0.742` Trending repository

---

### Wikipedia Rust PL — "Rust programming language features" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

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

### Wikipedia AI — "definition of artificial intelligence" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

### DuckDuckGo — "search engine privacy" 

**Legacy top 3:**
1. `0.715` translations.messages.R2RXjF[0].value: These browser permissions are used to add
2. `0.715` translations.messages.qUwfSi[2].value: . And when you leave our search engine an
3. `0.493` translations.messages.+bs8cY[0].value: Default search engine

**Hybrid top 3:**
1. `0.800` translations.messages.R2RXjF[0].value: These browser permissions are used to add
2. `0.720` translations.messages.qUwfSi[2].value: . And when you leave our search engine an
3. `0.650` translations.messages.5wG46Q[0].value: We make money from private ads on our sea

---

### Hacker News New — "newest submissions" 

**Legacy top 3:**
1. `0.424` submit
2. `0.299` 6 minutes ago
3. `0.298` 1 minute ago

**Hybrid top 3:**
1. `0.446` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 
2. `0.417` Hacker News new | past | comments | ask | show | jobs | submit
3. `0.416` Ubuntu MATE Is Seeking a New Primary Maintainer

---

### Wikipedia ML — "types of machine learning" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

### curl httpbin — "robots disallow rules" 

**Legacy top 3:**
1. `0.365` User-agent: *
Disallow: /deny
2. `0.345` User-agent: *
Disallow: /deny

**Hybrid top 3:**
1. `0.301` User-agent: *
Disallow: /deny
2. `0.301` User-agent: *
Disallow: /deny

---

