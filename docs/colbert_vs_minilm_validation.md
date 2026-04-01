# ColBERT vs MiniLM vs Hybrid — Live Validation

**Date:** 2026-03-31
**Mode:** Release build, bi-encoder (all-MiniLM-L6-v2, 384-dim) + ColBERTv2.0 (768-dim, ONNX, CPU)
**Sites:** 30 fetched / 30 total

## Summary

| Metod | Korrekthet | Avg latens | Avg top-1 score |
|-------|-----------|------------|----------------|
| MiniLM (bi-encoder) | 29/30 (96.7%) | 1234.0ms | 0.675 |
| ColBERT (MaxSim) | 29/30 (96.7%) | 433.5ms | 0.950 |
| Hybrid (adaptive α) | 29/30 (96.7%) | 430.9ms | 0.817 |

ColBERT wins (correct where MiniLM misses): **0**
Hybrid wins (correct where MiniLM misses): **0**
MiniLM-only (correct where ColBERT misses): **0**

## Per-Site Results

| # | Site | HTML | DOM | M-ok | C-ok | H-ok | M-ms | C-ms | H-ms | M-top1 | C-top1 | H-top1 |
|---|------|------|-----|------|------|------|------|------|------|--------|--------|--------|
| 1 | Hacker News | 34KB | 496 | PASS | PASS | PASS | 2462 | 865 | 886 | 0.238 | 1.000 | 0.850 |
| 2 | HN Newest | 40KB | 521 | PASS | PASS | PASS | 737 | 146 | 144 | 0.452 | 1.000 | 0.861 |
| 3 | Lobsters | 57KB | 484 | PASS | PASS | PASS | 1442 | 476 | 472 | 0.683 | 1.000 | 0.714 |
| 4 | CNN Lite | 330KB | 208 | PASS | PASS | PASS | 1287 | 116 | 121 | 0.563 | 1.000 | 0.893 |
| 5 | NPR Text | 5KB | 54 | PASS | PASS | PASS | 1085 | 127 | 132 | 0.794 | 1.000 | 0.939 |
| 6 | Reuters | 0KB | 1 | PASS | PASS | PASS | 78 | 50 | 47 | 0.173 | 0.500 | 0.271 |
| 7 | Rust Lang | 18KB | 79 | PASS | PASS | PASS | 1603 | 589 | 526 | 0.550 | 1.000 | 0.884 |
| 8 | MDN HTML | 173KB | 1050 | PASS | PASS | PASS | 1637 | 707 | 647 | 0.825 | 1.000 | 0.880 |
| 9 | Go Dev | 62KB | 245 | PASS | PASS | PASS | 1318 | 463 | 492 | 0.698 | 1.000 | 0.893 |
| 10 | TypeScript | 253KB | 201 | PASS | PASS | PASS | 1403 | 570 | 571 | 0.681 | 1.000 | 0.850 |
| 11 | Kotlin | 251KB | 1 | MISS | MISS | MISS | 116 | 49 | 51 | 0.182 | 0.500 | 0.278 |
| 12 | Node.js | 448KB | 32 | PASS | PASS | PASS | 930 | 516 | 521 | 0.462 | 1.000 | 0.850 |
| 13 | Ruby Lang | 88KB | 242 | PASS | PASS | PASS | 1181 | 444 | 440 | 0.712 | 1.000 | 0.869 |
| 14 | docs.rs | 17KB | 83 | PASS | PASS | PASS | 1259 | 552 | 543 | 0.758 | 1.000 | 0.865 |
| 15 | DevDocs | 8KB | 22 | PASS | PASS | PASS | 492 | 45 | 44 | 0.491 | 0.500 | 0.494 |
| 16 | PyPI | 21KB | 26 | PASS | PASS | PASS | 491 | 328 | 310 | 0.945 | 1.000 | 0.927 |
| 17 | pkg.go.dev | 32KB | 246 | PASS | PASS | PASS | 828 | 268 | 269 | 0.704 | 1.000 | 0.893 |
| 18 | RubyGems | 18KB | 89 | PASS | PASS | PASS | 1124 | 336 | 329 | 0.616 | 1.000 | 0.857 |
| 19 | NuGet | 16KB | 91 | PASS | PASS | PASS | 972 | 621 | 629 | 0.825 | 1.000 | 0.859 |
| 20 | Docker Hub | 388KB | 100 | PASS | PASS | PASS | 950 | 523 | 527 | 0.600 | 1.000 | 0.932 |
| 21 | Terraform | 120KB | 614 | PASS | PASS | PASS | 2546 | 889 | 792 | 0.960 | 1.000 | 0.988 |
| 22 | GitHub Explore | 395KB | 797 | PASS | PASS | PASS | 1638 | 796 | 806 | 0.887 | 1.000 | 0.774 |
| 23 | OpenStreetMap | 32KB | 122 | PASS | PASS | PASS | 1069 | 161 | 193 | 0.499 | 1.000 | 0.819 |
| 24 | httpbin HTML | 3KB | 3 | PASS | PASS | PASS | 158 | 85 | 82 | 0.570 | 1.000 | 0.828 |
| 25 | JSON Placeholder | 8KB | 91 | PASS | PASS | PASS | 1057 | 238 | 247 | 0.945 | 1.000 | 0.850 |
| 26 | Haskell.org | 63KB | 453 | PASS | PASS | PASS | 1240 | 546 | 582 | 0.945 | 1.000 | 0.840 |
| 27 | Elixir Lang | 26KB | 152 | PASS | PASS | PASS | 1731 | 540 | 529 | 0.872 | 1.000 | 0.845 |
| 28 | Zig Lang | 12KB | 118 | PASS | PASS | PASS | 1361 | 537 | 562 | 0.825 | 1.000 | 0.920 |
| 29 | Svelte | 87KB | 183 | PASS | PASS | PASS | 1035 | 562 | 558 | 0.825 | 1.000 | 0.951 |
| 30 | Tailwind CSS | 912KB | 9000 | PASS | PASS | PASS | 3792 | 859 | 872 | 0.960 | 1.000 | 0.840 |

## Top-3 Node Quality Analysis

Side-by-side comparison of what each reranker picks as top-3 nodes.

### Hacker News

**MiniLM top-3:**
1. `0.238` [generic] 10 points by jruohonen 2 hours ago | hide | discuss
2. `0.216` [generic] 7 points by wazHFsRy 1 hour ago | hide | discuss
3. `0.214` [generic] Hacker News new | past | comments | ask | show | jobs | submit login

**ColBERT top-3:**
1. `1.000` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis
2. `1.000` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis
3. `1.000` [table] Hacker News new | past | comments | ask | show | jobs | submit login 1. Claude Code Unpacked : A vis

---

### HN Newest

**MiniLM top-3:**
1. `0.452` [text] Hacker News new | past | comments | ask | show | jobs | submit
2. `0.375` [text] new
3. `0.075` [generic] New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | submit login 1. Atem

**ColBERT top-3:**
1. `1.000` [generic] New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | submit login 1. Atem
2. `0.990` [generic] Hacker News new | past | comments | ask | show | jobs | submit login 1. Atemis II Launch ( esa.int )
3. `0.046` [text] Hacker News new | past | comments | ask | show | jobs | submit

---

### Lobsters

**MiniLM top-3:**
1. `0.683` [heading] Your job isn't programming
2. `0.663` [link] Programming language theory, types, design
3. `0.639` [text] ask programming

**ColBERT top-3:**
1. `1.000` [text] Your job isn't programming practices codeandcake.dev authored by nick4 39 hours ago | caches | 47 co
2. `0.810` [text] 51 Your job isn't programming practices codeandcake.dev authored by nick4 39 hours ago | caches | 47
3. `0.307` [text] 52 Why have supply chain attacks become a near daily occurrence ? ☶ ask programming authored by dhru

---

### CNN Lite

**MiniLM top-3:**
1. `0.563` [link] Trump’s top litigator faces uphill battle with birthright citizenship
2. `0.563` [link] Behind the scenes and in front of cameras, Hegseth serving as top cheerleader for military power in 
3. `0.514` [text] Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery Company. All Rights

**ColBERT top-3:**
1. `1.000` [generic] Breaking News, Latest News and Videos | CNN CNN 4/1/2026 Latest Stories Walking away from the Strait
2. `0.615` [text] Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery Company. All Rights
3. `0.178` [link] Behind the scenes and in front of cameras, Hegseth serving as top cheerleader for military power in 

---

### NPR Text

**MiniLM top-3:**
1. `0.794` [heading] NPR : National Public Radio
2. `0.595` [text] NPR : National Public Radio Wednesday, April 1, 2026 Judge rules White House ballroom construction m
3. `0.489` [link] News

**ColBERT top-3:**
1. `1.000` [text] NPR : National Public Radio Wednesday, April 1, 2026 Judge rules White House ballroom construction m
2. `0.899` [generic] Text-Only Version Go To Full Site NPR : National Public Radio Wednesday, April 1, 2026 Judge rules W
3. `0.880` [generic] NPR : National Public Radio Text-Only Version Go To Full Site NPR : National Public Radio Wednesday,

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
1. `1.000` [text] Build it in Rust In 2018, the Rust community decided to improve the programming experience
for a few
2. `0.999` [generic] Install Learn Playground Tools Governance Community Blog Language English (en-US) Español (es) Franç
3. `0.988` [text] Why Rust? Performance Rust is blazingly fast and memory-efficient: with no runtime or
garbage collec

---

### MDN HTML

**MiniLM top-3:**
1. `0.825` [text] Reference for all HTML elements .
2. `0.825` [text] HTML HTML: Markup language HTML reference Elements Global attributes Attributes See all… HTML guides
3. `0.825` [text] HTML reference Elements Global attributes Attributes See all… HTML guides Responsive images HTML che

**ColBERT top-3:**
1. `1.000` [generic] HTML elements Reference for all HTML elements . HTML attributes Reference for all HTML attributes. A
2. `0.590` [text] HTML consists of elements , each of which may be modified by some number of attributes . HTML docume
3. `0.362` [text] HTML: Markup language HTML reference Elements Global attributes Attributes See all… HTML guides Resp

---

### Go Dev

**MiniLM top-3:**
1. `0.698` [text] Get Started Download Go
2. `0.675` [text] "...when a programming language is designed for exactly the environment most
 of us use right now—sc
3. `0.675` [text] Build simple, secure, scalable systems with Go An open-source programming language supported by Goog

**ColBERT top-3:**
1. `1.000` [text] “At the time, no single team member knew Go, but within a month, everyone was writing in Go and we w
2. `1.000` [text] “At the time, no single team member knew Go, but within a month, everyone was writing in Go and we w
3. `1.000` [text] “At the time, no single team member knew Go, but within a month, everyone was writing in Go and we w

---

### TypeScript

**MiniLM top-3:**
1. `0.681` [heading] Using TypeScript
2. `0.673` [heading] What is TypeScript?
3. `0.655` [heading] TypeScript is JavaScript with syntax for types.

**ColBERT top-3:**
1. `1.000` [text] Describe Your Data Describe the shape of objects and functions in your code. Making it possible to s
2. `1.000` [text] Describe Your Data Describe the shape of objects and functions in your code. Making it possible to s
3. `1.000` [text] Describe Your Data Describe the shape of objects and functions in your code. Making it possible to s

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
1. `1.000` [text] Run JavaScript Everywhere Node.js® is a free, open-source, cross-platform JavaScript runtime environ
2. `1.000` [main] Run JavaScript Everywhere Node.js® is a free, open-source, cross-platform JavaScript runtime environ
3. `0.890` [generic] Skip to content Learn About Download Blog Docs Contribute Courses Start typing... ⌘ K Run JavaScript

---

### Ruby Lang

**MiniLM top-3:**
1. `0.712` [text] “ Ruby is just the most beautiful programming language I have ever seen. And I pay a fair amount of 
2. `0.676` [text] Since 1995 Ruby Latest Version: 4.0.2 Download
3. `0.667` [text] A Programmer's Best Friend Since 1995 Ruby Latest Version: 4.0.2 Download

**ColBERT top-3:**
1. `1.000` [text] “ Ruby turns ideas into code fast.
Its simplicity keeps me focused; its expressiveness lets me write
2. `0.937` [text] “ Ruby is just the most beautiful programming language I have ever seen. And I pay a fair amount of 
3. `0.937` [text] “ Ruby is just the most beautiful programming language I have ever seen. And I pay a fair amount of 

---

### docs.rs

**MiniLM top-3:**
1. `0.758` [listitem] Rust Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Doc
2. `0.678` [form] Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Build queu
3. `0.650` [list] Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Document

**ColBERT top-3:**
1. `1.000` [generic] Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Build queu
2. `0.991` [generic] Docs.rs Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Bu
3. `0.427` [listitem] tako-rs-1.1.1 Multi-transport Rust framework for modern network services. 3 minutes ago

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
1. `1.000` [generic] PyPI · The Python Package Index Skip to main content Switch to mobile version Help Docs Sponsors Log
2. `0.939` [main] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse
3. `0.843` [generic] Skip to main content Switch to mobile version Help Docs Sponsors Log in Register Menu Help Docs Spon

---

### pkg.go.dev

**MiniLM top-3:**
1. `0.704` [text] Packages Standard Library Sub-repositories About Go Packages
2. `0.577` [link] About Go Packages
3. `0.528` [text] Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Packages Standard Libr

**ColBERT top-3:**
1. `1.000` [generic] Go Packages - Go Packages Skip to Main Content Why Go Case Studies Common problems companies solve w
2. `0.991` [text] Why Go Case Studies Common problems companies solve with Go Use Cases Stories about how and why comp
3. `0.955` [generic] Skip to Main Content Why Go Case Studies Common problems companies solve with Go Use Cases Stories a

---

### RubyGems

**MiniLM top-3:**
1. `0.616` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
2. `0.613` [text] Ruby Central
3. `0.497` [text] Operated by Ruby Central Designed by DockYard Hosted by AWS Resolved with DNSimple Monitored by Data

**ColBERT top-3:**
1. `1.000` [text] The RubyGems.org website and service are maintained and operated by Ruby Central’s Open Source Progr
2. `0.980` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
3. `0.980` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta

---

### NuGet

**MiniLM top-3:**
1. `0.825` [text] NuGet is the package manager for .NET. The NuGet client tools provide the ability to produce and con
2. `0.825` [text] What is NuGet? NuGet is the package manager for .NET. The NuGet client tools provide the ability to 
3. `0.675` [text] Create .NET apps faster with NuGet 0 package downloads 0 package versions 0 unique packages

**ColBERT top-3:**
1. `1.000` [text] What is NuGet? NuGet is the package manager for .NET. The NuGet client tools provide the ability to 
2. `0.970` [generic] NuGet Gallery       | Home Skip To Content Toggle navigation Packages Upload Statistics Documentatio
3. `0.943` [generic] Skip To Content Toggle navigation Packages Upload Statistics Documentation Downloads Blog Sign in Cr

---

### Docker Hub

**MiniLM top-3:**
1. `0.600` [generic] Docker Hub Container Image Library | App Containerization Search Docker Hub K Help Back Documentatio
2. `0.504` [heading] Docker Hardened Images - Now Free
3. `0.488` [text] Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System status ⁠ System theme 

**ColBERT top-3:**
1. `1.000` [text] Software supply chain Secure Your Supply Chain with Docker Hardened Images Use Docker's enterprise-g
2. `1.000` [link] Software supply chain Secure Your Supply Chain with Docker Hardened Images Use Docker's enterprise-g
3. `0.991` [generic] Docker Hub Container Image Library | App Containerization Search Docker Hub K Help Back Documentatio

---

### Terraform

**MiniLM top-3:**
1. `0.960` [data] content.get_started.body: Follow a code-complete, hands-on tutorial to learn the Terraform basics wi
2. `0.960` [data] content.overview.body: Terraform is an infrastructure as code tool that lets you build, change, and 
3. `0.690` [data] product.rootDocsPaths[0].iconName: code

**ColBERT top-3:**
1. `1.000` [data] content.overview.body: Terraform is an infrastructure as code tool that lets you build, change, and 
2. `0.946` [data] layoutProps.sidebarNavDataLevels[1].menuItems[4].routes[5].badge.color: highlight
3. `0.932` [data] layoutProps.sidebarNavDataLevels[1].menuItems[4].routes[5].badge.text: BETA

---

### GitHub Explore

**MiniLM top-3:**
1. `0.887` [heading] Trending repository
2. `0.825` [text] REPOSITORIES Topics Trending Collections
3. `0.825` [text] COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Maintainer Community Acc

**ColBERT top-3:**
1. `1.000` [text] Search or jump to... Search code, repositories, users, issues, pull requests... Search Clear Search 
2. `0.853` [text] Search code, repositories, users, issues, pull requests... Search Clear Search syntax tips
3. `0.728` [text] COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Maintainer Community Acc

---

### OpenStreetMap

**MiniLM top-3:**
1. `0.499` [text] OpenStreetMap is a map of the world, created by people like you and free to use under an open licens
2. `0.415` [text] Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people like you and free t
3. `0.398` [text] Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! OpenStreetMap is a map

**ColBERT top-3:**
1. `1.000` [text] Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! OpenStreetMap is a map
2. `0.934` [text] Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! OpenStreetMap is a map
3. `0.911` [generic] OpenStreetMap OpenStreetMap Where is this? GraphHopper OSRM Valhalla Edit Edit with iD (in-browser e

---

### httpbin HTML

**MiniLM top-3:**
1. `0.570` [heading] Herman Melville - Moby-Dick
2. `0.427` [generic] Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather that now reigned in th

**ColBERT top-3:**
1. `1.000` [generic] Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather that now reigned in th
2. `1.000` [generic] Herman Melville - Moby-Dick Availing himself of the mild, summer-cool weather that now reigned in th
3. `0.000` [heading] Herman Melville - Moby-Dick

---

### JSON Placeholder

**MiniLM top-3:**
1. `0.945` [heading] Free fake and reliable API for testing and prototyping.
2. `0.825` [text] {JSON} Placeholder Free fake and reliable API for testing and prototyping. Powered by JSON Server + 
3. `0.735` [text] JSONPlaceholder is a free online REST API that you can use whenever you need some fake data . It can

**ColBERT top-3:**
1. `1.000` [text] {JSON} Placeholder Free fake and reliable API for testing and prototyping. Powered by JSON Server + 
2. `0.903` [main] When to use JSONPlaceholder is a free online REST API that you can use whenever you need some fake d
3. `0.816` [generic] Check my new project 💧 MistCSS write React components with 50% less code JSONPlaceholder Guide Spons

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
2. `0.946` [text] A new paradigm Express your ideas clearly and learn a new way of thinking about programming. Based o
3. `0.911` [text] Why Haskell? A new paradigm Express your ideas clearly and learn a new way of thinking about program

---

### Elixir Lang

**MiniLM top-3:**
1. `0.872` [heading] Elixir is a dynamic, functional language for building scalable and maintainable applications.
2. `0.678` [heading] Functional programming
3. `0.659` [text] Elixir has been designed to be extensible, allowing developers to naturally extend the language to p

**ColBERT top-3:**
1. `1.000` [generic] Home Install Learning Docs Guides Cases Blog Elixir is a dynamic, functional language for building s
2. `1.000` [text] Home Install Learning Docs Guides Cases Blog Elixir is a dynamic, functional language for building s
3. `1.000` [text] Home Install Learning Docs Guides Cases Blog Elixir is a dynamic, functional language for building s

---

### Zig Lang

**MiniLM top-3:**
1. `0.825` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu
2. `0.675` [text] Focus on debugging your application rather than debugging your programming language knowledge.
3. `0.657` [text] ⚡ A Simple Language Focus on debugging your application rather than debugging your programming langu

**ColBERT top-3:**
1. `1.000` [text] Zig Software Foundation The ZSF is a 501(c)(3) non-profit corporation. The Zig Software Foundation i
2. `1.000` [text] Zig Software Foundation The ZSF is a 501(c)(3) non-profit corporation. The Zig Software Foundation i
3. `0.952` [text] Zig Software Foundation The ZSF is a 501(c)(3) non-profit corporation. The Zig Software Foundation i

---

### Svelte

**MiniLM top-3:**
1. `0.825` [text] Svelte is a UI framework that uses a compiler to let you write breathtakingly concise
			components 
2. `0.744` [text] attractively thin, graceful and stylish Svelte is a UI framework that uses a compiler to let you wri
3. `0.737` [heading] Svelte

**ColBERT top-3:**
1. `1.000` [generic] Skip to main content Docs Svelte SvelteKit CLI AI Tutorial Packages Playground Blog Svelte web devel
2. `1.000` [text] Skip to main content Docs Svelte SvelteKit CLI AI Tutorial Packages Playground Blog Svelte web devel
3. `0.942` [text] attractively thin, graceful and stylish Svelte is a UI framework that uses a compiler to let you wri

---

### Tailwind CSS

**MiniLM top-3:**
1. `0.960` [data] _rsc_332[3][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we
2. `0.960` [data] _rsc_332[1][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we
3. `0.960` [data] _rsc_332[10][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern w

**ColBERT top-3:**
1. `1.000` [data] _rsc_10[3].children[3].children[0][3].children[0][3].children[0][3].children[1][3].children[1][3].ch
2. `0.869` [data] _rsc_212[3].children[3].children[0][3].children[1]: button
3. `0.846` [data] _rsc_191[3].children[1][3].children[0][3].children[1][3].children[1][0]: $

---

