# Delivery: board-to-board contracts

Back to the [change records](../README.md).

## Scope

Phases 1 and 2 of the [delivery plan](tasks.md): the contract mechanism and
the `issues` contract definition, with the placement decision for
`framework/services/issues` ([spec](spec.md) section 6). No board declares
or uses a contract yet; phases 3 to 7 adopt it, including the items handed
over by the Issues workspace work (slices 4, 8 and 12 and the avatar support change's
cross-board part), which [NETT issue workspace tasks](../nett-issue-workspace/tasks.md)
now points here.

Nothing Subspace shows changes, so there are no screenshots.

Phase 4 (IRC adopts the `issues` contract) is delivered in
[IRC issue references](../irc-issue-references/delivery.md), with its
screenshots.

## What changed

| Area | Files |
| --- | --- |
| Contracts crate | `framework/services/contracts/` (`lib.rs`, `issues.rs`, `tests.rs`); workspace dependency in `Cargo.toml` |
| Declarations and broker | `framework/board-kit/dashboard/src/contracts.rs`, `contracts/declaration.rs`, `contracts/broker.rs`, `contracts/tests.rs`, `contracts/tests/{events,requests}.rs`; `Dashboard::contracts` in `lib.rs` |
| Test support | `framework/board-kit/dashboard/src/test_support/contracts.rs` (`Recorder`) |
| Host | `BoardRegistration::contracts` (`registry.rs`, `registry/configured.rs`); routing installed in `registry/background.rs`; checks moved to `registry/validation.rs`; tests in `registry/background/contracts_tests.rs`; fixture literals in `search_runtime.rs` and `settings/navigation_tests.rs` |
| Product | `the_documented_contract_inventory_matches_the_registrations` in `products/subspace/src/main.rs` |
| Docs | `docs/board-integration.md` (Communicating with other boards, audit, gaps), `docs/board-checklist.md`, `docs/README.md`, `AGENTS.md` (Structure), `docs/changes/nett-issue-workspace/tasks.md` (moved items), this record |

## Checks

Run on `feat/subspace-desktop` at `f2e1bd8` with this change and the
concurrent IRC search and history work in the tree:

- `make check`: passed (architecture, board docs, documentation index,
  format, cargo-shear, clippy with warnings denied, workspace tests, script
  fixtures).
- `make interfaces`: passed.
- `mise run quality`: no findings in this change's files. The run reports
  two findings, both in the concurrent IRC work (`spaces/irc/src/hub.rs`
  `SRP_STRUCT`, `spaces/irc/src/lib.rs` stale suppression).
- New tests: 4 in `subspace-contracts`, 11 in `subspace-dashboard` (one with
  `test-support`), 3 in `subspace-shell`, 1 in `subspace`.

## Phase 3: Issues responder

Part of the issue workspace implementation. The Issues board now answers `current-scope` and `resolve`
from its retained NETT hub and publishes `changed` at start, stop and reference
projection changes. This connects IRC's existing recognition and tooltip
presentation to the real board responder: with a signed-in loopback fixture
hub, references first resolve as loading, then show the fixture title, state,
tracker/archive markers and assignee when the per-reference read lands.
Cached 404s are unavailable; recorded read errors are failed. Signed out or
without an eligible organisation, the scope is absent. Disabling the board
drops both weak broker endpoints and IRC returns to plain references.

The existing 100 ms view timer moves to the retained responder; the visible
view observes it for its own hub updates. This lets IRC receive changes while
the Issues view is hidden, without adding a timer or thread. App origin comes
from the hub profile's API URL, including the loopback fixture's origin.
The reference card now retains just the assignee display name needed by the
contract summary. Cards, snapshots and broker answers remain memory-only
and cannot be formatted through `Debug` or serialised.

`Hub::publish` and the three free `hub::irc_*` registry functions are removed.
The client retains only the weak handle needed by `Hub::start` to reuse a
live hub. Real and debug fixture hubs use the same board attachment path.
The member `Hub::irc_snapshot` is the board's internal reference projection;
IRC has no client or board dependency.

This delivery uses the existing per-reference read. The responder batch read
with `POST /orgs/{orgId}/references/resolve` (the batch reference endpoint) is the next PR.
`BoardNavigation`, opening the issue in the Issues board and board-kit's
reference plugin remain outside both deliveries.

### Verification and evidence

`spaces/issues/src/tests/contracts.rs` exercises the real responder through
the real broker with a fixture asker and recorder: declarations; start,
scope-change and stop events while no view exists; unchanged projections;
all five resolution states; fallback team keys; profile isolation; summary
fields; missing organisation and pack-off scopes; unavailable requests after
stop; no card title or assignee in logs and no state-file write. Existing
client privacy, lifecycle, queue and reference tests and IRC's stand-in
recognition/tooltip tests remain in the workspace gate.

Screenshot capture is blocked on this Azure Linux worker: neither `DISPLAY`
nor `WAYLAND_DISPLAY` is set, and GPUI's Linux backend does not implement
`PlatformWindow::render_to_image`, which the repository's capture tooling
uses (the default returns “render_to_image not implemented for this
platform”). No stand-in screenshot is presented as evidence from the real
responder. Capture of IRC references and a tooltip with the real loopback
fixture hub remains for a supported desktop before visual approval. No live
NETT or WorkOS calls were made.

Gate results and the published head are recorded in the PR body prepared
with this delivery. The regenerated quality baseline changes only the function
count, from 15,061 to 15,125; every finding, warning and score field is unchanged.

### Board checklist: issues (phase 3)

- [x] Identity, product registration, artwork and retention: existing board;
  no ID or product changes; stopping disables reads and drops the responder.
- [x] List hooks, toolbar, status, badge, palette, keys, menus, empty/error
  states and quit warnings: existing behavior; no list or detail layout change.
- [x] Settings description, connection editor and feature flags: existing
  Issues profile editor; no new setting or experiment.
- [x] Stored choices: existing preferences; no new stored contract data.
- [x] Framework store and store-first refresh: not applicable because issue
  content must stay in memory. Reference TTLs and demand-driven reads remain.
- [x] Automatic refresh: existing reads only; the retained timer checks
  versions, never refreshes the network. Nothing blocks the UI thread.
- [x] Item/recent search, full-text index and indexed references: not
  applicable because this board keeps private issue content out of search.
- [x] Agent read/write tools, app-only tools and MCP documentation: not
  applicable because the Issues board exposes no tools or CLI actions.
- [x] Chat context: no issue content is attached, as existing privacy tests verify.
- [x] Notifications and badges: not applicable because this responder emits
  only the content-free broker `changed` event, with no user notice.
- [x] Help and documentation links: existing guide; developer contract
  description, inventory and audit updated; no stale “no board answers” text.
- [x] Contracts: both responders and `changed` declared, attached, tested and
  listed in Declared contracts; other boards use only the broker.
- [x] Framework state: board-owned global retains the hub/entity; saved
  `Services` still reaches the hub; no cross-board entity or global access.
- [x] Links, logging and diagnostics: summary URL uses the fixture/profile
  origin; existing link opening and diagnostics; no payload logged or written.
- [x] Headless testing: real broker, recorder, fixture HTTP and existing
  interaction/privacy tests cover the changed integration.
- [ ] Screenshots: blocked by Linux capture support, as documented above;
  a supported desktop capture is still required for visual approval.
