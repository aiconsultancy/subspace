# Build and package a web Space

This guide walks through making a web Space from an empty folder, loading it
in Subspace, using the bridge, testing it and packaging it for others. The
[Weather example](../web-spaces/examples/weather/README.md) is the worked
example throughout. Every field, limit and message is listed in the
[web Space reference](web-spaces-reference.md); this guide links there rather
than repeating it. Agents can follow the same steps with the `web-space`
skill in [.agents/skills/web-space](../.agents/skills/web-space/SKILL.md).

A web Space is a built web app (HTML, CSS, JavaScript and assets) with a
`subspace-board.json` manifest beside its `index.html`. The Web Space shows
it in a native web view on macOS and Windows. Subspace serves the files
itself: no Node.js process, HTTP server or build step runs in the app. The
manifest keeps the older name `subspace-board.json` for compatibility, as do
the `webBoards` configuration key and labels such as **Add board…**.

## 1. Create the folder

```text
hello/
  subspace-board.json   required: the manifest
  index.html            required: the start page
  main.js               your script
  styles.css            optional
  test/                 tests, left out of the package
```

Weather has the same shape: `subspace-board.json`, `index.html`,
`styles.css`, `src/*.js`, `assets/icon.svg` and `test/*.test.js`.

If you use a bundler such as Vite, the web Space folder is its build output
(for example `dist/`), and the manifest goes into that output, for example
from Vite's `public/` folder. Subspace never builds anything.

Use portable ASCII file names: letters, digits, space and `-_.@()+,[]{}`.
Subspace rejects other names, symbolic links and names that differ only by
case. See [Package rules](web-spaces-reference.md#package-rules).

## 2. Write the manifest

`subspace-board.json` needs only `name` (1 to 60 characters) and `version`
(1 to 40 characters):

```json
{
  "name": "Hello",
  "version": "0.1.0"
}
```

Weather's manifest is this plus `"permissions": []`. Add permissions and
settings only when the app uses them; [step 5](#5-use-settings-and-the-bridge)
does.

## 3. Write the page and script

Load scripts from files. The Content Security Policy blocks inline
`<script>` blocks, inline event handlers (`onclick="…"`), `eval` and scripts
from other hosts. Inline styles are allowed.

`index.html`:

```html
<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Hello</title>
  <script type="module" src="main.js"></script>
</head>
<body>
  <p id="output">Loading…</p>
</body>
</html>
```

`main.js`:

```javascript
const output = document.getElementById("output");
output.textContent = "Hello from a web Space";
```

Use relative paths (`main.js`, `src/main.js`) or root-relative paths
(`/main.js`); both load from the folder. Network requests may go to HTTPS
hosts or loopback HTTP, and the host must allow the app's origin. Weather
calls two HTTPS APIs from `src/api.js` this way.

## 4. Load it in Subspace

1. Open **Settings > Application > Web Spaces** and choose **Add board…**.
2. Enter a **Board ID** (a lower-case letter, then letters, digits or
   hyphens, up to 40 characters, not the ID of a built-in Space) and a
   **Name**.
3. Set **Source type** to **Local folder** and choose the `hello` folder with
   **Choose…**.
4. Choose **Validate source**. It shows `Ready to install: Hello` and the
   requested permissions.
5. Choose **Install now**. The web Space appears in the switcher at once.

Subspace draws one toolbar above your page. It has **Back**, **Forward**,
**Reload page** and **Go to the start page**, then the web Space's name and
`Local web app`. Your page gets the whole area below the toolbar.

An installed folder is a private snapshot. After you change the files, choose
**Edit… > Replace source…** and validate and install again.

For quicker iteration, configure a live folder by hand in the configuration
file (see [Settings and stored state](../README.md#settings-and-stored-state)).
Subspace then serves the folder directly, and **Reload page** in that toolbar
shows your changes:

```json
{
  "webBoards": {
    "hello": { "title": "Hello", "path": "~/code/hello" }
  }
}
```

You can also open the folder in a normal browser through a static server, for
example `python3 -m http.server --bind 127.0.0.1 8765`. There is no
`window.subspace` there, so feature-detect it as below.

## 5. Use settings and the bridge

A folder web Space with a manifest gets a frozen `window.subspace` object. Ask
for what you need in the manifest:

```json
{
  "name": "Hello",
  "version": "0.2.0",
  "permissions": ["settings", "badge", "notifications"],
  "settings": [
    { "key": "greeting", "label": "Greeting", "type": "text", "default": "Hello",
      "help": "Shown at the top of the page" }
  ]
}
```

Then read the settings and call Subspace:

```javascript
const output = document.getElementById("output");
const subspace = globalThis.subspace;

async function start() {
  if (!subspace) {
    output.textContent = "Hello from a browser";
    return;
  }
  const info = await subspace.info();
  let greeting = "Hello";
  if (info.permissions.includes("settings")) {
    const settings = await subspace.settings.get();
    greeting = settings.greeting ?? greeting;
  }
  output.textContent = `${greeting} from Subspace`;
  if (info.permissions.includes("badge")) {
    await subspace.setBadge("1");
  }
}

start().catch((error) => {
  output.textContent = error.message;
});
```

`info().permissions` lists only the permissions the user has allowed. Every
permission starts off: after installing, the user opens **Settings > Spaces >
Hello**, ticks **Allow this app to read its settings** (and the others) and
saves. The `greeting` field appears on the same page. A call without the
permission rejects with `Permission denied. Allow this action in the board's
Subspace settings.` Edits apply on Save; read `settings.get()` again to see
them, as there is no change event.

Setting types are `text`, `url`, `number`, `boolean`, `select` and `secret`.
`settings.get()` never returns secret values. An app that needs its own
token asks for the `secrets` permission and reads it with `secrets.get()`
([Read and save the app's secrets](web-spaces-reference.md#read-and-save-the-apps-secrets)).
The full method list and limits are in
[Read settings and call Subspace](web-spaces-reference.md#read-settings-and-call-subspace).

To match Subspace's look, read `subspace.theme()` at start, which needs no
permission, and listen for its changes:

```javascript
function applyTheme(theme) {
  const root = document.documentElement;
  root.style.colorScheme = theme.mode;
  for (const [name, value] of Object.entries(theme.tones)) {
    root.style.setProperty(`--${name}`, value);
  }
  document.body.style.fontFamily = theme.fonts.ui;
}

async function followTheme() {
  const subspace = window.subspace;
  if (typeof subspace?.theme !== "function") return;
  window.addEventListener("subspace:theme", (event) => applyTheme(event.detail));
  applyTheme(await subspace.theme());
}
```

See [Follow the app's theme](web-spaces-reference.md#follow-the-apps-theme).
An app that shows issue references can ask the Issues Space about them with
the `issues` permission, as Channels does
([Show issues](web-spaces-reference.md#show-issues)).

Changing the manifest changes its fingerprint, so a new manifest needs the
permissions allowed again ([Trust and compatibility](web-spaces-reference.md#trust-and-compatibility)).

## 6. Send notifications

Request `notifications`, then use either the bridge or the standard
`Notification` API, which Subspace adapts:

```javascript
await subspace.notify({ title: "Hello", body: "Something happened" });

// or
if (await Notification.requestPermission() === "granted") {
  new Notification("Hello", { body: "Something happened" });
}
```

`requestPermission()` returns `"granted"` only when the user has allowed
**show notifications** and a notification channel is on for the Space; it
never shows a prompt. Titles are up to 100 characters, bodies up to 500, and
Subspace accepts at most one notification every five seconds
(`Too many notifications. Wait five seconds.`). See
[Page Notification adapter](web-spaces-reference.md#page-notification-adapter).

## 7. Test it

Use Node's built-in test runner, as Weather does. Keep logic in plain modules
that do not touch the DOM, and pass the bridge in so tests can use a fake:

```javascript
// greeting.js
export async function greeting(subspace) {
  if (!subspace) return "Hello from a browser";
  const info = await subspace.info();
  if (!info.permissions.includes("settings")) return "Hello from Subspace";
  const settings = await subspace.settings.get();
  return `${settings.greeting ?? "Hello"} from Subspace`;
}
```

```javascript
// test/greeting.test.js
import assert from "node:assert/strict";
import { test } from "node:test";
import { greeting } from "../greeting.js";

test("uses the greeting setting when allowed", async () => {
  const subspace = {
    info: async () => ({ version: 1, permissions: ["settings"] }),
    settings: { get: async () => ({ greeting: "Hi" }) },
  };
  assert.equal(await greeting(subspace), "Hi from Subspace");
});
```

Run `node --test test/*.test.js`. Weather pins Node in its own `mise.toml`
(`mise run test` in its folder), and the repository runs it with
`mise run weather-space-test`, part of `make check`. Weather's
`test/package.test.js` also checks the package: manifest limits, that
`index.html` loads only local files that exist, no inline scripts and no
unexpected remote hosts. Copy those checks for your own web Space.

## 8. Package it

A package is the web Space folder: `subspace-board.json` and `index.html` at
its root, plus everything they load. It must not contain tests,
`node_modules`, source maps you do not want to publish, `.git`, secrets or
dotfiles. Subspace never runs a build, so ship built files only.

Package with the repository script. It applies the
[package rules](web-spaces-reference.md#package-rules) and the manifest rules
exactly as the app does, with the same messages, and writes a reproducible
ZIP: files in sorted order, fixed timestamps (1980-01-01) and fixed
permissions, so the same files always give the same bytes.

```sh
python3 scripts/package-web-space.py web-spaces/examples/weather
# or
mise run package-web-space -- web-spaces/examples/weather
# or
make package-web-space SPACE=web-spaces/examples/weather
```

For Weather it prints something like this (the size and hash change with its
files):

```text
web Space: Weather 1.0.0
permissions: none; settings: 0
files: 20, 95104 bytes
sha256: f88055cdc03f499f53fdfba98cbb379adc8bd17677f5b6469062da0b893834e1
wrote …/subspace-desktop/dist/web-spaces/weather-1.0.0.zip
```

- The second argument names the ZIP; the default is
  `dist/web-spaces/<folder>-<version>.zip`. The ZIP must be outside the
  folder. The mise task and make target run from `subspace-desktop/`, so
  give them paths relative to it or absolute paths.
- `--check` validates without writing.
- By default it leaves out dotfiles and dot folders, `test/`, `tests/`,
  `__tests__/`, `node_modules/`, `mise.toml`, `package.json`, lock files,
  `tsconfig.json` and `*.test.*` and `*.spec.*` files. `--include PATTERN`
  keeps a path that would be left out (for example `--include 'test/*'`), and
  `--exclude PATTERN` leaves out another (for example `--exclude drafts`).
  Patterns match the folder-relative path.
- Warnings, such as an unknown permission, are printed but do not stop the
  package.

Limits: at most 10,000 entries, 32 MiB per file, 256 MiB in total, a 64 MiB
ZIP, paths up to 1,024 bytes and 32 levels, and a 256 KiB manifest.

**Version.** `version` is free text shown to users when they validate or
update. Use semantic versioning (`1.2.0`) and bump it with every published
change. Subspace does not compare versions: GitHub updates follow the branch
commit or the release date (see [Updates](web-spaces-reference.md#updates)).

**Icon.** The manifest has no icon field. Every web Space uses the Web
Space's globe icon in the switcher. A `<link rel="icon">` in `index.html`, as
Weather has, is used only when the page is opened in a browser.

## 9. Publish and install

Put the result where your users can reach it. Each place has a source type in
**Settings > Application > Web Spaces > Add board…**:

| Where | Source type | What the user enters |
| --- | --- | --- |
| A folder on their computer, such as a checkout | **Local folder** | The folder path |
| A ZIP file anywhere they can download | **Local ZIP** | The `.zip` path after downloading |
| A folder in a GitHub organization repository | **GitHub repository** | `owner/repository`, a **Ref** and the **Board directory** |
| A ZIP attached to a GitHub release | **GitHub release ZIP** | `owner/repository`, then a release and the exact asset |

A ZIP may hold the web Space at its root, as the script writes it, or inside
one enclosing folder.

For a repository, commit the built folder, not only its sources. **Ref** is
blank for the default branch, a branch name, `tag:v1.0` or
`commit:<full SHA>`. **Board directory** is the folder's path from the
repository root, for example `subspace-desktop/web-spaces/examples/weather`.
Subspace downloads the whole repository archive for that commit, so the whole
repository must be within the package limits (64 MiB archive, 10,000 entries)
and use portable names; keep web Spaces in small repositories.

GitHub sources need a GitHub account selected in Settings that is an active
member of the repository's organization with read access. Personal
repositories are not eligible. Other Git hosts are not supported: share a ZIP
or a folder instead.

## 10. Updates

- **Local folder** and **Local ZIP**: the user chooses **Edit… > Replace
  source…** with the new folder or ZIP.
- **GitHub repository** on a branch: **Check for updates** offers the new
  commit. Pinned tags and commits are never offered updates.
- **GitHub release ZIP**: **Check for updates** offers the newest eligible
  published release with an asset of exactly the same name, so keep the
  asset name stable (for example `hello.zip`, not `hello-1.2.0.zip`).

Every update is downloaded and validated before the user confirms it with
**Replace now**. An update installs a new private folder, so the user allows
permissions again.

## 11. What Subspace checks at install

**Validate source** stages the package in a private folder and checks:

1. the source: the folder or ZIP is readable, or the GitHub account,
   organization membership, repository access and ref are valid;
2. the [package rules](web-spaces-reference.md#package-rules): names, links,
   case collisions, entries and sizes;
3. that `index.html` and `subspace-board.json` are regular files at the root;
4. the [manifest](web-spaces-reference.md#declare-an-app).

It then shows the name, version, source, requested permissions (`Nothing is
granted automatically.`) and any warnings. Nothing is installed until **Install
now**, and no permission is granted until the user allows it. Subspace does
not check scripts loaded at run time, so a package that validates can still
fail to run; open it after installing.

The [reference](web-spaces-reference.md) lists every install, manifest and
bridge error with its cause.
