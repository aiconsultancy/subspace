# Spec: IRC board

## Intent

An IRC client board with strong IRCv3 support, inspired by Halloy's model
(networks and buffers in a sidebar, a topic bar, a member list, nick
completion, condensed join/part, highlights) but built from Subspace's shared
GPUI Kit components so it matches the other boards and themes.

Halloy (GPL-3.0) code may be copied for this internal prototype; each copied
file starts with a comment naming its Halloy source path. Licensing is
resolved later.

## Crates

- `clients/irc` (`subspace-client-irc`): protocol, connections, IRCv3 session
  state and the background runtime. No GPUI dependency.
  - `model.rs` is the contract: `NetworkConfig`, `BufferKey`/`Target`, `Line`,
    `Event`, `Command`, `ConnectionStatus`, `Member`, `Typing`.
  - `runtime.rs`: `Runtime::start()`, `send(Command)`, `drain() -> Vec<Event>`.
    A dedicated thread owns a tokio runtime and one task per network.
  - `format.rs`: mIRC formatting to `Span`s, and `strip`.
  - `proto/`: message parsing and serialisation with message tags (from
    Halloy `irc/proto`).
- `spaces/irc` (`subspace-irc-board`, board id `irc`, title `IRC`).

## Configuration

`config.json`:

```json
{
  "boardSettings": {
    "irc": {
      "settings": {
        "networks": [
          {
            "id": "local", "name": "Local Ergo", "host": "127.0.0.1",
            "port": 16667, "tls": false, "nickname": "subspace",
            "sasl": { "username": "subspace", "password": "…" },
            "channels": ["#subspace"]
          }
        ],
        "highlights": ["subspace"]
      }
    }
  }
}
```

The config file is owner-only (0600). Passwords stay in it and never enter UI
state.

## IRCv3 support

Requested when offered: `cap-notify`, `sasl` (PLAIN, EXTERNAL), `server-time`,
`message-tags`, `batch`, `echo-message`, `labeled-response`, `away-notify`,
`account-notify`, `account-tag`, `extended-join`, `multi-prefix`,
`userhost-in-names`, `invite-notify`, `setname`, `chghost`,
`standard-replies`, `draft/chathistory` (with `draft/event-playback`),
`draft/read-marker`, `draft/multiline`, plus the `+typing` client tag.
`CAP LS 302` negotiation, ISUPPORT (`CHANTYPES`, `PREFIX`, `NETWORK`,
`CASEMAPPING`, `CHATHISTORY`), PING/PONG, nick collision fallback, and
reconnect with backoff. TCP connect and the TLS handshake time out after 15 s
(retried) and can be cancelled with Disconnect, Shutdown or a configuration
change.

Buffer identity follows `CASEMAPPING`: the runtime keeps one display spelling
per folded channel or nick (the first seen, carried across reconnects) and
every event names that `BufferKey`. Member events carry each member's stored
spelling, so the board compares nicks exactly. A `LoadHistory` that cannot be
sent is answered with an empty `Event::History`: `complete` once registered
without `draft/chathistory`, not complete while disconnected or registering.
Commands that need the server while disconnected leave a "Not connected" error
line (input in its buffer, join and part in the server buffer); `Query` still
opens the buffer, and read markers and typing lapse.

## Board UI

- List rows grouped by network (group header shows connection state); rows
  are the server buffer, channels, then queries. Unread counts trail rows;
  highlights make a row bold and raise a notification.
- The detail panel is the open buffer: a topic line, the transcript in Kit's
  `MessageScroller` (timestamps, nick colours, actions, notices, condensed
  join/part/quit runs, mIRC formatting, links), a member list column that can
  be toggled, and a `ChatInput` composer with history (Up/Down) and Tab nick
  completion. `/join`, `/part`, `/me`, `/msg`, `/query`, `/nick`, `/topic`,
  `/whois`, `/quit`, `/raw` and `/quote` work.
- Opening a channel loads recent `chathistory`; a **Load older messages** row
  at the top of the transcript loads older history (Kit's scroller exposes no
  top-of-list signal) and disappears once no older history will load. Live
  lines cap a buffer at 2,000 lines; loaded history raises that buffer's cap
  by what it added, to at most 10,000 lines. Reading a buffer sends
  `MARKREAD`.
- Toolbar: connect or disconnect, join a channel (only while connected; the
  tooltip says why otherwise), toggle members, and a menu to leave a channel
  or close a private conversation.
- Empty state explains the config file and links to Settings.

## Testing

- Unit tests for parsing, formatting, CAP/SASL/registration state machines and
  event mapping, using recorded server transcripts.
- An integration test against a local Ergo server runs when
  `IRC_TEST_SERVER` is set (skipped otherwise); `scripts/irc-test-server.sh` starts one on
  `127.0.0.1:16667` with in-memory history.
- Board render tests for rows, the transcript and the composer; screenshots
  from the capture build against the local Ergo server.
