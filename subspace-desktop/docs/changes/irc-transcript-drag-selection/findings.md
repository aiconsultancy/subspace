# Findings: GPUI Kit's copy order, and what option C covers (the Channels interaction improvements part B)

Kit route: option C, chosen by Jack. No upstream Kit PR, no `[patch]`, no
vendored or forked Kit: only what `gpui-base` / `gpui-kit` 0.9.1 offers.
Citations are to `~/.cargo/registry/src/index.crates.io-*/gpui-base-0.9.1/src/`.

## Summary

- **Kirk's reading is right; the earlier design note was wrong.** A
  selectable `TextView` registers *in paint order*. Text views copy in the
  order they painted, which in the transcript is reading order, and never in
  `HashMap` order.
- `SelectableText` defaults to order 0 and `InteractiveText` takes no part,
  so neither can be ordered among text views without Kit internals.
- So option C works: every transcript row's text is now a selectable
  `TextView`. A drag from any row to any other copies every message, server
  line and membership line between, in reading order.
- mIRC **colours cannot be text colours** on the `TextView` path. They are
  kept as a light `<mark>` tint of the same theme tone. Bold, italic,
  underline, strikethrough and monospace, links, references and
  highlights survive.
- Out of scope under C: sender headers, times and day pills in the copied text, and
  drags that auto-scroll far past the rows the `MessageScroller` keeps.

## Step 1: the tests and what they copied

### Kit's own behaviour (`spaces/irc/src/tests/selection/kit.rs`)

A window holds seven 32 px rows: `TextView`, `InteractiveText`,
`SelectableText`, `TextView`, `InteractiveText`, `SelectableText`,
`TextView` (`view0`, `interactive1`, `selectable2`, `view3`, `interactive4`,
`selectable5`, `view6`). Each test drags from the start of one row to the end
of another and reads `TextSelection::selected_text`.

| Drag | Copied |
| --- | --- |
| `view0` → `view6` | `view0\n\nview3\n\nview6\n`: text views only, in order |
| `selectable2` → `view6` | `selectable2`, `selectable5` (in either order), then `view3\n\nview6\n` |
| `view0` → `selectable5` | `selectable2`, `selectable5`, then `view0\n`; `view3` is left out |
| `interactive1` → `interactive4` | nothing |

### The board before option C (`spaces/irc/src/tests/selection.rs`, first run)

Run against the transcript as it was at `d7f1007`:

1. **Twelve Markdown rows**, the first six arriving last as history, so the
   older rows' views were created *after* the newer ones. Dragged top to
   bottom, and bottom to top with six rows: always `m01` … `m12` in reading
   order. A `HashMap` tie would have shuffled them.
2. **Mixed rows**: Markdown `md1`, plain `plain2`, mIRC `mirc3`, `carol
   joined`, `md5`, `plain6`, mIRC `mirc7`, `dave quit`, `md9`:
   - `md1` → `md9`: `md1\n\nmd5\n\nmd9\n`. Plain, mIRC and membership rows
     were missing.
   - `carol joined` → `md9`: `carol joined\ndave quit\nmd5\n\nmd9\n`. Both
     membership lines came first, so `dave quit` was copied before `md5`,
     out of reading order. `md1`, above the start, was not copied.
   - `plain2` → `mirc7`: nothing. A drag starting on `InteractiveText`
     selects nothing.

The tests now assert the board after option C (below). `kit.rs` keeps the
raw Kit behaviour, which the board no longer shows.

## Why: Kit's source

- **Text views number themselves in paint order.** The `TextView` selection
  adapter registers with `.in_paint_order()` (`text/selection_adapter.rs:357`).
  `in_paint_order` is `pub(crate)` (`text_selection.rs:311`).
  `register_quietly` gives a new paint-order participant the next number and
  records each paint (`text_selection.rs:1667-1699`). After a frame in which
  every paint-order participant painted, `finish_registrations` renumbers
  them 1, 2, 3… in paint order (`text_selection.rs:1587-1619`). The order of
  creation does not matter.
- **`SelectableText` is fixed at 0.** `SelectableText` starts with
  `document_order: 0` (`selectable_text.rs:50`) and registers that number
  (`selectable_text.rs:192`). `document_order(n)` is public
  (`selectable_text.rs:57`), but it sets a fixed number. Text views are
  renumbered every complete frame, from 1, so an app cannot choose a number
  that falls between two of them.
- **`InteractiveText` is not a participant.** It never registers with the
  window selection, so a drag that starts on it has no anchor.
- **Which participants a drag covers.** `participates` takes the participants
  whose number lies between the anchor's and the cursor's
  (`text_selection.rs:2611-2631`). From a text view to a text view, numbers
  1…n, so every `SelectableText` (0) is left out. With a `SelectableText` at
  one end, 0…k, so every `SelectableText` and the text views up to k take
  part. Each participant then keeps only the text between the two window
  points: `project_ranges` for plain runs (`text_selection.rs:463`), and
  `CopyProjection::project` for text views (`text/selection_adapter.rs:121`).
  That is why `md1`, above a drag's start, was not copied, and why `view3`,
  numbered after `view0`, was left out of `view0` → `selectable5`.
- **Copy order.** `copy_items` reads the participants from a `HashMap`
  (`text_selection.rs:1814`), and `resolve_copy_items` sorts them by number
  and joins them with `\n` (`text_selection.rs:675`). Text views have unique
  numbers, so their order is fixed. Every `SelectableText` ties at 0 and
  keeps the `HashMap` order. Each text view's text ends its paragraph with
  `\n`, so text views are separated by a blank line.

## Step 2: option C

Every transcript row's text is a selectable `TextView`
(`spaces/irc/src/text.rs`):

- **Markdown messages** are unchanged: the safe Markdown from the Channels Markdown change
  through `rich::rich_text_prepared`.
- **Plain and mIRC-coded messages, server lines and membership lines** become
  literal Markdown (`markup/markdown/literal.rs`), which shows the text
  exactly as sent. Every ASCII punctuation character is escaped (`<` as
  `&lt;`). Spaces and tabs at a line's start or end become `&#32;` and
  `&#9;`, which Markdown neither reads as indentation nor drops. A line break
  is a hard break. Tests parse the result with `TextView`'s parser for
  `*not bold*`, `` `x` ``, `[a](b)`, `# not a heading`, `1. not a list`,
  `<tag>`, a bare URL, `$math$`, `&amp;`, indentation, tabs and more, and
  copy plain and mIRC-coded rows in the real board.
- The board's reading of text is unchanged. Without mIRC codes, `*bold*` or
  `1. item` is still Slack-style Markdown. The literal path is for text the
  board already showed as typed.
- `markdown_view` in `rich.rs` is still the only `TextView::markdown(`
  caller: every row goes through `rich_text_prepared`. The Channels Markdown change tripwire
  test passes.

### mIRC formatting on the `TextView` path

| mIRC | On the `TextView` path |
| --- | --- |
| Bold, italic, underline, strikethrough | Kept, as `<b>`, `<i>`, `<u>`, `<s>`, which Kit's Markdown reads (`text/format/markdown.rs:216-231`) |
| Monospace | Kept, as a code span, now with the code background and size |
| Foreground colour | **Not possible as a text colour.** `TextMark` has bold, italic, strikethrough, underline, code, a highlight background and a link, with no colour (`text/node.rs:836`). Kept as a `<mark color="#rrggbbaa">` tint of the theme tone at 16 % opacity (`text/format/html.rs:164`). Kit draws marked text in black or white for contrast (`text/node.rs:2896`). White and black add no tint. |
| Background colour, reverse | Not shown before or after: the board never drew them |
| Links | Kept: URLs open through the link policy, and `#channel` and `@member` references through the board (tested by click) |

Options considered for colours:

1. **Keep coloured messages on `InteractiveText`.** The colours stay, but those
   rows silently drop out of every drag and copy. That breaks the feature's
   one promise.
2. **Drop colours.** Every row copies, but a bot's red FAIL and green PASS
   lose their meaning.
3. **Tint (chosen).** Every row copies and the colour's meaning stays, in
   the same way references were already tinted with the accent colour.

This loses least. A compact line's sender name (in the Server buffer) is bold
without its nick tone, as a tint there would read as mIRC colour. Kit's
inline Markdown plugins (`MarkdownPlugin`, `InlineElement`) might draw
coloured runs as inline objects. That was not tried: they copy as atomic
objects and may not wrap.

### Kept

- Link clicks, through the shared link policy, and reference clicks, which
  open a channel or private conversation. A headless test covers both.
- Highlight styling: a row that mentions us keeps its fill.
- References look like they did on Markdown rows: accent links. They lose
  the medium weight and tint that plain rows gave them.
- Hover from part A, and accessible names. Rows still read the text as shown.
- The per-row **Copy** menu, now on every row, Markdown rows included. It
  copies the selection, or the row's whole text.
- Muted and error tones: a `TextView` paints in its style's foreground
  (`text/text_view.rs:873`). It follows its container's text colour only
  when the app turns on `TextViewDefaults::with_inherit_text_color`
  (`text/text_view.rs:77`, `:777`), and Subspace leaves that off: turning it
  on would change every text view in the app. So `rich::tinted` sets the
  body colour per row, and the host now builds its rich text style from the
  same `rich::text_view_style`. A notice written in Markdown is now muted
  too. Before, its text view ignored the row's tone.

### What option C leaves out

- **Message sender headers, times and day pills** are labels, not text views.
  Names within compact server and topic lines are copied with their text.
  Making the headers text views would number them among
  the bodies. They would then copy as separate paragraphs, with a blank line
  after each name.
- **Long drags.** `MessageScroller` unmounts rows far off screen. A drag
  that auto-scrolls far enough loses its anchor's registration, and the
  selection is lost. A fix needs a `MessageScroller` selection adapter,
  which uses Kit internals, so it was not attempted.
- **Copy format.** Kit separates rows with a blank line. The app cannot
  change Kit's join without a custom copy callback on Kit's own handle.

## Verified where

- Linux, headless: every test above, and `make check` and `mise run quality`.
- Linux, Xvfb: the capture build shows the drag highlight and the pasted text
  (see `delivery.md`).
- Not on macOS: the ⌘C path through the macOS menu, Retina rendering of the
  tints, and italic, as the Linux fixture font has no italic face.
