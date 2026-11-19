# Issues board checklist

Part B owns `spaces/issues/**`. Part D should copy this checklist into the
shared change record. No git state was changed.

## Identity and registration

- [x] Board ID `issues`, title `Issues` and artwork key are preserved; descriptions now name Subspace API.
- [x] Existing product registrations remain the composition point (Part C owns them); artwork remains declared by the board.
- [x] `RETAIN_WHEN_DISABLED` stays off: the board owns no external processes.

## Layout and UI

- [x] Grouped lists retain `ListDashboard`; full-width detail preserves list selection.
- [x] Controls use shared actions, fields, menus, tabs and Composer; no new shared-control treatment or custom component.
- [x] Project picker, status, busy state and Activity sections use the new client snapshots. No board toolbar profile picker remains.
- [x] Palette keeps list/filter/navigation actions and exposes relation paging and discussion focus/view changes.
- [x] Board keys remain scoped; profile and image-viewer keys were removed; help matches the remaining commands.
- [x] Anchored menus report `Modal::Menu` and dismiss through the existing shared owner. Sign-in/editor/viewer dialogs were deleted.
- [x] Loading, empty, edge-access and network states offer the appropriate retry. First-load copy allows a minute for wake-up.
- [x] Quit warns about unsent or unconfirmed text drafts.

## Settings and configuration

- [x] Settings description names reads and text comments; notifications and background updates remain off.
- [x] Board-owned Settings has no inputs or saved connection choices. It shows the API address, sign-in guidance and connection state.
- [x] No experiment added. Credentials come from the shared account in Settings › Account; debug fixtures use the client's loopback seam.

## Data, refresh and search

- [x] Tab, project, sub-issue visibility and group folds use framework preferences. Retired profile/team preferences are not read.
- [x] Persistent data store/cache: not applicable; issue content and comment drafts intentionally stay in memory.
- [x] Store-first display: not applicable because content is not persisted. In-session snapshots preserve list selection.
- [x] Scheduled background refresh: not applicable; visible activation/reload drives reads in the hub. No blocking client call runs in render.
- [x] Scoped issue search remains debounced and exact-identifier aware.
- [x] Framework item search/Opened, full-text index and index metadata: intentionally absent under the retained issue-content privacy boundary.

## Agent tools, chat, notifications and contracts

- [x] No agent tools or MCP/CLI actions: this board keeps issue content out of agent context.
- [x] App-only tools and tool approvals: not applicable because no tools are registered.
- [x] `agent_context` stays empty; row and feedback menus keep chat off.
- [x] Notifications, attention badges and process notices: not applicable; no background process work is advertised.
- [x] `CurrentScope`, `Resolve` and `Changed` remain declared and routed through the broker. The scope contains API origin and project keys only.
- [x] Retained hub state is board-owned; no dependency on another board's entities or preferences.

## Links, diagnostics and help

- [x] Issue navigation accepts only the current API's versioned issue path and pinned profile; legacy app-link recognition was removed.
- [x] External Markdown links retain the shared safe-link path. Media links become display text.
- [x] Read failures use board-owned copy; diagnostic details omit bodies, queries, identifiers and credentials.
- [x] No diagnostic config path: there is no Issues connection config file anymore.
- [x] `help.md` uses the required guide headings and documents projects, sign-in, text posting and uncertain outcomes.
- [ ] Part D: update guide/developer links and the shared change record under `docs/**` (outside Part B ownership).

## Testing and evidence

- [x] Pure logic and native headless coverage updated: lists, filters, folds, paging, selection, keys, detail, timeline, settings, Markdown and comment-check/repost flow.
- [ ] Complete package validation after Part A finishes the client; any remaining cross-part failures are reported with exact tests in the handoff.
- [ ] Resolve workspace quality findings outside this board and run the full shared gates.
- [ ] Part D/program owner: capture Cupertino Dark before/after and changed states through `make capture`, inspect fixture-only images, store them under the shared change's evidence folder, and embed them in the eventual PR. No PR is ready without this evidence.

Capture attempt: the isolated capture build compiled, but macOS LaunchServices
refused the generated bundle with `kLSNoExecutableErr` (`NSOSStatusErrorDomain
-10827`). No images were captured. Resolve this launcher problem and capture
both comparison states before marking UI evidence complete.

## Latest validation

- `mise exec -- cargo test -p subspace-issues-board`: 90 passed, two client-behaviour regressions remain. `tests::detail::child_paging_recovers_and_offline_keeps_only_the_open_issue` loses the cached detail on `Hub::reload_issue`; `tests::first_pages::cursor_recovery_reads_dones_first_page_again` waits for recovery after a 400 cursor response. Keep both assertions and fix the client in Part A.
- `mise exec -- cargo test -p subspace-issues-board --test markdown --test prepared`: all six passed.
- `mise exec -- cargo clippy -p subspace-issues-board --tests -- -D warnings`: passed.
- `mise exec -- cargo fmt -p subspace-issues-board -- --check`: passed.
- `mise run audit`: passed for all three lockfiles (accepted-advisory warnings only).
- `rustqual . --findings`: no findings in `spaces/issues/**`.
- `mise run quality`: fails on two unused shared Composer methods outside this board (`set_images`, `set_accessory`). The baseline was not changed.
- `make check`: board docs and formatting checks ran, but cargo-shear stops the gate on unused workspace dependencies `async-channel` and `keyring` in root `Cargo.toml`.
- Part C also needs to update the product assertion at `products/subspace/src/main.rs` that still expects `issues.settings_editor.is_none()`; Issues now requires its information-only settings editor.
