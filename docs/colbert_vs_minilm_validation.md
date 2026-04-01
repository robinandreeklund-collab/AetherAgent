# ColBERT vs MiniLM vs Hybrid — Live Validation

**Date:** 2026-03-31
**Mode:** Release build, bi-encoder (all-MiniLM-L6-v2, 384-dim) + ColBERTv2.0 (768-dim, ONNX, CPU)
**Sites:** 11 fetched / 30 total

## Summary

| Metod | Korrekthet | Avg latens | Avg top-1 score |
|-------|-----------|------------|----------------|
| MiniLM (bi-encoder) | 11/11 (100.0%) | 946.1ms | 0.590 |
| ColBERT (MaxSim) | 11/11 (100.0%) | 428.1ms | 0.955 |
| Hybrid (adaptive α) | 11/11 (100.0%) | 451.5ms | 0.788 |

ColBERT wins (correct where MiniLM misses): **0**
Hybrid wins (correct where MiniLM misses): **0**
MiniLM-only (correct where ColBERT misses): **0**

## Per-Site Results

| # | Site | HTML | DOM | M-ok | C-ok | H-ok | M-ms | C-ms | H-ms | M-top1 | C-top1 | H-top1 |
|---|------|------|-----|------|------|------|------|------|------|--------|--------|--------|
| 1 | Hacker News | 34KB | 496 | PASS | PASS | PASS | 2194 | 1724 | 2067 | 0.219 | 1.000 | 0.850 |
| 2 | HN Newest | 40KB | 523 | PASS | PASS | PASS | 618 | 106 | 104 | 0.457 | 1.000 | 0.857 |
| 3 | Lobsters | FAIL | - | - | - | - | - | - | - | - | - | - |
| 4 | CNN Lite | FAIL | - | - | - | - | - | - | - | - | - | - |
| 5 | NPR Text | FAIL | - | - | - | - | - | - | - | - | - | - |
| 6 | Reuters | 0KB | 1 | PASS | PASS | PASS | 75 | 38 | 36 | 0.173 | 0.500 | 0.271 |
| 7 | Rust Lang | FAIL | - | - | - | - | - | - | - | - | - | - |
| 8 | MDN HTML | FAIL | - | - | - | - | - | - | - | - | - | - |
| 9 | Go Dev | 62KB | 245 | PASS | PASS | PASS | 1296 | 329 | 335 | 0.698 | 1.000 | 0.850 |
| 10 | TypeScript | FAIL | - | - | - | - | - | - | - | - | - | - |
| 11 | Kotlin | FAIL | - | - | - | - | - | - | - | - | - | - |
| 12 | Node.js | 450KB | 32 | PASS | PASS | PASS | 937 | 397 | 392 | 0.462 | 1.000 | 0.808 |
| 13 | Ruby Lang | FAIL | - | - | - | - | - | - | - | - | - | - |
| 14 | docs.rs | 17KB | 83 | PASS | PASS | PASS | 1047 | 465 | 454 | 0.758 | 1.000 | 0.879 |
| 15 | DevDocs | FAIL | - | - | - | - | - | - | - | - | - | - |
| 16 | PyPI | 21KB | 26 | PASS | PASS | PASS | 516 | 225 | 226 | 0.945 | 1.000 | 0.951 |
| 17 | pkg.go.dev | 32KB | 246 | PASS | PASS | PASS | 784 | 202 | 196 | 0.704 | 1.000 | 0.759 |
| 18 | RubyGems | 18KB | 89 | PASS | PASS | PASS | 1063 | 228 | 234 | 0.616 | 1.000 | 0.863 |
| 19 | NuGet | FAIL | - | - | - | - | - | - | - | - | - | - |
| 20 | Docker Hub | FAIL | - | - | - | - | - | - | - | - | - | - |
| 21 | Terraform | FAIL | - | - | - | - | - | - | - | - | - | - |
| 22 | GitHub Explore | 395KB | 808 | PASS | PASS | PASS | 1729 | 928 | 860 | 0.887 | 1.000 | 0.877 |
| 23 | OpenStreetMap | FAIL | - | - | - | - | - | - | - | - | - | - |
| 24 | httpbin HTML | 3KB | 3 | PASS | PASS | PASS | 149 | 67 | 63 | 0.570 | 1.000 | 0.699 |
| 25 | JSON Placeholder | FAIL | - | - | - | - | - | - | - | - | - | - |
| 26 | Haskell.org | FAIL | - | - | - | - | - | - | - | - | - | - |
| 27 | Elixir Lang | FAIL | - | - | - | - | - | - | - | - | - | - |
| 28 | Zig Lang | FAIL | - | - | - | - | - | - | - | - | - | - |
| 29 | Svelte | FAIL | - | - | - | - | - | - | - | - | - | - |
| 30 | Tailwind CSS | FAIL | - | - | - | - | - | - | - | - | - | - |

## Top-3 Node Quality Analysis

Side-by-side comparison of what each reranker picks as top-3 nodes.

### Hacker News

**MiniLM top-3:**
1. `0.219` [generic] 101 points by ericlewis 12 hours ago | hide | 22 comments
2. `0.214` [generic] Hacker News new | past | comments | ask | show | jobs | submit login
3. `0.206` [generic] 5 points by Munksgaard 20 minutes ago | hide | discuss

**ColBERT top-3:**
1. `1.000` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis
2. `1.000` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis
3. `1.000` [table] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis

---

### HN Newest

**MiniLM top-3:**
1. `0.457` [text] Hacker News new | past | comments | ask | show | jobs | submit
2. `0.375` [text] new
3. `0.049` [generic] New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | submit login 1. Mad 

**ColBERT top-3:**
1. `1.000` [generic] New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | submit login 1. Mad 
2. `0.968` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Mad Bugs: Claude Wrote a Ful
3. `0.894` [text] Hacker News new | past | comments | ask | show | jobs | submit

---

### Reuters

**MiniLM top-3:**
1. `0.173` [generic] reuters.com Please enable JS and disable any ad blocker

**ColBERT top-3:**
1. `0.500` [generic] reuters.com Please enable JS and disable any ad blocker

---

### Go Dev

**MiniLM top-3:**
1. `0.698` [text] Get Started Download Go
2. `0.675` [text] "...when a programming language is designed for exactly the environment most
 of us use right now—sc
3. `0.675` [text] Build simple, secure, scalable systems with Go An open-source programming language supported by Goog

**ColBERT top-3:**
1. `1.000` [text] Build simple, secure, scalable systems with Go An open-source programming language supported by Goog
2. `1.000` [text] Build simple, secure, scalable systems with Go An open-source programming language supported by Goog
3. `0.864` [generic] The Go Programming Language Skip to Main Content Why Go arrow_drop_down Press Enter to activate/deac

---

### Node.js

**MiniLM top-3:**
1. `0.462` [text] Get Node.js® Get Node.js® Get security support for EOL Node.js versions Node.js is proudly supported
2. `0.437` [text] Node.js is proudly supported by the partners above and more .
3. `0.424` [text] Skip to content Learn About Download Blog Docs Contribute Courses Start typing... ⌘ K Run JavaScript

**ColBERT top-3:**
1. `1.000` [generic] Node.js — Run JavaScript Everywhere Skip to content Learn About Download Blog Docs Contribute Course
2. `0.956` [text] Get Node.js® Get Node.js® Get security support for EOL Node.js versions Node.js is proudly supported
3. `0.956` [text] Get Node.js® Get Node.js® Get security support for EOL Node.js versions Node.js is proudly supported

---

### docs.rs

**MiniLM top-3:**
1. `0.758` [listitem] Rust Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Doc
2. `0.706` [form] Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Build queu
3. `0.650` [list] Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Document

**ColBERT top-3:**
1. `1.000` [generic] Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Bu
2. `0.997` [generic] Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Build queu
3. `0.785` [listitem] Rust Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Doc

---

### PyPI

**MiniLM top-3:**
1. `0.945` [heading] Find, install and publish Python packages with the Python Package Index
2. `0.825` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse
3. `0.807` [text] PyPI helps you find and install software developed and shared by the Python community. Learn about i

**ColBERT top-3:**
1. `1.000` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse
2. `1.000` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse
3. `0.966` [heading] Find, install and publish Python packages with the Python Package Index

---

### pkg.go.dev

**MiniLM top-3:**
1. `0.704` [text] Packages Standard Library Sub-repositories About Go Packages
2. `0.577` [link] About Go Packages
3. `0.528` [text] Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Packages Standard Libr

**ColBERT top-3:**
1. `1.000` [link] About Go Packages
2. `0.887` [text] Packages Standard Library Sub-repositories About Go Packages
3. `0.775` [generic] Go Packages - Go Packages Skip to Main Content Why Go Case Studies Common problems companies solve w

---

### RubyGems

**MiniLM top-3:**
1. `0.616` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
2. `0.613` [text] Ruby Central
3. `0.497` [text] Operated by Ruby Central Designed by DockYard Hosted by AWS Resolved with DNSimple Monitored by Data

**ColBERT top-3:**
1. `1.000` [generic] RubyGems.org | your community gem host ⬢ RubyGems Navigation menu Releases Blog Gems Guides Sign in 
2. `0.969` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
3. `0.937` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta

---

### GitHub Explore

**MiniLM top-3:**
1. `0.887` [heading] Trending repository
2. `0.825` [text] REPOSITORIES Topics Trending Collections
3. `0.825` [text] COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Maintainer Community Acc

**ColBERT top-3:**
1. `1.000` [text] REPOSITORIES Topics Trending Collections
2. `0.817` [heading] Trending repository
3. `0.817` [heading] Trending repository

---

### httpbin HTML

**MiniLM top-3:**
1. `0.570` [heading] Herman Melville - Moby-Dick
2. `0.427` [generic] Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather that now reigned in th

**ColBERT top-3:**
1. `1.000` [heading] Herman Melville - Moby-Dick
2. `0.000` [generic] Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather that now reigned in th
3. `0.000` [generic] Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather that now reigned in th

---

