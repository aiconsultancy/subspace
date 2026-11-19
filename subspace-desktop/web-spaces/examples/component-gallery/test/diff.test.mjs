import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { compareImages } from '../scripts/diff.mjs';

function image(width = 3, height = 3) {
  return { width, height, data: Buffer.alloc(width * height * 4, 255) };
}

test('identical renderer pixels pass with no changed pixels', () => {
  const result = compareImages(image(), image());
  assert.equal(result.pass, true);
  assert.equal(result.changedPixels, 0);
});

test('one solid colour difference fails, with zero tolerance', () => {
  const reference = image();
  const actual = image();
  actual.data.set([0, 0, 0, 255], 16);
  const result = compareImages(reference, actual);
  assert.equal(result.pass, false);
  assert.equal(result.changedPixels, 1);
  assert.equal(result.exactChangedPixels, 1);
});

test('different raster dimensions fail instead of cropping or resizing', () => {
  const result = compareImages(image(), image(4, 3));
  assert.equal(result.pass, false);
  assert.match(result.error, /dimensions/);
});

test('missing reference cases produce a failing report and nonzero CLI exit', async () => {
  const folder = await mkdtemp(join(tmpdir(), 'gallery-diff-test-'));
  try {
    const result = spawnSync(process.execPath, [
      new URL('../scripts/compare.mjs', import.meta.url).pathname,
      '--reference', join(folder, 'missing-reference'),
      '--actual', join(folder, 'missing-web'),
      '--output', join(folder, 'report'),
    ]);
    assert.equal(result.status, 1, result.stderr.toString());
    const report = JSON.parse(await readFile(join(folder, 'report/report.json'), 'utf8'));
    assert.equal(report.pass, false);
    assert.equal(report.cases.length, 6);
    assert.ok(report.cases.every(row => !row.pass && row.error.includes('ENOENT')));
  } finally {
    await rm(folder, { recursive: true });
  }
});

test('comparison refuses existing output rather than mixing stale evidence', async () => {
  const folder = await mkdtemp(join(tmpdir(), 'gallery-existing-report-'));
  try {
    await writeFile(join(folder, 'report.json'), 'previous evidence\n');
    const result = spawnSync(process.execPath, [
      new URL('../scripts/compare.mjs', import.meta.url).pathname,
      '--output', folder,
    ]);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr.toString(), /EEXIST/);
    assert.equal(await readFile(join(folder, 'report.json'), 'utf8'), 'previous evidence\n');
  } finally {
    await rm(folder, { recursive: true });
  }
});
