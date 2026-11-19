import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { after, before, test } from 'node:test';
import { chromium } from 'playwright';
import { preview } from 'vite';

const inventory = JSON.parse(await readFile(new URL('../src/inventory.json', import.meta.url), 'utf8'));
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

async function fixture(name, check) {
  const item = inventory.find(card => card.name === name);
  assert.ok(item, `Canonical inventory includes ${name}`);
  for (const theme of ['one-dark', 'cupertino-dark']) {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    try {
      await page.goto(`${origin}/?theme=${theme}&card=${item.id}&capture=1`);
      await page.getByRole('heading', { name, exact: true }).waitFor();
      await check(page);
      assert.deepEqual(errors, [], `${theme}: no runtime errors`);
      assert.equal(await page.locator('canvas,img').count(), 0, 'Samples use real DOM');
    } finally { await page.close(); }
  }
}

test('Kit checkbox supports Space and preserves disabled checked state', async () => {
  await fixture('gpui_kit::component::checkbox::Checkbox', async page => {
    const checkbox = page.getByRole('checkbox', { name: 'Show hidden fixtures' });
    assert.equal(await checkbox.isChecked(), true);
    await checkbox.focus();
    await page.keyboard.press('Space');
    assert.equal(await checkbox.isChecked(), false);
    await checkbox.click();
    assert.equal(await checkbox.isChecked(), true);
    const locked = page.getByRole('checkbox', { name: 'Locked option' });
    assert.equal(await locked.isDisabled(), true);
    assert.equal(await locked.isChecked(), true);
  });
});

test('Kit combobox supports filtering, keyboard selection, pointer multiselect and Escape', async () => {
  await fixture('gpui_kit::component::combobox::Combobox', async page => {
    const trigger = page.getByRole('button', { name: 'Choose a fruit', exact: true });
    await trigger.focus();
    await page.keyboard.press('ArrowDown');
    const search = page.getByRole('combobox', { name: 'Search fruit' });
    await search.waitFor();
    assert.equal(await search.evaluate(element => element === document.activeElement), true);
    await search.fill('ch');
    assert.equal(await page.getByRole('option').count(), 1);
    await page.keyboard.press('Enter');
    assert.equal(await trigger.innerText(), 'Cherry');
    assert.equal(await trigger.getAttribute('aria-expanded'), 'false');
    assert.equal(await trigger.evaluate(element => element === document.activeElement), true);
    await trigger.click();
    await page.getByRole('combobox', { name: 'Search fruit' }).waitFor();
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    assert.equal(await trigger.innerText(), 'Elderberry');
    await trigger.click();
    await page.getByRole('combobox', { name: 'Search fruit' }).waitFor();
    await page.keyboard.press('Escape');
    assert.equal(await trigger.getAttribute('aria-expanded'), 'false');
    assert.equal(await trigger.evaluate(element => element === document.activeElement), true);
    const multiple = page.getByRole('button', { name: 'Choose fruit', exact: true });
    await multiple.click();
    await page.getByRole('option', { name: 'Apple', exact: true }).click();
    await page.getByRole('option', { name: 'Banana', exact: true }).click();
    assert.equal(await multiple.innerText(), 'Apple, Banana');
    assert.equal(await page.getByRole('option', { name: 'Apple', exact: true }).getAttribute('aria-selected'), 'true');
    await page.getByRole('option', { name: 'Apple', exact: true }).click();
    assert.equal(await multiple.innerText(), 'Banana');
  });
});

test('Kit inputs clear text, reveal fixture secret and preserve disabled value', async () => {
  await fixture('gpui_kit::component::input::Input', async page => {
    const input = page.getByRole('textbox', { name: 'Fixture name', exact: true });
    await input.fill('Edited fixture');
    assert.equal(await input.inputValue(), 'Edited fixture');
    await page.getByRole('button', { name: 'Clear fixture name' }).focus();
    await page.keyboard.press('Enter');
    assert.equal(await input.inputValue(), '');
    assert.equal(await input.evaluate(element => element === document.activeElement), true);
    const secret = page.getByLabel('Fixture secret', { exact: true });
    assert.equal(await secret.getAttribute('type'), 'password');
    await page.getByRole('button', { name: 'Show password' }).click();
    assert.equal(await secret.getAttribute('type'), 'text');
    assert.equal(await secret.inputValue(), 'not-a-real-secret');
    const disabled = page.getByRole('textbox', { name: 'Disabled fixture', exact: true });
    assert.equal(await disabled.isDisabled(), true);
    assert.equal(await disabled.inputValue(), 'Read only');
  });
});

test('Kit date accepts and clears a real date field', async () => {
  await fixture('gpui_kit::component::date_picker::DatePicker', async page => {
    const date = page.getByLabel('Choose a date', { exact: true });
    assert.equal(await date.inputValue(), '');
    await date.fill('2026-11-19');
    assert.equal(await date.inputValue(), '2026-11-19');
    await page.getByRole('button', { name: 'Clear date' }).focus();
    await page.keyboard.press('Enter');
    assert.equal(await date.inputValue(), '');
    assert.equal(await date.evaluate(element => element === document.activeElement), true);
  });
});

test('Kit OTP masks digits, advances focus and handles backspace', async () => {
  await fixture('gpui_kit::component::input::OtpInput', async page => {
    const first = page.getByLabel('Digit 1 of 6', { exact: true });
    const second = page.getByLabel('Digit 2 of 6', { exact: true });
    await first.focus();
    await page.keyboard.type('12');
    assert.equal(await first.inputValue(), '1');
    assert.equal(await second.inputValue(), '2');
    assert.equal(await first.getAttribute('type'), 'password');
    await page.keyboard.press('Backspace');
    assert.equal(await second.inputValue(), '');
    assert.equal(await second.evaluate(element => element === document.activeElement), true);
  });
});

test('Kit accordion maintains single-open state through keyboard activation', async () => {
  await fixture('gpui_kit::component::accordion::Accordion', async page => {
    const first = page.getByRole('button', { name: 'What is a fixture?' });
    const next = page.getByRole('button', { name: 'Does it read files?' });
    await first.focus();
    await page.keyboard.press('Enter');
    assert.equal(await first.getAttribute('aria-expanded'), 'true');
    assert.equal(await page.getByText('Obviously fake data that a sample shows.').isVisible(), true);
    await page.keyboard.press('Tab');
    assert.equal(await next.evaluate(element => element === document.activeElement), true);
    await page.keyboard.press('Space');
    assert.equal(await first.getAttribute('aria-expanded'), 'false');
    assert.equal(await next.getAttribute('aria-expanded'), 'true');
  });
});

test('Kit speech honestly remains unsupported', async () => {
  await fixture('gpui_kit::component::speech::SpeechButton', async page => {
    const speech = page.getByRole('button', { name: 'Speech is not supported in this build' });
    assert.equal(await speech.isDisabled(), true);
    assert.match(await speech.innerText(), /Speech unavailable/);
    assert.equal(await page.locator('audio').count(), 0);
  });
});
