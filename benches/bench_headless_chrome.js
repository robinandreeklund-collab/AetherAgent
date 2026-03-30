#!/usr/bin/env node
/**
 * Headless Chrome (Playwright) Benchmark
 * =======================================
 * Same tests as AetherAgent + LightPanda benchmarks.
 * Sequential execution, no resource contention.
 *
 * Run: node benches/bench_headless_chrome.js
 */

const { chromium } = require('playwright');
const fs = require('fs');
const path = require('path');
const http = require('http');

const FIXTURE_DIR = path.join(__dirname, '..', 'tests', 'fixtures');
const CAMPFIRE_PATH = path.join(__dirname, 'campfire_fixture.html');
const FIXTURE_PORT = 18902;
const RUNS_CAMPFIRE = 100;

// ─── Local HTTP server ──────────────────────────────────────────────────────

function startServer(dir, port) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const filePath = path.join(dir, decodeURIComponent(req.url).slice(1));
      try {
        const content = fs.readFileSync(filePath, 'utf-8');
        res.writeHead(200, { 'Content-Type': 'text/html' });
        res.end(content);
      } catch {
        res.writeHead(404);
        res.end('Not found');
      }
    });
    server.listen(port, '127.0.0.1', () => resolve(server));
  });
}

// ─── Helpers ────────────────────────────────────────────────────────────────

function fmt(ms) {
  return ms >= 1000 ? `${(ms / 1000).toFixed(2)}s` : `${ms.toFixed(1)}ms`;
}

function countNodes(handle) {
  // Count all DOM elements
  return handle.evaluate(() => document.querySelectorAll('*').length);
}

// ─── Main ───────────────────────────────────────────────────────────────────

async function main() {
  console.log('='.repeat(80));
  console.log('  Headless Chrome (Playwright Chromium) Benchmark');
  console.log('  Sequential execution · Same tests as AetherAgent/LightPanda');
  console.log('='.repeat(80));

  // Prepare serve directory
  const serveDir = '/tmp/chrome_bench_fixtures';
  fs.mkdirSync(serveDir, { recursive: true });
  for (const f of fs.readdirSync(FIXTURE_DIR)) {
    if (f.endsWith('.html')) {
      fs.copyFileSync(path.join(FIXTURE_DIR, f), path.join(serveDir, f));
    }
  }
  if (fs.existsSync(CAMPFIRE_PATH)) {
    fs.copyFileSync(CAMPFIRE_PATH, path.join(serveDir, 'campfire.html'));
  }

  const server = await startServer(serveDir, FIXTURE_PORT);
  console.log(`\n  Fixture server: http://127.0.0.1:${FIXTURE_PORT}`);

  const browser = await chromium.launch({ headless: true });
  console.log(`  Chromium: ${browser.version()}`);

  const results = { campfire: {}, fixtures: [], live: [] };

  // ═══════════════════════════════════════════════════════════════════════
  // 1. RAW PERFORMANCE: 100 Sequential Campfire Parses
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  1. RAW PERFORMANCE — 100 Sequential Campfire Commerce Parses');
  console.log('='.repeat(80));

  const campfireUrl = `http://127.0.0.1:${FIXTURE_PORT}/campfire.html`;

  // Warmup
  for (let i = 0; i < 3; i++) {
    const page = await browser.newPage();
    await page.goto(campfireUrl, { waitUntil: 'domcontentloaded' });
    await page.close();
  }

  const chromeTimes = [];
  const chromeTokens = [];

  for (let i = 0; i < RUNS_CAMPFIRE; i++) {
    const page = await browser.newPage();
    const start = performance.now();
    await page.goto(campfireUrl, { waitUntil: 'domcontentloaded' });
    const content = await page.content();
    const elapsed = performance.now() - start;
    chromeTimes.push(elapsed);
    chromeTokens.push(Math.floor(content.length / 4));
    await page.close();

    if (i % 25 === 0) {
      const nodes = await (async () => {
        const p = await browser.newPage();
        await p.goto(campfireUrl, { waitUntil: 'domcontentloaded' });
        const n = await p.evaluate(() => document.querySelectorAll('*').length);
        await p.close();
        return n;
      })();
      console.log(`    [${String(i + 1).padStart(3)}/100] ${fmt(elapsed).padStart(8)}  nodes=${nodes}  tokens=${Math.floor(content.length / 4)}`);
    }
  }

  chromeTimes.sort((a, b) => a - b);
  const chromeTotal = chromeTimes.reduce((a, b) => a + b, 0);
  const chromeAvg = chromeTotal / RUNS_CAMPFIRE;
  const chromeMed = chromeTimes[49];
  const chromeP99 = chromeTimes[98];
  const chromeTokAvg = Math.floor(chromeTokens.reduce((a, b) => a + b, 0) / chromeTokens.length);

  console.log(`\n  Headless Chrome (Campfire 100x):`);
  console.log(`    Total:   ${fmt(chromeTotal)}`);
  console.log(`    Avg:     ${fmt(chromeAvg)}`);
  console.log(`    Median:  ${fmt(chromeMed)}`);
  console.log(`    P99:     ${fmt(chromeP99)}`);
  console.log(`    Tokens:  ~${chromeTokAvg}/parse`);

  results.campfire = {
    total: chromeTotal, avg: chromeAvg, median: chromeMed,
    p99: chromeP99, tokens: chromeTokAvg,
  };

  // ═══════════════════════════════════════════════════════════════════════
  // 2. LOCAL FIXTURES: 50 files
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  2. LOCAL FIXTURES — 50 Files (Headless Chrome)');
  console.log('='.repeat(80));

  const fixtures = fs.readdirSync(serveDir)
    .filter(f => f.match(/^\d{2}_/) && f.endsWith('.html'))
    .sort();

  console.log(`\n  ${'Fixture'.padEnd(38)} ${'Time'.padStart(8)} ${'Nodes'.padStart(6)} ${'Tokens'.padStart(7)}`);
  console.log('  ' + '-'.repeat(64));

  for (const f of fixtures) {
    const url = `http://127.0.0.1:${FIXTURE_PORT}/${f}`;
    const times = [];
    let lastNodes = 0, lastTokens = 0;

    for (let r = 0; r < 3; r++) {
      const page = await browser.newPage();
      const start = performance.now();
      await page.goto(url, { waitUntil: 'domcontentloaded' });
      const content = await page.content();
      const nodes = await page.evaluate(() => document.querySelectorAll('*').length);
      const elapsed = performance.now() - start;
      times.push(elapsed);
      lastNodes = nodes;
      lastTokens = Math.floor(content.length / 4);
      await page.close();
    }

    times.sort((a, b) => a - b);
    const med = times[1]; // median of 3

    results.fixtures.push({ fixture: f, time_ms: med, nodes: lastNodes, tokens: lastTokens });
    console.log(`  ${f.padEnd(38)} ${fmt(med).padStart(8)} ${String(lastNodes).padStart(6)} ${String(lastTokens).padStart(7)}`);
  }

  const fixAvg = results.fixtures.reduce((a, r) => a + r.time_ms, 0) / results.fixtures.length;
  const fixTotal = results.fixtures.reduce((a, r) => a + r.time_ms, 0);
  const fixAvgNodes = Math.floor(results.fixtures.reduce((a, r) => a + r.nodes, 0) / results.fixtures.length);
  const fixAvgTokens = Math.floor(results.fixtures.reduce((a, r) => a + r.tokens, 0) / results.fixtures.length);

  console.log(`\n  Chrome Local Summary:`);
  console.log(`    Avg time:    ${fmt(fixAvg)}`);
  console.log(`    Total time:  ${fmt(fixTotal)}`);
  console.log(`    Avg nodes:   ${fixAvgNodes}`);
  console.log(`    Avg tokens:  ${fixAvgTokens}`);

  // ═══════════════════════════════════════════════════════════════════════
  // 3. LIVE SITES: 20 URLs
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  3. LIVE SITES — 20 URLs (Headless Chrome)');
  console.log('='.repeat(80));

  const liveSites = [
    'https://books.toscrape.com',
    'https://news.ycombinator.com',
    'https://example.com',
    'https://httpbin.org',
    'https://en.wikipedia.org/wiki/Rust_(programming_language)',
    'https://github.com/nickel-org/rust-mustache',
    'https://jsonplaceholder.typicode.com',
    'https://quotes.toscrape.com',
    'https://www.scrapethissite.com/pages/simple/',
    'https://www.scrapethissite.com/pages/forms/',
    'https://en.wikipedia.org/wiki/WebAssembly',
    'https://en.wikipedia.org/wiki/Artificial_intelligence',
    'https://developer.mozilla.org/en-US/docs/Web/HTML',
    'https://www.rust-lang.org',
    'https://crates.io',
    'https://docs.rs',
    'https://play.rust-lang.org',
    'https://en.wikipedia.org/wiki/Linux',
    'https://en.wikipedia.org/wiki/World_Wide_Web',
    'https://lobste.rs',
  ];

  console.log(`\n  ${'URL'.padEnd(48)} ${'Time'.padStart(8)} ${'Nodes'.padStart(6)} ${'Tokens'.padStart(7)} ${'OK'.padStart(4)}`);
  console.log('  ' + '-'.repeat(78));

  let liveOk = 0;

  for (const url of liveSites) {
    const page = await browser.newPage();
    let elapsed = 0, nodes = 0, tokens = 0, status = 'FAIL';

    try {
      const start = performance.now();
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
      const content = await page.content();
      elapsed = performance.now() - start;
      nodes = await page.evaluate(() => document.querySelectorAll('*').length);
      tokens = Math.floor(content.length / 4);
      status = nodes > 2 ? 'OK' : 'FAIL';
      if (status === 'OK') liveOk++;
    } catch (e) {
      elapsed = 30000;
      status = 'FAIL';
    }

    await page.close();

    results.live.push({ url, time_ms: elapsed, nodes, tokens, ok: status === 'OK' });
    const short = url.length > 46 ? url.slice(0, 46) + '…' : url;
    console.log(`  ${short.padEnd(48)} ${fmt(elapsed).padStart(8)} ${String(nodes).padStart(6)} ${String(tokens).padStart(7)} ${status.padStart(4)}`);
  }

  const liveAvg = results.live.reduce((a, r) => a + r.time_ms, 0) / results.live.length;
  console.log(`\n  Chrome Live Summary:`);
  console.log(`    OK:          ${liveOk}/20`);
  console.log(`    Avg time:    ${fmt(liveAvg)}`);

  // ═══════════════════════════════════════════════════════════════════════
  // FINAL TABLE (JSON output for merging with other benchmarks)
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  HEADLESS CHROME SUMMARY');
  console.log('='.repeat(80));
  console.log(`
  Campfire 100x:     Total=${fmt(chromeTotal)}  Avg=${fmt(chromeAvg)}  Median=${fmt(chromeMed)}  P99=${fmt(chromeP99)}
  Local fixtures:    Avg=${fmt(fixAvg)}  Nodes=${fixAvgNodes}  Tokens=${fixAvgTokens}
  Live sites:        OK=${liveOk}/20  Avg=${fmt(liveAvg)}
  `);

  // Save results
  const outPath = path.join(__dirname, 'headless_chrome_results.json');
  fs.writeFileSync(outPath, JSON.stringify(results, null, 2));
  console.log(`  Results saved to: ${outPath}`);

  await browser.close();
  server.close();
}

main().catch(console.error);
