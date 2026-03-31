# ColBERT vs MiniLM vs Hybrid — Live Validation

**Date:** 2026-03-31
**Mode:** Release build, bi-encoder (all-MiniLM-L6-v2, 384-dim) + ColBERTv2.0 (768-dim, ONNX, CPU)
**Sites:** 30 fetched / 30 total

## Summary

| Metod | Korrekthet | Avg latens | Avg top-1 score |
|-------|-----------|------------|----------------|
| MiniLM (bi-encoder) | 29/30 (96.7%) | 1215.6ms | 0.674 |
| ColBERT (MaxSim) | 29/30 (96.7%) | 3589.6ms | 0.950 |
| Hybrid (adaptive α) | 29/30 (96.7%) | 3529.4ms | 0.789 |

ColBERT wins (correct where MiniLM misses): **0**
Hybrid wins (correct where MiniLM misses): **0**
MiniLM-only (correct where ColBERT misses): **0**

## Per-Site Results

| # | Site | HTML | DOM | M-ok | C-ok | H-ok | M-ms | C-ms | H-ms | M-top1 | C-top1 | H-top1 |
|---|------|------|-----|------|------|------|------|------|------|--------|--------|--------|
| 1 | Hacker News | 33KB | 490 | PASS | PASS | PASS | 2472 | 10500 | 9811 | 0.214 | 1.000 | 0.627 |
| 2 | HN Newest | 40KB | 523 | PASS | PASS | PASS | 775 | 605 | 614 | 0.456 | 1.000 | 0.619 |
| 3 | Lobsters | 57KB | 489 | PASS | PASS | PASS | 1314 | 2668 | 2561 | 0.683 | 1.000 | 0.764 |
| 4 | CNN Lite | 331KB | 208 | PASS | PASS | PASS | 949 | 927 | 913 | 0.563 | 1.000 | 0.906 |
| 5 | NPR Text | 5KB | 54 | PASS | PASS | PASS | 1093 | 610 | 605 | 0.794 | 1.000 | 0.643 |
| 6 | Reuters | 0KB | 1 | PASS | PASS | PASS | 77 | 198 | 196 | 0.173 | 0.500 | 0.271 |
| 7 | Rust Lang | 18KB | 79 | PASS | PASS | PASS | 1582 | 4062 | 3996 | 0.550 | 1.000 | 0.819 |
| 8 | MDN HTML | 173KB | 1050 | PASS | PASS | PASS | 1624 | 8166 | 8059 | 0.825 | 1.000 | 0.877 |
| 9 | Go Dev | 62KB | 245 | PASS | PASS | PASS | 1305 | 2213 | 2140 | 0.698 | 1.000 | 0.850 |
| 10 | TypeScript | 253KB | 201 | PASS | PASS | PASS | 1424 | 6327 | 6315 | 0.681 | 1.000 | 0.720 |
| 11 | Kotlin | 251KB | 1 | MISS | MISS | MISS | 119 | 202 | 202 | 0.182 | 0.500 | 0.278 |
| 12 | Node.js | 450KB | 32 | PASS | PASS | PASS | 923 | 2340 | 2281 | 0.462 | 1.000 | 0.808 |
| 13 | Ruby Lang | 88KB | 242 | PASS | PASS | PASS | 1204 | 4687 | 4629 | 0.712 | 1.000 | 0.940 |
| 14 | docs.rs | 17KB | 83 | PASS | PASS | PASS | 1147 | 2949 | 2764 | 0.758 | 1.000 | 0.830 |
| 15 | DevDocs | 8KB | 22 | PASS | PASS | PASS | 496 | 201 | 200 | 0.491 | 0.500 | 0.494 |
| 16 | PyPI | 21KB | 26 | PASS | PASS | PASS | 496 | 1488 | 1450 | 0.945 | 1.000 | 0.961 |
| 17 | pkg.go.dev | 32KB | 246 | PASS | PASS | PASS | 836 | 1201 | 1170 | 0.704 | 1.000 | 0.717 |
| 18 | RubyGems | 18KB | 89 | PASS | PASS | PASS | 1142 | 1567 | 1509 | 0.616 | 1.000 | 0.885 |
| 19 | NuGet | 16KB | 91 | PASS | PASS | PASS | 988 | 3072 | 2985 | 0.825 | 1.000 | 0.947 |
| 20 | Docker Hub | 388KB | 100 | PASS | PASS | PASS | 975 | 5406 | 5240 | 0.600 | 1.000 | 0.940 |
| 21 | Terraform | 120KB | 614 | PASS | PASS | PASS | 2598 | 6248 | 6379 | 0.960 | 1.000 | 0.988 |
| 22 | GitHub Explore | 393KB | 806 | PASS | PASS | PASS | 1610 | 5368 | 5378 | 0.887 | 1.000 | 0.877 |
| 23 | OpenStreetMap | 32KB | 122 | PASS | PASS | PASS | 1031 | 709 | 708 | 0.499 | 1.000 | 0.649 |
| 24 | httpbin HTML | 3KB | 3 | PASS | PASS | PASS | 149 | 385 | 390 | 0.570 | 1.000 | 0.699 |
| 25 | JSON Placeholder | 8KB | 91 | PASS | PASS | PASS | 1053 | 1038 | 1028 | 0.945 | 1.000 | 0.961 |
| 26 | Haskell.org | 63KB | 453 | PASS | PASS | PASS | 1255 | 6089 | 6159 | 0.945 | 1.000 | 0.924 |
| 27 | Elixir Lang | 26KB | 152 | PASS | PASS | PASS | 1710 | 6179 | 6136 | 0.872 | 1.000 | 0.880 |
| 28 | Zig Lang | 12KB | 118 | PASS | PASS | PASS | 1291 | 6009 | 6023 | 0.825 | 1.000 | 0.876 |
| 29 | Svelte | 87KB | 183 | PASS | PASS | PASS | 1015 | 4646 | 4711 | 0.825 | 1.000 | 0.951 |
| 30 | Tailwind CSS | 913KB | 9001 | PASS | PASS | PASS | 3814 | 11627 | 11329 | 0.960 | 1.000 | 0.972 |

## Top-3 Node Quality Analysis

Side-by-side comparison of what each reranker picks as top-3 nodes.

### Hacker News

**MiniLM top-3:**
1. `0.214` [generic] Hacker News new | past | comments | ask | show | jobs | submit login
2. `0.200` [generic] 27 points by jandrewrogers 3 hours ago | hide | 15 comments
3. `0.200` [generic] 13 points by ericlewis 1 hour ago | hide | 1 comment

**ColBERT top-3:**
1. `1.000` [generic] What major works of literature were written after age of 85? 75? 65? ( columbia.edu )
2. `0.885` [generic] 1632 points by treexs 10 hours ago | hide | 813 comments
3. `0.856` [generic] Hacker News new | past | comments | ask | show | jobs | submit login

---

### HN Newest

**MiniLM top-3:**
1. `0.456` [text] Hacker News new | past | comments | ask | show | jobs | submit
2. `0.375` [text] new
3. `0.070` [generic] New Links | Hacker News Hacker News new | past | comments | ask | show | jobs | submit login 1. NTSB

**ColBERT top-3:**
1. `1.000` [text] Hacker News new | past | comments | ask | show | jobs | submit
2. `0.422` [text] new
3. `0.422` [link] new

---

### Lobsters

**MiniLM top-3:**
1. `0.683` [heading] Your job isn't programming
2. `0.663` [link] Programming language theory, types, design
3. `0.639` [text] ask programming

**ColBERT top-3:**
1. `1.000` [link] Programming language theory, types, design
2. `0.614` [text] ask programming
3. `0.589` [link] C programming

---

### CNN Lite

**MiniLM top-3:**
1. `0.563` [link] Trump’s top litigator faces uphill battle with birthright citizenship
2. `0.556` [link] Behind the scenes and in front of cameras, Hegseth serving as top cheerleader for military power in 
3. `0.514` [text] Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery Company. All Rights

**ColBERT top-3:**
1. `1.000` [generic] Breaking News, Latest News and Videos | CNN CNN 3/31/2026 Latest Stories Judge rules that White Hous
2. `0.671` [text] Go to the full CNN experience ©2026 Cable News Network. A Warner Bros. Discovery Company. All Rights
3. `0.664` [generic] CNN 3/31/2026 Latest Stories Judge rules that White House ballroom construction ‘has to stop!’ Trump

---

### NPR Text

**MiniLM top-3:**
1. `0.794` [heading] NPR : National Public Radio
2. `0.595` [text] NPR : National Public Radio Tuesday, March 31, 2026 Federal judge finds Trump violated free speech b
3. `0.489` [link] News

**ColBERT top-3:**
1. `1.000` [link] News
2. `0.288` [generic] Text-Only Version Go To Full Site NPR : National Public Radio Tuesday, March 31, 2026 Federal judge 
3. `0.266` [generic] NPR : National Public Radio Text-Only Version Go To Full Site NPR : National Public Radio Tuesday, M

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
2. `0.870` [text] Build it in Rust In 2018, the Rust community decided to improve the programming experience
for a few
3. `0.839` [text] In 2018, the Rust community decided to improve the programming experience
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
3. `0.969` [generic] HTML elements Reference for all HTML elements . HTML attributes Reference for all HTML attributes. A

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
3. `0.639` [text] Build simple, secure, scalable systems with Go An open-source programming language supported by Goog

---

### TypeScript

**MiniLM top-3:**
1. `0.681` [heading] Using TypeScript
2. `0.673` [heading] What is TypeScript?
3. `0.655` [heading] TypeScript is JavaScript with syntax for types.

**ColBERT top-3:**
1. `1.000` [text] TypeScript file .
2. `0.932` [text] TypeScript
3. `0.858` [link] TypeScript Home Page

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
2. `0.945` [link] Node.js Github
3. `0.940` [text] Get Node.js® Get Node.js® Get security support for EOL Node.js versions Node.js is proudly supported

---

### Ruby Lang

**MiniLM top-3:**
1. `0.712` [text] “ Ruby is just the most beautiful programming language I have ever seen. And I pay a fair amount of 
2. `0.676` [text] Since 1995 Ruby Latest Version: 4.0.2 Download
3. `0.667` [text] A Programmer's Best Friend Since 1995 Ruby Latest Version: 4.0.2 Download

**ColBERT top-3:**
1. `1.000` [generic] Ruby Programming Language Ruby Install Docs Libraries Contribution Community News English ( en ) Бъл
2. `0.912` [text] “ Ruby is just the most beautiful programming language I have ever seen. And I pay a fair amount of 
3. `0.896` [text] “ Ruby is just the most beautiful programming language I have ever seen. And I pay a fair amount of 

---

### docs.rs

**MiniLM top-3:**
1. `0.758` [listitem] Rust Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Doc
2. `0.725` [form] Docs.rs docs.rs About docs.rs Badges Builds Metadata Shorthand URLs Download Rustdoc JSON Build queu
3. `0.718` [listitem] Rust website

**ColBERT top-3:**
1. `1.000` [list] Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Document
2. `0.997` [listitem] Rust Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Doc
3. `0.997` [list] Rust Rust website The Book Standard Library API Reference Rust by Example The Cargo Guide Clippy Doc

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
2. `0.845` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse
3. `0.845` [text] Find, install and publish Python packages with the Python Package Index Search PyPI Search Or browse

---

### pkg.go.dev

**MiniLM top-3:**
1. `0.704` [text] Packages Standard Library Sub-repositories About Go Packages
2. `0.577` [link] About Go Packages
3. `0.528` [text] Why Go Use Cases Case Studies Get Started Playground Tour Stack Overflow Help Packages Standard Libr

**ColBERT top-3:**
1. `1.000` [link] About Go Packages
2. `0.746` [text] Packages Standard Library Sub-repositories About Go Packages
3. `0.410` [generic] Go Packages - Go Packages Skip to Main Content Why Go Case Studies Common problems companies solve w

---

### RubyGems

**MiniLM top-3:**
1. `0.616` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
2. `0.613` [text] Ruby Central
3. `0.497` [text] Operated by Ruby Central Designed by DockYard Hosted by AWS Resolved with DNSimple Monitored by Data

**ColBERT top-3:**
1. `1.000` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
2. `0.736` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta
3. `0.736` [text] RubyGems.org is the Ruby community’s gem hosting service. Instantly publish your gems and then insta

---

### NuGet

**MiniLM top-3:**
1. `0.825` [text] NuGet is the package manager for .NET. The NuGet client tools provide the ability to produce and con
2. `0.825` [text] What is NuGet? NuGet is the package manager for .NET. The NuGet client tools provide the ability to 
3. `0.675` [text] Create .NET apps faster with NuGet 0 package downloads 0 package versions 0 unique packages

**ColBERT top-3:**
1. `1.000` [text] NuGet is the package manager for .NET. The NuGet client tools provide the ability to produce and con
2. `0.933` [text] What is NuGet? NuGet is the package manager for .NET. The NuGet client tools provide the ability to 
3. `0.856` [generic] Skip To Content Toggle navigation Packages Upload Statistics Documentation Downloads Blog Sign in Cr

---

### Docker Hub

**MiniLM top-3:**
1. `0.600` [generic] Docker Hub Container Image Library | App Containerization Search Docker Hub K Help Back Documentatio
2. `0.504` [heading] Docker Hardened Images - Now Free
3. `0.488` [text] Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System status ⁠ System theme 

**ColBERT top-3:**
1. `1.000` [generic] Docker Hub Container Image Library | App Containerization Search Docker Hub K Help Back Documentatio
2. `0.575` [heading] Docker Hardened Images - Now Free
3. `0.526` [generic] Search Docker Hub K Help Back Documentation ⁠ Forums ⁠ Contact support System status ⁠ System theme 

---

### Terraform

**MiniLM top-3:**
1. `0.960` [data] content.get_started.body: Follow a code-complete, hands-on tutorial to learn the Terraform basics wi
2. `0.960` [data] content.overview.body: Terraform is an infrastructure as code tool that lets you build, change, and 
3. `0.690` [data] product.rootDocsPaths[0].iconName: code

**ColBERT top-3:**
1. `1.000` [data] content.overview.body: Terraform is an infrastructure as code tool that lets you build, change, and 
2. `0.606` [data] content.blocks[5].cards[0].body: Describe infrastructure in Terraform configuration language.
3. `0.564` [data] product.rootDocsPaths[0].iconName: code

---

### GitHub Explore

**MiniLM top-3:**
1. `0.887` [heading] Trending repository
2. `0.825` [text] REPOSITORIES Topics Trending Collections
3. `0.825` [text] COMMUNITY GitHub Sponsors Fund open source developers PROGRAMS Security Lab Maintainer Community Acc

**ColBERT top-3:**
1. `1.000` [text] REPOSITORIES Topics Trending Collections
2. `0.413` [heading] Trending repository
3. `0.413` [heading] Trending repository

---

### OpenStreetMap

**MiniLM top-3:**
1. `0.499` [text] OpenStreetMap is a map of the world, created by people like you and free to use under an open licens
2. `0.415` [text] Welcome to OpenStreetMap! OpenStreetMap is a map of the world, created by people like you and free t
3. `0.398` [text] Where is this? GraphHopper OSRM Valhalla Loading... Welcome to OpenStreetMap! OpenStreetMap is a map

**ColBERT top-3:**
1. `1.000` [text] OpenStreetMap is a map of the world, created by people like you and free to use under an open licens
2. `0.700` [generic] OpenStreetMap OpenStreetMap Where is this? GraphHopper OSRM Valhalla Edit Edit with iD (in-browser e
3. `0.674` [generic] OpenStreetMap Where is this? GraphHopper OSRM Valhalla Edit Edit with iD (in-browser editor) Edit wi

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
2. `0.660` [text] {JSON} Placeholder Free fake and reliable API for testing and prototyping. Powered by JSON Server + 
3. `0.660` [generic] {JSON} Placeholder Free fake and reliable API for testing and prototyping. Powered by JSON Server + 

---

### Haskell.org

**MiniLM top-3:**
1. `0.945` [heading] Smart contract systems are largely about programming languages, 
and when it comes to programming la
2. `0.825` [text] IOHK Smart contract systems are largely about programming languages, 
and when it comes to programmi
3. `0.814` [heading] Functional Programming &Haskell, by Computerphile / John Hughes

**ColBERT top-3:**
1. `1.000` [text] A new paradigm Express your ideas clearly and learn a new way of thinking about programming. Based o
2. `0.981` [text] Why Haskell? A new paradigm Express your ideas clearly and learn a new way of thinking about program
3. `0.981` [text] Why Haskell? A new paradigm Express your ideas clearly and learn a new way of thinking about program

---

### Elixir Lang

**MiniLM top-3:**
1. `0.872` [heading] Elixir is a dynamic, functional language for building scalable and maintainable applications.
2. `0.678` [heading] Functional programming
3. `0.659` [text] Elixir has been designed to be extensible, allowing developers to naturally extend the language to p

**ColBERT top-3:**
1. `1.000` [generic] The Elixir programming language Home Install Learning Docs Guides Cases Blog Elixir is a dynamic, fu
2. `0.889` [heading] Elixir is a dynamic, functional language for building scalable and maintainable applications.
3. `0.841` [text] Elixir is a dynamic, functional language for building scalable and maintainable applications. Elixir

---

### Zig Lang

**MiniLM top-3:**
1. `0.825` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu
2. `0.675` [text] Focus on debugging your application rather than debugging your programming language knowledge.
3. `0.657` [text] ⚡ A Simple Language Focus on debugging your application rather than debugging your programming langu

**ColBERT top-3:**
1. `1.000` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu
2. `1.000` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu
3. `0.996` [text] Zig is a general-purpose programming language and toolchain for maintaining robust , optimal and reu

---

### Svelte

**MiniLM top-3:**
1. `0.825` [text] Svelte is a UI framework that uses a compiler to let you write breathtakingly concise
			components 
2. `0.744` [text] attractively thin, graceful and stylish Svelte is a UI framework that uses a compiler to let you wri
3. `0.737` [heading] Svelte

**ColBERT top-3:**
1. `1.000` [text] Svelte web development for the rest of us get started attractively thin, graceful and stylish Svelte
2. `1.000` [main] Svelte web development for the rest of us get started attractively thin, graceful and stylish Svelte
3. `0.965` [generic] Svelte • Web development for the rest of us Skip to main content Docs Svelte SvelteKit CLI AI Tutori

---

### Tailwind CSS

**MiniLM top-3:**
1. `0.960` [data] _rsc_331[3][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we
2. `0.960` [data] _rsc_331[1][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we
3. `0.960` [data] _rsc_331[10][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern w

**ColBERT top-3:**
1. `1.000` [data] _rsc_331[10][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern w
2. `0.991` [data] _rsc_331[3][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we
3. `0.991` [data] _rsc_331[1][3].content: Tailwind CSS is a utility-first CSS framework for rapidly building modern we

---

