import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { after, before, test } from 'node:test';
import { chromium } from 'playwright';
import { PNG } from 'pngjs';
import { preview } from 'vite';

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
  if (server) await new Promise((resolve, reject) => {
    server.httpServer.close(error => error ? reject(error) : resolve());
  });
});

test('both themes keep real editable inputs and a full-width masked field', async () => {
  for (const theme of ['one-dark', 'cupertino-dark']) {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
    try {
      await page.goto(`${origin}/?theme=${theme}&card=input&capture=1`);
      await page.evaluate(() => document.fonts.ready);
      const search = page.getByRole('textbox', { name: 'Search sample' });
      await search.fill('Edited fixture');
      assert.equal(await search.inputValue(), 'Edited fixture');
      assert.equal(await page.getByRole('textbox', { name: 'Disabled sample' }).isDisabled(), true);

      const masked = page.getByLabel('Masked sample', { exact: true });
      assert.equal(await masked.getAttribute('type'), 'password');
      assert.equal((await masked.boundingBox()).width, 240);
      await page.getByRole('button', { name: 'Show password' }).click();
      assert.equal(await masked.getAttribute('type'), 'text');
      assert.equal(await masked.inputValue(), 'not-a-real-secret');
      await page.getByRole('button', { name: 'Hide password' }).click();
      assert.equal(await masked.getAttribute('type'), 'password');

      const viewport = await page.locator('.gallery-viewport').boundingBox();
      assert.deepEqual(viewport, { x: 0, y: 0, width: 800, height: 600 });
      assert.equal(await page.locator('img,canvas').count(), 0, 'capture must render DOM, not a reference image');
    } finally {
      await page.close();
    }
  }
});

test('input font advances and mask glyph follow native layout without shrinking the hit area', async () => {
  for (const theme of ['one-dark', 'cupertino-dark']) {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
    try {
      await page.goto(`${origin}/?theme=${theme}&card=input&capture=1`);
      await page.evaluate(() => document.fonts.ready);
      const input = page.getByLabel('Masked sample', { exact: true });
      assert.equal(await input.evaluate(element => getComputedStyle(element).textRendering), 'geometricprecision');
      const bounds = await input.boundingBox();
      const toggle = page.getByRole('button', { name: 'Show password' });
      const hit = await toggle.boundingBox();
      const glyph = await toggle.locator('svg').boundingBox();
      // Native Text buttons have no internal padding: the glyph ends at the
      // input's 8px padding plus 1px border. Keep the larger web hit target.
      assert.equal(bounds.x + bounds.width - glyph.x - glyph.width, 9);
      assert.equal(hit.width, 17.5);
      assert.equal(hit.height, 17.5);
      assert.equal(await input.getAttribute('type'), 'password');
    } finally {
      await page.close();
    }
  }
});

test('input fixture fills match committed native interiors in both themes', async () => {
  for (const theme of ['one-dark', 'cupertino-dark']) {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
    try {
      await page.goto(`${origin}/?theme=${theme}&card=input&capture=1`);
      await page.evaluate(() => document.fonts.ready);
      const reference = PNG.sync.read(await readFile(new URL(`../public/references/${theme}-input.png`, import.meta.url)));
      const actual = PNG.sync.read(await page.screenshot({ caret: 'hide' }));
      // Blank interiors of the normal/disabled controls, away from text,
      // rounded corners and borders, in the fixed DPR2 fixture viewport.
      for (const [label, y] of [['Search sample', 210], ['Disabled sample', 320]]) {
        const offset = (y * reference.width + 500) * 4;
        const expected = [...reference.data.subarray(offset, offset + 4)];
        assert.equal(await page.getByRole('textbox', { name: label }).evaluate(element => getComputedStyle(element).backgroundColor), `rgb(${expected.slice(0, 3).join(', ')})`);
        assert.deepEqual([...actual.data.subarray(offset, offset + 4)], expected, `${theme} ${label} fill`);
      }
    } finally {
      await page.close();
    }
  }
});

test('small native monospace labels retain fractional font advances', async () => {
  const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
  try {
    await page.goto(`${origin}/?theme=one-dark&card=action-button&capture=1`);
    await page.evaluate(() => document.fonts.ready);
    const title = page.getByRole('heading', { name: 'action::action_button', exact: true });
    // JetBrains Mono has a 0.6em advance. Hint-rounded browser advances would
    // make this 168px instead of 163.8px and displace the adjacent Copy button.
    assert.ok(Math.abs((await title.boundingBox()).width - 21 * 13 * 0.6) < 0.02);
    assert.equal(await page.getByRole('button', { name: 'Save', exact: true }).isDisabled(), true);
    assert.equal(await page.getByRole('button', { name: 'Viewed', exact: true }).count(), 2);
  } finally {
    await page.close();
  }
});

test('error text is centred using native whole-pixel text layout widths', async () => {
  for (const theme of ['one-dark', 'cupertino-dark']) {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
    try {
      await page.goto(`${origin}/?theme=${theme}&card=error-box&capture=1`);
      await page.evaluate(() => document.fonts.ready);
      const title = await page.getByRole('heading', { name: 'Could not load' }).boundingBox();
      const message = await page.getByText('The fixture service answered 503 Service Unavailable.', { exact: true }).boundingBox();
      assert.equal(title.width, 104);
      assert.equal(title.x, 255);
      assert.equal(message.width, 336);
      assert.equal(message.x, 139);
    } finally {
      await page.close();
    }
  }
});

test('action labels use native rounded text widths rather than hinted control advances', async () => {
  // GPUI TextLayout ceilings text widths to whole logical pixels; its layout
  // then snaps frames to DPR2. Confirmed against the native plate boundaries.
  const expectedX = [27, 105.5, 182, 264.5, 347, 428.5, 498, 567.5, 635, 27];
  const expectedWidth = [68, 66, 72, 72, 71, 59, 59, 57, 70, 70];
  for (const theme of ['one-dark', 'cupertino-dark']) {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
    try {
      await page.goto(`${origin}/?theme=${theme}&card=action-button&capture=1`);
      await page.evaluate(() => document.fonts.ready);
      const buttons = page.locator('.action-button');
      assert.equal(await buttons.count(), expectedX.length);
      for (let i = 0; i < expectedX.length; i++) {
        const bounds = await buttons.nth(i).boundingBox();
        assert.ok(Math.abs(bounds.x - expectedX[i]) < 0.005, `${theme} button ${i} x`);
        assert.ok(Math.abs(bounds.width - expectedWidth[i]) < 0.005, `${theme} button ${i} width`);
      }
    } finally {
      await page.close();
    }
  }
});

test('action fixture plates retain native composites and neutral foreground', async () => {
  for (const theme of ['one-dark', 'cupertino-dark']) {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 }, deviceScaleFactor: 2 });
    try {
      await page.goto(`${origin}/?theme=${theme}&card=action-button&capture=1`);
      await page.evaluate(() => document.fonts.ready);
      const reference = PNG.sync.read(await readFile(new URL(`../public/references/${theme}-action-button.png`, import.meta.url)));
      const actual = PNG.sync.read(await page.screenshot());
      for (const name of ['Accept', 'Reject', 'Caution', 'Neutral', 'Save']) {
        const button = page.getByRole('button', { name, exact: true });
        const bounds = await button.boundingBox();
        const x = Math.round((bounds.x + bounds.width / 2) * 2);
        const y = Math.round(bounds.y * 2 + 4); // Blank interior above the label.
        const offset = (y * reference.width + x) * 4;
        assert.deepEqual([...actual.data.subarray(offset, offset + 4)], [...reference.data.subarray(offset, offset + 4)], `${theme} ${name} plate`);
      }
      assert.equal(await page.getByRole('button', { name: 'Neutral', exact: true }).evaluate(element => getComputedStyle(element).color), theme === 'one-dark' ? 'rgb(171, 178, 191)' : 'rgb(240, 239, 237)');
    } finally {
      await page.close();
    }
  }
});

test('selected presentation uses active secondary colour without implying a toggle', async () => {
  const page = await browser.newPage();
  try {
    await page.goto(`${origin}/?theme=cupertino-dark&card=action-button&capture=1`);
    const selected = page.getByRole('button', { name: 'Viewed', exact: true });
    assert.equal(await selected.nth(0).evaluate(element => getComputedStyle(element).backgroundColor), 'rgb(65, 65, 61)');
    assert.equal(await selected.nth(0).getAttribute('aria-pressed'), null);
    assert.equal(await selected.nth(1).getAttribute('aria-pressed'), 'true');
  } finally {
    await page.close();
  }
});
