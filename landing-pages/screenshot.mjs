import { chromium } from 'playwright-core';
const browser = await chromium.launch({
  executablePath: '/opt/pw-browsers/chromium-1194/chrome-linux/chrome',
  args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-gpu']
});
const ctx = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const page = await ctx.newPage();

console.log('Taking concept 1...');
await page.goto('file:///home/user/AetherAgent/landing-pages/concept-1-the-reduction.html', { waitUntil: 'domcontentloaded', timeout: 15000 });
await page.waitForTimeout(2000);
await page.screenshot({ path: '/home/user/AetherAgent/landing-pages/concept-1-hero.png' });
await page.screenshot({ path: '/home/user/AetherAgent/landing-pages/concept-1-full.png', fullPage: true });
console.log('Concept 1 done');

console.log('Taking concept 2...');
await page.goto('file:///home/user/AetherAgent/landing-pages/concept-2-the-signal.html', { waitUntil: 'domcontentloaded', timeout: 15000 });
await page.waitForTimeout(2000);
await page.screenshot({ path: '/home/user/AetherAgent/landing-pages/concept-2-hero.png' });
await page.screenshot({ path: '/home/user/AetherAgent/landing-pages/concept-2-full.png', fullPage: true });
console.log('Concept 2 done');

await browser.close();
console.log('All done');
