# Channels Thread tab

## Intent

The owner's mock-up of the Channels redesign gives a ship's channel's
inspector the tabs **Ship · Thread · People** (a Starbase **Starbase ·
Thread · People**) and a settled request line reading `Done in 42 m · Wren
closed NAV-6 · 3 replies ›`. [Channels fleet live](../channels-fleet-live/README.md)
built Ship, Starbase and People. This change builds **Thread**: one thread at
a time, being a request (a message addressed to an agent) or any message
with replies, opened from its reply count or its status line, with a way to
reply into it. The mock-up does not draw the tab's contents, so they are
designed here within the design guide, kept plain.

Desktop only: no Framework, API or Stargate change.

## Spec

### What the tab shows

Top to bottom (`view/inspector/thread.rs`, `thread/steps.rs`):

1. **The root message**: author, rank pill, time and text with its reference
   chips, drawn by the transcript's own header and body
   (`transcript::thread_message`), without avatar, tiles or attachments.
2. **Status**, for a request with trusted progress or a kept ending: a plain
   list, one row per step the agent reported. A step is a change of the
   status line's stage (`thread_status::Stage`): **Accepted**, **Working**,
   **Waiting for an answer**, **Writing the reply**, then **Done**,
   **Failed** (red text) or **Ended**. Each row has its server time and how
   long it lasted, until the next step began; the ending has the request's
   whole time (`in 42 m`, `after 3 m`); a running last step says
   `since 17:50` and no duration. If the ending is kept but no notice before
   it is held: `The steps before the end are not loaded.`, with **Load around
   this message** (`Command::LoadAround`, as Search uses) while the network
   is connected and keeps `draft/chathistory`.
3. **Replies · N**, grouped by author by the transcript's rule
   (`lines::continues`), or `No replies yet.`

With no thread chosen the tab says, in one line, how to open one. Its footer
holds **Reply in thread**, or, without a connection or `message-tags`, the
reason in its place.

### Which messages belong, and the trust rule

- A reply is a message (`PRIVMSG`, action or notice), not folded progress,
  whose `+draft/reply` leads through held parents to the root, read by the
  same `ThreadRoots` as the transcript's reply counts
  (`thread/model.rs`). The tab's replies and the count therefore agree, a
  reply to a reply belongs, and nothing in a message's text places it in a
  thread.
- A step is only a notice the buffer folded (`Buffer::folded`), which it does
  only when the notice's server-set `account` equals the nick the held
  request asks: the rule the status line already uses. An untrusted or
  lookalike notice stays an ordinary reply and never makes a step; a notice
  after the ending makes none.
- A message addressed by its first word but with no progress, such as
  `Morning, all.`, has no timeline, as it has no status line.

### Opening it

- In a ship's channel or a Starbase (`thread::has_tab`) a message's reply
  count reads `3 replies ›` and is a button; a request's running or settled
  line is a button and ends with `· 3 replies ›` only when it has real
  replies, and the separate count row is then left out. Choosing either
  emits `ConversationEvent::OpenThread(<msgid>)`; the board chooses the
  thread, shows the inspector if hidden and opens its Thread tab. Enter on a
  focused message with either does the same (`conversation/keys.rs`).
- Elsewhere (the command channel, other channels, private conversations)
  counts and lines stay read-only as before: there is no Thread tab there.

### State

`thread::Threads` keeps one chosen root per conversation for the session,
so it survives the inspector's tabs and switching conversations. On each
board sync it rebuilds a chosen thread only when its buffer's fingerprint
changed (held count, first and last sequence, folded count, the root's
ending), so a sync that leaves the buffer alone reads none of its lines; it
forgets a thread whose root is no longer held (line cap, conversation
removed) and takes its composer out of reply mode.

### Reply mode

**Reply in thread** puts the conversation's composer into reply mode
(`compose/reply.rs`): a row above the input, `Replying in thread to mira:
<start of the root>`, with **Cancel reply** (×), Escape also cancelling, and
the placeholder `Reply in thread to mira`. Sending sends
`Command::Reply { buffer, msgid: <root>, text }`: a `PRIVMSG` to the channel
with `+draft/reply=<root msgid>`, so everyone in the channel sees it, then
leaves reply mode. A `/command` is refused (a leading `//` is a literal
slash), as are attachments, each with the reason in the composer. Reply mode
starts only in a channel; `reply::command` refuses any other buffer, so
nothing here sends a private message. Without a connection or `message-tags`
it is refused with the same reason the footer shows. Choosing a different
thread leaves reply mode.

### Design choices where the mock-up is silent

- No avatars in the tab: the root and replies show author, rank pill and
  time as text, so no agent icon appears and the Icons setting has nothing
  to govern here.
- The timeline is a plain list without dots or a rule: the design guide has
  no timeline pattern. Selection, grouping and the failed step show through
  text and tone; there is no accent edge on the tab, the rows, the root or
  the reply row.
- The tab's content starts at the inspector body's own text edge, with no
  inset of its own.
- The tab has no count: the reply count is in the Replies heading.

### Left out, and what each needs

- **`Wren closed NAV-6`** on the settled line, and the mock-up's
  *delegated* and *pull request* steps: Stargate's `+stargate/progress`
  carries a stage only. They need the progress contract to carry who closed
  what, delegation and a pull request.
- **A reply count from the server**: counts and the tab cover loaded lines
  only; this needs the reply-search follow-up (a server lookup by thread).
- **Attachments in a reply**: the upload path sends its own lines without
  the reply tag; it needs `uploads::outgoing` to carry a reply target.
- **A reply in a private conversation**: by the owner's rule, never.

## Delivery

- Model and state: `spaces/irc/src/thread.rs`, `thread/model.rs`,
  `thread/chosen.rs`; `thread_status` exposes `agent_name`.
- Transcript: `transcript/replies.rs` (`ThreadLink`, `link`, the count as a
  button), `transcript/status.rs` (the line as a button with the count),
  `transcript/line/message.rs` (`thread_message`, `thread_facts`),
  `RowContext::threads`.
- Inspector: `view/inspector.rs` (`InspectorTab::Thread`, three tabs, the
  footer), `view/inspector/thread.rs`, `thread/steps.rs`, `thread/footer.rs`.
- Composer: `compose/reply.rs`, `compose.rs`, `compose/send.rs`.
- Board: `conversation.rs` (`OpenThread`), `dock/open.rs`, `view.rs` (the
  refresh on sync), `conversation/keys.rs` (Enter).
- Fixture: `scripts/irc-history-fixture.py --thread` and
  [Desktop capture](../../capture.md).
- Tests: `thread/tests.rs`, `thread/tests/timeline.rs` and
  `thread/tests/lifetime.rs` (membership, nested replies, foreign buffers,
  folded progress, steps and durations, a running step, untrusted notices,
  an ending without its notices, the count agreeing, where the tab is
  offered, reply refusal, rebuild only on change, lifetime under the line
  cap and on removal); `compose/reply/tests.rs` (what is sent, to where,
  with which tag, the literal slash, refusals); `view/inspector/thread/steps.rs`
  (step words); `view/inspector/tests.rs` (three tabs);
  `tests/initiatives/thread.rs` (render tests: the prompt, the settled line's
  count and click, the root, steps and replies drawn, a count opening a
  plain message's thread and a reply arriving live, the choice kept per
  conversation, reply mode sending `Command::Reply` and cancelling, the
  footer's refusal without `message-tags`).
- Documentation: [Channels](../../irc.md#the-thread-tab) and the in-app
  Channels help.

## Captures

Captured on macOS with `make capture` at 1440 x 900 in Twilight Two, with
`CAPTURE_FLEET=1 CAPTURE_FLEET_API=1`, against a fresh loopback Ergo 2.19.1
(`scripts/irc-test-server.sh` with `IRC_TEST_LISTEN=127.0.0.1:16951`) and
`scripts/irc-history-fixture.py --status --thread`, both stopped by their
process ids afterwards, and a Channels config naming that server as the
network **Acme IRC** with the aurora channels. Every name, issue and message
is fictional. The fixture plays the requests a few seconds apart, so the
durations are seconds, not the mock-up's minutes. Steps:
`wait:9000;click:70,215;wait:4000;shot:status-replies;click:1194,55;wait:1500;shot:thread-prompt;click:340,375;wait:2000;shot:thread-finished;click:380,591;wait:2000;shot:thread-running;click:340,375;wait:2000;click:1196,851;wait:1000;type:Thanks, that settles the baseline.;wait:800;shot:reply-mode`.

![The ship's channel: mira's finished request reads "Done in 11 s · 3 replies ›" and her running one "aurora-lyra · Using a tool … working · 1 reply ›"; the inspector's tabs read Ship, Thread, People 6](evidence/status-replies.png)

![The Thread tab before a thread is chosen: one line saying to choose a message's replies or a request's status line](evidence/thread-prompt.png)

![The Thread tab on the finished request: the root, Status with Accepted, Working, Writing the reply and Done in 11 s, then Replies · 3 grouped by author, and Reply in thread in the footer](evidence/thread-finished.png)

![The Thread tab on the running request: Accepted, then Working since 22:29 with no duration yet, and its one reply](evidence/thread-running.png)

![Reply mode: above the input, "Replying in thread to mira:" and the start of the request, with × to cancel, and the draft typed](evidence/reply-mode.png)

Each image was compared with the design guide's shell anatomy: the tabs sit
in the inspector header on the strip line, the open tab shows by its surface
and stronger text, the footer's action is in the inspector footer, text
starts at the panel's edge, and no image has an accent line on one edge.

## Not verified

- A light theme, a narrow window and a Starbase's Thread tab: covered by
  tests, not captured.
- **Load around this message** against a real gap: the capture fixture has
  no stored ending without notices; tests cover the note, not the load.
- The refusal footer without `message-tags`: covered by a render test, not
  captured, since the loopback Ergo agrees it.
- A sent reply reaching other clients: `Command::Reply` and the client's
  tagging are tested; no second client was watched in a capture.
- A real Stargate: only the fixture's tags.

## Tasks

- [x] Thread tab with root, timeline and replies.
- [x] Reply counts and status lines that open it; Enter on a message.
- [x] Chosen thread per conversation, rebuilt only on change.
- [x] Reply mode to the channel with the reply tag, refusals.
- [ ] Who closed what, delegation and pull request steps, once the progress
      contract carries them.
- [ ] Server-side reply counts (the reply-search follow-up).
- [ ] Attachments in a thread reply.
