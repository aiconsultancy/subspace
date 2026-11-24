# Web Spaces guide

<!--
Evidence: spaces/web/src/{lib,toolbar,source,policy,state,manifest,bridge,
bridge/{runtime,protocol,theme,issues},page,page/unsupported,page/native,app,
install/*}.rs, bridge/init.js, browser_session.rs, browser_session/*.rs,
page/native/{view,session,cookies}.rs, pages.rs (Spaces signed in by the app), manager.rs, manager/{view,
view/collection,view/editor,editor,model,operations}.rs;
products/subspace/src/main.rs (one registration per webBoards entry, shared guide);
products/subspace/src/web_spaces.rs, spaces/web/src/builtin.rs (built-in Fleet Console);
subspace-framework/board-kit/dashboard/src/settings.rs; web-spaces/examples/weather/*;
docs/web-spaces-reference.md, docs/web-spaces-guide.md.
Not checked in this pass: a live web page, a GitHub import, a ZIP import,
WebKit or WebView2 behavior, or the native bridge in a running app. The Linux
claim comes from the source of page/unsupported.rs.
-->

## Overview

A web Space shows one web page, or a web app from a folder on this computer, in
a native web view: WebKit (WKWebView) on macOS and Microsoft Edge WebView2 on
Windows. Linux does not show web Spaces yet. The board then says `Web Spaces
need macOS or Windows. Open the page in your browser instead.`

This guide covers every web Space. Each `webBoards` entry in the configuration
file, and each board that you install in Settings, appears as its own board with
this shared guide. A board has the title that you give it, and the switcher
tooltip says `<title>: web page on <site>` or `<title>: local web app`.

Subspace comes with one web Space built in: **Fleet Console**, which shows your
Stargate fleets. It is in the tab strip from the first start on macOS and
Windows, with nothing to install. Enter its Stargate API address and token in
**Settings > Spaces > Fleet Console**. Being part of the app, it starts with
the permissions its manifest asks for allowed; you can turn any of them off
there. To hide it, turn it off in Settings. Removing it under **Web Spaces**
only resets it: it comes back at the next start.

There are three ways to add another web Space:

- Add a website with its address.
- Install a built web app from a folder, ZIP file, GitHub repository directory
  or GitHub release ZIP.
- Install the **Weather** example package. It is a keyless UK-postcode weather
  app that ships in the Subspace repository.

To build and package your own web Space, see
[Build and package a web Space](../../docs/web-spaces-guide.md). The manifest
keeps the older name `subspace-board.json`, and the configuration key stays
`webBoards`, for compatibility.

Open a web Space from the switcher. Press `?` while board navigation has focus,
choose **Help** in the command palette (`Cmd+K` on macOS, `Ctrl+K` on Windows)
or use the **Help (?)** toolbar button to show this guide.

## Before you start

- **Platform.** macOS (WKWebView) or Windows (WebView2). Windows CI builds and
  tests the code, but native Windows runtime validation is outstanding.
- **Sign-in.** A website board uses its own site's sign-in. Subspace does not
  sign in for you, unless your app includes a web Space that signs in with
  your account (see [Spaces signed in by the app](#spaces-signed-in-by-the-app)).
  Each board keeps its own cookies and site data across restarts, and boards
  do not share sign-ins with each other or with your browser.
- **GitHub imports** need a GitHub account selected in Settings. The account
  must belong to the repository's organization and have read access to the
  repository. Personal repositories are not eligible. Only built files are
  imported. No build, repository script or dependency installation runs. Changing
  accounts does not disable an installed copy, but it discards old-account
  work.
- **Built apps.** An app needs `index.html` at its root. For a new import it
  also needs `subspace-board.json`. A folder app is typically the `dist` folder
  of a Vite or React build. Build it yourself first. Subspace never builds.
- **Addresses.** A website must use `https://`, or `http://` to `localhost` or a
  loopback address for local development. Anything else is blocked.
- **Weather service.** The Weather example calls the free hosted
  Open-Meteo and Postcodes.io services. They are for non-commercial evaluation
  and prototyping. Review the package terms before other use.

Warning: install an app only from a source you trust. A manifest grant binds to
the app folder and the exact manifest bytes, but changing only JavaScript or
other app files does not change the manifest fingerprint. Revoke permissions
before you install an update from a source you do not trust.

## Main view

The board has a toolbar and, below it, the page in a native view that fills the
rest of the board. A web Space has no navigator and no inspector, and it has
no tab strip because it shows one page. Subspace menus and dialogs that open
over the page can appear behind it. The board hides the native view while
another board is active or something covers it.

- **Opening.** The page is created the first time you show the board. Until
  then the board says `Opening the page…` below the toolbar.
- **Cannot show this page.** When the page cannot open, the board shows an
  error box below the toolbar, with the reason and a **Retry** button. Reasons
  include
  `Set an https:// address for this board under webBoards in the config file.`
  and `<path> has no index.html. Build the app into this folder, then select
  Retry.` An app with an invalid manifest says that it cannot open.
- **Status bar.** The status shows the host of the page you are on, with the
  port when it is not the default, or `Local web app` for a folder app. A
  notice replaces it until the next page loads.
- **Busy.** The board shows a busy state while the page loads.
- **Window title.** The page title is shown as one line of at most 80
  characters, with `…` when longer.
- **Tab badge.** An app with the `badge` permission can show a badge of at most
  8 characters on its board tab.

### Web apps from a folder

The board serves the folder at its own private address,
`subspace://<id>/` on macOS and `https://subspace.<id>/` on Windows. No web
server or Node.js runs.

- The app's own address is a secure context, so browser storage and
  `crypto.subtle` work.
- Root-relative paths, such as `/assets/index.js`, load from the folder.
- An address that names no file and has no file extension returns
  `index.html`, so client-side routes survive a reload.
- Files cannot be served from outside the folder, including through links. A
  single file is limited to 64 MiB.
- Imported folders are private snapshots. To install a new build, use
  **Edit… > Replace source…**. For a manually configured live folder, rebuild
  and select **Reload page**.
- Requests to other sites are cross-origin. They work only when the site
  allows the app's origin.
- An app with a manifest runs under a Content Security Policy. Load scripts
  from app files, not inline script blocks, external script hosts or `eval`.
  Inline styles are allowed. Connections can use the app origin, HTTPS or
  loopback HTTP, and the destination must still allow the app's origin. Frames,
  objects, base URL changes and form navigation are blocked. React and Vite
  client routing and JavaScript form handlers still work. A folder app without
  a manifest keeps its earlier behavior and gets no bridge.

### Spaces signed in by the app

An app built on Subspace can include a web Space that signs in to its site
with your account under **Settings › Account**, so the site needs no second
sign-in. Subspace on its own includes none, and you cannot add one in
Settings: the app's makers build it in, on macOS only. Such a Space works
like other website boards, with these differences:

- **Sign-in required.** Until you sign in, the Space says `Sign-in required`
  and asks you to sign in under **Settings › Account**. Signing out or
  switching account closes its page at once.
- **Nothing on disk.** Its page keeps the site's session, and anything else
  the site stores, in memory only, and forgets it all when the page closes.
  The page cannot read the session, and Subspace never puts it in an
  address or a log.
- **Renewal.** Subspace renews the session before it ends. If the site asks
  for a new session, Subspace renews it and reloads the page. If the site
  asks again straight away, or the session cannot be renewed in time, the
  Space says `Your <title> session has ended.` with **Retry**, which reopens
  the page where you were. If the page's private store does not accept a
  renewed session, the Space says
  `Cannot install the private session for this page. Try again.` with
  **Retry**. Unsaved changes in the page may be lost.
- **Back.** Back stops at the first page the Space showed.
- **Only the site's pages for the app.** The page loads only the part of the
  site made for the app. Other pages are blocked, and the status bar says
  `Blocked a page this Space does not show.`, including frames embedded
  from other sites. **Open in browser** opens the site's ordinary page,
  which signs in on its own.
- **No bridge.** The page gets no `window.subspace` and no developer tools.

Closing the Space, signing out, switching account and quitting the app each
also end the session at the site, when the site offers that. Quitting waits
at most two seconds for it. On Windows and Linux the app shows the site
with its own sign-in instead, or not at all, as its makers choose.

### The Weather example

**Install Weather example…** fills the new-board form with the board ID
`weather`, the name `Weather`, the source type **GitHub repository**, the
release repository shown in **About** and the board directory
`subspace-desktop/web-spaces/examples/weather`. It uses the same GitHub
authorization and validation as other imports. To install from a local checkout
instead, choose its `subspace-desktop/web-spaces/examples/weather` folder as a
**Local folder**, or package it as a ZIP with
`make package-web-space SPACE=web-spaces/examples/weather` and choose
`dist/web-spaces/weather-1.0.0.zip` as a **Local ZIP**.

The package is plain HTML, CSS and JavaScript with no build step. It requests no
permissions and declares no settings. Enter a UK postcode and select **Show
weather**. The board remembers the postcode in local storage on this device.
**Refresh** reloads the forecast, at most once a minute (once every 15
seconds after a failure). The page refreshes a forecast at most every 15
minutes while it is visible and labels data older than 30 minutes as saved.

The board shows invented sample data, with a banner and a `Synthetic preview`
label, when its page address has `?preview=NAME`. NAME is `clear`, `partly`,
`cloudy`, `fog`, `drizzle`, `rain`, `heavyrain`, `snow` or `storm`, optionally
followed by `-night`. A board whose source is a local folder starts at the
folder's root and cannot add that query itself. To see a preview, use a
website board that points at a local static server, for example
`http://127.0.0.1:8765/index.html?preview=rain`.

Weather sends the postcode you enter to Postcodes.io and the postcode's
coordinates, rounded to four decimal places, to Open-Meteo. Nothing goes to
GitHub, Subspace chat or analytics. Forecasts can be wrong. Do not use the board
for safety decisions.

<!-- Screenshot omitted: a populated web Space needs a page. A web Space has no
bounded fixture mode. The Weather package offers a synthetic path: serve the
folder on 127.0.0.1 and add a website board with ?preview=rain, which shows a
labelled sample location and no network request to a weather service. The parent
decides whether to capture it. No image is claimed for the Weather scenes; their
provenance is not recorded in this guide. -->

## Toolbar and filters

The page controls are in the board's own toolbar, directly above the page, not
in the title bar. They are disabled until the page exists.

The navigation controls are at the start of the toolbar:

| Control | Action |
| --- | --- |
| **Back** | Move back through the page's history. |
| **Forward** | Move forward through the page's history. |
| **Reload page** | Load the page again. The shell's reload (`r` or `Cmd+R`) does the same. |
| **Go to the start page** | Open the configured address. |

After them the toolbar shows the board's title and where the page is: the host
of the page you are on, with the port when it is not the default, or
`Local web app` for a folder app. This is the real site, whatever the page
calls itself. Until the page reports its address, the toolbar shows the site
of the configured address. Once it has, the status bar shows the same place
unless a notice replaces it.

The end of the toolbar has the page's other action:

| Control | Action |
| --- | --- |
| **Open in browser** | Open the current page in your default browser. Web app boards do not have it, and an app's private address never opens there. |

In a narrow window the controls at the start wrap onto a second line, and the
page starts below them. There are no filters, groups or search items. A failed
navigation action puts its error in the status bar.

Links that open a new window open in your default browser when they are
`http://` or `https://` pages. The board opens at most one browser page per
second. Other links, such as `mailto:`, and unsafe addresses are blocked, and
the status bar says `Blocked a <scheme>: link. Web Spaces only load web pages.`
or `Blocked an insecure http:// page. Web Spaces load HTTPS, or HTTP on
localhost.` An HTTPS address the Space does not show, such as one with a
user name or password in it, says `Blocked a page this Space does not
show.`

## Details

A web Space has no detail panel. The page is the whole view below the toolbar.
A folder app that has a manifest can use the native bridge described below.

### App bridge

A folder app with `subspace-board.json` gets a frozen `window.subspace` object in
its main document. URL boards and manifest-free folder apps do not. All methods
return promises. A denied call rejects with an error that tells the user to
allow the action in the board's Subspace settings.

| Method | Permission | Result or limit |
| --- | --- | --- |
| `info()` | None | Bridge version, granted permissions and notification permission |
| `settings.get()` | `settings` | Declared non-secret saved values, or defaults when unset |
| `setBadge(textOrNull)` | `badge` | At most 8 characters, without control characters |
| `setContext(valueOrNull)` | `context` | Label up to 100 characters, content up to 8192 UTF-8 bytes |
| `notify({title, body})` | `notifications` | Title up to 100 characters, body up to 500, without control characters |
| `theme()` | None | The app's theme: id, name, light or dark mode, tones as `#rrggbb` and CSS font families |
| `secrets.get()` | `secrets` | Saved values of the declared secrets and of the page's own |
| `secrets.set(key, valueOrNull)` | `save-secrets` | Saves or deletes one secret of the page's own; at most 32, and 10 saves in 10 seconds |
| `issues.scope()` | `issues` | The issue scope (profile name, API base, project keys), or `null` |
| `issues.resolve(identifiers)` | `issues` | One resolution per distinct identifier, at most 50 |
| `issues.open(identifier)` | `issues` | Opens the issue in the Space that answers issue requests |

- Unknown setting keys and secret fields never appear in `settings.get()`.
  Only `secrets.get()`, with the `secrets` permission, returns saved secrets:
  those the manifest declares as `secret` that have a value, and those the
  app saved itself with `secrets.set()`, such as a token for each account
  you add in the page. Saving needs its own permission, `save-secrets`
  (**save its own secrets**), and never changes a secret the manifest
  declares: those are yours to set here. A page saves secrets only while its
  Space is shown, and sees only those its own app saved. An update from the
  same GitHub repository, release asset or local folder keeps them; an app
  from another source, or a ZIP reinstall, starts with none. A page allowed `secrets`
  holds them in its own memory, so allow it only for an app you trust.
- The page hears two events on `window`. `subspace:theme` carries the new
  theme as its `detail` whenever the theme, mode, a tone or a font changes.
  `subspace:issues-changed` tells a page allowed `issues` to ask again for
  the scope and the issues it shows, for example after sign-in or when a
  `pending` issue has loaded. Events reach only the document the web Space
  shows, and only while it has the app's own origin.
- `issues.*` uses the same issue contract as the issue tiles in Channels.
  With the Issues Space disabled or signed out, the scope is `null` and
  issues resolve as `unavailable` or `signed-out`. `issues.open()` switches
  to the Space that answers and shows the issue there.
- Chat treats page-provided context as untrusted data, not instructions. Badge
  and context state reset when the page navigates.
- Requests are at most 16 KiB of UTF-8 JSON. At most 32 calls can be pending,
  and each times out after 10 seconds. A call can also fail during navigation,
  when the app closes or when the native event queue is full.
- The sender URL comes from the native engine, and Subspace also checks the
  page's exact origin. Another site cannot use the bridge after navigation.
- The page `Notification` adapter reads the native decision with
  `Notification.requestPermission()`. It shows no web prompt and grants
  nothing. Delivery uses Subspace's configured notification channel, and at most
  one request per board is accepted every five seconds. It is not Web Push: no
  service worker, actions, click callbacks or OS retraction. `close()` does
  nothing. Acceptance does not prove that the OS showed an alert.
- The bridge has no shell, process, filesystem, clipboard or arbitrary native
  API, and no credential access beyond the app's own secrets.

### The manifest

`subspace-board.json` sits next to `index.html`. It is at most 256 KiB. `name`
(up to 60 characters) and `version` (up to 40) are required. `permissions` and
`settings` are optional. The known permissions are `notifications`, `settings`,
`badge`, `context`, `secrets`, `save-secrets` and `issues`. An unknown permission name produces a note in Settings
and grants nothing. An invalid manifest prevents the app from opening.

```json
{
  "name": "Notes",
  "version": "1.0.0",
  "permissions": ["settings", "badge", "context", "notifications"],
  "settings": [
    { "key": "name", "label": "Notebook", "type": "text", "default": "Personal" },
    { "key": "limit", "label": "Notes to keep", "type": "number", "default": 50 },
    { "key": "compact", "label": "Compact list", "type": "boolean", "default": false },
    { "key": "sort", "label": "Sort order", "type": "select", "options": ["Newest", "Oldest"], "default": "Newest" },
    { "key": "token", "label": "Sync token", "type": "secret" }
  ]
}
```

Each setting has a unique `key` (an ASCII letter first, then up to 39 letters,
digits, `_` or `-`), a `label` of up to 60 characters, a `type` (`text`, `url`,
`number`, `boolean`, `select` or `secret`) and an optional `help` of up to 200
characters. Declare at most 32. Select fields have 1 to 50 text options. Secret
fields cannot have defaults. Text, URL and number inputs accept up to 2000
characters, and URL inputs accept HTTP and HTTPS addresses.

## Actions

### Add a web Space

Open **Settings > Application > Web Spaces** and choose **Add board…**. The
form is titled **New board**.

1. Enter a **Board ID**: a lower-case letter first, then letters, digits or
   hyphens, up to 40 characters. Use the ID in `boards` and `boardOrder` like
   any other board.
2. Enter a **Name** of 1 to 60 characters. It is the name in the switcher.
3. Choose the **Source type**: **Website URL**, **Local folder**, **Local ZIP**,
   **GitHub repository** or **GitHub release ZIP**.
4. Enter the **URL or absolute path** (use **Choose…** for a folder or ZIP) or,
   for GitHub, the repository as `owner/name` or its root URL.
5. For a repository, optionally enter a **Ref** (a branch name, `tag:v1.0` or
   `commit:<full SHA>`; blank is the default branch) and a **Board directory**
   (blank is the repository root). The directory must already contain
   `subspace-board.json`, `index.html` and built assets. Pinned tags and commits
   do not track a branch.
6. For a release ZIP, optionally turn on **Include prereleases**, choose **Load
   release ZIPs**, and choose an asset in **Release and ZIP asset**. Updates
   match that exact asset name.
7. Choose **Validate source**. The page shows `Ready to install: <name>`, the
   package name and version, the source, the requested native permissions
   (`Nothing is granted automatically.`) and any warnings.
8. Choose **Install now**. During retrieval the page says `Retrieving and
   validating source… Installed boards are unchanged.` **Cancel import**
   stops it.

The board appears without a restart, and the page says `Applied immediately. No
restart is needed.` Installs, renames and removals apply immediately.
**Settings** Save and Cancel do not undo them.

ZIPs contain one board at the root or inside one enclosing directory. An import
rejects unsafe paths, links, platform-colliding entries and oversized expansion.
The importer accepts portable ASCII package paths and ordinary ZIPs, not ZIP64
archives. It checks the required root files, not every dynamic JavaScript asset
reference. Limits: a 64 MiB archive, 32 MiB per file, 10,000 entries and 256 MiB
expanded.

A new app import needs `subspace-board.json` and `index.html` at the selected
board root. Existing manifest-free configured folders keep working.

### Configure a board by hand

For manual configuration, the existing format remains supported in the
configuration file:

```json
{
  "webBoards": {
    "wiki": { "title": "Team wiki", "url": "https://wiki.example.com" },
    "notes": { "title": "Notes", "path": "~/apps/notes/dist" }
  }
}
```

- The key is the board id: lower-case letters, digits and `-`, starting
  with a letter. Use it in `boards` and `boardOrder` like any other board.
- `title` is the name in the switcher. It is the id when you leave it out.
- Set either `url` or `path`:
  - `url` is the start page. It must use `https://`, or `http://` to
    `localhost` or a loopback address for local development.
  - `path` is a folder of built web app files with an `index.html`, such as
    the `dist` folder of a Vite or React build. Use an absolute path, or one
    that starts with `~/`.

### Update, replace and remove

Use **Check for updates** on a tracked GitHub board. A check does not install
anything. It says `No newer eligible version was found.` or `Update available
for <title>`. Choose **Download update**, then review the package and choose
**Replace now**. Download, validation and confirmation are separate. Release
updates match the selected ZIP's exact name. Prereleases are excluded by
default.

Choose **Edit…** to rename a board with **Save name**, or **Replace source…** to
import a new build. Replacing an imported app refreshes its view immediately.
Existing grants apply only to their original source and exact manifest.

Choose **Remove…**, then **Remove board** (or **Keep board**). Removal
unregisters a board immediately. It does not delete the original app folder,
ZIP or browsing data.

### Agent tools

A web Space adds no Copilot or MCP tool of its own. A page with the `context`
permission can share a label and text with chat. The generic Help lookup
`board_help` can read this guide.

## Keyboard shortcuts

A web Space defines no board keys. The page can have its own keys. While a web
Space is visible, keyboard shortcuts go to the page when it has focus. Click
outside the page to use Subspace shortcuts.

| Key | Action |
| --- | --- |
| `r` / `Cmd+R` | Reload the page. Acts only when the page does not have focus. |
| `Cmd+1` to `Cmd+9` (`Ctrl+1` to `Ctrl+9`) | Switch to the board at that switcher position. |
| `F1` to `F12` | Switch to the board at that position in the enabled-board order shown in the app. |

Typing in a page field is ordinary text input. This guide does not describe keys
that the page itself defines. The Weather page supports `Tab` through its form,
**Refresh** and the hourly controls, and `←`, `→`, `Page Up`, `Page Down`,
`Home` and `End` to scroll its hourly forecast.

## Settings

Web Spaces have two settings places.

### Manage boards

**Settings > Application > Web Spaces** is described under
[Add a web Space](#add-a-web-space). It lists installed boards with a source line
such as `Folder · <path>`, `ZIP · <path>`, `GitHub · <repository> / <directory>
· <ref>` or a release asset, and **Edit…**, **Check for updates** (tracked
GitHub boards) and **Remove…**. With none it says `No web Spaces installed. Add a website, choose a built folder or ZIP, or install from one of your GitHub organizations.`

### A board's settings

Open **Settings > Spaces > [Space name]**. The page describes web Spaces as
showing a website or a built local app in a native web view, available on macOS
and Windows, not Linux.

- **Enabled** applies on Save without a restart.
- A folder app with `subspace-board.json` shows its declared fields here, a note
  `<name> <version>, from <folder>` and one **Allow this app to …** checkbox for
  each requested permission: **show notifications**, **read its settings**,
  **show a badge on its tab**, **share context with Copilot chat**, **read
  its saved secrets**, **save its own secrets** and **read and open issues**.
  Permissions start off. Select the ones you trust, then **Save**. Setting and
  permission edits apply on Save without a restart. The page reads the current
  values again; settings and secrets have no change event. Cancel leaves the
  saved configuration unchanged.
- Notification permission also needs a channel under **Settings >
  Notifications > [Space name]**. Delivery, in-app style and sound work as for
  other boards. A permission is not delivery.
- Non-secret values are saved under `webBoards.<id>.settings` and secrets under
  `secrets` in the private config file. Saved secrets never populate inputs.
  Leave a secret input blank to keep it, or replace or clear it explicitly.
  Secrets are plaintext in that file, not OS-keychain entries or encrypted app
  storage.
- An app allowed **read its saved secrets** may save secrets of its own. The
  page then says **This app has saved N secrets** (never what they are), and
  **Clear** removes them all on **Save**. They are removed with the Space.
- Grants bind to the canonical app folder and the exact manifest bytes,
  including whitespace. Changing either needs new grants. Restart after a
  manifest change, then review permissions. The running old bridge refuses calls
  once the manifest identity changes.

A web page board has no settings fields, and a website cannot request
permissions. There are no background-update controls.

## Troubleshooting

| Symptom | Check or action |
| --- | --- |
| `Cannot show this page` with an address message | The URL is not `https://` or loopback HTTP. Fix `url` in `webBoards` or the board's **Edit…** form and select **Retry**. |
| `<path> has no index.html` | Build the app into the folder, then select **Retry**. |
| `This app cannot open.` | The manifest is invalid, over 256 KiB or has the wrong shape. Fix `subspace-board.json` and reload. |
| `Web Spaces need macOS or Windows.` | Linux has no web view. Open the page in your browser. |
| `Sign-in required` | The Space signs in with your account. Sign in under **Settings › Account**. |
| `Your <title> session has ended.` | The site ended the session, or it could not be renewed. Select **Retry**. |
| `Cannot install the private session for this page. Try again.` | The page's private store did not accept the session, when the Space opened or renewed it. Select **Retry**. |
| `Blocked a page this Space does not show.` | Any web Space blocks an address with a user name or password in it; a Space signed in by the app also blocks pages outside the part of the site made for it. Select **Open in browser**. |
| A blank page | The page may need a site setting or a cross-origin permission that its host does not allow. Check the status bar host. Select **Reload page**. |
| A client route returns an error after reload | Folder apps return `index.html` for addresses with no file extension. Check that the route has no extension and that `index.html` exists. |
| `Blocked an insecure http:// page` | Web Spaces load HTTPS, or HTTP on `localhost` and loopback only. |
| `Blocked a mailto: link` (or similar) | Web Spaces only load web pages. Open the link in a mail or other app yourself. |
| A download does nothing | The status bar says `Downloads are not available in web Spaces yet. Use Open in browser.` |
| The bridge call rejects with a permission error | Allow the action in **Settings > Spaces > [Space name]** and save. |
| `window.subspace` is missing | The board is a website, or a folder app without `subspace-board.json`. |
| Issues stay plain or `issues.scope()` is `null` | Enable the Issues Space and sign in under Settings › Account. The page's `subspace:issues-changed` handler then asks again. |
| `issues.open()` does nothing | It opens issues only while this web Space is the one shown, and only for project keys in the scope. |
| Notification never appears | Check both the permission and a channel under **Settings > Notifications > [Space name]**. Acceptance by the host does not prove that the OS showed an alert. |
| A new build does not show | An imported folder is a private snapshot. Use **Edit… > Replace source…**. For a manually configured live folder, rebuild and select **Reload page**. |
| `Updates are unavailable for this account or source.` | Select a GitHub account in Settings that belongs to the repository's organization. |
| `GitHub account changed.` | Check or validate again with the current account. Installed boards are unchanged. |
| Install fails validation | The importer rejects unsafe paths, links, ZIP64 archives and oversized expansion. Repackage the app. |
| Weather shows `Synthetic preview` | The page address has `?preview=`. Remove the query to see a live forecast. |

A successful validation does not prove that the app works, that its remote
services accept its origin, or that a GitHub grant authorizes later updates.

## Limits

- Each board keeps its own cookies and site data across restarts. Boards
  do not share sign-ins with each other or with your browser. A Space signed
  in by the app keeps nothing across restarts, and needs macOS.
- The page only loads HTTPS pages, or HTTP pages on this computer. Links
  that open a new window open in your default browser. Other links, such as
  `mailto:`, are blocked.
- Downloads are not available yet. Use **Open in browser**.
- While a web Space is visible, keyboard shortcuts go to the page when it
  has focus. Click outside the page to use app shortcuts.
- Menus that open over the page can appear behind it.
- Linux has no web view. Windows runtime validation is outstanding.
- Web Spaces have no board tools, no search items and no background refresh.
- Package limits: 64 MiB per archive, 32 MiB per file, 256 MiB expanded, 10,000
  entries, paths up to 1,024 bytes and depth 32. A served file is at most 64
  MiB. A manifest is at most 256 KiB.
- The Weather example uses free hosted services for non-commercial evaluation
  and prototyping only. Open-Meteo allows 600 calls a minute, 5,000 an hour and
  10,000 a day. Northern Ireland (`BT`) postcode data is free only for
  non-commercial use. The board does not claim that any deployment is licensed.
  Weather data is by Open-Meteo.com under CC BY 4.0, and postcode data is from
  Postcodes.io and Crown-copyright sources.
- **Not checked in this pass:** a live page, a GitHub, ZIP or folder import,
  WebKit or WebView2 behavior, the native bridge, notification delivery, and
  Weather calls to Open-Meteo or Postcodes.io. These statements come from source,
  tests and change records.
- This guide has no screenshot. Weather's synthetic `?preview=` path loaded
  its preview title in the isolated macOS development build, but background
  capture did not include the web content. No blank image is published.
  See [The Weather example](#the-weather-example).

For the full manifest, package and bridge reference, see
[the web Space reference](../../docs/web-spaces-reference.md). To build and
package your own web Space, see
[Build and package a web Space](../../docs/web-spaces-guide.md).
