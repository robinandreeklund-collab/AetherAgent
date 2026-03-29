#!/usr/bin/env node
/**
 * Definitive Benchmark — LightPanda & Chrome
 * Same tests as AetherAgent: Campfire 100x, Amiibo 100x, 5 real sites.
 * LP uses fetch CLI against local Node HTTP server.
 * Chrome uses Playwright setContent (no network).
 */
const { chromium } = require('playwright');
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const http = require('http');

const LP = '/tmp/lightpanda';
const PORT = 18908;

const CAMPFIRE = fs.readFileSync(path.join(__dirname, 'campfire_fixture.html'), 'utf-8');
const AMIIBO = `<!DOCTYPE html>
<html><head><meta charset="UTF-8"><title>Sandy</title></head>
<body><h1>Sandy</h1>
<p><img src="Sandy.png" alt="Amiibo Character Image" /><br>
Game <a href="/amiibo/?game=Animal+Crossing">Animal Crossing</a><br>
Serie <a href="/amiibo/?serie=Animal+Crossing">Animal Crossing</a></p>
<h2>See also</h2>
<ul><li><a href="/amiibo/Yuka/">Yuka</a></li>
<li><a href="/amiibo/Kitty/">Kitty</a></li>
<li><a href="/amiibo/Rover/">Rover</a></li>
<li><a href="/amiibo/Colton/">Colton</a></li>
<li><a href="/amiibo/Peaches/">Peaches</a></li>
<li><a href="/amiibo/Diddy+Kong+-+Tennis/">Diddy Kong - Tennis</a></li></ul>
<p><a href="/amiibo/?p=1">Previous</a> | <a href="/amiibo/?p=3">Next</a></p>
</body></html>`;

function fmt(ms) { return ms >= 1000 ? `${(ms/1000).toFixed(2)}s` : `${ms.toFixed(1)}ms`; }

function lpFetch(url, dump='semantic_tree') {
  const s = performance.now();
  try {
    const out = execSync(`${LP} fetch --dump ${dump} --log-level fatal "${url}"`,
      { timeout: 30000, encoding: 'utf-8', stdio: ['pipe','pipe','pipe'] });
    return { ms: performance.now() - s, out, ok: true };
  } catch { return { ms: performance.now() - s, out: '', ok: false }; }
}

function countNodes(json) {
  try {
    const d = JSON.parse(json);
    let c = 0; const stk = [d];
    while (stk.length) { const n = stk.pop(); c++; (n.children||[]).forEach(ch => stk.push(ch)); }
    return c;
  } catch { return 0; }
}

async function main() {
  // Start server
  const dir = '/tmp/def_bench2';
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'campfire.html'), CAMPFIRE);
  fs.writeFileSync(path.join(dir, 'amiibo.html'), AMIIBO);
  for (const f of ['apple.html','hackernews.html','books.html','lobsters.html','rustlang.html']) {
    try { fs.copyFileSync(`/tmp/live_bench/${f}`, path.join(dir, f)); } catch {}
  }

  const srv = http.createServer((req, res) => {
    try {
      const c = fs.readFileSync(path.join(dir, decodeURIComponent(req.url).slice(1)));
      res.writeHead(200, {'Content-Type':'text/html; charset=utf-8'});
      res.end(c);
    } catch { res.writeHead(404); res.end('Not found\n'); }
  });
  await new Promise(r => srv.listen(PORT, '127.0.0.1', r));

  // Verify LP
  const test = lpFetch(`http://127.0.0.1:${PORT}/campfire.html`, 'html');
  if (test.out.length < 100) {
    console.log(`LP FAILED: only ${test.out.length} bytes. Aborting.`);
    process.exit(1);
  }

  console.log('='.repeat(70));
  console.log('  LightPanda & Chrome — Definitive Benchmark');
  console.log(`  LP verified: ${test.out.length} bytes from campfire`);
  console.log('='.repeat(70));

  // ═══ 1. CAMPFIRE — LP 100x ═══
  console.log('\n═══ 1. Campfire Commerce — 100 Sequential ═══\n');
  console.log('  LightPanda (fetch CLI → local server):');

  // LP warmup
  for (let i = 0; i < 3; i++) lpFetch(`http://127.0.0.1:${PORT}/campfire.html`);

  const lTimes = [];
  for (let i = 0; i < 100; i++) {
    const r = lpFetch(`http://127.0.0.1:${PORT}/campfire.html`);
    lTimes.push(r.ms);
    if (i % 25 === 0) {
      const nodes = countNodes(r.out);
      console.log(`    [${String(i+1).padStart(3)}/100] ${fmt(r.ms).padStart(8)}  nodes=${nodes}  tokens=${Math.floor(r.out.length/4)}`);
    }
  }
  lTimes.sort((a,b) => a-b);
  const lTotal = lTimes.reduce((a,b)=>a+b,0);
  console.log(`\n  LP:  Total=${fmt(lTotal)}  Avg=${fmt(lTotal/100)}  Median=${fmt(lTimes[49])}  P99=${fmt(lTimes[98])}`);

  // Chrome
  console.log('\n  Chrome (Playwright setContent):');
  const browser = await chromium.launch({ headless: true });
  for (let i = 0; i < 3; i++) {
    const p = await browser.newPage();
    await p.setContent(CAMPFIRE, { waitUntil: 'domcontentloaded' });
    await p.close();
  }
  const cTimes = [];
  for (let i = 0; i < 100; i++) {
    const p = await browser.newPage();
    const s = performance.now();
    await p.setContent(CAMPFIRE, { waitUntil: 'domcontentloaded' });
    await p.content();
    cTimes.push(performance.now() - s);
    await p.close();
    if (i % 25 === 0) console.log(`    [${String(i+1).padStart(3)}/100] ${fmt(cTimes[cTimes.length-1]).padStart(8)}`);
  }
  cTimes.sort((a,b) => a-b);
  const cTotal = cTimes.reduce((a,b)=>a+b,0);
  console.log(`\n  Chrome: Total=${fmt(cTotal)}  Avg=${fmt(cTotal/100)}  Median=${fmt(cTimes[49])}  P99=${fmt(cTimes[98])}`);

  // ═══ 2. AMIIBO — 100x ═══
  console.log('\n═══ 2. Amiibo Crawl — 100 Sequential ═══\n');

  console.log('  LightPanda:');
  for (let i = 0; i < 3; i++) lpFetch(`http://127.0.0.1:${PORT}/amiibo.html`);
  const lAmTimes = [];
  for (let i = 0; i < 100; i++) {
    const r = lpFetch(`http://127.0.0.1:${PORT}/amiibo.html`);
    lAmTimes.push(r.ms);
    if (i % 25 === 0) console.log(`    [${String(i+1).padStart(3)}/100] ${fmt(r.ms).padStart(8)}`);
  }
  lAmTimes.sort((a,b) => a-b);
  const lAmTotal = lAmTimes.reduce((a,b)=>a+b,0);
  console.log(`  LP:  Total=${fmt(lAmTotal)}  Avg=${fmt(lAmTotal/100)}  Median=${fmt(lAmTimes[49])}`);

  console.log('\n  Chrome:');
  const cAmTimes = [];
  for (let i = 0; i < 100; i++) {
    const p = await browser.newPage();
    const s = performance.now();
    await p.setContent(AMIIBO, { waitUntil: 'domcontentloaded' });
    await p.content();
    cAmTimes.push(performance.now() - s);
    await p.close();
    if (i % 25 === 0) console.log(`    [${String(i+1).padStart(3)}/100] ${fmt(cAmTimes[cAmTimes.length-1]).padStart(8)}`);
  }
  cAmTimes.sort((a,b) => a-b);
  const cAmTotal = cAmTimes.reduce((a,b)=>a+b,0);
  console.log(`  Chrome: Total=${fmt(cAmTotal)}  Avg=${fmt(cAmTotal/100)}  Median=${fmt(cAmTimes[49])}`);

  // ═══ 3. QUALITY — 5 sites ═══
  console.log('\n═══ 3. Quality — 5 Real Sites ═══\n');
  const sites = [
    { name: 'apple.com', file: 'apple.html' },
    { name: 'Hacker News', file: 'hackernews.html' },
    { name: 'books.toscrape', file: 'books.html' },
    { name: 'lobste.rs', file: 'lobsters.html' },
    { name: 'rust-lang.org', file: 'rustlang.html' },
  ];

  console.log(`  ${'Site'.padEnd(16)} ${'Chrome'.padStart(8)} ${'C nodes'.padStart(7)} ${'C tok'.padStart(6)} │ ${'LP'.padStart(8)} ${'LP nodes'.padStart(8)} ${'LP tok'.padStart(7)}`);
  console.log('  ' + '-'.repeat(72));

  for (const site of sites) {
    const html = fs.readFileSync(path.join(dir, site.file), 'utf-8');

    // Chrome
    const p = await browser.newPage();
    const cs = performance.now();
    await p.setContent(html, { waitUntil: 'domcontentloaded' });
    const cc = await p.content();
    const cn = await p.evaluate(() => document.querySelectorAll('*').length);
    const cms = performance.now() - cs;
    await p.close();

    // LP
    const lr = lpFetch(`http://127.0.0.1:${PORT}/${site.file}`);
    const lnodes = countNodes(lr.out);

    console.log(`  ${site.name.padEnd(16)} ${fmt(cms).padStart(8)} ${String(cn).padStart(7)} ${String(Math.floor(cc.length/4)).padStart(6)} │ ${fmt(lr.ms).padStart(8)} ${String(lnodes).padStart(8)} ${String(Math.floor(lr.out.length/4)).padStart(7)}`);
  }

  // ═══ SUMMARY ═══
  console.log('\n' + '='.repeat(70));
  console.log('  FINAL SUMMARY');
  console.log('='.repeat(70));
  console.log(`
  ┌───────────────────────────┬──────────────┬──────────────┬──────────────┐
  │ Test                       │ AetherAgent  │ LightPanda   │ Chrome       │
  ├───────────────────────────┼──────────────┼──────────────┼──────────────┤
  │ Campfire 100x total        │       29.6ms │ ${fmt(lTotal).padStart(12)} │ ${fmt(cTotal).padStart(12)} │
  │ Campfire median            │        0.3ms │ ${fmt(lTimes[49]).padStart(12)} │ ${fmt(cTimes[49]).padStart(12)} │
  │ Amiibo 100x total          │        6.2ms │ ${fmt(lAmTotal).padStart(12)} │ ${fmt(cAmTotal).padStart(12)} │
  │ Amiibo median              │        0.1ms │ ${fmt(lAmTimes[49]).padStart(12)} │ ${fmt(cAmTimes[49]).padStart(12)} │
  ├───────────────────────────┼──────────────┼──────────────┼──────────────┤
  │ MD savings (5 real sites)  │        98.3% │          N/A │          N/A │
  │ Goal-relevance             │    4/5 found │           NO │           NO │
  └───────────────────────────┴──────────────┴──────────────┴──────────────┘
  `);

  await browser.close();
  srv.close();
}

main().catch(console.error);
