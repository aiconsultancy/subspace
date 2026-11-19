# Spec: drag selection across the IRC transcript

## Behaviour

- A drag from any point in one transcript row to any point in another
  selects the text of both rows and of every row between. That covers
  messages of every kind (Markdown, plain, mIRC-coded, notices and actions),
  server lines and membership lines.
- Copy (the platform shortcut or a row's **Copy** item) gives that text in
  reading order, whichever way the drag went. Rows are separated by a blank
  line, Kit's separator.
- Message sender headers, times and day pills are not copied. Names within
  compact server and topic lines are copied with their text.
- Plain and mIRC-coded text shows exactly as sent. Markdown characters in it
  stay as typed, as before.
- mIRC bold, italic, underline and strikethrough show as before. Monospace
  is now inline code, with the code background and size. An mIRC colour
  shows as a tint of its theme tone behind the text, at 16 % opacity. White
  and black (which follow the text colour) and the extended colours add
  none.
- Links open through the shared link policy. `#channel` and `@member`
  references open through the board. A row that mentions us keeps its fill.
- Muted rows (notices, server information, membership lines) and error rows
  keep their tone. A notice written in Markdown is now muted too, as plain
  notices already were; before, its text view ignored the row's tone.
- Every row has the shared **Copy** / **Add to chat** menu, which copies the
  selection, or the row's whole text when nothing is selected.

## Design

- `spaces/irc/src/text.rs`: `selectable` draws a row's text as one
  `rich::rich_text_prepared` view inside `rich::text_menu`, tinted with
  `rich::tinted` when its tone is not the body tone. `literal` builds the
  literal Markdown with the theme's tints.
- `spaces/irc/src/markup/markdown/literal.rs` (pure): `Styled` text to
  literal Markdown. It splits pieces at runs, links, references and line
  breaks, escapes text with `inline::label`, writes code spans, links and
  tags (`literal/shown.rs`), and wraps a colour in `<mark>`.
- `spaces/irc/src/style.rs`: `RunStyle` loses the `link` and `reference`
  flags and runs are no longer split at links, as only the old renderer read
  them. A compact line's sender run is bold without a tone.
- `framework/board-kit/dashboard/src/rich.rs`: `text_view_style` (the
  theme's rich text colours with the app's link and code colours) and
  `tinted`. The host's `sync_rich_text` uses `text_view_style`.
- `framework/host/app/src/capture.rs`: a debug-only
  `drag:<x1>,<y1>,<x2>,<y2>` capture step for the evidence.

## Out of scope

- Message sender headers, times and day pills in copied text, and drags past rows that
  `MessageScroller` has unmounted. Neither can be done without Kit
  internals; see [findings](findings.md).
- `clients/irc` mIRC parsing is unchanged.
