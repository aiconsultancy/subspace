# Fleet Console: officer activity

## Intent

People see everything agents see (SUB-70). Fleet Console showed what each
officer is working on, but not what it is doing: its turns, tool calls,
replies and reasoning. Stargate now keeps each officer's recent turns and
streams their changes (STAR-6). This adds an Activity view for the rear
admiral and each captain, built against the Fleet API's fixtures, with a
sample officer so it works without a gateway. Crew activity, View diff and
the desktop side are separate work.

## Spec

- **Routes.** `src/api.js` adds `activity(agent, {turn})` for
  `GET /api/v1/agents/{id}/activity` and `activityItemPage(agent, turn,
  item, page)` for its pages. `events()` sends each watched officer as a
  repeated `agent` query parameter. `readPagedItem` joins an item's pages
  in full and checks each page as it arrives, before holding more: it must
  be the agent, turn, item and page asked for, `pages` at least 1, data at
  most 64 KiB of UTF-8, `bytes` at most `pages × 64 KiB` and at most the
  console's 64 MiB item limit, and the data joined so far never past
  `bytes`, with the whole equal to it. Anything else is an invalid response
  and the read stops. It starts again from page 0 when `seq` or the page
  count changes between pages; after eight restarts it reports that the
  item kept changing. A snapshot body is capped at 128 MiB and a page body
  at 512 KiB before either is decoded. Nothing is ever truncated.
- **One stream.** The gateway allows eight event-stream viewers in total.
  `src/live.js` `setAgents(ids)` re-opens the fleet's one stream with
  `?agent=` (quietly, keeping `Last-Event-ID`) and `onOpen(agents)` runs
  after each open. `src/fleet-manager.js` `watchActivity(fleet, agent)`
  sets the agent and hands its `agent-activity` events to the view instead
  of the fleet reducer; `stop()` re-opens without it. A 400 (a malformed
  parameter, or a ninth agent) or 404 on the subscribed stream drops the
  agent and restores the plain stream; the view then reads the snapshot
  every five seconds and says "Live updates aren't available here, so turns
  refresh every few seconds.", as Channels does, until the view closes or
  a later subscription is accepted.
- **Source change.** The view's client resolves the fleet's current client
  at each read. When the fleet's address or token changes, the view aborts
  its reads, drops its snapshot and loaded items, publishes nothing read
  from the old source and waits for the new stream to answer before reading
  again; a read started earlier that answers late is ignored. When the
  fleet loses its token or is removed, the view closes.
- **Join.** `src/activity.js` `createActivityWatcher` buffers events until
  the snapshot arrives, and reads the snapshot only once the stream with
  `?agent=` has answered, because the gateway registers the watch when the
  events response starts. Events with `seq` at or below the snapshot cursor
  are dropped. A `seq` at or below the last applied one, a new
  `generation`, an item for an unknown turn and every stream (re)open read
  the snapshot again, single-flight. `seq` may skip; a skip is not a gap.
  Readiness is per stream attempt: each new attempt, and each close or
  failure of the one that carried the agent, makes the view wait again for
  the next answer, and **Try again** queues until then. If a read fails
  while a snapshot is on show, the view goes stale: it keeps the last
  snapshot, says so with the error, applies no events but keeps buffering
  them (at most 512 events or 8 MiB, as Channels; on overflow the buffer is
  dropped and the snapshot read again once), and reads again by
  itself, single-flight, after 1, 2, 4 … seconds up to 30, or the
  gateway's `Retry-After` up to 30. After a good read the buffered events
  above its `cursor.seq` are applied. **Try again** stays available. A 404
  is "unavailable" and is not retried. Closing the view aborts the read in
  flight, and a queued rerun checks for that before it issues a request.
  Turn events upsert the turn and keep its items; a turn the view does not
  hold is placed by `startedAt`, so a late `turn-ended` for an evicted turn
  falls off again rather than showing as the newest. Item events upsert by
  ID, keeping the higher `seq`; an item that fails its checks is kept as
  unreadable and shown as its JSON. A loaded paged item is not read again
  by itself when its `seq` rises: the view says a newer version is on the
  gateway and offers **Load the latest**, as Channels does, so a running
  tool is not downloaded back to back. One item is read at a time per view. Loaded items take at
  most 64 MiB per view: past that the least recently opened is freed and
  shows **Load in full** again. Loaded items go when their turn leaves the
  snapshot, and when the officer or fleet changes.
- **View.** `src/activity-view.js` owns the inspector while open, like the
  fleet form. Turns are newest first, the running one open; steps show
  model, settled tokens and cost; tool calls show name, input (both the
  parsed `input` and the streamed `rawInput` when both came),
  progress (`metadata`), full output (`content`) and errors; when a tool
  call has both a parsed `input` and the streamed `rawInput`, both show,
  labelled, as they can differ. Text and reasoning show in full. A loaded
  item over 256 KiB shows in 64 KiB parts, split between code points, with
  First, Previous, Next and Last and "Part k of N"; only the current part is
  in the page. An unknown kind or content type, and a known kind that fails
  its checks, show as indented JSON: nothing is withheld (SUB-70). DOM is built with `createElement` and text nodes
  only, never `innerHTML`; attributes are an allow-list. Control and bidi
  characters other than newline and tab show as `⟨U+XXXX⟩`, including in
  nested JSON keys and values and the backend label; a backslash typed
  before `u0000` stays escaped, so it reads differently from a marker. Unchanged turns
  and items keep their nodes across updates, so a selection survives.
- **Messages.** "Some changes may be missing (the stream reconnected)" when
  `turn.reconnects > 0`; "This officer's backend isn't collected yet." when
  `collected` is false; "Activity isn't available on this gateway." on 404.
- **Memory only.** Activity is never written to storage or logged, and is
  dropped when the view closes.
- **Chat-free** (SUB `80e3cc19`). Activity text never reaches chat or an
  agent's context. The page's `setContext` value stays the crew-tier
  summary built from fleet state alone, and opening activity does not
  change it. The host offers **Add to chat** only in its native text menu;
  a web Space's page is a native web view with the platform's own text
  menu, so there is no Add to chat on a selection in activity. Copy and
  select work as before.
- **Registration ID.** The view takes `fleet.admiral.id` and
  `ship.captain.id`, the officers' configured registration IDs.
- **Sample mode.** `createSampleActivitySim` invents turns in the contract's
  shapes: a paged 1400-line log, a failed tool with markup, an escape
  sequence and a bidi override, a reconnect, and a running turn advanced
  every 900 ms. Only OpenCode officers are collected, as on the gateway.

## Contract assumptions

Stargate wire revision `982489e`: `V1AgentActivity` (snapshot and pages)
and `agent-activity` events (`docs/api.md` "Live activity events"):

- Event data is `{agentId, turnId, generation, seq, at, change}` with
  `change.type` `turn-started`, `turn-updated` or `turn-ended` (`{turn}`
  without items) or `item` (`{item}`, possibly a paged placeholder).
  Changes are coalesced and `seq` may skip; the server closes the stream
  rather than dropping events.
- Open the stream with `?agent=` and wait for it to answer before reading
  the snapshot, on every (re)open.
- `seq` is per agent. Each watcher follows one agent, so its cursor and
  last applied `seq` are that agent's.
- A page is `{agent, turn, item, seq, page, pages, bytes, data}` with at
  most 64 KiB of UTF-8 data. The `?agent=` stream answers 400 for a
  malformed parameter or a ninth agent, 404 for an unknown one and 503 when
  no place is free; snapshot reads answer 503 when busy.
- A turn may carry `session {id, parent}`, and the agent and items may carry
  `unavailable`; child turns may appear. These decode and show; fields this
  console does not use are ignored.

## Checks

`mise run test` in `web-spaces/fleet-console`: `test/activity.test.js`
covers decoding fixture snapshots and events, page joins with a `seq`
change, the buffered join with no gap or duplicate, refetches on a falling
`seq`, a new `generation` and a re-open, single-flight refetch, join order,
404, `collected: false`, a refused subscription polling with fake timers,
the `agent` parameters, the one stream re-opening, event routing, rendering
that never parses HTML and marks hidden characters, keyed updates and the
sample officer. The round-1 tests switch the fleet's token while a snapshot
and a page read are held (nothing old is published, no later request uses
the old token, removing the token closes the view); refuse a page claiming
`pages: 1000000, bytes: 60000000000` after one read and cap a page body;
reach a sentinel on the final page through the parts; evict the least
recently opened item and release items whose turn left; show a `rawInput`
sentinel absent from `input`, inline and paged; hold Try again until a new
stream attempt answers; recover from a snapshot 503 with no click and no
new stream, honouring and capping `Retry-After`; stop a queued rerun after
close; mark a nested NUL, a bidi control and a typed `\u0000`; leave the
chat context unchanged; show invalid and unknown items as JSON; and decode
`session`, `unavailable` and child turns. The phase 1b tests show a
`rawInput`-only marker in a paged item's document and parts; follow a token
change with the next read and close the view when the fleet is removed;
read a loaded item once, not on each of ten `seq` rises, then once more on
Load the latest; refetch exactly once on a byte or count overflow of the
event buffer; keep a malformed item event as unreadable; and place turn
events for turns the view does not hold by `startedAt`.
