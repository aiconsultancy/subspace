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
  status and size, with **Load in full** when it and its drawn copy both
  fit the room left (SUB decision `6f5e67dd`), or **Load in parts**. No item is too large to read (SUB
  decision `a17870f4`). The client reads every page, restarting if the
  item's `seq` changes midway, and the view shows the joined item whole:
  as its kind, or, for a kind or shape this version does not know, as its
  JSON, as an inline item shows. The joined item's ID and `seq` must be
  the ones its pages carry. A newer `seq` in the snapshot offers **Load
  the latest**: what was read stays shown and charged, with its window,
  until the newer version arrives, and then gives way to it; if that read
  fails, what was read stays beside the error and **Try again**. Beside
  it the newer read counts only what it asks for: a page for a part, the
  whole item only when both fit the room. A snapshot refresh meanwhile
  never lets go of what is shown (SUB decision `084f5236` item 2). A
  snapshot of a new generation still lets go of it at once. Page counts and sizes are checked before anything is
  allocated: a size the pages could not carry, or a joined length other
  than the declared one, is an invalid answer.
- **One budget.** Everything the view holds counts against 64 MiB: the
  snapshot's inline content (estimated from its strings and values, every
  field and token count this version does not know included, drawn or not)
  and every item read, inline text counted in UTF-8 bytes. Room is made before
  each read for the item's declared size and its drawn copy, and the
  budget holds during the read as after it: a read never holds more than
  its room. An item whose page 0 claims more is read again once with room
  made for that claimed size and its drawn copy when they fit the budget
  beside every request made since, and in parts otherwise: reading it
  again is no request, so it never lets go of a newer one (SUB decision
  `084f5236` item 4). Room is made from the lowest priority up, items
  read and bodies asked for alike, the least recently asked for first (an
  item let go shows its load action again); only a person's request
  changes priority, never a read completing. Items go when
  their turn leaves the snapshot. A reply that arrives after its item was
  let go, or after it was read again, is dropped.
- **What is drawn counts too** (SUB decisions `ed71a299` and `0c13b784`).
  What the open turns draw counts against the same budget, beside the
  inline content and the items read, in the UTF-8 bytes it mounts: each
  block as drawn, every `⟨U+XXXX⟩` marker in full, or one 64 KiB part when
  it shows in parts, plus the formatted text kept for it while drawn (an
  indented JSON value, an error); an item read on demand what it shows.
  Collapsed turns build no bodies and draw nothing. Each body is measured
  once per version of its item, by turn, item, snapshot generation and
  `seq` (a `seq` is compared only within one generation), and which bodies
  are drawn is worked out again only when the snapshot, the items read,
  the turns opened or the bodies asked for change, never for a frame.
- **Draw order** (SUB decision `33ccecf6` item 6). The bodies asked for
  with **Show** come first, the latest first. Then come the open turns:
  those opened by hand first, the most recently opened first, then those
  open by default because they are running, the newest first; within each
  turn its items in order. Each is drawn while it fits the room left; one
  that would pass it shows only its fixed heading (its kind and a short
  status), its size and that it is not drawn to stay within the view's
  memory budget, with **Show**. Tool names, argument keys and values are
  its body, never its heading. **Show** first makes room, giving back any
  read's optional growth, then letting go of the lowest priority, items
  read and bodies asked for alike, the least recently asked for first,
  until what is held and drawn fits beside the body asked for. If the body still does not fit, it is drawn in parts within
  the room left, each of at most 64 KiB, with **First part**, **Previous
  part**, **Next part**, **Last part** and `Part k of N`: never hidden and
  never cut. Only the part shown is written out, from an index of where
  some parts start, made once per version of the body and charged with the
  part it draws: it keeps at most 4 KiB, its starts sparser as the body
  grows, and moving to a part walks at most the parts from the nearest
  start before it.
- **The floor** (SUB decisions `8357ec55` §2 and `33ccecf6` item 1).
  Inline content is the snapshot itself and is never let go. Only the item
  last asked for may pass the room, and only when no room is left: the view
  then holds the inline content, one part of that item at the pager's part
  size (at most 64 KiB) and its drawn window. When it is an item read,
  the floor is that read's alone: what its placeholder names draws only
  within the room left, held back when none is, with a pager of its own
  when drawn in parts beside the read's (SUB decision `084f5236` item 1).
  With any room left, every request stays within it. While the reader moves between parts, the
  outgoing part and its drawn window stay charged beside the page in flight
  until the new part arrives. A snapshot refresh never lets go of the part
  last asked for. One ledger enforces both, for admission, a reply
  arriving, a refresh and the plan of what is drawn alike: each read counts
  from admission until it is let go or replaced, within a steady
  allowance for what it holds and draws, beside a temporary allowance for
  what is coming in, and draws within it, never in the plan of the
  snapshot's bodies. When formatting makes an item read whole draw more
  than its copy, it may grow past its allowance only into room nothing
  else holds, no other read and no body asked for, letting nothing go, and
  else draws in parts within its allowance. That growth gives way to any
  newer request: it is given back before anything is let go. A snapshot
  of a new generation discards every measure, formatted text, part index
  and item read of the old one.
- **Larger than the budget.** An item that, with its drawn copy, does not
  fit the room left is read one page at a time: its JSON, one 64 KiB page
  per part, with **First part**, **Previous part**, **Next part**, **Last
  part** and `Part k of N`; a page whose markers draw more than 64 KiB
  shows in parts of its own. Only the page shown is held; while the next
  part is read, the page shown and its drawn window stay charged, with
  room for the incoming page beside them, and once it arrives that page
  and its drawn window are charged in their place. When the room made is under one page, the part
  is read anyway. Each part is pinned to the `seq`, page count and size of
  the part before, never a copy of it; when the item changed, the view
  starts again once from the newest version's first part and says a newer
  version is on the gateway. A part past the
  item's last page (the item shrank) is a change too: the first part is
  read once, and **Try again** after a failure reads the first part, never
  the old one.
- **Long text in parts.** A text drawing more than 256 KiB, markers and
  all, shows one part of at most 64 KiB drawn at a time, cut on character
  boundaries, with **First part**, **Previous
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
every five seconds. A refusal is about the officers the refused stream
asked for: one that arrives after the watched officers changed never
refuses those watched since. Only that explicit fallback, or a fleet the service
reads on a timer without a stream, polls: a stream that is connecting or
reconnecting is waited for. Each officer's kept events are capped at 512
events and 8 MiB.

`ActivityJoin` joins the snapshot with the events, one per agent:

- Events are buffered until the snapshot answers, then applied when their
  `seq` is above the snapshot's cursor. Events at or below the cursor are
  dropped without a refetch. An event at or below the cursor as fetched is
  covered: it neither refetches nor resets the `seq` later events must
  pass. Events that wait for a fetch are first matched to the snapshot it
  returns, and only those it does not cover are checked for a `seq` that
  rises, so an event covered by a snapshot still in flight never fetches
  again. A skipped `seq` is not a gap.
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
  page. Closing the view, hiding the inspector, leaving Channels, and
  evicting or pruning the item set it, so no page is asked for afterwards and no answer is
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
- Round 3 of review added: a paged item of an unknown kind and of a known
  kind in a changed shape loaded to its last bytes; a joined item with
  another ID or `seq` refused; an item over the budget read page by page
  with its first and last parts in reach and the budget never passed; a
  part restarting once on a newer version; a late reply after eviction
  dropped; leaving Channels mid-read asking for no page and dropping
  `?agent=` while another follow keeps the stream; a late 400 or 404
  refusing only the officers its stream asked for; and a covered event
  never fetching again.
- Round 4 of review added: an open turn drawing only up to the room left
  with **Show** on the rest, **Show** drawing the body asked for within
  the budget, and a collapsed turn drawing nothing; a part read holding a
  page of room until it arrives; an item that shrank starting again from
  its first part, with **Try again** reading that part; an event covered
  by the snapshot in flight never fetching again; **Load in parts** for an
  item larger than the room; and a turn ID with `:` keeping its part.
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
