---
name: web-space
description: Scaffold, test, package and publish a Subspace web Space (a built web app with a subspace-board.json manifest shown by the Web Space). Use when asked to make, build, bundle, zip, package, ship, install or debug a web Space or web app for Subspace; to use window.subspace settings, badge, context, notifications, theme, secrets or issues; to put a web Space in a folder, a ZIP or a GitHub repository; or when a web Space install fails validation.
compatibility: Subspace desktop repository (aiconsultancy/subspace, subspace-desktop/) with python3, and Node.js through mise for tests.
metadata:
  owner: aiconsultancy/subspace
  last-reviewed: "2026-11-19"
---

# Web Space

A web Space is a folder with `index.html` and `subspace-board.json` at its
root, plus the built files they load. Subspace serves it in a native web view
(macOS, Windows), with no server and no build. The manifest keeps the old name
`subspace-board.json`, the config key `webBoards` and labels such as **Add
board…** for compatibility; do not rename them.

Paths are relative to `subspace-desktop/`. The rules live in the docs; this
skill routes to them. Read the
[guide](../../../docs/web-spaces-guide.md) for the walkthrough and the
[reference](../../../docs/web-spaces-reference.md) for every field, limit and
message. The worked example is
[Weather](../../../web-spaces/examples/weather/README.md).

## 1. Scaffold

- [ ] Copy `assets/template/` to the new folder (in this
  repository: `web-spaces/examples/<name>/`). It has a manifest with one
  setting, `index.html`, `main.js` (DOM wiring), `app.js` (pure logic) and
  `test/app.test.js`.
- [ ] Edit `subspace-board.json`: `name` (1 to 60 characters) and `version`
  (1 to 40) are required; `permissions` and `settings` are optional
  ([Declare an app](../../../docs/web-spaces-reference.md#declare-an-app)).
  Request only what the app calls.
- [ ] Load scripts from files only. No inline `<script>` blocks, no `onclick=`
  attributes, no `eval`, no script CDNs: the Content Security Policy blocks
  them. `fetch` may reach HTTPS or loopback HTTP hosts that allow the origin.
- [ ] Bundled apps (Vite, React): the web Space folder is the build output.
  Put the manifest in it (for example Vite's `public/`). Subspace never builds.
- [ ] Portable ASCII file names only (letters, digits, space,
  `-_.@()+,[]{}`), no symbolic links, no names differing only by case.

## 2. Use the bridge

- [ ] Feature-detect `globalThis.subspace`; it is absent in a browser and in
  website Spaces.
- [ ] Call `subspace.info()` first. Its `permissions` lists only what the user
  allowed; every permission starts off.
- [ ] `settings.get()` needs `settings`; `setBadge` needs `badge`;
  `setContext` needs `context`; `notify` or `new Notification(...)` needs
  `notifications` plus a notification channel. Limits and messages:
  [Read settings and call Subspace](../../../docs/web-spaces-reference.md#read-settings-and-call-subspace)
  and [Page Notification adapter](../../../docs/web-spaces-reference.md#page-notification-adapter).
- [ ] Feature-detect newer methods (`typeof subspace.theme === "function"`,
  `subspace.issues`) so the app still runs in an older Subspace.
- [ ] Follow the app's look: call `subspace.theme()` at start (no permission)
  and listen for `subspace:theme` on `window`; map `tones` to CSS custom
  properties and use `fonts.ui` and `fonts.mono` as `font-family` values
  ([Follow the app's theme](../../../docs/web-spaces-reference.md#follow-the-apps-theme)).
- [ ] Secrets: `settings.get()` never returns them. Only `secrets.get()`, with
  the `secrets` permission, returns the saved `secret` fields the manifest
  declares. Send a secret only to its service; never into a URL, context,
  notification or page storage
  ([Read the app's secrets](../../../docs/web-spaces-reference.md#read-the-apps-secrets)).
- [ ] Issues: with `issues`, `issues.scope()`, `issues.resolve(ids)` (at most
  50 distinct) and `issues.open(id)` reach the Issues Space as Channels'
  issue tiles do. Re-resolve on `subspace:issues-changed`; `pending` answers
  settle that way ([Show issues](../../../docs/web-spaces-reference.md#show-issues)).
- [ ] Fakes in tests need the same shapes: `theme` returns the documented
  `Theme`, `issues.resolve` returns one resolution per identifier.

## 3. Test with Node

- [ ] Keep logic out of the DOM and pass the bridge in, as `app.js` does.
- [ ] Run `node --test test/*.test.js`, or give the folder a `mise.toml`
  pinning Node as Weather does and run `mise run test`.
- [ ] In this repository, add a root `mise.toml` task like
  `weather-space-test` and add it to the `check` task's `depends`.
- [ ] Copy Weather's `test/package.test.js` checks: manifest limits, local
  references exist, no inline scripts, only expected remote hosts.

## 4. Package

```sh
python3 scripts/package-web-space.py <folder> [out.zip]   # or:
make package-web-space SPACE=<folder> [OUT=<file.zip>]
```

- [ ] Run with `--check` first; fix every `error:` line (see
  [references/errors.md](references/errors.md)).
- [ ] Bump `version` for each published change.
- [ ] The default output is `dist/web-spaces/<folder>-<version>.zip`. It must
  be outside the folder. The script leaves out dotfiles, `test/`, `tests/`,
  `node_modules/`, `mise.toml`, `package.json`, lock files and `*.test.*`;
  `--include PATTERN` keeps one, `--exclude PATTERN` drops more.
- [ ] Report the printed `wrote <path>` and `sha256:` lines.

Details: [Package it](../../../docs/web-spaces-guide.md#8-package-it).

## 5. Put it somewhere and load it

All installs start at **Settings > Application > Web Spaces > Add board…**,
then **Validate source** and **Install now**. Tell the user which row applies:

| Put the result | Source type | User enters |
| --- | --- | --- |
| A folder (a checkout, a shared drive) | **Local folder** | The absolute folder path, or **Choose…** |
| A ZIP anywhere (the script's output, an attachment, a download) | **Local ZIP** | The absolute `.zip` path, or **Choose…** |
| A folder committed to a GitHub organization repository | **GitHub repository** | `owner/repository`; **Ref** blank (default branch), a branch, `tag:v1.0` or `commit:<full SHA>`; **Board directory** from the repository root |
| A ZIP asset on a published GitHub release | **GitHub release ZIP** | `owner/repository`, **Load release ZIPs**, then the asset |

- [ ] GitHub needs a selected GitHub account that is an active member of the
  repository's organization with read access; personal repositories are not
  eligible. Other Git hosts are not supported: use a folder or ZIP.
- [ ] A repository import downloads the whole repository archive, which must
  itself be at most 64 MiB and 10,000 entries with portable names.
- [ ] For release updates keep the asset name stable (`hello.zip`).
- [ ] After installing, the user allows permissions in **Settings > Spaces >
  [Space name]** and saves. Updates and replacements need them allowed again.
- [ ] Folder and ZIP installs are snapshots: to update, **Edit… > Replace
  source…**. GitHub branch and release installs offer **Check for updates**.

Details: [Publish and install](../../../docs/web-spaces-guide.md#9-publish-and-install)
and [Install sources](../../../docs/web-spaces-reference.md#install-sources).

## 6. When it fails

Look up the exact message in [references/errors.md](references/errors.md).
The full tables are in the [reference](../../../docs/web-spaces-reference.md#package-rules).
