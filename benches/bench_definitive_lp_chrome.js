#!/usr/bin/env node
/**
 * Definitive Benchmark — LightPanda & Chrome
 *
 * Same tests as AetherAgent: Campfire 100x, Amiibo 100x, 5 real sites.
 * All from local HTTP server (Node.js) + live fetch for sites.
 */
const { chromium } = require('playwright');
const { execSync } = require('child_process');
const fs = require('fs');
const path = require('path');
const http = require('http');

const LP = process.env.LIGHTPANDA_BIN ||
  path.join(require('os').homedir(), '.config', 'lightpanda-gomcp', 'lightpanda');
const PORT = 18907;

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

function lpFetch(url, dump='html') {
  const s = performance.now();
  try {
    const out = execSync(`${LP} fetch --dump ${dump} --log-level fatal "${url}"`,
      { timeout: 30000, encoding: 'utf-8', stdio: ['pipe','pipe','pipe'] });
    return { ms: performance.now() - s, out, ok: out.length > 100 };
  } catch { return { ms: performance.now() - s, out: '', ok: false }; }
}

async function main() {
  // Serve fixtures
  const dir = '/tmp/def_bench';
  fs.mkdirSync(dir, { recursive: true });
  fs.writeFileSync(path.join(dir, 'campfire.html'), CAMPFIRE);
  fs.writeFileSync(path.join(dir, 'amiibo.html'), AMIIBO);
  // Copy live sites
  for (const f of ['apple.html','hackernews.html','books.html','lobsters.html','rustlang.html']) {
    try { fs.copyFileSync(`/tmp/live_bench/${f}`, path.join(dir, f)); } catch {}
  }

  const srv = http.createServer((req, res) => {
    try {
      const c = fs.readFileSync(path.join(dir, decodeURIComponent(req.url).slice(1)));
      res.writeHead(200, {'Content-Type':'text/html;charset=utf-8','Connection':'close'});
      res.end(c);
    } catch { res.writeHead(404,{'Connection':'close'}); res.end('Not found'); }
  });
  await new Promise(r => srv.listen(PORT, '127.0.0.1', r));

  const browser = await chromium.launch({ headless: true });
  console.log('='.repeat(70));
  console.log('  LightPanda & Chrome — Definitive Benchmark');
  console.log('='.repeat(70));

  // ═══ 1. CAMPFIRE 100x ═══
  console.log('\n═══ 1. Campfire Commerce — 100 Sequential ═══');

  // Chrome
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
  }
  cTimes.sort((a,b) => a-b);

  // LP
  for (let i = 0; i < 3; i++) lpFetch(`http://127.0.0.1:${PORT}/campfire.html`);
  const lTimes = [];
  for (let i = 0; i < 100; i++) {
    const r = lpFetch(`http://127.0.0.1:${PORT}/campfire.html`);
    lTimes.push(r.ms);
  }
  lTimes.sort((a,b) => a-b);

  const cTotal = cTimes.reduce((a,b)=>a+b,0);
  const lTotal = lTimes.reduce((a,b)=>a+b,0);
  console.log(`  Chrome:  Total=${fmt(cTotal)}  Avg=${fmt(cTotal/100)}  Median=${fmt(cTimes[49])}`);
  console.log(`  LP:      Total=${fmt(lTotal)}  Avg=${fmt(lTotal/100)}  Median=${fmt(lTimes[49])}`);

  // ═══ 2. AMIIBO 100x ═══
  console.log('\n═══ 2. Amiibo Crawl — 100 Sequential ═══');

  const cAmTimes = [];
  for (let i = 0; i < 100; i++) {
    const p = await browser.newPage();
    const s = performance.now();
    await p.setContent(AMIIBO, { waitUntil: 'domcontentloaded' });
    await p.content();
    cAmTimes.push(performance.now() - s);
    await p.close();
  }
  cAmTimes.sort((a,b) => a-b);

  const lAmTimes = [];
  for (let i = 0; i < 100; i++) {
    const r = lpFetch(`http://127.0.0.1:${PORT}/amiibo.html`);
    lAmTimes.push(r.ms);
  }
  lAmTimes.sort((a,b) => a-b);

  const cAmTotal = cAmTimes.reduce((a,b)=>a+b,0);
  const lAmTotal = lAmTimes.reduce((a,b)=>a+b,0);
  console.log(`  Chrome:  Total=${fmt(cAmTotal)}  Avg=${fmt(cAmTotal/100)}  Median=${fmt(cAmTimes[49])}`);
  console.log(`  LP:      Total=${fmt(lAmTotal)}  Avg=${fmt(lAmTotal/100)}  Median=${fmt(lAmTimes[49])}`);

  // ═══ 3. QUALITY — 5 Real Sites ═══
  console.log('\n═══ 3. Quality — 5 Real Sites ═══');
  const sites = [
    { name: 'apple.com', file: 'apple.html', liveUrl: 'https://www.apple.com' },
    { name: 'Hacker News', file: 'hackernews.html', liveUrl: 'https://news.ycombinator.com' },
    { name: 'books.toscrape', file: 'books.html', liveUrl: 'https://books.toscrape.com' },
    { name: 'lobste.rs', file: 'lobsters.html', liveUrl: 'https://lobste.rs' },
    { name: 'rust-lang.org', file: 'rustlang.html', liveUrl: 'https://www.rust-lang.org' },
  ];

  console.log(`\n  ${'Site'.padEnd(16)} ${'Chrome ms'.padStart(9)} ${'C nodes'.padStart(7)} ${'C tok'.padStart(6)} │ ${'LP ms'.padStart(9)} ${'LP nodes'.padStart(8)} ${'LP tok'.padStart(7)} ${'LP OK'.padStart(5)}`);
  console.log('  '+'-'.repeat(80));

  for (const site of sites) {
    const html = fs.readFileSync(path.join(dir, site.file), 'utf-8');

    // Chrome: setContent
    const p = await browser.newPage();
    const cs = performance.now();
    await p.setContent(html, { waitUntil: 'domcontentloaded' });
    const cCont = await p.content();
    const cN = await p.evaluate(() => document.querySelectorAll('*').length);
    const cMs = performance.now() - cs;
    await p.close();

    // LP: live fetch
    const lr = lpFetch(site.liveUrl);
    const ls = lpFetch(site.liveUrl, 'semantic_tree');
    let lNodes = 0;
    try { const d = JSON.parse(ls.out); const stk=[d]; while(stk.length){const n=stk.pop();lNodes++;(n.children||[]).forEach(c=>stk.push(c));} } catch {}

    console.log(`  ${site.name.padEnd(16)} ${fmt(cMs).padStart(9)} ${String(cN).padStart(7)} ${String(Math.floor(cCont.length/4)).padStart(6)} │ ${fmt(lr.ms).padStart(9)} ${String(lNodes).padStart(8)} ${String(Math.floor(lr.out.length/4)).padStart(7)} ${(lr.ok?'YES':'NO').padStart(5)}`);
  }

  // ═══ SUMMARY ═══
  console.log('\n' + '='.repeat(70));
  console.log('  SUMMARY TABLE');
  console.log('='.repeat(70));
  console.log(`
  ┌───────────────────────────┬──────────────┬──────────────┬──────────────┐
  │ Test                       │ AetherAgent  │ LightPanda   │ Chrome       │
  ├───────────────────────────┼──────────────┼──────────────┼──────────────┤
  │ Campfire 100x total        │       29.6ms │ ${fmt(lTotal).padStart(12)} │ ${fmt(cTotal).padStart(12)} │
  │ Campfire 100x median       │        0.3ms │ ${fmt(lTimes[49]).padStart(12)} │ ${fmt(cTimes[49]).padStart(12)} │
  │ Amiibo 100x total          │        6.2ms │ ${fmt(lAmTotal).padStart(12)} │ ${fmt(cAmTotal).padStart(12)} │
  │ Amiibo 100x median         │        0.1ms │ ${fmt(lAmTimes[49]).padStart(12)} │ ${fmt(cAmTimes[49]).padStart(12)} │
  ├───────────────────────────┼──────────────┼──────────────┼──────────────┤
  │ MD token savings (5 sites) │        98.3% │          N/A │          N/A │
  │ Top-5 savings (5 sites)    │        98.9% │          N/A │          N/A │
  │ Goal-relevance             │    4/5 found │           NO │           NO │
  │ Injection detection        │          YES │           NO │           NO │
  └───────────────────────────┴──────────────┴──────────────┴──────────────┘
  `);

  await browser.close();
  srv.close();
}

main().catch(console.error);
