import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { after, before, test } from 'node:test';
import { chromium } from 'playwright';
import { preview } from 'vite';

const inventory = JSON.parse(await readFile(new URL('../src/inventory.json', import.meta.url), 'utf8'));
let server, browser, origin;
before(async () => {
  server = await preview({ logLevel: 'silent', preview: { host: '127.0.0.1', port: 0 } });
  origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true });
});
after(async () => {
  await browser?.close();
  if (server) await new Promise((resolve, reject) => server.httpServer.close(error => error ? reject(error) : resolve()));
});

async function select(page, name) {
  const id = inventory.find(card => card.name === name).id;
  await page.getByLabel('Component', { exact: true }).selectOption(id);
  await page.getByRole('article', { name, exact: true }).waitFor();
}

async function style(locator) {
  return locator.evaluate(element => {
    const computed = getComputedStyle(element);
    return Object.fromEntries(['position', 'width', 'padding', 'borderWidth', 'display', 'fontSize', 'minHeight'].map(key => [key, computed[key]]));
  });
}

test('lazy controls and content styles stay isolated in both navigation orders', async () => {
  const probes = [
    ['overflow_toolbar::OverflowToolbar', page => page.getByRole('article').locator('details')],
    ['composer::Composer', page => page.getByRole('textbox')],
    ['fields::textarea', page => page.getByRole('textbox', { name: 'Fixture notes' })],
    ['fields::note', page => page.getByText('Changes take effect after a restart.', { exact: true })],
  ];
  for (const theme of ['one-dark', 'cupertino-dark']) {
    for (const [name, locate] of probes) {
      const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
      try {
        await page.goto(`${origin}/?theme=${theme}`);
        await select(page, name);
        const before = await style(locate(page));
        await select(page, name.startsWith('fields::') ? 'overflow_toolbar::OverflowToolbar' : 'anchored::Anchor');
        await select(page, name);
        assert.deepEqual(await style(locate(page)), before, `${theme}: ${name}`);
      } finally { await page.close(); }
    }
  }
});
