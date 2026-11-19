import { createServer } from 'node:http';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { resolve, join, extname, sep, dirname } from 'node:path';
import { chromium } from 'playwright';
import { cases, option } from './cases.mjs';
import { checkInteractions } from './interactions.mjs';

const root = resolve('dist');
const output = resolve(option('--output', 'artifacts/web'));
await mkdir(dirname(output), { recursive: true });
await mkdir(output); // Refuse stale output; choose a new path for each capture.
const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.ttf': 'font/ttf', '.woff2': 'font/woff2', '.png': 'image/png', '.svg': 'image/svg+xml', '.json': 'application/json' };
const server = createServer(async (request, response) => {
  try {
    const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    const path = resolve(root, '.' + (pathname === '/' ? '/index.html' : pathname));
    if (!path.startsWith(root + sep) || !(await stat(path)).isFile()) {
      response.writeHead(404).end();
      return;
    }
    response.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: https:; font-src 'self' data:; connect-src 'self'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'");
    response.setHeader('Content-Type', mime[extname(path)] ?? 'application/octet-stream');
    response.end(await readFile(path));
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const address = `http://127.0.0.1:${server.address().port}`;
let browser;
try {
  browser = await chromium.launch({ headless: true, args: ['--force-color-profile=srgb'] });
  const records = [];
  for (const { theme, card, id } of cases) {
    const context = await browser.newContext({
      viewport: { width: 800, height: 600 }, deviceScaleFactor: 2,
      colorScheme: 'dark', reducedMotion: 'reduce', locale: 'en-GB',
      timezoneId: 'UTC', serviceWorkers: 'block',
    });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('response', response => {
      if (response.status() >= 400) errors.push(`${response.status()} ${response.url()}`);
    });
    await page.goto(`${address}/?theme=${theme}&card=${card}&capture=1`);
    await page.locator('[data-testid="gallery-card"]').waitFor();
    await page.evaluate(async () => {
      await document.fonts.ready;
      const loaded = [...document.fonts].filter(font => font.status === 'loaded').map(font => font.family.replaceAll('"', ''));
      if (!loaded.includes('Inter') || !loaded.includes('JetBrainsMono Nerd Font')) {
        throw new Error(`Required bundled font faces did not load: ${loaded.join(', ')}`);
      }
      await Promise.all([...document.images].map(image => image.decode()));
      await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
    });
    await page.addStyleTag({ content: '*{animation:none!important;transition:none!important;caret-color:transparent!important}' });
    if (errors.length) throw new Error(errors.join('\n'));
    await page.screenshot({ path: join(output, `${id}.png`), animations: 'disabled', caret: 'hide' });
    records.push({ id, theme, card, viewport: { width: 800, height: 600 }, scaleFactor: 2, browser: browser.version() });
    await context.close();
  }
  const interactions = await checkInteractions(browser, address, output);
  await writeFile(join(output, 'manifest.json'), JSON.stringify({ renderer: 'headless Chromium', cases: records, interactions }, null, 2) + '\n');
  console.log(`Captured ${records.length} real React cases under ${output}`);
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
