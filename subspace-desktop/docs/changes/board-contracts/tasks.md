# Tasks: board-to-board delivery plan

Back to the [change records](../README.md). This is the plan for all
board-to-board work, including the items handed over by the Issues workspace
work on 15 November 2026. Each phase is one or more pull requests into
`feat/subspace-desktop`; each passes `make check`, `make interfaces` and
`mise run quality`, and a phase that changes what Subspace shows adds
screenshots as [UI change evidence](../../../AGENTS.md#ui-change-evidence)
asks. A phase is done when every criterion under it holds.

Handed-over items keep their origin: slice numbers refer to
[NETT issue workspace tasks](../nett-issue-workspace/tasks.md), section
numbers to its [spec](../nett-issue-workspace/spec.md), and the hub API to
[Issues data and sessions](../../issues.md).

## 1. Contract mechanism (this change)

- [x] `framework/services/contracts` (`subspace-contracts`): `Contract`,
  `Event`, `Request`, `MessageKey`.
- [x] `Dashboard::contracts()` and `Declaration`s, captured by
  `BoardRegistration` without a view and listed by
  `BoardRegistration::contracts`; configured boards declare none.
- [x] Registration validation: names, repeated roles, one responder per
  request.
- [x] The broker in `subspace_dashboard::contracts`: `install`, `publish`,
  `subscribe`, `request`, `respond`; weak receivers; deferred, ordered,
  at-most-once delivery; depth limit; request cycles; refusals logged
  without payloads.
- [x] The host installs routing from the running boards in
  `sync_backgrounds`, between stops and starts.
- [x] `test_support::contracts::Recorder` for boards' published events.
- [x] Tests: [spec](spec.md) section 7.
- [x] Docs: [Board-to-board contracts](../../board-integration.md#board-to-board-contracts)
  replaces the proposal; audit rows, board checklist item, AGENTS.md
  structure rule, documentation index.

Done when: the tests above pass, the product inventory test passes with no
declarations, and `make check`, `make interfaces` and `mise run quality`
pass.

## 2. The `issues` contract and placement (this change, then one follow-up)

- [x] `subspace_contracts::issues` v1: `current-scope` and `resolve`
  requests (batch of at most 50, for the slice 12 batch resolver), the
  `changed` event, `IssueSummary` (title, state, tracker, archived,
  assignee, URL; no credentials). Replaces IRC's planned `irc_snapshot` and
  `irc_version` polling and `irc_resolve_reference` call (docs/issues.md,
  *IRC snapshot and privacy*; hub slice 4 in `clients/nett/src/hub.rs`).
- [x] Placement decision for `framework/services/issues`: [spec](spec.md)
  section 6.
- [x] Move `ReferenceRecognizer`, `IssueReference` and `issue_link` into
  `subspace_contracts::issues::recognition` (with `url`), with their tests
  (`framework/services/contracts/tests`); `nett_markdown` and `clients/nett`
  import them from there.
- [x] Follow-up pull request (whoever holds `clients/nett` then; it need not
  wait for phases 3 and 4): move `IssueResolver`, `resolution_cache`,
  `profile` and `credential` into `clients/nett`; update `clients/nett`
  imports.
- [x] With the issue workspace implementation slice 2 (`spaces/issues` exists): move `nett_markdown`
  into `spaces/issues`, with board-kit's `rich/prepared_tests.rs`; board-kit
  drops its `subspace-issues` dev-dependency; remove
  `framework/services/issues`.

Done when: the contract is in the crate with tests (done), and after the
follow-ups no `subspace-issues` package remains, IRC can build a recogniser
from `IssueScope` alone, and `docs/issues.md` and the NETT spec's section 1
ownership table name the new places.

## 3. The Issues board answers and publishes

Origin: slice 4's hub side (`Hub::publish`, `hub::irc_snapshot()`,
`irc_version()`, `irc_resolve_reference()`), spec sections 1 and 4.

- [x] `spaces/issues` declares `responds` `current-scope` and `resolve`, and
  `publishes` `changed`.
- [x] In `start_background`, after `Hub::start`, an entity owning the hub
  handle attaches both responders and publishes `changed`; `stop_background`
  publishes `changed`, then stops the hub and drops the entity.
- [x] `current-scope` answers from the hub's scope: profile, organisation,
  app origin and selected keys (loaded, else fallback); `None` while signed
  out, without an organisation, or when the pack is off.
- [x] `resolve` answers each identifier from the resolution cache: a card is
  `Available`, a cached 404 is `Unavailable`, a recorded read error is
  `Failed`, signed out is `SignedOut`; otherwise it starts the read
  (`resolve_reference`'s checks and reference priority) and answers
  `Pending`. Another profile's identifiers are `Unavailable`.
- [x] The board publishes `changed` when the hub's reference projection
  version changes (scope or cards), checked on the board's existing timer.
- [x] Remove `Hub::publish` and the free functions `hub::irc_snapshot()`,
  `hub::irc_version()` and `hub::irc_resolve_reference()`; keep only what
  `Hub::start` needs to reuse a live hub. Debug fixture hubs are attached the
  same way as real ones.
- [x] Tests: the recorder sees `changed` on start, stop and scope change; a
  fixture asker gets each resolution state; requests are unavailable after
  stop; the privacy tests still hold (no card logged or written).
- [x] Docs: `docs/issues.md` *IRC snapshot and privacy* becomes the contract
  responder's description; [Declared contracts](../../board-integration.md#declared-contracts)
  rows; audit.

Done when: no code reads the hub's process-wide registry for IRC, and a
fixture board receives every answer and announcement through the broker.

## 4. IRC uses the `issues` contract

Origin: slice 4 (IRC references and opening an issue, the IRC side), slice 8
(reference tooltip), slice 12 (batch reference resolver), spec section 4.
Delivered in [IRC issue references](../irc-issue-references/tasks.md).

- [x] IRC declares `subscribes` `changed`, `requests` `current-scope` and
  `resolve`. Its process-owned hub asks for the scope on start and on every
  `changed`, and builds its recogniser from `IssueScope`; it never depends on
  `clients/nett` or `spaces/issues`.
- [x] Slice 4, IRC side: recognition in formatted and Markdown lines (code
  stays plain); issue links as references with the organisation check;
  `InteractiveText` tones with normal and unavailable looks, catalogued in
  `docs/ui-components.md`; `spaces/irc/help.md` and `docs/irc.md`.
  Partial: Markdown lines show references as links until board-kit's
  reference plugin; **Open issue reference…** is not built (the transcript's
  keyboard focus, slice 9, reaches every reference).
- [ ] Opening: `BoardNavigation` and `Dashboard::open_link` (slice 4,
  section 4) in the Framework, with the return target. The request names the
  `issues` contract, and the shell opens the board that answers it, so IRC
  never names the `issues` board. Design this before building it. Until
  then IRC opens the issue's page in the browser.
- [x] Slice 8: the tooltip on hover and on keyboard focus; `Pending` shows
  the loading tooltip until `changed`; summary, unavailable, signed-out and
  error wording from section 5; unavailable references turn muted.
- [x] Slice 12, IRC side: IRC asks once per frame for the references it
  draws, at most 50 distinct a request, and again after each `changed`. The
  responder's batch read with `POST org/references/resolve` belongs to
  phase 3 (waits for the batch reference endpoint on NETT dev).
- [x] With the Issues board disabled or absent: no recognition, links stay
  ordinary links opened in the browser, no tooltips, and nothing is logged.
- [x] Tests: a fixture responder drives recognition, the tooltip states and
  the batch size; the disabled case; references re-render after `changed`.
- [x] Evidence: references, the tooltip (available, unavailable, loading),
  keyboard focus and IRC with no answering board, from a capture-only
  stand-in responder. Not captured: opening an issue in the Issues board
  (needs `BoardNavigation`), `error-not-found` from IRC, and signed-out and
  error tooltips (wording tested).

Done when: slices 4 (IRC side), 8 and 12 meet their criteria in the NETT
tasks through the contract, and the M1 and full checks in spec section 12
can run. Remaining: `BoardNavigation`, the reference plugin and phase 3's
responder.

## 5. IRC to PRs: `references` `mentioned`

Coordinate with the IRC search and history work
([irc-search-history](../irc-search-history/spec.md)), which indexes IRC messages and
their GitHub references in the framework store.

- [ ] Define `subspace_contracts::references` v1 with the `mentioned`
  event: `ReferenceIdentity`, the framework store target (`Target`) of the
  stored message, and its time; no message text. The contracts crate gains a
  `subspace-search` dependency for those types.
- [ ] IRC publishes it after the message's store write lands, so a
  subscriber that reads the record finds it.
- [ ] PRs subscribes from its background work: marks the pull request as
  discussed and refreshes it; the pull request's related items list the IRC
  message through `related_items`.
- [ ] Tests: the recorder in IRC; PRs' handler with fixture events; the
  docs inventory.

Done when: a GitHub link said in IRC shows in the pull request's related
items, and PRs refreshes it without polling.

## 6. Cross-board user avatars (design first)

Origin: the avatar support change, handed over with the reference and avatar hook designs.

- [ ] Design: how an IRC nick or account maps to a NETT person (or another
  directory); whether avatar images cross the broker as shared bytes or as a
  framework cache entry the message names; privacy and retention.
- [ ] Then a `people` contract: an avatar request answered from memory with
  `Pending` and a `changed` event, as `issues` does; IRC renders avatars it
  gets and falls back to its own initials.

Done when: the design is approved in this record, and IRC shows avatars
from the answering board, with nothing shown when that board is disabled.

## 7. Remaining candidates

From the original proposal's candidate events, now
[candidate contracts](../../board-integration.md#candidate-contracts):

- [ ] `pull-requests` `changed` (PRs background refresh) for Copilot Agents
  and IRC tooltips.
- [ ] `actions` `run-finished` (Actions poll) for the PRs board's checks.

Done when: each is declared, tested with the recorder and listed in the
inventory, and the boards it replaces polling in no longer poll.
