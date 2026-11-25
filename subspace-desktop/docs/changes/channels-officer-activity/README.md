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
  member's detail does. Switching conversation, hiding the inspector,
  showing another inspector tab, the ship leaving the live fleet and the
  channel's fleet changing all close the turns.
- **Which officer.** The captain's registration ID is the ship's
  `ShipLive::id`, the key the Fleet API routes on; the roster nick only
  picks the ship.
- **Turns.** Newest first. A running turn is open, others closed; each
  heading opens or closes its turn and reads `Running turn · 3 steps ·
  1,234 tokens · $0.0420`. Steps show their model, finish reason, settled
  tokens and cost, or **Not settled yet**. Tool calls show their name and
  status, every input field as parsed (**Input**) and the input as the
  model streamed it (**Input as streamed**), which can differ, progress or
  metadata, every output part and the provider error with its status and
  response. Reply and reasoning text show whole. An item kind or output
  part this version does not know shows as its indented JSON, never
  dropped.
- **Paged items.** An item the snapshot sends as paged shows its kind, name,
  status and size, with **Load in full**. The client reads every page,
  restarting if the item's `seq` changes midway, and the view shows the
  joined item whole. A newer `seq` in the snapshot offers **Load the
  latest**. Page counts and sizes are checked before anything is
  allocated: a size the pages could not carry, or a joined length other
  than the declared one, is an invalid answer. Loaded items are kept up
  to 64 MiB per view, the least recently opened going first (it shows
  **Load in full** again), and go when their turn leaves the snapshot.
- **Long text in parts.** A text over 256 KiB shows one 64 KiB part at a
  time, cut on character boundaries, with **First part**, **Previous
  part**, **Next part**, **Last part** and `Part k of N`, so every byte,
  the last included, is in reach.
- **Untrusted text.** Text keeps its newlines and tabs; every other control
  character (Cc), bidirectional formatting character (U+061C, U+200E,
  U+200F, U+202A–U+202E, U+2066–U+2069), line or paragraph separator
  (U+2028, U+2029) and byte order mark (U+FEFF) shows as `⟨U+XXXX⟩`, the
  same set as the Fleet Console. Copy and Select all take the text as
  shown, markers included, as copying from the console does. It wraps to
  the inspector's width, scrolls with the inspector and is selectable. The
  helper (`view/inspector/activity/text.rs`) duplicates the crew detail's
  on purpose until one of the two lands.
- **What the view says.** `Some changes may be missing (the stream
  reconnected)` on a turn with reconnects; `This officer's backend isn't
  collected yet.` when the snapshot says so; `Activity isn't available on
  this gateway.` on a 404, with no retry. Any other failure keeps the last
  turns, shows its error with **Try again** (`irc-activity-retry`), and is
  retried after 1, 2, 4 … seconds up to 30, or after Stargate's
  `Retry-After` up to the same cap.
- **Not chat context.** Every text in the view uses the chat-free menu
  (**Copy** and **Select all**, no **Add to chat**), per SUB decision
  `80e3cc19`. The title bar's Add to chat and the palette take Channels'
  agent context, which is the conversation's recent messages only, so no
  activity text reaches the chat.
- **Memory only.** Turns and loaded items live in the open selection and go
  when it closes. Nothing is written to disk, cached or logged; activity
  types do not derive `Debug`, and the event's `Debug` names IDs and `seq`
  only.

## Event connection

The fleet keeps one event stream. While the turns are open, the Fleet
service asks for the captain's `agent-activity` events by reopening that
same `/api/v1/events` with `?agent=<id>`. Once the view closes, its watch
handle goes and the service's next check (every 250 ms) reopens the
stream without the agent. No second stream is opened. The query carries
at most eight officers; a ninth, or any officer the gateway refuses (404
or 400), is treated as refused: the view says so and reads the snapshot
every five seconds. Only that explicit fallback, or a fleet the service
reads on a timer without a stream, polls: a stream that is connecting or
reconnecting is waited for. Each officer's kept events are capped at 512
events and 8 MiB.

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
- **Failure.** A failed fetch keeps the last snapshot and its error, and
  applies no event until a good fetch. Events wait in a buffer of at most
  512 events and 8 MiB; on overflow the buffer is dropped and the next
  snapshot covers them. After a good fetch the buffered events above the
  cursor are applied. One retry runs at a time, on the backoff above, at
  once on a new stream connection, or on **Try again**.
- **Cancellation.** Reading a paged item checks a cancel flag before every
  page. Closing the view, hiding the inspector, and evicting or pruning
  the item set it, so no page is asked for afterwards and no answer is
  shown.

## Delivery and follow-up

- Contract: Stargate's officer activity API (`docs/api.md`, *Officer
  activity*, and `types.ts`) at wire revision `982489e`. Optional fields
  the client does not know yet, such as STAR-7's coming `session`, still
  decode.
- Fixture tests cover client decoding, the page join with a `seq` change
  midway, the snapshot and event join without gap or duplicate, refetch on a
  gap and a generation change, join order, 404 and `collected: false`, the
  single stream reopening with and without the agent, and a GPUI test that
  opens Activity from the captain card, reads the turns and goes Back, plus
  one that scrolls to the end of a long tool output. Round 1 of review
  added: a failed first snapshot reconciled by the next without reopening;
  a stale snapshot holding a later event until **Try again**; backoff and
  `Retry-After`; the buffer bounds; the snapshot waiting for the stream's
  headers; a page count of a million refused before allocating; cancelling
  between pages; a ninth officer polled; the per-officer byte cap; the
  stream dropping `?agent=` when the last watch goes; and GPUI tests for
  **Try again**, both inputs, control and bidi markers, unknown shapes as
  JSON, the chat-free menu, closing on hide, tab and the ship leaving,
  **Load in full** to **Load the latest**, parts reaching the last bytes,
  an item released with its turn, and a closed view reading nothing.
- Not in this change: opening the turns from the status line, crew
  activity, and **View diff**.

## Board checklist: `irc`

- **Registration and layout:** no new Space, dock or process; the turns sit
  in the existing inspector's Ship tab, built from board-kit sections,
  action buttons and selectable plain text.
- **Settings and refresh:** no new settings or credentials. The Fleet
  service owns the connection; the view subscribes only while open and polls
  only on the explicit fallback.
- **Storage, search and chat:** content stays in memory; it is not history,
  searchable or agent context, and its menus offer no **Add to chat**.
- **Tools and notifications:** none added.
- **Navigation and diagnostics:** errors name the failure, never the
  content.
