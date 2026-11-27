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
  newer?}`. Each part is charged with the window it draws (`drawn(part)`,
  one more copy by default; SUB decisions `6f5e67dd`, `1c19f413` §1). The
  activity view draws one window of a part, so it charges the part plus
  at most 64 KiB of shown bytes, its markers at 12 bytes: a settled
  256 KiB part holds 320 KiB, not 512. Before each read the pager
  reserves the incoming part's raw size (`partBytes`) beside the outgoing
  part and its window, which stay on show. That outgoing charge is
  temporary: when the reply arrives it is let go, and the key holds only
  the new part and its window, charged then (`drawn(part)`), not before
  the read. A failed read keeps the outgoing part and its window, charged
  as before, with the error and **Try again**. A pager opened in place of
  an item read whole (`known.carried`) holds that item's window on show the
  same way until its first part arrives. The pager decides nothing about
  the limit: the budget does, and a reservation it refuses evicts the
  pager. Every read is fenced by a revision: a
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
  the compact JSON of the turns held. It is pinned: never freed. Each
  turn's own fields are measured once per turn object and each item once
  per item object (`turnSizer`), so an event on a long running turn
  measures only that turn's fields and the item that changed, never the
  whole turn again. A reservation fits only beside the pinned entries.
  Room is made before each read by evicting the least recently opened item
  read; an evicted item offers **Load in full** again. Eviction forgets the
  client's copy only: the item stays on the gateway, whose retention of the
  running families is its own (STAR `465b440e`), and is read again on
  request. An item read whole is charged twice its length (the item and
  its drawn copy), its base; one that does not fit so beside the snapshot
  is read in parts. **Show** on it adds what its window costs beyond that
  base to the same key (`drawWhole`, SUB decision `533c2b29`), and gives it
  back when another body is asked for.
- **Refit** (SUB decisions `533c2b29`, `a5275d81`, `8346cf82`,
  `fbc47f97`). An item read whole that no longer fits reads in parts
  instead: of the version it read when **Show** cannot keep it and its
  window beside the pinned content or the inline snapshot grows, and of
  the newest version, from its first part, when **Load the latest** or a
  read larger than its placeholder advertised cannot hold that version
  whole. Before the inline charge grows, the watcher plans every refit
  first (`planRefits`): it asks the budget what the larger charge would
  free (`wouldFree`, with `as` naming the planned refits' handover
  charges, so nothing changes while it asks) and, while that names an
  item read whole whose window is on show, plans, from every item read
  whole whose window is on show, named or not, the one whose refit gives
  back the most, then asks again. The latest request is never one of
  them: it reads in parts only when it no longer fits itself, never to
  make room for another. A refit that gives nothing back is not
  planned, and one that keeps no window on show is dropped: the growth
  frees that refit itself, or frees as many without it. Only then are the planned pagers opened,
  their charges set without freeing anything (`fitted`), and the one
  pinned reservation frees only what the final floor still needs. So one
  large refit keeps the smaller wholes on show, also a small one the
  growth would free first, and every window stays when the handovers fit.
  One not on show is freed as before, so no item read whole is kept for
  good. Its window stays on show and charged (`carried`) until the first
  part arrives, and stays with the error and **Try again** if that read
  fails. The part opened is the one that holds the start of the window on
  show: the view reports where that window starts in the item's document,
  the watcher maps it to the item as served (`servedUnit`), pages that the
  way the gateway does (`servedPart`, 64 KiB pages between code points,
  four to a part) and opens the part at the window holding it. A reload
  reads the newest version's first part instead, also once it has failed:
  the whole entry keeps that intent (`fresh`) until a read of the newest
  version succeeds, so **Try again** after the handover reads page 0 of
  the newest version, unpinned. A window the view
  unmounts (its turn closed, the items window moved off it) owns no bytes:
  a later refit carries none for it, though the place it showed is kept.
- **Floor** (SUB decisions `0c13b784` §4, `8357ec55`, `1c19f413` §1). The
  budget alone owns the floor. `request(key)` names the latest request, the
  one thing last asked for: a **Show**, a **Load** or another part. The
  pinned entries and the latest request are the only ones that may take
  the total past the limit, and neither is freed for another entry; every
  other reservation fits beside them (`fits(key, bytes)`) or is refused. A
  new request ends the last one's claim, so a **Show** after a **Load**
  frees the read and a **Load** after a **Show** sends that body back to
  its caption: at the floor the view holds the inline content, one
  incoming part (at most 256 KiB) and one drawn window (at most 64 KiB
  shown), never two windows. While that part is read, the window it
  replaces is the drawn one: it stays charged in the same key until the
  part arrives, and the part's own window is charged only then.
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
  default; within a turn, top to bottom. A part's body costs nothing more:
  its read reserved the window it draws, so a short final part heavy in
  markers draws as soon as it arrives. The plan is charged as `draw`. A body
  that does not fit shows "Not drawn, to stay within the view's memory
  budget (N bytes)." and **Show**, and its turn stays open. **Show** makes
  its body the latest request; at the floor it is charged under its own key,
  the budget frees items read that are not kept to fit it, and bodies asked
  for earlier go back to their captions. The latest asked for always
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
  Items read belong to the snapshot generation they were read in (SUB
  decision `1c19f413` §4): a snapshot of a new generation aborts every page
  read, settles its waiters and drops every item read, and a reply from the
  old generation is never shown. A snapshot read again in the same
  generation keeps every item read, part and window on show, and a refresh
  of an item in the same generation (**Load the latest**) keeps what is on
  show until the newer version arrives. `seq` is compared only within a
  generation.
  Readiness is per stream attempt: each new attempt, and each close or
  failure of the one that carried the agent, makes the view wait again for
  the next answer, and **Try again** queues until then. The wait is checked
  when a queued read starts, not when it was asked for: a retry or an
  overflow refetch queued behind a read in flight never reads before the
  new stream answers, and then reads once. A snapshot read in flight when
  the stream reopens, or when the attempt that carried the agent ends, is
  aborted and its reply dropped: it started before the new stream
  registered the watch, and one that never settles would otherwise hold
  every later read behind it, as the reads are single-flight. The client
  sets no timeout of its own on the snapshot. If a read fails
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
  progress (`metadata`), full output (`content`) and errors, then any
  fields of the item or its error that the friendly layout does not show,
  as **Other fields**; when a tool
  call has both a parsed `input` and the streamed `rawInput`, both show,
  labelled, as they can differ. Text and reasoning show in full. What is
  mounted is bounded: a closed turn mounts no items; an open turn mounts at
  most 20, with **Earlier items** and **Later items**; an item whose text is
  over one window (64 KiB or 1,000 lines) shows one window at a time with
  First, Previous, Next and Last and "Section k of N". An item read in parts
  has First part, Previous part, Next part and Last part, and "Part k of N",
  and each part shows in windows the same way. What is drawn is planned
  and charged to the view's budget first (see **Draw plan**). An unknown kind or content type, and a known kind that fails
  its checks, show as indented JSON: nothing is withheld (SUB-70). The
  short view and the windowed document share one block model for output
  parts (`partBlocks` in `src/document.js`, SUB decision `1c19f413` §5): a
  text part is its other fields (an `annotation`, say) as JSON, then its
  text; any other part is its JSON whole. What is measured is what shows. DOM is built with `createElement` and text nodes
  only, never `innerHTML`; attributes are an allow-list. Control and bidi
  characters other than newline and tab show as `⟨U+XXXX⟩`, each its own
  marker, with a run of them in one element, so a copy is exactly what
  shows; including in
  nested JSON keys and values and the backend label; a backslash typed
  before `u0000` stays escaped, so it reads differently from a marker. Unchanged turns
  and items keep their nodes across updates, so a selection survives. Each
  item row has three parts, keyed apart (SUB decision `1c19f413` §3): its
  head and foot by the placeholder's revision (seq, status, pages, bytes)
  and the read's state, its body by the body's revision. A placeholder
  going from 2 to 4 pages redraws its size, a raised seq offers **Load the
  latest**, and the body, with any selection in it, stays mounted.
- **Controller** (SUB decision `1c19f413` §6). `src/activity-controller.js`
  owns the view's lifecycle in the inspector: the subscription, the
  watcher, the panel, its redraw frame and the teardown. Every callback is
  tied to the view it was made for and does nothing once that view closed.
  Closing stops the subscription and the watcher, cancels the frame and
  disposes of the panel. `src/main.js` keeps selection, focus and the
  shell's wiring.
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

The round 5 tests are in `test/activity-round5.test.js` (SUB decision
`1c19f413`); each fails at the commit before its fix, except the check
that a snapshot of the same generation keeps what was read. They:

- charge a settled 256 KiB part at the floor as the part and one window,
  320 KiB;
- send a **Show** back to its caption on a later **Load**, and free a read
  on a later **Show**, so one window draws at the floor;
- keep the outgoing part charged only until its replacement arrives, and
  refuse a part read that is no longer the latest request;
- draw a short final part heavy in bidi markers right after its read;
- redraw a newer placeholder's status and size, offer **Load the latest**
  on a raised seq, and keep the body's nodes;
- drop items read in an old generation, and abort a whole or part read in
  flight when the generation changes;
- show a text part's `annotation` in the short view and in the document,
  and an item's, error's or file part's other fields;
- open, redraw and close the view through the controller, ignore a closed
  view's callbacks, and keep `src/main.js` under 1,000 lines.

The round 6 tests are in `test/activity-round6.test.js` (SUB decision
`533c2b29`); each fails at the commit before its fix. They:

- draw **Show** on an item read whole within the floor, freeing another
  read, or read it in parts when it cannot be kept with its window;
- read in parts an item read whole that no longer fits beside grown
  inline content, its window kept until the part arrives, even when it is
  the latest request;
- keep the part on show and charged through **Load the latest** on an item
  read in parts, swapping on arrival and keeping it with **Try again** on
  failure;
- show a step's model and token members the short view does not word, and
  an own `__proto__` field, in the short view and the windows.

The round 7 tests are in `test/activity-round7.test.js` (SUB decision
`a5275d81`); each fails at `16e6e39b` or at the commit before its fix,
except the generation control. They:

- refit an item read whole on show before inline growth would free it,
  with another item the latest request and the part held back;
- open the part, and the window in it, that hold the content on show, for
  uneven content and for **Show**'s fallback, and map the document to the
  item as served and paged;
- keep the window on show and charged while **Load the latest**, or a read
  larger than advertised, reads the newest version in parts, keeping it
  with **Try again** on failure, and still drop it at once on a new
  generation;
- charge nothing for a window unmounted by closing its turn or moving the
  items window, when its item is refitted later.

The round 8 tests are in `test/activity-round8.test.js` (SUB decision
`cf21d510`); each fails at `12f2d2e1`, except the check that a handover
whose turn closed still shows its part. They:

- read in parts every item on show that inline growth would free,
  counting the room the latest request gives back as it reads in parts;
- map a text part's other fields to where the part serves them;
- read page 0 of the newest version when the snapshot grows during
  **Load the latest**, and again on **Try again**;
- let go of the outgoing window when its turn closes during a handover,
  keeping the read and leaving nothing held when it fails;
- name nothing in `wouldFree` that `reserve` would refuse.

The round 9 tests are in `test/activity-round9.test.js` (SUB decision
`8346cf82`); each fails at `e407707c`, except the regression checks that
two refits are planned when one is not enough and that **Try again**
after growth during a reload reads the newest version. They:

- plan every refit before opening any pager, refitting the whole that
  gives back the most first, so smaller wholes stay whole and every window
  stays on show when the handovers fit (both reviewers' probes);
- ask `wouldFree` as if some entries held other sizes, changing nothing;
- read the newest version from page 0, unpinned, on a pressure handover
  after a failed whole reload and on **Try again** after it, and on
  **Try again** after growth during the reload (the regression check);
- say in the README that only growth and **Show** keep the shown part, a
  newest version too large to read whole coming under **Load the latest**;
- name the part read, and that the item is no longer held whole, when its
  turn is closed and reopened during a refit's read, without claiming a
  window stays on show;
- lower a loading pager's charge to its part when its outgoing window
  unmounts, even when the budget would refuse that part now (`shrink`).

The round 10 tests are in `test/activity-round10.test.js` (SUB decision
`fbc47f97`); each fails at `631987f1`, except the check that a refit
keeping no window is dropped, which fails at `ba82fafd`, before its fix.
They:

- refit the large whole on show when the growth names only a small one
  whose refit gives nothing back, so every window stays (Charon's probes
  A and A2);
- drop a planned refit that keeps no window, when the growth frees the
  small whole anyway;
- never refit the latest request to make room for another, though its
  refit would give back the most.

The round 11 tests (SUB decision `25e1a31a`) are in files named for what
they check. `test/activity-inline-charge.test.js` checks that a turn's
measured length equals its compact JSON for varied turns, that the inline
charge stays exact as events change the turns, and that an event measures
only the changed turn's own fields and item; it fails at `ffec732c`, which
has no `turnSizer`. `test/activity-stream-reopen.test.js` reads the
snapshot again past a read that never settles, after a re-open and after
the stream ends, and drops that read's late answer; each fails at
`b2cb71c1`. `test/activity-refit.test.js` drops a planned refit the growth
frees anyway, reserving no handover for it (Charon's probe A grown by
1,200,000 bytes); it fails at `7103c1a3`. The refit tests share the
helpers in `test/activity-helpers.js`.
