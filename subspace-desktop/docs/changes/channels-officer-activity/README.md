# Channels officer activity

Back to the [change records](../README.md).

## Intent

Let a person see what a ship's captain is doing, turn by turn, from the
Ship tab in Channels: each model step with its settled tokens and cost,
each tool call with its full input, progress, output and errors, and the
reply and reasoning text, updating live. Nothing is redacted or cut: an
item too large to send inline is paged and loads on demand, never hidden.

## Spec

- **Entry and exit.** The Ship tab's captain card has an **Activity**
  action (`irc-inspector-activity`). It replaces the tab's body with the
  captain's turns, under **Back to Ship** (`irc-activity-back`), as a crew
  member's detail does. Switching conversation closes the turns.
- **Which officer.** The captain's registration ID is the ship's
  `ShipLive::id`, the key the Fleet API routes on; the roster nick only
  picks the ship.
- **Turns.** Newest first. A running turn is open, others closed; each
  heading opens or closes its turn and reads `Running turn · 3 steps ·
  1,234 tokens · $0.0420`. Steps show their model, finish reason, settled
  tokens and cost, or **Not settled yet**. Tool calls show their name and
  status, every input field, progress or metadata, every output part and the
  provider error with its status and response. Reply and reasoning text
  show whole.
- **Paged items.** An item the snapshot sends as paged shows its kind, name,
  status and size, with **Load in full**. The client reads every page,
  restarting if the item's `seq` changes midway, and the view shows the
  joined item whole. A newer `seq` in the snapshot offers **Load the
  latest**.
- **Untrusted text.** Text keeps its newlines and tabs; every other control
  or bidirectional formatting character shows as `⟨U+XXXX⟩`. It wraps to
  the inspector's width, scrolls with the inspector and is selectable. The
  helper (`view/inspector/activity/text.rs`) duplicates the crew detail's
  on purpose until one of the two lands.
- **What the view says.** `Some changes may be missing (the stream
  reconnected)` on a turn with reconnects; `This officer's backend isn't
  collected yet.` when the snapshot says so; `Activity isn't available on
  this gateway.` on a 404. A 400 or any other failure shows its error.
- **Memory only.** Turns and loaded items live in the open selection and go
  when it closes. Nothing is written to disk, cached or logged; activity
  types do not derive `Debug`, and the event's `Debug` names IDs and `seq`
  only.

## Event connection

The fleet keeps one event stream. While the turns are open, the Fleet
service asks for the captain's `agent-activity` events by reopening that
same `/api/v1/events` with `?agent=<id>`, and reopens it without the agent
once the view closes. No second stream is opened, as the gateway allows
eight viewers. If the gateway refuses the agent (404 or 400), the stream
reopens without it and the view reads the snapshot every five seconds;
it does the same while the fleet is not live.

`ActivityJoin` joins the snapshot with the events, one per agent:

- Events are buffered until the snapshot answers, then applied when their
  `seq` is above the snapshot's cursor. Events at or below the cursor are
  dropped without a refetch. A skipped `seq` is not a gap.
- The snapshot is fetched again, one fetch at a time, when the stream
  closes or reopens, the generation changes, an event's `seq` does not rise
  above the last one, or an item names a turn the join does not hold.
- **Join order.** The snapshot is not fetched until the `?agent=` stream
  has answered, including for refetches after a reconnect, so no event
  falls between the snapshot and the subscription.
- **Per-agent `seq`.** The cursor and last `seq` are kept per agent.

## Delivery and follow-up

- Contract: Stargate's officer activity API (`docs/api.md`, *Officer
  activity*, and `types.ts`) as drafted at `bfbf51b`.
- Fixture tests cover client decoding, the page join with a `seq` change
  midway, the snapshot and event join without gap or duplicate, refetch on a
  gap and a generation change, join order, 404 and `collected: false`, the
  single stream reopening with and without the agent, and a GPUI test that
  opens Activity from the captain card, reads the turns and goes Back, plus
  one that scrolls to the end of a long tool output.
- Not in this change: opening the turns from the status line, crew
  activity, and **View diff**.

## Board checklist: `irc`

- **Registration and layout:** no new Space, dock or process; the turns sit
  in the existing inspector's Ship tab, built from board-kit sections,
  action buttons and selectable plain text.
- **Settings and refresh:** no new settings or credentials. The Fleet
  service owns the connection; the view subscribes only while open and polls
  only without a live stream.
- **Storage, search and chat:** content stays in memory; it is not history,
  searchable or agent context.
- **Tools and notifications:** none added.
- **Navigation and diagnostics:** errors name the failure, never the
  content.
