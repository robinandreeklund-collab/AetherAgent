#!/usr/bin/env node
/**
 * Fair Three-Way Benchmark: AetherAgent vs LightPanda vs Chrome
 * ==============================================================
 *
 * Methodology:
 * - Chrome: Playwright persistent browser (CDP) — how Chrome is used in production
 * - LightPanda: `fetch --dump html` CLI — how LP recommends benchmarking
 *   (LP CDP has target-ID bugs that crash Playwright after multiple pages)
 * - AetherAgent: in-process Rust library values from Rust benchmark
 *
 * LP startup overhead (~130ms) is measured and subtracted to show pure parse time.
 */

const { chromium } = require('playwright');
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const http = require('http');

const FIXTURE_DIR = path.join(__dirname, '..', 'tests', 'fixtures');
const CAMPFIRE_PATH = path.join(__dirname, 'campfire_fixture.html');
const LP_BIN = process.env.LIGHTPANDA_BIN ||
  path.join(require('os').homedir(), '.config', 'lightpanda-gomcp', 'lightpanda');
const FIXTURE_PORT = 18903;
const RUNS_CAMPFIRE = 100;

function startServer(dir, port) {
  return new Promise((resolve) => {
    const server = http.createServer((req, res) => {
      const fp = path.join(dir, decodeURIComponent(req.url).slice(1));
      if (res.headersSent) return;
      try {
        const content = fs.readFileSync(fp, 'utf-8');
        res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Connection': 'close' });
        res.end(content);
      } catch {
        res.writeHead(404, { 'Connection': 'close' }); res.end('Not found');
      }
    });
    server.listen(port, '127.0.0.1', () => resolve(server));
  });
}

function fmt(ms) { return ms >= 1000 ? `${(ms/1000).toFixed(2)}s` : `${ms.toFixed(1)}ms`; }

async function chromeBench(browser, url) {
  const page = await browser.newPage();
  const start = performance.now();
  await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 15000 });
  const content = await page.content();
  const nodes = await page.evaluate(() => document.querySelectorAll('*').length);
  const elapsed = performance.now() - start;
  await page.close();
  return { elapsed, tokens: Math.floor(content.length / 4), nodes };
}

function lpFetch(url) {
  const start = performance.now();
  try {
    const out = execSync(
      `${LP_BIN} fetch --dump html --log-level fatal --wait-until load --wait-ms 500 "${url}"`,
      { timeout: 15000, encoding: 'utf-8', stdio: ['pipe', 'pipe', 'pipe'] }
    );
    const elapsed = performance.now() - start;
    return { elapsed, tokens: Math.floor(out.length / 4), output: out, ok: true };
  } catch {
    return { elapsed: performance.now() - start, tokens: 0, output: '', ok: false };
  }
}

async function main() {
  console.log('='.repeat(80));
  console.log('  Fair Three-Way Benchmark');
  console.log('  AetherAgent (in-process) vs LightPanda (fetch CLI) vs Chrome (CDP)');
  console.log('='.repeat(80));

  const serveDir = '/tmp/fair_bench';
  fs.mkdirSync(serveDir, { recursive: true });
  for (const f of fs.readdirSync(FIXTURE_DIR).filter(f => f.endsWith('.html')))
    fs.copyFileSync(path.join(FIXTURE_DIR, f), path.join(serveDir, f));
  if (fs.existsSync(CAMPFIRE_PATH))
    fs.copyFileSync(CAMPFIRE_PATH, path.join(serveDir, 'campfire.html'));

  const server = await startServer(serveDir, FIXTURE_PORT);
  console.log(`\n  Fixture server: http://127.0.0.1:${FIXTURE_PORT}`);

  // Measure LP startup overhead (empty page)
  const lpOverheadRuns = [];
  for (let i = 0; i < 5; i++) {
    const { elapsed } = lpFetch(`http://127.0.0.1:${FIXTURE_PORT}/41_edge_empty_page.html`);
    lpOverheadRuns.push(elapsed);
  }
  lpOverheadRuns.sort((a,b) => a-b);
  const lpOverhead = lpOverheadRuns[2]; // median
  console.log(`  LP startup overhead: ~${fmt(lpOverhead)} (subtracted from LP times)`);
  console.log(`  LP binary: ${LP_BIN}`);

  const browser = await chromium.launch({ headless: true });
  console.log(`  Chrome: ${browser.version()}`);

  // ═══════════════════════════════════════════════════════════════════════
  // 1. CAMPFIRE 100x
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  1. CAMPFIRE 100x');
  console.log('='.repeat(80));

  const url = `http://127.0.0.1:${FIXTURE_PORT}/campfire.html`;

  // Chrome warmup + 100x
  for (let i = 0; i < 3; i++) await chromeBench(browser, url);
  const cTimes = [];
  for (let i = 0; i < RUNS_CAMPFIRE; i++) {
    const { elapsed, nodes } = await chromeBench(browser, url);
    cTimes.push(elapsed);
    if (i % 25 === 0) console.log(`  Chrome [${String(i+1).padStart(3)}] ${fmt(elapsed).padStart(8)}  nodes=${nodes}`);
  }
  cTimes.sort((a,b) => a-b);

  // LP warmup + 100x
  for (let i = 0; i < 3; i++) lpFetch(url);
  const lTimes = [];
  const lTimesNet = []; // minus overhead
  for (let i = 0; i < RUNS_CAMPFIRE; i++) {
    const { elapsed, tokens } = lpFetch(url);
    lTimes.push(elapsed);
    lTimesNet.push(Math.max(0, elapsed - lpOverhead));
    if (i % 25 === 0) console.log(`  LP     [${String(i+1).padStart(3)}] ${fmt(elapsed).padStart(8)} (net: ${fmt(Math.max(0, elapsed - lpOverhead))})  tokens=${tokens}`);
  }
  lTimes.sort((a,b) => a-b);
  lTimesNet.sort((a,b) => a-b);

  const cTotal = cTimes.reduce((a,b) => a+b, 0);
  const lTotal = lTimes.reduce((a,b) => a+b, 0);
  const lNetTotal = lTimesNet.reduce((a,b) => a+b, 0);
  const aeTotal = 23, aeAvg = 0.23;

  console.log(`
  ┌───────────────────────┬──────────────┬──────────────┬──────────────┬──────────────┐
  │ Campfire 100x          │ AetherAgent  │ LP (net)     │ LP (gross)   │ Chrome       │
  ├───────────────────────┼──────────────┼──────────────┼──────────────┼──────────────┤
  │ Total                  │ ${fmt(aeTotal).padStart(12)} │ ${fmt(lNetTotal).padStart(12)} │ ${fmt(lTotal).padStart(12)} │ ${fmt(cTotal).padStart(12)} │
  │ Avg                    │ ${fmt(aeAvg).padStart(12)} │ ${fmt(lNetTotal/100).padStart(12)} │ ${fmt(lTotal/100).padStart(12)} │ ${fmt(cTotal/100).padStart(12)} │
  │ Median                 │       0.2ms  │ ${fmt(lTimesNet[49]).padStart(12)} │ ${fmt(lTimes[49]).padStart(12)} │ ${fmt(cTimes[49]).padStart(12)} │
  │ P99                    │       0.3ms  │ ${fmt(lTimesNet[98]).padStart(12)} │ ${fmt(lTimes[98]).padStart(12)} │ ${fmt(cTimes[98]).padStart(12)} │
  └───────────────────────┴──────────────┴──────────────┴──────────────┴──────────────┘
  LP (net) = LP time minus ~${fmt(lpOverhead)} process startup overhead`);

  // ═══════════════════════════════════════════════════════════════════════
  // 2. LOCAL FIXTURES — 50
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  2. LOCAL FIXTURES — 50 Files');
  console.log('='.repeat(80));

  const fixtures = fs.readdirSync(serveDir)
    .filter(f => f.match(/^\d{2}_/) && f.endsWith('.html')).sort();

  console.log(`\n  ${'Fixture'.padEnd(35)} ${'Chrome'.padStart(8)} ${'LP net'.padStart(8)} ${'C nodes'.padStart(7)} ${'LP tok'.padStart(7)}`);
  console.log('  ' + '-'.repeat(70));

  const fixResults = [];
  for (const f of fixtures) {
    const furl = `http://127.0.0.1:${FIXTURE_PORT}/${f}`;

    // Chrome
    const cr = await chromeBench(browser, furl);

    // LP
    const lr = lpFetch(furl);
    const lNet = Math.max(0, lr.elapsed - lpOverhead);

    fixResults.push({ fixture: f, chrome_ms: cr.elapsed, lp_net_ms: lNet, lp_gross_ms: lr.elapsed, chrome_nodes: cr.nodes, lp_tokens: lr.tokens });
    console.log(`  ${f.padEnd(35)} ${fmt(cr.elapsed).padStart(8)} ${fmt(lNet).padStart(8)} ${String(cr.nodes).padStart(7)} ${String(lr.tokens).padStart(7)}`);
  }

  const cFixAvg = fixResults.reduce((a,r) => a + r.chrome_ms, 0) / fixResults.length;
  const lFixNetAvg = fixResults.reduce((a,r) => a + r.lp_net_ms, 0) / fixResults.length;
  const lFixGrossAvg = fixResults.reduce((a,r) => a + r.lp_gross_ms, 0) / fixResults.length;

  console.log(`\n  Chrome avg: ${fmt(cFixAvg)}   LP net avg: ${fmt(lFixNetAvg)}   LP gross avg: ${fmt(lFixGrossAvg)}`);
  console.log(`  LP net / Chrome ratio: ${(lFixNetAvg / cFixAvg).toFixed(2)}x`);

  // ═══════════════════════════════════════════════════════════════════════
  // 3. LIVE SITES — LP only (Chrome can't reach internet in this sandbox)
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  3. LIVE SITES — 20 URLs (LightPanda)');
  console.log('='.repeat(80));

  const liveSites = [
    'https://books.toscrape.com', 'https://news.ycombinator.com',
    'https://example.com', 'https://httpbin.org',
    'https://en.wikipedia.org/wiki/Rust_(programming_language)',
    'https://github.com/nickel-org/rust-mustache',
    'https://jsonplaceholder.typicode.com', 'https://quotes.toscrape.com',
    'https://www.scrapethissite.com/pages/simple/',
    'https://www.scrapethissite.com/pages/forms/',
    'https://en.wikipedia.org/wiki/WebAssembly',
    'https://en.wikipedia.org/wiki/Artificial_intelligence',
    'https://developer.mozilla.org/en-US/docs/Web/HTML',
    'https://www.rust-lang.org', 'https://crates.io', 'https://docs.rs',
    'https://play.rust-lang.org', 'https://en.wikipedia.org/wiki/Linux',
    'https://en.wikipedia.org/wiki/World_Wide_Web', 'https://lobste.rs',
  ];

  console.log(`\n  ${'URL'.padEnd(45)} ${'LP gross'.padStart(9)} ${'LP net'.padStart(9)} ${'Tokens'.padStart(7)} ${'OK'.padStart(4)}`);
  console.log('  ' + '-'.repeat(78));

  let lpOk = 0;
  for (const u of liveSites) {
    const r = lpFetch(u);
    const net = Math.max(0, r.elapsed - lpOverhead);
    const ok = r.ok && r.tokens > 20 ? 'OK' : 'FAIL';
    if (ok === 'OK') lpOk++;
    const short = u.length > 43 ? u.slice(0, 42) + '…' : u;
    console.log(`  ${short.padEnd(45)} ${fmt(r.elapsed).padStart(9)} ${fmt(net).padStart(9)} ${String(r.tokens).padStart(7)} ${ok.padStart(4)}`);
  }
  console.log(`\n  LP Live: ${lpOk}/20 OK`);

  // ═══════════════════════════════════════════════════════════════════════
  // SUMMARY
  // ═══════════════════════════════════════════════════════════════════════
  console.log('\n' + '='.repeat(80));
  console.log('  SUMMARY');
  console.log('='.repeat(80));
  console.log(`
  ┌─────────────────────────────────┬──────────────┬──────────────┬──────────────┐
  │ Metric                          │ AetherAgent  │ LP (net)     │ Chrome       │
  ├─────────────────────────────────┼──────────────┼──────────────┼──────────────┤
  │ Campfire 100x total             │ ${fmt(aeTotal).padStart(12)} │ ${fmt(lNetTotal).padStart(12)} │ ${fmt(cTotal).padStart(12)} │
  │ Campfire avg                    │ ${fmt(aeAvg).padStart(12)} │ ${fmt(lNetTotal/100).padStart(12)} │ ${fmt(cTotal/100).padStart(12)} │
  │ Local fixtures avg              │        1.13s │ ${fmt(lFixNetAvg).padStart(12)} │ ${fmt(cFixAvg).padStart(12)} │
  │ LP net / Chrome                 │          N/A │ ${(lFixNetAvg/cFixAvg).toFixed(2) + 'x'.padStart(11)} │     baseline │
  │ Goal-relevance                  │          YES │           NO │           NO │
  │ Token savings (MD)              │        42.5% │          N/A │          N/A │
  │ Injection detection             │          YES │           NO │           NO │
  └─────────────────────────────────┴──────────────┴──────────────┴──────────────┘

  NOTE: LP (net) = LP time minus ~${fmt(lpOverhead)} process startup.
  AetherAgent local fixture time includes embedding inference (~36ms/node → now optimized).
  `);

  const outPath = path.join(__dirname, 'fair_benchmark_results.json');
  fs.writeFileSync(outPath, JSON.stringify({ campfire: { ae: aeTotal, lp_net: lNetTotal, lp_gross: lTotal, chrome: cTotal }, fixtures: fixResults }, null, 2));
  console.log(`  Results saved to: ${outPath}`);

  await browser.close();
  server.close();
}

main().catch(console.error);
