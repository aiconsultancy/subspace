# IRC client reactions

## Intent

The Channels redesign will show message reactions and mark the reactions
that stop an agent. This change prepares the IRC client layer for it: send,
receive and aggregate IRCv3 reactions, and describe Stargate's stop
reactions as data. The Space's UI is separate work and is not changed here.

## Specification

- Reactions are a `TAGMSG` with `+draft/react` or `+draft/unreact` and
  `+draft/reply=<msgid>`, as in the work-in-progress IRCv3
  [react client tag](https://ircv3.net/specs/client-tags/react), which
  requires `message-tags` and forbids both tags on one message.
- `Update::ReactionSupport` says whether a connection can use them
  (`message-tags` and no blocking `CLIENTTAGDENY`);
  `reactions::reactions_supported(enabled)` answers from capabilities alone,
  and `reactions::check_reaction` returns a typed `ReactionError` before
  anything is sent.
- `Command::React` and `Command::Unreact` send one `TAGMSG`. The session
  rechecks registration, support, the destination, the `msgid` and the
  reaction; a refusal sends nothing and is answered with
  `Update::ReactionRefused`. Offline or unconfigured networks answer
  `NotConnected`; a failed write answers `ConnectionLost`, and the local
  echo is reported only after a successful write.
- Incoming reactions, live, echoed (`own`) and from `chathistory`
  (`history`), become `Update::Reaction`. `Runtime::drain_updates` returns
  them in order with every event; `Runtime::drain` is unchanged for
  existing owners. `Event` gains no variant, because the Channels Space
  matches it exhaustively and must compile unchanged.
- `reactions::ReactionBook` aggregates per buffer and `msgid`, in
  first-reacted order, keeping the latest operation per message, reaction
  and person, with "did I react", unreact, duplicates, nick changes and
  reactions for unseen messages, bounded to 2,048 messages, 20 reactions per
  message and 100 people per reaction (and as many remembered removals).
- `subspace-irc/ircd.yaml.tmpl` also stores `+draft/unreact` in history.
- `reactions::agent_stop` lists Stargate's default stop reactions (✋, 🛑)
  and its matching rule, with the conditions in its documentation, and each
  `ReactionSummary` carries `stops_agent` so a UI can mark them. It has no
  behaviour.
- `NetworkConfig` and `Sasl` moved to `model/config.rs` and the model's
  tests to `model/tests.rs`, unchanged, to keep the model module within the
  quality gate's size limit. Their public paths are unchanged.
- Network input is untrusted: reactions are 1 to 32 characters without
  controls or surrounding spaces, and message IDs at most 128 bytes without
  spaces or controls. Anything else is ignored. Nothing is logged.

Details are in [IRC: Reactions protocol](../../irc.md#reactions-protocol).

## Delivery

What was checked against sources rather than memory:

- **IRCv3 draft.** The react page: `+draft/react` and `+draft/unreact`,
  used with the reply tag, `message-tags` required, no value limits (clients
  may set their own), both tags never on one message.
- **Ergo 2.19.1** (the version `subspace-irc/Dockerfile` pins), read at the
  `v2.19.1` tag: `messageHandler` relays a `TAGMSG` only when it carries
  client-only tags, `dispatchMessageToTarget` sends it only to sessions with
  `message-tags`, and `addEchoMessage` echoes it only with `echo-message` and
  `message-tags`. History keeps a `TAGMSG` only when `itemIsStorable` finds
  one of its tags in `history.tagmsg-storage.whitelist` (any tag name, so
  `+draft/unreact` can be listed), and the channel (`channel.go`) and direct
  message (`client.go`) replays send a stored `TAGMSG` with its tags only to
  clients with `draft/event-playback`. Ergo does not advertise
  `CLIENTTAGDENY`. Our template now lists `+draft/unreact` as well; history
  recorded before that holds additions without their removals.
- **Stargate `main` at `da242f8`**, read only on 22 November 2026, 74
  commits after the installer's pinned v0.43.0 (`fcf6ac7`): `docs/irc.md`
  "Stop by reaction", `routeIrcStop` and `reactionKey` in
  `src/irc/routing.ts`, the `stopReactions` setting (now in
  `src/irc/config.ts`, formerly `src/config.ts`) and its caller in
  `src/irc/ship.ts`. The stop behaviour is unchanged from v0.43.0. The
  installer does not override `stopReactions`.

Not verified: no run against a live Ergo server (so the new whitelist entry
is checked against Ergo's source, not a running server) or a Stargate
gateway; reactions carried on `PRIVMSG` with text (only `TAGMSG` is read).
`CLIENTTAGDENY` handling is tested against recorded lines only.

Checks: the crate's unit tests (sending lines, refusals, live, echoed,
malformed and history parsing, the aggregation's edge cases, the stop
data), `cargo fmt`, Clippy with warnings denied and `mise run quality`.

## Tasks

- [x] Capability support and typed pre-send checks.
- [x] Send and refuse reactions; offline refusals.
- [x] Parse live, echoed and history reactions into ordered updates.
- [x] Bounded per-message aggregation.
- [x] Stargate stop reactions as data.
- [x] Developer documentation.
- [x] The Channels Space shows reactions:
      [Channels reactions](../channels-reactions/README.md), which also
      closes the twelve findings left open here.

No UI change, so no screenshots and no board checklist items change.

## Review findings

Pull request 67's two model reviews (Claude Opus 5.5 and GPT-6.2 Sol, both
clear of P0 and P1) raised these; each was checked against the code.

- **Reactions crossed conversations** (both, P2): fixed. The book keys by
  `BufferKey` and `msgid`; `reactions` and `reacted` take `&BufferKey`.
- **Local echo before the write** (both): fixed. Echoes wait for a
  successful write; a failed write answers `ConnectionLost`.
- **Unroutable reactions dropped silently** (both): fixed. The runtime
  answers `NotConnected`.
- **Identity matching too broad** (both): fixed. The network's case mapping
  (carried on each `Reaction`) folds nicks and accounts; an entry with an
  account matches only that account, so an accountless removal cannot take
  it; renames skip entries with another known account and merge nobody, so
  no list is deduplicated on a nick change. A rename still scans the
  network's entries once, comparing nicks under the case mapping without
  allocating a folded copy (`Casemapping::same`), which the bounds keep
  small. (Corrected in the Channels reactions change: nothing is
  pre-folded any more.)
- **History ownership by current nick** (both): fixed. Ours by account when
  both are known (our account from `RPL_LOGGEDIN`), else by nick.
- **History re-adds removed reactions** (both): fixed in the book (latest
  operation per person by time, with bounded remembered removals) and in
  Ergo's configuration (`+draft/unreact` stored; see Delivery).
- **Eviction test, recency and regrouping** (both): fixed. Recency moves
  only on a change, including removals; groups re-sort by their first
  person; the test now tells the eviction order apart.
- **Test support** (Opus): fixed. `test_support::reaction` builds a
  `Reaction`, and `Answer` carries `updates` and `lost_updates`.
- **Destination validation** (Sol): fixed. `InvalidTarget` refuses a
  buffer that is not one valid channel or nick.
- **`CLIENTTAGDENY`** (Sol): fixed. Honoured for sending and reported
  through `Update::ReactionSupport`.
- **`Runtime::drain` clones** (Sol): fixed. Events are moved.

The second Opus pass confirmed the findings above resolved and found three
more in the fixes, each checked against the code:

- **An older addition rewound the newest operation's time** (P2): fixed.
  Each person's record keeps two times: when their newest operation
  happened, which alone decides what is newer, and when their current
  addition started, for display. An older replayed addition can only move
  a shown person's place earlier, so a live addition at 5 survives a
  replayed addition at 1 and removal at 3.
- **Removal-only groups blocked new reactions** (P3): fixed. When a
  message's group places are full, the group holding only removals with
  the oldest newest operation is dropped before a new group is refused.
- **`CLIENTTAGDENY` ignored `draft/reply`** (P3): fixed. Support needs the
  reply tag allowed too.
