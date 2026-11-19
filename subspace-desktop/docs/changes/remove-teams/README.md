# Change: remove the Teams board and Microsoft 365 crates

## Intent

The Teams board and its Microsoft 365 clients should no longer be part of the
repository. Remove them completely, as the ADO, Apiiro, Sonar, Code Q, Dev
Tunnel and Azure Costs boards were removed, and keep every other board and
check working.

## Spec

Removed packages:

| Path | Package |
| --- | --- |
| `boards/teams` | `subspace-teams-board` (board, help guide, logo) |
| `boards/teams/crates/m365-auth-gpui` | `subspace-m365-auth-gpui` (native macOS sign-in window, Objective-C build script, page hook and its Node test) |
| `clients/m365` | `subspace-client-m365` (Graph and Teams API client) |
| `framework/services/m365-auth` | `subspace-m365-auth` (token capture and cache) |

Removed workspace dependencies used only by those packages: `cc`, `markdown`,
`strum` and `tungstenite` (with `sha1` from the lockfile). `markdown` and
`strum` stay in `Cargo.lock` as transitive dependencies of other crates.
`wry`/`gpui-wry` stay for the web board.

Other removals and edits:

- The Subspace product no longer registers or depends on Teams; the product
  descriptor drops the `teams` board, and the artwork fingerprint test drops
  `logos/teams.svg`.
- `mise run auth-hook-test`, `make auth-hook-test` and their `make check`
  dependency are gone.
- `deny.toml`: the two `lb-wry` advisories now name the web board as their
  reason. No advisory entry applied only to the removed crates.
- `docs/teams.md` and its table-of-contents entries are gone. README, AGENTS.md,
  the agent chat, feature flag, MCP, report-issue, search, product and UI
  component docs no longer mention Teams. Shared-component "used in" notes
  name the boards that still use them.
- Tests that used `teams` only as a board ID fixture now use `irc` (or
  `terminal` in the Settings connection fixture, to stay clear of IRC's own
  settings work). Architecture-check fixtures use `spaces/irc`.
- The architecture check's toolbar file list drops the Teams toolbar.
- `scripts/test-product-interfaces.sh` no longer expects a `teams` board or
  board command.

- Dead shared code that only Teams used, found by rustqual: the
  `images::ImageViewer` modal (with its Escape binding), `images::thumbnail`
  and `images::decode_avatar`. `images` now holds bounded decoding only.

Kept on purpose:

- Generic shared surfaces with no current user (`avatar`,
  `ListDashboard::extend_row_menu`, `ListDashboard::panel_header_label`) stay.
  The Component gallery still shows the avatars, marked as unused, as it does
  for other unused shared widgets.
- `spaces/github` keeps "your teams" (GitHub review teams).
- Historical change records under `docs/changes` still mention Teams.

Behaviour change: the Teams board, its `teams_read` and `teams_write` agent
and MCP tools, and the `subspace teams` command no longer exist. A saved
`"boards": {"teams": ...}` entry in `config.json` is ignored like any other
unknown board ID.

## Delivery

No visible UI of a remaining board changes, so there are no screenshots. The
Component gallery's "used in" notes change text only.

Checks: architecture, board-docs, cargo-shear, clippy, the workspace tests,
script, web-bridge and Weather tests, `make interfaces`, `mise run quality`
(0 findings) and `mise run audit` pass. `make check`'s format step reported
only IRC settings files that were being edited at the same time.

## Tasks

- [x] Delete the board, client and auth crates and their workspace entries.
- [x] Remove product registration, descriptor entries and tests.
- [x] Remove the auth hook test task and Make target.
- [x] Update scripts, architecture checks and interface tests.
- [x] Update README, AGENTS.md, docs and tables of contents.
- [x] Rename Teams fixture IDs in tests.
- [x] Run the checks.
