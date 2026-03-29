#!/usr/bin/env node
/**
 * LightPanda CDP Benchmark — persistent server, raw WebSocket
 *
 * Navigates pages via CDP protocol (same as production usage).
 * No process spawn overhead — fair comparison with Chrome/AetherAgent.
 */
const WebSocket = require('ws');
const fs = require('fs');
const path = require('path');

const LP_CDP = 'ws://127.0.0.1:9333/';
const FIXTURE_PORT = 18920; // Python fixture server must be running

function cdpNavigate(wsUrl, pageUrl) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(wsUrl);
    let id = 1;
    let sessionId = null;
    let startTime = 0;
    let resolved = false;

    const timeout = setTimeout(() => {
      if (!resolved) { resolved = true; ws.close(); reject(new Error('timeout')); }
    }, 15000);

    ws.on('open', () => {
      startTime = performance.now();
      ws.send(JSON.stringify({ id: id++, method: 'Target.createTarget', params: { url: 'about:blank' } }));
    });

    ws.on('message', (data) => {
      const msg = JSON.parse(data);

      if (msg.id === 1 && msg.result?.targetId) {
        ws.send(JSON.stringify({ id: id++, method: 'Target.attachToTarget',
          params: { targetId: msg.result.targetId, flatten: true } }));
      }

      if (msg.method === 'Target.attachedToTarget') {
        sessionId = msg.params.sessionId;
        ws.send(JSON.stringify({ id: id++, method: 'Page.enable', sessionId }));
        ws.send(JSON.stringify({ id: id++, method: 'Page.navigate',
          params: { url: pageUrl }, sessionId }));
      }

      if (msg.method === 'Page.loadEventFired' && !resolved) {
        const elapsed = performance.now() - startTime;
        resolved = true;
        clearTimeout(timeout);
        // Get DOM node count
        ws.send(JSON.stringify({ id: 999, method: 'DOM.getDocument',
          params: { depth: 0 }, sessionId }));
        // Close target
        setTimeout(() => {
          ws.send(JSON.stringify({ id: id++, method: 'Target.closeTarget',
            params: { targetId: msg.params?.frameId || 'FID-0000000001' } }));
          setTimeout(() => { ws.close(); resolve({ elapsed, ok: true }); }, 100);
        }, 50);
      }
    });

    ws.on('error', (e) => {
      if (!resolved) { resolved = true; clearTimeout(timeout); reject(e); }
    });
  });
}

async function benchmarkUrl(url, runs) {
  const times = [];
  for (let i = 0; i < runs; i++) {
    try {
      // Each navigation needs a fresh WS connection (LP reuses target IDs)
      const { elapsed } = await cdpNavigate(LP_CDP, url);
      times.push(elapsed);
    } catch (e) {
      times.push(15000);
    }
    // Small delay between runs to let LP clean up targets
    await new Promise(r => setTimeout(r, 50));
  }
  return times;
}

function fmt(ms) { return ms >= 1000 ? `${(ms/1000).toFixed(2)}s` : `${ms.toFixed(1)}ms`; }

async function main() {
  console.log('='.repeat(70));
  console.log('  LightPanda CDP Benchmark — Persistent Server');
  console.log('='.repeat(70));

  // Verify
  try {
    const { elapsed } = await cdpNavigate(LP_CDP, `http://127.0.0.1:${FIXTURE_PORT}/campfire.html`);
    console.log(`\n  LP CDP verified: campfire in ${fmt(elapsed)}`);
  } catch (e) {
    console.log(`\n  LP CDP FAILED: ${e.message}`);
    process.exit(1);
  }

  // ═══ Campfire 100x ═══
  console.log('\n═══ Campfire Commerce — 100x (CDP persistent) ═══');
  const campfireUrl = `http://127.0.0.1:${FIXTURE_PORT}/campfire.html`;

  // Warmup
  await benchmarkUrl(campfireUrl, 3);

  const cTimes = await benchmarkUrl(campfireUrl, 100);
  cTimes.sort((a, b) => a - b);
  const cTotal = cTimes.reduce((a, b) => a + b, 0);
  console.log(`  Total:  ${fmt(cTotal)}`);
  console.log(`  Median: ${fmt(cTimes[49])}`);
  console.log(`  P99:    ${fmt(cTimes[98])}`);
  console.log(`  Min:    ${fmt(cTimes[0])}`);
  console.log(`  Max:    ${fmt(cTimes[99])}`);

  // ═══ Amiibo 100x ═══
  console.log('\n═══ Amiibo Crawl — 100x (CDP persistent) ═══');
  const amiiboUrl = `http://127.0.0.1:${FIXTURE_PORT}/amiibo.html`;

  await benchmarkUrl(amiiboUrl, 3);
  const aTimes = await benchmarkUrl(amiiboUrl, 100);
  aTimes.sort((a, b) => a - b);
  const aTotal = aTimes.reduce((a, b) => a + b, 0);
  console.log(`  Total:  ${fmt(aTotal)}`);
  console.log(`  Median: ${fmt(aTimes[49])}`);

  // ═══ Live Sites ═══
  console.log('\n═══ 5 Live Sites (CDP persistent) ═══\n');
  const sites = [
    ['apple.com', 'https://www.apple.com'],
    ['Hacker News', 'https://news.ycombinator.com'],
    ['books.toscrape', 'https://books.toscrape.com'],
    ['lobste.rs', 'https://lobste.rs'],
    ['rust-lang.org', 'https://www.rust-lang.org'],
  ];

  for (const [name, url] of sites) {
    try {
      const { elapsed } = await cdpNavigate(LP_CDP, url);
      console.log(`  ${name.padEnd(16)} ${fmt(elapsed).padStart(8)}  ✓`);
    } catch (e) {
      console.log(`  ${name.padEnd(16)} FAILED: ${e.message}`);
    }
  }

  // Summary
  console.log(`\n${'='.repeat(70)}`);
  console.log('  LP CDP SUMMARY');
  console.log(`${'='.repeat(70)}`);
  console.log(`  Campfire 100x:  Total=${fmt(cTotal)}  Median=${fmt(cTimes[49])}`);
  console.log(`  Amiibo 100x:    Total=${fmt(aTotal)}  Median=${fmt(aTimes[49])}`);
}

main().catch(console.error);
