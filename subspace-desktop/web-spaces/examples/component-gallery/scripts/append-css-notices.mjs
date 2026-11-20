import { readFileSync, writeFileSync } from 'node:fs';

// Tailwind inlines its CSS before Vite gathers licences from the JavaScript chunks.
const tailwind = new URL('../node_modules/tailwindcss/', import.meta.url);
const { name, version, license } = JSON.parse(readFileSync(new URL('package.json', tailwind), 'utf8'));
const text = readFileSync(new URL('LICENSE', tailwind), 'utf8').trim();
const notices = new URL('../dist/THIRD_PARTY_NOTICES', import.meta.url);
const bundled = readFileSync(notices, 'utf8');
writeFileSync(notices, `${bundled}\n## ${name} - ${version} (${license})\n\n${text}\n`);
