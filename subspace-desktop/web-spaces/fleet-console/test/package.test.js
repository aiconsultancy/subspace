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
  .filter((path) => !path.startsWith("test/") && !path.endsWith(".md") && path !== "mise.toml");
const scripts = shipped.filter((file) => file.endsWith(".js"));

test("the manifest declares the settings and permissions Fleet Console uses", () => {
  const manifest = JSON.parse(read("subspace-board.json"));
  assert.ok(manifest.name.length > 0 && manifest.name.length <= 60);
  assert.ok(manifest.version.length > 0 && manifest.version.length <= 40);
  assert.deepEqual(manifest.permissions, ["settings", "secrets", "issues", "badge", "context", "notifications"]);
  const byKey = Object.fromEntries(manifest.settings.map((setting) => [setting.key, setting]));
  assert.equal(byKey.endpoint.type, "url");
  assert.equal(byKey.token.type, "secret");
  assert.equal("default" in byKey.token, false, "a secret has no default");
  assert.deepEqual(byKey.motion.options, ["Full", "Reduced"]);
  assert.equal(byKey.motion.default, "Full");
  assert.equal(byKey.perfReadout.type, "select");
  assert.deepEqual(byKey.perfReadout.options, ["Off", "On"]);
  assert.equal(byKey.perfReadout.default, "Off");
  assert.match(byKey.perfReadout.help, /Shift\+P/);
  for (const setting of manifest.settings) {
    assert.match(setting.key, /^[A-Za-z][A-Za-z0-9_-]{0,39}$/);
    assert.ok(setting.label.length <= 60 && (setting.help ?? "").length <= 200, setting.key);
  }
  assert.ok(statSync(join(root, "subspace-board.json")).size <= 256 * 1024);
});

test("index.html loads only local files that exist", () => {
  const html = read("index.html");
  const references = [...html.matchAll(/(?:href|src)="([^"#]+)"/g)].map(([, value]) => value);
  assert.ok(references.includes("styles.css") && references.includes("src/main.js"));
  for (const reference of references) {
    assert.doesNotMatch(reference, /^[a-z]+:/i, "no remote or data references");
    assert.ok(existsSync(join(root, reference)), `${reference} exists`);
  }
});

test("the map takes focus and tells keyboard users about Shift+P", () => {
  const html = read("index.html");
  const stage = html.match(/<div class="stage"[^>]*>/)[0];
  assert.match(stage, /tabindex="0"/);
  const described = stage.match(/aria-describedby="([^"]+)"/)[1];
  assert.match(html, new RegExp(`id="${described}">[^<]*Shift\\+P`));
});

test("every relative import resolves to a shipped file", () => {
  for (const path of scripts) {
    for (const [, target] of read(path).matchAll(/from\s+"(\.[^"]+)"/g)) {
      const resolved = relative(root, join(root, dirname(path), target));
      assert.ok(shipped.includes(resolved), `${path} imports ${target}`);
    }
  }
});

test("no inline scripts, handlers, eval, external hosts or remote fonts", () => {
  const html = read("index.html");
  assert.doesNotMatch(html, /<script(?![^>]*\bsrc=)/i, "inline script block");
  assert.doesNotMatch(html, /\son[a-z]+\s*=/i, "inline event handler");
  for (const path of scripts) {
    assert.doesNotMatch(read(path), /\beval\s*\(|new Function\s*\(|document\.write|sendBeacon|XMLHttpRequest|new WebSocket|new EventSource|importScripts/, path);
  }
  const css = read("styles.css");
  assert.doesNotMatch(css, /@import|url\(\s*["']?https?:|@font-face/i);
  const allowedHosts = new Set(["www.w3.org", "linear.app", "github.com"]);
  for (const path of shipped.filter((file) => /\.(js|css|html|svg|json)$/.test(file))) {
    // Backslashes come from regular-expression sources such as linear\\.app.
    for (const [url] of read(path).replace(/\\+/g, "").matchAll(/https?:\/\/[^\s"'`)<>]+/g)) {
      let host;
      try {
        host = new URL(url.replace(/\$\{.*$/, "x")).hostname;
      } catch {
        continue; // placeholders such as http://localhost:&lt;port&gt; in help text
      }
      // Sample fleets use reserved example.com names.
      assert.ok(allowedHosts.has(host) || host === "localhost" || host === "x" || host.endsWith(".example.com"), `${path} references ${host}`);
    }
  }
});

test("only the API client calls fetch, and only chrome.js writes innerHTML", () => {
  for (const path of scripts) {
    const source = read(path);
    assert.equal(/\bfetch\s*\(|fetchImpl\s*\(/.test(source), path === "src/api.js", `${path} fetch use`);
    const writes = source.match(/\.innerHTML\s*=/g) ?? [];
    assert.equal(writes.length > 0, path === "src/chrome.js", `${path} innerHTML writes`);
  }
});

test("no internal names, real hosts or secrets in shipped files", () => {
  for (const path of shipped) {
    const source = read(path);
    assert.doesNotMatch(source, /[a-z0-9-]+\.ts\.net\b|BEGIN [A-Z]+ PRIVATE KEY|\b(?:ghp|gho|sk)_[A-Za-z0-9]{20,}/, path);
  }
});

test("the canvas has an accessible name, and the page has a live region and a skip link", () => {
  const html = read("index.html");
  assert.match(html, /<canvas id="scene" role="img" aria-label="[^"]+"/);
  assert.match(html, /aria-live="polite"/);
  assert.match(html, /class="skip-link" href="#inspector"/);
  assert.match(html, /<nav class="depth" id="depth" aria-label="Tiers">/);
});
