# Comment authoring delivery

[Intent](intent.md) · [Spec](spec.md) · [Design](design.md) · [Tasks](tasks.md)

## PR B — approved Remove identity amendment

The tracker-lead captain approved the §7 amendment on 15 November 2026:
unacknowledged Remove retains only a memory-only declaration-hash/request-key
memo, with any known media/upload IDs and no bytes, row or cleanup controls.
Every acknowledged Cancel 200 clears that identity, whatever its retention.
Deliberate same-declaration re-intake replays the memo key. A live result
resumes it; a definitively closed/expired result permits a fresh key; an
uncertain replay keeps the same-key Failed row. Different declarations never
reuse it. Memos clear on post, Discard and scope clearing. Late completions
never recreate rows. This decision resolves the reported Remove/new-key gap.

Freshly fetched Indra `fdc1b63fdffdeabd34f9f3cf49c43cdd2e08dba9`,
`backend/src/Infrastructure/Services/IssueTracking/IssueMediaService.cs:80–117`,
finds a reservation by actor/request key, compares the declaration hash only
within that reservation, and allocates new media/upload IDs for a new key.
It does not deduplicate a fresh key by image hash. Cancellation may be
unavailable or conflict; a lost acknowledgement also cannot establish closure.
The approved memo rule preserves the original identity until that uncertainty
is resolved, without retaining an orphaned image buffer or cleanup UI.

PR B is based on `e259f4765514fea7d620a461c05e176dfb686b42`. Implementation
continued from the retained uncommitted draft. The spec amendment is committed
separately before implementation, as requested. Delivery evidence follows.

### Implementation and TDD

Native clipboard paste and Attach image… feed bounded, off-thread validation
of original JPEG/PNG/WebP bytes. Clipboard raster conversion stays in memory.
Animated containers, unsupported files, oversized bytes, edges, pixel counts
and decoder allocations are rejected before Prepare. Rows have independent
progress, Retry/Check again and Remove controls between editor and submit.
Only Ready semantic `nett-media:` blocks join the frozen comment body and its
draft UUID, including image-only comments and replies. The response's media
descriptor is available to the existing private reader immediately, even when
the following discussion refresh fails.

The hub owns immutable attempts, generation fences, memo synchronization,
eight-image/32 MiB draft budgets and a 64 MiB global encoded-buffer budget.
Weak buffer accounting includes active transfers. Removed work drops both its
worker and row buffers as soon as transfer stops, before byte-free Cancel calls.
Retry recovers the original status and Complete before retransferring; one
same-key descriptor renewal handles a 404. Checking stops after 30 polls at
two-second intervals while the issue is open. Check again resumes the original.
Cancellation rereads version and retries one conflict. Remove drops the row
immediately; late workers settle only its byte-free memo. Retry-After controls
wake through weak UI timers. No capability URL, request key, hash or bytes are
logged, copied, persisted or displayed.

Observed red → green results include missing fixture/client/intake APIs;
two initial Prepare calls instead of one; expired Ready acceptance; missing
memo storage; hidden image-only reply recovery after Escape; an absent finite
Retry-After deadline; the fixture's incorrect expired-transfer error; and
attachment controls preceding the editor. Each failed assertion or compilation
was observed before the corresponding implementation, then rerun successfully.
The headless native-paste test asserts editor → attachments → submit bounds.

Independent fixture tests cover Prepare replay/conflict, expired/rejected
intents, consumed drafts, lost acknowledgements, checking and cancellation
races. HTTP fixture tests assert exactly one reservation, transfer, reference
and comment after dropped PUT/Complete recovery. Rust transport tests cover
exact fields, SHA-256, pinned descriptor metadata/origin/path/grant, redirects,
credential isolation and redacted errors. Hub fixtures cover all six approved
Remove/memo scenarios, finite checking, budgets and retry deadlines. Headless
tests exercise native paste and picker, retained navigation, held image-only
replies and returned-image projection.

### Evidence and integration

Rebased onto freshly fetched
`e259f4765514fea7d620a461c05e176dfb686b42`, preserving the original four commits in
order: spec amendment, implementation, delivery documentation and intake fix.
The review fixes follow in a new fix commit. The incoming integration merges
since `add3fb5` are:

- `253720b`: feat(irc-client): add reply tags and thread roots (the IRC threading work) (#43).
- `b1be430`: Fix IRC issue links and exact Issues search (#44).
- `7a9288c`: fix(irc): compile selectable issue links in normal builds (#45).
- `8750fba`: feat(theme): macOS button tokens and a fuller, live theme designer (#46).
- `e259f47`: feat(irc): show loaded reply counts on root messages (the IRC threading work) (#47).

The final fetch found #47 after the theme-base gates. Its only conflict was the
baseline `total` field; it is resolved to the measured final count. No timeline
rendering file was touched. The four new theme frames remain applicable because
#47 changes IRC replies only.

The base also includes issue-property editing, IRC issue-preview navigation,
the screenshot policy, IRC selection, jt's timeline layout and reference access
guards. **No timeline rendering file is changed.** The only
reader adjustment is projected-media lookup in `spaces/issues/src/media/view.rs`.
The [four PR B frames](evidence/README.md#pr-b) show uploading, a failed image
beside a kept Ready image, a posted returned image and a real 960 px composition.
Every unedited native frame was inspected. The recaptured posted fixture
recorded two reservations, transfers and references in one image-only comment,
one of each media operation per intended image. The PR A empty-composer frame is
the historical before evidence; the new frames include the merged timeline.
All four PR B frames are recaptured for the new Cupertino Dark button tokens.
Attach image…, Retry and Remove keep the shared Action role: these are labelled
actions inside board content, rather than the Compact role's inline row icons.

Raw rustqual inspection returns only the 86 existing chart findings, with none
in changed code. This review updates only the baseline's `total` field,
15,949 → 16,075, including incoming theme/IRC functions and the review changes.
The theme-base intermediate count was 16,046, then #47 added 29 functions.
The cumulative PR B count is 15,530 → 16,075; score,
warnings and all other fields are unchanged. Shared composer availability state
and clipboard ingestion were separated to remove the new quality findings
before regenerating the baseline.

The four foreground gates passed: `make check`, `mise run quality`,
`make interfaces` and `mise run audit`. The full pre-push hooks and the same
four gates are repeated at the signed published head, with results in the
prepared PR body. An initial full-suite native-paste test timed out because
Paste followed a focus change before a rendered focus path; the test now uses
the same rendered-focus helper as keyboard submission. The full check passed
after that fix. A later full hook run exposed another native-test race: hub
readiness preceded the rendered submit policy. The paste test now waits for
the actual submit control to become enabled before issuing its shortcut.

Final review also found that submit could overtake off-thread preflight before
any row existed. The regression failed with `Posting` instead of `Editing`,
then passed with a byte-free hub intake lease. The lease pins the draft UUID
before file reads/decoding, holds submission through all validation, survives
navigation and atomically refuses admission after Discard or scope clearing.
An old lease cannot release a replacement draft's hold. The separate hub test
first failed on the missing API; all 11 attachment hub tests and 216 board tests
passed after implementation. The same four gates and complete hooks are rerun
for this follow-up before and after its signed publication.

### Captain review follow-up

The fixture regression first observed two reservations after Remove A,
re-intake B while A's Cancel is held, Remove B, then another deliberate intake.
It now observes exactly one reservation and only the original request key.
The row retains its replay memo until identity installation, and removal hands
that same memo back to the draft. A removed waiter never owns or settles the
original worker's cleanup. An attempt records Prepare dispatch: an unsent key
is never Prepared merely to cancel it, while a lost Prepare response still
recovers the same key before Cancel. A further regression first found that
reusing an unsent memo forgot this fact; the memo now carries its dispatch bit.

The memo-lock probe first failed because `current()` ran while the memo mutex
was held; it now passes with the current-scope check outside that guard.
Cleanup also clones its memo and drops the row guard before settling it or
taking the hub lock. The expired Ready replay first failed with Closed; it now
confirms expiry and prepares a fresh key. A blocked-Cancel regression first
found live removed buffers; it now proves both buffers and their global-budget
retention are released before Cancel finishes.

Coverage now goes through `Hub::discard_comment`, real successful comment
submission, and `Hub::select_organization` for scope clearing. Every boundary
then re-attaches the same declaration and asserts two distinct reservations,
two Prepare calls and no replay. A different-byte intake with the same filename
also asserts distinct size, hash and request key. All 283 NETT client tests
pass after these changes; the coverage extensions preserve already-correct
boundary and different-declaration behaviour.

PUT's per-request global timeout is 30 seconds plus one second per started
32 KiB, or 286 seconds at 8 MiB. The arithmetic test first failed on its missing
helper, then passed at zero, one byte, both sides of a chunk boundary and 8 MiB.
The private transfer agent bypasses Framework request logging. The native app
installs no Rust `log` facade subscriber: `RUST_LOG` cannot activate ureq Trace,
and `SUBSPACE_LOG` changes only the separate redacting Framework sink. A future
facade bridge must cap ureq below Trace before it can be installed. The
transport comment records this actual guarantee rather than implying that a
separate agent disables ureq's own logging.

The finite checking contract now explicitly excludes Retry/Check again's one
initial recovery read from its 30 polling reads. The fixture asserts 30 reads
on the first run and 61 after a second still-checking run. Mixed clipboard
text and image paste is intentional under “Keep normal text paste”; the spec
now spells that out. Complete remains the missing-PUT probe as approved.

The initial parallel check/interface attempts were interrupted by SIGTERM and
are not counted as passes. Verification is rerun in the foreground, and the
published-head results are recorded in the PR body. Captures use an owned
Xvfb, loopback fixture hub, in-memory clipboard and current capture build;
the isolated font set is Noto Sans and Liberation Mono. The startup update
preference is explicitly `0`; an earlier `false` value triggered local token
resolution, which refused before any GitHub update request.

Linux skips the existing PowerShell installer and macOS bundle
fixtures; existing cargo-shear and advisory-ignore notices are unchanged.
One full hook run lost its backing `target` directory and log during execution;
it is not counted as a pass. Build output was recreated and the gates rerun,
with verification logs retained outside the managed target directory.

No live NETT, WorkOS or file service was contacted. Source
confirms the media contract and the approved memo amendment; actual service
availability, scanning latency, quota outcomes, WorkOS sign-in and macOS native
clipboard/picker acceptance remain the captain/jt live acceptance step.

## Phase 1

The contract is now rebased onto Subspace delivery base
`f41c4ad69773e8331a0c87fb2d2fe30001cb3a1b`. The comment authoring work was read in Linear;
Indra API, database guards and NETT composer were inspected at
`d849f70060aa68fad1292f53ecc12dd6dc203fad` after the authorised read-only
fetch. The spec pins the inspected source paths and lists proposed decisions
and unverified runtime behaviour.

No production implementation, live service acceptance or UI screenshots belong
to this phase. The tracker-lead captain must approve the contract before the
worker continues to PR A. Verification and the signed publication are reported
with the Phase 1 handoff; implementation gate and evidence results will be
added here during A and B.

Phase 1 validation on the documentation tree:

- The complete pre-push hooks passed, including `mise run check` (the same
  task as `make check`), formatting, quality and secret/file hygiene checks.
- An explicit `mise run quality` passed at 100.0%, with 15,345 functions and
  no baseline change. No Rust source or dependencies changed.
- `mise run docs-index` passed; all 40 pinned Indra links were additionally
  checked against the fetched Git objects, and local change-record links were
  checked separately because the index excludes change records.
- The captain checkout had no `core.worktree` or local `user.name`/`user.email`
  before publication; it must be checked again after signed publication.

`make interfaces`, `mise run audit`, implementation TDD, screenshots and live
acceptance remain Phase 2 work. No new test is needed for a prose-only record.

## Approved continuation

The captain approved PR A on 15 November with the draft fence as the retry
mechanism, no identity read or matching scan, warned Discard, and best-effort
image cancellation in PR B. The create endpoint and `SendFailureAsync` were
checked at Indra `069b486f5eb49faed173c067d6ce5e896fb4baa1`: Conflict maps
to 409 ProblemDetails with `detail: "IssueMedia.DraftAlreadyUsed"`. PR B waits
for A to merge. The amended spec, intent, design and tasks record that order.

Use [the frame matrix](spec.md#11-delivery-and-evidence) and append actual
published-head gate results, TDD observations, baseline movement and inspected
screenshots for each PR. Local fixtures do not replace the final captain + jt
NETT dev check, which needs human WorkOS sign-in and explicit issue scope.

## PR A — text comments and replies

The native composer sits beneath the timeline and opens a one-level reply
editor below a root's replies. It uses the shared Markdown composer with
retained Escape, frozen/busy input and IME-safe submit. The hub owns scoped
memory-only drafts, single-flight submission, permissions from `auth/orgs`,
quit warnings and generation fencing. No draft is serialized or indexed.
Unknown outcomes use only the approved random draft UUID fence. Retry sends
the original body/parent/UUID; exact 409 ProblemDetails confirms an earlier
save. Other retry refusals stay frozen, with warned Discard and read-only
Refresh discussion. There is no identity read or matching heuristic.

A valid 201 projects the returned comment immediately. The discussion head
then refreshes, retaining loaded history and the selected Comments/All view.
Confirmed content survives a failed refresh. Replies restore thread focus;
root posts restore editor focus. Late scope completions and editor events
are ignored. No Properties, Record or workflow-indicator implementation was
changed. The delivery rebases onto integration
`f41c4ad69773e8331a0c87fb2d2fe30001cb3a1b`, including the image loading and cache-pressure change's bounded
image loading and stable preview cache (#36). Slice 5 had not merged.
Composer and timeline rendering are unchanged by this rebase, so all eight
frames are retained. The historical before frame remains pinned to its
requested `52d2768` binary; this is evidence provenance, not the delivery base.

### TDD and source boundary

- The new loopback write tests first failed with missing POST routes (404).
  The fixture now atomically consumes draft UUIDs and models exact 409
  ProblemDetails, including `type`, `title`, `status`, `detail`, `instance`
  and `traceId`, without a `code` field. Its tests assert one saved comment
  after commit/drop + retry/409, refusal-before-save + retry/201, concurrent
  same-UUID requests, and a different retry refusal. Strict fields, root/reply
  rules and read availability are covered too.
- Typed client tests first failed before the write API existed. They verify
  exact fields, UTF-16 limits, response scope, precise fence detail, redirects,
  unauthorized responses and a single POST attempt. Retry-After's seconds
  and HTTP-date test first failed before the parser was added.
- Draft tests cover retained navigation, scope clearing, single flight,
  frozen edits, same-intent retry, a new UUID after discard, rate limiting,
  standalone/permission refusal and late responses after sign-out.
- Headless board tests cover multiline drafts, issue switching, Escape,
  reply opening, All retention, immediate projection despite refresh failure,
  uncertain Retry followed by 403, read-only Refresh and warned Discard.
  The projection test first failed because the returned comment had no
  prepared Markdown. The IME test first failed because submit emitted while
  text was marked; the shared composer now blocks that submission.
  A focus regression test also failed with the focus guard removed: a root
  failure stole the newly opened reply's input. The guard keeps that reply
  focused while the root outcome is retained.

The draft fence, empty text-only media binding and failure response are
source-confirmed at the Indra revisions pinned in the spec. No new NETT
behaviour is assumed. Delays, dropped acknowledgements and post-save read
refusals in `scripts/nett_comment_fixture.py` / `nett_test_server.py` are
synthetic failure injection, not observations of a deployed NETT instance.
Actual dev acceptance remains the captain and jt's later WorkOS step.

### Captures

All nine [evidence frames](evidence/README.md) were examined at native size.
The eight PR A frames use the real capture-feature app, fictional fixture
sign-in and comments, Cupertino Dark, an owned Xvfb display and unedited
`ffmpeg -f x11grab` output. The before frame uses the retained `52d2768`
binary, driven with XTest on the same owned display because that binary has
no capture feature. The narrow frame resizes the actual window to 960 px.
No image is wider than 1440 px. HOME/XDG directories and DejaVu fonts are
isolated in the slot's ignored build area; updates/chat are off and proxy
settings refuse non-loopback destinations. Each runner stopped its app,
display and fixture server. No live service or real account was used.

### Gates and publication

The four required foreground gates and the complete pre-push hooks are run
for delivery; their results and the signed published head are included in
the captain's PR body and final handoff. The quality baseline may change
only its function count. Signed publication must preserve the reviewed tree,
leave the checkout clean and verify no captain Git-config leak. No PR is
opened by this worker. PR B starts only after A merges.

`make check` passed, including shared composer tests, Clippy, architecture
and documentation checks. The focused runs passed all 201 Issues board unit
tests, 251 NETT client unit tests, 294 shared dashboard unit tests and their
integration suites, plus all 26
loopback-server tests. `make interfaces` and `mise run audit` passed.
The explicit quality run and raw finding inspection
passed with no new findings in changed code. `rustqual-baseline.json` changes
only `total`, from 15,341 to 15,529 (including the integration branch's new
functions). The image loading and cache-pressure change rebase moves the previous PR A count from 15,474 to
15,494; scores and every warning field are unchanged. The four gates
are repeated on the signed published head, with exact results in the PR body.
The pre-existing cargo-shear warning for `spaces/issues/src/tests/lanes.rs`
remains; PowerShell and macOS-only script fixtures skip on this Linux host.

### Rebase onto the image loading change

Resolved the sole content conflict in `scripts/nett_test_server.py` by retaining
both scenario sets. The image loading and cache-pressure change's `images-pressure` anchor fallback, `has_image`
helper and memory-only pressure rasters remain intact; PR A's `Discussion`,
comment routes/scenarios and 310,000-byte POST limit also remain intact.
Both fixture test sets are retained, including the inline/pressure-anchor
regression and the draft-fence comment-count tests. The client manifest and
lockfile retain both `async-channel` for bounded image loading and `rand` for
random draft UUIDs, with no dependency version changes. The shared hub retains
the asynchronous file queue and the separate comment module/draft exports.
Focused Issues board, NETT client and fixture tests, all four foreground gates
and the pre-push hooks are rerun for this delivery. No frame was recaptured:
the composer/timeline source and all eight PNGs are unchanged.

### PR A review fixes

Each finding was checked against the rebased source. The reply action was
already limited to non-deleted roots; the requested keyboard/depth coverage
was missing. The remaining reported paths were confirmed and corrected.

1. An ordinary POST 401 now prepares credentials once through
   `session_client(..., Some(&client))`, without POST replay. Success or a
   transient refresh refusal keeps editable text and its UUID; missing or
   revoked credentials expire the private scope. Both refresh tests failed
   first, then passed with the bounded-refresh path.
2. Rust hub fixtures now atomically save into a UUID set, rather than merely
   counting requests. Commit/drop + retry/409 and pre-save 400 + retry/201
   each leave one saved comment. Racing hub submits also leave one, with
   single flight permitting only one POST.
3. The sign-out race waits for a test-only completion counter incremented
   after `finish` returns. Its absence assertions no longer use a fixed sleep.
4. A headless test submits through the composer's real shortcut and event
   subscription into the board, verifies one POST and restored editor focus.
   Another checks that replies have neither Reply actions nor second-level
   thread/composer controls under Comments or All.
5. A closed dirty/frozen reply keeps Open and state-appropriate Retry/Discard
   controls, including after resolution, root deletion or permission loss. Reachability
   tests first failed at the missing row, then passed. Existing root, thread
   and notice focus ownership is retained.
6. Retry-After is clamped to five minutes before storage. Decimal accumulation
   saturates, and extreme values cannot wrap into an immediately ready retry.
   Numeric, date and draft-delay regressions failed first, then passed.
7. All timeline installs prune projections by comment ID. Rendering also
   filters against the entire loaded page, even outside its visible slice.
   Both page-install and rendered-merge regressions failed before the fix.
8. Discard warnings clear when an intent leaves the unclear state. Refresh
   warnings belong to the post's key/confirmation serial, rather than any
   future timeline error. Both notice regressions failed first, then passed.
9. Preflight token/network errors and credential panics are known failures;
   they retain text, Retry and the UUID. The regression asserts zero POSTs
   before recovery and one with that UUID afterward. Unwinding after a
   possible send still freezes the intent as unclear.
10. Frozen input says “Choose Retry or Discard.” The shared composer accepts
    an owner-supplied state hint, also exposed to the headless accessibility
    query. Its rendering test failed before the hint was supplied.

The wide multiline draft and unknown-state frames were recaptured and
inspected. The six other original frames are unchanged. An additional
held-reply frame shows the new compact recovery row. The shared composer's
empty submit button is disabled for the GitHub board too; its keyboard
path still reports “Text is required.”

Review quality inspection leaves the same 86 pre-existing chart findings,
with none in changed code. The only baseline field that moves is `total`,
15,494 → 15,529; quality, IOSP and every warning field remain unchanged.
The raw findings check, focused suites, 26 Python fixture tests and four
foreground delivery gates are recorded at the final published tree in the
PR body. No new NETT runtime assumption or live service call was introduced.

Headless waits continue polling the board after the live hub completes: the
worker can finish between a board poll and its test predicate. Queries scope
the hint to the root composer when a reply is also open. The wait observes
rendered hint/value/focus, rather than assuming a fixed number of frames.

The final lifecycle check extended the same regression through root deletion.
It failed because the old timeline filter skipped the entire recovery row for
a deleted root. The filter now delegates to the thread view, whose existing
permission policy still refuses Reply and whose held-draft row remains visible.
This is a separate small follow-up commit; the nine inspected frames are
unchanged because their fixture roots are not deleted.

### Clearance test follow-up

The held-reply test now chooses the platform's document-end key: Command+Down
on macOS, Control+End elsewhere. No other new test uses a platform-specific
editing key; submit shortcuts already use the shared platform macro. Linux
execution verifies this branch; macOS execution remains a captain check.

The discard-warning regression now checks for the absence of Keep draft and
the ordinary unclear copy before clicking Discard on the second intent.
Removing the lifecycle clearing made that assertion fail; the clearing was
restored. The confirmed-post refresh regression also positively checks the
post's own refresh-failure notice, alongside the existing negative later-read
check. The 401 regression requires `Failed(WriteError::Unauthorized)`, retries
to 201 with the same UUID, and asserts exactly one saved comment.

The concurrent-submit fixture holds POST responses until both submit calls
return. A test-only queued-worker counter pairs with the completion counter
so the test waits for every queued send before asserting one POST and one
saved comment. Removing the Posting guard produced two POSTs and failed the
assertion; the guard was restored. No production behaviour changed.

Known visibility limitation: a reply draft is reachable through its root's
thread row only while that root is loaded and visible in the timeline. If
the root is paged out, the draft remains in the memory-only hub; its recovery
row returns when the root is shown again.

The focused authoring/comment suites pass (12 board tests and 14 client
tests), as does warnings-denied Clippy for both crates and all targets.
Quality inspection retains the same 86 chart findings and adds none in
changed code. Only the baseline function count changes, 15,529 → 15,530,
for the assertion helper; all scores and warning fields remain unchanged.
Full foreground gates and pre-push results at the published tree are in the
PR body. Frames are unchanged because this follow-up changes no rendering.
