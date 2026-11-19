import assert from 'node:assert/strict';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { chromium } from 'playwright';
import { preview } from 'vite';
import { option } from './cases.mjs';

const inventory = JSON.parse(await readFile(new URL('../src/inventory.json', import.meta.url), 'utf8'));
const start = Number(option('--start', '0'));
const limit = Number(option('--limit', '20'));
const minimumHeight = Number(option('--height', '600'));
assert.ok(Number.isInteger(start) && start >= 0 && start < 360);
assert.ok(Number.isInteger(limit) && limit >= 1 && limit <= 30, 'Capture at most30views per batch');
assert.ok(Number.isInteger(minimumHeight) && minimumHeight >= 600 && minimumHeight <= 4096);
const cases = ['one-dark', 'cupertino-dark'].flatMap(theme => inventory.map(card => ({ theme, card: card.id, name: card.name, id: `${theme}-${card.id}` }))).slice(start, start + limit);
const output = resolve(option('--output', `artifacts/full-${start}`));
await mkdir(dirname(output), { recursive: true });
await mkdir(output); // A fresh batch only; stale evidence cannot become coverage.
let server;
let browser;
try {
  server = await preview({ logLevel: 'silent', preview: { host: '127.0.0.1', port: 0 } });
  const origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true, args: ['--force-color-profile=srgb'] });
  const records = [];
  for (const record of cases) {
    const context = await browser.newContext({ viewport: { width: 800, height: minimumHeight }, deviceScaleFactor: 2, locale: 'en-GB', timezoneId: 'UTC', reducedMotion: 'reduce', serviceWorkers: 'block' });
    try {
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('response', response => { if (response.status() >= 400) errors.push(`${response.status()} ${new URL(response.url()).pathname}`); });
      await page.goto(`${origin}/?theme=${record.theme}&card=${record.card}&capture=1&full=1`);
      const card = page.locator('[data-testid="gallery-card"]');
      await card.waitFor();
      assert.equal(await card.getAttribute('data-native-name'), record.name);
      await page.evaluate(async () => {
        await document.fonts.ready;
        await Promise.all([...document.images].map(image => image.decode()));
      });
      await page.addStyleTag({ content: '*{animation:none!important;transition:none!important;caret-color:transparent!important}' });
      assert.deepEqual(errors, [], record.id);
      const bounds = await page.locator('.gallery-viewport').boundingBox();
      assert.ok(bounds.height <= 4000, `Unbounded card height: ${record.id}`);
      const raster = await page.screenshot({ path: join(output, `${record.id}.png`), fullPage: true, animations: 'disabled', caret: 'hide' });
      // Full-page raster follows the browser's scroll extent rounding, not ceil(bounds).
      assert.equal(raster.readUInt32BE(16), 1600, record.id);
      records.push({ ...record, viewport: { width: raster.readUInt32BE(16) / 2, height: raster.readUInt32BE(20) / 2 }, scaleFactor: 2, browser: browser.version() });
    } finally {
      await context.close();
    }
  }
  await writeFile(join(output, 'manifest.json'), JSON.stringify({ renderer: 'Bowser headless Chromium', cases: records }, null, 2) + '\n');
  console.log(`Verified batch ${start}..${start + records.length - 1}: ${records.length} real DOM captures`);
} finally {
  await browser?.close();
  if (server) await new Promise((resolve, reject) => server.httpServer.close(error => error ? reject(error) : resolve()));
}
