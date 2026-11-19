# Delivery: NETT issue workspace

## Final review — P1 lifecycle repair

- Reproduced the blocked-save lifecycle failure before changing production code.
  Start, Cancel and Stop now take only the state lock. Sign-out clears scoped
  content, organisations and IRC immediately, then waits for credential deletion.
- Installation checks the attempt under the session lock before saving and
  again before publishing. A retired installation drops access and deletes only
  its matching refresh credential under that same lock. Failed deletes retain
  the existing retry-on-next-read behaviour.
- Held-save tests cover all four actions, empty credentials/access after rollback,
  and responsive snapshot/version/IRC reads. Bounds are seconds, not 250 ms.
  All nine sign-in tests pass. Full workspace gates follow the P2 repairs.
- Clarification: the prescribed sign-out sequence cannot return while the held
  save owns the session lock. Its state publication is prompt; its documented
  blocking credential delete runs off the UI thread. Stop runs synchronously
  from stop_background. This supersedes the earlier lifecycle-lock guidance.

## Final review — P2 repairs

1. IRC recognition remains enabled with an organisation through permission,
   membership, edge, network and other read errors. Signed-out, expired-session,
   pack-off and stopped states disable recognition.
2. Both keychain read and rotation-save failures map to KeychainUnavailable.
   Without remaining access they clear scoped content and organisations.
   This includes team-only reads; a failing-store regression covers both paths
   without relying on a concurrent list read to reveal the account error.
3. `HubError::AlreadySignedIn` rejects a new device flow for an active/restoring
   session without taking its lock. The guard is maintained under the state
   lock around reader restoration/refresh and successful installation. Pending
   device attempts can still supersede each other. The spec does not define
   reauthentication while signed in, so this follows the offered fallback.
4. A later read signing out resets Complete to Idle, preserving Starting/Waiting.
5. Only transport failures/timeouts give Offline with content. Invalid WorkOS
   responses and unexpected statuses give Network, as requested; this supersedes
   the earlier review's broader mapping below.
6. load_more is a no-op while the list is loading. A held-reload regression
   confirms the previous list's cursor is never queued into the new revision.
7. Both initial reads and 401 retries recheck generation after acquiring the
   session lock. A stopped reader cannot begin another restore/refresh. Restart
   uses a fresh Shared: reviving its irrevocably stopped queue would also revive
   old handles/readers and requires a separate lifecycle design.
8. A crate-private session-factory seam tests registration entirely offline:
   start/reuse, publish, stop/None and replacement. Store-held reader bounds are
   seconds and Flow::finish has a join deadline, so a deadlock fails the test.
9. `hub::irc_version() -> Option<u64>` checks projection metadata without cloning
   cards. Snapshot access and the version read share the same semantic version.
   The public IrcSnapshot type is unchanged; its definition moved to a small
   state submodule to keep the module within the quality limit.

Ten failing regressions were reproduced before the functional repairs. All 42
hub tests pass at 64 test threads. Public API changes since 8ef2f88 are only
`HubError::AlreadySignedIn` and `hub::irc_version() -> Option<u64>`; existing
signatures remain unchanged. Final workspace gates are recorded after merging
the requested mention notification integration revision.

Final merged-tree verification (integration parent
`193b47dc17b4ef85f6a1ef9af01d560ccc2f3f1b`):

- `mise exec -- make check`: passes, **2,740 Rust tests**, five existing ignores;
  architecture, board guides, formatting, unused dependencies, strict Clippy,
  script fixtures and offline NETT server contracts pass.
- `mise run quality`: passes at 100%, zero blocking findings and no new findings
  in changed source. The quality baseline records the added functions.
- `mise exec -- cargo check -p subspace-nett-client --release`: passes, including
  the debug-only fixture boundary.
- `mise run audit`: passes for all three lockfiles. The integration merge updates
  the workspace version/lockfiles; no dependencies were added by the hub fixes.
- The client has **92 passing tests**, including **42 hub/queue tests**; all NETT
  and WorkOS test requests stay behind the loopback guard. No live service calls,
  pushes, PRs or external messages were made.

## Slice 2a review — P1 repairs

- Expired-access refresh failures now give Offline with loaded content and
  Network without it, including WorkOS network, status and invalid-response
  errors. `ReadError::KeychainUnavailable` is the distinct keychain-read reason.
- Device attempts carry an identity and their own cancellation flag. Read scope
  invalidation never cancels them; Reload and saved-settings changes survive.
  Starting a replacement, Cancel, sign-out and stop retain explicit cancellation.
  A signed-out bootstrap no longer repeatedly invalidates its empty scope.
- Joined, loopback-only poll tests cover cancellation during pending, a
  superseding start, stop/sign-out during late success, denied/expired outcomes
  and successful completion across Reload/settings changes. Discarded outcomes
  assert no refresh credential, no session access and no stale Waiting state.
- Regression tests first reproduced the refresh mapping and cancelled-on-Reload
  failures. All 73 client tests and strict client Clippy pass; rustqual reports
  no findings in changed source. The P2 delivery records the final workspace gates.

## Slice 2a review — P2 repairs

1. Token installation releases the state lock for all credential I/O. Lifecycle
   actions serialize on the session lock, preserving the session → state → queue
   order and atomic cancellation/supersession. A blocked-store regression checks
   snapshot, version and IRC reads before releasing the credential write.
2. Cancel acts only on Starting/Waiting attempts, preserving Idle, Complete and
   failed states. Poll tests include both pending and late-success cancellation.
3. Sign-out on a stopped hub returns `HubError::Stopped`; it cannot revive a
   stopped queue or make the process registry reuse a zombie.
4. Membership/permission errors carry optional names. An unknown saved
   organisation gives a name-less reason, never a UUID as display text.
5. Read failures that sign out or expire the session clear organisation metadata
   together with scoped content.
6. Tests cover 404 with the pack present, 403 permission loss, 404 with the pack
   off, and cyclic team cursors with no partial workflow publication.
7. The debug session constructor accepts only `Arc<MemoryCredentialStore>`.
   A function-type regression protects that restriction; custom slow stores are
   injected through test-owned code, unavailable in production builds.
8. Search over 200 UTF-16 units returns `HubError::SearchTooLong` directly from
   activate/reload, with no state change or queued HTTP read.
9. The IRC projection caches its own version and compares all projected fields.
   List and page updates or an unchanged team reload leave it unchanged.

Public API names are unchanged. Additions/changes are
`ReadError::KeychainUnavailable`, `HubError::SearchTooLong`, optional names in
`MembershipLost`/`PermissionLost`, the narrowed fixture store argument, and
`PartialEq`/`Eq` on `IrcSnapshot`, `IssueReference` and `StateSummary` for semantic
projection comparisons. The IRC version is now independent of the board's.
Lifecycle actions may wait for session work and must run off the UI thread;
snapshot reads never wait for the credential store.

Final review-fix verification:

- `mise exec -- make check`: passes, including all **2,705 Rust tests**, five
  existing ignores, architecture, guides, formatting, unused dependencies,
  strict Clippy, script fixtures, the web bridge and Weather tests.
- Client tests: **82 passed**, including all added review regressions and the
  guarded, joined sign-in race tests. No live NETT/WorkOS/Cloudflare requests.
- `mise run quality`: **100%**, zero findings in changed source. The baseline
  refresh retains the score and changes only the function count.
- `mise exec -- cargo check -p subspace-nett-client --release`: passes.
- No dependencies changed; no audit run was required. No board or GPUI files
  were changed. The captain's final read-only Opus review remains outstanding.

No review finding was rejected. Lifecycle serialization is the commit boundary
for cancellation: a Cancel that linearizes after Complete leaves it alone,
while a cancelled/superseded pending attempt installs no credentials.

## Slice 2a — data hub (Bashir)

`clients/nett` now owns the GPUI-free hub, the one supported NETT dev session,
two API readers, priority scheduling, scope generations and list revisions.
It reads profile settings at start and accepts saved settings from the board.
The versioned board and IRC snapshots never serialize issue content; IRC's
cards remain empty until slice 4. Public signatures and integration details
are in [docs/issues.md](../../issues.md).

Membership discovery implements the one/several/none eligible organisation
rules. Automatic choices never become saved settings. Refused reads recheck
membership without changing the saved organisation. Team pages include
archived teams and workflow states, with fallback keys on catalogue failure.
List activation observes the 60-second freshness boundary; explicit Reload
also refreshes teams. Groups and their continuations share one typed query;
terminal groups can load on unfolding. Cursor rejection restarts the grouped
first page once. Offline content is limited to the current in-memory scope.

### Stated amendments to slice 1

- Removed `complete_sign_in`: clone `WorkOsSession::poller()`, start/poll it
  without borrowing the session, then call `install_sign_in(tokens)` under
  its lock. The hub supplies a dedicated device-flow thread and drops
  cancelled or superseded completions before installation. `WorkOsClient`
  derives `Clone`; no poll constructs a replacement development authority.
- `compare_and_drop_access(&Client)` invalidates only the access token that
  received a 401. The hub performs one refresh/retry; a second 401 expires
  the scope, while refresh `invalid_grant` signs out.
- Added `#[cfg(debug_assertions)] WorkOsSession::fixture(api_origin,
  workos_base, store)`. Both addresses must be numeric loopback; it skips
  `validate()`, whose pinned production checks are unchanged.
- Widened `WorkOsClient::fixture` to `cfg(any(test, debug_assertions))`, made
  it a checked public constructor returning `Result`, and bound device URL
  validation to its own verification origin. Legacy provider-response unit
  tests use a test-owned constructor; fixture-flow tests use the real seam.
- Removed the `MemoryCredentialStore` test-helper marker: it is a public
  injected store for debug consumers, never a production fallback.
- Added the pure, unconditional `fixture_from_env(debug, api, workos)`;
  `debug=false` is explicitly tested. The board's debug-only call site,
  release board check and Settings no-write fixture test belong to Dax.
- Kept `IssueQuery` per-team/per-parent and added a separate `ListQuery` for
  optional-team grouped reads and continuations. Teams now always request
  archived entries, including their workflow states.
- Organisation selection in `WorkOsSession` now also checks eligibility;
  the hub's selection uses its latest membership snapshot and never writes.

### Offline proof and scope

The foreground `scripts/nett-test-server.sh` serves recorded fictional orgs,
teams/workflows, grouped/continued rows, typed errors and a fake WorkOS flow
with loopback verification URLs. Its contract tests run under `make check`.
`fixture_sign_in_stays_on_loopback` checks both verification URLs, the cloned
poll authority and availability of the session lock during polling. Every
Rust test API/auth send has a transport-level loopback assertion.

Data privacy tests exercise the shared HTTP log with content, search, cursor,
grant and credential canaries while retaining allowed path identifiers. A
private Settings round trip asserts the exact metadata-only saved shape.

No board/GPUI implementation, UI capture, file fetcher, reference resolver,
cache or later-slice read is included. Dax owns board integration, Settings
Save, UI error wording, capture evidence and the board's release check.

### Verification

- `mise exec -- make check`: passes on the final source, including architecture,
  board guides, format, Cargo Shear, strict workspace Clippy, Rust tests, script
  fixtures, web bridge and Weather tests. Linux skips the existing native macOS
  bundle fixtures and PowerShell checks when PowerShell is unavailable.
- Data tests: **83 passed** (65 NETT client/auth/hub, 18 issue-service/profile/
  recognition tests). The server's two offline contract tests also pass.
- `mise run quality`: **100%**, zero blocking findings and no findings in
  changed source. After fixing findings, the refreshed baseline retains the
  same score and changes only the function count. Existing chart magic-number
  notes are unchanged.
- `mise exec -- cargo check --release -p subspace-nett-client`: passes; the
  pure environment seam compiles without the debug-only session constructor.
  Dax still owns the release check for the board crate.
- No dependency change, so no dependency audit was required. No live NETT,
  WorkOS or Cloudflare calls were made. Native keychain/platform integration
  and NETT dev end-to-end checks remain integration checks for the board.

### Divergence and evidence

No material spec divergence. Fixture overrides require numeric loopback IPs,
excluding DNS rebinding and credential/query-bearing origins. The reference
queue tier and card shape are present; reference resolution and the file
fetcher remain for their planned slices.

The approved [design captures](evidence/design/) remain fixture mocks, as
[design.md](design.md) explains. This data-only slice changes no UI and adds
no app captures. Dax adds the board's captures and checks. The NETT dev
end-to-end evidence belongs here once the development access update's route works, as spec section
12 describes. There are no open questions specific to this data delivery.

## Slice 2b — the Issues board (Dax)

`spaces/issues` is registered in the Subspace product and reads through the
slice 2a hub. Phase A built the views, keys and states against a request
seam; phase B wired the seam to the hub. How the board drives the hub is in
[docs/issues.md](../../issues.md#the-boards-side).

### What each state comes from

| Board state | Hub source |
| --- | --- |
| `list-loading`: *Loading issues…* over rows shaped like the list | `list_loading()` with no list yet; the scope line keeps the chosen organisation. |
| `list` | `HubStatus::Ready` and `list()`. Groups are named from the team's workflow (`state_id`), else the first row's state, else NETT's state-type name for All teams. `page_loading` and `page_failed` give *Loading more…* and *Couldn't load more*. |
| `list-signed-out` | `HubStatus::SignedOut`. |
| The no-organisation state | `HubStatus::ChooseOrganization`. `NoEligibleOrganizations` has its own state. |
| `error-session` | `ReadError::SessionExpired`. |
| *no longer belongs* | `ReadError::MembershipLost(name)`; without a name it reads *this organisation*, never an ID. |
| *can't read*, *Issue Tracking isn't turned on* | `PermissionLost(name)`, `PackUnavailable(name)`. |
| Keychain | `KeychainUnavailable`: *Subspace couldn't use the system keychain, so you're not signed in.* with **Sign in…** and **Retry**. |
| Edge, network, offline | `EdgeAccessNeeded`; `Network`; `Offline` keeps the list under the banner. `Session(_)` is the unexpected error. |
| Settings sign-in panel | `DeviceSignIn`: `Starting` → *Getting a code…*; `Waiting` → the code panel (*Open again* after the page opens); `Failed` → the expired, denied or other ending; `Complete` → signed in. |

Account-level errors (session, keychain, membership, pack, permission) stand
in for the list and clear its content. Errors while an organisation is chosen
keep the tabs and toolbar. Signing out, an expired session and stopping the
board clear the content from memory.

### Fixture seam, privacy and checks

- The fixture branch sits under `#[cfg(debug_assertions)]` in `link.rs` and
  `fixture.rs`, through 2a's `fixture_from_env(true, api, workos)`, with a
  memory credential store. The fixture's organisation picker calls
  `select_organization` and leaves the Settings snapshot unchanged
  (`fixture_run_never_writes_config`). `fixture_sign_in_stays_on_loopback`
  signs in through the offline mock.
- Chat is off on every Issues surface: rows, state boxes, the offline
  banner, Settings notes and panel and text menus; `agent_context` is
  `None`. The chat tests cover each, with a positive control in board-kit.
- `board_state_keeps_choices_but_no_issue_content` saves the state file
  after reads, a search, a fold and a tab change: it holds the choices but
  no title, identifier, search text or cursor, nothing else is written
  beside it, and the board offers no item search. The log test keeps
  request paths and drops content, queries and cursors.
- The offline server gained `--rows design` (fuller fictional rows for
  captures) and the `slow` scenario (a held list read, for the loading
  state), with two contract tests.

### Changes made while comparing with the design

- **Sign in…** in Settings no longer stretches across the page.
- The device code uses the theme's code font on the base background, as in
  `signin-code`.
- **In use** follows the organisation once the profile is signed in, as in
  `signin-code`'s NETT dev card; it stays beside the status while signed out.
- A state's first action is primary and the rest are secondary, so
  *Your session has expired* has one default action.
- A tracker's progress bar uses the strip's green, as in `list`.

### Captures and remaining differences

Captured from the debug build under `xvfb-run` against the offline fixture
server (`--rows design`), in Cupertino Dark at 1440 × 900:
list,
[list-loading](evidence/list-loading-1440-dark.png),
[list-signed-out](evidence/list-signed-out-1440-dark.png),
[no organisation](evidence/no-organisation-1440-dark.png),
[error-session](evidence/error-session-1440-dark.png),
[no longer belongs](evidence/no-longer-belongs-1440-dark.png),
[settings-profiles](evidence/settings-profiles-1440-dark.png),
[signin-code](evidence/signin-code-1440-dark.png),
[signin-waiting](evidence/signin-waiting-1440-dark.png),
[signin-signed-in](evidence/signin-signed-in-1440-dark.png),
[signin-expired](evidence/signin-expired-1440-dark.png),
[signin-denied](evidence/signin-denied-1440-dark.png) and
[signed-out](evidence/signed-out-1440-dark.png).

Differences from the design that remain:

1. **Platform.** Linux renders with DejaVu Sans, not the system font the
   macOS mocks use, so text is wider. The capture build's `shot:` step
   renders only on macOS, so the frames were recorded from the virtual
   display.
2. **One profile.** A fixture run has one profile, *NETT fixture*, with its
   loopback address. So there is no second Subspace API card
   (`settings-profiles`, `signin-signed-in`) and no **Choose another
   profile** (`list-signed-out`, `error-session`); the board offers it when
   there is another profile.
3. **No account name.** The mocks show *· Jo Fixture* after the address. The
   hub exposes no account name, so the line is the address alone.
4. **Use for issues.** `signin-signed-in` shows a **Use for issues** button
   that the spec doesn't have: the title-bar profile picker chooses the
   board's profile.
5. **Shared list chrome.** The shared list insets the heading and rows by
   14 px, not 24, puts each group's count at the end of its line rather
   than beside the name, and folds with ▼. Identifiers use the body font,
   not the code font. The tabs carry glyphs, as every Subspace tab does,
   and the team picker has one. The captain accepted these for slice 2;
   [slice 5](tasks.md) records the list's fidelity with NETT's layout.
6. **Left out by the tasks.** The Assignee, Priority and Filters pickers,
   the My issues count and the hidden sub-issue count, all in slice 5.
7. **Session expiry** also offers **Retry**, since the 401 can come from
   Cloudflare One rather than the session; the message says so.

### Verification

- `mise exec -- make check`: passes, with 2,888 Rust tests and none failing:
  107 in the board, 92 in the NETT client and 278 in board-kit's dashboard.
  The architecture, board-guide and script suites pass, including the
  server's four contract tests.
- `mise run quality`: 100%, no findings and none in changed code. The
  existing chart-example magic-number notes are unchanged.
- `mise exec -- cargo check --release -p subspace-issues-board`: passes, so
  the fixture branch compiles out of release builds.
- `mise run audit`: advisories pass for all three lockfiles. This slice
  changes no dependency.
- No call reached NETT, WorkOS or Cloudflare. The captures used only the
  loopback fixture server and fictional data.

After merging `feat/subspace-desktop` at `3360edc` (the Channels Markdown
and slice 4's services half), the gates ran again: `make check` with 2,990
Rust tests passing, five ignored and none failing (107 in the board, 98 in
the NETT client, 281 in board-kit's dashboard), and the script suites;
`mise run quality` at 100% with no findings; `cargo check --release` for
`subspace-issues-board` and `subspace-nett-client`; and `mise run audit`.
The board calls neither changed Markdown signature, and the merge changes
nothing the board shows, so the captures stand.

### Cross-model review fixes

GPT-6.1 Sol reviewed `d195369` read-only and found no P0. The captain
accepted seven findings and declined P2-6: account errors stand in for the
list, as spec section 5 says. Each fix began with a test that failed on the
old code. A stub run with the new types but the old behaviour failed ten
board tests and the store test. The fixture server's slow test timed out on
a real four-second sleep.

| Finding | Fix | Proving tests |
| --- | --- | --- |
| P1-1 saved Settings waited for the board | `link::install` registers an `observe_global::<Services>` that calls `apply_settings`, dropped when the hub stops; a fixture run registers none | `saved_settings_reach_the_hub_before_the_board_shows` (no board at all), `a_fixture_run_keeps_its_own_organisation` |
| P1-2 a hidden board kept content | `deactivate` drops the board's copy of the list; `activate` rebuilds it from the hub's snapshot before the next frame and re-selects the line. Chosen over polling while hidden, which would have let a hidden board ask for reads | `a_hidden_board_holds_no_rows_after_sign_out`, `returning_rebuilds_the_list_at_once_with_its_selection` |
| P1-3 the keychain note came first | Settings shows the profile signed out with no note while the delete runs; the note follows success; failure says *Subspace signed you out but couldn't remove the stored session from this computer's keychain.* with **Retry**. `MemoryCredentialStore::refuse_deletes` stands in for a locked keychain | `a_failed_keychain_delete_says_so_and_retry_removes_the_session` (hub-backed), `sign_out_takes_effect_at_once_and_notes_the_keychain_after_it`, `a_failed_keychain_delete_says_so_and_offers_retry`, `a_late_result_never_replaces_a_new_sign_in`, `a_memory_store_can_refuse_deletes_as_a_locked_keychain_does` |
| P2-4 a fresh list stayed busy | After a request the board applies the hub's snapshot even when its version is unchanged | `returning_to_a_fresh_list_is_not_busy`, which also checks no grouped read was sent |
| P2-5 an unread group folded back | The board makes the fold itself and reports the activation handled | `unfolding_an_unread_group_keeps_it_open_while_its_page_loads`, with the page held; reverting to `false` fails it at *unfolded once* |
| P2-7 a server error showed Offline | `ReadError::Offline` maps to a new `Failure::Offline`, the only failure that keeps content; `Network` is the `error-network` state | `a_server_error_after_content_says_couldnt_load_not_offline` (a real 500 through the hub), `another_status_after_content_is_the_network_error_not_offline`, the updated Offline banner and mapping tests |
| P2-8 the no-write test never saved | `tests/config.rs` runs the host's board-editor Save on `SettingsFile` in an isolated directory | `fixture_run_never_writes_config` (existing files keep their bytes), `fixture_run_creates_no_config`, and the control `outside_a_fixture_run_save_writes_the_pick` |
| P2-9 the slow test never read the slow route | `FixtureServer` takes an injected `sleep` | `test_slow_holds_only_the_grouped_list` reads the grouped list and the teams route |

The network state's message now reads *{profile} didn't send the list.
Check your connection, then Retry, or copy the details.*, since it also
covers a server error. The Help guide's sign-out and troubleshooting text
follow the new behaviour. The only visible changes are the timing of the
existing removal note, the new failure note and the network message; no
capture was retaken.

The base was then merged twice: `de0a83a` (slice 3b's hub issue view,
aiconsultancy/subspace#12) and `d7f1007` (macOS signing build scripts). The
fixture server's two conflicts were additive. The scenario list keeps
`slow` beside the base's `children-error`, `invalid-json` and
`server-error`, and the test script keeps both sides' tests. The base's
issue-read routes and the slow grouped route with its injected sleeper
both stay. Slice 3b generalised the hub's error handling for issue reads,
but the list's behaviour is unchanged, and the board doesn't read issues
yet.

Gates on the merged head:

- The fixture server's contract tests: five pass.
- The board and NETT client tests five times at 64 threads: 248 passed
  each run, none failed.
- `mise exec -- make check`: 3,034 Rust tests passed, five ignored, none
  failed (120 in the board, 128 in the NETT client, 281 in board-kit's
  dashboard), plus the script suites.
- `mise run quality`: 100%, no findings.
- `cargo check --release` for `subspace-nett-client` and
  `subspace-issues-board`: both pass.
- `mise run audit`: advisories pass for all three lockfiles.

### Final review fixes

Merged `e2f1013` (slice 4 hub) through `origin/feat/subspace-desktop` at `f2e1bd8` on 15 November 2026, keeping both sides.
Merged `origin/feat/subspace-desktop` at `cac89a8` (board contracts, IRC history and toolbar updates) on 15 November 2026; a merge preserves both histories and the existing GitHub-signed commits, which a rebase would rewrite.
The merged `make interfaces` gate first failed because its expected product board lists omitted Issues; both platform expectations now include it. `Dashboard::contracts` remains the empty trait default; adoption belongs to board-contracts phase 3.
The gates refreshed both excluded Terminal lockfiles for the Framework's new transitive `subspace-contracts` dependency; those lock entries are included in the merge.

- Re-review P2-1: pending first pages are separate from completed empty pages; a changed hub request revision drops pending markers. `a_cancelled_first_page_is_read_again_after_an_offline_reload` first failed because Done became `More::None`, then passed with a real loopback transport disconnect.
- Re-review P2-2: every successful hub list install increments `list_id`; the board combines that counter with the scope generation and keeps `updated_at` for freshness only. `equal_timestamps_do_not_identify_replacement_installs` was added before the API (initially a compile failure) and now proves two installs at exactly the same instant have different identities.
- Re-review P2-3: the stale-activation regression now keeps the query unchanged, ages only the fixture timestamp through a test-support helper, and reactivates through the real stale path while the installed list stays present. It begins with a completed empty Done page and requires another read after replacement; a temporary “clear only on None” mutation made it fail before restoring the fix.
- Re-review P2-4: cursor recovery counts only grouped list reads, excluding `limitPerGroup=0`, and requires a different installation plus a second Done first-page read. The strengthened regression also fails against the “clear only on None” mutation and passes after its removal.
- Last review 1: `fold_changing` now uses the same loading guard as `show_groups`, so unfolding during a Reload records no unqueued first page. `unfolding_during_a_failed_reload_does_not_complete_an_unqueued_page` first failed with `More::None`; it now reads Done after the held Reload disconnects.
- Last review 2: the stale-activation test uses `Instant::checked_sub`; an unrepresentable earlier instant clears only the test timestamp, which still makes the installed list stale. The hub freshness test explicitly checks this fallback, and the unchanged-query activation regression still passes.
- Last review 3: profile changes already call `Issues::set_profile` → `listing.clear()`. Direct hub replacement now clears the listing in `Issues::attach` too; `replacing_the_hub_forgets_empty_pages_even_when_list_ids_match` first timed out with equal scope/install identities and now reads the new hub's Done page. No publication path was added.
- Last review 4: a failed initial list read leaves the count summary blank, matching spec §5's error row. `a_failed_load_does_not_claim_to_be_loading` first saw “Loading issues…” and now passes; `evidence/slice-2-review/error.png` was recaptured with the same isolated loopback/Xvfb method below and inspected before commit.

Last-review foreground gates passed in order: five board/client runs at 64
threads (137 + 182 tests per run), eight fixture-server tests, `make check`
(3,109 Rust tests passed, five ignored, plus script suites), `mise run quality`
(100%, no findings in changed code), release checks for both crates, and audit
for all three lockfiles. A duplicated three-statement test setup was extracted
before the final gate run; the refreshed quality baseline changes only its
function count. The explicit findings scan still reports only the 86 existing
chart magic-number findings. The recapture runner stopped every owned process.

The Issues board checklist is recorded in [tasks.md](tasks.md). Small,
board-local palette commands reuse the existing profile/team choices, tab,
sub-issue and search actions, recovery paths and selected-row copies. The
shell still owns Reload. `diagnostic_config_path` offers the board-owned
`issues/config.json`, except for in-memory fixture sessions. Five new tests
cover these actions, clipboard output and the normal/fixture diagnostic paths;
the initial palette/path tests failed before implementation, and the fixture
path test first failed against an unconditional file-path hook. No new hub
publication path was added.

The remaining palette coverage is deferred because shared Copy details and
arbitrary text-selection menus expose no callable palette seam. Completing
that item needs Framework work; duplicating those controls' diagnostics,
selection and confirmation behavior inside the board would be incorrect.

#### Slice 2 review evidence (15 November 2026)

Captured with `cargo build -p subspace --features subspace-shell/capture` on
Linux. `SUBSPACE_CAPTURE_STEPS` drives the real app's clicks and keys under
an owned Xvfb display; `ffmpeg -f x11grab -draw_mouse 0 -framerate 2` records
the virtual screen, and the kept PNGs are unedited frames. The window is
1000 × 720 at `GPUI_X11_SCALE_FACTOR=1.6`, producing 1600 × 1152 images.
Cupertino Dark is selected by leaving the theme unset. Only Issues is enabled,
chat is off, and startup update checks use `app.check-updates = "0"`.

The capture uses an isolated HOME and XDG directories. Dax's existing DejaVu
2.37 TTFs were copied into that HOME's `.local/share/fonts`; `fc-cache -f`
and `fc-match 'DejaVu Sans'` verified DejaVu Sans before launch, resolving
the earlier missing-font panic without installing system packages. Body and
heading fonts are DejaVu Sans, with DejaVu Sans Mono for the mono role.

Both debug overrides point at `scripts/nett_test_server.py` on numeric
loopback: `SUBSPACE_NETT_API_URL=http://127.0.0.1:18196/api` and
`SUBSPACE_WORKOS_URL=http://127.0.0.1:18196`. Its fictional device flow signs
in through Settings, with memory-only credentials and `BROWSER=true`. No live
NETT or WorkOS service is used. Every kept image was inspected for real
people, hosts and secrets. The runner stopped its app, Xvfb, ffmpeg and fixture
server after each run; the owned display socket and listener are gone.

| Image | What it shows |
| --- | --- |
| list | Recorded fixture rows with In progress and Done expanded, including Done's first-page row. |
| [empty](evidence/slice-2-review/empty.png) | Search with zero matches; the fixture retains its empty group headings. |
| [loading](evidence/slice-2-review/loading.png) | The `slow` fixture's real initial-list skeleton and busy indicator. |
| [error](evidence/slice-2-review/error.png) | The `server-error` fixture's failure with Retry, Copy details and Show log. |
| [signed out](evidence/slice-2-review/signed-out.png) | The unsigned fixture profile with Sign in. |
| offline | After a successful read, the fixture's `network` disconnect makes Reload retain rows under Offline with Retry. |
| palette | The new Issues tab, sub-issue and search commands filtered in the shared palette. |

Issue detail is not captured: `Issues::detail` returns `None` on this branch,
and slice 3 owns that surface. Design-mode fixture groups deliberately have
no Done rows and ignore search text; the Done and zero-match evidence uses
the recorded fixture mode instead. No state was faked.

Sol's final review of `bf0cb8d` (read-only, xhigh) found one P1 and two
P2s. Both fixes began with tests that failed on the old code.

- **P1: a stale snapshot hid the keychain failure.** **Sign out** pulled the
  hub's snapshot at once. If the hub's version had moved, that snapshot
  predated the sign-out, signed the card back in and reset the pending
  `Removing`, so a failed delete was dropped with no note and no **Retry**.
  Now a pending `Removing` survives snapshots: `set_session` neither clears
  it nor shows the card signed in, and only **Sign in…** from the page
  starts over. Tests: `a_stale_snapshot_never_hides_a_failed_keychain_delete`
  and `a_stale_snapshot_never_hides_a_removed_session` move the hub's
  version with a reload just before **Sign out**. Both failed before the
  fix, at the check that `Removing` holds. They now settle on the failure
  note with the token still stored, and on the removed note with it gone.
  `a_snapshot_never_signs_the_card_in_while_the_delete_runs` covers the page
  alone. `a_late_result_never_replaces_a_new_sign_in` now signs in through
  **Sign in…**, the real path.
- **P2-2: an empty first page re-read forever.** A group with a count and no
  rows stayed `Unread` after its first page came back empty, so each
  snapshot asked again under *Loading more…*. `Listing` now records which
  groups' first pages this scope asked for. Such a group settles as
  finished. Another scope or Reload clears the record; hiding the board
  keeps it, as it holds state ids only. Tests:
  `an_empty_first_page_settles_after_one_read`, which uses the offline mock's
  new empty-page switch and timed out before the fix. It now sees one page
  read and no loading row after five more polls.
  `an_empty_first_page_ends_the_group_until_the_scope_or_a_reload_changes`
  covers the rules.
- **P2-3: Copy details loses the HTTP status. Deferred to slice 5.** A real
  500 reaches the board as `ReadError::Network`, so the copied details say
  *network*, and the board's numeric-status branch can't be reached yet.
  The fix needs `ReadError` to carry an optional status. Two open hub
  branches change that mapping, so the captain deferred it. It is listed
  under slice 5 in [tasks.md](tasks.md).

Sol's re-review of the fix commits found the sign-out fix clean and two
gaps in the empty-first-page fix, both written test-first:

- **P1: the first-page record outlived its list.** It was keyed by group
  only, so a replacement list (a stale activation, a cursor's recovery)
  found Done "already read" and hid its rows. The record now belongs to
  one list, identified by the snapshot's scope generation and the time the
  hub installed the grouped list (`updated_at`), which pages leave alone
  and every replacement changes. A new list empties it; paging within the
  same list still settles after one empty read. Tests:
  `returning_to_a_replaced_list_reads_dones_first_page_again` and
  `cursor_recovery_reads_dones_first_page_again` both failed before the fix,
  waiting for Done's rows. `an_empty_first_page_settles_after_one_read`
  still passes, and the listing unit test now covers the list identity.
- **P2: Reload recorded a first page the hub refused.** While the hub reads
  a new list it queues no page, so the board no longer asks for first pages
  from a snapshot that is still loading the list. And since the fresh list
  has a new identity, any earlier record no longer applies to it. Test:
  `reload_reads_a_once_empty_group_again` (an empty page, then rows, then a
  held grouped Reload) failed before the fix and now reads Done's rows.

Gates on the fix: board and NETT client tests five times at 64 threads
(273 each, none failed); `make check` with 3,059 Rust tests passing and five
ignored, plus the docs index and script suites; `mise run quality` at 100%
with no findings; release checks for both crates; and `mise run audit`.

Merged `feat/subspace-desktop` at `db8831f` (slice 5's data half,
aiconsultancy/subspace#16, and the docs index) without conflicts or board
changes; the board uses none of slice 5's filters or counts yet. Slice 5's
fixture server builds grouped rows from its own filtered records, which
overwrote `--rows design`; the design rows are now served as drawn again,
filtered by state type, while recorded rows keep slice 5's filtering. The
Issues guide joined `docs/README.md`, and three board-kit doc comments that
named Issues were made generic. Gates after the merge: board and NETT client
tests five times at 64 threads (270 each, none failed); `make check` with
3,056 Rust tests passing and five ignored, the docs index and the server's
seven contract tests; `mise run quality` at 100% with no findings; release
checks for both crates; and `mise run audit`.

Gates on `1d3f397` (the base was still `d7f1007`):

- The board and NETT client tests five times at 64 threads: 253 passed
  each run, none failed.
- `mise exec -- make check`: 3,039 Rust tests passed, five ignored, none
  failed (125 in the board, 128 in the NETT client, 281 in board-kit's
  dashboard), plus the script suites.
- `mise run quality`: 100%, no findings.
- `cargo check --release` for `subspace-nett-client` and
  `subspace-issues-board`: both pass.
- `mise run audit`: advisories pass for all three lockfiles.

## Slice 3a: Markdown preparation (15 November 2026)

- Added the pure `subspace_issues::nett_markdown` module, next to reference
  recognition. This is the captain's 15 November placement decision so the
  board's slice 2 and Markdown's slice 3 can run in parallel.
- Added `rich::rich_text_prepared(id, source, claim)` without changing the
  existing rich-text paths. It skips `prepare_markdown`, retains selection
  and the shared link policy, and installs the same data-image-only resolver.
- Added the exact `markdown = "=1.0.0"` dependency and a direct workspace
  `url` dependency for origin/path validation; no lockfile versions changed.
- The mdast parser has GFM enabled, HTML and maths disabled. Source-range edits
  preserve code bytes and ordinary link destinations. HTML and dollar signs
  stay visible, media and mentions use body-local metadata, file text includes
  name, MIME type and human-readable size, and issue links use current team keys.
- `Stage::PlainText` is the only implemented stage. Slices 4 and 7 extend
  this single enum when their plugins exist; this slice emits no
  `subspace-issue:`, `subspace-member:` or `subspace-media:` forms.
- TDD: the first service run had eight expected failures; the prepared-view
  render test first failed to compile before its entry point was added.
- Tests are split at the service/board-kit boundary: the service cannot depend
  upward on dashboard. `nett_markdown_keeps_all_text` checks prepared source;
  dashboard's `rich_text_prepared_keeps_all_text` renders prepared fixtures
  and checks native plain-text selection, including literal HTML inside code.
  Markdown soft line breaks render as spaces. The end-to-end test lands in
  `spaces/issues` in slice 3b.

### Slice 3b integration

Construct `Media::new(id, name, content_type, size)` and
`MentionStatus::new(id, current_name, is_member)` from the body's DTOs. Call
`IssueLinkContext::new(app_origin, organisation_id, &references)` and
`Options::new(context, &media, &mentions, Stage::PlainText)`. Build `references`
once with `ReferenceRecognizer::new(loaded_keys, fallback_keys)`, including
archived teams; reuse it for text and links. Use an empty recogniser while
signed out or without an organisation. This is the final-review API below.
Call `nett_markdown::prepare_nett_markdown(source, &options)`, then
`rich::rich_text_prepared(id, &prepared, claim)` directly. Do not pass the
prepared source through the existing `rich_text`/`rich_text_with_links` path.
The board owns rendering the inert file text with any future chip treatment.

### Verification

- `mise exec -- make check`: passed on the final implementation, including
  architecture (35 packages), format, cargo-shear, clippy with warnings denied,
  workspace tests/doc tests and offline script fixtures. The service has 19
  Markdown tests; dashboard's prepared-fixture render/selection test passes.
- `mise run quality`: passed, 100.0%; the findings scan has no findings in
  touched code. The 86 existing findings are in chart code/examples.
- `mise run audit`: passed for the workspace and both excluded Terminal
  lockfiles. No advisory exceptions or dependency versions were changed.
- Linux's existing script skips remain: PowerShell fixtures require `pwsh`,
  and macOS bundle fixtures require macOS. No live NETT calls were made.

### Scope and remaining work

The placement change and split testing follow the captain's brief. The
options also take current team keys so section 4's recognition rules apply
to explicit issue links. Two obsolete `qual:api` markers on reference
recognition were removed because the new pre-processor is a production caller.
Only the slice 3a item in `tasks.md` is checked. Slice 3b owns DTO mapping,
the issue view, visual evidence, and the combined end-to-end render test.
Slices 4 and 7 own the future plugin URL forms. The live NETT dev acceptance
check from section 12 remains for later delivery.

## Slice 3a review fixes (15 November 2026)

### P1: parser panic containment

Committed separately as `bf836a2` (`fix(rich): render Markdown that crashes
the parser as plain text`). The failing test ran the actual IRC
`is_markdown` → `to_markdown` → shared `prepare_markdown` path before parsing
with Kit's GFM + `math_text` + `math_flow` options:

IRC escapes the first input's inline backticks, making it safe. It appends
a closing fence to the second input, which still panics with
`ListOrdered / ListUnordered`. The exact fixture strings were:

~~~text
input: "1. ```\n- x\n"
IRC:   "1. \\`\\`\\`\n- x"
input: "1. a\n   ```\n- x\n"
IRC:   "1. a\n   ```\n- x\n```"
~~~

Both original inputs also panicked in the NETT pre-processor before the fix.

All three `TextView::markdown` call sites in `rich.rs` now use one guard
helper. It catches the same parser's unwind, escapes every ASCII punctuation
character if needed, and checks that escaped candidate too. Character
references provide a parser-inert last resort if both parses fail. Native
render/selection tests cover all three rich entry points, the two original
inputs and the actual IRC-transformed crash, preserving visible punctuation.
The pre-processor also catches unwinds and verifies its escaped fallback.

The cache holds at most 256 entries and 512 KiB of source/result payloads,
uses source hashes plus full source equality, and caches both valid inputs
and panic fallbacks. Oversized messages are checked but not retained.
The deterministic test counts parser checks rather than asserting wall time.
On this devbox in the debug test, a 2,000-message transcript with 200 visible
messages cost 87.9 ms cold; 60 cached frames took 12.0 ms total, about
0.200 ms per frame including the mutex, hashing and returned-text allocation.
There were exactly 200 checks across initial display and all 60 redraws.

`[profile.dev]` and `[profile.release]` previously inherited Cargo's
`unwind` default and now specify `panic = "unwind"` explicitly. Test inherits
dev (test harnesses unwind); bench inherits release. There is no separate
package profile: `package-release.sh` and `build-macos-app.sh` use `--release`,
and macOS packaging/release/install call that build. No aborting profile or
panic-strategy environment override was found. The excluded terminal crates
declare no separate profiles. No upstream report or live app/service call
was made.

### P2: Markdown correctness and integration

- Shortcut/collapsed external reference labels retain their source so dollar
  escaping cannot disconnect them from their definitions.
- Blank or whitespace-only media alt text produces *Image*; other alt text
  is trimmed before producing *Image: {alt}*.
- `Node::Break` contributes a newline to labels. Generated plain link and
  mention labels write Markdown hard breaks so native copy keeps the newline.
- Tests cover `$`/`<` in link/definition titles, trailing-slash issue URLs,
  and a table of no-panic inputs including both parser crashes and IRC's
  transformed input. Titles remain opaque Markdown metadata, unchanged.
- `rich_text_prepared` documents that callers must neutralise raw `<`
  themselves and that supported `data:` images remain drawable.
- The public entry point is now `prepare_nett_markdown`; there is no old-name
  alias. Slice 3b must use that name.
- No reusable size formatter exists in the service layer. A small decimal
  formatter now produces bytes or one-decimal KB–EB (for example `1.0 MB`),
  tested from zero through `u64::MAX`. The GitHub image label's private KB
  formatting cannot be depended on from a service.

The shared fixture list is
`framework/services/issues/fixtures/nett_markdown.json`. Each case has `name`,
`source`, `prepared`, `visible`, and optional body-local `media`/`mentions`.
Its profile is `https://app.example.test`, team keys `["NET"]`, stage
`PlainText`. The service tests run the real `prepare_nett_markdown` output
and verify Kit-option parsing for every case. Slice 3b's combined test should
reuse this JSON, feed the real prepared output through `rich_text_prepared`,
and assert native rendered/copied text against `visible`. Do not substitute
the `prepared` expectation for the real pre-processor output.

Review-fix gates passed: `mise exec -- make check` (127.09 s),
`mise run quality` (100.0%, no findings in touched code), and `mise run audit`
(all three lockfiles). The required quality-baseline refresh changes only
the function count; all scores and zero-violation thresholds are unchanged.
There is no disagreement with the requested fixes. The captain's final
read-only Opus review remains; no push or PR was made by this worker.

## Final review follow-up — 15 November 2026

### P1: parser containment and bounded rendering

This supersedes the earlier render-cache and catch-unwind implementation above.
`markdown` 1.0.0 is vendored with only public `to_mdast` changed: unwinding
panics return `Err(Message)` with the fixed reason documented in
`vendor/markdown/PATCHED.md`. Both crash inputs now return that error without
unwinding to their callers. The actual IRC path still reaches the second crash
shape, and now returns the contained error too. Kit and NETT resolve the same
single path crate in all three lockfiles. The standalone terminal workspace
roots need their own patch entries because Cargo does not inherit patches
from dependencies. The original MIT licence is unchanged; `mise run audit`
accepts the path crate in all three lockfiles (the repository task checks
advisories, not a separate licence allowlist).

The render preflight is limited to 4 KiB, matching Kit's
`MAX_SYNC_FULL_REPLACE_BYTES`, and requires a backtick, tilde or math fence run.
Parsing happens outside the lock. The 256-entry LRU holds only `(hash, length,
verdict)` metadata; it retains no source or fallback text across sign-out.
Valid `SharedString` inputs pass through without copying. Larger failures use
Kit's parse-error display, including on its now-protected background parser.
The service no longer catches panics itself. Markdown uses opt-level 3 in dev.
Dev and release remain explicitly `panic = "unwind"`; test inherits dev,
bench inherits release, and package builds use release with no abort override.

Debug measurements on this devbox, with Markdown optimized: 2,000 messages,
200 visible, ordinary cold 0.241 ms and warm 0.166 ms/frame; fenced cold
6.844 ms and warm 0.233 ms/frame (60 frames). A 20,000-byte reply in 200 chunks
through real `rich_text` construction takes 13.279 ms total, versus 0.394 ms
for Kit construction and 3.968 ms for Kit plus the existing Markdown preparation;
the bounded preflight adds about 9.31 ms across all chunks. A 300 KiB source
takes 63 ns per guard call and 3.583 ms through `rich_text`. These measure
construction/preflight only; Kit's later parse/render work is common to both.

Vendor code is excluded from the workspace, architecture and clippy targets.
The formatter now selects actual workspace members instead of following path
dependencies. The requested vendor exclusion requires a minimal `rustqual.toml`
because rustqual 1.8.3 otherwise scans all vendor sources; only `exclude_files`
is set, preserving built-in thresholds. This is the explicit task's exception
to AGENTS.md's usual no-config rule. Architecture, targeted service/dashboard/
IRC tests, quality and the advisory audit passed before the P1 commit.

### P2: final review correctness and merge safety

1. Text-node brackets are escaped, preventing an outer link from becoming
   active after an inner issue, mention or reference is flattened. All three
   probes have service assertions for zero resulting links and native fixtures.
2. Shortcut/collapsed references resolve first, become inline links with safe
   prose labels, and retain their definitions. HTML tags and prices survive
   native rendering; spec section 9 now states this rule.
3. The P1 cache test checks LRU eviction, the length component, fixed capacity
   and the complete metadata-only entry/storage types. No source text remains.
4. The HTML branch test constructs the exact crashing list/fence Markdown from
   real `<ol>`/`<pre>`/`<ul>` HTML, verifies Kit-option parsing fails safely,
   then checks readable native selection through `Format::Html`.
5. The one shared JSON now holds origin, team keys, stage and 42 cases. Both
   test crates share a fixture adapter. Dashboard invokes the real pre-processor
   for every case and asserts `visible` against native rendered/copied text;
   the hand-copied prepared list is gone. This supersedes the earlier plan to
   defer the combined test to slice 3b. Slice 3b should reuse this fixture for
   its board/DTO wiring tests.
6. Private `markdown_view` is the sole `TextView::markdown` caller in `rich.rs`,
   with a source-count tripwire. All rich entry points use it. Final signature:

   ```rust
   pub fn rich_text_prepared(
       id: impl Into<ElementId>,
       source: impl Into<SharedString>,
       claim: impl Fn(&str, &mut Window, &mut App) -> bool + Send + Sync + 'static,
   ) -> TextView
   ```

   It accepts `&str`, owned `String` and retained `SharedString`. Callers still
   neutralise raw `<`; supported `data:` images remain drawable.
7. NETT retries parser errors once with complete backtick, tilde and math fence
   runs escaped, then applies its normal media/mention/link rewrite. Literal
   fallback is reserved for a second failure. Tests cover both original crashes
   with private identifiers and paths, including a native recovered-body case.
   Recovery lives in a small private module to keep the rewrite module focused.
8. File sizes promote units after rounding and omit `.0` (`999,950` bytes is
   `1 MB`); blank external-image labels are `Image ↗`. Boundary tests pass.

The public service API remains
`prepare_nett_markdown(source: &str, options: &Options<'_>) -> String`.
New regression tests first failed for all four reproduced behaviors, then
passed after the fixes. Targeted service/native tests, architecture, clippy
and dependency checks pass; no new rustqual findings remain in touched code.
The formatter explicitly covers the two owned standalone Terminal workspaces
as well as main workspace members, while preserving vendor bytes.
No disagreement with the captain's design; larger parser failures deliberately
use Kit's error box. Final integration and gate results follow below.

### Integration and final verification

Merged `origin/feat/subspace-desktop` at
`193b47dc17b4ef85f6a1ef9af01d560ccc2f3f1b` (the mention notification change) cleanly after the
separate P1 and P2 commits. No `rich.rs` changes came from that merge.

All required gates passed on the combined tree, with foreground waits:

- `mise exec -- make check`: 140.39 s; architecture (35 packages including
  excluded terminals), owned-workspace formatting, cargo-shear, clippy with
  warnings denied, workspace tests and offline script/JavaScript fixtures.
- `mise run quality`: 5.74 s, 100.0%, no new findings in touched code.
- `mise run audit`: 7.92 s, all three lockfiles accepted, including the single
  vendored MIT `markdown` 1.0.0 path dependency used by both Kit and NETT.

The normal platform-specific harness skips remain: PowerShell is unavailable
and macOS bundle fixtures do not run on Linux. The upstream source comparison
still shows only the approved `to_mdast` function patch; licence, README and
manifest retain their crates.io bytes. No push, PR, live service call or external
write was made. The captain's read-only Opus review is the next step.

## Slice 3b: hub data

The M1 issue view now has memory-only hub data, without a GUI or reference
resolver. Work is on `issue-workspace-slice-3-data`, based on `5f40a4385559`.
Indra was fetched and read only at `origin/main`
`15a445aaf282e58d05745e7b75e52f040cc0d603`: the issue detail endpoint and
`Shared/Contracts/IssueTypes.cs`, the shared `MentionContracts.cs`, and
`Infrastructure/Abstractions/IssueTracking/IIssueMediaService.cs`.

### Contract and integration

- `IssueDetail.media: Vec<IssueMedia>` comes from `IssueDetail.Media` and
  `IssueMediaDescriptor`: media ID, version, kind, file name, content type,
  signed 64-bit byte size, nullable expiry, public file ID. There is no grant.
- `IssueDetail.mentions: Vec<MentionStatus>` comes from the shared mention
  status contract: user ID, status, nullable display name, role and reason.
  Both `canRead` and `cannotRead` name active members; `notMember` does not.
- `IssueDetail.provenance: Option<IssueProvenance>` comes from
  `IssueProvenanceDto`: required source system/original identifier and
  nullable URL, creator name and four original timestamps.
- Media and mentions default to empty when omitted; explicit null fails.
  Provenance can be absent/null. All new DTOs remain `#[non_exhaustive]`.
  Header progress was already `IssueFields.progress: Option<Progress>`;
  its fixture coverage now includes tracker counts and `hasMore`.

The board maps these facts into slice 3a's `Media::new` and
`MentionStatus::new`, using loaded team keys (archived included) or fallback
keys from the unchanged IRC projection. See [docs/issues.md](../../issues.md)
for the full API and state contract. The new commands are:

```rust,ignore
Hub::open_issue(&self, identifier: &str, profile_id: &str) -> Result<(), HubError>
Hub::reload_issue(&self) -> Result<(), HubError>
Hub::load_more_subissues(&self) -> Result<(), HubError>
Hub::close_issue(&self)
HubSnapshot::issue(&self) -> Option<&IssueSnapshot>
```

`IssueSnapshot` readers are `identifier(&self) -> &str`,
`profile_id(&self) -> &str`, `status(&self) -> &IssueStatus`,
`detail(&self) -> Option<&IssueDetail>`, `updated_at(&self) -> Option<Instant>`,
`age(&self, now: Instant) -> Option<Duration>`, `children_loading(&self) -> bool`
and `children_failed(&self) -> bool`. Every open reads at visible priority;
Reload re-reads the issue and first children page. Continuation uses parent,
manual order, no team and limit 20, at later-page priority. Duplicate pending
continuations share one read. A bad cursor restarts the issue once.

### States, scope and privacy

`IssueStatus::{Loading, Loaded, SignedOut, Unavailable, Error(ReadError)}`
uses the hub's existing error map. A second API 401 after refresh gives
`Error(SessionExpired)`; invalid-grant signs out. A 403/404 checks membership
again: missing org gives `MembershipLost`, missing pack `PackUnavailable`,
and missing permission/403 `PermissionLost`. With membership and pack intact,
an issue 404 gives `Unavailable`, rendered neutrally as “{identifier} isn't
available with {profile name}.” Profile mismatch never sends a read.
Access challenges give `EdgeAccessNeeded`; first transport errors and
unexpected statuses/invalid JSON give `Network`. Transport failure on Reload
keeps that issue's content as `Offline`; a loaded list alone is insufficient.
Keychain errors retain the shared keychain mapping and scope-clearing policy.
Other child-page failures set the row's failed flag for Retry and retain the
detail. Successful access retry restores Ready.

An independent issue revision guards details, pages, queue dispatch and
membership rechecks. Back/Escape clears content immediately; new opens and
Reload retire earlier results. Scope changes, sign-out, expiry and stop
discard content. Expiry/access errors can retain only a content-free shell.
All new UI calls use only state/queue locks, never session/keychain/network
waits. No Serialize implementation or persistence path was added. Log
canaries now cover child rows/cursors/parent filters, media names, mention
names and provenance as well as titles/descriptions and credentials.

### TDD, fixtures and gates

DTO and initial hub tests first failed on the missing fields and API.
The fixture contract first failed with an issue-detail 404. Additional
regressions first demonstrated two recovery failures: a fresh open after
expiry showed Unavailable, and successful retry left account access-error
status. Both now preserve the correct state. Deterministic gated HTTP reads
cover late detail/page/refusal results after Back, including cursor restart;
session-lock tests cover all four commands. Other coverage includes scope
clears, paging/retry, exact parent/manual/no-team queries, 404 pack checks,
session/access/edge/network/offline states and tracker progress.

`scripts/nett_test_server.py` now serves `TST-1612` with 25 fictional children,
`TST-1616` with media, mentions, links, dollar signs and code, and
`TST-999999` as unavailable. `children-error`, `server-error` and
`invalid-json` join the existing session/access/edge/network scenarios.
The three loopback server tests pass. No live NETT/WorkOS service was called.
GUI evidence and DTO-to-Markdown mapping belong to the Claude slice; this
delivery adds no visible product surface. Reference cards remain untouched.

Final verification passed, with foreground waits and gates run in order:

- The final hub suite passed five consecutive runs of **58 tests at 64
  threads**. The full client suite has **112 tests**, all passing in the
  workspace gate. The fixture server's **three** offline tests pass.
- `mise exec -- make check`: **2,828 Rust tests passed, five ignored**,
  architecture for 35 packages, formatting, cargo-shear, clippy with warnings
  denied and offline script/JavaScript fixtures; **209.10 s**. Existing Linux
  skips remain for PowerShell (no `pwsh`) and macOS bundle fixtures.
- `mise run quality`: **100.0%**, no findings in changed code. The findings
  scan retains only the 86 existing chart findings. The refreshed baseline
  changes only its function count, keeping its score and warning totals.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **19.76 s**; no debug fixture seam leaks into release.
- `mise run audit`: passed for all three lockfiles, without dependency or
  advisory-policy changes. Existing unmatched-advisory warnings are non-blocking.

No live-service verification or GUI capture was performed. The next slice
maps these DTOs into Markdown preparation and builds the visible issue view;
the reference resolver remains O'Brien's separate work.

### Review fixes

The captain's triage of the cross-model review of `e1a425c` is addressed on
the same branch. A fresh fetch still has `origin/feat/subspace-desktop` at
`5f40a4385559`, so no integration merge was needed. Public API signatures
and DTOs are unchanged.

- **P1, opening during bootstrap:** `open_issue` retains a content-free
  Loading snapshot until the first organisation install. A successful
  bootstrap requeues the issue at visible priority under the new generation;
  no-organisation states close it, and bootstrap errors become its error.
  Other scope changes still discard it. The captain's decision is recorded
  in spec section 4, dated **15 November 2026**. Tests in
  `hub/tests/issue/bootstrap.rs` gate the membership response and prove the
  single read, new generation, ChooseOrganization/NoEligibleOrganizations,
  account/read errors, and cancellation by profile change or sign-out.
  `pending_issue_is_queued_at_visible_priority` proves queue precedence.
- **P2-1, list/teams refusals:** a scope-clearing refusal now retains the
  open identifier/profile with the same Error reason for every target. Its
  detail is discarded. `list_and_teams_refusals_preserve_an_issue_error_shell`
  proves both routes after loading a real fictional detail. Fatal session
  failures use the same content-free shell handling.
- **P2-2, child-page error states:** the page dispatcher explicitly reserves
  row-level failure for transport, unexpected-status and invalid-response
  errors. Other failures use the detail's shared mapper. The existing
  401/403/404 routing is retained; edge challenges now use it too. Tests in
  `hub/tests/issue/page_errors.rs` prove edge, second 401, membership,
  permission and pack errors drop content, while transport, 5xx, unexpected
  status and invalid JSON retain the issue for row Retry.
- **P2-3, stale refusal reads:** `refused_for` checks the scope and target
  revision before attempting membership discovery, retaining the existing
  check after discovery too. `stale_refusals_do_not_read_organizations`
  proves stale list and issue revisions make no additional organisations
  request and publish no state change.

The tests were written first. Seven regression tests failed against the
reviewed implementation: bootstrap opens showed Unavailable/disappeared,
bootstrap errors failed to update the pending view, list refusals lost the
shell, edge page errors stayed row-level, and stale refusals made a second
organisations request. The separate priority test first dequeued a reference
job instead of the visible issue. All **69 hub tests** then passed after the
fixes. The findings scan is clear in changed code; the quality baseline is
regenerated from the combined tree rather than selecting a prior total.

Final review-fix gates passed in the requested order, with foreground waits:

- Hub tests: **five consecutive runs of 69 tests each at 64 threads**, all
  passing; no compile warnings.
- `mise exec -- make check`: **2,839 Rust tests passed, five ignored**,
  including all **123 NETT client tests**; architecture for 35 packages,
  formatting, cargo-shear, clippy with warnings denied, and offline
  script/JavaScript fixtures passed; **124.97 s**. The existing PowerShell
  and macOS fixture skips apply on Linux.
- `mise run quality`: **100.0%**; no findings in changed code. The findings
  scan retains only the 86 existing chart findings. `mise run quality-baseline`
  regenerated the total as **13,612**, with score and warning totals unchanged.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **1.00 s**.
- `mise run audit`: all three lockfiles passed, with only the existing
  unmatched-advisory warnings. No dependency or advisory-policy changes.

No desktop app was launched and no live NETT/WorkOS calls were made. The
captain's final Opus review follows; no PR is opened by this worker.

### Final review fixes

The captain accepted the final Opus review's four P2 findings on `f08e6ee`
(no P0/P1). The fixes preserve all public signatures and DTOs:

- **P2-A:** `Target::Children(revision)` distinguishes child-page refusals
  from detail refusals. A 404 with membership, permission and pack intact
  leaves detail shown with row Retry; pack loss retains the account error.
  `child_page_404_with_intact_access_keeps_detail_for_retry` proves the new
  case; the existing child-page pack-loss case still proves content removal.
- **P2-B:** both Reload paths retry a bootstrap error for a content-free
  current-profile issue. Hub and issue return to Loading and the visible
  Bootstrap job reuses the first-install carry-over. The captain's dated
  section 4 decision is extended accordingly.
  `reload_issue_retries_bootstrap_and_reads_the_issue_once` and
  `list_reload_retries_bootstrap_and_reads_the_issue_once` each gate the
  initial 500, then prove exactly one issue read after recovery.
- **P2-C:** rebuilding a content-free shell preserves its status when it
  belongs to another profile or is Unavailable.
  `refusals_and_expiry_do_not_relabel_other_profile_or_unavailable_shells`
  proves both cases through account refusal and session expiry, including
  the existing test seam that simulates the current profile changing.
- **P2-D:** a successful issue read recovering the hub from Error to Ready
  uses the shared scope enqueue operation to reload teams and the list when
  a query exists. `recovering_through_the_issue_reloads_teams_and_the_existing_list`
  proves both reads happen again after a refusal cleared their content.

All five new regression tests first failed against `f08e6ee`: the page 404
became Unavailable; direct issue Retry did nothing; list Reload discarded
the issue; account/session errors relabelled the shell; and successful issue
recovery left teams and list empty. After the fixes, all **74 hub tests**
passed at 64 threads. The child-page response fixture is shared with the
existing access tests to avoid duplicate setup.

Merged `origin/feat/subspace-desktop` at
`3360edc478267b07bbe70d3f84d185f6de56d2f0`, including the Channels Markdown change
and the slice 4 services half. Both delivery records and the spec decisions
merged cleanly. The only conflict was the quality baseline's total; it was
regenerated with `mise run quality-baseline` from the combined tree as
**13,756 functions**, retaining the 100% score and warning totals. No side's
total was selected. This client's code has no calls to the changed
`nett_markdown::Options::new` or `issue_link_identifier` signatures, so no
client call-site adaptation was needed. The merge commit uses
`merge: integrate subspace desktop through the issue workspace implementation slice 4 services`.

Final gates passed on the combined tree, in order, with foreground waits:

- Hub tests: **five consecutive runs of 74 tests at 64 threads**, all passed.
- `mise exec -- make check`: **2,905 Rust tests passed, five ignored**,
  including all **128 NETT client tests**. Architecture for 35 packages,
  formatting, cargo-shear, clippy with warnings denied and offline
  script/JavaScript fixtures passed; **130.67 s**. Existing Linux skips
  remain for PowerShell and macOS bundle fixtures.
- `mise run quality`: **100.0%**, with no findings in changed code. The
  findings scan retains only the 86 existing chart findings.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **1.94 s**, including the merged services APIs.
- `mise run audit`: passed for all three lockfiles, with the existing
  unmatched-advisory warnings; no dependency or advisory-policy changes.

No desktop app was launched and no live NETT/WorkOS calls were made. The
captain reads the combined diff and handles the PR/merge; this worker opens
no PR. Public hub/DTO signatures remain stable; `Target::Children` and the
reused scope enqueue/retry helpers are internal implementation details.

## Slice 5: data half

Work is on `issue-workspace-slice-5-data`, based on the merged slice 3b
`de0a83a43c7fb40af180877364264bef877feec9`. No board UI, writes or new POST
routes are included, and no slice checkbox is changed.

### Contract and public API

Fetched the slot's Indra worktree and read only `origin/main`
`5bfb6c7b9e` with `git show`. Under
`backend/src/Web.Bff/Features/Orgs/IssueTracking/`, checked:

- `Issues/List/Endpoint.cs`, `Issues/Groups/Endpoint.cs`,
  `Shared/Contracts/IssueFilterRequest.cs`, `Shared/IssueListFilters.cs`
  and `Shared/IssueReader.Filters.cs` for shared parameters, finite values,
  AND across filters, status selection, due dates and grouped count reads.
- `Issues/Relations/Endpoint.cs`, `Issues/Relations/Contracts/Types.cs`,
  `Issues/PullRequests/Contracts/Types.cs`, `Shared/Contracts/IssueTypes.cs`
  and `Shared/Contracts/IssuePullRequestTypes.cs` for first pages, independent
  cursors, relation direction and retained pull request facts. Relations allow
  1–100 rows; pull requests allow 1–50; both hub continuations use 20.
- `IssueTeams/List/Endpoint.cs`, `WorkflowStates/List/Endpoint.cs` and
  `Shared/Contracts/Types.cs` for teams and workflow fields. Also read
  `backend/src/Domain/IssueTracking/{IssueType,IssueWorkflowStateType}.cs`
  and the frontend's `workspace-query.ts` and `IssueWorkspaceStatusFilter.tsx`
  to verify camelCase values and intersection with the tab predicates.

`ListQuery` adds these consuming builders (all return `Self`):
`assignee(self, value: Option<&str>)`, `priorities(self, values: &[u8])`,
`states(self, values: &[&str])`, `state_types(self, values: &[&str])`,
`kind(self, value: Option<&str>)`, `due(self, value: Option<&str>)` and
`include_archived(self, include: bool)`. Grouped, folded and more-row reads
use the same query. Invalid values and contradictory My issues/status
selections return `HubError::InvalidQuery` before any state or request changes.
State IDs/names require a team in this client; All teams uses state types.
Priority/status sets are canonicalised for cache equality.

New hub/snapshot entry points:

```rust,ignore
Hub::load_more_relations(&self) -> Result<(), HubError>
Hub::load_more_pull_requests(&self) -> Result<(), HubError>
HubSnapshot::hidden_subissues_count(&self) -> Option<u32>
HubSnapshot::my_issues_count(&self) -> Option<u32>
IssueSnapshot::relations_loading(&self) -> bool
IssueSnapshot::relations_failed(&self) -> bool
IssueSnapshot::pull_requests_loading(&self) -> bool
IssueSnapshot::pull_requests_failed(&self) -> bool
Client::pull_requests(&self, org: &str, reference: &str, page: &PageRequest) -> Result<Page<PullRequest>, Error>
```

Counts are distinct visible-priority jobs queued after the successful list.
Hidden count preserves every filter except parent and uses checked total
arithmetic; My issues uses the spec's exact independent open-state query.
Either failure leaves its count absent without changing the list. Both query
revision and scope generation reject late count results. Scope invalidation
and new queries clear counts immediately; unchanged-query refreshes keep them
until the new count reads replace them.

Relation continuations append unique `(issue ID, direction)` pairs and PR
continuations append unique IDs. Both share duplicate in-flight
calls and expose independent row loading/Retry flags. They reuse the
`Target::Children` account-check pattern through `Target::Rows`: 404 with
intact access gives row Retry; account loss follows the existing shared map.
Cursor 400 queues one fresh issue read. Back, Reload and scope changes retire
old page completions and account checks. Shared hub/worker changes are small;
the new count and related-page logic lives in separate modules for slice 4
integration.

### Board inputs and fixtures

The existing issue fields already cover type, state, assignee/former member,
priority, estimate, due date, labels, cycle, parent/duplicate, project/event/
milestone links, blocking and archive status, native timestamps and provenance.
Team workflow states already carry ID, team ID, name, type, position, default
flag and issue count. No extra workflow/catalogue read or DTO field is needed.
Tracker notes derive from issue type; `fields.progress` supplies done/total,
canceled/duplicate, points, sample and has_more for the header. Blocked notices
use `fields.blocked` and `relations.items[].kind == "blockedBy"`.

Added the missing `IssueDetail.pull_requests: Option<Page<PullRequest>>`
and the full retained PR/repository facts, plus
`IssueFields.pull_request_summary: Option<PullRequestSummary>`.
The summary counts current associations; historical retained rows can exceed
its total, so it cannot promise the number of remaining page rows. Record
presentation also now has `display_dates: Option<DisplayDates>` (value and
NETT/original source) and `archived_by: Option<Person>`. Existing
`IssueDetail.relations` already carries its first page and cursor. All content
remains memory-only with no serialization, persistence or new logs.

The loopback fixture filters one fictional dataset for grouped/count and
ordinary pages, including archived and parent handling, both assignees, all
five priorities, status, type, due dates, team and text. Relation and PR reads
have independent 20+5 pages. Existing cursor and row-error scenarios apply to
these pages. No live NETT/WorkOS service or desktop app was used.

### TDD and gates

Filter tests, count tests and paging/Retry tests were written first and failed
to compile on the absent API. The fixture's two new tests first failed on its
unchanging counts and empty relations. A later page-size regression first
failed because PR pages incorrectly accepted 51/100; it passes with the
verified 50-row cap. DTO coverage first failed on missing display dates,
archive actor and PR summary fields. The toolbar test covers 60 combinations
across grouped, folded and cursor reads. New lifecycle tests hold reads and
prove completed stale results cannot update a new query, scope or closed issue.
Existing request-count tests now distinguish first list pages from counts;
the team-only keychain regression waits for count jobs before expiring access.

Final gates passed in the requested order, with foreground waits:

- Hub tests: **five consecutive passes of 84 tests at 64 threads**. The full
  client suite has **141 passing tests**, including 13 added in this slice.
- `mise exec -- make check`: **2,918 Rust tests passed, five existing
  ignored tests**; architecture (35 packages), board guides, format,
  cargo-shear, Clippy with warnings denied, JavaScript and offline script
  fixtures passed. The NETT fixture suite has five tests, including two new
  contract tests. Total **170.63 s**; the usual PowerShell/macOS fixture skips
  apply on Linux.
- `mise run quality`: **100.0%**, with zero findings in changed code; only
  the 86 existing chart findings remain. The normal baseline refresh changed
  only the total function count, from 13,756 to **13,815**.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **4.51 s**.
- `mise run audit`: passed for all three lockfiles, with the existing
  unmatched-advisory warnings. No dependencies or advisory policy changed.

Fetched `origin/feat/subspace-desktop` before final gates; it remained at
`de0a83a43c7fb40af180877364264bef877feec9`, so no integration merge or baseline
conflict was needed. No PR is opened by this worker; the captain handles review
and integration with the board half.

### Cross-model review fixes

The captain accepted P2-1 and P2-3 from the Sonnet 5.5 review of `858a831`.
The two relation regressions and the stale-refresh regression were written
first; all three failed against that head. The relation cases retained only
one direction, and refreshing the unchanged query changed both badges to
None. A fourth regression then reproduced the same clearing in cursor restart.

- **P2-1:** relation identity is `(issue ID, direction)`, including rows
  appended earlier in the same page. PR identity remains its own ID.
  `relations_keep_the_same_issue_under_two_directions_in_one_page` retains
  `blocks` and `relatedTo` for one issue.
  `relations_drop_repeated_edges_across_pages_but_keep_new_directions`
  drops repeated edges from the old page and within the incoming page,
  without losing the new direction.
- **P2-3:** stale activation and explicit reload keep both counts for an
  unchanged query until each count read replaces its value. Cursor restart
  follows the same rule. `an_unchanged_stale_refresh_retains_counts_until_replacements_arrive`
  holds the reader and checks 5/7 remain visible, then observes replacement
  values 9/11. `an_unchanged_query_cursor_restart_keeps_both_count_badges`
  covers cursor recovery. The existing
  `query_and_scope_changes_clear_counts_and_ignore_old_completions` and
  held late-count test still prove clearing and stale-result rejection.
- **P2-2 decision [D], 15 November 2026:** counts remain at
  `ReadPriority::Visible`, behind the list. My issues is visible in the tab
  and the hidden sub-issues summary is visible below the list; both use
  cheap `limitPerGroup=0` reads. Recorded in spec section 4, with no runtime
  priority change.

Merged `origin/feat/subspace-desktop` at `d7f1007` (the two macOS packaging
commits) cleanly before final gates. No baseline conflict occurred. No public
API, fixture or dependency changed, and no live NETT/WorkOS calls or desktop
launch were used.

Final gates passed in order, with foreground waits:

- Client: **145 tests passed**, including the four new review regressions.
  Hub: **five consecutive runs of 88 tests at 64 threads**, all passed.
- `mise exec -- make check`: **2,922 Rust tests passed, five existing
  ignores**, plus architecture (35 packages), format, cargo-shear, Clippy
  with warnings denied, board guides and offline script/JavaScript fixtures;
  **147.23 s**. Existing Linux PowerShell/macOS fixture skips remain.
- `mise run quality`: **100.0%**, zero findings in changed code; only the
  86 existing chart findings remain. The normal baseline refresh changed
  only the function total, from 13,815 to **13,822**.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **3.83 s**.
- `mise run audit`: all three lockfiles passed with the existing
  unmatched-advisory warnings. No dependency or advisory-policy changes.

## Evidence

The approved design's key captures are in [evidence/design/](evidence/design/)
([design.md](design.md) lists them). They are mocks drawn with fixture data,
not app captures. Each UI slice adds its own captures from the capture build
under `evidence/`. Slice 2b's are listed in its
[captures section](#captures-and-remaining-differences). Slice 3a adds no
visible product surface.

## Slice 4: services half (15 November 2026)

Added the shared `subspace_issues::issue_link` parser and bare app-relative
candidate finder. Both IRC and Markdown can use the same origin, path and
exact-key rules. Explicit Linear ports (including `:443`) and user information
(including an empty username) are rejected before URL normalisation can erase
them. Markdown now imports the shared parser, same-app path helper and finder;
it now borrows the same `ReferenceRecognizer` through a link context, preserving
its identifier grammar and key selection. The old private
copies are removed and all existing test expectations are unchanged.

Added `subspace_issues::resolution_cache`, generic over a caller-owned card.
The cache holds at most 500 resolved entries, touches LRU order on positive and
negative hits without extending expiry, and deduplicates pending reads. Cards
expire five minutes after completion; post-pack-check unavailable results
expire after one minute. Other errors release their read without caching.
Changing profile, organisation or generation clears cards and pending reads;
an old completion cannot end a current pending read. The hub must use a fresh
generation for each lifecycle change and return every ticket to the same cache.
Pending reads are tracked separately from the resolved-entry capacity so
eviction cannot start duplicate network reads. There is no I/O, logging,
serialization or GPUI dependency; card-bearing types do not implement `Debug`.

Public entry points (under their respective modules):

```rust
pub fn issue_link_identifier(url: &str, context: &IssueLinkContext<'_>) -> Option<String>;
pub fn relative_issue_addresses(text: &str) -> Vec<(Range<usize>, &str)>;

impl ResolutionScope {
    pub fn new(profile: &str, organisation: &str, generation: u64) -> Self;
}
impl<C> ResolutionCache<C> {
    pub fn new(scope: ResolutionScope) -> Self;
    pub fn set_scope(&mut self, scope: ResolutionScope);
    pub fn request(&mut self, identifier: &str, now: Instant) -> ResolutionLookup<'_, C>;
    pub fn finish(&mut self, read: ResolutionRead, outcome: ResolutionOutcome<C>, now: Instant) -> bool;
}
impl ResolutionRead {
    pub fn identifier(&self) -> &str;
    pub fn generation(&self) -> u64;
}
```

`ResolutionLookup` is `Available(&C)`, `Unavailable`, `Pending` or
`Start(ResolutionRead)`. `ResolutionOutcome` is `Available(C)`, `Unavailable`
or `Error`. Only `Start` authorises a new read; `finish` consumes its ticket
and returns false for a stale result. `relative_issue_addresses` takes plain
text segments only (the caller excludes code and link destinations), retains
slice 3a's boundaries and punctuation trimming, and returns candidates for the
parser to validate. Crate/module docs describe both new capabilities.

TDD: wrote the four link/finder tests and eight cache tests before implementing
the APIs. The first run failed to compile on the absent modules; compiling
stubs then produced two expected link/finder failures and eight cache failures.
The implementation made all twelve pass. A thirteenth test first reproduced
an explicit-port bypass using extra authority slashes, then passed after the
raw-authority helper rejected that malformed spelling. Coverage includes every
requested URL form, UTF-8 byte ranges, both TTL boundaries, a recent hit surviving the
501st insertion, each scope component, stale completions, retry after errors
and duplicate pending reads. Tests pass caller-supplied instants and never
sleep. All 54 service tests pass, including the unchanged 23 Markdown tests
and nine reference tests.

All gates passed in the requested order with foreground waits:

- `mise exec -- make check`: 119.16 s on the final implementation;
  architecture (35 packages), format, cargo-shear, Clippy with warnings denied,
  2,736 Rust test passes (five existing ignored tests), 88 Python tests,
  111 JavaScript tests and the offline shell fixtures. The service's 54 tests
  include 13 new tests. PowerShell and macOS fixture skips remain on Linux.
- `mise run quality`: 100.0%, zero findings in changed code. The separate
  findings scan reports only the 86 existing chart findings. The normal
  `mise run quality-baseline` command refreshed only the function count, from
  13,189 to 13,283; scores and thresholds are unchanged. Two documented
  `qual:api` markers describe the cache and scope setter's pending hub consumer.
- `mise run audit`: all three lockfiles pass. No dependencies, lockfiles or
  advisory exceptions changed.

No slice checkbox is changed; client/hub integration and IRC presentation
remain with the later workers. No live NETT, WorkOS or application-service
calls were made. There are no unresolved implementation questions.

### Review fixes and base integration (15 November 2026)

- P2-1: `reference::valid_identifier` is now the single crate-private
  identifier predicate used by text and link recognition. It reuses
  `valid_key`, rejects leading zeroes and retains the technical-name exclusions.
  `links_use_the_same_identifier_rule_as_plain_references` tests both entry
  points against `TST-0`, `TST-0827`, `TST-007`, supplied one- and six-character
  keys, `UTF-8` and `ISO-8601`, across all three link origins.
- P2-2: pending reads keep their start instant and a private ticket serial.
  `request` abandons them at 60 seconds; late tickets cannot remove a newer
  pending read or overwrite its cached result. `ResolutionRead` and `finish`
  require callers to finish every read, using `Error` on cancellation; timeout
  is only a safety net. Tests are
  `abandoned_reads_restart_at_sixty_seconds`,
  `late_abandoned_completions_cannot_end_the_replacement_read` (all outcomes),
  and `late_abandoned_completion_cannot_replace_a_new_cached_result`.
- P2-3: the captain accepted a deliberate change from slice 3a: absolute
  app URLs without `//` remain ordinary links in Markdown instead of being
  flattened as app links. `absolute_links_require_two_authority_slashes` and
  `app_urls_without_authority_slashes_remain_ordinary_links` pin both
  `https:host/path` and `https:/host/path`. Existing expectations are unchanged.
- P2-4: `queries_and_fragments_do_not_change_the_identifier` pins Linear
  queries/fragments and relative fragments; `percent_encoded_identifiers_remain_ordinary_links`
  rejects `TST%2D827` and `TST-8%32` across all three origins. Cache tests
  `unavailable_hits_move_to_the_most_recent_lru_position` and
  `expired_entries_are_removed_before_evicting_a_live_lru_entry` cover both
  eviction gaps. `stale_scope_error_leaves_the_new_pending_read_intact` adds
  the error case to the existing stale-generation success regression.

TDD: all 11 review tests were added before the implementation changes. The
first run reproduced four failures: the shared-grammar case and all three
abandonment cases. The seven accepted-behaviour and coverage-gap tests already
passed. After the fixes all 65 service tests passed; no existing expectation
changed. Shared cache-fixture setup removed the new quality duplication
findings. Public API signatures are unchanged; the predicate, timing and
serial state are private. Final merged-tree gate results follow below.

Integrated `origin/feat/subspace-desktop` at `5f40a43` after fixes commit
`d8ac6fd`. Both appended delivery records were retained when resolving the
document conflict. `mise run quality-baseline` regenerated the baseline on
the merged tree (13,594 functions, 100.0%); neither side's total was selected.
The first merged-tree check stopped when Cargo could not execute a missing
`subspace-mxc-worker` test binary, without a test assertion failure. A targeted
rerun rebuilt that package and passed all 23 tests, with no source change.

Final gates passed on the merged tree, in order, with foreground waits:

- `mise exec -- make check`: 267.87 s; architecture (35 packages), format,
  cargo-shear, Clippy with warnings denied, 2,837 Rust test passes (five
  existing ignored tests), 90 Python tests, 111 JavaScript tests and offline
  shell fixtures. The service has 66 passing tests after the base merge,
  including all 11 review additions. The usual PowerShell and macOS fixture
  skips remain on Linux.
- `mise run quality`: 100.0%, zero findings in changed code; only the 86
  existing chart findings remain. The regenerated baseline retains all scores
  and thresholds and records 13,594 functions.
- `mise run audit`: advisories accepted for all three lockfiles, with no
  dependency or advisory-exception edits in these fixes.

No live application-service calls or PR creation were needed. No public API
signature changed and no implementation question remains open.

### Final review fixes (15 November 2026)

Post-review integration: merged `db8831f` (slice 5 data and docs index); `hub.rs` keeps both modules, `worker.rs` keeps reference/count/row dispatch and reader leases, `worker/error.rs` keeps Reference and Rows in the shared mapper, `nett_test_server.py` keeps archived cards plus enriched detail/paging, `test-nett-test-server.py` keeps both suites, and the conflicting baseline total was regenerated; new count/row producers use the shared cancellation-aware enqueue path without changing priorities, generations, filters, count retention or paging dedupe.

Post-merge gates passed sequentially: hub **124 tests × five runs at 64 threads**
(620 executions), resolution cache **18**, client **181**, and fixture server
**six**; `mise exec -- make check` passed **2,962 Rust tests, five ignored** in
**217.14 s**, including the new docs index check (six checker tests; all pages
reachable and local links valid). Quality remains **100.0%**, with no findings
in changed code and only the 86 existing chart findings; the regenerated
baseline contains **13,918 functions** with unchanged thresholds. Release
client check passed in **2.17 s**, and all three lockfile audits passed with
the existing unmatched-advisory warnings. All public API names, including
`irc_*`, are unchanged. No live NETT/WorkOS service or desktop app was used.

- P2-1: absolute authorities are taken immediately after the first scheme
  colon and must start with `//`. Validation runs before URL parsing and
  rejects tabs, LF and CR anywhere, plus leading/trailing C0 controls and
  spaces. `a_later_delimiter_cannot_supply_the_absolute_authority` covers all
  five reproduced bypasses; `absolute_links_reject_characters_that_url_parsing_would_strip`
  covers both origins and all edge C0 characters. The Markdown regression
  `later_delimiters_do_not_turn_malformed_urls_into_app_links` proves a later
  delimiter cannot revive a malformed link.
- P2-2: NETT issue links require every decoded `orgId` to match the current
  organisation, case-insensitively. Spec section 4 records the captain's
  decision. `nett_links_require_every_decoded_organisation_to_match` covers
  matching, upper-case, mismatched, duplicate and encoded values/keys on
  relative and absolute URLs. `linear_links_are_unaffected_by_organisation_queries`
  pins the Linear exception. `markdown_preserves_issue_links_naming_another_organisation`
  proves issue links are preserved while non-issue app-link flattening remains.
- P2-3: `ReferenceRecognizer::recognises` checks complete identifiers using
  the recogniser's selected keys. `find` and the link parser call it, so the
  duplicated Markdown `references.find` check is removed. Tests
  `recognises_uses_the_selected_keys_and_complete_identifier_grammar`,
  `text_and_links_use_the_same_fallback_key_set`, and
  `markdown_uses_the_contexts_fallback_recognizer` prove the shared fallback.

The small `IssueLinkContext` binds app origin, organisation and a borrowed
recogniser; Markdown options own that context. No independent key list is
accepted by the parser or Markdown. The final public signatures are:

```rust
impl ReferenceRecognizer {
    pub fn recognises(&self, identifier: &str) -> bool;
}
impl<'a> IssueLinkContext<'a> {
    pub fn new(app_origin: &'a str, organisation_id: &'a str, references: &'a ReferenceRecognizer) -> Self;
}
pub fn issue_link_identifier(url: &str, context: &IssueLinkContext<'_>) -> Option<String>;
impl<'a> Options<'a> {
    pub fn new(issue_links: IssueLinkContext<'a>, media: &'a [Media], mentions: &'a [MentionStatus], stage: Stage) -> Self;
}
pub fn prepare_nett_markdown(source: &str, options: &Options<'_>) -> String;
pub fn relative_issue_addresses(text: &str) -> Vec<(Range<usize>, &str)>;
```

TDD: the three authority/Markdown regressions first failed against `1955cb9`,
then passed after the raw-input validation fix. The six context/organisation/
recogniser tests first failed to compile against the missing API; after the
context was introduced, the two organisation tests failed behaviorally until
scope checking and Markdown preservation were added. All 75 service tests now
pass. Existing expectations are unchanged; the shared fixture adapter and
fixture options now supply an explicit organisation for both service and
native rendering tests. Cache source and tests are unchanged; P2-4 remains
for the next hub brief. The accepted fixes are committed as `f73d3db`.

The base subsequently advanced from `5f40a43` to `36107bb` (the Channels Markdown change
Markdown). Merge commit `33858bf` integrates it cleanly; both delivery records
are retained. There were no conflicts. The normal `mise run quality-baseline`
command refreshed the combined tree's function count before the merge commit.

Removing the independent Markdown `team_keys` input also exposed rustqual's
previously masked test-only `IrcSnapshot::team_keys` accessor. A single
`qual:api` comment documents its already-planned IRC consumer, matching the
adjacent snapshot accessors; no client behavior changed.

Final gates passed on the merged tree with foreground waits, in the requested order:

- `mise exec -- make check`: 117.85 s; architecture (35 packages), format,
  cargo-shear, Clippy with warnings denied, 2,874 Rust test passes (five
  existing ignores), 90 Python tests, 111 JavaScript tests and offline shell
  fixtures. The service has 75 passing tests, including nine new final-review
  regressions. Native rendering still passes every unchanged shared fixture.
  The usual PowerShell and macOS fixture skips remain on Linux.
- `mise run quality`: 100.0%, no findings in changed code; only the 86
  existing chart findings remain. The normal baseline refresh changes only
  the function count, from 13,609 to 13,681; scores and thresholds are unchanged.
- `mise run audit`: all three lockfiles pass. No dependency, lockfile or
  advisory-exception changes were needed.

## Slice 4: hub half (15 November 2026)

Merged `issue-workspace-slice-3-data` at `f08e6ee` before implementation
(merge commit `9ce93d3`). The baseline conflict was regenerated with
`mise run quality-baseline`, rather than selecting either total. This builds
on the issue view's generation checks, session reader and shared error mapper.

The hub now owns one `ResolutionCache<ReferenceCard>`. Scope invalidation
finishes queued reference tickets with Error, clears the cache and immediately
discards the retained IRC cards. In-flight and abandoned completions cannot
publish or apply account errors to a replacement scope/read. References have
no pending-open exception during bootstrap. `IssueResolver::resolve` runs on
the existing two workers at `ReadPriority::Reference`, behind visible reads.
The worker immediately drops all detail except identifier, title, state, type
and archived. No card-bearing type supports Debug or serialization.

New public API (existing snapshot metadata readers remain unchanged):

```rust
pub fn irc_resolve_reference(identifier: &str, profile_id: &str);
impl Hub {
    pub fn resolve_reference(&self, identifier: &str, profile_id: &str);
}
impl IrcSnapshot {
    pub fn cards(&self) -> &[ResolvedReference];
    pub fn reference_error(&self, identifier: &str) -> Option<&ReadError>;
}
impl ResolvedReference {
    pub fn identifier(&self) -> &str;
    pub fn card(&self) -> Option<&ReferenceCard>;
}
impl ReferenceCard {
    pub fn identifier(&self) -> &str;
    pub fn title(&self) -> &str;
    pub fn kind(&self) -> &str;
    pub fn state(&self) -> &StateSummary;
    pub fn archived(&self) -> bool;
}
impl<C> ResolutionCache<C> {
    pub fn cards(&self, now: Instant) -> impl Iterator<Item = (&str, Option<&C>)>;
    pub fn peek(&self, identifier: &str, now: Instant) -> Option<Option<&C>>;
    pub fn is_pending(&self, read: &ResolutionRead) -> bool;
}
```

Both demand calls take only registry/state/queue locks. Wrong-profile,
disabled/stopped, signed-out or missing-organisation calls do nothing; the
shared recogniser also rejects invalid or unrecognised identifiers. Cache
hits and pending reads enqueue nothing. IRC's version check reads live cache
entries without changing LRU or starting work; sorted cards keep hits from
changing versions, and expiry removes cards on the next timer read.

Success is Available for five minutes. An issue 404 rechecks organisations
and becomes Unavailable for one minute only when membership, permission and
the pack are intact. 403 uses the same recheck/account mapping as issue reads.
401 silently refreshes/retries once, then expires the session and disables
recognition; revoked refresh signs out. Edge, transport, 5xx and invalid JSON
finish as Error without caching. The IRC projection exposes the shared typed
error reason for the current account or last failed reference; the latter is
one transient diagnostic slot, replaced on the next Start, never a negative
cache entry. Presentation supplies the approved neutral/unreachable wording.

TDD: the cache read tests first failed against missing read APIs. Hub demand,
card and error tests likewise first failed against absent APIs. The abandoned
job regression then reproduced an old ticket incorrectly changing Ready to
PermissionLost; exact pending-ticket checks now prevent it. The scope-clear
regression reproduced cards retained until the next snapshot read; clearing
now projects immediately. Deterministic clock tests pin both TTL boundaries,
stable versions on hits/pending work and absence of implicit reads. Worker
tests cover queue cancellation and stale completions. Log canaries cover both
the retained card and discarded detail through actual reference workers.
Reference errors are staged by the shared mapper and published under the
state lock only when their ticket finishes, so Retry cannot observe an error
while that same read remains pending.

The loopback fixture adds `TST-1613`, fictional archived launch notes. Its
contract test first returned 404, then passed after that fixture was added.
Existing `TST-1612` and `TST-999999` supply available/unavailable evidence.
No IRC/board-kit presentation files are changed and no desktop app or live
NETT/WorkOS service is used. The slice checkbox remains for the Claude half.

Final base check: fetched origin again; `origin/feat/subspace-desktop` remains
`3360edc47826` and does not contain Bashir's work. Re-merging the requested
`issue-workspace-slice-3-data` (`f08e6ee18dfd`) reports already up to date.
No further integration commit was needed.

Final gates passed, in order, with foreground waits:

- Hub tests: **five consecutive runs of 85 tests at 64 threads**, all passed.
- `mise exec -- make check`: **2,920 Rust tests passed, five existing ignores**,
  including all **139 NETT client tests** and **79 issue-service tests**.
  Architecture (35 packages), formatting, cargo-shear, Clippy with warnings
  denied and offline script fixtures passed, including the four fixture-server
  contract tests. Usual PowerShell/macOS skips apply on Linux. The first run
  stopped on a missing, never-executed `subspace-issues` test binary; rebuilding
  the target passed its four library tests, then the full retry passed in
  **293.47 s**, without a source change.
- `mise run quality`: **100.0%**, zero findings in changed code. The separate
  findings scan contains only the 86 existing chart findings. The regenerated
  baseline records **13,798** functions; scores and thresholds are unchanged.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **18.29 s**.
- `mise run audit`: all three lockfiles passed, with the existing unmatched
  advisory warnings. Dependencies and advisory policy are unchanged.

No implementation blocker remains. The branch is delivered for the captain's
review; no PR is opened by this worker.

### Base integration for review (15 November 2026)

Merged `origin/feat/subspace-desktop` at `de0a83a43c7f` (PR #12), including
`0bbb2fb`, in `47d3fef` with the requested merge message. Both branches'
delivery sections are retained. Conflicts were resolved by behavior:

- `worker/error.rs` keeps `Target::Children`, `children_failed`,
  `intact_missing` and `restore_issue`'s keep rule. References use the same
  refusal/session mapper. Their distinct target remains necessary: its
  validity depends on the pending ticket serial, it has no list/issue content
  for Offline, and transient errors must wait for ticket completion. Treating
  a reference as Issue, Children or List would respectively mark the open
  issue unavailable, fail its child page or invalidate the scope on an intact
  reference 404. The reference arm of `intact_missing` returns the unavailable
  outcome without changing the open view, retaining the shared version update
  for refreshed organisation metadata.
- `worker/issue.rs` keeps recovery through `enqueue_scope` and dispatches
  child refusals with `Target::Children`; its shared-mapper boolean is ignored
  by the view callers, as intended.
- The conflicting baseline total was regenerated from the merged tree with
  `mise run quality-baseline`, then refreshed for the added tests. It records
  13,813 functions with scores/thresholds unchanged; no side's total was chosen.

Scope audit: `clear_scope` is still the sole production generation increment
and calls `set_scope` before updating the retained IRC projection.
`retry_opening_issue` changes the issue revision, not scope generation; its
successful bootstrap installation calls `invalidate`/`clear_scope` before
`enqueue_scope`. A refused issue read already invalidates the scope; successful
issue recovery retains that new generation while requeuing teams/list data.
The issue shell keep rule never retains reference cards.

Four additional offline regression tests passed on the merged implementation:

- `bootstrap_retry_installs_a_fresh_reference_scope_without_carrying_cards`:
  seeds available/unavailable cards and an old pending ticket, retries a
  failed bootstrap, verifies generation changes at installation, then proves
  resolution requires a fresh read. References requested during retry are
  discarded while there is no organisation.
- `issue_recovery_requeues_the_scope_without_reviving_old_reference_cards`:
  seeds the same state, refuses the issue read, recovers its teams/list through
  `enqueue_scope`, and proves the old ticket/cards remain retired.
- `kept_unavailable_navigation_never_keeps_reference_cards_after_a_refusal`:
  tests own-profile Unavailable and mismatched-profile navigation; the shell
  stays unavailable while cached cards and pending tickets are discarded.
- `a_missing_reference_preserves_the_loaded_issue_and_its_children_state`:
  pins the distinct intact-reference-404 behavior against the merged mapper.
  It also changes the organisation name in the membership recheck. The first
  merged resolution skipped the shared version update; this regression failed
  before that update was restored and passes with metadata publication kept.

Final merged-tree gates passed in order, with foreground waits:

- Hub tests: **five runs of 94 tests each at 64 threads**, all passed after
  the metadata-publication fix.
- `mise exec -- make check`: **2,929 Rust tests passed, five existing ignores**,
  including all **148 NETT client tests** and **79 issue-service tests**;
  architecture (35 packages), formatting, cargo-shear, Clippy with warnings
  denied and offline fixtures passed. Final run: **131.64 s**. The usual
  PowerShell/macOS fixture skips apply on Linux.
- `mise run quality`: **100.0%**, no findings in changed code. The separate
  findings scan retains only the 86 existing chart findings.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **2.01 s**.
- `mise run audit`: all three lockfiles passed with existing unmatched-advisory
  warnings. No dependency, lockfile or advisory-policy changes.

No public API changed during integration. No desktop app was launched and no
live NETT/WorkOS calls were made. This delivery stops for the captain's cross-
model and final reviews; no PR is opened by this worker.

### Cross-model review fixes (15 November 2026)

Addressed the four accepted P2 findings on reviewed head `4fdc160`:

- **P2-1:** `PendingReference` owns the read ticket before any resolver or
  conversion runs. Its drop path finishes Error unless normal completion has
  consumed the ticket. `a_panicking_card_conversion_finishes_the_ticket_immediately`
  runs the actual scalar fixture read with a panicking conversion, catches the
  unwind and proves the same identifier starts again without waiting for TTL.
  The reader loop catches request unwinds so both workers remain available.
- **P2-2:** Queue `push`/`stop` return rejected/drained jobs, just as `clear`
  does. Every production producer goes through `Shared::enqueue`, and Stop/
  invalidation/drop use the same cancellation helper. Tests
  `stopping_a_queue_returns_its_reference_jobs_for_cancellation` and
  `pushing_to_a_stopped_queue_returns_the_reference_job_for_cancellation`
  prove immediate retry after caller cancellation. The additional
  `the_hub_cancels_a_reference_rejected_by_its_queue` covers that caller path.
- **P2-3:** The queue atomically reserves at most one active reference. While
  reserved, another reader skips the reference tier and takes visible/later
  pages. A drop lease releases the reservation even on stale work or unwind.
  `a_second_reference_cannot_block_a_new_visible_read` holds the reference
  response, queues another reference and proves visible work starts before
  release. Queue tests cover later pages and permit release on unwind.
- **P2-4:** Replaced the single failure slot with a map bounded to 500 keys,
  evicting the oldest completed failures first. Retry removes only its key;
  scope changes clear the whole map and retained projection. Snapshot errors
  remain keyed by identifier and never suppress a read. Tests cover X's error
  surviving Y's Start, retry clearing only X, capacity/replacement order and
  immediate scope clearing of every error. Account-error precedence is kept.

TDD: the held-reader and X/Y error-preservation tests first failed behaviorally
on the reviewed implementation. Panic-injection/queue-return/map tests first
failed to compile against the missing seams; after implementation they pass.
The real panicking conversion is caught by its test and releases its ticket.
No card content is formatted or logged by the guards or scheduling state.
The immutable snapshot's public API, including
`reference_error(&self, identifier: &str) -> Option<&ReadError>`, is unchanged;
all new queue, guard and diagnostic-map types are internal to the hub.

Before the gates, fetched origin: integration remains `de0a83a43c7f`, already
merged into this branch, so no new integration merge is required. The findings
scan is clear in changed code; its 86 existing chart findings are unchanged.

Final foreground gates passed, one at a time:

- Hub tests: **106 passed**, five consecutive runs at **64 threads** (530
  successful executions; 4.20, 4.11, 4.13, 4.14 and 4.15 s). This adds 12
  regression tests to the reviewed head.
- `mise exec -- make check`: **2,941 Rust tests passed, five ignored**,
  including all **160 NETT client tests**; architecture (35 packages), format,
  cargo-shear, Clippy with warnings denied and offline fixtures passed,
  **100.74 s**. PowerShell/macOS fixture skips are unchanged on Linux. The
  first full check caught the obsolete priority-queue `clear` method after
  cancellation moved to draining jobs; it was removed before these final runs.
- `mise run quality`: **100.0%**, no findings in changed code; the 86 existing
  chart findings are unchanged. Regenerated baseline: **13,843 functions**,
  with no quality-score or threshold changes.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **1.27 s**.
- `mise run audit`: all three lockfiles passed with existing unmatched-advisory
  warnings; no dependency, lockfile or advisory-policy changes.

No desktop app was launched and no live NETT/WorkOS calls were made. This
delivery stops for the captain's final Opus review; no PR is opened by this
worker.

### Final review fixes (15 November 2026)

Merged integration `d7f1007a6e7e44eccfb62ecb2d66e4918a89c0b9`, including the
two macOS packaging commits since `de0a83a`, without conflicts.

- **P1-1:** The IRC projection's shared account-error filter now admits only
  `SessionExpired`, `MembershipLost`, `PermissionLost`, `PackUnavailable`,
  `KeychainUnavailable` and `Session`. List `Network`, `EdgeAccessNeeded` and
  `Offline` no longer override a reference's own result or change its version.
  `a_list_network_error_and_recovery_do_not_change_reference_errors_or_version`
  and `a_list_offline_error_and_recovery_do_not_change_reference_errors_or_version`
  load a real fixture list and card, fail a reload, and verify both identifiers
  remain error-free, the card is unchanged, and failure/recovery preserve the
  IRC version. Both tests first failed on the leaked reference error.
- **P2-1:** Demand resolution uses the same account-error filter to reject
  hovers while recovery requires board Reload or sign-in. The combined
  `account_refusals_are_published_for_every_identifier_without_starting_more_reads`
  covers permission, membership and pack loss: the account reason is still
  published for every identifier, no ticket/job or HTTP request starts, and
  generation/version stay fixed. It first failed on the unexpected pending
  ticket in PermissionLost.
- **P2-2:** Strengthened
  `every_scope_change_discards_cards_and_ignores_late_completions` and moved it
  into the reference scope test module. After releasing the held response, it
  waits for the reader's reference lease to end; settings/organisation cases
  also wait for Ready with a live organisation. The live snapshot excludes
  TST-99 before cleanup Stop. A test-only queue observer synchronizes completion
  without changing production scheduling or the public API.
- **P2-3:**
  `a_panicking_reference_leaves_both_readers_available_for_concurrent_work`
  sends a normal reference job through the real reader loop using a credential
  fixture that panics once during read. After its ticket finishes, it holds
  another reference response and proves a visible list starts concurrently.
  Removing the reader's `catch_unwind` makes this test fail with “both real
  readers must survive the panic”; restoring it passes. No new production
  panic hook or dispatch seam is needed.

`docs/issues.md` now describes the account-error boundary and suppressed hover
reads. Public signatures are unchanged. All network evidence uses loopback
fixtures; no desktop app or live NETT/WorkOS service was used.

Final foreground gates passed in order, one at a time:

- Hub: **110 passed**, five consecutive runs at **64 threads** (550 successful
  executions; 4.16, 4.16, 4.75, 4.17 and 4.41 s). Four new tests plus the
  strengthened existing scope test cover the final-review findings.
- `mise exec -- cargo test -p subspace-issues --test resolution_cache`:
  **18 passed**.
- `mise exec -- make check`: **2,945 Rust tests passed, five ignored**,
  including all **164 NETT client tests**; architecture (35 packages), format,
  cargo-shear, Clippy with warnings denied and offline fixtures passed,
  **134.49 s**. The usual PowerShell/macOS fixture skips apply on Linux.
- `mise run quality`: **100.0%**, no findings in changed code; the separate
  findings scan retains only the 86 existing chart findings. Regenerated
  baseline: **13,852 functions**, with no score or threshold changes.
- `mise exec -- cargo check -p subspace-nett-client --release`: passed,
  **1.73 s**.
- `mise run audit`: all three lockfiles passed with existing unmatched-advisory
  warnings. No dependency, lockfile or advisory-policy changes.

This delivery stops for the captain's focused re-review; no PR is opened by
this worker.

## Hub follow-ups

the issue workspace implementation closes the four P2s from the focused hub check, without public API
changes or live service calls:

- Sign-out marks its clear/delete window pending. Readers cannot obtain a
  session client in that window; the flag clears under the session lock even
  when deletion fails. `readers_cannot_restore_content_between_the_sign_out_clear_and_delete`
  first failed with a usable client after the clear. It seeds a real fixture
  list and IRC scope and pauses at the production clear/session-lock boundary.
- Rollback accumulates deletion failures and retains whether the pending
  delete covers any credential or just its own secret. A pending sign-out
  delete cannot be narrowed. `rollback_cannot_clear_or_narrow_a_failed_sign_out_delete`
  first failed after sign-out, save and fallback-delete failures followed by
  retirement. `failed_rollback_retry_keeps_a_newer_credential` first failed
  because retry deleted a replacement secret. Both include load-failure cases;
  a third test proves eventual deletion of a matching secret.
- Lifecycle clear assertions now seed content; Cancel's unseeded clear
  assertions are removed. A recording store proves the operation order
  `save → rollback load → rollback delete → sign-out delete`. Two-second
  race-test limits are raised to ten seconds.
- Shared owns the immutable WorkOS client clone outside the session lock.
  `superseding_attempt_reaches_waiting_while_the_old_install_is_blocked`
  first failed because the replacement could not poll until the old install
  was released; it now reaches Waiting while that install remains blocked.

The documented cross-hub/process conditional-delete race is unchanged.
`docs/issues.md` still describes the resulting lifecycle correctly.

Verification passed: all 98 client tests at 64 threads, then five consecutive
hub runs of 45 tests each at 64 threads. Foreground gates passed in order:
`mise exec -- make check` (2,779 Rust tests passed, five ignored, plus offline
fixtures; 121.62 s), `mise run quality` (100.0%, no findings in changed code),
`mise exec -- cargo check -p subspace-nett-client --release`, and
`mise run audit` (all three lockfiles). The usual PowerShell and macOS fixture
skips apply on Linux. No public API or UI changes were needed.

## Slice 3 read-only issue view

The issue page lives in the Issues board and reads the existing hub snapshot.
Back preserves the list state. Parent and child rows navigate within the
board. The Markdown adapter maps NETT DTOs to the existing pre-processor's
plain-text stage, and the hub owns cursor reads and discards results that
belong to an issue that has closed.

The macOS ARM64 capture build ran in the background with an isolated home,
Cupertino Dark, and the loopback NETT and WorkOS fixtures. The scalar fixture
used the recorded tracker with 25 sub-issues, the fictional ordinary issue
with a parent and a long Markdown body, and delayed or refused scalar reads.
List reads and sign-in use the repository's recorded fixtures. The screenshots
show fictional issue data.

| Capture | State |
| --- | --- |
| Before | On slice 2, Enter leaves the tracker row selected in the list. |
| Tracker | Tracker header, state, provenance, description and the first sub-issues. |
| Issue | Parent path and prepared Markdown with plain mentions and issue references. |
| Long description | The body scrolls while Back and the parent stay available. |
| Show more | The continuation offers five more sub-issues. |
| Page loaded | The next page appears at the end of the existing children. |
| Back | Escape returns to the selected tracker row. |
| Loading | The issue has a skeleton and Back is available. |
| [Expired session](evidence/slice-3-macos/error-session.png) | The issue view offers Sign in and Retry. |
| [Access refused](evidence/slice-3-macos/error-access.png) | Account-level wording and organisation recovery. |
| Unavailable | Neutral wording with Back. |
| Network failure | Retry, Copy details and Show log. |
| [Cloudflare One](evidence/slice-3-macos/error-edge.png) | Edge recovery wording and Retry. |
| Offline | The last successful issue read remains visible with Retry. |

## Issue service placement

The remaining `framework/services/issues` code now lives with its consumers:

- `clients/nett::{credential, profile, reference, resolution_cache}` owns
  refresh-token storage, profile preferences, `IssueResolver` and the resolution
  cache, with their original unit and integration tests. All client and board
  callers use these paths; recognition remains in the contracts crate.
- `spaces/issues::nett_markdown` owns the pure NETT pre-processor and its
  original tests. The JSON fixture and shared test adapter moved with it.
  `spaces/issues/tests/prepared.rs` exercises the public dashboard renderer
  with every NETT-prepared fixture, including selected-text assertions.
- Dashboard keeps its parser panic, HTML and guarded-constructor tests, plus
  a literal prepared-input rendering test. It has no dependency on the NETT
  client or Issues board. Its unused test-only `serde` dependency is removed.

The old service crate and workspace dependency are removed; the workspace's
wildcard member list discovers the remaining crates without an explicit entry.
The lockfiles change only local package dependencies. Audit also refreshed
both excluded Terminal lockfiles with the contracts crate's `url` dependency
from the earlier recognition move. Ownership docs and the
placement task list now name the client, contracts and board modules.
Module bodies and behavior are unchanged. `Hub::publish`, `hub::irc_*` and
`IrcSnapshot` remain as before; phase 3 contract adoption is separate.
No live services were called, and no UI or screenshots changed.

Verification, all in foreground commands:

- The client move passed all 241 tests; dashboard's four rich prepared-input
  tests passed without its NETT dependency. The Issues board passed all 166
  tests, including its moved pre-processor and fixture-to-renderer tests.
- `mise exec -- make check` passed: 3,211 Rust tests, five ignored, plus
  architecture (36 packages), docs, formatting, cargo-shear, Clippy with
  warnings denied and offline script fixtures. The usual PowerShell/macOS
  fixture skips apply on Linux. Cargo-shear retains the existing unlinked
  `spaces/issues/src/tests/lanes.rs` warning.
- The first full run passed every runtime test but failed during Actions-board
  doctest compilation with E0463 loading GPUI/dashboard/Lucide crate artifacts.
  After the overlapping interface build finished, a full rerun passed in
  163.27 seconds with no source changes. No timing-sensitive test failed.
- `mise run quality` passed at 100.0%. `rustqual . --findings` retains only
  the 86 existing chart findings, with none in changed code. Regenerated
  `mise run quality-baseline`: only the function total changes, from 14,785
  to 14,886; no findings, scores, thresholds or suppressions change.
- `mise exec -- make interfaces` passed against the real headless product;
  `mise run audit` passed all three lockfiles with existing unmatched-advisory
  warnings.
- A precise removed-crate scan finds no references outside change records.
  The requested broad `subspace_issues|subspace-issues|services/issues` scan
  also matches the retained board's `subspace_issues_board` and
  `subspace-issues-board` names; those are intentional.

### Follow-up after slice 3 merged

Rebased both placement commits onto `e4a32c4` (PR #27), without merge commits.
The delivery-record conflict retains both the slice 3 evidence and this move's
record. Automatic merging also retained a duplicate `subspace-contracts`
manifest entry, which is reduced to one dependency. Slice 3's recovery tests
stay unchanged; its offline fixture changes coexist with the client-owned
credential import.

The existing `detail/markdown.rs` adapter now imports the pre-processor from
`crate::nett_markdown` and `NETT_DEV_API_URL` from the NETT client's `profile`.
Its shared-fixture test reads `spaces/issues/fixtures/nett_markdown.json`.
The adapter and pre-processor remain single implementations. Both Terminal
lockfiles already include the earlier contracts refresh from PR #27 and
contain no removed service dependency.

Quality remains 100.0% with no new findings after the rebase. Regenerating
the baseline changes only the function total from 14,886 to 14,937; all
scores, findings and policy values remain unchanged. The captain authorised
publication of this branch, with PR creation left to the captain.

The rebased `mise exec -- make check` passes in 123.05 seconds: 3,221 Rust
tests passed, five ignored, including the slice 3 adapter and recovery tests.
No timing-sensitive test failed. The original pre-rebase doctest artifact
failure and clean rerun remain recorded above. Publication runs the installed
prek pre-push check and quality hooks; the PR body records the final head's
interface and audit results as well.

### Follow-up after phase 4 and upstream fixture isolation

Rebased the two refactor commits onto `4e34dd3`, including phase 4's IRC
contract consumer (`9f4affc`), without conflicts or merge commits. Dropped
the duplicate test-only fixture-isolation commit in favour of withakay's
upstream fix, which also isolates global/system Git config and handles
case-insensitive reserved branch names on macOS. The prerelease test file,
IRC files and contracts code are unchanged from the new integration base.

The dropped regression explicitly asserted that a simulated hook
repository's complete config stayed byte-for-byte unchanged and its hook
index was absent after fixture commits and an integration plan. That includes
protection against `core.worktree` and identity leaks. The upstream fix has
no equivalent explicit regression assertion; this coverage gap is reported
to the captain rather than adding it to this refactor.

The removed-crate scan remains clean outside historical change records,
including phase 4's source and all three lockfiles. The PR body records the
new published head's full check, quality, interfaces and audit results.
Quality remains 100.0%; the phase 4 baseline refresh changes only the
function total from 14,937 to 15,061, with all other baseline fields unchanged.

## Slice 5: list menus, counts and design fidelity (15 November 2026)

The list now offers Assignee (Me, No assignee, Anyone), multi-select Priority,
and Filters (workflow statuses for one team, state types for All teams, type,
due date and Show archived). All choices use `ListQuery` and the existing hub
activation/reload path. No API reads or publish paths were added. The shared
anchored menus own search, keyboard picking, dismissal and focus restoration;
Priority and Filters stay open while their checks update. The palette opens
the same menus. Status choices respect the tab predicate and clear on tab,
team or account changes. Toolbar filters stay in memory and clear on profile
changes.

The summary and My issues tab use the hub's optional count results. The hidden
segment and Show sub-issues action appear only for a positive hidden count;
the My issues count is organisation-wide open assigned work. Missing/failed
counts are omitted. Scope changes clear old counts immediately, and hiding the
board drops them with its content.

Identifiers use the active code font. Tabs use `TabSpec::plain`, each group
count follows its name, and the header, controls and list content use the
approved 24 px side edge. The shared list gains two default-preserving board
options, `list_side_inset` and `group_count_after_name`; the Issues list opts in.
Activity keeps its existing layout defaults, covered by a regression that first
failed when Activity inherited the 24 px list inset.
The existing scroll owner and selection fills are unchanged. The toolbar can
wrap at narrow widths without losing access to its controls.

`ReadError::Response { status: Option<u16> }` retains safe HTTP response
metadata from the API or token authority, without retaining a URL, query or
response body. The board maps it into its existing network-error surface and
Copy details includes `HTTP 500`. Shared error handling still distinguishes
transport failures (Offline with previously read content), HTTP failures,
account failures and independent row retries.

### Test-first evidence

The first six new headless board tests failed on the original implementation:
missing menu controls, absent counts, trailing group counts and Copy details
reporting only `network` for a real loopback HTTP 500. They now exercise pointer
opening, typed search, digit and Enter selection, multiple priorities, filter
composition, the My issues assignee constraint, Escape/focus restoration,
hub-backed counts, Show sub-issues and the actual clipboard. Further coverage
checks omitted failed counts, stale count/status clearing, code-font role,
24 px heading geometry and compact text-only tabs. Existing server-error tests
now inspect the preserved status rather than expecting its erasure.

The integration regression checks Activity keeps its existing inset and count
placement. Repeated parallel runs also exposed an Activity pagination test
that began before both independent sections had loaded; its setup now waits
for both sections before exercising the existing retry assertions.

The design-fixture count test first failed with zero hidden sub-issues; the
local server now supplies the approved fictional totals (seven root issues,
twelve hidden sub-issues, three assigned open issues) on its existing count
routes. Its recorded-data filter/count behavior remains covered separately.

The issue-view half of slice 5, cross-board contracts and the service move
remain outside this delivery. No live NETT or WorkOS verification is included.

### Captures and remaining design differences

These five unedited Xvfb frames were inspected against
`evidence/design/list-1440-dark.png`, `compare-list-1440-light.png` and the
slice 2 review captures. The app uses Cupertino Dark, an isolated HOME, the
DejaVu Sans/Mono fonts and the local fixture server's `--rows design` mode.
Both API origins use numeric loopback; other boards and chat are disabled,
update checks are off, and proxy settings refuse other destinations. The
1000 × 720 window is captured at scale 1.6 (1600 × 1152 PNG); the wide window
is 1440 × 900 at scale 1. Every owned app, fixture server, Xvfb and ffmpeg
process was stopped. The harness and its isolated HOME live outside the
build cache, in `/tmp/obrien-capture/slice5/`.

| Capture | Caption |
| --- | --- |
| List, 1000 | Complete list at 1000 × 720: all three menus, both counts and selected TST-955; project lanes yield to the narrower window. |
| List, 1440 | Complete wide list: code identifiers, adjacent group counts, plain tabs, 24 px content edge and project lanes. |
| Assignee | Anyone, Me and No assignee in the anchored searchable menu. |
| Priority | All five NETT priorities plus Any priority, with checked state and keyboard numbers. |
| Filters | Workflow status, type and due-date groups in the scrollable Filters menu; search also reaches Show archived. |

The four requested fidelity changes are present. Remaining differences from
the mock are the Linux DejaVu text metrics; canonical rounded controls and
plain numeric counts rather than the mock's count pills; and existing shared
density/row lanes. In the wide frame the first group starts about 26 px higher
and issue titles about 30 px left of the mock. The new board options change
only the requested content edge and count position; shared density, row
indents, progress rendering and other boards retain their existing behavior.
The fixture-only shell shows Issues alone rather than the mock's other boards.
The current integration also supplies an Activity tab; its existing glyph and
behavior are retained alongside the four plain issue-list tabs.

### Gates

Before publication, rebased onto `95e30299011596007f8def45a4a98364bf9e2b2b`,
which includes slice 3's issue view, Activity, IRC issue references and the
issue-service ownership move.
Resolutions retain the detail module, its status line, Activity navigation,
palette actions and fixture failure routing. The existing detail and activity
HTTP-500 tests now expect safe status metadata. No issue-view behavior or
cross-board implementation was added by this slice. The gates below were
rerun on the integrated code, and list captures refreshed.

All requested foreground gates passed:

- Board and NETT client tests: five consecutive 64-thread runs, each with
  160 board and 216 client unit tests passing, plus 28 board and 25 client
  integration tests.
- `python3 subspace-desktop/scripts/test-nett-test-server.py`: fourteen passed.
- `mise exec -- make check`: architecture, guide/index/format checks,
  dependency checks, Clippy, script fixtures and 3,259 Rust test passes;
  five existing ignores.
- `mise run quality`: 100.0%. The separate findings scan shows only the 86
  existing chart findings, none in changed code. The normal baseline refresh
  changes only its function count, 15,061 → 15,139.
- `cargo check --release -p subspace-nett-client -p subspace-issues-board`:
  passed through mise, including exclusion of fixture seams.
- `mise run audit`: all three lockfiles passed; existing unmatched-ignore
  notices remain, with no dependency or advisory exception changes.
- `make interfaces`: real product headless interface checks passed.

## Slice 6 — Issue comments and activity on macOS

The issue detail timeline opens in **Comments**. The **All** tab adds the
activity events interleaved by time. Both views use the same scoped,
paginated issue activity read. Comments show Markdown, replies, author and
time, imported provenance, and deleted-comment tombstones. Press `m` in
the issue view to focus the timeline. The board-wide Activity tab keeps
its own feed.

These captures use a loopback fixture with fictional issues and comments.
the Activity filter change is the reference for the Comments default and the All switch.
Before this slice shows the same issue.

| Window | Comments | All |
| --- | --- | --- |
| 1,120 points | [Comments](evidence/slice-6-macos/comments-wide.png) | [All](evidence/slice-6-macos/all-wide.png) |
| 650 points | [Comments](evidence/slice-6-macos/comments-narrow.png) | [All](evidence/slice-6-macos/all-narrow.png) |

## Slice 5 list review follow-up (15 November 2026)

Rebased the published list branch from `186dd32` onto
`b60bd6315f5b74c872872f3fafd73f0526016b9f`, including the issue-detail
Comments/All timeline (#30) and retained Issues contract responder (#31).
The delivery-record conflict keeps the original list entry before the later
Comments/All entry. The only other conflict was the generated function count
in `rustqual-baseline.json`; code merged cleanly. The view still observes the
retained responder, with its existing timer outside the view. No removed
`Hub::publish` or free `hub::irc_*` functions were restored.

One fix commit addresses the three requested review points:

- Leaving My issues clears its toolbar assignee, so picking its checked Me
  row does not constrain Active or Backlog.
- An empty list under any Assignee, Priority or Filters choice says
  **No issues match your filters**, with **Try changing your filters.**
  Unfiltered search and tab wording remains as before. There is no new
  clear-all control.
- The My issues spoken count uses **1 open issue**, retaining the plural
  for zero and larger counts.

Each new headless regression failed before the fix. The focused suite then
passed all eleven tests. It covers both tab switches, every toolbar filter
kind with and without search text, unfiltered search/tab wording, and counts
of zero, one and two. The other review points remain deferred: loading team
status choices, unchanged-pick reads, spoken active-filter labels, palette
tests and filtered-count fixtures.

The five complete list/menu captures above remain accurate and are retained.
The new [filtered empty state](evidence/slice-5-list/empty-filtered-1440-dark.png)
shows the changed wording at 1440 × 900, Cupertino Dark, with Urgent selected
against the recorded loopback fixture. A capture-only adapter omits its
zero-count group headers to exercise the empty-groups response shape; counts
and repository fixtures remain unchanged.
It uses the same isolated HOME, DejaVu fonts and Xvfb harness as before; all
owned processes were stopped. The original design differences above remain.

Foreground validation:

- `mise exec -- make check`: passed, including 3,271 Rust tests, five existing
  ignores, architecture, guides, docs, formatting, cargo-shear, Clippy and
  script fixtures. The usual Linux PowerShell/macOS fixture skips apply.
- `mise run quality`: 100.0%. Explicit findings retain only the 86 existing
  chart findings, with none in changed code. Baseline regeneration changes
  only `total`: 15,139 → 15,206 from the previous published branch, or
  15,125 → 15,206 against the new base. No score, finding or policy field
  changes.
- `make interfaces`: real product headless interface checks passed.
- `mise run audit`: all three lockfiles passed, with existing unmatched-ignore
  notices and no dependency or advisory-exception changes.

Publication reruns the installed pre-push check and quality hooks against the
final committed tree, verifies every GitHub-signed commit's tree, and leaves
PR creation to the captain. No live NETT or WorkOS calls were made.

## Slice 7: images and the viewer

Part of the issue workspace implementation. The client now reads private file grants and image occurrence
pages. The hub has one bounded file reader per session, separate from its two
API readers. Pending file reads share deduplicated grant batches (at most 50);
unexpired grants are reused until 30 seconds before expiry. A failed byte read
renews once. Redirects, credential-bearing file requests, other origins,
oversized bodies and descriptor/content-type mismatches are refused.

The board owns a Markdown media plugin, viewport demand, background decoding,
the two-issue 128 MiB decoded-image LRU and the image viewer. Descriptions and
loaded comments use private `subspace-media:` links; file chips are inert.
The Images panel sits beside the description from a 1,000 px main column.
The viewer follows pages, reports the partial count, shows alt text, returns
focus on close and can reveal an image in loaded prose. An `around=` 404 opens
the image alone. The `media-stage` tone is declared in all bundled themes and
their example counterparts; the host tests its palette and contrast.

Generation checks and the board lifecycle clear image state on sign-out,
profile/organisation changes, expiry, refused reads and stop, and reject late
work. Encoded bytes are discarded after decoding. There are no image export,
clipboard-image or drag-out handlers, and capability URLs remain below the
board boundary. The privacy regression now exercises the actual grant and byte
paths as well as checking redacted diagnostics and safe selected text.

Implementation choices not fixed by the spec: thumbnails use a 208 px tile
and contain a 112 px preview; the auxiliary panel is 288 px wide. The stage is
opaque warm black in dark themes and warm grey in light themes, with One Dark's
own background retained. Tab skips disabled Previous/Next controls. A visible
Open image button provides the native keyboard action alongside image clicks.

API assumptions (offline fixtures only): grants return a `grants` array of
`publicFileId`, `url`, `expiresAtUtc`; image page entries contain a `media`
descriptor, `altText` and nullable `commentId` (null means description). Page
order is the service's order, with occurrences deduplicated by media ID. The
fixture models these in `file_grants`, `image_items` and `image_page` in
`scripts/nett_test_server.py`. No call reached NETT or WorkOS.

Tests were written ahead of the transport, Markdown, cache, decode-limit and
interaction fixes. Red runs exposed the missing APIs, the generic oversized
decode failure, Escape escaping the issue and pointer-open focus loss. The
resulting tests cover credential stripping, origin/type/byte limits, batching,
renewal, empty-page continuation, anchor 404, LRU eviction, inline click,
cross-page arrows, Tab, Escape, memory clearing and sanitized logs.

Verification is recorded in the captain's PR body at the published head.
The quality baseline changes only the function count, 15,206 → 15,341 against
the new integration base; all scores and warning
counts remain unchanged. Existing chart magic-number findings are outside this
slice and unchanged.

### Rebase and Linux capture follow-up (15 November 2026)

Rebased onto `6bb731d67130842c7517e94b99c2a0ac2499588b` (#32), without a
merge. Help retains the list menus and Images section, test registration
retains both modules, and this delivery entry follows the slice 5 review
follow-up. The baseline was regenerated. Combining both slices put the shared
error module over the module-size limit; its unchanged page-failure helpers
now live in `hub/worker/error/pages.rs`. No quality finding or score change is
accepted. The new `images` fixture scenario makes TST-1616 reachable directly
from the list; its regression failed before the route was added.

Built the real app with `cargo build -p subspace --features
subspace-shell/capture`. `SUBSPACE_CAPTURE_STEPS` drives Settings sign-in,
clicks and scrolling on an owned Xvfb display. `ffmpeg -f x11grab
-draw_mouse 0 -framerate 2` captures unedited PNG frames, avoiding Linux's
unsupported `render_to_image` path. Wide frames use 1440 × 900 at scale 1;
the 1000 × 720 window uses `GPUI_X11_SCALE_FACTOR=1.6`, giving 1600 × 1152
PNGs and a main column below the 1,000 px panel breakpoint.

The isolated HOME/XDG directories contain DejaVu 2.37 TTFs; `fc-match
'DejaVu Sans'` confirms DejaVu Sans before every launch. Cupertino Dark comes
from the unset theme. Issues alone is enabled, chat is off, and
`app.check-updates = "0"`. Both API overrides use numeric loopback on port
18207, the fictional device flow signs in through Settings with `BROWSER=true`,
and proxy settings refuse other destinations. The image bytes are synthetic.
Every kept frame was inspected for layout, real people, hosts and secrets;
only fictional fixture content appears. All owned app, Xvfb, ffmpeg and
fixture-server processes were stopped.

| Frame | What it shows |
| --- | --- |
| Issue, wide | Description and decoded inline image with the Images panel beside them at 1440 × 900. |
| Issue, narrow top | The 1000 × 720 issue has its inline image in the description and no panel beside it. |
| [Issue, narrow panel](evidence/slice-7-images/issue-narrow.png) | Scrolling the same narrow issue shows Images below the description, with ready, failed and oversized tiles. |
| [Viewer](evidence/slice-7-images/image-viewer.png) | Opaque stage, “1 of 3 on this issue”, alt text, navigation and Show in the description. |
| Failed tile | The persistent fixture failure still offers Retry after another attempted read. |
| [Too large](evidence/slice-7-images/too-large.png) | Oversized decode fallback with its filename and encoded size in the viewer. |
| [Pending position](evidence/slice-7-images/image-viewer-pending.png) | The decoded image is visible while `around=` is pending; its count is hidden and both navigation actions are disabled. |
| [Standalone viewer](evidence/slice-7-images/image-viewer-alone.png) | After `around=` 404, the same image shows “1 of 1 on this issue”, its alt text and disabled navigation. |
| [Partial window](evidence/slice-7-images/image-viewer-partial.png) | A successful `around=` replacement shows “2 of 3+ on this issue” without claiming a complete total. |

Foreground gates passed: `make check` (3,282 Rust tests, five existing
ignores, and 18 fixture-server tests), `mise run quality` (100.0%, unchanged
findings), `make interfaces` and `mise run audit` (all three lockfiles).
The first check run was interrupted by SIGTERM without a failed test; the
complete rerun passed. Existing Linux macOS/PowerShell fixture skips and
unmatched audit-ignore notices remain. Publication also runs the installed
pre-push checks against the final committed tree. No PR is opened by this worker.

### Review fixes (15 November 2026)

The Sonnet review's two P1s and the requested small P2s are addressed. Each
behavioural fix had a failing regression first. The image fixtures now serve
binary PNG responses through the real client, hub and decoder, rather than
skipping the path with an oversized descriptor.

| Finding | Change and regression |
| --- | --- |
| 1. Grant failure replaces the issue | Non-refusal grant errors fail only the tile, including renewal errors. `grants_server_error_preserves_issue`, `grants_network_error_preserves_issue` and `grants_not_found_preserves_issue` each retain Loaded/detail for both stages; `grants_forbidden_clears_private_scope` checks the 403 refusal path. Refusals use the Images target. |
| 2. Comment reveal overflows | Reveal uses the loaded comment count and saturating display limits. `revealed_comments_can_load_earlier_pages_with_a_bounded_goal` reproduces the old panic, then loads the next page. `sparse_comments_stop_automatic_paging_and_can_continue` retains the four-page automatic limit. |
| 3. Real image-path coverage | `inline_image_fetches_and_decodes_real_pixels` verifies Ready and the decoded BGRA pixels; `failed_image_retry_fetches_and_decodes_successfully` clicks Retry and reaches Ready; `around_404_opens_a_decoded_image_alone` exercises a real decoded standalone image. The unused `images-renew` scenario is removed, guarded by `test_unused_images_renew_scenario_is_not_offered`. |
| 4. Unknown position | `unknown_viewer_position_hides_count_and_disables_navigation` checks hidden count, disabled Previous/Next and Tab skipping them. |
| 5. Standalone state and around windows | `standalone_viewer_survives_later_image_page_reads` checks the latch; `late_anchor_404_does_not_make_a_new_viewer_standalone` binds the result to its own media ID when viewers change mid-read. `around_window_count_stays_partial_without_a_forward_cursor` checks the lower-bound count after replacement. |
| 6. Authored private schemes | Authors could reach the media plugin with these links. The pre-processor now flattens authored `subspace-media:` and `subspace-file:` links, leaving only its own generated links. `authored_private_media_and_file_schemes_cannot_reach_the_plugin` covers direct/reference links and mixed-case schemes. |
| 7. Unsupported images | The pre-processor uses the client's PNG/JPEG/GIF/WebP allowlist. `unsupported_image_types_are_file_chips` covers SVG and HEIC. |
| 8. Reload retries | The hub projects its issue revision; the board clears failed/loading entries and cancels old tasks when it changes. Completion guards check both scope and revision, retaining Ready entries. `reload_retries_failed_images_in_the_new_issue_revision` checks recovery without a manual Retry. |
| 9. Opaque custom-theme fallback | The fallback preserves the background colour with alpha forced to 1. `media_stage_fallback_is_opaque_with_a_translucent_custom_theme` checks a theme without a media-stage token. |

Section 6 does not specify whether a successful `around=` response replaces
earlier pages. This implementation retains replacement and the returned forward
cursor: there is no absolute position or total with which to merge arbitrary
windows. A replacement remains explicitly partial (`m+`) even after its forward
cursor ends. Positions refer to the loaded window. The viewer suppresses the
count until its image belongs to that window, or is confirmed standalone by its
own `around=` 404. The choice is documented in `docs/issues.md` and the PR body.

The full check exposed an older privacy assertion that rejected the permitted
`/files/grants` path from parallel image tests. Its regression now fetches and
decodes an image itself, asserts that the grant route is logged, and rejects
grant query strings and the actual fixture token, alongside the existing private
content, search, cursor and credential checks. The test is
`the_log_holds_request_paths_but_no_issue_content_queries_or_cursors`.

The six original frames were re-inspected; the normal viewer was recaptured and
the three viewer states above added with the same owned Xvfb/ffmpeg fixture
method. All kept frames contain fictional data only. All owned processes were
stopped. The `images-gone` fixture now makes the missing inline anchor reachable
and delays its 404, covered by
`test_missing_anchor_scenario_exposes_the_inline_image_outside_the_page`.

Quality regeneration changes only `total`: 15,307 → 15,341 from the previous
published slice, or 15,206 → 15,341 against the integration base. Scores,
warnings and policy fields remain unchanged; the existing 86 chart findings
remain outside this change. Final gate results and the signed head are recorded
in the PR body.

Review **P2-8** (blocking fetch on the shared background pool) and **P2-9** (LRU
thrash) remain in **the image loading and cache-pressure change**. Those larger changes are not included here.

### Stale grant follow-up (15 November 2026)

Part of the issue workspace implementation. A stale image grant is now a separate cancellation result
(`Ok(None)`), rather than a fabricated `SessionError::SignInRequired`. Both
stale checks in `grant` return that result; its only caller fails the tile
without entering the refusal mapper. The other `current` checks already fail
locally. Genuine session/read errors still use the existing refusal handling,
including sign-in loss, forbidden access, a second 401 and revoked refresh.

`stale_image_renewal_preserves_the_session_and_issue_error` first failed with
scope generation 2 instead of 1: a held byte fetch resumed after a children
read put the issue into `Error(EdgeAccessNeeded)`, with generation and issue
revision unchanged. It now passes, retaining the session, organisation and
the existing issue error, failing only the tile and issuing no renewal grant.
`grants_unauthorized_after_refresh_expires_the_session` covers two 401s at
both initial-grant and renewal stages, one refresh, and the existing
`SessionExpired` state with private scope cleared. This preservation test
already passed before the fix; there was no 401 classification defect to
reproduce. The existing 503, dropped-transport, 404 and 403 tests still pass.

No frame's steady state changes, so the nine inspected screenshots remain
valid without recapture. Quality passes at 100.0% with no new findings; the
baseline is not regenerated. Final gate results and the signed head are
recorded in the PR body.
