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
  repeated `agent` query parameter. Every page is checked as it arrives,
  before the next is asked for: it must be the agent, turn, item and page
  asked for, `pages` at least 1, data at most 64 KiB of UTF-8 and non-empty
  in a multi-page item, and `bytes` between the minimum its pages need
  (`pages ≤ max(1, bytes)`) and `pages × 64 KiB`. The data joined so far
  must fit within `bytes`, with the whole equal to it. Invalid responses stop
  the read. A snapshot body is capped at 128 MiB and a page body at 512 KiB
  before either is decoded. Nothing is ever truncated.
- **Paged items** (SUB decision `a17870f4`). `readPagedItem` reads an item
  whole. After page 0 it refuses an item over `MAX_ITEM_BYTES` (64 MiB), or
  one its view cannot make room for (`room(bytes)`), with `TooLargeError`
  before page 1 is asked for; no read is unbounded, even without a `room`.
  The joined JSON must be an object with the ID asked for. A page of another
  `seq`, `bytes` or page count belongs to another version, so the read starts
  again from page 0, at most five reads in all, as Channels
  (`PAGE_ATTEMPTS`); then it says the item kept changing, and **Try again**
  reads it again. An item too large to read whole is read and shown a part
  at a time instead (`readItemPart`: four pages, at most 256 KiB, pinned to
  one version; a newer version restarts once from its first part and says
  so). Any part can be reached directly by page number.
- **Pager.** `src/pager.js` `createPager({budget, key, read, partBytes,
  drawn?})` shows one part of anything read part by part. `read({part,
  cursor, version, signal})` answers `{data, next, version, count?,
  newer?}`. Each part is charged with its drawn copy (`drawn(bytes)`, one
  more copy by default; SUB decision `6f5e67dd`). Room for the part and its
  drawn copy is reserved before each read, beside the part on show and its
  copy, and only the part shown and its copy stay charged. These are forced
  reservations: the part last asked for is the floor. Every read is fenced by a revision: a
  reply that arrives after an eviction, a restart, a drop or another read is
  dropped. An evicted pager reads again from its first part with no cursor,
  so the next read may have different page boundaries. A reply that holds
  more than `partBytes`, has no data, or does not advance is a failure.
  `splitBounded(text, {maxBytes, maxLines})` splits text into windows
  between code points.
- **Budget.** `src/budget.js` is one view's byte budget (64 MiB). It is a
  cache budget only (SUB `20ce0349`): larger content is paged, never hidden
  or cut. The budget is shared by the inline snapshot, the items read and
  what the panel draws. The inline snapshot is charged first as `inline`,
  the compact JSON of the turns held. It is pinned: never freed, counted
  once per turn object. A reservation fits only beside the pinned entries.
  Room is made before each read by evicting the least recently opened item
  read; an evicted item offers **Load in full** again. An item read whole
  is charged twice its length (the item and its drawn copy); one that does
  not fit so beside the snapshot is read in parts. `freeOldest(skip)` frees
  one entry for the panel's **Show**.
- **Floor** (SUB decision `0c13b784` §4). The view goes past its budget
  only by its floor: the inline content, plus one part or window (at most
  256 KiB read, one 64 KiB window drawn) of the item last asked for, plus
  its drawn copy. Pinned and forced reservations are the only ones that may
  exceed the limit.
- **Draw plan** (SUB decisions `ed71a299`, `6f5e67dd`, `0c13b784`). Each
  frame, the panel measures every body of every open turn before building
  any. A body is measured once per version (`sourceVersion`: generation and
  seq; read seq and length; or part, version and pages), as numbers only,
  from its pieces (`src/document.js`), so its text is never built whole.
  Only the window drawn is built. Sizes are the visible UTF-8 bytes: each
  hidden character is a 12-byte `⟨U+XXXX⟩` marker, and no window splits
  one. `planBodies` fits bodies to the room the snapshot and items read
  leave, in this order: asked for with **Show** (latest first), then turns
  opened by hand (opened last first), then the running turn open by
  default; within a turn, top to bottom. A body whose read already reserved
  its drawn copy costs nothing more. The plan is charged as `draw`. A body
  that does not fit shows "Not drawn, to stay within the view's memory
  budget (N bytes)." and **Show**, and its turn stays open. **Show** frees
  items read (least recently opened first, never its own), then forgets
  bodies asked for earlier, until it fits. The latest asked for always
  draws, a window at a time, with First, Previous, Next and Last. Closing a
  turn forgets its asks. Measures and windows are forgotten when their
  version is no longer shown: a new seq, an evicted or replaced read, or
  another part.
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
  the snapshot again, single-flight, as does an item event with no `id`.
  `seq` may skip; a skip is not a gap.
  Readiness is per stream attempt: each new attempt, and each close or
  failure of the one that carried the agent, makes the view wait again for
  the next answer, and **Try again** queues until then. The wait is checked
  when a queued read starts, not when it was asked for: a retry or an
  overflow refetch queued behind a read in flight never reads before the
  new stream answers, and then reads once. If a read fails
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
  unreadable and shown as its JSON. A turn that fails its checks, such as
  one with no `startedAt`, is kept too, as an unreadable turn whose one item
  is the turn as sent. A loaded paged item is not read again
  by itself when its `seq` rises: the view says a newer version is on the
  gateway and offers **Load the latest**, as Channels does, so a running
  tool is not downloaded back to back. Page reads go one at a time per
  view. Loaded items go when their turn leaves the snapshot, and when the
  officer or fleet changes.
- **View.** `src/activity-view.js` owns the inspector while open, like the
  fleet form. Turns are newest first, the running one open; steps show
  model, settled tokens and cost; tool calls show name, input (both the
  parsed `input` and the streamed `rawInput` when both came),
  progress (`metadata`), full output (`content`) and errors; when a tool
  call has both a parsed `input` and the streamed `rawInput`, both show,
  labelled, as they can differ. Text and reasoning show in full. What is
  mounted is bounded: a closed turn mounts no items; an open turn mounts at
  most 20, with **Earlier items** and **Later items**; an item whose text is
  over one window (64 KiB or 1,000 lines) shows one window at a time with
  First, Previous, Next and Last and "Section k of N". An item read in parts
  has First part, Previous part, Next part and Last part, and "Part k of N",
  and each part shows in windows the same way. What is drawn is planned
  and charged to the view's budget first (see **Draw plan**). An unknown kind or content type, and a known kind that fails
  its checks, show as indented JSON: nothing is withheld (SUB-70). DOM is built with `createElement` and text nodes
  only, never `innerHTML`; attributes are an allow-list. Control and bidi
  characters other than newline and tab show as `⟨U+XXXX⟩`, each its own
  marker, with a run of them in one element, so a copy is exactly what
  shows; including in
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

The round 3 tests are in `test/pager.test.js` and
`test/activity-round3.test.js`; each of the latter fails at `822c411d`
except the multibyte-boundary case, which guards a valid page split. They
read an item of 1,600 pages (100 MiB) in parts, its first and last part
only; show a run of 5,000 hidden characters as one element; mount no items
in a closed turn and a bounded window of items and text in an open one;
hold a snapshot, queue a retry or an overflow refetch, then disconnect or
reset, and see no read until the new stream answers and then one; refuse
pages that together run past `bytes`; keep a turn with no `startedAt` as
unreadable; read the snapshot again on an item event with no `id`; show an
ID mismatch as failed with **Try again**; and, with a 300 KiB budget, hold
an item's second part, evict the item by reading another, release the
reply, and read the item again from its first part in order, its pages
once each, with a line split across a part boundary joined exactly once.
The pager tests cover cursors, a re-read at a new page size after
eviction, A → B → A, a newer version and malformed replies. Every fake page
reader stops itself after a few reads. The #118 tests stop at an
unexpected second read of an oversized item, and refuse a page count that
needs empty pages or an empty page in a multi-page item.

The round 4 tests are in `test/activity-round4.test.js` (SUB-69 comment
`5d0227f2`); each fails without its fix. They:

- keep a turn whose `items` is not a list as unreadable;
- measure and split a body by its bytes as shown, never splitting a marker;
- check that the measured pieces join into exactly the text shown;
- keep a window by version across a snapshot read again, measuring each
  version once;
- forget an evicted read's measure and window;
- charge and pin the inline snapshot, reading in parts what does not fit
  beside it;
- show a caption and **Show** under pressure without closing the turn;
- free the least recently opened item read on **Show**;
- draw in the order asked, opened by hand, then running;
- draw the latest body asked for a window at a time past the room;
- reserve a part's drawn copy before it is read.
