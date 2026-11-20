import assert from 'node:assert/strict';
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const dist = join(root, 'dist');
const read = (file) => readFileSync(join(dist, file), 'utf8');
const walk = (dir) => readdirSync(dir).flatMap((name) => {
  const path = join(dir, name);
  return statSync(path).isDirectory() ? walk(path) : [path];
});

test('the built web Space has a permission-free manifest', () => {
  const manifest = JSON.parse(read('subspace-board.json'));
  assert.equal(manifest.name, 'Component gallery');
  assert.equal(manifest.version, '0.1.0');
  assert.deepEqual(manifest.permissions, []);
  assert.ok(statSync(join(dist, 'subspace-board.json')).size < 256 * 1024);
});

test('the built entry loads existing relative assets without inline code', () => {
  const html = read('index.html');
  assert.doesNotMatch(html, /<script(?![^>]*\bsrc=)|\son[a-z]+\s*=/i);
  const refs = [...html.matchAll(/(?:src|href)="([^"]+)"/g)].map((m) => m[1]);
  assert.ok(refs.some((ref) => ref.endsWith('.js')));
  assert.ok(refs.some((ref) => ref.endsWith('.css')));
  for (const ref of refs) {
    assert.ok(ref.startsWith('./'), `${ref} is relative`);
    assert.ok(existsSync(join(dist, ref)), `${ref} exists`);
  }
});

test('built scripts and styles need no network or dynamic code', () => {
  for (const path of walk(dist).filter((file) => /\.(css|js)$/.test(file))) {
    const source = readFileSync(path, 'utf8');
    assert.doesNotMatch(source, /\beval\s*\(|new Function\s*\(|sendBeacon|new WebSocket|\bfetch\s*\(/, path);
    assert.doesNotMatch(source, /(?:url\(|@import\s*)\s*["']?https?:/i, path);
  }
});

test('the six native references retain their full DPR2 viewport', () => {
  const manifest = JSON.parse(read('references/manifest.json'));
  assert.equal(manifest.width, 800);
  assert.equal(manifest.height, 600);
  assert.equal(manifest.scaleFactor, 2);
  assert.equal(manifest.cards.length, 6);
  for (const card of manifest.cards) {
    const png = readFileSync(join(dist, 'references', card.png));
    assert.equal(png.readUInt32BE(16), 1600);
    assert.equal(png.readUInt32BE(20), 1200);
  }
});

test('bundled fonts ship with upstream licences, not an invented patch licence', () => {
  for (const name of ['Inter-Regular.ttf', 'JetBrainsMonoNerdFont-Regular.woff2']) {
    assert.ok(statSync(join(dist, 'fonts', name)).size > 100_000);
    assert.ok(statSync(join(dist, 'fonts', name)).size <= 1024 * 1024);
  }
  for (const name of ['Inter-OFL.txt', 'JetBrainsMono-OFL.txt']) {
    assert.match(read(`fonts/${name}`), /SIL OPEN FONT LICENSE Version 1\.1/);
  }
  assert.match(read('fonts/NerdFonts-LICENSE'), /MIT License/);
  assert.match(read('fonts/NerdFonts-license-audit.md'), /Font Awesome/);
  assert.match(read('fonts/provenance.md'), /3\.5\.1/);
});

test('bundled dependencies ship the licence texts from their locked versions', () => {
  const lock = JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8'));
  const notices = read('THIRD_PARTY_NOTICES');
  const licences = {
    '@radix-ui/react-compose-refs': 'LICENSE',
    '@radix-ui/react-slot': 'LICENSE',
    'class-variance-authority': 'LICENSE',
    clsx: 'license',
    'lucide-react': 'LICENSE',
    react: 'LICENSE',
    'react-dom': 'LICENSE',
    scheduler: 'LICENSE',
    'tailwind-merge': 'LICENSE.md',
    tailwindcss: 'LICENSE',
  };
  for (const [name, file] of Object.entries(licences)) {
    const packagePath = join('node_modules', name);
    const { version, license } = lock.packages[`node_modules/${name}`];
    const text = readFileSync(join(root, packagePath, file), 'utf8').trim();
    assert.ok(notices.includes(`## ${name} - ${version} (${license})\n\n${text}\n`), name);
  }
});
