import assert from 'node:assert/strict';
import { join } from 'node:path';

// Runs against the built package and the same CSP as the raster capture.
export async function checkInteractions(browser, address, output) {
  const context = await browser.newContext({ viewport: { width: 1600, height: 1000 }, locale: 'en-GB' });
  const page = await context.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    await page.goto(address);
    await page.getByLabel('Theme', { exact: true }).selectOption('cupertino-dark');
    await page.getByLabel('Component', { exact: true }).selectOption('input');
    await page.getByLabel('Search sample', { exact: true }).fill('Edited fixture');
    assert.equal(await page.getByLabel('Search sample', { exact: true }).inputValue(), 'Edited fixture');
    assert.equal(await page.getByLabel('Disabled sample', { exact: true }).isDisabled(), true);
    assert.equal(await page.getByLabel('Masked sample', { exact: true }).getAttribute('type'), 'password');
    await page.getByRole('button', { name: 'Show password', exact: true }).click();
    assert.equal(await page.getByLabel('Masked sample', { exact: true }).getAttribute('type'), 'text');
    await page.getByRole('button', { name: 'Hide password', exact: true }).click();
    assert.equal(await page.getByLabel('Masked sample', { exact: true }).getAttribute('type'), 'password');
    // A rejected clipboard write must not claim success.
    await page.evaluate(() => Object.defineProperty(navigator, 'clipboard', {
      configurable: true, value: { writeText: async () => { throw new Error('Fixture clipboard refusal'); } },
    }));
    await page.getByRole('button', { name: 'Copy fields::input', exact: true }).click();
    await page.getByRole('status').filter({ hasText: 'Clipboard unavailable' }).waitFor();
    await page.getByLabel('Component', { exact: true }).selectOption('action-button');
    assert.equal(await page.getByRole('button', { name: 'Save', exact: true }).isDisabled(), true);
    await page.getByLabel('Component', { exact: true }).selectOption('input');
    assert.equal(await page.getByLabel('Search sample', { exact: true }).inputValue(), 'Search the board');
    await page.evaluate(async () => {
      await document.fonts.ready;
      await Promise.all([...document.images].map(image => image.decode()));
    });
    await page.screenshot({ path: join(output, 'comparison-app.png'), fullPage: true });
    assert.deepEqual(errors, []);
    return ['theme/component selectors', 'editable/disabled inputs', 'mask toggle', 'clipboard refusal', 'card-change reset'];
  } finally {
    await context.close();
  }
}
