# Real-World Validation — Hybrid Scoring Pipeline

**Date:** 2026-03-30
**Mode:** Release build, no embeddings (text similarity only)
**Method:** Fetch → Legacy parse_top_nodes → Hybrid parse_top_nodes_hybrid

## Summary

| Metric | Value |
|--------|-------|
| Sites tested | 20 |
| Successfully fetched | 20 |
| Legacy correctness (keyword in top 3) | 10/20 (50%) |
| Hybrid correctness (keyword in top 3) | 11/20 (55%) |
| Avg legacy parse time | 2.1ms |
| Avg hybrid parse time | 9.6ms |

## Per-Site Results

| # | Site | Fetch | HTML | Legacy ms | Hybrid ms | L-nodes | H-nodes | L-correct | H-correct |
|---|------|-------|------|-----------|-----------|---------|---------|-----------|----------|
| 1 | Wikipedia Stockholm | 36ms | 0KB | 1ms | 0ms | 1 | 1 | MISS | MISS |
| 2 | Hacker News | 287ms | 33KB | 4ms | 17ms | 10 | 10 | PASS | PASS |
| 3 | Rust Lang | 820ms | 18KB | 1ms | 4ms | 10 | 10 | PASS | PASS |
| 4 | httpbin HTML | 181ms | 3KB | 0ms | 0ms | 3 | 3 | PASS | PASS |
| 5 | Python.org | 328ms | 47KB | 1ms | 1ms | 0 | 0 | MISS | MISS |
| 6 | MDN Web Docs | 337ms | 173KB | 8ms | 26ms | 10 | 10 | PASS | PASS |
| 7 | W3C | 296ms | 50KB | 0ms | 0ms | 0 | 0 | MISS | MISS |
| 8 | Example.com | 510ms | 0KB | 0ms | 0ms | 7 | 5 | PASS | PASS |
| 9 | BBC News | 273ms | 0KB | 0ms | 0ms | 1 | 1 | MISS | MISS |
| 10 | Crates.io | 104ms | 3KB | 0ms | 0ms | 1 | 1 | PASS | PASS |
| 11 | StackOverflow | 224ms | 0KB | 0ms | 0ms | 1 | 1 | MISS | MISS |
| 12 | Reuters | 465ms | 0KB | 0ms | 0ms | 1 | 1 | PASS | PASS |
| 13 | GitHub Explore | 1306ms | 386KB | 20ms | 39ms | 10 | 10 | MISS | PASS |
| 14 | Wikipedia Rust PL | 35ms | 0KB | 0ms | 0ms | 1 | 1 | MISS | MISS |
| 15 | NPM | 137ms | 28KB | 0ms | 1ms | 10 | 6 | PASS | PASS |
| 16 | Wikipedia AI | 33ms | 0KB | 0ms | 0ms | 1 | 1 | MISS | MISS |
| 17 | DuckDuckGo | 210ms | 157KB | 5ms | 86ms | 10 | 10 | PASS | PASS |
| 18 | Hacker News New | 288ms | 39KB | 3ms | 17ms | 10 | 9 | PASS | PASS |
| 19 | Wikipedia ML | 35ms | 0KB | 0ms | 0ms | 1 | 1 | MISS | MISS |
| 20 | curl httpbin | 360ms | 0KB | 0ms | 0ms | 2 | 2 | MISS | MISS |

## Hybrid Pipeline Stage Breakdown

| Site | TF-IDF build | HDC build | TF-IDF query | HDC prune | Embed score | Total pipeline | Candidates | Survivors |
|------|-------------|-----------|-------------|-----------|-------------|---------------|-----------|----------|
| Wikipedia Stockholm | 14µs | 27µs | 0µs | 19µs | 1µs | 66µs | 0 | 1 |
| Hacker News | 1186µs | 12052µs | 12µs | 79µs | 345µs | 13829µs | 0 | 452 |
| Rust Lang | 288µs | 2714µs | 6µs | 34µs | 16µs | 3111µs | 33 | 23 |
| httpbin HTML | 23µs | 159µs | 0µs | 19µs | 2µs | 212µs | 0 | 3 |
| Python.org | 0µs | 0µs | 1µs | 26µs | 0µs | 89µs | 0 | 0 |
| MDN Web Docs | 1910µs | 15818µs | 17µs | 51µs | 76µs | 18306µs | 133 | 132 |
| W3C | 0µs | 0µs | 1µs | 26µs | 0µs | 91µs | 0 | 0 |
| Example.com | 38µs | 308µs | 2µs | 3µs | 4µs | 362µs | 5 | 5 |
| BBC News | 14µs | 27µs | 1µs | 23µs | 1µs | 28µs | 0 | 1 |
| Crates.io | 6µs | 33µs | 0µs | 28µs | 1µs | 77µs | 0 | 1 |
| StackOverflow | 14µs | 27µs | 1µs | 23µs | 1µs | 27µs | 0 | 1 |
| Reuters | 10µs | 56µs | 0µs | 23µs | 2µs | 97µs | 0 | 1 |
| GitHub Explore | 1812µs | 16996µs | 7µs | 11µs | 25µs | 19471µs | 53 | 37 |
| Wikipedia Rust PL | 14µs | 27µs | 1µs | 29µs | 1µs | 33µs | 0 | 1 |
| NPM | 117µs | 1067µs | 2µs | 19µs | 4µs | 1252µs | 6 | 6 |
| Wikipedia AI | 14µs | 27µs | 1µs | 27µs | 1µs | 30µs | 0 | 1 |
| DuckDuckGo | 6530µs | 74220µs | 19µs | 57µs | 90µs | 81470µs | 128 | 128 |
| Hacker News New | 1141µs | 12097µs | 10µs | 8µs | 13µs | 13418µs | 22 | 9 |
| Wikipedia ML | 14µs | 27µs | 1µs | 26µs | 1µs | 30µs | 0 | 1 |
| curl httpbin | 8µs | 56µs | 0µs | 21µs | 1µs | 90µs | 0 | 2 |

## Top-3 Node Quality Comparison

### Wikipedia Stockholm — "population of Stockholm" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

### Hacker News — "top stories today" 

**Legacy top 3:**
1. `0.148` q
2. `0.130` 
3. `0.130` Hacker News

**Hybrid top 3:**
1. `0.210` Hacker News
2. `0.210` new
3. `0.210` past

---

### Rust Lang — "latest Rust version download" 

**Legacy top 3:**
1. `0.208` Read Rust
2. `0.208` Watch Rust
3. `0.208` Rust Programming Language Install Learn Playground Tools Governance Community Bl

**Hybrid top 3:**
1. `0.316` Read Rust
2. `0.316` Watch Rust
3. `0.273` Build it in Rust In 2018, the Rust community decided to improve the programming 

---

### httpbin HTML — "Herman Melville story" 

**Legacy top 3:**
1. `0.537` Herman Melville - Moby-Dick
2. `0.437` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.417` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

**Hybrid top 3:**
1. `0.460` Herman Melville - Moby-Dick
2. `0.360` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th
3. `0.360` Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather th

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
1. `0.900` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 
2. `0.900` MDN HTML HTML: Markup language HTML reference Elements Global attributes Attribu
3. `0.825` HTML HTML: Markup language HTML reference Elements Global attributes Attributes 

---

### W3C — "web standards specifications" 

**Legacy top 3:**

**Hybrid top 3:**

---

### Example.com — "domain information" 

**Legacy top 3:**
1. `0.425` Example Domain
2. `0.345` Example Domain Example Domain This domain is for use in documentation examples w
3. `0.340` Example Domain This domain is for use in documentation examples without needing 

**Hybrid top 3:**
1. `0.386` Example Domain
2. `0.317` Example Domain This domain is for use in documentation examples without needing 
3. `0.311` This domain is for use in documentation examples without needing permission. Avo

---

### BBC News — "breaking news headlines" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

### Crates.io — "most downloaded Rust crates" 

**Legacy top 3:**
1. `0.345` crates.io: Rust Package Registry

**Hybrid top 3:**
1. `0.285` crates.io: Rust Package Registry

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
1. `0.060` reuters.com Please enable JS and disable any ad blocker

---

### GitHub Explore — "trending repositories" 

**Legacy top 3:**
1. `0.505` REPOSITORIES Topics Trending Collections
2. `0.470` REPOSITORIES Topics Trending Collections
3. `0.405` Trending

**Hybrid top 3:**
1. `0.825` REPOSITORIES Topics Trending Collections
2. `0.664` Advanced analytics of GitHub data (projects and repositories)
3. `0.664` Search code, repositories, users, issues, pull requests...

---

### Wikipedia Rust PL — "Rust programming language features" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

### NPM — "search JavaScript packages" 

**Legacy top 3:**
1. `0.253` npm | Home skip to: content package search sign in ❤ Pro Teams Pricing Documenta
2. `0.243` Take your JavaScript development up a notch
3. `0.130` Learn about Pro

**Hybrid top 3:**
1. `0.456` Take your JavaScript development up a notch
2. `0.305` skip to: content package search sign in ❤ Pro Teams Pricing Documentation npm Se
3. `0.305` skip to: content package search sign in ❤ Pro Teams Pricing Documentation npm Se

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
1. `0.148` q
2. `0.130` 
3. `0.130` Hacker News

**Hybrid top 3:**
1. `0.348` Microsoft kills Windows Remote Desktop app in favor of the new Windows App
2. `0.348` new
3. `0.348` Ubuntu MATE Is Seeking a New Primary Maintainer

---

### Wikipedia ML — "types of machine learning" 

**Legacy top 3:**
1. `0.070` Blocked by egress policy

**Hybrid top 3:**
1. `0.060` Blocked by egress policy

---

### curl httpbin — "robots disallow rules" 

**Legacy top 3:**
1. `0.253` User-agent: *
Disallow: /deny
2. `0.233` User-agent: *
Disallow: /deny

**Hybrid top 3:**
1. `0.210` User-agent: *
Disallow: /deny
2. `0.210` User-agent: *
Disallow: /deny

---

