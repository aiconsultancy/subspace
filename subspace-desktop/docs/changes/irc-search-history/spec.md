# IRC history and full-text search: spec

Back to the [change records](../README.md). Intent: [intent.md](intent.md).

## Design note

### One store: the board's SQLite full-text store

Subspace already has a private, per-board SQLite database with an FTS5 index:
the search service (`framework/services/search`). Each searchable board has its
own file (`<state dir>/search/<app>/<board>.sqlite`, directory 0700, file 0600),
its own policy (enabled, retention days, byte quota) and one active scope. The
Search view, chat and MCP already query it.

IRC keeps its messages there and nowhere else. There is no JSON history cache
and no second database. Every channel and private message the hub sees (live,
replayed by a bouncer, or sent by the server as chat history) becomes one
`Document` in IRC's store. On startup the hub reads its own documents back to
restore each buffer, then asks the server only for what it missed.

This is the pattern for every board: a board persists its data in the
Framework's SQLite full-text store and reads it back through the board storage
API below; global search queries the same records. `prefs` stays for small UI
choices, and `prefs::write_cache` only for small view snapshots that are not
searchable.

### Board storage API (search service, services layer, no GPUI)

The write path exists: `Service::upsert` and `Service::index_metadata`. This
change adds the generic read and delete paths a board needs, all scoped to the
board's active scope and checked like every other call (policy enabled, scope
active, within retention):

| Call | What it does |
| --- | --- |
| `records(scope, &RecordQuery)` | A board's own documents matching `Filters` (kind, account, project, parent, time range), newest first, with a keyset cursor (`refreshed_at`, `id`) for the next page. Up to 1,000 per page. Payloads are returned, unlike search hits. |
| `groups(scope, &Filters, limit)` | The distinct `parent_id`s of matching documents, with their count and newest time: a board's containers, such as IRC buffers. |
| `remove_matching(scope, &Filters)` | Deletes every matching document and its references. At least one facet is required; `clear` wipes a whole board. |
| `index_metadata_batch(scope, &[(id, IndexMetadata)])` | `index_metadata` for many documents in one transaction, so a batch of 100 messages costs one write, not 100. |
| `offer_default(board, policy)` | Applies a board's declared default policy until the person chooses one in the Search view. Their choice always wins. |

Boards reach the service through `subspace_dashboard::full_text_service` and
`ToolEnvironment::search`, as before. Nothing in the host changes except one
call that offers the declared default.

### Schema migration

`migration.rs` moves the schema from version 2 to 3 by adding
`settings.chosen` (0 or 1). `set_policy` sets it; `offer_default` applies a
policy only while it is 0. Existing stores whose policy differs from the
default are marked chosen, so an earlier choice is kept. Facet columns and
indexes (`kind`, `project`, `parent_id`, `refreshed_at`) already exist from
version 2; `records` and `groups` use them. No other compatibility code.

### One control per board

The Search view's **Cache IRC** switch is the only switch. On means "keep IRC
messages on this computer": the hub stores, restores and indexes them. Off
erases the store (as it does for every board) and IRC keeps lines in memory
only, as before this change. **Clear cache…** beside it deletes every stored
IRC message. There is no second "keep history" switch in IRC's settings: two
switches over one store would let them disagree (history kept but not
searchable is the same disk data), and the store already has the retention,
quota and clear controls a history setting would need. IRC's Settings page
says where the control is.

A board declares `Dashboard::STORE_BY_DEFAULT`. IRC sets it, so its source
starts on, as the owner asked for history. Other boards keep the opt-in
default; this change does not alter them.

### Retention, eviction and quota

Each board has its own database and policy, so IRC messages are only evicted
by IRC's own limits, never by another board's data. A message's `refreshed_at`
is its server time (clamped to now), so retention (60 days by default, up to
3,650) means "keep messages from the last N days" and the sweep and the quota
evict the oldest messages first, as a log would. No new retention class is
needed. The store keeps every message within those limits, not 2,000 per
buffer: search covers all of them. A restore loads the newest 2,000 lines per
buffer (`LINE_CAP`).

### Permissions and privacy

The store's directory is 0700 and its file 0600 (`files.rs`), so private
messages stay owner-only. Nothing goes into `state.json`. Message text is
never logged. Agent tools mark message text as untrusted.

## Behaviour

### What is stored

- One document per channel or private message (`PRIVMSG`, `ACTION`,
  `NOTICE`), ours included. Joins, parts, quits, nick, topic, mode and server
  lines are not stored. The server buffer is not stored.
- `id`: `msgid:<network>:<msgid>` when the server sends a `msgid`, else
  `line:<sha-256 of network, target, time, sender and text>`. Re-publishing a
  line replaces it, so live, replayed and history copies dedupe.
- `kind`: `Channel message` or `Private message`. `title`: `#channel · nick`,
  or `nick` in a private conversation. `body`: the text with mIRC formatting
  stripped. `parent_id`: the buffer (`buffer:<network>:<target>`).
  `refreshed_at`: the server time in seconds. `navigation`: network, target,
  `msgid` and time in milliseconds. `payload`: the exact line, to restore it.
- Index metadata: `project` is the network id (the network filter), and each
  GitHub pull request or issue URL in the text is an exact
  `ReferenceIdentity::github` with relation `mentioned`, so `related_items`
  for a pull request lists the IRC messages that link it.
- Writes run in order on the background executor, at most 100 documents per
  batch, about a second after new lines arrive and when the board stops.

### Restore and catch-up

- When the hub starts with the store on, it reads each buffer's newest 2,000
  stored lines before it configures the runtime, and adds them as history:
  they never notify, never count as unread and never change the badge.
- It then tells the runtime the newest stored line of each buffer (an anchor:
  `msgid` and time). On joining a channel, or after registration for a stored
  private conversation, the client asks
  `CHATHISTORY LATEST <target> msgid=<id>` (or `timestamp=<time>`) for up to
  500 lines, the server's limit permitting. With no anchor it asks
  `LATEST * 100` as before. The anchor follows every line the session sees,
  so a reconnect also asks only for what it missed.
- Missed lines arrive as `Event::Backfill` and merge into place by time,
  skipping lines held already by `msgid`, then by time, sender and text.

### Search, opening and agent tools

- A hit's excerpt now comes from the document body (`snippet` on the body
  column) rather than whichever column matched best, so a match in the title
  (`#deploys` for the query `deploy`) still shows the message, not the title
  twice. This applies to every board.
- `SEARCHABLE = true`. The scope is local: endpoint `local`, account `local`.
  The network is the `project` filter.
- Opening a result reveals the buffer's panel, scrolls to the line and selects
  it. When the line is not held, the hub asks
  `CHATHISTORY AROUND <target> msgid=<id> 50` (or `timestamp=`), merges the
  answer and then scrolls.
- `irc_read search` searches the held lines and the store (older lines),
  newest first, deduplicated by id, and still marks text as untrusted.
- Leaving a channel, removing it from the list or closing a private
  conversation deletes its stored messages (`remove_matching` by parent).

### Smaller items

- `/` and the palette find channels and conversations by name
  (`rows::search`); opening one emits `Opened` for recent items.
- Palette commands: Join channel…, Connect to / Disconnect from the selected
  network, Mark … as read, and the selected row's Leave, Remove or Close.
- **Add to chat** attaches the conversation in front with its 40 newest
  messages, marked as other people's text, capped at 16 KB.
- `diagnostic_config_path` offers `irc/config.json` to an issue report.
- Settings > IRC has a **History** note naming where the control is.
