import { mkdir, readFile, writeFile, copyFile } from 'node:fs/promises';
import { resolve, join, dirname } from 'node:path';
import { PNG } from 'pngjs';
import { cases, option } from './cases.mjs';
import { compareImages } from './diff.mjs';

const reference = resolve(option('--reference', 'public/references'));
const actual = resolve(option('--actual', 'artifacts/web'));
const output = resolve(option('--output', 'artifacts/comparison'));
await mkdir(dirname(output), { recursive: true });
await mkdir(output); // Refuse stale evidence, like capture.mjs.
const records = [];
for (const { id, theme, card } of cases) {
  try {
    const native = PNG.sync.read(await readFile(join(reference, `${id}.png`)));
    const web = PNG.sync.read(await readFile(join(actual, `${id}.png`)));
    const { difference, ...result } = compareImages(native, web);
    const files = { reference: `${id}-reference.png`, actual: `${id}-react.png` };
    await copyFile(join(reference, `${id}.png`), join(output, files.reference));
    await copyFile(join(actual, `${id}.png`), join(output, files.actual));
    if (difference) {
      files.diff = `${id}-diff.png`;
      files.overlay = `${id}-overlay.png`;
      await writeFile(join(output, files.diff), PNG.sync.write({
        width: native.width, height: native.height, data: difference,
      }));
      const overlay = Buffer.from(native.data);
      for (let i = 0; i < overlay.length; i += 4) {
        for (let channel = 0; channel < 3; channel++) {
          overlay[i + channel] = Math.round((native.data[i + channel] + web.data[i + channel]) / 2);
        }
        overlay[i + 3] = 255;
      }
      await writeFile(join(output, files.overlay), PNG.sync.write({
        width: native.width, height: native.height, data: overlay,
      }));
    }
    records.push({ id, theme, card, width: native.width, height: native.height, ...result, files });
  } catch (error) {
    records.push({ id, theme, card, pass: false, error: error.message });
  }
}
const report = {
  threshold: 0, includeAA: false, maxDiffPixels: 0,
  pass: records.every(record => record.pass),
  cases: records,
};
await writeFile(join(output, 'report.json'), JSON.stringify(report, null, 2) + '\n');
await writeFile(join(output, 'index.html'), html(report) + '\n');
console.log(`${records.filter(record => record.pass).length}/${records.length} cases pass; ${output}/index.html`);
if (!report.pass) process.exitCode = 1;

function escape(value) {
  return String(value).replace(/[&<>"']/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[ch]));
}

function html(report) {
  const rows = report.cases.map(row => `<section><h2>${escape(row.id)}: ${row.pass ? 'PASS' : 'FAIL'}</h2>
    <p>${escape(row.error ?? `${row.changedPixels} non-edge pixels differ (${row.changedPercent.toFixed(4)}%); ${row.exactChangedPixels} exact differences`)}</p>
    <div>${Object.entries(row.files ?? {}).map(([kind, file]) => `<figure><figcaption>${escape(kind)}</figcaption><a href="${escape(file)}"><img src="${escape(file)}" alt="${escape(row.id)} ${escape(kind)}"></a></figure>`).join('')}</div></section>`).join('');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>GPUI · React comparison</title>
    <style>body{background:#20201f;color:#f0efed;font:14px system-ui;margin:24px}section{border-top:1px solid #555;padding:16px 0}section div{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:12px}figure{margin:0}img{width:100%;height:auto}a{color:inherit}</style>
    <h1>GPUI reference / React comparison: ${report.pass ? 'PASS' : 'FAIL'}</h1><p>Zero non-antialiased differences allowed. Pixel threshold 0. No masks, resize or colour tolerance.</p>${rows}</html>`;
}
