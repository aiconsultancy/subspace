import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';

const root = new URL('../', import.meta.url);

test('web inventory maps exactly the native 180-card planning inventory', () => {
  const inventory = JSON.parse(readFileSync(new URL('src/inventory.json', root), 'utf8'));
  const appendix = readFileSync(new URL('../../../docs/changes/web-component-gallery/inventory-and-themes.md', root), 'utf8');
  const names = [...appendix.split('## Interpretation limits')[0].matchAll(/^\| `([^`]+)` \| `[^`]+` \| `[^`]+` \|/gm)].map(match => match[1]);
  assert.equal(names.length, 180);
  assert.equal(inventory.length, 180);
  assert.deepEqual(inventory.map(card => card.name), names);
  assert.equal(new Set(inventory.map(card => card.id)).size, 180);
  assert.equal(new Set(inventory.map(card => card.section)).size, 16);
  assert.equal(inventory.filter(card => card.library === 'Subspace').length, 81);
  assert.equal(inventory.filter(card => card.library === 'Shell').length, 15);
  assert.equal(inventory.filter(card => card.library === 'Kit').length, 84);
  for (const card of inventory) {
    assert.match(card.id, /^[a-z0-9-]+$/);
    assert.ok(card.states.length > 0);
    assert.ok(card.source.length > 0);
    assert.doesNotMatch(JSON.stringify(card), /\/home\/|file:\/\//);
  }
  assert.equal(inventory.find(card => card.name === 'action::action_button').id, 'action-button');
  assert.equal(inventory.find(card => card.name === 'fields::input').id, 'input');
  assert.equal(inventory.find(card => card.name === 'feedback::error_box').id, 'error-box');
});
