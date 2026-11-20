import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
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

test('bundled fonts ship with their upstream licences', () => {
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
  for (const name of [
    'README.md',
    'codicons-LICENSE.txt',
    'devicon-LICENSE',
    'devicons-vorillaz-LICENSE',
    'font-awesome-LICENSE.txt',
    'font-awesome-extension-LICENCE',
    'font-logos-LICENSE',
    'hack-LICENSE.md',
    'materialdesign-LICENSE',
    'octicons-LICENSE',
    'pomicons-LICENSE',
    'powerline-extra-LICENSE',
    'powerline-symbols-LICENSE.txt',
    'power-symbols-iec-LICENSE.txt',
    'seti-ui-LICENSE.md',
    'weather-icons-OFL.txt',
  ]) {
    const file = `fonts/glyph-licences/${name}`;
    assert.equal(read(file), readFileSync(join(root, 'public', file), 'utf8'), file);
  }
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
    vite: 'LICENSE.md',
  };
  for (const [name, file] of Object.entries(licences)) {
    const packagePath = join('node_modules', name);
    const { version, license } = lock.packages[`node_modules/${name}`];
    const licence = readFileSync(join(root, packagePath, file), 'utf8');
    const text = name === 'vite'
      ? licence.split('\n# Licenses of bundled dependencies\n')[0].split('\n').slice(3).join('\n').trim()
      : licence.trim();
    const heading = `## ${name} - ${version} (${license})`;
    assert.equal(notices.split('\n').filter((line) => line === heading).length, 1, name);
    assert.ok(notices.includes(`${heading}\n\n${text}\n`), name);
  }
});

test('rerunning the notice script preserves the generated notices', (t) => {
  const fixture = mkdtempSync(join(tmpdir(), 'gallery-notices-'));
  t.after(() => rmSync(fixture, { recursive: true, force: true }));
  mkdirSync(join(fixture, 'dist'));
  mkdirSync(join(fixture, 'scripts'));
  symlinkSync(join(root, 'node_modules'), join(fixture, 'node_modules'), 'junction');
  const script = 'scripts/append-css-notices.mjs';
  copyFileSync(join(root, script), join(fixture, script));
  const file = join(fixture, 'dist', 'THIRD_PARTY_NOTICES');
  copyFileSync(join(dist, 'THIRD_PARTY_NOTICES'), file);

  execFileSync(process.execPath, [join(fixture, script)]);
  assert.equal(readFileSync(file, 'utf8'), read('THIRD_PARTY_NOTICES'));
});
