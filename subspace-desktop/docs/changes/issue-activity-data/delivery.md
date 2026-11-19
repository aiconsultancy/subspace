# Delivery

The branch starts at the issue workspace implementation slice 4 (`e2f1013`), confirmed as an ancestor
after fetching and fast-forwarding the task branch.

Verification on 15 November 2026:

- `mise trust` and `mise install`: passed as separate commands.
- `mise exec -- make check`: passed, including workspace tests, clippy with
  warnings denied, formatting, architecture, documentation and script fixtures.
- `mise run quality`: passed at 100%, no regression and no new findings in
  changed source. Existing chart magic-number findings remain outside this slice.
- The client suite now has 201 passing tests, including 20 activity tests;
  the offline fixture server has nine passing contract tests.
- Repository and workspace pre-commit checks: passed, including gitleaks.

Tests cover omitted/null activity, imported timestamps, unknown kinds/actors,
all 11 named-list kinds and optional batch count, request offsets and cursors,
independent list retention, demand-only freshness and forced reloads, priorities,
per-section Retry, team/org changes, sign-out/Stop, and late success/refusal
rejection. Keyset tests move a page-two issue into the refreshed head and assert
its newest activity appears once while the tail cursor remains usable.

CodeRabbit CLI authenticated and completed its branch review with zero findings.
The subsequent read-only Codex review found no actionable issues after checking
query/scope guards, late errors, independent caches, freshness, head merging,
tail cursors, DTOs, batch counts, fixtures, tests and docs. It also passed
`git diff --cached --check`; it did not rerun the full gates or modify files.
The captain owns the final Claude review; this worker did not run one.

## Captain review fixes

The captain's final Claude review of `56c41074d3` found one P1, two P2s and
four P3 areas. The captain accepted all of them; this follow-up fixes them:

- P1: `activitySince` now uses whole seconds with an explicit offset, and the
  server fixture enforces NETT's instant pattern. A nanosecond-clock request
  test and accepted/rejected precision fixtures cover this.
- P2a: a disjoint partial head replaces rows and adopts its next cursor,
  including after a previously exhausted first page. Both cases are tested.
- P2b: complete heads replace the whole result; overlapping partial heads
  remove missing rows in their keyset range while preserving older rows and
  the tail cursor. Tests cover all three behaviours and empty complete heads.
- P3: the head copy is authoritative even for a lower activity time; the
  merge test verifies the row copy and keyset re-sort, including ID ties.
- P3: head reads defer until an in-flight Load more completes. A blocked
  continuation test confirms its row and cursor survive the subsequent head.
- P3: late-result tests join the production read body before asserting the
  newer snapshot's rows, version and section revisions. Both tests failed
  when the activity guard was temporarily disabled, then passed with it restored.
- P3: the integration docs require activation again after org/team changes.

CodeRabbit and Codex reviews are not rerun for these fixes, as instructed.
The captain will rerun only the final Claude check on the updated PR.

Follow-up verification: `mise exec -- make check` passed with 207 client tests
(26 activity tests) and ten fixture-server tests. `mise run quality` passed at
100% with no regression or new findings in changed code. Root and workspace
pre-commit checks, including gitleaks, passed. The temporary guard mutation is
restored; both strengthened late-result tests pass with the production guard.

## Voyager timestamp refinement

Fixtures now keep `lastActivityAt` at or after creation and equal to the
entry's original `occurredAt` whenever last activity is present. The null row
represents an odd import whose entries all predate issue creation. The fixture
contract test checks that creation floor, equality and a non-decreasing activity
advance. The existing Rust mismatch test deliberately uses non-conforming input
to preserve tolerant decoding. Age still uses `lastActivityAt`, and the
head-authoritative replacement and its lower-time regression test are unchanged.

Verification for this refinement: `mise exec -- make check` and `mise run quality`
both passed; quality remains 100% with no regression. The fixture test first
failed against the old timestamps and passes after the update (ten server tests).

## Captain recheck of bf64a58

The captain's Claude recheck confirmed seven earlier fixes and accepted one
remaining P2 and two P3 findings. This round fixes all three:

- Partial heads retain a tail only with an unchanged ID/timestamp anchor.
  Regression tests cover a moved shared issue plus 49 new rows with and without
  an old cursor, and a smaller head whose only shared row moved. Both adopt the
  new head cursor instead of hiding overflow.
- Deferred heads are skipped by freshness checks. A scheduling test verifies
  activation preserves Trackers' revision and queues no new read, both when
  Trackers is fresh and when its first page is still loading.
- An unwind guard clears the current section's loading and deferred-head state
  and records a retryable error. A blocked credential-store panic during Load
  more verifies both flags clear and a subsequent Reload completes.

All four new tests failed against the previous implementation and passed after
the fixes. The activity suite now contains 30 passing tests. No CodeRabbit,
Codex or Claude review was rerun by this worker; the captain owns the final check.

Verification for this round: `mise exec -- make check` passed with 211 client
tests and ten fixture-server tests. `mise run quality` passed at 100%, with no
new findings or baseline change. Root/workspace pre-commit checks, including
formatting and gitleaks, passed.

No screenshots are required: this slice has no visible changes. GUI design and
implementation follow the Issues board delivery. Live NETT dev verification
remains for that pane delivery after the Activity read rollout deploys. No CI exists in this repo.
