# IRC nickname status protocol (PR 1)

## Intent

Agents publish nickname-wide `status` metadata while working. The IRC client
must learn and report it so the Channels Space can later place a temporary
row in the conversation where the nickname is typing. This slice changes
only the protocol foundation. No visible change; the status row follows in
PR 2.

The captain's settled points dated 17 November 2026 govern this work:
typing gates the future row, people and agents follow the same rules, no
members-panel status, status editor or Stop button. The Channels reducer
explicitly ignores the new event for now.

## Spec

- Request `draft/metadata-2` only with its `batch` dependency available.
  After acknowledgment and registration, send `METADATA * SUB status`.
  Support late capability activation and stop on capability removal.
- Read 761 values, 766 deletions and draft `METADATA` notifications, including
  streaming `metadata` batches and join snapshots. Ignore other keys and
  channel targets. Handle 770–774 without conversation lines; obey 774's
  target and delay for SYNC, including Ergo's `*ALL` initial snapshot.
- Emit `Event::NickStatus { network, nick, status: Option<String> }` only
  for a set, change or clear. Scope values to a connection and compare nicks
  with its case mapping; retain one spelling for updates and clears.
- Clear on QUIT, NICK, deletion, last known shared channel PART/KICK, loss
  of either required capability, and connection end. Emit clears, and never
  persist statuses in reconnect carry data. Existing member maps suffice;
  unrelated departures cannot discard statuses outside those maps.
- Clean at ingestion: strip IRC formatting/colour arguments, remove controls
  and bidi overrides, flatten line separators, trim, cap at 80 Unicode
  characters including the ellipsis, and treat cleaned emptiness as a clear.
  The workspace has no segmentation dependency. Keep Markdown, links,
  mentions and issue references literal; consumers must use plain text.

### Protocol sources verified

- [IRCv3 metadata draft](https://ircv3.net/specs/extensions/metadata):
  `batch` dependency; subscription; notification, numeric and deletion
  syntax; batch targets; deferred synchronization.
- [Ergo v2.19.1 metadata.go](https://github.com/ergochat/ergo/blob/v2.19.1/irc/metadata.go):
  `broadcastMetadataUpdate` sends 761/766; `syncChannelMetadata` streams member
  values; `syncAllMetadata` includes shared users and monitored nicknames.
- [Ergo v2.19.1 handlers.go](https://github.com/ergochat/ergo/blob/v2.19.1/irc/handlers.go):
  SET replies use 761/766; SUB returns 770 followed by 774 `*ALL 0` after
  registration; `METADATA *ALL SYNC` fetches subscribed current values.
- Local Ergo 2.19.1 fixture: set, change, clear and a join during an existing
  status all deliver the expected client events. The release default already
  enables metadata (100 subscriptions), so no server configuration change
  is needed.

## Tasks

- [x] Read the ticket and binding captain comment; verify current base.
- [x] Add protocol/state tests before implementation (initial red build:
  missing status event, state and lifecycle API).
- [x] Implement capability negotiation, subscription, parsing and deferred sync.
- [x] Add connection-scoped state, deduplication and stale-value clears.
- [x] Test pure text ingestion and preserve literal plain text.
- [x] Keep exhaustive event matches compiling; explicitly ignore in Channels.
- [x] Add and run local Ergo propagation and join-snapshot integration tests.
- [x] Update IRC documentation and this record.
- [x] Complete root checks, quality/findings, interfaces and audits.
- [x] Prepare the captain's PR body and signed publication procedure; require
  complete pre-push hooks before publishing. The PR body records the final
  hook result and signed head. Do not open the PR from this slot.

### Board checklist: Channels (`irc`)

All Framework integration items are not applicable to this protocol-only
slice: identity/registration/artwork/retention; layout/components/toolbar/
status/badges/busy/palette/keys/menus/dialogs/empty states/quit warnings;
settings/connections/feature flags; storage/refresh; search/index metadata;
tools/MCP/CLI; chat context; notifications; help surfaces; contracts/globals;
and links are unchanged. The reducer ignores `NickStatus` without creating
buffers, rows, attention or persisted data. No new UI component, gallery
card, interaction, layout invariant or screenshot is needed. Protocol
logic and socket lifecycle have pure and local-fixture tests; errors log
only a command/code, never metadata contents or credentials.

## Delivery

Base: `bfe047c4e28566fd11e6a384ce48ff35de534234`, on
the nickname status feature branch, targeting `feat/subspace-desktop`.

Moved the existing build directory once, with the captain's literal command,
from `.local/pr-nav-framework/target` to `.local/irc-agent-status/target`. No second
full target directory was created.
Builds use `COPILOT_SKIP_CLI_DOWNLOAD=1`, `MBX_DISABLE=1`,
`MBX_RESTORE_HARDLINK=false`, `RUST_TEST_THREADS=1`, `CARGO_BUILD_JOBS=4`
and the absolute new `CARGO_TARGET_DIR`.

The first root-check attempt stopped at a dangling Framework `target`
symlink. The existing packaging script ignores `CARGO_TARGET_DIR` for its
verification folder and invokes cleanup of extracted Framework copies in
`CARGO_HOME` before packaging. Subsequent gates use an isolated
`.local/irc-agent-status/cargo-home`: upstream registry/git caches are reused through
links, while temporary registry sources stay private. The worktree's ignored
Framework `target` and `package-build` links point at the reused target, so
verification creates no second full build folder. The packaging script and
all test selections remain unchanged.

Focused verification: 148 IRC unit tests, all four local Ergo integration
tests and the example test pass against `127.0.0.1:17638`. Socket loss and
explicit owner disconnect each emit exactly one clear. Both projects'
quality and audit gates pass. Desktop findings are empty; the Framework
prints existing chart magic-number diagnostics but adds no findings to its
quality baseline. Only the desktop baseline is updated: its function count
changes from 7,904 to 7,959, with every other field identical. No Framework
source or baseline changes are part of this PR.

Full root `make check` passes: 4,268 Rust tests passed, zero failed, five
existing ignores; both projects' architecture, documentation, format,
unused-dependency and clippy gates; script/JavaScript fixtures; Framework
README examples and optional Report Issue checks; and all 24 packaged-crate
verifications plus metadata/licence checks. No manual test exclusions were
used. Existing macOS bundle fixtures are conditional and do not run on Linux.

Desktop `make interfaces` passes against the real headless product with
isolated CLI/MCP fixtures. The Framework has no separate `interfaces`
target; its facade examples and packaged public interfaces are checked by
the root gate. Terminal code and Framework APIs consumed by it are unchanged,
so the separate terminal feature gate does not apply.

Publication requires the complete pre-push hooks on the final reviewed local
commit, GitHub App signatures, equal reviewed/published trees, linear
parents, a clean task checkout and empty captain Git identity/worktree
overrides. The captain's prepared PR body records that final receipt.
No pull request is opened by this worker. Logs live in `.local/irc-agent-status/`.

No visible change; the status row follows in PR 2.
