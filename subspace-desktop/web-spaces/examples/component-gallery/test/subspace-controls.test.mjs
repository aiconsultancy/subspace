import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { after, before, test } from 'node:test';
import { chromium } from 'playwright';
import { preview } from 'vite';

const inventory = JSON.parse(readFileSync(new URL('../src/inventory.json', import.meta.url), 'utf8'));
const sections = ['buttons', 'choices', 'inputs', 'menus', 'dialogs', 'feedback'];
const retained = ['action::action_button', 'fields::input', 'feedback::error_box'];
const owned = inventory.filter(card => sections.includes(card.section) && !retained.includes(card.name));
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

async function openCard(page, name, theme = 'cupertino-dark') {
  const card = inventory.find(item => item.name === name);
  assert.ok(card, `Canonical card exists: ${name}`);
  await page.goto(`${origin}/?theme=${theme}&card=${card.id}&capture=1`);
  await page.getByRole('article', { name, exact: true }).waitFor();
  // Interaction checks are not raster captures. Let tall native fixture sets
  // scroll so all controls remain reachable below the 600px capture boundary.
  await page.addStyleTag({ content: '[data-capture="true"], [data-capture="true"] body { height:auto; overflow:auto } .gallery-viewport { height:auto }' });
}

async function focused(locator) {
  return locator.evaluate(element => element === document.activeElement);
}

test('all 48 owned canonical cards render semantic React DOM in both themes', async () => {
  assert.equal(owned.length, 48);
  const source = readFileSync(new URL('../src/components/subspace-controls.tsx', import.meta.url), 'utf8');
  const registrations = source.split('export const subspaceControlSamples:')[1];
  for (const card of owned) {
    assert.ok(registrations.includes(card.name === 'Toast' ? 'Toast:' : `'${card.name}':`), card.name);
  }
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  try {
    for (const theme of ['one-dark', 'cupertino-dark']) {
      for (const card of owned) {
        await openCard(page, card.name, theme);
        assert.ok(await page.locator('.sc-sample').count() > 0, `${card.name}: samples exist`);
        assert.equal(await page.locator('img, canvas').count(), 0, `${card.name}: no image substitute`);
      }
    }
    assert.deepEqual(errors, []);
  } finally { await page.close(); }
});

test('segmented controls, tabs, switches and sliders support keyboard state changes', async () => {
  const page = await browser.newPage();
  try {
    await openCard(page, 'segmented::control');
    const segments = page.getByRole('radiogroup', { name: 'No choice yet', exact: true });
    const day = segments.getByRole('radio', { name: 'Day', exact: true });
    const week = segments.getByRole('radio', { name: 'Week', exact: true });
    assert.equal(await day.getAttribute('aria-checked'), 'false');
    await day.focus();
    await page.keyboard.press('ArrowRight');
    assert.equal(await week.getAttribute('aria-checked'), 'true');
    assert.equal(await focused(week), true);

    await openCard(page, 'tabs::tab_bar');
    const tabs = page.getByRole('tablist', { name: 'Plain tabs' });
    await tabs.getByRole('tab', { name: 'Overview', exact: true }).focus();
    await page.keyboard.press('ArrowRight');
    const settings = tabs.getByRole('tab', { name: 'Settings', exact: true });
    assert.equal(await settings.getAttribute('aria-selected'), 'true');
    assert.equal(await focused(settings), true);
    assert.equal(await page.getByRole('tablist', { name: 'Disabled tabs' }).getByRole('tab', { name: 'Issues' }).isDisabled(), true);

    await openCard(page, 'toggle::switch');
    assert.equal(await page.getByRole('switch').count(), 42);
    const toggle = page.getByRole('switch', { name: 'Window, off', exact: true });
    await toggle.focus();
    await page.keyboard.press('Space');
    assert.equal(await toggle.getAttribute('aria-checked'), 'true');
    const disabled = page.getByRole('switch', { name: 'Window, disabled on', exact: true });
    assert.equal(await disabled.isDisabled(), true);
    assert.equal(await disabled.getAttribute('aria-checked'), 'true');

    await openCard(page, 'slider::control');
    const slider = page.getByRole('slider', { name: 'Opacity', exact: true });
    await slider.focus();
    await page.keyboard.press('ArrowRight');
    assert.equal(await slider.inputValue(), '45');
    await page.keyboard.press('End');
    assert.equal(await slider.inputValue(), '100');
    assert.equal(await page.getByRole('slider', { name: 'Locked opacity' }).isDisabled(), true);
  } finally { await page.close(); }
});

test('native input fixtures edit real values and saved secrets clear without exposing a saved value', async () => {
  const page = await browser.newPage();
  try {
    await openCard(page, 'fields::labelled_input');
    await page.getByRole('textbox', { name: 'Inline label', exact: true }).fill('Edited fixture');
    assert.equal(await page.getByRole('textbox', { name: 'Inline label', exact: true }).inputValue(), 'Edited fixture');
    assert.equal(await page.getByRole('textbox', { name: 'Required label', exact: true }).getAttribute('aria-invalid'), 'true');

    await openCard(page, 'fields::code_editor');
    const editor = page.getByRole('textbox', { name: 'JSON sample', exact: true });
    await editor.fill('{"fixture": false}');
    assert.equal(await editor.inputValue(), '{"fixture": false}');

    await openCard(page, 'secret::secret_field');
    const token = page.getByLabel('Token, token saved. Clear the saved token to change it.', { exact: true });
    assert.equal(await token.inputValue(), '');
    assert.equal(await token.getAttribute('readonly'), '');
    await page.getByRole('button', { name: 'Clear saved token', exact: true }).click();
    const replacement = page.getByLabel('Token', { exact: true });
    await replacement.fill('not-a-real-token');
    assert.equal(await replacement.inputValue(), 'not-a-real-token');
    assert.equal(await replacement.getAttribute('type'), 'password');
    await page.getByRole('button', { name: 'Undo clear', exact: true }).click();
    assert.equal(await token.getAttribute('readonly'), '');
    assert.equal(await page.getByRole('button', { name: 'Clear saved password', exact: true }).isDisabled(), true);
  } finally { await page.close(); }
});

test('dropdown keyboard navigation skips disabled actions and Escape restores trigger focus', async () => {
  const page = await browser.newPage();
  try {
    await openCard(page, 'anchored::dropdown_menu');
    const trigger = page.getByRole('button', { name: 'Options', exact: true });
    await trigger.focus();
    await page.keyboard.press('ArrowDown');
    const checked = page.getByRole('menuitemcheckbox', { name: 'Checked option', exact: true });
    await checked.waitFor();
    assert.equal(await focused(checked), true);
    assert.equal(await trigger.getAttribute('aria-expanded'), 'true');
    await page.keyboard.press('ArrowDown');
    assert.equal(await focused(page.getByRole('menuitem', { name: 'Plain option', exact: true })), true);
    await page.keyboard.press('ArrowDown');
    assert.equal(await focused(checked), true);
    assert.equal(await page.getByRole('menuitem', { name: 'Disabled option', exact: true }).isDisabled(), true);
    await page.keyboard.press('Space');
    await page.getByRole('menu', { name: 'Options', exact: true }).waitFor({ state: 'hidden' });
    await trigger.click();
    assert.equal(await checked.getAttribute('aria-checked'), 'false');
    await page.keyboard.press('Escape');
    await page.getByRole('menu', { name: 'Options', exact: true }).waitFor({ state: 'hidden' });
    assert.equal(await focused(trigger), true);

    await openCard(page, 'anchored::Anchor');
    const popover = page.getByRole('button', { name: 'Show popover', exact: true });
    await popover.click();
    await page.getByRole('dialog', { name: 'Show popover', exact: true }).waitFor();
    await page.keyboard.press('Escape');
    await page.getByRole('dialog', { name: 'Show popover', exact: true }).waitFor({ state: 'hidden' });
    assert.equal(await focused(popover), true);
  } finally { await page.close(); }
});

test('filterable menus retain favourites, accept fixture colour edits and use no external navigation', async () => {
  const page = await browser.newPage();
  try {
    await openCard(page, 'menu::MenuAnchors');
    await page.getByRole('button', { name: 'Sample menu', exact: true }).click();
    await page.getByRole('textbox', { name: 'Filter the sample options', exact: true }).fill('glyph');
    assert.equal(await page.getByRole('menuitem', { name: 'Option with a glyph', exact: true }).count(), 1);
    assert.equal(await page.getByRole('menuitemcheckbox').count(), 0);

    await openCard(page, 'repositories::RepositoryPicker');
    await page.getByRole('button', { name: 'Repositories (0 favourite)', exact: true }).click();
    const repo = page.getByRole('menuitem', { name: 'example/fixtures, not a favourite', exact: true });
    await repo.click();
    assert.equal(await page.getByRole('button', { name: 'Repositories (1 favourite)', exact: true }).getAttribute('aria-expanded'), 'true');
    assert.equal(await page.getByRole('menuitem', { name: 'example/fixtures, favourite', exact: true }).count(), 1);
    const filter = page.getByRole('menuitemcheckbox', { name: 'Favourites only', exact: true });
    await filter.click();
    assert.equal(await filter.getAttribute('aria-checked'), 'true');
    await page.getByRole('textbox', { name: 'Search repositories on GitHub', exact: true }).fill('no-match');
    assert.equal(await page.getByText('No repositories found', { exact: true }).isVisible(), true);

    await openCard(page, 'anchored::color_select');
    await page.getByRole('button', { name: 'Fixture colour', exact: true }).click();
    const hex = page.getByRole('textbox', { name: 'Hex colour', exact: true });
    await hex.fill('#cf222e');
    await hex.press('Tab');
    assert.equal(await hex.inputValue(), '#cf222e');
    assert.equal(await page.getByRole('spinbutton', { name: 'Hue', exact: true }).inputValue(), '356');
    assert.ok(page.url().startsWith(origin));
  } finally { await page.close(); }
});

test('dialog examples stay inline, expose exact fixture decisions and keep keyboard access', async () => {
  const page = await browser.newPage();
  try {
    await openCard(page, 'dialog::frame');
    const frame = page.getByRole('dialog', { name: 'Save the changes?', exact: true });
    assert.equal(await frame.getAttribute('aria-modal'), null, 'native sample is an inline frame, not a modal');
    assert.equal(await frame.getByText('Fixture body text of a dashboard dialog.', { exact: true }).isVisible(), true);
    const cancel = frame.getByRole('button', { name: 'Cancel', exact: true });
    const save = frame.getByRole('button', { name: 'Save', exact: true });
    await cancel.focus();
    await page.keyboard.press('Tab');
    assert.equal(await focused(save), true);
    await page.keyboard.press('Enter');
    assert.equal(await frame.isVisible(), true, 'fixture callback does not save or close a real dialog');

    await openCard(page, 'dialog::footer');
    assert.equal(await page.getByRole('button', { name: 'Cancel', exact: true }).count(), 2);
    for (const name of ['Save', 'Delete', 'Keep editing', 'Discard']) {
      assert.equal(await page.getByRole('button', { name, exact: true }).isEnabled(), true);
    }
    await openCard(page, 'dialog::backdrop');
    assert.equal(await page.getByRole('dialog', { name: 'Dialog over a backdrop', exact: true }).isVisible(), true);
  } finally { await page.close(); }
});

test('sign-in and toast fixtures show local results without starting authentication', async () => {
  const page = await browser.newPage();
  try {
    await openCard(page, 'github_sign_in::status');
    const signIn = page.getByRole('button', { name: 'Sign in with GitHub', exact: true });
    assert.equal(await signIn.nth(0).isEnabled(), true);
    assert.equal(await signIn.nth(1).isDisabled(), true);
    assert.equal(await page.getByText('OCTO-1234', { exact: true }).isVisible(), true);
    const url = page.url();
    await page.getByRole('button', { name: 'Open GitHub', exact: true }).click();
    assert.equal(page.url(), url);
    assert.equal(await page.getByText('Fixture only: no GitHub sign-in starts.', { exact: true }).isVisible(), true);

    await openCard(page, 'Toast');
    await page.getByRole('button', { name: 'Show error toast', exact: true }).click();
    assert.equal(await page.getByRole('alert').textContent(), 'Sample error toast from the Component gallery.');
    await page.getByRole('button', { name: 'Dismiss notification', exact: true }).click();
    assert.equal(await page.getByRole('alert').count(), 0);
  } finally { await page.close(); }
});
