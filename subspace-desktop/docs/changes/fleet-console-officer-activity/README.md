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
  in full and starts again from page 0 when `seq` or the page count changes
  between pages; after eight restarts it reports that the item kept
  changing. Nothing is ever truncated.
- **One stream.** The gateway allows eight event-stream viewers in total.
  `src/live.js` `setAgents(ids)` re-opens the fleet's one stream with
  `?agent=` (quietly, keeping `Last-Event-ID`) and `onOpen(agents)` runs
  after each open. `src/fleet-manager.js` `watchActivity(fleet, agent)`
  sets the agent and hands its `agent-activity` events to the view instead
  of the fleet reducer; `stop()` re-opens without it. A 400 or 404 on the
  subscribed stream drops the agent, restores the plain stream and shows the
  view as unavailable.
- **Join.** `src/activity.js` `createActivityWatcher` buffers events until
  the snapshot arrives, and reads the snapshot only once the stream with
  `?agent=` has answered, because the gateway registers the watch when the
  events response starts. Events with `seq` at or below the snapshot cursor
  are dropped. A `seq` at or below the last applied one, a new
  `generation`, an item for an unknown turn and every stream (re)open read
  the snapshot again, single-flight. `seq` may skip; a skip is not a gap.
  Turn events upsert the turn and keep its items; item events upsert by ID,
  keeping the higher `seq`. A loaded paged item is read again when its
  `seq` rises.
- **View.** `src/activity-view.js` owns the inspector while open, like the
  fleet form. Turns are newest first, the running one open; steps show
  model, settled tokens and cost; tool calls show name, full input,
  progress (`metadata`), full output (`content`) and errors; text and
  reasoning show in full. DOM is built with `createElement` and text nodes
  only, never `innerHTML`; attributes are an allow-list. Control and bidi
  characters other than newline and tab show as `⟨U+XXXX⟩`. Unchanged turns
  and items keep their nodes across updates, so a selection survives.
- **Messages.** "Some changes may be missing (the stream reconnected)" when
  `turn.reconnects > 0`; "This officer's backend isn't collected yet." when
  `collected` is false; "Activity isn't available on this gateway." on 404.
- **Memory only.** Activity is never written to storage or logged, and is
  dropped when the view closes.
- **Registration ID.** The view takes `fleet.admiral.id` and
  `ship.captain.id`, the officers' configured registration IDs.
- **Sample mode.** `createSampleActivitySim` invents turns in the contract's
  shapes: a paged 1400-line log, a failed tool with markup, an escape
  sequence and a bidi override, a reconnect, and a running turn advanced
  every 900 ms. Only OpenCode officers are collected, as on the gateway.

## Contract assumptions

Stargate #355 (`V1AgentActivity`, snapshot and pages) and STAR-6 PR 2
(`agent-activity` events, `docs/api.md` "Live activity events"):

- Event data is `{agentId, turnId, generation, seq, at, change}` with
  `change.type` `turn-started`, `turn-updated` or `turn-ended` (`{turn}`
  without items) or `item` (`{item}`, possibly a paged placeholder).
  Changes are coalesced and `seq` may skip; the server closes the stream
  rather than dropping events.
- Open the stream with `?agent=` and wait for it to answer before reading
  the snapshot, on every (re)open.
- `seq` is per agent. Each watcher follows one agent, so its cursor and
  last applied `seq` are that agent's.

## Checks

`mise run test` in `web-spaces/fleet-console`: `test/activity.test.js`
covers decoding fixture snapshots and events, page joins with a `seq`
change, the buffered join with no gap or duplicate, refetches on a falling
`seq`, a new `generation` and a re-open, single-flight refetch, join order,
404, `collected: false`, a refused subscription, the `agent` parameters,
the one stream re-opening, event routing, rendering that never parses HTML
and marks hidden characters, keyed updates and the sample officer.
