# Delivery: IRC thread status

## What changed

- `subspace-client-irc`: `Line::client_tags` keeps the client-only tags of a
  `PRIVMSG` or `NOTICE` except `+draft/reply`, live and in history; a
  multiline message takes them from its opening `BATCH` only
  (`session/lines.rs`, `session/batch/multiline.rs`, `model.rs`). The field
  is stored with the line, and older stored lines load without it.
- `spaces/irc/src/thread_status.rs`: the agreed marker (`PROGRESS_TAG`), its
  values and their labels.
- `thread_status/trust.rs`: the trust rule. `trusted` lists a buffer's
  trusted progress notices; `folds` checks one live line before it is
  counted.
- `thread_status/model.rs`: one status per request message ID from the
  folded notices, with endings that stay final, staleness, and the Search
  mapping (`shown_entry`).
- `state.rs`, `state/folding.rs`, `state/apply.rs`, `state/counts.rs`:
  `Buffer::folded` is refreshed after every new line, history page,
  backfill, restore and member list change; folded lines never count as
  unread or notify, and lines counted from history before the server
  vouched are taken back out. `Network::held_at_connect` decides staleness.
- `lines.rs` makes no row for folded lines and never runs a membership row
  across one; `transcript/replies.rs` leaves them out of reply counts.
- `transcript/status.rs` draws one line per request with GPUI Kit's
  `Marker` and the shared `Pill`; `conversation.rs` rebuilds the lines on
  every sync, remeasures a request row whose line changed and reveals a
  folded Search hit at its request.
- `scripts/irc-thread-status-fixture.py`: the disposable fixture below.
- Docs: `docs/irc.md` (Request status lines, Design, Test with a local
  server) and the guide's Transcript section.

## Tests

- Pure (`thread_status/tests.rs`, with the tests in `thread_status/tests/`:
  `markers.rs`, `trust.rs` and `lifecycle.rs`):
  - marker eligibility: untagged, unknown value, unknown suffix, suffix on
    an ending, no reply ID, our own line, a tagged `PRIVMSG` or action and
    the server buffer are not markers;
  - every value's label, and `,disconnected` on stages before the end;
  - trust: only the addressed ship's server account folds; a lookalike from
    another account, the ship's nick with another account, no account, an
    unknown request and a reply to a notice stay notices and count as
    unread; a request without an addressee or without a reported account
    folds nothing; `@nick` addresses too;
  - two requests to the same ship, two of them in one thread, where an
    update to one leaves the others alone and one ends while the others
    work;
  - a question waiting for our answer, the question staying a message, the
    answer, then `clarified`;
  - a main-room final reply and a tagged `PRIVMSG` clearing nothing until
    `failed`;
  - late progress after `completed`, and older progress loaded from history
    afterwards, never reopening the request;
  - a lost connection, a reconnect and `,disconnected` showing the last stage
    as stale;
  - in a private conversation, trusted progress neither counting nor
    notifying while a lookalike does both;
  - history counted before the member list vouched, taken back out of the
    unread count when it arrives;
  - reopening a completed thread after a store round trip without a line,
    and unfinished work reading as stale;
  - the Search mapping;
  - the agent's nick shown as one bounded line of plain text (added when the
    change was re-applied onto `main`).
- `lines/tests.rs`: folded lines make no row, keep a sender's group together
  and split a membership run.
- Client (`session/tests/client_tags.rs`): client-only tags kept live, from the
  opening batch only and in history; server tags and the reply reference
  left out.
- Headless render (`tests/thread_status.rs`): the real board with three
  requests, the third in the first's thread; one line each; folded rows
  absent and the reply count covering only the follow-up and the lookalike;
  an update to the second leaving the first; the question visible; typing
  still in state; the stale line and pill while reconnecting; a main-room
  final clearing nothing; `completed` clearing only the second; and progress
  from a ship whose account the server does not report staying a notice.

Not run by the author of this change on its first branch: `make check`,
`mise run quality`, the tests above and the capture. They were first run
after the re-application onto `main`; see the checks at the end of
[Re-applied onto `main`](#re-applied-onto-main-after-pull-request-29).

## Fixture evidence

Fixture evidence is separate from live fleet evidence, which this change
does not cover: it uses no deployed Stargate, IRC server, fleet or real
account. The fixture plays a fictional ship, `astra`, and a fictional
lookalike, `nova`, on a fresh local Ergo server from
`scripts/irc-test-server.sh`. Both register throwaway accounts there, so the
server tags their messages with `account`, and `astra` sends the agreed
`+stargate/progress` notices.

1. Start a fresh server (its history and accounts are thrown away when it
   stops, so restart it between runs):

   ```sh
   scripts/irc-test-server.sh &
   ```

2. Start the fixture and wait for `thread-status fixture: ready`. It writes
   the Channels configuration, has `subspace` send three requests (the third
   in the first one's thread) and answer the second one's question, then
   leave, and has `astra` and `nova` answer:

   ```sh
   python3 scripts/irc-thread-status-fixture.py --config "${TMPDIR:-/tmp}/irc-thread-status.json" &
   ```

3. Capture from `subspace-desktop`. The fixture times its steps from when
   the capture build joins: after 15 seconds the first request's stream is
   lost, after 30 the second request's reply starts and its final answer
   goes to the main room without a reply tag, and after 45 the second
   request completes.

   ```sh
   CAPTURE_THEME="Twilight Two" \
   CAPTURE_IRC_CONFIG="${TMPDIR:-/tmp}/irc-thread-status.json" \
   CAPTURE_STATE='{"app.window":"[0,0,1440,900]","app.dashboard":"irc","irc.dock-layout":"{\"center\":{\"kind\":\"tabs\",\"buffers\":[{\"network\":\"local\",\"target\":{\"Channel\":\"#thread-status\"}}],\"active\":0}}"}' \
   make capture OUT=docs/changes/irc-thread-status/evidence \
     STEPS='wait:10000;shot:thread-status-working;wait:15000;shot:thread-status-stale;wait:15000;shot:thread-status-final-reply;wait:15000;shot:thread-status-completed'
   ```

   `CAPTURE_STATE` sets a 1440 x 900 window (`app.window`) and opens
   Channels (`app.dashboard`) with `#thread-status` as the one tab of the
   pane, through the layout key Channels stores on `main`,
   `irc.dock-layout` (`spaces/irc/src/dock/layout.rs`). The layout leaves
   out `buffers` and `members`, so the navigator and the inspector both
   show, as they do by default. When another checkout already uses port
   16667, start the server and the fixture with the same `IRC_TEST_LISTEN`,
   such as `IRC_TEST_LISTEN=127.0.0.1:16922`.

The shots should show, in Twilight Two, on the workspace shell: the
navigator with the network and `#thread-status` selected, the pane's tab
strip naming `#thread-status`, the conversation, and the members inspector.
In the conversation:

- `thread-status-working`: the first request with `astra · Using a tool`, its
  ordinary untagged notice and `nova`'s lookalike notice, both visible; the
  third request, in the first one's thread, with `astra · Queued`; the second
  with its question, answer instructions and our answer, and
  `astra · Answer received`. No trusted progress notice shows as a message.
- `thread-status-stale`: the first request's line with an Unplug glyph,
  `Last update HH:MM` and the **Stale** pill; the others unchanged.
- `thread-status-final-reply`: the final answer at the end of the channel and
  the second request still `astra · Writing a reply`.
- `thread-status-completed`: the second request without a line; the first
  still stale and the third still queued.

For the before image, run the same fixture and the same `make capture`
command from an `origin/main` build (the workspace shell, after pull request
#29), where every progress notice shows as a reply. The fixture needs no
particular build. Keep each image at most 1600 px wide; a 1440 px window
captures at that width.

A disconnect of Channels itself is covered by the render and pure tests,
not by a shot: the capture cannot take the network down without clicking
coordinates that have not been measured.

## Re-applied onto `main` after pull request #29

The change was first written on a branch from `27ee242`, before pull
request #29 moved Channels onto the workspace shell (navigator, tab strip,
toolbar and members inspector) and rewrote much of `spaces/irc`. It was
never committed there. It was carried onto `main` (`40b9ebd`) by hand, file
by file, on a fresh branch, `feat/irc-thread-status`; the history above
describes the change as first written.

- **Applied as they were.** Every file the change touched that `main` had
  not changed since `27ee242` took the original hunks by a three-way merge
  against that base, without conflicts: the IRC client (`model.rs`,
  `model/replies.rs`, `network/offline.rs`, `session/batch/multiline.rs`,
  `session/lines.rs`, `session/tests/replies.rs`) and, in Channels,
  `conversation.rs`, `lines.rs`, `lines/tests.rs`, `state.rs`,
  `state/apply.rs`, `state/apply/status.rs`, `state/counts.rs` and
  `transcript/replies.rs`. The new files (`thread_status.rs`,
  `thread_status/`, `state/folding.rs`, `transcript/status.rs`,
  `tests/thread_status.rs`, the fixture and this record) were copied.
- **Re-applied onto changed code.** `lib.rs`, `tests.rs`, `transcript.rs`,
  `transcript/line.rs` and `transcript/line/message.rs` changed on `main`
  (the mono gutter time, the reading-width cap, `cx` passed to the row
  helpers) but not where this change touches them, so the module lines,
  `RowContext::statuses`, the folded set passed to `lines::items` and the
  status line after the reply count merged without conflicts. `docs/irc.md`
  and `spaces/irc/help.md` conflicted with the shell's text and were merged
  by hand, keeping both. In `docs/irc.md` the Request status lines section
  now follows the **Join…** paragraph, so that paragraph stays under its
  own section rather than under this one.
- **Placement.** The status line stays where the change first put it: in
  the transcript, under its request, after the reply count. The shell did
  not change the message row's structure, and the design guide puts a
  conversation's content in the pane, not the inspector (which shows the
  selection's detail, here the members) or the toolbar (search, filters
  and actions). The line sits in the message column, whose text is capped
  at `workspace::READING_WIDTH_PX`; the line itself is short.
- **Render tests.** `tests/thread_status.rs` mounts the board with `mount`,
  injects events with `inject` and opens the channel with `view::reveal`,
  as the other render tests on `main` do; the element ids it reads
  (`irc-line:<seq>:row`, `irc-replies:<msgid>`, `irc-status:<msgid>` and
  `irc-status:<msgid>:stale`) still exist. It reads accessible labels, not
  drawn text, so it needs no `test_text`.
- **Bounded agent name.** The only text from the network the line shows is
  the agent's nick; the stage labels are fixed and the notice's own text is
  never shown. Re-applying added `thread_status::model::agent_name`, which
  drops control characters and whitespace and cuts the nick to 32
  characters with an ellipsis, rather than relying on the server's nick
  rules, with a unit test.
- **Nick status kept apart.** `draft/metadata-2` nick status
  (`Event::NickStatus`) is still ignored here; a separate change,
  `channels-agent-status`, shows it. The two are deliberately different
  (nick-wide, against per request by message ID); see the Findings in
  [the spec](spec.md).
- **Code quality.** `mise run quality` found what the change added to
  `main`'s baseline, all fixed rather than suppressed or rebaselined: the
  pure tests were split into `thread_status/tests/` and the client test
  moved out of `session/tests/replies.rs` into `session/tests/client_tags.rs`
  (module size and a repeated fragment); the long render test became a
  short test over one helper per stage; the message row draws its reply
  count and status line through one helper, `thread_facts`, to stay under
  60 lines; the row context shares the two counts with `Rc::clone`; and
  `ListedChannel` moved from `clients/irc/src/model.rs` into
  `model/listing.rs`, re-exported as before, to keep that module under the
  size limit with `Line::client_tags` added.
- **Capture commands** now use Twilight Two, a 1440 x 900 window and the
  layout key Channels uses on `main`; see [Fixture evidence](#fixture-evidence).
- **Not carried across:** nothing was left out.

Checks after the re-application, run on 21 November 2026 on macOS
(arm64), Rust 1.99.0 through mise. This was the change's first build.

- [x] `cargo test -p subspace-client-irc -p subspace-irc-board`: compiled
      first time, and every test passes (Channels 461 passed, 4 ignored as
      on `main`; the IRC client 241 passed).
- [x] `make check` in `subspace-desktop/`: the first run failed in clippy
      on one `Some(x).filter(|_| …)` (`clippy::some_filter`) in
      `thread_status/model.rs` `shown_position`, now
      `bool::then_some`; the second run passes, exit 0: architecture, board
      docs, documentation index, format, licences, clippy with warnings
      denied, the tests in both configurations (2,380 passed, none failed)
      and the script fixtures. `subspace-framework/` did not change.
- [x] `mise run quality`: passes, 100.0% against the baseline. Its one
      finding, a stale suppression in `products/subspace/build.rs`, is also
      reported on a clean copy of `main` and is not in code this change
      touches.
- [x] `rustfmt --edition 2024 --check` on every changed Rust file.
- [x] `scripts/check-docs-index.py` and `scripts/check-board-docs.py`.
- [x] The fixture against a live local Ergo 2.19.1 server. Its first run
      failed: Ergo answers NickServ's `REGISTER` with notices, not 900, so
      the fixture never saw the login. It now also sends `WHOIS` for its own
      nick and accepts 330 (logged in as) or 900. With that, both accounts
      register and the server tags their lines with `account`.
- [x] Before and after captures under `evidence/`; see [Evidence](#evidence).
- [ ] Not checked: a real Stargate gateway on a fleet (only the fixture);
      a disconnect of Channels itself in a capture (tests only); screen
      readers (accessible labels are checked in headless tests only).
- [ ] Both model reviews (GPT-6.2 Sol and Claude Opus 5.5, high effort):
      not yet run; required before merging.

## Evidence

Captured on 21 November 2026 with `make capture` and the commands in
[Fixture evidence](#fixture-evidence), in Twilight Two at 1440 x 900, from
a fresh local Ergo 2.19.1 server for each run. Fixture data only: the
fictional ship `astra`, the lookalike `nova` and the capture build's
`subspace`, on loopback; each image was examined for hostnames, tokens and
real names before it was added.

After, this branch:

- ![After: one status line per request while the agent works](evidence/after-working.png)
  Working: `astra · Using a tool` under the first request, `astra · Answer
  received` under the second, `astra · Queued` under the third. The
  ordinary untagged notice ("Note: the release dashboard is slow today.")
  and `nova`'s lookalike "finished" notice from another account stay
  ordinary notices; no trusted progress notice shows as a message.
- ![After: a lost stream marks the first request stale while the second writes its reply](evidence/after-stale-and-reply.png)
  The first request's line with the Unplug glyph, `Last update` and the
  **Stale** pill; the second at `astra · Writing a reply` while its untagged
  final answer arrives in the main room, which ends nothing; the third
  still queued.
- ![After: completed clears only its own request](evidence/after-completed.png)
  The second request, `completed`, has no line; the first is still stale
  and the third still queued.

The documented `STEPS` take four shots. The fixture's clock started about
ten seconds before the capture's own steps did, so the second shot already
showed the final reply and the third (`thread-status-final-reply`) matched
the fourth exactly; it was dropped as a duplicate.

Before, an `origin/main` build (`40b9ebd`) with the same fixture:

- ![Before: every progress notice is a message](evidence/before-working.png)
  Every `+stargate/progress` notice shows as a notice message, and the
  request lines carry no status.
- ![Before: progress notices count as unread](evidence/before-completed.png)
  After the stream loss, reply and completion, the progress notices arrive
  as new messages and the channel counts 4 unread.

Compared with the design guide's shell anatomy and review checklist: the
zones are unchanged and nothing is added to the navigator, tab strip,
toolbar, inspector or status bar. The line sits in the conversation under
its request, at the message text's edge like the reply count, in the
caption size with muted text; the Stale pill is the shared `Pill` and the
glyphs are GPUI Kit `Marker` icons, theme tokens only.
