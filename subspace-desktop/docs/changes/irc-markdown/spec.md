# Spec: write Markdown in IRC messages and render it like Slack

## Captain's decisions

Recorded from the Channels Markdown change brief (Voyager's captain, from Defiant's
defaults):

- **Scope:** Slack's whole set: bold, italic, strikethrough, inline code,
  fenced code blocks, block quotes, bulleted and numbered lists, and links
  (bare URLs and `[text](url)`).
- **Syntax:** Slack's single markers: `*bold*` is bold, `_italic_` italic,
  `~strike~` strikethrough. Standard `**bold**` and `~~strike~~` also work.
  Slack's markers are translated into the shared Markdown before rendering;
  there is no separate renderer. Markers format only at word boundaries, so
  `snake_case_names`, `2*3*4` and file globs stay literal. Inline code and
  code blocks are never formatted inside.
- **Safety:** no HTML; no remote content, so images stay links; links open in
  the system browser only for safe schemes, through `links::safe_url`.
- **Keep working:** mIRC formatting, clickable `#channel` and `@member`
  references, and mention highlighting.
- **Multi-line formats** (code blocks, lists, quotes) are built and tested
  now. A `draft/multiline` batch is one message. Shift+Return belongs to
  the Channels interaction improvements; the composer's key handling is unchanged.
- **Other IRC clients** see the raw characters.

## Syntax

| Written | Shown |
| --- | --- |
| `*bold*`, `**bold**` | bold |
| `_italic_` | italic |
| `~strike~`, `~~strike~~` | strikethrough |
| `` `code` `` | inline code |
| ```` ``` ```` lines around code; ```` ```code``` ````; ```` ```first line ````…```` last line``` ```` | a code block |
| ```` ```rust ```` (one word after the fence) | a code block whose language shows in its corner |
| `> text`, `>` alone | a quoted line |
| `>>>` | quotes the rest of the message |
| `- `, `* `, `• `, `+ `, `1. `, `1) ` | lists (`•` becomes `-`) |
| `https://…`, `[text](https://…)`, `[text](mailto:…)` | links |

## Precedence

1. **mIRC codes** (`markup::rich`): a message with mIRC formatting codes
   keeps the IRC presentation; its Markdown characters show as typed.
2. **Code** (`blocks.rs`, `spans::code_span`): fenced lines are code; a code
   span is a run of backticks closed by the next run of the same length on
   the line. Nothing inside is formatted, linked or referenced. One word
   after an opening fence names the language only when code follows it;
   otherwise it is the code (```` ```ls ````). The board writes every fence
   longer than any run of backticks in the message's code, so no line of
   code can close its block early and let the lines after it be read as
   Markdown.
3. **Links and URLs** (`spans::link`, `spans::url`): `[text](url)` is a link
   only when `links::safe_url` opens its URL; its text shows as typed, and
   the typed URL ends at the `)` that balances its parentheses, at most 32
   deep, as the parser allows, so each `[` scans a bounded stretch. A bare
   `http(s)` URL is a link that shows the URL as typed, without trailing
   punctuation or markers, unless it holds `<` or `>`. Nothing inside is
   formatted. The rewrite never copies a typed destination: it writes the
   URL `safe_url` returned (`spans::destination`), with `(`, `)`, `\` and `&`
   escaped with a backslash, which the parser decodes, and `<`, `>`, spaces
   and control characters percent-encoded. The link therefore opens exactly
   the URL the policy approved, brackets included, whatever escapes,
   parentheses or control characters were typed.
4. **Markers** (`emphasis.rs`): a run of one or two `*`, one `_`, or one or
   two `~` opens before text, after a space, the line's start, an opening
   bracket or quote, or another marker. It closes after text, before a space,
   punctuation or the line's end. A run between two punctuation characters
   other than markers (`(*.md)`, `"*"`, `[*]`) does neither. A closing run
   pairs with the nearest open run of the same character and length on the
   line; runs opened after that one stay literal (CommonMark's rule). Paired
   markers become CommonMark's `**`, `_` and `~~`.
5. **Everything else is literal**: unpaired markers, other runs (`__`,
   `***`, `~~~`), backslashes, `|`, `#`, `&`, `$`, `:`, `@` and the `.` of
   `www.` are escaped with a backslash and `<` is written `&lt;`; a line,
   list item text or quoted line that would start a heading, rule, setext
   underline, nested quote or list item is escaped; `![` keeps its `!` as
   text so no image loads. Lines end at `\n` and at a lone `\r`, as in
   Markdown. Indentation is spaces and tabs only, a list's lines lose the
   first item's indentation, so an indented list is not code, and an
   ordered list number has at most nine digits. A list marker with no text
   after it is a paragraph line, shown as typed: as a list item, an empty
   nested bullet's dash would underline the item above as a heading.

`:`, `@` and the `.` of `www.` are escaped because GFM autolink literals
(`https://…`, `www.…`, `ops@example.org`, `mailto:`, `xmpp:`) are found by
the parser, not the board, and run to the next space or `<`, swallowing the
syntax in between. In `` https://example.org/`<b>x</b>` `` (a URL the board
does not link, as it holds `<`) the literal took the code span's opening
backtick, and `<b>` became HTML. With those characters escaped, no literal
forms; the board links real URLs itself, and `www.` addresses and email
addresses show as text, as in plain messages. `<` is an entity rather than
`\<` for the same reason: a literal would take in the backslash. The
emitted Markdown holds no `<` outside code.

`#channel` and `@member` references and `nick:` addressing are found word by
word as before, and a word now also ends at a formatting marker, so
`*@mira*` is bold and opens a private conversation.

## Rendering

- `markup::rich(text, members)` returns `Rendered { markdown, plain }` when the
  message uses formatting, code, a quote, a list or a link, and `None` for
  plain text and mIRC text, which keep `text::formatted`. Bare URLs and
  references alone do not make a message Markdown.
- The rewritten Markdown has no HTML and no images, so the row shows it with
  the shared `rich::rich_text_prepared` (from the issue workspace implementation slice 3a), which skips
  `prepare_markdown`. `prepare_markdown` reads tags inside code as HTML and
  drops text after them; it stays for every other board. Links still go
  through the board's reference claim, then the shared link policy, and the
  image resolver still loads only data images. Slice 3a's parser guard runs
  in front of every `TextView`; because every IRC rewrite parses, it leaves
  them unchanged.
- A row's accessible name is the text as shown (`Rendered::plain`): no
  markers, link text for links, code without backticks (and, as Markdown
  shows it, less one space at each end when both ends have one).

## Edge cases tested

`spaces/irc/src/markup/markdown/tests.rs` and `markup/tests.rs`:

- Each format, and `•` lists becoming `-`.
- Literal: `snake_case_names`, `_private_var`, `__init__`, `2*3*4`,
  `ls *.rs and *.toml`, `src/*.rs`, `/usr/*/bin/*`, `**/*.rs`,
  `~/notes and ~/code`, `a * b * c`, `** spaced **`, `_ spaced _`, `*bold*s`,
  `(*.rs) and (*.md)`, `"*" or "*"`, `[*] and [*]`, `***three***`,
  `~~~tilde~~~`, `*mismatched**`, and the same escaped inside a formatted
  message.
- List items: `- ---`, `- > x`, extra spaces after the marker, an indented
  list, a nested list, and empty nested items (`- a\n  - `, a tab, three to
  five spaces, a non-breaking space only, `•`, `+` and under `1. `).
- No raw `<` reaches the Markdown for HTML, comments, CDATA and processing
  instructions in text, URLs, list items, quotes and link text.
- Closing before punctuation and opening after brackets and quotes:
  `*done*,`, `(*aside*)`, `"*quoted*"`, `*Mira*'s`, `*now*!`, `_why?_`.
- Nesting and crossing: `*_both_*`, `_*both*_`, `~*gone*~`, `*a _b* c_`,
  `*a*b*`, adjacent pairs.
- Code: markers inside code spans and blocks, a span with a shorter run
  inside, a marker pair around a span that contains a marker, and tags in
  code keeping their text.
- Slack fences: one-line, text after the opening fence, a language word,
  a lone word with no code after it, text after a closing fence, an unclosed
  block, indented fences, six-tick fences.
- Quotes: hard breaks between lines, an empty `>` line, `>>>`, formatting in
  a quote, and list, heading and nested-quote markers inside one.
- Links: `mailto`, `javascript:`, `file:`, the board's own reference scheme,
  formatting in link text, images, a URL inside formatting, markers inside a
  URL, balanced, unbalanced and 32- and 33-deep parentheses in a
  destination, a URL holding `<`.
- HTML, entities, `$`, backslashes and Windows paths.
- References inside formatting and `nick:` addressing before formatting.
- Bare `-`, `--`, `=`, `1.` and `+` lines.
- mIRC codes (bold, colour, reset, italic) beside Markdown keep the IRC
  presentation.

`spaces/irc/src/markup/markdown/parsed.rs` parses the rewrite with the
`markdown` crate `TextView` uses (the board's `=1.0.0` dev-dependency, which
resolves to the vendored copy in `vendor/markdown`, with `TextView`'s GFM and
math options). It asserts that every rewrite parses, so slice 3a's guard
never has to step in, that no HTML, image, link reference, definition,
footnote, heading, rule, table or math node appears, and that every link is
one `safe_url` opens or a reference whose text names what it opens. It
covers every hostile input the reviews found and every three-character piece
over 21 syntax characters in six templates, one of them an empty or short
nested list item. The hostile inputs include:

- escaped and deeply nested parentheses and control characters in
  destinations and URLs;
- autolink literals swallowing a code span;
- an empty one-line fence and empty nested list items;
- a lone `\r`, entities, definitions, footnotes and math.

Beside it:

- a test checks that a link opens exactly the URL `safe_url` approved, for
  written links and bare URLs with brackets, backslashes and entity-like
  text;
- a test covers Unicode indentation (NBSP, U+3000), which used to panic, and
  ten-digit list numbers;
- `markup/tests.rs` runs the two list-and-fence messages that crashed the
  parser through the old IRC path and checks that the parser accepts their
  rewrites;
- a 32 KiB word and a 64 KiB line of `[a](` check that the rewrite stays
  linear.

Headless render tests (`spaces/irc/src/tests/formatting.rs`): rows on the real
board read the text as shown for each format; literal markers and mIRC text
read as typed; highlight words in `*bold*` and `~strike~` still count as
highlights; and the shared rich text, given the board's Markdown, shows bold,
italic and struck text without markers and keeps `` `<script>` `` (the same
test fails with `prepare_markdown`, which shows "bold italic gone and `").

## Not included

- Live formatting in the composer.
- Slack's `<url|text>` wire syntax, mentions by user ID, emoji shortcodes.
- Formatting inside link text, headings, tables, task lists, images.
- `_italic_` around a nick or highlight word does not highlight, because `_`
  is a nick character in both the runtime's and the board's word match.

## Known edges

- In a formatted message, `www.example.org` and `ops@example.org` are text,
  not links, as in a plain message; only `http(s)` URLs and `[text](url)`
  link.
- Rows are rewritten on every render without a cache. Words, markers and
  spaces are each walked once, in order; a line with many `[` or backticks
  that never close is rescanned per opener with a fast byte search. IRC lines
  are at most 512 bytes and multi-line batches a few kilobytes.
