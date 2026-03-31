# ColBERT vs MiniLM vs Hybrid — Live Validation

**Date:** 2026-03-31
**Mode:** Release build, embeddings (all-MiniLM-L6-v2) + ColBERTv2.0 (110M params, CPU)
**Sites:** 44 fetched / 45 total

## Summary

| Metod | Korrekthet | Avg latens | Avg top-1 score |
|-------|-----------|------------|----------------|
| MiniLM (bi-encoder) | 33/44 (75.0%) | 886.3ms | 0.517 |
| ColBERT (MaxSim) | 32/44 (72.7%) | 828.4ms | 0.773 |
| Hybrid (adaptive α) | 32/44 (72.7%) | 831.8ms | 0.638 |

ColBERT wins (correct where MiniLM misses): **0**
Hybrid wins (correct where MiniLM misses): **0**
MiniLM-only (correct where ColBERT misses): **1**

## Per-Site Results

| # | Site | HTML | DOM | M-ok | C-ok | H-ok | M-ms | C-ms | H-ms | M-top1 | C-top1 | H-top1 |
|---|------|------|-----|------|------|------|------|------|------|--------|--------|--------|
| 1 | Hacker News | 33KB | 1 | PASS | PASS | PASS | 2442 | 2938 | 2882 | 0.218 | 1.000 | 0.847 |
| 2 | HN Newest | 40KB | 1 | PASS | PASS | PASS | 807 | 326 | 324 | 0.512 | 1.000 | 0.863 |
| 3 | Lobsters | 57KB | 1 | PASS | PASS | PASS | 1260 | 782 | 780 | 0.683 | 1.000 | 0.747 |
| 4 | CNN Lite | 331KB | 1 | PASS | PASS | PASS | 1129 | 185 | 185 | 0.563 | 1.000 | 0.902 |
| 5 | NPR Text | 5KB | 1 | PASS | PASS | PASS | 1047 | 219 | 217 | 0.794 | 1.000 | 0.856 |
| 6 | BBC News | 0KB | 1 | MISS | MISS | MISS | 75 | 75 | 74 | 0.047 | 0.500 | 0.183 |
| 7 | Al Jazeera | 400KB | 28 | PASS | PASS | PASS | 1342 | 649 | 708 | 0.755 | 1.000 | 0.875 |
| 8 | Reuters | 0KB | 1 | PASS | PASS | PASS | 74 | 72 | 72 | 0.173 | 0.500 | 0.271 |
| 9 | AP News | 0KB | 1 | MISS | MISS | MISS | 38 | 73 | 73 | 0.083 | 0.500 | 0.208 |
| 10 | Rust Lang | 18KB | 1 | PASS | PASS | PASS | 1517 | 1161 | 1185 | 0.550 | 1.000 | 0.819 |
| 11 | MDN HTML | 173KB | 1 | PASS | PASS | PASS | 1649 | 2369 | 2280 | 0.825 | 1.000 | 0.877 |
| 12 | Go Dev | 62KB | 1 | PASS | PASS | PASS | 1311 | 716 | 715 | 0.698 | 1.000 | 0.850 |
| 13 | TypeScript | FAIL | - | - | - | - | - | - | - | - | - | - |
| 14 | Kotlin | 251KB | 1 | MISS | MISS | MISS | 118 | 76 | 76 | 0.182 | 0.500 | 0.278 |
| 15 | Node.js | 451KB | 1 | PASS | PASS | PASS | 917 | 698 | 700 | 0.462 | 1.000 | 0.814 |
| 16 | Ruby Lang | 88KB | 1 | PASS | PASS | PASS | 1197 | 1330 | 1336 | 0.712 | 1.000 | 0.940 |
| 17 | docs.rs | 17KB | 1 | PASS | PASS | PASS | 1002 | 762 | 759 | 0.758 | 1.000 | 0.878 |
| 18 | DevDocs | 8KB | 1 | PASS | PASS | PASS | 479 | 74 | 75 | 0.491 | 0.500 | 0.494 |
| 19 | Can I Use | 18KB | 0 | MISS | MISS | MISS | 39 | 37 | 37 | 0.000 | 0.000 | 0.000 |
| 20 | Crates.io | 3KB | 1 | PASS | PASS | PASS | 113 | 75 | 74 | 0.504 | 0.500 | 0.503 |
| 21 | PyPI | 21KB | 1 | PASS | PASS | PASS | 494 | 462 | 461 | 0.945 | 1.000 | 0.961 |
| 22 | pkg.go.dev | 32KB | 1 | PASS | PASS | PASS | 821 | 415 | 413 | 0.704 | 1.000 | 0.746 |
| 23 | RubyGems | 18KB | 1 | PASS | PASS | PASS | 1120 | 483 | 482 | 0.616 | 1.000 | 0.880 |
| 24 | NuGet | 16KB | 1 | PASS | PASS | PASS | 955 | 889 | 889 | 0.825 | 1.000 | 0.947 |
| 25 | Docker Hub | 388KB | 1 | PASS | PASS | PASS | 941 | 1489 | 1547 | 0.600 | 1.000 | 0.940 |
| 26 | Kubernetes | 37KB | 0 | MISS | MISS | MISS | 39 | 38 | 38 | 0.000 | 0.000 | 0.000 |
| 27 | Terraform | 120KB | 340 | PASS | PASS | PASS | 2495 | 1809 | 1809 | 0.960 | 1.000 | 0.880 |
| 28 | GitHub Explore | 393KB | 15 | PASS | PASS | PASS | 1558 | 1611 | 1725 | 0.887 | 1.000 | 0.896 |
| 29 | GitLab | 203KB | 79 | PASS | MISS | MISS | 1968 | 2726 | 2766 | 0.802 | 1.000 | 0.906 |
| 30 | DuckDuckGo | 157KB | 1990 | PASS | PASS | PASS | 565 | 2388 | 2399 | 0.550 | 1.000 | 0.765 |
| 31 | OpenStreetMap | 32KB | 1 | PASS | PASS | PASS | 1027 | 244 | 245 | 0.499 | 1.000 | 0.767 |
| 32 | W3C | 50KB | 0 | MISS | MISS | MISS | 40 | 38 | 37 | 0.000 | 0.000 | 0.000 |
| 33 | Python.org | 47KB | 0 | MISS | MISS | MISS | 78 | 37 | 37 | 0.000 | 0.000 | 0.000 |
| 34 | IETF | 82KB | 0 | MISS | MISS | MISS | 78 | 37 | 42 | 0.000 | 0.000 | 0.000 |
| 35 | Wikipedia Main | 0KB | 1 | MISS | MISS | MISS | 38 | 74 | 76 | 0.093 | 0.500 | 0.215 |
| 36 | Wiktionary | 0KB | 1 | MISS | MISS | MISS | 39 | 78 | 77 | 0.049 | 0.500 | 0.184 |
| 37 | httpbin HTML | 3KB | 1 | PASS | PASS | PASS | 148 | 139 | 138 | 0.570 | 1.000 | 0.699 |
| 38 | httpbin JSON | 0KB | 1 | PASS | PASS | PASS | 77 | 75 | 79 | 0.220 | 0.500 | 0.304 |
| 39 | JSON Placeholder | 8KB | 1 | PASS | PASS | PASS | 1045 | 348 | 342 | 0.945 | 1.000 | 0.961 |
| 40 | Stack Overflow | 0KB | 1 | MISS | MISS | MISS | 38 | 73 | 71 | 0.049 | 0.500 | 0.184 |
| 41 | Haskell.org | 63KB | 1 | PASS | PASS | PASS | 1242 | 1763 | 1727 | 0.945 | 1.000 | 0.950 |
| 42 | Elixir Lang | 26KB | 1 | PASS | PASS | PASS | 1661 | 1697 | 1719 | 0.872 | 1.000 | 0.911 |
| 43 | Zig Lang | 12KB | 1 | PASS | PASS | PASS | 1266 | 1679 | 1678 | 0.825 | 1.000 | 0.880 |
| 44 | Svelte | 87KB | 1 | PASS | PASS | PASS | 969 | 1286 | 1336 | 0.825 | 1.000 | 0.951 |
| 45 | Tailwind CSS | 913KB | 8749 | PASS | PASS | PASS | 3737 | 3958 | 3915 | 0.960 | 1.000 | 0.953 |

## Top-1 Comparison (selected)

### Hacker News — top-1 labels
- **MiniLM**: `0.218` 15 points by jandrewrogers 2 hours ago | hide | 8 comments
- **ColBERT**: `1.000` 15 points by jandrewrogers 2 hours ago | hide | 8 comments
- **Hybrid**: `0.847` Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude C

### HN Newest — top-1 labels
- **MiniLM**: `0.512` I Decompiled the White House's New App
- **ColBERT**: `1.000` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 
- **Hybrid**: `0.863` New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | 

### Lobsters — top-1 labels
- **MiniLM**: `0.683` Your job isn't programming
- **ColBERT**: `1.000` ask programming
- **Hybrid**: `0.747` ask programming

### CNN Lite — top-1 labels
- **MiniLM**: `0.563` Behind the scenes and in front of cameras, Hegseth serving as top cheerleader fo
- **ColBERT**: `1.000` Breaking News, Latest News and Videos | CNN CNN 3/31/2026 Latest Stories Takeawa
- **Hybrid**: `0.902` Breaking News, Latest News and Videos | CNN CNN 3/31/2026 Latest Stories Takeawa

### NPR Text — top-1 labels
- **MiniLM**: `0.794` NPR : National Public Radio
- **ColBERT**: `1.000` NPR : National Public Radio
- **Hybrid**: `0.856` NPR : National Public Radio

### BBC News — top-1 labels
- **MiniLM**: `0.047` Blocked by egress policy
- **ColBERT**: `0.500` Blocked by egress policy
- **Hybrid**: `0.183` Blocked by egress policy

### Al Jazeera — top-1 labels
- **MiniLM**: `0.755` jsonLd.headline: Breaking News, World News and Video from Al Jazeera
- **ColBERT**: `1.000` jsonLd.headline: Breaking News, World News and Video from Al Jazeera
- **Hybrid**: `0.875` Breaking News, World News and Video from Al Jazeera Skip links Skip to Featured 

### Reuters — top-1 labels
- **MiniLM**: `0.173` reuters.com Please enable JS and disable any ad blocker
- **ColBERT**: `0.500` reuters.com Please enable JS and disable any ad blocker
- **Hybrid**: `0.271` reuters.com Please enable JS and disable any ad blocker

### AP News — top-1 labels
- **MiniLM**: `0.083` Blocked by egress policy
- **ColBERT**: `0.500` Blocked by egress policy
- **Hybrid**: `0.208` Blocked by egress policy

### Rust Lang — top-1 labels
- **MiniLM**: `0.550` Read Rust
- **ColBERT**: `1.000` Rust Programming Language Install Learn Playground Tools Governance Community Bl
- **Hybrid**: `0.819` Rust Programming Language Install Learn Playground Tools Governance Community Bl

### MDN HTML — top-1 labels
- **MiniLM**: `0.825` Reference for all HTML elements .
- **ColBERT**: `1.000` Reference for all HTML elements .
- **Hybrid**: `0.877` Reference for all HTML elements .

### Go Dev — top-1 labels
- **MiniLM**: `0.698` Get Started Download Go
- **ColBERT**: `1.000` Build simple, secure, scalable systems with Go An open-source programming langua
- **Hybrid**: `0.850` Build simple, secure, scalable systems with Go An open-source programming langua

### Kotlin — top-1 labels
- **MiniLM**: `0.182` isDarkTheme: true
- **ColBERT**: `0.500` isDarkTheme: true
- **Hybrid**: `0.278` isDarkTheme: true

### Node.js — top-1 labels
- **MiniLM**: `0.462` Get Node.js® Get Node.js® Get security support for EOL Node.js versions Node.js 
- **ColBERT**: `1.000` Node.js — Run JavaScript Everywhere Skip to content Learn About Download Blog Do
- **Hybrid**: `0.814` Get Node.js® Get Node.js® Get security support for EOL Node.js versions Node.js 

### Ruby Lang — top-1 labels
- **MiniLM**: `0.712` “ Ruby is just the most beautiful programming language I have ever seen. And I p
- **ColBERT**: `1.000` Ruby Programming Language Ruby Install Docs Libraries Contribution Community New
- **Hybrid**: `0.940` Ruby Programming Language Ruby Install Docs Libraries Contribution Community New

### docs.rs — top-1 labels
- **MiniLM**: `0.758` Rust Rust website The Book Standard Library API Reference Rust by Example The Ca
- **ColBERT**: `1.000` Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Down
- **Hybrid**: `0.878` Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Down

### DevDocs — top-1 labels
- **MiniLM**: `0.491` DevDocs API Documentation Clear search DevDocs Preferences Offline Data Changelo
- **ColBERT**: `0.500` DevDocs API Documentation Clear search DevDocs Preferences Offline Data Changelo
- **Hybrid**: `0.494` DevDocs API Documentation Clear search DevDocs Preferences Offline Data Changelo

### Can I Use — top-1 labels
- **MiniLM**: `0.000` 
- **ColBERT**: `0.000` 
- **Hybrid**: `0.000` 

### Crates.io — top-1 labels
- **MiniLM**: `0.504` crates.io: Rust Package Registry
- **ColBERT**: `0.500` crates.io: Rust Package Registry
- **Hybrid**: `0.503` crates.io: Rust Package Registry

### PyPI — top-1 labels
- **MiniLM**: `0.945` Find, install and publish Python packages with the Python Package Index
- **ColBERT**: `1.000` Find, install and publish Python packages with the Python Package Index
- **Hybrid**: `0.961` Find, install and publish Python packages with the Python Package Index

