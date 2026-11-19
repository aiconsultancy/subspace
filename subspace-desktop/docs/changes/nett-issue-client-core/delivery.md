# Delivery: NETT issue client core

## UI crew entry points

Add the Issues board and product registration when design is approved. Add
`subspace-nett-client` to workspace dependencies then: an unused declaration
is deliberately absent so `cargo shear` passes.

- `subspace_nett_client::session::WorkOsSession::nett_dev()` creates the native
  keychain-backed canonical session; `new(profile, store)` accepts loaded
  metadata or an injected test store.
- `start_sign_in()` gives the user code and verification URLs. Display these
  values while retaining the device code in the worker; run
  `complete_sign_in(authorization, cancelled)` there.
- `state()`, `client()`, `select_organization(id)` and `sign_out()` own session
  lifecycle. `client()` restores/refreshes as needed; organisation selection
  checks the current accessible list. Save selected metadata separately.
  Transient refresh failures keep unexpired access usable. Treat the distinct
  `SessionError::KeychainSaveFailed` / `SignInState::KeychainSaveFailed` as a
  request to sign in again, rather than retrying a stale rotated refresh token.
- `Client::{organizations, teams, issues, issue, comments, replies, relations}`
  returns the DTOs in `model`. Start with `PageRequest::default()` and
  `IssueQuery::new(team)`; their public non-secret fields configure queries.
  Continue with the returned `nextCursor`. For a detail's children, start with
  `IssueQuery::new("")`, set `parent` to the detail's `id`, `order` to `manual`,
  and `page.limit` to 20; copy `children.nextCursor` into `page.cursor`.
  Keep those filters and order unchanged on subsequent pages.
- `subspace_issues::profile::ProfileSettings::{open, apply}` participates in
  the existing Settings transaction. Its `nett_dev` and optional `standalone`
  fields are non-secret records, revalidated before any network/session use.
- `subspace_issues::reference::ReferenceRecognizer::{new, find}` takes loaded
  and fallback keys. `IssueReference::{identifier, range}` gives the link text
  and byte range. `IssueResolver::resolve(org, reference)` reads its detail.

All client/auth/keychain calls block: run them in `cx.background_spawn`, never
render. Use one worker/session per profile. Bind results to profile,
organisation, issue and load revision; drop clients/content and reject old
completions on scope changes. No board or UI design is created here.

The stored-token comparison before deletion is not an atomic cross-process
operation. A lock covering load, refresh, rotation save and conditional deletion
is required before a second process, including future headless Issues tools,
uses the same profile. The current slice supports one serialised worker per
profile in one process.

## Deferred reads and checks

- `GET /api/v1/auth/context` is not wrapped yet. The initial team-scoped reads
  use `auth/orgs` and explicit organisation/team selection, so they do not
  need the signed-in user's identity. Slice 2's **My issues** filter and
  signed-in member presentation need a typed context read and the broader
  assignee filter before they can be delivered. Do not infer that identity
  from the access token's expiry payload.
- Markdown links such as `[TST-1](https://…)` and URL-shaped tokens hiding
  references remain for the IRC references slice. That slice must also turn
  NETT and Linear issue links into references, rather than relying on this
  plain-text recogniser to parse links.
- Cancellation interrupts waits but waits for any in-flight WorkOS request
  to finish, up to its 30-second timeout. This is accepted for Slice 1.
- Native keychain integration and `WorkOsSession::nett_dev()` product/channel
  namespace and profile-entry naming checks are macOS checks for the UI slice.
  Linux CI uses injected stores and cannot prove those native behaviours.

## Headless proof

The product CLI derives commands from registered board capabilities through
`subspace-board-command` (`github scope`, `github item`, etc.). There is no
service-command hook for an unregistered Issues board. This delivery therefore
adds no product `issues` command or second router. The UI slice can expose
readonly Issues tools through that catalogue and extend `make interfaces`.

The core proof runs offline:

```sh
mise exec -- cargo test -p subspace-nett-client -p subspace-issues
```

Fixtures prove routes, headers, JSON, device/refresh forms, rotation and profile
isolation without live credentials or a native keychain. They do not prove dev
edge admission. An initial organisation-selection test accidentally used the
canonical API origin with a synthetic token and received an Access challenge.
Its session fixture was corrected, and every test HTTP send now asserts a
loopback endpoint before transport. No WorkOS live request or real credential
was involved. macOS/Windows native stores were not exercised on the Linux worker.

## Review fixes

Device polling follows indractl's three-attempt retry policy for network
failures, HTTP 408/429 and 5xx. It honours positive `Retry-After` seconds or
HTTP dates; otherwise it waits 200/400 ms. Request timeouts and retry waits
remain bounded by the device deadline, including oversized delay headers.

Refresh keeps existing access until successful rotation, terminal revocation
or actual expiry. Transient refresh failure can still return an unexpired
client. Rotation saves get three attempts before returning the distinct
keychain error above. A failed save best-effort deletes only that profile's
stale refresh entry and latches the error in memory so later client calls do
not reuse it and misreport revocation. A locked keychain may also refuse that
deletion; the latch lasts for this session, not across process restarts.

Reference recognition rejects hyphenated prefixes and date continuations,
discards invalid keys from both sources before choosing the fallback, and
keeps `UTF-8` / `ISO-8601` plain even if their prefixes are loaded teams.
Unmatched backtick runs no longer hide the remainder of a message.

The final review also closes these eight cases:

1. Optional `parent` and `order` allow child-page continuation without a team;
   a team or parent is still required.
2. Expiry uses receipt time plus `exp - iat` when both claims exist. Without
   `iat`, `exp` remains a scheduling hint even if it is behind the local clock.
   Successful token responses save rotation before access validation, including
   missing or malformed access, so a bad scheduling hint cannot lose sign-in.
3. Explicit `SignedOut` never restores credentials. Delete gets three attempts
   and is retried on subsequent client calls if needed; those calls still return
   `SignInRequired` until explicit device sign-in succeeds.
4. Revocation deletes only a stored token that still equals the rejected one;
   a newer stored rotation is preserved. Cross-process use requires the lock above.
5. Saved `nettDev` holds only selected organisation and team keys. Load supplies
   current built-ins and tolerates legacy copies and unknown profile fields;
   applying settings rewrites the canonical preference shape. Unknown inner
   fields are ignored, not re-emitted; unrelated board-file keys are preserved.
   Runtime validation still refuses a different WorkOS address or client.
6. Comments/replies accept limits 1–50; teams/issues/relations accept 1–100.
   Search is trimmed, blank search omitted and lengths above 200 UTF-16 units
   rejected locally, matching NETT's .NET string-length check.
7. An unavailable or missing refresh entry keeps unexpired access usable and
   state `RefreshRequired`. Missing credentials become `SignedOut` at expiry.
8. Backtick matching skips an unmatched run and keeps scanning, so a later
   closed span remains protected.

These changes started with failing regression tests. All authentication and
API fixture sends still pass through the loopback guard.

## Verification

- Core tests: **61 passed** (44 client/auth/session, 5 profile, 9 reference,
  2 preference adapters, 1 native-store error mapping). The shared fixture
  package and doc tests pass.
- Targeted Clippy for both new crates and shared test support passes with
  `-D warnings`; `cargo shear` passes for the workspace.
- `mise run quality` passes at **100%**, with no findings in changed source.
  The baseline was refreshed after fixing findings, retaining the same score
  and zero blocking findings. Existing chart magic-number notes remain.
- `mise run audit` passes all three lockfiles. The initial core added `keyring`
  4.2 (locked to 4.2.0); the review fixes reuse the workspace's existing
  `chrono` dependency for HTTP-date parsing. No new crate version or
  `deny.toml` exception was added in the follow-up. Existing unmatched-advisory
  warnings in excluded Terminal workspaces are unrelated.
- The full `mise exec -- make check` passes on Linux after the separate
  platform repair below: architecture (35 packages), board guides, formatting,
  Cargo Shear, strict workspace Clippy, Rust tests, script fixtures, web bridge
  (5 tests) and Weather board (106 tests). Rust reports **2,666 passed,
  0 failed and 5 ignored**, including subprocess fixture output and doc tests.

Script fixtures explicitly skip PowerShell without `pwsh` and native macOS
bundle checks on Linux. No product `make interfaces` run is claimed: there is
no new Issues command.

## Linux build repair

The captain authorised a separate commit to repair pre-existing platform
assumptions uncovered by the full gate. No layout or native macOS behaviour
changes.

- `framework/host/app/src/app/titlebar.rs` compiles traffic-light positioning
  only on macOS; the other platforms' path does nothing. The height-change
  helper remains available to tests on every platform.
- `framework/host/app/src/app/toolbar/tests/titlebar.rs` checks traffic-light
  clearance and position only on macOS. Logo and help alignment remain tested
  everywhere.
- `spaces/web/src/app.rs` and `spaces/web/src/policy.rs` allow unused native
  entry points only on unsupported platforms outside tests. File confinement
  and navigation policy tests continue to run on Linux.
- `spaces/web/src/page.rs` allows unused native page events and profile fields
  only where there is no native web view, following its existing platform
  attributes.
- `spaces/board-lab/src/project_browser.rs` exports the example test helper
  only for the macOS ARM64 tests that consume it.

The full check and quality gate both pass after this repair. Quality stays at
100%, with no findings in these files and no baseline change.
