import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(join(root, path), "utf8");

function filesUnder(directory) {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name);
    return statSync(path).isDirectory() ? filesUnder(path) : [path];
  });
}

const shipped = filesUnder(root)
  .map((path) => relative(root, path))
  .filter((path) => !path.startsWith("test/") && !path.endsWith(".md"));

test("the manifest follows the web Space contract and requests no permissions", () => {
  const manifest = JSON.parse(read("subspace-board.json"));
  assert.equal(typeof manifest.name, "string");
  assert.ok(manifest.name.length > 0 && manifest.name.length <= 60);
  assert.equal(typeof manifest.version, "string");
  assert.ok(manifest.version.length > 0 && manifest.version.length <= 40);
  assert.deepEqual(manifest.permissions, []);
  assert.equal(manifest.settings, undefined);
  assert.ok(statSync(join(root, "subspace-board.json")).size <= 256 * 1024);
});

test("index.html loads only local files that exist", () => {
  const html = read("index.html");
  const references = [...html.matchAll(/(?:href|src)="([^"#]+)"/g)]
    .map(([, value]) => value)
    .filter((value) => !/^https:\/\//.test(value));
  assert.ok(references.includes("styles.css") && references.includes("src/main.js"));
  for (const reference of references) {
    assert.ok(existsSync(join(root, reference)), `${reference} exists`);
  }
});

test("the page needs no inline script, build step or dependency", () => {
  const html = read("index.html");
  assert.doesNotMatch(html, /<script(?![^>]*\bsrc=)/i, "inline script block");
  assert.doesNotMatch(html, /\son[a-z]+\s*=/i, "inline event handler");
  assert.equal(existsSync(join(root, "package.json")), false);
  assert.equal(existsSync(join(root, "node_modules")), false);
});

test("shipped files make no remote requests except to the two providers", () => {
  const allowedFetchHosts = new Set(["api.postcodes.io", "api.open-meteo.com"]);
  // Documentation links in the footer are plain anchors, never loaded resources.
  const anchorHosts = new Set(["open-meteo.com", "creativecommons.org", "postcodes.io"]);
  for (const path of shipped) {
    const source = read(path);
    for (const [url] of source.matchAll(/https?:\/\/[^\s"'`)<>]+/g)) {
      const host = new URL(url).hostname;
      if (host === "www.w3.org") continue; // SVG namespace
      const allowed = allowedFetchHosts.has(host) || (path === "index.html" && anchorHosts.has(host));
      assert.ok(allowed, `${path} references ${host}`);
    }
    if (path.endsWith(".css")) {
      assert.doesNotMatch(source, /@import|url\(\s*["']?https?:/i, `${path} has no remote CSS`);
      assert.doesNotMatch(source, /@font-face/, `${path} loads no fonts`);
    }
  }
});

test("no analytics, eval or dynamic code", () => {
  for (const path of shipped.filter((file) => file.endsWith(".js"))) {
    const source = read(path);
    assert.doesNotMatch(source, /\beval\s*\(|new Function\s*\(|document\.write|sendBeacon|XMLHttpRequest|WebSocket/, path);
  }
});

test("only the API client and its tests call fetch", () => {
  for (const path of shipped.filter((file) => file.endsWith(".js"))) {
    const calls = /\bfetch\s*\(/.test(read(path));
    assert.equal(calls, path === "src/api.js", `${path} fetch use`);
  }
});

test("API-derived text is never written with innerHTML", () => {
  for (const path of shipped.filter((file) => file.endsWith(".js"))) {
    const uses = read(path).match(/innerHTML/g) ?? [];
    const allowed = { "src/ui.js": 1, "src/scene.js": 1 }[path] ?? 0;
    assert.equal(uses.length, allowed, `${path} innerHTML uses`);
  }
});
