# Web Space bridge: theme, secrets and issues

Back to the [change records](../README.md).

## Intent

A first-party web Space should look like the rest of Subspace, read the
API token its user saved for it, and show live issue tiles the way
Channels (IRC) does. The bridge had none of these: pages could not see the
theme, secrets never reached the page, and only boards compiled into the
app could follow the `issues` contract.

## Spec

- `subspace.theme()` needs no permission. It returns the theme's id (the
  bundled theme file, or `custom`), its variant name, light or dark mode,
  the sixteen dashboard tones as `#rrggbb` (translucent tones flattened onto
  `bg`) and CSS `font-family` values for the interface and monospaced
  fonts. `subspace:theme` (`CustomEvent`, `detail` a `Theme`) fires on
  `window` when any of them changes and differs from the last value sent.
- `subspace.secrets.get()` needs the new `secrets` permission (**Allow this
  app to read its saved secrets**). It returns the saved, non-empty values
  of the settings the manifest declares as `secret`, and nothing else.
- `subspace.issues.scope()`, `issues.resolve(identifiers)` and
  `issues.open(identifier)` need the new `issues` permission (**Allow this
  app to read and open issues**). They forward the Framework's `issues`
  contract: `current-scope`, `resolve` (at most 50 distinct identifiers) and
  the shell's board navigation that an IRC tile click uses.
  `subspace:issues-changed` (a plain `Event`) fires after each contract
  `changed` while `issues` is allowed. Resolution states and state kinds map
  to the lowercase strings in the reference.
- The Issues hub supplies the active Subspace API base and project keys as
  `appOrigin` and `teamKeys`. The bridge keeps `linearWorkspace` in the
  scope payload with a `null` value. Opening a recognised issue uses the
  same API and profile binding as Channels.
- Both permissions start off, bind to the manifest fingerprint like the
  others, and are reported by `info().permissions` when allowed. Request
  limits (16 KiB, 32 pending, 10 s) and exact-origin checks are unchanged.
  Pushed events run only in the document the web Space shows, after a
  native check of its address and a `location.origin` check in the page.
- Framework: `subspace_dashboard::ThemeId` names the active theme's bundled
  file (set by the host with each theme it applies), and
  `BoardRegistration::configured` now captures the type's `contracts()`, so
  every configured web Space declares the `issues` roles under its own ID.

The full API is in the
[reference](../../web-spaces-reference.md#follow-the-apps-theme).

## Delivery

- Unit tests: permission parsing and labels (`manifest.rs`), secrets
  filtering, grant gating and issue validation (`bridge/tests.rs`), theme
  JSON and font families (`bridge/theme.rs`), issue JSON mapping,
  navigation and the broker round trip with a fixture responder
  (`bridge/issues.rs`), protocol parsing and event scripts
  (`bridge/protocol.rs`), and the JavaScript methods and pushed events
  (`tests/bridge.test.mjs`). The Framework's appearance snapshot test covers
  `ThemeId`.
- Settings shows the two new **Allow this app to …** checkboxes only for a
  manifest that requests them; they are drawn by the existing permission
  rows. Screenshots are still to capture with `make capture` before a pull
  request.
- Not checked in a running app: WebKit delivery of pushed events and
  navigation to the Issues Space from a web page.

## Board checklist: web (changed parts)

- [x] Contracts: subscribes `issues@1/changed`, requests
  `issues@1/current-scope` and `issues@1/resolve`, declared in
  `contracts()`; the board view subscribes when an app that asks for
  `issues` opens; recorded in
  [Declared contracts](../../spaces-audit.md#declared-contracts).
- [x] Framework state it follows: the GPUI Kit `Theme` and `ThemeId`
  globals; no other Space's crate or globals.
- [x] No tools, search or notifications carry issue content; nothing is
  stored.
- [x] `help.md`, the reference, the guide and the `web-space` skill updated.
- [ ] Screenshots of the new Settings checkboxes from the capture build.

## Tasks

- [x] `secrets` and `issues` permissions with Settings labels.
- [x] `theme()` and the `subspace:theme` push.
- [x] `secrets.get()`.
- [x] `issues.scope`, `issues.resolve`, `issues.open` and
  `subspace:issues-changed`.
- [x] Framework `ThemeId` and configured-board contracts.
- [x] Docs, help, skill, packaging script and tests.
- [ ] Capture screenshots for the pull request.
