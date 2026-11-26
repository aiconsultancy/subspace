import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { runInNewContext } from "node:vm";

const script = readFileSync(new URL("../src/agentsview/auth.js", import.meta.url), "utf8");
const origin = "https://fleet.example.com:8443";
const apiPath = "/view/api/";
const settings = apiPath + "v1/settings";

function start({ page = origin, status = 200, key = "example-key" } = {}) {
  const calls = [];
  const events = [];
  const nativeFetch = async (input, options) => {
    calls.push([input, options]);
    return new Response("{}", { status, headers: { "Content-Type": "application/json" } });
  };
  const window = { location: { origin: page }, fetch: nativeFetch, ipc: { postMessage: event => events.push(event) } };
  class PageRequest extends Request {
    constructor(input, options) {
      super(typeof input === "string" ? new URL(input, page) : input, options);
    }
  }
  runInNewContext(script.replace("/*__SUBSPACE_AGENTSVIEW_CONFIG__*/", JSON.stringify({ origin, apiPath, key })),
    { window, document: { baseURI: page }, URL, Request: PageRequest });
  return { fetch: window.fetch, events, calls };
}

test("API requests get the saved key and never follow redirects", async () => {
  const view = start();
  await view.fetch(settings, { headers: { Accept: "application/json" } });
  await view.fetch(apiPath + "v1/events");
  assert.deepEqual(view.events, ["ready"]);
  for (const [request, options] of view.calls) {
    assert.equal(request.headers.get("Authorization"), "Bearer example-key");
    assert.equal(options.redirect, "error");
  }
  assert.equal(view.calls[0][0].headers.get("Accept"), "application/json");
});

test("the key is confined to the configured origin, port and API prefix", async () => {
  const view = start();
  for (const url of ["/api/v1/settings", "/view/assets/main.js", "https://elsewhere.example.com" + settings,
    "https://fleet.example.com" + settings]) {
    await view.fetch(url);
    assert.deepEqual(view.calls.at(-1), [url, undefined]);
  }
  const elsewhere = start({ page: "https://elsewhere.example.com" });
  await elsewhere.fetch(settings);
  assert.deepEqual(elsewhere.calls, [[settings, undefined]]);
});

test("a rejected key is reported without returning the upstream key prompt's 401", async () => {
  const view = start({ status: 401 });
  await assert.rejects(view.fetch(settings), /authentication failed/);
  assert.deepEqual(view.events, ["rejected"]);
});

test("an authenticated SSH tunnel can connect without a key", async () => {
  const view = start({ key: null });
  await view.fetch(settings);
  assert.deepEqual(view.events, ["ready"]);
  assert.equal(view.calls[0][0].headers.get("Authorization"), null);
});
