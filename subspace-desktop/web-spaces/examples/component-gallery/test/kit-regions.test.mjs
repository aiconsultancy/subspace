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
  server = await preview({ logLevel: 'silent', build: { outDir: process.env.KIT_REGIONS_DIST ?? 'dist' }, preview: { host: '127.0.0.1', port: 0 } });
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
    const context = await browser.newContext({ viewport: { width: 1000, height: 800 } });
    const page = await context.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    try {
      await page.goto(`${origin}/?theme=${theme}&card=${item.id}&capture=1&full=1`);
      await page.getByRole('heading', { name, exact: true }).waitFor();
      await check(page);
      assert.deepEqual(errors, [], `${theme}: no runtime errors`);
      assert.equal(await page.locator('canvas,img,iframe,object,embed').count(), 0, 'Fixture uses real DOM');
    } finally {
      await context.close();
    }
  }
}

async function focused(locator) {
  return locator.evaluate(element => element === document.activeElement);
}

test('Kit PopupMenu restores the actual keyboard or pointer opener on Escape', async () => {
  await fixture('gpui_kit::component::menu::PopupMenu', async page => {
    const target = page.getByRole('button', { name: 'Right-click here', exact: true });
    const actions = page.getByRole('button', { name: 'Actions', exact: true });
    for (const key of ['ContextMenu', 'Shift+F10']) {
      await target.focus();
      await page.keyboard.press(key);
      await page.getByRole('menuitem', { name: 'Rename…', exact: true }).waitFor();
      await page.keyboard.press('Escape');
      assert.equal(await focused(target), true, key);
    }
    await target.click({ button: 'right' });
    await page.getByRole('menuitem', { name: 'Rename…', exact: true }).waitFor();
    await page.keyboard.press('Escape');
    assert.equal(await focused(target), true, 'pointer context menu');
    await actions.click();
    await page.getByRole('menuitem', { name: 'Rename…', exact: true }).waitFor();
    await page.keyboard.press('Escape');
    assert.equal(await focused(actions), true, 'dropdown trigger');
  });
});

test('Kit Dialog traps Tab, cancels with Escape, restores trigger focus and commits only Rename', async () => {
  await fixture('gpui_kit::component::dialog::Dialog', async page => {
    const trigger = page.getByRole('button', { name: 'Open dialog…', exact: true });
    await trigger.focus();
    await page.keyboard.press('Enter');
    const dialog = page.getByRole('dialog', { name: 'Rename fixture', exact: true });
    await dialog.waitFor();
    const close = dialog.getByRole('button', { name: 'Close', exact: true });
    const rename = dialog.getByRole('button', { name: 'Rename', exact: true });
    await close.focus();
    await page.keyboard.press('Shift+Tab');
    assert.equal(await focused(rename), true, 'Reverse Tab wraps to final dialog action');
    await page.keyboard.press('Tab');
    assert.equal(await focused(close), true, 'Forward Tab wraps to first dialog action');
    await page.keyboard.press('Escape');
    assert.equal(await dialog.isVisible(), false);
    assert.equal(await focused(trigger), true, 'Escape returns focus to opener');
    assert.equal(await page.getByText('Fixture renamed.', { exact: true }).count(), 0);

    await page.keyboard.press('Enter');
    await dialog.getByRole('button', { name: 'Cancel', exact: true }).click();
    assert.equal(await dialog.isVisible(), false);
    assert.equal(await focused(trigger), true);
    assert.equal(await page.getByText('Fixture renamed.', { exact: true }).count(), 0);
    await trigger.click();
    await rename.click();
    assert.equal(await dialog.isVisible(), false);
    assert.equal(await focused(trigger), true);
    await page.getByText('Fixture renamed.', { exact: true }).waitFor();
    assert.equal(await page.getByText('Fixture renamed.', { exact: true }).isVisible(), true);
  });
});

test('Kit AlertDialog focuses safe Cancel and requires explicit destructive confirmation', async () => {
  await fixture('gpui_kit::component::dialog::AlertDialog', async page => {
    const trigger = page.getByRole('button', { name: 'Delete fixture…', exact: true });
    await trigger.click();
    const dialog = page.getByRole('alertdialog', { name: 'Delete “Sample fixture”?', exact: true });
    await dialog.waitFor();
    assert.equal(await focused(dialog.getByRole('button', { name: 'Cancel', exact: true })), true);
    await page.keyboard.press('Escape');
    assert.equal(await dialog.isVisible(), false);
    assert.equal(await focused(trigger), true);
    assert.equal(await page.getByText('Sample fixture deleted from this demonstration.', { exact: true }).count(), 0);
    await trigger.click();
    await dialog.getByRole('button', { name: 'Delete', exact: true }).click();
    assert.equal(await dialog.isVisible(), false);
    assert.equal(await focused(trigger), true);
    await page.getByText('Sample fixture deleted from this demonstration.', { exact: true }).waitFor();
    assert.equal(await page.getByText('Sample fixture deleted from this demonstration.', { exact: true }).isVisible(), true);
  });
});

test('Kit FocusTrap cycles both directions, retains input and permits Escape exit', async () => {
  await fixture('gpui_kit::component::FocusTrapElement', async page => {
    const first = page.getByRole('textbox', { name: 'First trapped field', exact: true });
    const second = page.getByRole('textbox', { name: 'Second trapped field', exact: true });
    await first.fill('First fixture');
    await page.keyboard.press('Tab');
    assert.equal(await focused(second), true);
    await page.keyboard.type('Second fixture');
    await page.keyboard.press('Tab');
    assert.equal(await focused(first), true);
    await page.keyboard.press('Shift+Tab');
    assert.equal(await focused(second), true);
    assert.equal(await first.inputValue(), 'First fixture');
    assert.equal(await second.inputValue(), 'Second fixture');
    await page.keyboard.press('Escape');
    assert.equal(await page.getByText('Focus released. Click a field to trap again.', { exact: true }).isVisible(), true);
    await page.keyboard.press('Tab');
    assert.equal(await focused(first), false, 'Escape breaks the loop');
    await first.click();
    await page.keyboard.press('Shift+Tab');
    assert.equal(await focused(second), true, 'Click reactivates trap');
  });
});

test('Kit ResizablePanelGroup supports keyboard changes and clamps width to native limits', async () => {
  await fixture('gpui_kit::component::resizable::ResizablePanelGroup', async page => {
    const separator = page.getByRole('separator', { name: 'Pane width', exact: true });
    const first = page.getByText('Drag the edge', { exact: true });
    const rest = page.getByText('The rest', { exact: true });
    const initialWidth = (await first.boundingBox()).width;
    const initialRest = (await rest.boundingBox()).width;
    await separator.focus();
    await page.keyboard.press('ArrowRight');
    assert.equal(await separator.getAttribute('aria-valuenow'), '150');
    assert.equal((await first.boundingBox()).width, initialWidth + 10);
    assert.equal((await rest.boundingBox()).width, initialRest - 10);
    await page.keyboard.press('Home');
    await page.keyboard.press('ArrowLeft');
    assert.equal(await separator.getAttribute('aria-valuenow'), '80');
    assert.equal((await first.boundingBox()).width, 80);
    await page.keyboard.press('End');
    await page.keyboard.press('ArrowRight');
    assert.equal(await separator.getAttribute('aria-valuenow'), '300');
    assert.equal((await first.boundingBox()).width, 300);
    assert.equal(await focused(separator), true);
  });
});

test('Kit DataTable sorts native fixture values numerically and reverses without losing rows', async () => {
  await fixture('gpui_kit::component::table::DataTable', async page => {
    const table = page.getByRole('table', { name: 'Sortable fixtures', exact: true });
    const names = () => table.locator('tbody tr td:first-child').allTextContents();
    assert.equal(await table.locator('tbody tr').count(), 8);
    assert.deepEqual(await names(), ['Sample fixture', 'Another fixture', 'Old fixture', 'New fixture', 'Large fixture', 'Small fixture', 'Shared fixture', 'Last fixture']);
    const count = table.getByRole('button', { name: /^Count/ });
    await count.focus();
    await page.keyboard.press('Enter');
    assert.equal(await table.getByRole('columnheader', { name: /^Count/ }).getAttribute('aria-sort'), 'ascending');
    assert.deepEqual(await table.locator('tbody tr td:last-child').allTextContents(), ['1', '3', '5', '7', '9', '12', '21', '40']);
    await page.keyboard.press('Enter');
    assert.equal(await table.getByRole('columnheader', { name: /^Count/ }).getAttribute('aria-sort'), 'descending');
    assert.deepEqual(await table.locator('tbody tr td:last-child').allTextContents(), ['40', '21', '12', '9', '7', '5', '3', '1']);
    await table.getByRole('button', { name: /^Name/ }).click();
    assert.deepEqual(await names(), ['Another fixture', 'Large fixture', 'Last fixture', 'New fixture', 'Old fixture', 'Sample fixture', 'Shared fixture', 'Small fixture']);
    assert.equal(await table.getByRole('columnheader', { name: /^Count/ }).getAttribute('aria-sort'), 'none');
  });
});

test('Kit searchable List filters fruit and supports native keyboard selection and empty state', async () => {
  await fixture('gpui_kit::component::list::List', async page => {
    const search = page.getByRole('textbox', { name: 'Search fruit', exact: true });
    const list = page.getByRole('listbox', { name: 'Fixture fruit', exact: true });
    assert.equal(await list.getByRole('option').count(), 6);
    await list.focus();
    await page.keyboard.press('End');
    assert.equal(await list.inputValue(), 'Fig');
    assert.equal(await page.getByText('Selected: Fig', { exact: true }).isVisible(), true);
    await search.fill('a');
    assert.deepEqual(await list.getByRole('option').allTextContents(), ['Apple', 'Banana', 'Date']);
    await list.focus();
    await page.keyboard.press('End');
    assert.equal(await list.inputValue(), 'Date');
    assert.equal(await page.getByText('Selected: Date', { exact: true }).isVisible(), true);
    await page.keyboard.press('ArrowUp');
    assert.equal(await list.inputValue(), 'Banana');
    assert.equal(await page.getByText('Selected: Banana', { exact: true }).isVisible(), true);
    await search.fill('no such fruit');
    assert.equal(await list.getByRole('option').count(), 0);
    assert.equal(await page.getByText('No fruit found.', { exact: true }).isVisible(), true);
    await search.fill('ch');
    assert.deepEqual(await list.getByRole('option').allTextContents(), ['Cherry']);
    await list.focus();
    await page.keyboard.press('Home');
    assert.equal(await page.getByText('Selected: Cherry', { exact: true }).isVisible(), true);
  });
});

test('Kit GridLayout resizes through keyboard and swaps named tiles without duplication', async () => {
  await fixture('gpui_kit::component::grid_layout::GridLayoutState', async page => {
    const revenue = page.getByRole('button', { name: 'Move Revenue', exact: true });
    const orders = page.getByRole('button', { name: 'Move Orders', exact: true });
    const resize = page.getByRole('button', { name: 'Resize Revenue; use arrow keys or drag', exact: true });
    const initial = await revenue.boundingBox();
    await resize.focus();
    await page.keyboard.press('ArrowLeft');
    assert.ok((await revenue.boundingBox()).width < initial.width);
    await page.keyboard.press('ArrowRight');
    assert.equal((await revenue.boundingBox()).width, initial.width);
    const tile = revenue.locator('..');
    const initialHeight = (await tile.boundingBox()).height;
    await page.keyboard.press('ArrowDown');
    assert.ok((await tile.boundingBox()).height > initialHeight);
    await page.keyboard.press('ArrowUp');
    assert.equal((await tile.boundingBox()).height, initialHeight);
    await revenue.focus();
    await page.keyboard.press('Alt+ArrowRight');
    assert.ok((await revenue.boundingBox()).x > (await orders.boundingBox()).x);
    assert.equal(await page.getByRole('button', { name: /^Move (Revenue|Orders|Notes)$/ }).count(), 3);
    await page.keyboard.press('Alt+ArrowLeft');
    assert.ok((await revenue.boundingBox()).x < (await orders.boundingBox()).x);
    assert.equal(await focused(revenue), true, 'Stable tile identity preserves keyboard focus');
  });
});

test('Kit Tree expands, selects and collapses through keyboard with parent focus restoration', async () => {
  await fixture('gpui_kit::component::tree::Tree', async page => {
    const tree = page.getByRole('tree', { name: 'Fixture files', exact: true });
    const src = tree.getByRole('treeitem', { name: 'src', exact: true });
    assert.equal(await src.getAttribute('aria-expanded'), 'true');
    await src.focus();
    await page.keyboard.press('ArrowRight');
    const lib = tree.getByRole('treeitem', { name: 'lib.rs', exact: true });
    assert.equal(await focused(lib), true);
    await page.keyboard.press('Enter');
    assert.equal(await lib.getAttribute('aria-selected'), 'true');
    await page.keyboard.press('ArrowLeft');
    assert.equal(await focused(src), true);
    await page.keyboard.press('ArrowLeft');
    assert.equal(await src.getAttribute('aria-expanded'), 'false');
    assert.equal(await lib.count(), 0);
    await page.keyboard.press('ArrowDown');
    const docs = tree.getByRole('treeitem', { name: 'docs', exact: true });
    assert.equal(await focused(docs), true);
    await page.keyboard.press('ArrowRight');
    assert.equal(await docs.getAttribute('aria-expanded'), 'true');
    await page.keyboard.press('ArrowRight');
    assert.equal(await focused(tree.getByRole('treeitem', { name: 'README.md', exact: true })), true);
  });
});

test('Kit Command filters, skips disabled actions and confirms keyboard choice', async () => {
  await fixture('gpui_kit::component::command::Command', async page => {
    const search = page.getByRole('combobox');
    await search.focus();
    await page.keyboard.press('End');
    await page.keyboard.press('Enter');
    assert.equal(await page.getByText('Copy a name selected', { exact: true }).isVisible(), true);
    await page.keyboard.press('Home');
    await page.keyboard.press('ArrowDown');
    const active = await search.getAttribute('aria-activedescendant');
    assert.equal(await page.locator(`[id="${active}"]`).innerText(), 'Open fixture');
    await page.keyboard.press('ArrowDown');
    assert.equal(await page.getByRole('option', { name: 'Copy a name', exact: true }).getAttribute('aria-selected'), 'true');
    assert.equal(await page.getByRole('option', { name: 'Delete fixture', exact: true }).isDisabled(), true);
    await search.fill('open');
    assert.equal(await page.getByRole('option').count(), 1);
    await page.keyboard.press('Enter');
    assert.equal(await page.getByText('Open fixture selected', { exact: true }).isVisible(), true);
    await search.fill('no such command');
    assert.equal(await page.getByRole('option').count(), 0);
    assert.equal(await page.getByText('No commands found.', { exact: true }).isVisible(), true);
  });
});
