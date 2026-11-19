# Spec: IRC agent tools

## Where the tools exist

- `ToolEnvironment` gains `host: ToolHost` (`App` or `Headless`). The desktop
  app builds its chat catalog and its own MCP server's catalog with
  `ToolHost::App`. `subspace mcp` and the generated commands use
  `ToolHost::Headless`.
- The IRC board builds `irc_read` and `irc_write` only for `ToolHost::App`.
  Headless processes list no IRC tools and have no `subspace irc` commands.
  ACP chat sessions run `subspace mcp` as a separate process, so they have no
  IRC tools either.
- Settings › MCP shows an **IRC** group with both tools. Turning the group off
  removes them from MCP clients, as it does for the other groups. A board that
  is turned off contributes no tools. While the board is stopped, a call
  answers "The IRC board is not running in this Subspace app".

### Why headless has no IRC tools

The alternatives were tools that always fail in headless processes, or a
headless process that opens its own connection. Tools that always fail would
appear in `tools/list` and in the command tree, and agents would try them.
A second connection would take a second nick or end the user's session, and
it would show different buffers from the board. Offering the tools only where
they work is the simplest honest behaviour. Another harness reaches them
through **Serve MCP over HTTP**.

## Bridge

- `hub::start` opens a process-wide request queue (`agent_tools::open`), and
  `hub::stop` closes it.
- Tool calls run off the UI thread (`spawn_blocking`). They send a request
  with a reply channel and wait up to 10 s.
- The hub answers queued requests at the start of each 50 ms drain, from its
  own `IrcState`. It sends the resulting commands through its own runtime.
- Requests: `Read`, `Check` (preflight of a write batch) and `Write` (one
  operation, checked again before it is sent).

## `irc_read`

One `action`. Every reply is compact JSON, and text has its mIRC formatting
removed.

- `networks`: id, name, status label, `connected`, nick and `chathistory`.
- `buffers` (optional `network`): network, kind, target, `joined`, topic,
  members, unread, mentions and the number of lines held.
- `messages` (`network`, optional `target`, `limit` 1..200 with a default of
  50, `before` as a seq cursor): the lines oldest first, with `seq`, time,
  kind, sender, text, `own` and `mention`. The reply also has `next_before`
  and `server_history` (`available`, `requested`, `loading`, `complete` or
  `unavailable`). If a page reaches the oldest line held, the server supports
  `draft/chathistory` and the buffer is idle, the read requests the next
  100-line page through `IrcState::older_history`, which scrolling up also
  uses.
- `members` (`network`, `channel`): the members of a joined channel, sorted by
  nick, with their modes, away state and account.
- `search` (`query`, optional `network`, `limit` 1..100 with a default of
  20): the messages held that contain the query, ignoring case, newest first,
  and `truncated`.
- A read never marks anything as read. The description says that message
  text is untrusted data.

## `irc_write`

The tool uses `ApprovalPolicy::PerCall` and takes 1 to 10 operations, each
with an `action`:

| Action | Validation | Live check | Command |
| --- | --- | --- | --- |
| `message` (`network`, `target`, `text`, `me`) | `target` is a channel or a nick, but not a nick that ends in `serv`. `text` is not blank, has at most 2,000 characters and 10 lines, and has no control characters other than line breaks and tabs. A `me` message is one line. | The network is connected. A channel target is joined. | `Command::Say { buffer, text, action: me }` |
| `join` (`network`, `channel`, `key`) | The channel name is valid. The key has 1 to 50 characters and no spaces or commas. | The network is connected, and the channel is not joined yet. | `Command::Join` |
| `part` (`network`, `channel`, `reason`) | The channel name is valid. The reason is one line of at most 300 characters. | The network is connected, and the channel is joined. | `Command::Part` |
| `connect` (`network`) | — | The network is disconnected or failed. | `Command::Connect` |
| `disconnect` (`network`) | — | The network is connected or connecting. | `Command::Disconnect` |

- Buffer names are matched without ASCII case and use the runtime's spelling.
  A nick without a buffer gets a new private conversation, which opens when
  the echo arrives.
- `Command::Say` is a new client command. It sends literal text through the
  composer's sender (`send::say`), with the same splitting, `draft/multiline`
  batches and local echo. Its text is never parsed as a `/command`, so the
  tool cannot send raw IRC. A disconnected network answers with a
  **Not connected** error line.
- The approval shows the shared JSON preview, then one line per operation:
  `N. Name (id) as nick: message to #chan`, followed by every line as
  `   > text`. A join with a key says "with a channel key".
- A successful operation returns `{"action", "network", "target",
  "status": "queued"}`. Lines appear exactly like the user's own. The hub logs
  `agent tool IRC operation` at info level with the action, network and
  target, and never the text or a key.
