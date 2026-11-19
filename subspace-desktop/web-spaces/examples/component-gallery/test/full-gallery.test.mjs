import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { before, after, test } from 'node:test';
import { chromium } from 'playwright';
import { preview } from 'vite';

const inventory = JSON.parse(readFileSync(new URL('../src/inventory.json', import.meta.url), 'utf8'));
let server;
let browser;
let origin;
before(async () => {
  server = await preview({ logLevel: 'silent', preview: { host: '127.0.0.1', port: 0 } });
  origin = `http://127.0.0.1:${server.httpServer.address().port}`;
  browser = await chromium.launch({ headless: true });
});
after(async () => {
  await browser?.close();
  if (server) await new Promise((resolve, reject) => server.httpServer.close(error => error ? reject(error) : resolve()));
});

test('all180canonical cards render real DOM in both themes without missing implementations', async () => {
  const context = await browser.newContext({ viewport: { width: 800, height: 600 } });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.goto(origin);
    for (const theme of ['one-dark', 'cupertino-dark']) {
      await page.getByLabel('Theme', { exact: true }).selectOption(theme);
      assert.equal(await page.locator('html').getAttribute('data-theme'), theme);
      for (const item of inventory) {
        await page.getByLabel('Component', { exact: true }).selectOption(item.id);
        const card = page.getByRole('article', { name: item.name, exact: true });
        await card.waitFor();
        assert.equal(await card.getAttribute('data-card-id'), item.id);
        assert.equal(await card.getAttribute('data-implemented'), 'true');
        assert.ok(await card.locator('.sample-caption').count() > 0, item.name);
        assert.equal(await card.locator('canvas, iframe, object, embed').count(), 0, item.name);
        assert.ok(await card.locator('button, input, textarea, select, svg, p, span, table, details, fieldset').count() > 0, item.name);
      }
    }
    assert.deepEqual(errors, []);
  } finally {
    await context.close();
  }
});

test('lazy module reuse preserves fixture state through parent rerenders and theme changes', async () => {
  const page = await browser.newPage();
  const item = inventory.find(card => card.name === 'gpui_kit::component::checkbox::Checkbox');
  try {
    await page.goto(`${origin}/?card=${item.id}`);
    const checkbox = page.getByRole('checkbox', { name: 'Show hidden fixtures' });
    await checkbox.uncheck();
    await page.getByRole('button', { name: `Copy ${item.name}`, exact: true }).click();
    assert.equal(await checkbox.isChecked(), false);
    await page.getByLabel('Theme', { exact: true }).selectOption('cupertino-dark');
    assert.equal(await checkbox.isChecked(), false);
  } finally {
    await page.close();
  }
});

test('category and multiword search give honest coverage with keyboard review navigation', async () => {
  const page = await browser.newPage();
  try {
    await page.goto(origin);
    assert.equal(await page.locator('[data-implementation-count]').getAttribute('data-implementation-count'), '180');
    assert.equal(await page.getByLabel('Component', { exact: true }).locator('option').count(), 180);
    await page.getByLabel('Category', { exact: true }).selectOption('choices');
    assert.equal(await page.getByRole('group', { name: 'Matching components' }).getByRole('button').count(), 5);
    await page.getByLabel('Search components').fill('toggle switch');
    const matching = page.getByRole('group', { name: 'Matching components' }).getByRole('button');
    assert.equal(await matching.count(), 1);
    await matching.focus();
    await page.keyboard.press('Enter');
    assert.equal(await page.getByRole('article').getAttribute('data-native-name'), 'toggle::switch');
    await page.getByLabel('Search components').fill('no-such-card');
    assert.equal(await page.getByText('No matching cards. Clear the search or choose another category.').isVisible(), true);
    await page.getByLabel('Search components').fill('');
    await page.getByLabel('Category', { exact: true }).selectOption('all');
    await page.getByLabel('Component', { exact: true }).selectOption('action-button');
    await page.getByRole('button', { name: 'Next card', exact: true }).focus();
    await page.keyboard.press('Enter');
    assert.notEqual(await page.getByRole('article').getAttribute('data-card-id'), 'action-button');
    assert.match(page.url(), /card=/);
  } finally {
    await page.close();
  }
});
