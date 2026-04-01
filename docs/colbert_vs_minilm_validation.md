# ColBERT vs MiniLM vs Hybrid — Live Validation

**Date:** 2026-03-31
**Mode:** Release build, bi-encoder (all-MiniLM-L6-v2, 384-dim) + ColBERTv2.0 (768-dim, ONNX, CPU)
**Sites:** 30 fetched / 30 total

## Summary

| Metod | Korrekthet | Avg latens | Avg top-1 score |
|-------|-----------|------------|----------------|
| MiniLM (bi-encoder) | 29/30 (96.7%) | 1175.4ms | 0.674 |
| ColBERT (MaxSim) | 29/30 (96.7%) | 691.2ms | 0.950 |
| Hybrid (adaptive α) | 29/30 (96.7%) | 679.2ms | 0.823 |

ColBERT wins (correct where MiniLM misses): **0**
Hybrid wins (correct where MiniLM misses): **0**
MiniLM-only (correct where ColBERT misses): **0**

## Per-Site Results

| # | Site | HTML | DOM | M-ok | C-ok | H-ok | M-ms | C-ms | H-ms | M-top1 | C-top1 | H-top1 |
|---|------|------|-----|------|------|------|------|------|------|--------|--------|--------|
| 1 | Hacker News | 34KB | 496 | PASS | PASS | PASS | 2328 | 3192 | 2826 | 0.214 | 1.000 | 0.851 |
| 2 | HN Newest | 40KB | 523 | PASS | PASS | PASS | 632 | 109 | 108 | 0.449 | 1.000 | 0.858 |
| 3 | Lobsters | 57KB | 484 | PASS | PASS | PASS | 1400 | 365 | 362 | 0.683 | 1.000 | 0.747 |
| 4 | CNN Lite | 330KB | 208 | PASS | PASS | PASS | 1134 | 95 | 91 | 0.563 | 1.000 | 0.896 |
| 5 | NPR Text | 5KB | 54 | PASS | PASS | PASS | 1005 | 100 | 98 | 0.794 | 1.000 | 0.856 |
| 6 | Reuters | 0KB | 1 | PASS | PASS | PASS | 69 | 36 | 35 | 0.173 | 0.500 | 0.271 |
| 7 | Rust Lang | 18KB | 79 | PASS | PASS | PASS | 1430 | 696 | 704 | 0.550 | 1.000 | 0.819 |
| 8 | MDN HTML | 173KB | 1050 | PASS | PASS | PASS | 1557 | 1480 | 1488 | 0.825 | 1.000 | 0.877 |
| 9 | Go Dev | 62KB | 245 | PASS | PASS | PASS | 1243 | 358 | 355 | 0.698 | 1.000 | 0.850 |
| 10 | TypeScript | 253KB | 201 | PASS | PASS | PASS | 1370 | 1229 | 1264 | 0.681 | 1.000 | 0.886 |
| 11 | Kotlin | 251KB | 1 | MISS | MISS | MISS | 180 | 49 | 51 | 0.182 | 0.500 | 0.278 |
| 12 | Node.js | 453KB | 32 | PASS | PASS | PASS | 948 | 472 | 458 | 0.462 | 1.000 | 0.808 |
| 13 | Ruby Lang | 88KB | 242 | PASS | PASS | PASS | 1271 | 893 | 911 | 0.712 | 1.000 | 0.940 |
| 14 | docs.rs | 16KB | 83 | PASS | PASS | PASS | 1149 | 609 | 600 | 0.758 | 1.000 | 0.869 |
| 15 | DevDocs | 8KB | 22 | PASS | PASS | PASS | 488 | 41 | 39 | 0.491 | 0.500 | 0.494 |
| 16 | PyPI | 21KB | 26 | PASS | PASS | PASS | 485 | 237 | 247 | 0.945 | 1.000 | 0.961 |
| 17 | pkg.go.dev | 32KB | 246 | PASS | PASS | PASS | 871 | 218 | 210 | 0.704 | 1.000 | 0.759 |
| 18 | RubyGems | 18KB | 89 | PASS | PASS | PASS | 1135 | 259 | 261 | 0.616 | 1.000 | 0.847 |
| 19 | NuGet | 16KB | 91 | PASS | PASS | PASS | 951 | 581 | 600 | 0.825 | 1.000 | 0.947 |
| 20 | Docker Hub | 388KB | 100 | PASS | PASS | PASS | 1007 | 951 | 927 | 0.600 | 1.000 | 0.940 |
| 21 | Terraform | 120KB | 614 | PASS | PASS | PASS | 2418 | 1146 | 1174 | 0.960 | 1.000 | 0.988 |
| 22 | GitHub Explore | 395KB | 803 | PASS | PASS | PASS | 1539 | 1037 | 985 | 0.887 | 1.000 | 0.877 |
| 23 | OpenStreetMap | 32KB | 122 | PASS | PASS | PASS | 966 | 115 | 117 | 0.499 | 1.000 | 0.767 |
| 24 | httpbin HTML | 3KB | 3 | PASS | PASS | PASS | 140 | 66 | 64 | 0.570 | 1.000 | 0.699 |
| 25 | JSON Placeholder | 8KB | 91 | PASS | PASS | PASS | 992 | 171 | 170 | 0.945 | 1.000 | 0.961 |
| 26 | Haskell.org | 63KB | 453 | PASS | PASS | PASS | 1200 | 1111 | 1117 | 0.945 | 1.000 | 0.942 |
| 27 | Elixir Lang | 26KB | 152 | PASS | PASS | PASS | 1620 | 1075 | 1075 | 0.872 | 1.000 | 0.902 |
| 28 | Zig Lang | 12KB | 118 | PASS | PASS | PASS | 1236 | 1105 | 1104 | 0.825 | 1.000 | 0.877 |
| 29 | Svelte | 87KB | 183 | PASS | PASS | PASS | 948 | 836 | 840 | 0.825 | 1.000 | 0.950 |
| 30 | Tailwind CSS | 914KB | 9013 | PASS | PASS | PASS | 3553 | 2102 | 2095 | 0.960 | 1.000 | 0.956 |

## Top-3 Node Quality Analysis

Side-by-side comparison of what each reranker picks as top-3 nodes.

### Hacker News

**MiniLM top-3:**
1. `0.214` [generic] Hacker News new | past | comments | ask | show | jobs | submit login
2. `0.210` [generic] 104 points by ericlewis 12 hours ago | hide | 23 comments
3. `0.206` [generic] 5 points by jruohonen 1 hour ago | hide | discuss

**ColBERT top-3:**
1. `1.000` [table] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis
2. `1.000` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis
3. `1.000` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis

---

### HN Newest

**MiniLM top-3:**
1. `0.449` [text] Hacker News new | past | comments | ask | show | jobs | submit
2. `0.375` [text] new
3. `0.056` [generic] New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | submit login 1. The 

**ColBERT top-3:**
1. `1.000` [generic] New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | submit login 1. The 
2. `0.850` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. The German state (Schleswig-
3. `0.785` [text] Hacker News new | past | comments | ask | show | jobs | submit

---

### Lobsters

**MiniLM top-3:**
1. `0.683` [heading] Your job isn't programming
2. `0.663` [link] Programming language theory, types, design
3. `0.639` [text] ask programming

**ColBERT top-3:**
1. `1.000` [text] ask programming
2. `0.900` [link] Programming language theory, types, design
3. `0.761` [text] Your job isn't programming practices codeandcake.dev authored by nick4 38 hours ago | caches | 47 co

---

### CNN Lite

**MiniLM top-3:**
1. `0.563` [link] Trump’s top litigator faces uphill battle with birthright citizenship
2. `0.563` [link] Behind the scenes and in front of cameras, Hegseth serving as top cheerleader for military power in 
3. `0.514` [text] Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery Company. All Rights

**ColBERT top-3:**
1. `1.000` [generic] Breaking News, Latest News and Videos | CNN CNN 4/1/2026 Latest Stories Is China positioning itself 
2. `0.643` [text] Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery Company. All Rights
3. `0.168` [link] Trump’s top litigator faces uphill battle with birthright citizenship

---

### NPR Text

**MiniLM top-3:**
1. `0.794` [heading] NPR : National Public Radio
2. `0.595` [text] NPR : National Public Radio Wednesday, April 1, 2026 Judge rules White House ballroom construction m
3. `0.489` [link] News

**ColBERT top-3:**
1. `1.000` [heading] NPR : National Public Radio
2. `0.484` [generic] NPR : National Public Radio Text-Only Version Go To Full Site NPR : National Public Radio Wednesday,
3. `0.359` [generic] Text-Only Version Go To Full Site NPR : National Public Radio Wednesday, April 1, 2026 Judge rules W

---

### Reuters

**MiniLM top-3:**
1. `0.173` [generic] reuters.com Please enable JS and disable any ad blocker

**ColBERT top-3:**
1. `0.500` [generic] reuters.com Please enable JS and disable any ad blocker

---

### Rust Lang

**MiniLM top-3:**
1. `0.550` [heading] Read Rust
2. `0.540` [heading] Watch Rust
3. `0.414` [text] Rust in production Hundreds of companies around the world are using Rust in production
today for fas

**ColBERT top-3:**
1. `1.000` [generic] Rust Programming Language Install Learn Playground Tools Governance Community Blog Language English 
2. `0.854` [generic] Install Learn Playground Tools Governance Community Blog Language English (en-US) Español (es) Franç
3. `0.475` [text] In 2018, the Rust community decided to improve the programming experience
for a few distinct domains

---

### MDN HTML

**MiniLM top-3:**
1. `0.825` [text] Reference for all HTML elements .
2. `0.825` [text] HTML HTML: Markup language HTML reference Elements Global attributes Attributes See all… HTML guides
3. `0.825` [text] HTML reference Elements Global attributes Attributes See all… HTML guides Responsive images HTML che

**ColBERT top-3:**
1. `1.000` [generic] Reference for all HTML elements .
2. `1.000` [text] Reference for all HTML elements .
3. `0.797` [generic] HTML reference Elements Global attributes Attributes See all…

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

### TypeScript

**MiniLM top-3:**
1. `0.681` [heading] Using TypeScript
2. `0.673` [heading] What is TypeScript?
3. `0.655` [heading] TypeScript is JavaScript with syntax for types.

**ColBERT top-3:**
1. `1.000` [generic] TypeScript: JavaScript With Syntax For Types. Skip to main content TypeScript Download Docs Handbook
2. `0.986` [text] TypeScript
3. `0.966` [text] TypeScript Download Docs Handbook Community Playground Tools

---

### Kotlin

**MiniLM top-3:**
1. `0.182` [data] isDarkTheme: true

**ColBERT top-3:**
1. `0.500` [data] isDarkTheme: true

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

### Ruby Lang

**MiniLM top-3:**
1. `0.712` [text] “ Ruby is just the most beautiful programming language I have ever seen. And I pay a fair amount of 
2. `0.676` [text] Since 1995 Ruby Latest Version: 4.0.2 Download
3. `0.667` [text] A Programmer's Best Friend Since 1995 Ruby Latest Version: 4.0.2 Download

**ColBERT top-3:**
1. `1.000` [generic] Ruby Programming Language Ruby Install Docs Libraries Contribution Community News English ( en ) Бъл
2. `0.841` [text] A Programmer's Best Friend Since 1995 Ruby Latest Version: 4.0.2 Download
3. `0.841` [text] A Programmer's Best Friend Since 1995 Ruby Latest Version: 4.0.2 Download

---

### docs.rs

**MiniLM top-3:**
1. `0.758` [listitem] Rust Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Doc
2. `0.712` [form] Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Build queu
3. `0.662` [listitem] Rust website

**ColBERT top-3:**
1. `1.000` [generic] Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Build queu
2. `0.997` [generic] Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Bu
3. `0.780` [list] Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Document

---

### DevDocs

**MiniLM top-3:**
1. `0.491` [generic] DevDocs API Documentation Clear search DevDocs Preferences Offline Data Changelog Guide About Report

**ColBERT top-3:**
1. `0.500` [generic] DevDocs API Documentation Clear search DevDocs Preferences Offline Data Changelog Guide About Report

---

### PyPI

**MiniLM top-3:**
1. `0.945` [heading] Find, install and publish Python packages with the Python Package Index
2. `0.825` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse
3. `0.807` [text] PyPI helps you find and install software developed and shared by the Python community. Learn about i

**ColBERT top-3:**
1. `1.000` [heading] Find, install and publish Python packages with the Python Package Index
2. `0.969` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse
3. `0.969` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse

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
2. `0.946` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
3. `0.936` [generic] ⬢ RubyGems Navigation menu Releases Blog Gems Guides Sign in Sign up Find, install, and publish Ruby

---

### NuGet

**MiniLM top-3:**
1. `0.825` [text] NuGet is the package manager for .NET. The NuGet client tools provide the ability to produce and con
2. `0.825` [text] What is NuGet? NuGet is the package manager for .NET. The NuGet client tools provide the ability to 
3. `0.675` [text] Create .NET apps faster with NuGet 0 package downloads 0 package versions 0 unique packages

**ColBERT top-3:**
1. `1.000` [text] NuGet is the package manager for .NET. The NuGet client tools provide the ability to produce and con
2. `0.968` [text] What is NuGet? NuGet is the package manager for .NET. The NuGet client tools provide the ability to 
3. `0.956` [main] Create .NET apps faster with NuGet 0 package downloads 0 package versions 0 unique packages What is 

---

### Docker Hub

**MiniLM top-3:**
1. `0.600` [generic] Docker Hub Container Image Library | App Containerization Search Docker Hub K Help Back Documentatio
2. `0.504` [heading] Docker Hardened Images - Now Free
3. `0.488` [text] Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System status ⁠ System theme 

**ColBERT top-3:**
1. `1.000` [generic] Docker Hub Container Image Library | App Containerization Search Docker Hub K Help Back Documentatio
2. `0.841` [generic] Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System status ⁠ System theme 
3. `0.841` [text] Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System status ⁠ System theme 

---

### Terraform

**MiniLM top-3:**
1. `0.960` [data] content.get_started.body: Follow a code-complete, hands-on tutorial to learn the Terraform basics wi
2. `0.960` [data] content.overview.body: Terraform is an infrastructure as code tool that lets you build, change, and 
3. `0.690` [data] product.rootDocsPaths[0].iconName: code

**ColBERT top-3:**
1. `1.000` [data] content.overview.body: Terraform is an infrastructure as code tool that lets you build, change, and 
2. `0.907` [generic] Terraform | HashiCorp Developer HashiConf 2025 Don't miss the live stream of HashiConf Day 2 happeni
3. `0.763` [data] content.get_started.body: Follow a code-complete, hands-on tutorial to learn the Terraform basics wi

---

### GitHub Explore

**MiniLM top-3:**
1. `0.887` [heading] Trending repository
2. `0.825` [text] REPOSITORIES Topics Trending Collections
3. `0.825` [text] COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Maintainer Community Acc

**ColBERT top-3:**
1. `1.000` [text] REPOSITORIES Topics Trending Collections
2. `0.789` [heading] Trending repository
3. `0.789` [heading] Trending repository

---

### OpenStreetMap

**MiniLM top-3:**
1. `0.499` [text] OpenStreetMap is a map of the world, created by people like you and free to use under an open licens
2. `0.415` [text] Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people like you and free t
3. `0.398` [text] Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! OpenStreetMap is a map

**ColBERT top-3:**
1. `1.000` [generic] OpenStreetMap Where is this? GraphHopper OSRM Valhalla Edit Edit with iD (in-browser editor) Edit wi
2. `0.984` [generic] OpenStreetMap OpenStreetMap Where is this? GraphHopper OSRM Valhalla Edit Edit with iD (in-browser e
3. `0.289` [text] Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people like you and free t

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

### JSON Placeholder

**MiniLM top-3:**
1. `0.945` [heading] Free fake and reliable API for testing and prototyping.
2. `0.825` [text] {JSON} Placeholder Free fake and reliable API for testing and prototyping. Powered by JSON Server + 
3. `0.735` [text] JSONPlaceholder is a free online REST API that you can use whenever you need some fake data . It can

**ColBERT top-3:**
1. `1.000` [heading] Free fake and reliable API for testing and prototyping.
2. `0.829` [generic] {JSON} Placeholder Free fake and reliable API for testing and prototyping. Powered by JSON Server + 
3. `0.829` [text] {JSON} Placeholder Free fake and reliable API for testing and prototyping. Powered by JSON Server + 

---

### Haskell.org

**MiniLM top-3:**
1. `0.945` [heading] Smart contract systems are largely about programming languages, 
and when it comes to programming la
2. `0.825` [text] IOHK Smart contract systems are largely about programming languages, 
and when it comes to programmi
3. `0.814` [heading] Functional Programming &Haskell, by Computerphile / John Hughes

**ColBERT top-3:**
1. `1.000` [text] Abstraction Build powerful abstractions that are not possible in other languages. Only your imaginat
2. `0.983` [text] Why Haskell? A new paradigm Express your ideas clearly and learn a new way of thinking about program
3. `0.983` [text] Why Haskell? A new paradigm Express your ideas clearly and learn a new way of thinking about program

---

### Elixir Lang

**MiniLM top-3:**
1. `0.872` [heading] Elixir is a dynamic, functional language for building scalable and maintainable applications.
2. `0.678` [heading] Functional programming
3. `0.659` [text] Elixir has been designed to be extensible, allowing developers to naturally extend the language to p

**ColBERT top-3:**
1. `1.000` [generic] The Elixir programming language Home Install Learning Docs Guides Cases Blog Elixir is a dynamic, fu
2. `0.971` [heading] Elixir is a dynamic, functional language for building scalable and maintainable applications.
3. `0.890` [text] Check our Getting Started guide and our Learning page to begin your journey with Elixir. Or keep scr

---

### Zig Lang

**MiniLM top-3:**
1. `0.825` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu
2. `0.675` [text] Focus on debugging your application rather than debugging your programming language knowledge.
3. `0.657` [text] ⚡ A Simple Language Focus on debugging your application rather than debugging your programming langu

**ColBERT top-3:**
1. `1.000` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu
2. `1.000` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu
3. `1.000` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu

---

### Svelte

**MiniLM top-3:**
1. `0.825` [text] Svelte is a UI framework that uses a compiler to let you write breathtakingly concise
			components 
2. `0.744` [text] attractively thin, graceful and stylish Svelte is a UI framework that uses a compiler to let you wri
3. `0.737` [heading] Svelte

**ColBERT top-3:**
1. `1.000` [text] Svelte is a UI framework that uses a compiler to let you write breathtakingly concise
			components 
2. `0.999` [main] Svelte web development for the rest of us get started attractively thin, graceful and stylish Svelte
3. `0.999` [text] Svelte web development for the rest of us get started attractively thin, graceful and stylish Svelte

---

### Tailwind CSS

**MiniLM top-3:**
1. `0.960` [data] _rsc_333[1][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we
2. `0.960` [data] _rsc_333[3][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we
3. `0.960` [data] _rsc_333[10][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern w

**ColBERT top-3:**
1. `1.000` [generic] Tailwind CSS - Rapidly build modern websites without ever leaving your HTML. v 4.2 ⌘K Ctrl K Docs Bl
2. `0.946` [data] _rsc_333[10][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern w
3. `0.943` [data] _rsc_333[1][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we

---

