import { readFileSync, writeFileSync } from 'node:fs';

const notices = new URL('../dist/THIRD_PARTY_NOTICES', import.meta.url);
const bundled = readFileSync(notices, 'utf8');
let complete = bundled;

// Vite's collector misses inlined Tailwind CSS and its own virtual preload helper.
for (const [packageName, licenceFile] of [['tailwindcss', 'LICENSE'], ['vite', 'LICENSE.md']]) {
  const directory = new URL(`../node_modules/${packageName}/`, import.meta.url);
  const { name, version, license } = JSON.parse(readFileSync(new URL('package.json', directory), 'utf8'));
  const heading = `## ${name} - ${version} (${license})`;
  if (complete.split('\n').includes(heading)) continue;

  const licence = readFileSync(new URL(licenceFile, directory), 'utf8');
  let text = licence.trim();
  if (name === 'vite') {
    const core = /^# Vite core license\n[^\n]+\n\n([\s\S]+?)\n\n# Licenses of bundled dependencies\n/.exec(licence);
    if (!core) throw new Error(`Cannot locate the Vite core licence in ${licenceFile}`);
    text = core[1];
  }
  complete += `\n${heading}\n\n${text}\n`;
}

if (complete !== bundled) writeFileSync(notices, complete);
