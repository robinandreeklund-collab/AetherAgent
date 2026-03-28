#!/usr/bin/env node
/**
 * Quality Benchmark — 5 Real Sites, Real Questions
 * =================================================
 *
 * Fair methodology with honest limitations:
 *
 * AetherAgent: Reads pre-fetched HTML from disk (Rust binary).
 *   Measures: parse + embedding + semantic analysis. No network.
 *
 * Chrome: page.setContent(html) on pre-fetched HTML.
 *   Measures: DOM parse + render. No network.
 *
 * LightPanda: fetch from live URL (LP requires HTTP, can't read local files).
 *   Measures: network fetch + parse + render. INCLUDES network latency.
 *   This is documented honestly — LP's architecture requires fetching.
 *
 * All pre-fetched HTML is identical (curl'd once, saved to disk).
 */
const { chromium } = require('playwright');
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');

const LP_BIN = process.env.LIGHTPANDA_BIN ||
  path.join(require('os').homedir(), '.config', 'lightpanda-gomcp', 'lightpanda');

const SITES = [
  { name: 'apple.com',      file: 'apple.html',     goal: 'find iPhone price',           liveUrl: 'https://www.apple.com' },
  { name: 'Hacker News',    file: 'hackernews.html', goal: 'find latest news articles',   liveUrl: 'https://news.ycombinator.com' },
  { name: 'books.toscrape', file: 'books.html',      goal: 'find book titles and prices', liveUrl: 'https://books.toscrape.com' },
  { name: 'lobste.rs',      file: 'lobsters.html',   goal: 'find technology articles',    liveUrl: 'https://lobste.rs' },
  { name: 'rust-lang.org',  file: 'rustlang.html',   goal: 'download and install Rust',   liveUrl: 'https://www.rust-lang.org' },
];

function fmt(ms) { return ms >= 1000 ? `${(ms/1000).toFixed(2)}s` : `${ms.toFixed(1)}ms`; }

function lpFetch(url, dump = 'html') {
  const start = performance.now();
  try {
    const out = execSync(
      `${LP_BIN} fetch --dump ${dump} --log-level fatal "${url}"`,
      { timeout: 30000, encoding: 'utf-8', stdio: ['pipe','pipe','pipe'] }
    );
    return { elapsed: performance.now() - start, output: out, ok: out.length > 100 };
  } catch {
    return { elapsed: performance.now() - start, output: '', ok: false };
  }
}

function countNodes(jsonStr) {
  try {
    const d = JSON.parse(jsonStr);
    let c = 0;
    const stk = [d];
    while (stk.length) { const n = stk.pop(); c++; (n.children||[]).forEach(ch => stk.push(ch)); }
    return c;
  } catch { return 0; }
}

async function main() {
  console.log('='.repeat(80));
  console.log('  Quality Benchmark — 5 Real Sites, Real Questions');
  console.log('  AetherAgent (disk) · Chrome (setContent) · LightPanda (live fetch)');
  console.log('='.repeat(80));

  const browser = await chromium.launch({ headless: true });
  console.log(`\n  Chrome: ${browser.version()}`);
  console.log(`  LP: ${LP_BIN}`);
  console.log();

  const results = [];

  for (const site of SITES) {
    const htmlPath = path.join('/tmp/live_bench', site.file);
    const html = fs.readFileSync(htmlPath, 'utf-8');
    const htmlTokens = Math.floor(html.length / 4);

    console.log(`━━ ${site.name} ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━`);
    console.log(`  Goal: "${site.goal}"`);
    console.log(`  HTML: ${htmlTokens} tokens (${html.length} bytes)`);

    // ── Chrome: setContent (no network) ──
    const page = await browser.newPage();
    const cStart = performance.now();
    await page.setContent(html, { waitUntil: 'domcontentloaded' });
    const cContent = await page.content();
    const cNodes = await page.evaluate(() => document.querySelectorAll('*').length);
    const cElapsed = performance.now() - cStart;
    await page.close();
    const cTokens = Math.floor(cContent.length / 4);
    console.log(`  Chrome:      ${fmt(cElapsed).padStart(8)}  nodes=${cNodes}  out_tokens=${cTokens}`);

    // ── LightPanda: live fetch (includes network) ──
    const lpHtml = lpFetch(site.liveUrl, 'html');
    const lpSt = lpFetch(site.liveUrl, 'semantic_tree');
    const lpHtmlTokens = Math.floor(lpHtml.output.length / 4);
    const lpStNodes = countNodes(lpSt.output);
    const lpStTokens = Math.floor(lpSt.output.length / 4);
    const lpOk = lpHtml.ok && lpHtmlTokens > 50;
    console.log(`  LP:          ${fmt(lpHtml.elapsed).padStart(8)}  nodes=${lpStNodes}  html_tok=${lpHtmlTokens}  st_tok=${lpStTokens}  ${lpOk ? '✓' : '✗ (error page)'}`);
    if (!lpOk) {
      console.log(`    (LP returned error/minimal page — site may require JS or blocked LP)`);
    }

    // ── AetherAgent: values from Rust benchmark ──
    // (We can't call Rust from JS, so we reference the output)
    console.log(`  AetherAgent: (see Rust quality benchmark output)\n`);

    results.push({
      site: site.name, goal: site.goal, html_tokens: htmlTokens,
      chrome: { ms: cElapsed, nodes: cNodes, tokens: cTokens },
      lp: { ms: lpHtml.elapsed, nodes: lpStNodes, html_tokens: lpHtmlTokens, st_tokens: lpStTokens, ok: lpOk },
    });
  }

  // ═══════════════════════════════════════════════════════════════════════
  // Summary
  // ═══════════════════════════════════════════════════════════════════════
  console.log('='.repeat(80));
  console.log('  RESULTS TABLE — Chrome & LightPanda');
  console.log('='.repeat(80));
  console.log(`\n  ${'Site'.padEnd(16)} ${'HTML tok'.padStart(8)} │ ${'Chrome'.padStart(8)} ${'C nodes'.padStart(7)} ${'C tok'.padStart(6)} │ ${'LP'.padStart(8)} ${'LP nodes'.padStart(8)} ${'LP tok'.padStart(7)} ${'LP OK'.padStart(5)}`);
  console.log('  ' + '-'.repeat(82));
  for (const r of results) {
    console.log(`  ${r.site.padEnd(16)} ${String(r.html_tokens).padStart(8)} │ ${fmt(r.chrome.ms).padStart(8)} ${String(r.chrome.nodes).padStart(7)} ${String(r.chrome.tokens).padStart(6)} │ ${fmt(r.lp.ms).padStart(8)} ${String(r.lp.nodes).padStart(8)} ${String(r.lp.html_tokens).padStart(7)} ${(r.lp.ok ? 'YES' : 'NO').padStart(5)}`);
  }

  const cAvg = results.reduce((a,r) => a + r.chrome.ms, 0) / results.length;
  const lpOkCount = results.filter(r => r.lp.ok).length;
  console.log(`\n  Chrome avg: ${fmt(cAvg)}   LP OK: ${lpOkCount}/5`);

  const outPath = path.join(__dirname, 'quality_all_results.json');
  fs.writeFileSync(outPath, JSON.stringify(results, null, 2));
  console.log(`  Results: ${outPath}`);

  await browser.close();
}

main().catch(console.error);
