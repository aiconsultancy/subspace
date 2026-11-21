# Channels agent status lines: behaviour

Back to the [change record](intent.md).

## Holding statuses

- `Event::NickStatus { network, nick, status }` sets, replaces or clears the
  nick's status in its network's `Network::statuses`
  (`spaces/irc/src/state/statuses.rs`). Statuses are held per network,
  because they describe the whole nick, and never per buffer.
- The value is cleaned again where the Space holds it, with the client's
  own cleaner, now public as `subspace_client_irc::status_text`: IRC
  formatting, controls and bidi overrides removed, line separators made
  spaces, trimmed, at most 80 Unicode characters including the ellipsis of
  a cut. `None`, or nothing left after cleaning, clears the status.
- Nicks compare ignoring ASCII case, as `members::note` does: the Space
  does not know the network's case mapping, and the runtime reports a status
  under the first spelling it saw, which may differ from the member list's.
  A status set under another spelling replaces the held one.
- Statuses are never stored. The hub's store keeps lines only; the state
  file and the reconnect carry data have no statuses.

## Clearing

- The runtime's clear (`status: None`): the gateway ended the turn, or the
  nick quit, changed nick, left the last channel we share, or the
  capability or connection went.
- Whenever a network's `ConnectionStatus` becomes anything but connected,
  every status of that network is dropped at once, whether or not the
  runtime's clears arrived, so no stale status shows after a reconnect.
- When a nick leaves a channel (`MemberLeft`, or the old nick of
  `MemberChanged`) and no channel of that network we hold still lists it,
  its status is dropped. This matches the runtime's own rule for the last
  shared channel, and covers a late clear. It cannot clear the peer of a
  private conversation wrongly: a peer that shares no channel with us has no
  channel departure, and a peer that leaves our last shared channel is
  cleared by the runtime too.
- A line from the nick ends its typing notice, as before, but not its
  status: the gateway clears that itself.
- When the nick is no longer in a conversation, that conversation simply
  stops showing its status.

## Above the composer

- The line under the transcript (`spaces/irc/src/transcript/activity.rs`)
  shows, for the open conversation, the statuses of the nicks in it: a
  channel's members, or a private conversation's peer. Never our own nick's
  status, and never the status of a nick that is not in the conversation.
  The server buffer shows no status.
- Composition, a pure function (`activity::compose`): the status texts
  first, verbatim, joined with ` · ` and ordered by nick; then the typing
  text for the nicks that have no status (`alice is typing…`,
  `alice and bob are typing…`, `3 people are typing…`). A nick with a
  status that is also typing shows its status only. With no status, the
  line reads exactly as the typing line did.
- One line, the same caption size, muted colour and fixed row height as the
  typing line, so the transcript does not jump. It is plain text, cut with
  an ellipsis when too long, with the whole line in a tooltip
  (`truncated::text`).
- Accessible name: each status as `Status of <nick>: <text>`, then the
  typing text, joined with `; `, such as
  `Status of galactica: Galactica is thinking…; alice is typing…`. The line
  has no name while it is empty.

## In the members inspector

- A member with a status has a second line under the nick inside the same
  row: one line, caption size, muted, cut with an ellipsis and the whole
  status in a tooltip when cut (`truncated::text`). The row grows to fit
  (`row_button(…).h_auto()` with `py_1`, as the Issues Space's end-state
  picker does with its row button); rows without a status keep their
  height. The role note (`you`, `owner`, …) stays after the nick on the
  first line, and away members are dimmed as before, status included.
- The row's accessible name adds the status:
  `Open private messages with <nick>, <status>`, then `, away` as before.

## Not changed

- Nothing in the transcript: no line, unread count, highlight, notification
  or search document comes from a status.
- Agent tools, chat context and the Spaces' contracts do not see statuses.
- No setting, stored choice, palette command, key or menu is added.
