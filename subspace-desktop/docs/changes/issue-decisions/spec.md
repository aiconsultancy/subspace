# Issue decisions behaviour

Back to the [change record](intent.md).

## Tabs and paging

- Each issue starts on **Comments**. **All** shows logged decision events
  using the timeline's supplied references. An event with no reference says
  “Decision logged, details unavailable” and offers no opener or per-entry
  read. The **Decisions** tab has no composer.
- The first **Decisions** visit reads a page of 50 using the issue identifier.
  **Show more** follows the opaque cursor, preserving API order and
  deduplicating by ID. A failed continuation offers **Retry** without clearing
  existing rows. Issue refresh reads a fresh cursor chain to the loaded depth,
  including any requested continuation, or to its end. It replaces the rows
  only after success. Offline, **Show more** can reconnect a continuation but
  is disabled if another read is paused; **Retry** resumes that read. The tab
  shows busy, not a count.
- Loading shows a skeleton. Empty reads say “No decisions linked to this
  issue”. API refusal or a missing list endpoint retires the rows and shows
  “You don’t have access to decisions for this issue.” without Retry. A
  connection failure or a sign-in problem remains retryable.

## Panel and focus

- Either opener selects one decision in the right-hand panel, using the
  attachment panel's width clamp. At 700 px, the tabs wrap below the Activity
  heading. The issue page remains visible but does not accept pointer clicks
  behind the panel.
- Use the native tab controls with Left/Right and Enter/Space. Kit's tab
  rendering already supplies selection semantics and pointer activation;
  a local adapter supplies keyboard focus without a Framework change.
- Close receives focus on open. Tab and Shift+Tab stay in the decision panel,
  and each control they reach scrolls the panel body just far enough to show
  it. The arrow keys, Page Up, Page Down, Home and End scroll the body from
  any control, so reasoning without links can be read by keyboard. The issue
  page behind the panel is hidden from assistive technology, and a control
  there that takes focus another way hands it back to **Close decision**;
  focus that leaves the board, such as to the command palette, is left alone.
  Closing returns focus to the opener if still present, otherwise the board.
  Escape closes a text attachment viewer first, then the decision panel,
  then follows the issue's Back destination. The panel's own Back arrow
  steps through up to eight decisions. Escape closes the whole panel, not one
  stack entry. Opening a linked issue closes the panel and preserves the
  first issue's Back destination.
- The panel shows status, project, decided-on date, tags, Markdown reasoning,
  provenance, evidence, supersession and linked issues. Empty reasoning
  fields say “Not recorded”. Stale supersession references appear as text.
  The evidence link shows the URL's host and announces that it opens the
  evidence (`links::external_marked_with_label`). A missing detail (404) has
  Close but no Retry; all other failures, including 400, allow Retry.
- The account and issue scope fence every read. The list has an additional
  epoch for paging and refresh; the detail has a generation per selection
  so even closing and reopening the same ID drops the previous reply.

## API limits

| Limit | Desktop behaviour or follow-up |
| --- | --- |
| Order is by decided-on date, newest first | Keep API order. The row's accessible description says so. |
| A person maker has no display name in the projection | Show “Made by a person”; do not substitute the recorder or expose an ID. A named maker needs an API projection. |
| No total | Show no count on the tab. |
| Accepter and rejecter are IDs only | Show acceptance or rejection times, not those IDs. |
| Unlinking a decision logs no issue activity | **All** can still show a historical event after unlinking. **Decisions** is authoritative for current links. |

The recorder's `kind` follows the client's other user projections: the
client retains unknown strings and adds “(agent)” only for `agent`.
Decision payloads do not expose free text through Rust `Debug`.
