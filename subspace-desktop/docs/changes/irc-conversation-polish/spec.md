# Spec: IRC conversation polish

## Hover (`spaces/irc/src/transcript/fill.rs`)

- Messages, continuation lines, compact server lines and membership runs are
  rounded rows (`control_radius`) with a small vertical padding. The day pill
  and the older-history control are not.
- Under the pointer a row takes the foreground at 6 % opacity. A row that
  mentions you keeps its `select` fill and does not change under the pointer.
- A continuation line still shows its time in the gutter under the pointer.

## Return and Shift+Return (`framework/board-kit/dashboard/src/chat_input.rs`)

- Only a plain Enter sends: `InputEvent::PressEnter { shift: false, .. }`.
  Shift+Enter keeps Kit's line break and sends nothing. Option+Enter (the
  `alt-enter` binding) still adds a line break. Cmd+Enter or Ctrl+Enter
  still sends.
- While the completion list is open, Enter and Tab insert the choice
  (`a91dce7`). Shift+Enter is not bound in that context, so it adds a line
  break; the list then closes because the word before the cursor is empty.
- `set_recall_keys(true)` adds `SubspaceChatRecall` (Tab completes) and, while
  the draft has no line break, `SubspaceChatLineRecall` (Up and Down recall
  sent lines). In a multi-line draft Up and Down move the cursor, except
  while it is a recalled line the user has not edited: the owner marks that
  with `set_recalled(true)`, so a multi-line message does not stop the walk
  through the sent lines. The IRC composer sets it after each recall step
  and on every edit (`Recall::showing`).
- The message input's hint: `Return to send · Shift+Return for a new line` on
  macOS, `Enter to send · Shift+Enter for a new line` elsewhere, prefixed with
  `Markdown · ` in a Markdown input.

## Member list (`spaces/irc/src/members.rs`)

The list fills its host (`size_full`). The right dock owns the width: 240 px
by default, then whatever the user drags and the saved layout restores.

## Removing items

### Menus (`spaces/irc/src/remove.rs`, `view.rs`, `toolbar.rs`)

| Row | Row menu, after **Open to the right** and **Open below** and a separator | Available |
| --- | --- | --- |
| Server | Nothing | — |
| Joined channel | **Leave channel** (`LogOut` icon) | When its network is connected |
| Channel we are no longer in (kicked, `/part`) | **Remove from list** (`X` icon) | Always |
| Private conversation | **Close conversation** (`X` icon) | Always |

The toolbar's **More** menu lists a channel item (named for the selected
channel) and **Close conversation**, each disabled for other kinds of row or
when unavailable.

Choosing one:

1. sends `Command::Part` for a joined channel, then `Command::Forget` for
   any removed buffer. For a private conversation the session forgets the
   nick, so its notices go to the server buffer again;
2. removes the buffer (`Hub::remove`, `IrcState::remove`) and saves the
   removals;
3. closes the buffer's dock panel;
4. keeps the conversation in front in front, or brings the network's server
   buffer forward when the removed buffer was in front, so the list keeps a
   selection.

### Saved state (`spaces/irc/src/state/removed.rs`)

`irc.removed` in the state file is a JSON array of removals:

```json
[{"buffer": {"network": "local", "target": {"Query": "mira"}},
  "since": "2026-11-15T03:22:20.749Z"}]
```

`since` is the time of the newest message someone else sent in the buffer,
which the server set, or absent when there is none; our own lines and local
error lines are left out, because our clock stamped them. Names match
ignoring ASCII case only, without RFC 1459 folding or Unicode case, so on a
network with ASCII case mapping `#a[1]` and `#a{1}` stay two channels; the
runtime matches removed channels the same way.
Unreadable text reads as no removals. The hub loads it at startup, and writes it whenever it
changes: after a removal, after events that bring an item back and after a
Settings save.

### Events (`IrcState::apply`)

Events for a removed buffer are dropped, except:

| Event | Effect |
| --- | --- |
| `Event::Joined` for the buffer (our own join, or `/query`) | Forgets the removal and applies |
| Our own join line in a removed channel | Forgets the removal and applies |
| A message, action or notice in a removed private conversation, newer than `since` or with no `since` | Forgets the removal and applies, with the usual unread count and notification |

`IrcState::open_query` (the member list or a reference) also forgets the
removal. Replayed history (`Event::History` and a bouncer's
`Event::Replayed`), typing notices, quits, nick changes, topics, member
lists and our own part are dropped.

Choosing a notification for a removed private conversation opens it again
through `open_query`; one for a removed channel does nothing
(`view::show_notified`).

### Joining at connect (`spaces/irc/src/hub/running.rs`, `clients/irc`)

- At startup the hub configures the runtime with each network's `channels`
  minus the channels left.
- On a Settings save, a network whose settings did not change keeps the
  configuration it runs, so a channel left since restarts nothing. A network
  whose settings changed starts without the channels left. A channel newly
  added to a network's `channels` forgets its removal first.
- `Command::Forget` (new in `subspace-client-irc`) puts a channel in the
  network task's `Carry` (`left`), whether a session runs, is connecting or
  the network is disconnected. Each connection skips those channels in the
  configured autojoin and in the channels it rejoins after a reconnect, until
  we join them again. A typed `/part` does not use it, so a reconnect still
  joins a configured channel after `/part`, as before.
- Restoring the dock layout drops removed conversations.

## Capture steps (`framework/host/app/src/capture.rs`)

The debug capture build gains `right-click:<x>,<y>` and `move:<x>,<y>` (the
pointer without a click, for hover states).
`scripts/irc-test-server.sh` takes `IRC_TEST_LISTEN` for another port on
`127.0.0.1`; it refuses any other address.
