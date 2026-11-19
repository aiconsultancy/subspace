# Comment authoring tasks

[Intent](intent.md) · [Spec](spec.md) · [Design](design.md) · [Delivery](delivery.md)

## Phase 1 — contract only

- [x] Read the assigned Subspace base, repository guides and the comment authoring work in Linear.
- [x] Fetch Indra main in the permitted checkout and inspect source without editing it.
- [x] Pin API/permission/media/interaction evidence; distinguish source facts from proposals and unknowns.
- [x] Specify drafts, errors, duplicate safety, attachment lifecycle, replies, keys/focus and immediate projection.
- [x] Split delivery into text A and images B; plan fixtures, screenshots and human live acceptance.
- [x] Validate the documentation and prepare the requested signed publication; report the final head/tree/config verification in the handoff.
- [x] Tracker-lead captain approves the amended contract: use the draft fence; PR B waits for A to merge.

The 15 November continuation authorises PR A after committing these amendments.

## PR A — text comments and replies

- [x] Write failing loopback-server tests for strict root/reply creation, flat DTOs, permissions, lifecycle, pagination, draft reuse, commit-then-disconnect and response-read denial.
- [x] Extend `scripts/nett_test_server.py` with deterministic comment writes, counters and unclear-failure modes; make its tests pass.
- [x] Write failing client tests for exact request fields, omitted optional nulls, meaningful/UTF-16 body limits, exact ProblemDetails detail, status classification, fixed org/profile and no middleware POST replay. No identity read.
- [x] Implement typed create transport/save response and safe structured errors in `clients/nett`; use `mediaDraftId` for text-only posts.
- [x] Write failing draft/retry tests: one submit, issue/thread retention, scope/expiry clearing, late results, same UUID on retry, discard warning/new UUID and frozen drafts after another refusal.
- [x] Implement hub-owned memory drafts and the draft fence. Assert one server comment after commit/drop then retry/409, refusal-before-commit then retry/201 and concurrent same-UUID requests; a different retry refusal stays frozen with Refresh discussion.
- [x] Write failing headless interactions for root/reply composers, permissions, readonly standalone profile, keys/IME/Escape/focus, failures and Comments/All retention.
- [x] Compose shared input/buttons in the comments area; root-only Reply, no resolving/edit/delete or Properties/Record work.
- [x] Project response immediately, merge/deduplicate the refreshed timeline, retain confirmed content on refresh failure and avoid focus theft after navigation.
- [x] Add synthetic capture states and before/after empty/draft/posting/failed/unknown/reply/narrow screenshots; inspect for layout and privacy.
- [x] Update `spaces/issues/help.md`, `docs/issues.md`, this delivery record and tick tasks; complete the applicable board checklist.
- [x] Run foreground `make check`, `mise run quality`, `make interfaces`, `mise run audit`; document baseline movement (only function count may justify regeneration).
- [x] Publish signed, verify identical local/remote trees and a clean checkout; inspect captain config for hook leaks.
- [x] Write template-compliant PR A body to `/home/jac/src/indra-claude-2-worktrees/.content/comment-authoring/pr-body-a.md`: Part of the comment authoring work, checks at published head, baseline note, checklist, pinned links and screenshots. Captain opens PR.

## PR A review follow-up

- [x] Verify each review finding against the rebased source before changing it.
- [x] Bound 401 credential refresh without POST replay; keep drafts except confirmed expiry.
- [x] Assert saved server counts in stateful Rust fence fixtures and use explicit worker completion for sign-out races.
- [x] Cover the native keyboard submit path, restored focus and root-only reply controls.
- [x] Keep closed dirty/frozen reply drafts reachable after Escape, other threads, resolution, root deletion and permission loss.
- [x] Clamp Retry-After, deduplicate projected rows on every page install and retire stale warning state.
- [x] Classify pre-send failures as known and show the frozen editor's recovery hint.
- [x] Recapture multiline draft and unknown frames, add held-reply evidence and retain the other original frames.
- [x] Update Help, developer/delivery docs and PR body, including the shared empty-submit effect on GitHub.
- [x] Run focused tests, raw quality inspection and all four gates; publish signed and verify tree/config/cleanliness.

## PR B — image paste and attach

- [x] Retain byte-free same-key memos after unacknowledged Remove; TDD covers failed/acknowledged/lost Cancel, live/closed/uncertain replay, different declarations and clearing on Discard/post/scope clear.

- [x] After A merges, start B from updated `origin/feat/subspace-desktop`; do not stack it on A.
- [x] Write failing independent server tests for Prepare key/hash replay, PUT bytes/progress/failures, Complete 200/202, status, versioned cancel, consumed draft and expired/rejected states.
- [x] Extend the loopback server with feature media routes and credential-free opaque upload transfer; include lost Prepare/PUT/Complete/cancel acknowledgements and counters.
- [x] Write failing client tests for exact media fields/states, origin/path/grant/method/deadline validation, no redirect/credential forwarding and redacted diagnostics.
- [x] Implement immutable same-key media attempts, recover-before-retry, finite checking polls and cancellation races; assert one upload/reference per intended image.
- [x] Write failing intake/headless tests for supported clipboard/file types, exact byte hashes, size/pixel/memory limits, duplicate intake, progress, individual errors, removal and navigation/scope clearing.
- [x] Implement native paste/picker intake with memory-only bytes; no GIF/general files, disk temporary images or URL-copy UI.
- [x] Hold submit during preflight with a hub-owned draft lease; cover release, navigation and late-result fencing after Discard, and wait for the native submit control in the paste test.
- [x] Fix repeated Remove during memo replay, dispatch-aware cleanup, lock ordering and expired Ready replay with red/green regressions; strengthen hub boundary and different-byte coverage, release removed buffers before Cancel and scale PUT timeout.
- [x] Rebase onto the theme-button integration, keep labelled attachment actions in the shared Action role, clarify checking recovery reads and mixed clipboard paste, and recapture all four PR B frames.
- [x] Add attachment rows, per-image Retry/Remove, semantic Ready blocks, image-only comments and immediate response image rendering.
- [x] Capture uploading/failed/posted-image states and narrow composition against fixtures; inspect every image and add pinned evidence.
- [x] Update help/developer docs, delivery, tasks and board checklist; repeat all four foreground gates and baseline assessment at final head.
- [x] Publish signed, verify tree/cleanliness/config, and write the full template PR B body at `/home/jac/src/indra-claude-2-worktrees/.content/comment-authoring/pr-body-b.md`. Captain opens PR.

## Captain and jt — acceptance and follow-ons

- [ ] Human WorkOS sign-in and explicitly authorised NETT dev synthetic issue: root, reply, pasted/picked images, clear failure and uncertain retry, no duplicate comments/uploads, images visible.
- [ ] Record the actual deployed revision, permission outcomes and redacted evidence; do not count fixture evidence as a live pass.
- [ ] Track creating or editing issues, comment edit/delete, thread resolution, mention picking/notification UI and any request for larger images as separate follow-ons.

## Board checklist: Issues (PR A and PR B)

Identity and registration
- [x] ID, title and icon retained; description now includes comments.
- [x] Existing product registration and `ARTWORK` retained; no new board.
- [x] `RETAIN_WHEN_DISABLED` remains off; the board owns no service process.

Layout and UI
- [x] Existing `ListDashboard` and issue layout; authoring is confined to discussion.
- [x] Existing toolbar/status/badge; submit has its own frozen Posting state.
- [x] Palette coverage retained for toolbar/menu actions; contextual Reply/Retry/Discard are composer controls.
- [x] Existing board keys retained; shared composer keys and guide agree.
- [x] Existing menus/dialogs retain `modal`/`dismiss`; Discard uses an inline warning, no new dialog.
- [x] Empty, posting, validation and unknown states have appropriate controls.
- [x] Image progress/failure/Ready rows use shared controls after the editor, with per-image Retry/Remove and submit gated on Ready.
- [x] `quit_warnings` explains that unsent/unconfirmed drafts will be discarded.

Settings and configuration
- [x] Settings description updated; notifications/background opt-ins remain off.
- [x] Existing connection settings and scoped memory session used; no new config key.
- [x] No experimental flag needed for the approved authoring contract.

Data caching and refresh
- [x] Existing preferences retain choices; draft persistence is explicitly prohibited.
- [x] Framework data persistence is not applicable: private drafts/content stay in memory.
- [x] Existing loaded discussion remains visible while the head refreshes.
- [x] Posting, intake, uploads and refresh run off the UI thread; image checking is bounded to 30 polls while its issue is open.
- [x] Draft images and byte-free Remove memos remain in memory and clear on post, Discard or scope expiry; buffer budgets include active transfers, and removed work releases buffers before Cancel.

Search
- [x] Existing item search/recent-item navigation retained.
- [x] Full-text indexing is not applicable: private issue content and drafts are not indexed.
- [x] Cross-board metadata is not applicable to draft authoring; existing references remain read only.

Agent tools, MCP and CLI
- [x] No tools: writes require an explicit native composer action; drafts are not exposed to agents.
- [x] App-only tool registration is not applicable; no tool added.
- [x] MCP/CLI tool lists need no change because their interfaces are unchanged.

Chat context
- [x] Draft text is never attached to chat; the board has no chat context.

Notifications and attention
- [x] Notifications are out of scope; no `notifications::request` added.
- [x] Process notices/badges are not applicable; composer outcomes are inline.

Help and docs
- [x] Help follows the board guide template; native screenshot evidence is included.
- [x] Existing index/board links reach Help and the updated developer page.

Communicating with other boards
- [x] Existing issue-reference contracts retained; no authoring contract published.
- [x] Only the shared NETT hub is followed; no dependency on another board.

Links, logging and diagnostics
- [x] Existing safe link/navigation handling retained; no upload or grant URL UI added.
- [x] Writes expose typed safe errors and redacted draft Debug; neither requests nor response bodies are logged or copied.

Testing and evidence
- [x] Pure client/draft logic and headless composer interactions are tested.
- [x] PR A native fixture frames retained, including the base before frame; PR B adds four inspected upload/failure/posted-image/narrow frames.
