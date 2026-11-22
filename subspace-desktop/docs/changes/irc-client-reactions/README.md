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
- `reactions::reactions_supported(enabled)` says whether a connection can
  use them; `reactions::check_reaction` returns a typed `ReactionError`
  before anything is sent.
- `Command::React` and `Command::Unreact` send one `TAGMSG`. The session
  rechecks registration, `message-tags`, the buffer, the `msgid` and the
  reaction; a refusal sends nothing and is answered with
  `Update::ReactionRefused`. Offline networks answer `NotConnected`.
- Incoming reactions, live, echoed (`own`) and from `chathistory`
  (`history`), become `Update::Reaction`. `Runtime::drain_updates` returns
  them in order with every event; `Runtime::drain` is unchanged for
  existing owners. `Event` gains no variant, because the Channels Space
  matches it exhaustively and must compile unchanged.
- `reactions::ReactionBook` aggregates per network and `msgid`, in
  first-reacted order, with "did I react", unreact, duplicates, nick
  changes and reactions for unseen messages, bounded to 2,048 messages, 20
  reactions per message and 100 people per reaction.
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
  `message-tags`. History keeps a `TAGMSG` only when `tagmsg-storage` allows
  it, which `subspace-irc/ircd.yaml.tmpl` limits to `+draft/react` and
  `+react`, and replays it only with `draft/event-playback`. Ergo does not
  advertise `CLIENTTAGDENY`. So an unreact is not stored: a removed
  reaction can come back from history.
- **Stargate `main` at `da242f8`**, read only on 22 November 2026, 74
  commits after the installer's pinned v0.43.0 (`fcf6ac7`): `docs/irc.md`
  "Stop by reaction", `routeIrcStop` and `reactionKey` in
  `src/irc/routing.ts`, the `stopReactions` setting (now in
  `src/irc/config.ts`, formerly `src/config.ts`) and its caller in
  `src/irc/ship.ts`. The stop behaviour is unchanged from v0.43.0. The
  installer does not override `stopReactions`.

Not verified: no run against a live Ergo server or a Stargate gateway; the
client's handling of a `CLIENTTAGDENY` that blocks reactions (it does not
read it); reactions carried on `PRIVMSG` with text (only `TAGMSG` is read).

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
- [ ] The Channels Space shows reactions (separate UI work).

No UI change, so no screenshots and no board checklist items change.
