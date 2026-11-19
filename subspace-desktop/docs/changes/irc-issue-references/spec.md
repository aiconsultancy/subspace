# Spec: IRC issue references

Back to the [change records](../README.md). The developer description is
[IRC issue references](../../irc.md#issue-references); the contract is in
[board-contracts spec](../board-contracts/spec.md) section 5.

## 1. Roles and scope

- IRC declares `subscribes` `issues@1/changed` and `requests`
  `issues@1/current-scope` and `issues@1/resolve` (`issues::CONTRACTS`).
- The process-owned hub subscribes from `hub::start` and asks for the scope
  at start and on every `changed`, so references stay current with the view
  closed. `None` and `Unavailable` are no scope: nothing is recognised.

## 2. Recognition

- `markup::IssueKeys` wraps the contract's `ReferenceRecognizer` (the scope's
  team keys, which already fall back to the profile's) and `IssueLinkContext`
  (app origin, organisation). `markup::classify` checks, per word: an issue
  link, another URL, `#channel`, `@member`, a leading issue identifier, then
  `nick:` addressing.
- Formatted text (`style::with_refs`): an issue link becomes a reference and
  leaves the links. Markdown lines (`markup/markdown/inline.rs`): bare
  identifiers, bare issue links and `[label](issue link)` become
  `subspace-irc:issue/{identifier}` links; code stays as typed.

## 3. Resolving (slice 12)

- Each drawn message row reports its distinct identifiers
  (`transcript::row_issues`). `IssueReferences::want` keeps those not yet asked
  this round, in order, and the first new one of a frame defers one
  `resolve_wanted` (`App::defer`), which asks in requests of at most
  `MAX_RESOLVE` (50).
- Answers are kept per identifier with their round. `changed` starts a new
  round: answers stay shown and are asked again when drawn. A scope with
  another profile, organisation or keys forgets them. A refused `resolve`
  drops the scope until the next `changed`.
- Everything is in the hub's memory. Nothing is written to IRC's store,
  search, state file or log.

## 4. Look and tooltip (slices 4 and 8)

- `RunStyle` gains `unavailable` and `focused`. References: accent on a 16 %
  accent tint (unchanged); `Unavailable`: muted text with a muted underline;
  focused: strong text on a 40 % accent tint, underlined in the accent.
- The tooltip (`issues::tooltip`) is a Kit `Tooltip::element` that reads the
  hub when it draws: identifier (and *Archived*), title, state glyph (the
  Issues board's state icons and tones) and the facts *state · Tracker ·
  assignee or Unassigned · profile*. Without a summary: *Loading {id}…*,
  *{id} isn't available with {profile}.*, *Sign in to {profile} to see this
  issue.*, *Couldn't reach {profile}.* It has no menu, so no chat.
- Formatted (Markdown) lines show issue references as links without a card,
  muted look or focus highlight until board-kit's shared reference plugin.

## 5. Keyboard (slice 9)

- `Shift+Tab` in the message input (`IrcConversation > Input`, ahead of the
  text area's outdent) focuses the newest message. In `IrcTranscript`: `↑`/`↓`
  previous and next message (stopping at the ends), `Tab`/`Shift+Tab` the
  message's links and references in reading order (`Shift+Tab` from the first
  returns to the message), `Enter` opens, `Esc` returns to the input. Blur
  clears the focus.
- A 2 px ring in the theme's `ring` colour is drawn over the focused row, so
  the text does not move, and a focused issue reference shows its card under
  the message.

## 6. Opening

A click or `Enter` opens the summary's URL, or `{app origin}/issues/{id}`, in
the browser (`links::open`). Opening in the answering board is the
Framework's `BoardNavigation`, not built yet.

## 7. Evidence fixture

Debug capture builds with `SUBSPACE_CAPTURE_ISSUES` add a stand-in board,
`capture-issues`, to the broker's routing (`capture::issues::with_issues` in
`sync_backgrounds`) that answers `current-scope` and `resolve` with fictional
data and publishes `changed` when it attaches. Release builds never contain
it (`#[cfg(all(debug_assertions, feature = "capture"))]`).

## 8. Tests

- Contracts crate: the recogniser and issue-link tests moved with the code.
- `issues/tests.rs`: no scope, coalesced and deduplicated requests, the 50
  cap, rounds and `changed`, scope changes, a refused request, tooltip
  wording, summary facts, open targets.
- `markup/issue_tests.rs`, `style/tests.rs`: recognition in plain and
  formatted text, the organisation check, no scope, issue links as
  references, unavailable marks.
- `transcript/focus.rs`: focus steps.
- `tests/issues.rs`: the declarations; through the real broker with a fixture
  responder board, one deduplicated request for a frame, `Pending` then
  `changed` then asked again, no responder leaves text plain, a stopped
  responder ends recognition. `tests/focus.rs`: the keyboard path and opening
  an issue.
- `capture/issues.rs`: the fixture's answers.
