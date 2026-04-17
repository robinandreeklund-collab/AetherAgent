# Slaash Dev Plan — From Alpha to Production

*2026-04-14*

## Where We Are

50-site test results: CRFR works on ~70% of the web. The remaining ~30% are JS-heavy SPAs that return empty results. Feedback learning works on 8/10 tested sites. Token reduction is real (99.9%). Cost argument holds ($4,000/day → $2/day).

The architecture is right. The gaps are in execution.

---

## Phase 1: Zero Empty Results (2 weeks)

**Goal: Every URL returns useful content. No exceptions.**

Today 16/50 sites returned empty or near-empty results. This is the #1 blocker for production use. An agent that gets nothing back 30% of the time is unreliable.

### 1.1 Smarter SPA Detection in Escalation

**File**: `src/escalation.rs` (lines 106-264, `select_tier()`)

Current `select_tier()` detects SPA markers (React, Vue, Angular, Next.js, Nuxt) but the decision logic is conservative — it often picks Tier 1 (static parse) when the page has *some* text content, even if that text is just boilerplate nav.

**Changes:**
- Add content quality check: not just "≥5 text elements" but "≥5 text elements with ≥30 chars that aren't navigation"
- If SPA markers detected AND content is thin, escalate to Tier 2 (QuickJS+ArenaDom) before Tier 4 (CDP)
- Add `Tier2WithHydration` path: try hydration extraction first (Tier 0), then QuickJS if that fails
- Track escalation success rates per-domain in CRFR field metadata

**New escalation flow:**
```
HTML in → Hydration check (Tier 0, <1ms)
  ├─ SSR data found? → extract, done
  └─ No SSR data → Content quality check
       ├─ Rich content (≥5 substantive paragraphs)? → Tier 1, done  
       └─ Thin/SPA shell? → QuickJS + ArenaDom (Tier 2, ~50ms)
            ├─ Content appeared? → done
            └─ Still empty? → QuickJS lifecycle (Tier 2.5, ~200ms)
                 ├─ Content appeared? → done
                 └─ Still empty? → mark as "requires CDP" for next visit
```

The key insight: **we have 200+ DOM APIs in ArenaDom and QuickJS can run framework code**. We should exhaust Tier 2/2.5 before declaring a page needs Chrome. Today we skip straight from Tier 1 to Tier 4 too often.

### 1.2 Framework-Specific QuickJS Bootstrapping

**Files**: `src/js_eval.rs`, `src/dom_bridge/mod.rs`, `src/hydration.rs`

Many SPAs that "need Chrome" actually just need their hydration code to run. React's `hydrateRoot()`, Vue's `createSSRApp()`, Svelte's `hydrate` — these execute against a DOM that already has HTML structure.

**Changes:**
- For detected Next.js pages: inject `__NEXT_DATA__` into QuickJS global, run hydration
- For detected Nuxt pages: inject `__NUXT__` / `__NUXT_DATA__` (devalue parser already exists in hydration.rs)
- For detected SvelteKit: inject `__SVELTEKIT_DATA__` 
- For React/Vue generic: try running `<script>` tags that reference `getElementById`/`querySelector` patterns (the `eval_js_batch` allowlist already handles these)

This is NOT about running the full React runtime. It's about running the specific hydration scripts that transform `<div id="root">` into actual content.

### 1.3 Automatic CDP Fallback with Caching

**Files**: `src/vision_backend.rs`, `src/escalation.rs`, `src/resonance.rs`

When Tier 0-2.5 all fail, we need CDP. But we should:

- **Cache the result**: Store the CDP-rendered HTML in the CRFR field. Next time this URL is visited, skip CDP and use cached HTML
- **Learn per-domain**: If 3+ pages on a domain need CDP, mark the domain as "CDP-required" in domain registry. Skip Tier 1-2 on future visits
- **Don't block on CDP**: Return what we have from Tier 1 immediately, fire CDP in background, merge results when ready

**New field in ResonanceField:**
```rust
cdp_rendered_html: Option<String>,  // Cached CDP output
domain_requires_cdp: bool,          // Learned from repeated failures
```

### 1.4 Empty Result Safety Net

**File**: `src/lib.rs` (parse_crfr_from_tree_js)

When CRFR returns 0 relevant nodes, instead of returning empty:

- Return the page title + meta description as fallback nodes
- Add `"suggested_action": "retry_with_js"` or `"retry_with_cdp"` hint
- Include `total_nodes` and `spa_detected` flags so the agent knows WHY it's empty

---

## Phase 2: Result Quality (2 weeks)

**Goal: Every result is clean, relevant, and actionable.**

### 2.1 Blob Text Splitting

**Problem found in 50-site test**: NYT, Al Jazeera, W3Schools return single nodes with 500+ character labels containing entire page sections as one blob.

**File**: `src/semantic.rs` (process_arena_element / process_element)

**Changes:**
- When a node's inner text exceeds 300 chars and contains multiple sentences, split into child nodes at paragraph/sentence boundaries
- Preserve parent relationship so CRFR can still propagate relevance
- Each sub-node gets its own relevance score

### 2.2 Navigation vs Content Discrimination

**Problem found**: Guardian returned "US news" 5x, DN.se returned "Nyheter" links, HN returned "hide" buttons 7x.

**Files**: `src/resonance.rs` (propagate_inner), `src/lib.rs` (crfr_post_filter)

**Changes in resonance.rs:**
- Add structural role detection: nodes in `<nav>`, `<header>`, `<footer>` get a 0.5x penalty
- Repeated action links (same label + same action pattern) get progressive penalty: 1st=1.0, 2nd=0.5, 3rd+=0.0
- Links with generic labels ("click here", "read more", "more", "hide", "show") get 0.3x penalty unless they match the goal

**Changes in crfr_post_filter:**
- Filter nodes where `role == "link"` and label length < 15 and label is a common UI verb (hide, show, close, dismiss, skip, more, less)
- Filter nodes that appear >2x with identical label text (already done in BUG-3 fix, verify it works)

### 2.3 BBC/SSR JSON Opaque URL Problem

**Problem found**: BBC returns opaque article URLs like `/news/articles/ckgw8w7mzxgo` without headline text because the page is SSR JSON.

**File**: `src/hydration.rs`, `src/lib.rs`

**Changes:**
- When SSR JSON is detected (e.g., BBC's `page.@"home"` format), extract headline text from JSON values alongside URLs
- Map `relatedUrls[].url` → create link node with label from parent `title` field
- Add BBC/Guardian-specific JSON path patterns to hydration extractor

### 2.4 Label Cleaning Improvements

Already fixed BUG-6 (HTML entities, tag stripping). Additional improvements:

- Collapse `\n` sequences to single space
- Strip common prefixes: "Subscribers only", "ANZEIGE", "Advertisement", "Sponsored"
- Truncate at first occurrence of common boilerplate markers: "Cookie", "Sign up for newsletter"

---

## Phase 3: CRFR Link-Following Intelligence (1 week)

**Goal: When the answer isn't on the current page, automatically follow the right links.**

### 3.1 Auto-Feedback in Link Following

**File**: `src/bin/mcp_server.rs` (follow_relevant_links, lines 1641-1833)

Current `follow_relevant_links()` fetches linked pages and replaces link nodes with content nodes. But it doesn't feed back to CRFR.

**Changes:**
- After following a link and finding relevant content, automatically call `field.feedback(goal, &successful_ids)` on the TARGET page's field
- This means: if an agent clicks a CNN article link and gets good content, the CNN article's CRFR field remembers "these nodes were useful for news queries"
- Add `auto_feedback: bool` parameter to adaptive_crawl config (default: true)

### 3.2 Cross-Page Relevance Merging

When link-following returns nodes from multiple pages, the relevance scores aren't comparable (different fields, different scales).

**Changes:**
- Normalize relevance scores across pages: divide by max amplitude per-page
- Re-rank merged results by normalized relevance
- Mark nodes with `source_url` so the agent knows provenance

### 3.3 XHR → CRFR Pipeline

**Files**: `src/intercept.rs`, `src/lib.rs`

XHR interception already detects fetch()/XHR URLs in page JavaScript. But it doesn't auto-fetch them.

**Changes:**
- When CRFR results are thin (< 5 relevant nodes) and XHR URLs were detected:
  1. Fetch top 3 XHR URLs (filtered through semantic firewall)
  2. Parse responses as JSON/HTML
  3. Convert to semantic nodes
  4. Merge into CRFR results with `source: "xhr"` metadata
- This handles SPAs that load content via AJAX after initial render

---

## Phase 4: Anti-Bot Hardening (1 week)

**Goal: Fetch pages that currently block us.**

5/50 sites returned "Access Denied" or fetch errors.

### 4.1 TLS Fingerprint Impersonation

**File**: `src/fetch.rs`

**Changes:**
- Replace default `reqwest` TLS config with browser-mimicking profile
- Use `rustls` with Chrome-like cipher suite ordering
- Set `User-Agent`, `Accept`, `Accept-Language`, `Accept-Encoding` headers to match real Chrome
- Add `Sec-CH-UA`, `Sec-CH-UA-Platform`, `Sec-Fetch-Site` headers

### 4.2 Blocked Response Detection + Auto-Escalation

**File**: `src/fetch.rs`, `src/escalation.rs`

**Changes:**
- Detect blocked responses: 403, 429, empty body with `<noscript>` tags, Cloudflare challenge pages
- On first block: retry with stealth headers
- On second block: retry with CDP (Chrome has real TLS fingerprint)
- Cache the result: "this domain blocks non-browser requests"
- Log blocked domains for analysis

### 4.3 Cookie Consent Auto-Handling

Many sites show cookie consent banners that obscure content.

**Changes in crfr_post_filter:**
- Already filtering cookie consent text (done in existing code)
- Add: detect common consent banner patterns and skip them during tree building
- Add: if page only contains consent banner (< 5 content nodes after filtering), retry with `Cookie: consent=accepted` header

---

## Phase 5: Implicit Learning at Scale (2 weeks)

**Goal: Slaash gets smarter with every interaction, without explicit feedback.**

### 5.1 Automatic Implicit Feedback

**File**: `src/lib.rs` (crfr_implicit_feedback, already exists)

The function exists but isn't wired into the main pipeline.

**Changes:**
- When an MCP agent calls `parse_crfr` and then calls another tool (like `extract_data` or `find_and_click`) using the same URL, infer that the top-ranked nodes were useful
- Auto-trigger `implicit_feedback` with the nodes that were referenced
- Track success rate per-goal-category to weight future boosts

### 5.2 Domain-Level Learning Transfer

**File**: `src/resonance.rs` (transfer_from, already exists)

Current `transfer_from` matches by role + text HV similarity. But it only transfers when explicitly called.

**Changes:**
- When a new page on a known domain is parsed, automatically check if sibling pages (same domain) have CRFR fields with learning
- Transfer structural patterns: "on this domain, price nodes are usually in `<span class="price">`, navigation is in `<nav>`"
- Weight by domain similarity (same TLD = high, same registerable domain = medium)

### 5.3 Cross-Session Memory Persistence

**File**: `src/resonance.rs` (FIELD_CACHE), needs `persist` feature

The in-memory cache evicts fields when full. Learning is lost between server restarts.

**Changes:**
- Enable SQLite persistence by default in MCP server (already implemented behind `persist` feature flag)
- Save CRFR fields with `hit_count > 0` to disk on every feedback call
- Load from disk on cache miss
- Add periodic compaction: merge old fields, prune unused nodes

---

## Phase 6: Production Reliability (ongoing)

### 6.1 Monitoring & Metrics

- Track per-URL: parse time, node count, relevance distribution, cache hit rate
- Track per-domain: success rate, escalation frequency, blocked rate
- Expose via MCP tool: `diagnostics` returns health summary
- Alert on: >10% empty results, >5% fetch failures, average parse time > 500ms

### 6.2 Graceful Degradation

When things go wrong, return SOMETHING useful:

- Timeout → return cached result if available, else title + meta description
- Parse error → return raw text extraction (strip tags, first 1000 chars)
- Fetch blocked → return `suggested_action: "use_proxy"` with domain blocklist status

### 6.3 Test Coverage

- Add 50-site regression test to CI (using cached HTML fixtures)
- Add feedback learning regression test (parse → feedback → re-parse → verify boost)
- Add SPA escalation test suite: React, Vue, Angular, Next.js, Nuxt, SvelteKit fixtures
- WPT score gate: never regress

---

## Priority Order

| Phase | Impact | Effort | Priority |
|-------|--------|--------|----------|
| **Phase 1: Zero Empty Results** | Critical — 30% of web fails today | 2 weeks | **P0** |
| **Phase 2: Result Quality** | High — noisy results erode trust | 2 weeks | **P0** |
| **Phase 3: Link-Following** | High — answers often on linked pages | 1 week | **P1** |
| **Phase 4: Anti-Bot** | Medium — 10% of sites blocked | 1 week | **P1** |
| **Phase 5: Implicit Learning** | High — compounds over time | 2 weeks | **P2** |
| **Phase 6: Production** | Essential — can't ship without it | Ongoing | **P2** |

**Total estimated timeline: 8-10 weeks to production-ready.**

---

## Success Metrics

| Metric | Today | Target |
|--------|-------|--------|
| Sites with useful results | 34/50 (68%) | 48/50 (96%) |
| Empty result rate | 32% | < 2% |
| Feedback learning success | 8/10 (80%) | 10/10 (100%) |
| Average token reduction | 99.9% | 99.9% (maintain) |
| Median parse time (cold) | 84ms | < 100ms |
| Median parse time (cached) | < 1ms | < 1ms (maintain) |
| Duplicate nodes in results | ~30% of slots wasted | < 5% |
| False injection warnings | 13 on Walmart alone | 0 |
| Blocked fetch rate | 10% | < 2% |
