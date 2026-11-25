# Web Space manifest, packages and bridge reference

A web Space is a web app that the Web Space shows in a native web view. A
folder-based web Space can declare native settings and request selected
Subspace actions through `subspace-board.json`, which sits beside its built
`index.html`. No Node.js process or HTTP server runs in Subspace. URL web
Spaces (websites) do not receive this API. On macOS a product can also
compose a web Space whose site is signed in with the app's account; see
[Sign a web Space in with the app's account](#sign-a-web-space-in-with-the-apps-account).

This page is the reference. For a step-by-step walkthrough, from an empty
folder to a published package, read the
[web Space authoring guide](web-spaces-guide.md). The packages in the
repository, including the Weather example, are described in
[web-spaces/README.md](../web-spaces/README.md). The in-app guide is
[Web Spaces](../spaces/web/help.md).

Some names keep the older word *board* for compatibility, because changing
them would orphan installed web Spaces or break existing packages: the
manifest file `subspace-board.json`, the `webBoards` configuration key, the
private data folders `web-board-apps` and `web-boards`, labels such as
**Add board…** and **Board directory**, and the `subspace-web-board` crate.

## Install sources

Install web Spaces in **Settings > Application > Web Spaces**. Choose **Add
board…**, enter a **Board ID** and **Name**, choose a **Source type**, then
**Validate source**, review the package and requested permissions, and choose
**Install now**. Installation, renaming and removal apply immediately.
Settings Save and Cancel do not undo them.

| Source type | Location | What is imported |
| --- | --- | --- |
| **Website URL** | An `https://` address, or `http://` to `localhost` or a loopback address | Nothing; the page is shown live |
| **Local folder** | An absolute folder path, or **Choose…** | A private snapshot of the folder |
| **Local ZIP** | An absolute `.zip` path, or **Choose…** | The ZIP's web Space folder |
| **GitHub repository** | `owner/repository` or `https://github.com/owner/repository`, plus **Ref** and **Board directory** | The board directory at the resolved commit |
| **GitHub release ZIP** | `owner/repository`, then **Load release ZIPs** and a **Release and ZIP asset** | The selected release asset |

Git sources are GitHub only (`github.com`). There is no import from other Git
hosts or from a local Git repository; use **Local folder** for a checkout.

- **Folder** imports copy a validated snapshot into Subspace's private store.
  Rebuilding the original folder does not change the installed copy. Use
  **Edit… > Replace source…** to import it again. Removing a web Space does
  not delete original files or browsing data. The folder must not be inside
  the managed store (`Select an app folder outside the managed board store.`).
- **ZIP** imports accept a web Space at the ZIP root or inside one enclosing
  directory (`ZIP must contain a board at its root or in one enclosing
  directory.`). The **Choose…** picker accepts any file; the importer rejects
  anything that is not a readable ZIP.
- **GitHub repository**: a blank **Ref** uses the default branch. A branch
  name (optionally `branch:<name>`) tracks that branch; `tag:v1.0` and
  `commit:<full SHA>` pin a version and are never offered updates. A blank
  **Board directory** is the repository root. The URL form must be the
  repository root, without credentials, query or fragment. Subspace downloads
  the repository archive for the resolved commit, so the whole archive, not
  only the board directory, must pass the [package rules](#package-rules).
- **GitHub release ZIP**: the release must be published, not a draft, and
  the asset name must end in `.zip`. Prereleases are excluded unless
  **Include prereleases** is on. Updates match the exact asset name.
- GitHub operations need a GitHub account selected in Settings with verified,
  active membership of the repository's organization and read access to the
  repository. Personal repositories are not eligible. Changing accounts leaves
  installed files usable but discards in-progress work for the old account.
- Subspace never runs a build, repository hook or dependency installation.

**Install Weather example…** fills in the GitHub repository form for the
keyless UK-postcode [Weather web Space](../web-spaces/examples/weather/README.md):
repository `aiconsultancy/subspace`, board directory
`subspace-desktop/web-spaces/examples/weather`. It uses the same authorization
and validation as other imports. From a checkout, install the same folder as a
**Local folder**, or package it as a ZIP (see
[Package a web Space](web-spaces-guide.md#8-package-it)). The free hosted
weather endpoint is for non-commercial evaluation and prototyping; review the
package's service terms before other use.

### Updates

**Check for updates** applies to GitHub sources only. A tracked branch
(including the default branch) offers an update when it resolves to a
different commit. A release source offers the newest eligible release whose
assets include the exact same ZIP name and that was published after the
installed one. A check never installs anything: choose **Download update**,
review the package, then **Replace now**. The manifest `version` is shown but
does not decide whether an update is offered. Folder and ZIP sources are
updated with **Edit… > Replace source…**.

## Package rules

Every import is staged and validated before anything is installed. The rules
are in `spaces/web/src/install/` (`paths.rs`, `files.rs`, `archive.rs`,
`zip_header.rs`, `stage.rs`). An import that breaks one fails with the
message shown.

| Rule | Message |
| --- | --- |
| `index.html` is a regular file at the web Space root | `The board root needs a regular, readable index.html. Select the built app folder.` |
| `subspace-board.json` is a regular file at the root | `The board root needs a regular, readable subspace-board.json.` |
| Names use ASCII letters, digits, space and `-_.@()+,[]{}`, are not `.`, `..` or `.git`, do not end in `.` or space, are not Windows device names (`CON`, `PRN`, `AUX`, `NUL`, `COM0`–`COM9`, `LPT0`–`LPT9`) and are at most 255 bytes | `The package contains an unsafe or unsupported filename. Use portable ASCII names without .git or device names.` |
| A path is at most 1,024 bytes and relative | `Select a non-empty relative board path of at most 1024 bytes.` |
| At most 32 directory levels | `The package exceeds 32 directory levels.` |
| No two paths differ only by letter case | `The package has duplicate or platform-colliding paths.` |
| Only regular files and directories; no symbolic links, special files or (in a folder) hard-linked files | `Package links and special files are not supported.` / `Package files must be regular files, not links or special files.` |
| At most 10,000 entries | `Package contains more than 10,000 entries.` / `ZIP contains duplicate names or more than 10,000 entries.` |
| A file is at most 32 MiB | `A package file exceeds 32 MiB.` |
| All files together are at most 256 MiB | `Package expansion exceeds 256 MiB.` |
| A ZIP (or repository archive) is at most 64 MiB | `ZIP exceeds 64 MiB.` / `ZIP exceeds the 64 MiB download limit.` |
| A ZIP is regular: no ZIP64, no split disks | `Use a regular ZIP with at most 10,000 entries, without ZIP64 or split disks.` |

Installed files are made read-only. The importer checks the required root
files and the manifest, not every asset that JavaScript loads at run time.
When the web Space runs, a single served file is at most 64 MiB.

`scripts/package-web-space.py` applies the same rules to a folder before you
publish it and writes a reproducible ZIP; see
[Package it](web-spaces-guide.md#8-package-it).

## Declare an app

```json
{
  "name": "Notes",
  "version": "1.0.0",
  "permissions": ["settings", "badge", "context", "notifications"],
  "settings": [
    { "key": "name", "label": "Notebook", "type": "text", "default": "Personal" },
    { "key": "endpoint", "label": "Sync address", "type": "url" },
    { "key": "limit", "label": "Notes to keep", "type": "number", "default": 50 },
    { "key": "compact", "label": "Compact list", "type": "boolean", "default": false },
    { "key": "sort", "label": "Sort order", "type": "select", "options": ["Newest", "Oldest"], "default": "Newest" },
    { "key": "token", "label": "Sync token", "type": "secret" }
  ]
}
```

The rules are in `spaces/web/src/manifest.rs` and
`spaces/web/src/manifest/fields.rs`.

| Field | Required | Rule |
| --- | --- | --- |
| `name` | Yes | Text of 1 to 60 characters after trimming |
| `version` | Yes | Text of 1 to 40 characters after trimming; free-form, shown to users |
| `permissions` | No | A list of text: `notifications`, `settings`, `badge`, `context`, `secrets`, `save-secrets`, `issues` |
| `settings` | No | A list of at most 32 setting objects |

Unknown permission names produce a warning (`Unknown permission "camera"
ignored.`) and grant nothing. Other unknown fields are ignored. The file must
be UTF-8 JSON of at most 256 KiB (`subspace-board.json is larger than 256
KiB.`). An invalid manifest prevents the app from opening, and the Space's
settings page says `This app cannot open.` with the reason.

Each setting has:

- a unique `key`: 1 to 40 ASCII letters, digits, `_` or `-`, starting with a
  letter;
- a `label` of 1 to 60 characters;
- a `type`: `text`, `url`, `number`, `boolean`, `select` or `secret`;
- an optional `help` of up to 200 characters;
- an optional `default` of the matching kind: text for `text`, `url` and
  `select`, a number for `number`, `true` or `false` for `boolean`.

Select fields have 1 to 50 text `options` of 1 to 60 characters, and a
default must be one of them. Secret fields cannot have defaults. In Settings,
text, URL and number inputs accept up to 2000 characters, and URL inputs
accept HTTP and HTTPS addresses.

Manifest errors name the problem, prefixed with the file name, for example:

| Problem | Message after `subspace-board.json: ` |
| --- | --- |
| Not an object | `expected a JSON object.` |
| Not JSON | `invalid JSON (<parser detail>).` |
| Missing name | `expected name to be text of 1 to 60 characters.` |
| Missing version | `expected version to be text of 1 to 40 characters.` |
| `permissions` not a list | `expected permissions to be a list.` |
| Too many settings | `expected at most 32 settings.` |
| Bad key | `settings[0]: expected key to be 1 to 40 letters, digits, _ or -, starting with a letter.` |
| Bad type | `settings[0]: expected type to be text, url, number, boolean, select or secret.` |
| Secret with default | `settings[0]: a secret cannot have a default.` |
| Bad options | `settings[0]: expected 1 to 50 options of 1 to 60 characters.` |
| Default not an option | `settings[0]: expected default to be one of the options.` |
| Duplicate key | `settings[1]: the key name is used twice.` |

After installing, open **Settings > Spaces > [Space name]**. Each requested
permission has an **Allow this app to …** checkbox: **show notifications**,
**read its settings**, **show a badge on its tab**, **share context with
Copilot chat**, **read its saved secrets**, **save its own secrets** and
**read and open issues**.
Every permission starts off. Select the ones you trust and
save. Notification delivery also needs an enabled notification channel for
the Space in **Notifications** settings.

## Read settings and call Subspace

The main app document receives a frozen `window.subspace` object. Its
methods return promises. A denied call rejects with `Permission denied. Allow
this action in the board's Subspace settings.`

```javascript
const info = await window.subspace.info();
// { version: 1, permissions: ["settings", ...], notificationPermission: "denied" }

const settings = await window.subspace.settings.get();
await window.subspace.setBadge("3");
await window.subspace.setContext({
  label: "Selected note",
  content: "Page-provided note text"
});
await window.subspace.notify({ title: "Notes updated", body: "Three new notes" });

// Clear page-owned UI state.
await window.subspace.setBadge(null);
await window.subspace.setContext(null);
```

| Method | Permission | Result or limit |
| --- | --- | --- |
| `info()` | None | Bridge version, granted permissions and notification permission |
| `settings.get()` | `settings` | Declared non-secret saved values, or defaults when unset |
| `setBadge(textOrNull)` | `badge` | At most 8 characters, without control characters |
| `setContext(valueOrNull)` | `context` | Label up to 100 characters, content up to 8192 UTF-8 bytes |
| `notify({title, body})` | `notifications` | Title up to 100 characters, body up to 500, without control characters |
| `theme()` | None | The app's theme: tones, mode, id, name and fonts ([Follow the app's theme](#follow-the-apps-theme)) |
| `secrets.get()` | `secrets` | Saved values of the declared secrets and of the page's own ([Read and save the app's secrets](#read-and-save-the-apps-secrets)) |
| `secrets.set(key, valueOrNull)` | `save-secrets` | Saves or deletes one of the page's own secrets; at most 32, each up to 4096 characters, 10 saves in 10 seconds |
| `issues.scope()` | `issues` | The scope issue identifiers are recognised in, or `null` ([Show issues](#show-issues)) |
| `issues.resolve(identifiers)` | `issues` | One resolution per distinct identifier, at most 50 distinct, each up to 32 characters |
| `issues.open(identifier)` | `issues` | Opens the issue in the Space that answers issue requests |

Errors from the bridge (`spaces/web/src/bridge.rs` and
`spaces/web/src/bridge/init.js`):

| Message | Cause |
| --- | --- |
| `Permission denied. Allow this action in the board's Subspace settings.` | The permission is not requested or not allowed |
| `A badge must be at most 8 characters, without control characters.` | Badge too long |
| `Context is too long: use a label up to 100 characters and content up to 8192 bytes.` | Context too long |
| `Use a notification title up to 100 characters and body up to 500 characters.` | Notification too long |
| `Notifications are off. Choose a notification channel in Subspace Settings.` | No notification channel |
| `Too many notifications. Wait five seconds.` | More than one notification in five seconds |
| `Use a secret key of 1 to 40 letters, digits, _ or -, starting with a letter.` | `secrets.set` with a malformed key |
| `This key belongs to one of the app's settings, which only Settings changes. Choose another key.` | `secrets.set` with a key the manifest declares, secret or not |
| `Use a secret of 1 to 4096 characters, without control characters, or null to delete it.` | `secrets.set` with an empty, long or malformed value |
| `This app has saved 32 secrets, the most it may. Delete one before saving another.` | A 33rd secret of the page's own |
| `Too many secret saves. Wait a few seconds, then try again.` | More than 10 `secrets.set` calls in 10 seconds |
| `This Space changed in the config file. Review it in Settings, then try again.` | The config file now gives the Space another app folder or install source, or no longer allows `save-secrets` |
| `Open this Space to save a secret.` | `secrets.set` while another Space is shown or something covers this one, such as Settings |
| `Subspace could not save the secret. Check the config file in Settings, then try again.` | The config file could not be read or written, or the Space is no longer in it |
| `secrets.set expects a key and a string value, or null to delete it.` | `secrets.set` with a non-string key or value (a `TypeError`, before any request) |
| `Resolve at most 50 distinct issue identifiers.` | `issues.resolve` with more than 50 distinct identifiers |
| `Use issue identifiers of at most 32 characters, without control characters.` | An empty, long or malformed identifier |
| `No Space that answers issue requests recognises this issue identifier.` | `issues.open` with no scope, or an identifier whose project key is not in it |
| `issues.resolve expects an array of issue identifiers.` | `issues.resolve` with something other than an array (a `TypeError`, before any request) |
| `The app manifest changed. Restart Subspace, then review its permissions in Settings.` | The manifest changed under a running page |
| `Too many pending Subspace requests.` / `Subspace request is too large.` / `Subspace request timed out.` | Request limits below |

Unknown setting keys and secret fields never appear in `settings.get()`.
Unset fields without defaults are absent. A key also present in `secrets`
is excluded, even if the manifest later describes it as non-secret.

Native settings save values under `webBoards.<id>.settings` and secrets
under `secrets` in the private config file. Saved secrets never populate
inputs. Leave a secret input blank to keep it; replace or clear it
explicitly. Only `secrets.get()`, with the `secrets` permission, returns
them to the page, and `secrets.set()`, with `save-secrets`, lets the page
save its own beside them. They are not OS-keychain entries or encrypted app storage.

Setting and permission edits apply on **Save** without restarting. The page
can read the current values again. Settings and secrets have no change
event; only the theme and issues push events (below). Cancel leaves the
saved configuration unchanged.

## Follow the app's theme

```javascript
const theme = await window.subspace.theme();
window.addEventListener("subspace:theme", event => apply(event.detail));

function apply(theme) {
  const root = document.documentElement;
  root.style.colorScheme = theme.mode;
  for (const [name, value] of Object.entries(theme.tones)) {
    root.style.setProperty(`--${name}`, value);
  }
  root.style.fontFamily = theme.fonts.ui;
}
```

`theme()` needs no permission. It resolves to:

```typescript
type Theme = {
  id: string;            // the bundled theme file, e.g. "one-dark", "cupertino"; "custom" for a user's theme
  name: string;          // the variant's display name, e.g. "Subspace One Dark", "Cupertino Dark"; "Custom" for a user's theme
  mode: "light" | "dark";
  tones: {               // "#rrggbb"
    bg: string; pane: string; line: string; hair: string; select: string;
    fg: string; strong: string; muted: string; accent: string;
    red: string; orange: string; yellow: string; green: string;
    cyan: string; blue: string; magenta: string;
  };
  fonts: { ui: string; mono: string };   // CSS font-family values
};
```

The tones are the colours Subspace's own Spaces use for each tone, from the
active theme and its tone overrides. A translucent tone is flattened onto
`bg`, so every value is an opaque `#rrggbb`. `fonts.ui` and `fonts.mono` are
complete `font-family` values: the app's family, quoted, then generic
fallbacks (`system-ui, -apple-system, sans-serif` and
`ui-monospace, monospace`). The platform interface font is the fallbacks
alone. A family the app bundles but the system does not have falls back.

`subspace:theme` is a `CustomEvent` on `window` whose `detail` is a `Theme`.
It fires when the theme, light or dark mode, a tone or a font changes, and
only when the value differs from the last one sent to the page. A newly
loaded document gets no event until something changes: call `theme()` at
start.

## Read and save the app's secrets

```javascript
const { token } = await window.subspace.secrets.get();

// One token per account the user adds in the page:
await window.subspace.secrets.set("fleet-staging", tokenTheUserPasted);
await window.subspace.secrets.set("fleet-staging", null); // delete it
```

Reading needs the `secrets` permission (**read its saved secrets**). Saving
needs `save-secrets` (**save its own secrets**), a separate grant: a page
allowed only to read cannot save, replace or delete anything. A page that
saves and reads back its own secrets asks for both.

`secrets.get()` returns an object of the app's secrets, keyed by key: each
setting the manifest declares with `"type": "secret"` that has a saved
value, and every secret this app saved itself. A secret with no saved value,
or an empty one, is absent. A key the manifest declares as a non-secret
setting is never returned. Nothing else in the private config is reachable:
other web Spaces' secrets, service tokens, and secrets another app saved in
this Space stay out.

`secrets.set(key, value)` saves one of the page's own secrets, and `value`
`null` deletes it. It resolves once the secret is in the config file, and
the next `secrets.get()` returns it. The Space stays open: saving a secret
does not reload the page.

- `key` is 1 to 40 ASCII letters, digits, `_` or `-`, starting with a letter.
  A key the manifest declares, secret or not, is refused: declared secrets
  are the user's to set in Settings, and the page never replaces or deletes
  them.
- `value` is 1 to 4096 characters without control characters.
- The page may keep at most 32 secrets of its own. Replacing or deleting one
  of them is always allowed.
- At most 10 saves in any 10 seconds.
- The Space must be the one shown, with nothing over it, so a page saves only
  while the user can see it, and never behind Settings, whose own Save would
  then find the file changed under it.
- The config file must still give the Space this app folder, the same
  install source and the `save-secrets` grant when the secret is written. A
  Space pointed at another app, or whose grant was turned off, while the
  page was open is not written.

The page's secrets are saved with the Space's other secrets, under
`webBoards.<id>.secrets` in the private config file, and are removed with
the Space. Each belongs to the app that saved it, named by where the Space
installed it from, and a page sees only the secrets its own app saved:

| Installed from | The page's secrets belong to | An update keeps them |
| --- | --- | --- |
| A GitHub repository | The repository and board folder | Yes, from the same repository and folder |
| A GitHub release | The repository and asset name | Yes, for the same asset |
| A local folder | The folder it was copied from | Yes, when imported again from that folder |
| A ZIP file, or a `path` written into the config file | The app folder and its manifest, as grants are | No: a reinstall or a changed manifest starts with none |
| Built into the app, such as Fleet Console | The Space's ID | Yes, through every app update |

Installing an app from another source in the Space starts it with none of
its own, and the earlier app's stay hidden. Secrets saved by an earlier
Subspace, before they had owners, belong to the source the Space was
installed from; when Subspace did not record one, they stay hidden. Grants
start off again after any update. Settings never shows the values: the
Space's page says **This app has saved N secrets**, counting hidden ones
too, and **Clear** removes them all on Save (the declared secrets have their
own fields).

A page granted `secrets` holds its secrets in its own JavaScript memory, so
anything the page loads can read them. Keep the page's scripts in its own
files (the Content Security Policy already blocks inline and remote
scripts), send the secret only to the service it is for, and never put it
in a URL, `setContext`, a notification or the page's storage.

## Show issues

The `issues` permission forwards the Framework's `issues` board contract,
the one Channels (IRC) uses for its live issue tiles, to the page. The page
never names the Space that answers; in the Subspace App it is the Issues
Space while that is enabled and signed in.

```javascript
const scope = await subspace.issues.scope();
if (scope) {
  const resolutions = await subspace.issues.resolve(["ABC-827", "ENG-12"]);
  // render the available ones; pending ones settle with the event below
}
window.addEventListener("subspace:issues-changed", () => refresh());
tile.addEventListener("click", () => subspace.issues.open("ABC-827"));
```

```typescript
subspace.issues.scope(): Promise<IssueScope | null>
type IssueScope = {
  profileName: string;
  appOrigin: string;              // Subspace API base, including /api
  teamKeys: string[];              // project keys, e.g. ["ABC", "ENG"]
  linearWorkspace: string | null; // null with the Subspace API
};

subspace.issues.resolve(identifiers: string[]): Promise<Resolution[]>
type Resolution = {
  identifier: string;
  state: "available" | "unavailable" | "pending" | "signed-out" | "failed";
  summary?: IssueSummary;         // only when state is "available"
};
type IssueSummary = {
  identifier: string;
  title: string;
  url: string;
  state: {
    name: string;                 // e.g. "In Progress"
    kind: "triage" | "backlog" | "unstarted" | "started" | "completed" | "canceled" | "duplicate";
  };
  assignee: string | null;
  archived: boolean;
};

subspace.issues.open(identifier: string): Promise<void>
```

- `scope()` is `null` while no Space answers or while the answering Space
  has no active project scope, including when it is signed out. The scope
  omits profile and organisation IDs.
- `resolve()` answers once per distinct identifier, in the order given. It
  accepts at most 50 distinct identifiers of at most 32 characters each;
  repeats are dropped. `pending` means a read has started:
  `subspace:issues-changed` follows when it lands. `unavailable` never says
  whether an issue exists. Without a scope nothing is asked: every
  identifier is `signed-out` when a Space answers without a scope, and
  `unavailable` when no Space answers.
- `open()` opens the issue in the answering Space, as selecting an issue
  tile in Channels does, and switches to that Space. It needs an identifier
  whose project key is in `scope().teamKeys`. It uses the summary's navigation
  URL when available and otherwise builds an issue URL under the same API
  base. The web Space must be the one shown, which it is when the user clicks
  in it.
- `subspace:issues-changed` is a plain `Event` on `window`. It fires after
  each contract `changed`: when the answering Space starts or stops, when
  the scope changes (sign-in, sign-out, API base or project keys) and when
  pending reads finish. Ask for the scope and the identifiers you show
  again; keep nothing across it. It is sent only while `issues` is allowed.

The web Space keeps no issue content: summaries pass straight to the page,
and Subspace adds none of them to search or notifications.

## Page Notification adapter

```javascript
const permission = await Notification.requestPermission();
if (permission === "granted") {
  new Notification("Notes updated", { body: "Three new notes" });
}
```

`requestPermission()` reads the native decision. It does not display a web
prompt or grant anything. Delivery uses Subspace's configured notification
channel. The adapter reports `show` when the host accepts a request and
`error` when it rejects one. Acceptance does not prove that the OS displayed
an alert. At most one request per web Space is accepted every five seconds.

This is a page adapter, not Web Push. It has no service-worker integration,
notification actions, click/close callbacks or OS retraction. `close()` is
a no-op. `Notification.permission` updates after `info()` or
`requestPermission()`, so call one again after native permission edits.

## Sign a web Space in with the app's account

This section is for product builders. On macOS, a product can compose a
web Space whose site is signed in with the account under
**Settings › Account**, so the person signs in once. It is off unless a
product composes it: no setting, manifest field or page script turns it
on, and a build that does not compose it has no setting, control or state
for it. Only the in-app guide, which every build ships, mentions it, in
[Spaces signed in by the app](../spaces/web/help.md#spaces-signed-in-by-the-app),
which describes what the person sees in an app that includes one.

The product supplies a trusted native adapter that exchanges the account
for a session cookie the site issues, on the adapter's own HTTPS path. The
Space installs only that cookie in its view; the account's tokens never
reach the page. The API is compiled for macOS only, so keep the
composition in a module under `#[cfg(target_os = "macos")]`, in the
product crate beside its other registrations, and compose
`subspace_web_board::create`, or nothing, on other platforms. The names
below are fictional:

```rust
#[cfg(target_os = "macos")]
mod desk {
    use std::time::Duration;

    use subspace_framework::core::Secret;
    use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};
    use subspace_framework::shell::{AccountSession, BoardRegistration, account_session};
    use subspace_web_board::{
        NativeSession, SessionAccount, SessionCookie, SessionFailure, SessionGrant, SessionScope,
    };

    /// A Space cannot name the shell, so the product forwards its account.
    struct Account(AccountSession);

    impl SessionAccount for Account {
        fn generation(&self) -> u64 {
            self.0.generation()
        }

        fn is_current(&self) -> bool {
            self.0.is_current()
        }
    }

    pub(crate) fn registration() -> BoardRegistration {
        let page = WebBoard {
            id: "desk".into(),
            title: "Desk".into(),
            // What Open in browser opens: the site's ordinary page.
            source: WebBoardSource::Url("https://desk.example.com/".into()),
            settings: Default::default(),
            secrets: Default::default(),
            permissions: Default::default(),
        };
        BoardRegistration::configured::<subspace_web_board::WebBoard>(
            "desk",
            "Desk",
            "The team desk",
            move |window, cx| {
                let scope = SessionScope::new("https://desk.example.com/app/")
                    .and_then(|scope| scope.with_signal("session/required"))
                    .and_then(|scope| scope.with_reserved("session"));
                match scope {
                    Ok(scope) => {
                        let account = |cx: &gpui_kit::App| account_session(cx).map(Account);
                        let session = NativeSession::new(scope, account, sign_in);
                        subspace_web_board::create_with_session(&page, session, window, cx)
                    }
                    Err(_) => subspace_web_board::create(&page, window, cx),
                }
            },
        )
    }

    /// Runs off the UI thread. `desk_api` is the product's own client.
    fn sign_in(account: &Account) -> Result<SessionGrant, SessionFailure> {
        let token = account
            .0
            .token_for("desk", Duration::from_secs(240))
            .map_err(|_| SessionFailure::action("Sign in again under Settings › Account."))?;
        let session = desk_api::exchange(&token)
            .map_err(|_| SessionFailure::temporary("Desk is unreachable. Try again."))?;
        let revoked = session.clone();
        let cookie = SessionCookie::new("__Secure-Desk", Secret::new(session.value()));
        Ok(SessionGrant::new(cookie, session.lifetime())
            .with_revoke(move || desk_api::revoke(&revoked)))
    }
}
```

| Item | What it is |
| --- | --- |
| `SessionScope::new(home)` | The address the view starts at: `https://`, a host, no credentials, query or fragment, and a path ending in `/`. The path is the prefix every page must sit under. |
| `with_signal(route)` | A route under the home that the site sends the view to when it no longer accepts the session, such as `session/required`. |
| `with_reserved(route)` | A route under the home that the view never navigates to, such as the site's session endpoints, with the same route under a format suffix such as `.json`, and everything under either. |
| `SessionAccount` | The account a session belongs to: its generation, which changes with another account, and whether it is still current. Implement it on a newtype around the shell's `AccountSession`. |
| `NativeSession::new(scope, account, acquire)` | `account` reads the account signed in now, on the UI thread; `acquire` turns it into a grant, off the UI thread. |
| `with_start(route)` | Optional. Where the view starts, from the app's own state: `route` reads the app on the UI thread and gives a reference relative to the home, such as `?team=blue` or `teams/blue`, or `None` for the home itself. See [A start that follows the app](#what-the-space-does). |
| `SessionGrant::new(cookie, lifetime)` | The cookie and how long it lasts: at least `MIN_SESSION_LIFETIME` (4 minutes), or the Space refuses it, and at most `MAX_SESSION_LIFETIME` (24 hours). `with_revoke` ends it at the site; calling it again replaces the earlier revocation, which then never runs. |
| `SessionCookie::new(name, value)` | The cookie's name and opaque value. `with_path` narrows its path within the scope: the scope's path, with its final `/`, or a route under it. |
| `SessionFailure::action` or `temporary` | Why an attempt failed, in words for the person. A temporary failure of a renewal is retried. Never put a credential in it. |

### What the Space does

- **An isolated view.** The view has a new, in-memory WebKit store, not the
  per-board store other web Spaces keep on disk, and it goes when the view
  closes. The view has no app files, no bridge, no initialisation script and
  no developer tools.
- **The cookie, natively.** The Space checks the cookie and sets every
  attribute itself: host-only on the scope's host, the scope's path (or the
  cookie's narrower one), `Secure`, `HttpOnly`, `SameSite=Strict`, and
  expiring when the grant does. It installs it through `WKHTTPCookieStore`
  without blocking the UI thread, reads the store back to confirm every
  attribute and the expiry, and only then loads the scope. The value never
  reaches a script, an address, a log or a store on disk, and formatting a
  grant or cookie hides it. A cookie path outside the scope, or a name that
  is not a token, is the product's mistake: the Space logs it, without the
  value, and asks the person to open the site in their browser.
- **Only the scope.** The view starts on `about:blank` and then loads HTTPS
  pages on the home's origin under its path, and nothing else, redirects
  included; once a page in the scope has loaded, `about:blank` is refused
  too, so Back stops at the first page. The rule applies to every frame:
  an iframe from another site, `about:srcdoc`, `blob:` or `data:` is
  refused, and the status bar shows a Blocked notice. It refuses addresses
  with credentials, empty or dot segments, a backslash or `;`, and a
  percent-escape of an unreserved character, a separator, `%`, a control
  character or anything outside ASCII. Reserved routes and the signal
  match ignoring ASCII case, and a reserved route such as `session` also
  covers `session.json` or `session.html`, which many web frameworks send
  to the same handler. Reserved routes limit navigations only: a page in
  the scope can still `fetch` them, and the request carries the cookie.
  A pop-up never opens a window; a web page it asks for opens in the
  default browser, without the session.
- **Sign-in required.** While `account` returns no current account, the
  Space shows `Sign-in required` and starts no attempt.
- **A start that follows the app.** With `with_start`, the Space reads the
  route at every poll, ten times a second, so keep it to a lookup. When its
  answer changes, a view showing a page loads the new start at once, even
  while the session renews, and a session that shows no page yet, or that
  ended, starts there: **Retry** opens the start rather than the page the
  view showed. While the site refuses the session, after its signal, the
  view goes to the new start once a renewal succeeds. Until the session
  next renews or ends, the signal's return and **Retry** never go back to
  the page the move left, where a load the site sent to its signal leaves
  the view. **Go to the start page** goes there too. The Space joins the
  route to the home as a browser would. A route's path must be a plain
  path, as every page in the scope must: ASCII only, with no `;`, `..`,
  `//` or backslash, and no percent-escape the scope refuses. Carry other
  names, such as one with accents, in the query, percent-encoded. Never put
  a credential in a route: the address reaches the site and its logs. A
  route that leaves the scope, or names the signal or a reserved route, is
  the product's mistake: the Space logs it and starts at the home.
- **Renewal.** The Space acquires a new grant two minutes before the current
  one ends, or at half its life if that is shorter, and installs it in the
  same view; the old grant is then revoked, so each grant must be a session
  of its own at the site, not the same one extended. A grant shorter than
  `MIN_SESSION_LIFETIME` is refused, so renewals never follow each other at
  once. A temporary failure is retried three times, 30 seconds apart. A
  renewal the store refuses, or does not confirm within 10 seconds, ends
  the session and closes the view, which says
  `Cannot install the private session for this page. Try again.` with
  **Retry**: a late write could otherwise replace the live cookie with one
  whose grant was revoked. Keep the cookie's name and path across renewals,
  so each new cookie replaces the last.
- **The signal.** The view never loads the signal. The Space renews the
  session at once and loads the page the view shows again. If the site
  sends the view to the signal again within 30 seconds of that, or after
  renewing has failed for good, the Space closes the view and says the
  session ended, with **Retry**. While a failed renewal waits 30 seconds to
  try again, a signal waits with it rather than call the adapter at once.
  A signal that waited, or whose own renewal failed and is tried again,
  still loads the page again, and starts the 30 seconds, once a renewal
  succeeds.
- **Time limits.** An attempt that takes more than 60 seconds counts as a
  temporary failure, so **Retry** always works, and a grant it returns
  later is revoked. The Space cannot stop the adapter, so a hung adapter
  keeps its background thread until its own call returns: give its
  requests their own timeouts.
- **Revocation.** A grant is revoked once, on its own thread, when a renewal
  replaces it, the session ends or expires, the account changes or signs
  out, the Space closes, or a grant arrives for an attempt that no longer
  applies. Quitting revokes every live grant and waits at most two seconds
  for the revocations to finish. Bound the revocation's own request with a
  timeout.

After a renewal the Space dispatches `subspace-session-renewed` on the
page's `window`, with no data.

WebView2 has an asynchronous cookie store too, but the capability is not
built or tested there yet, so Windows and Linux builds have none of this
API.

## Trust and compatibility

Grants bind to the canonical app folder and the exact manifest bytes,
including whitespace. Changing either needs new grants, so replacing or
updating an imported web Space, which installs a new private folder, starts
with every permission off. Restart after a manifest change in a manually
configured folder, then review permissions. A web Space built into the app
is the exception: when an app update changes its manifest or moves its
folder, each permission keeps the choice made for it and a new one starts
allowed. The running old bridge refuses
calls once the manifest identity changes. Settings reads the current manifest
each time it opens. Changing only JavaScript or other app files does not
change the manifest fingerprint. Install updates only from a source you
trust, or revoke permissions before updating.

The native engine supplies the sender URL. Subspace also checks the current
page's exact origin. Another site cannot use the bridge after navigation.
Replies address one document, and badge/context state resets on navigation.
Pushed events (`subspace:theme`, `subspace:issues-changed`) go only to the
document the web Space shows at that moment, and only while it has the app's
exact origin: Subspace checks the page address before sending, and the event
script checks `location.origin` again in the page.
Chat treats page-provided context as untrusted data, not instructions.

Requests are limited to 16 KiB of UTF-8 JSON. There can be at most 32 pending
calls, each with a 10-second timeout. Calls can time out during navigation,
when the app closes, or when its bounded native event queue is full.

Manifest apps are served with this Content Security Policy:

```text
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline';
img-src 'self' data: blob: https:; font-src 'self' data:;
connect-src 'self' https: http://localhost:* http://127.0.0.1:* http://[::1]:*;
frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'
```

Load scripts from app files (classic or ES modules), not inline script blocks,
external script hosts or `eval`. Inline styles are allowed. Connections may
use the app origin, HTTPS or loopback HTTP; the destination must still allow
the app's cross-origin request. Frames, objects, base URL changes and form
navigation are blocked. React/Vite client routing and JavaScript form
handlers still work. Manifest-free folder apps retain their previous behavior
and receive no bridge.

The app is served from `subspace://<id>/` on macOS and
`https://subspace.<id>/` on Windows. Root-relative paths load from the
folder, and an address with no file extension that names no file returns
`index.html`.

The bridge provides no shell, process, filesystem, clipboard or arbitrary
native API, and no credential access beyond the app's own secrets once
`secrets` is allowed. macOS uses WKWebView and Windows uses
WebView2. Linux has no web view and shows a message instead of the page.
Windows CI builds and tests the code;
native Windows runtime validation remains outstanding.
