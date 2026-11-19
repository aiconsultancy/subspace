# Intent: write Markdown in IRC messages and render it like Slack

As part of the Issues and Channels initiative, the IRC Space should
behave more like Slack. People should be able to write Markdown in the
composer and send it, and the conversation should show it formatted the way
Slack formats it, in channels and private conversations, for messages from
anyone.

Sending stays raw text: the composer has no live formatting, and other IRC
clients see the characters as typed.

## What rendered before (v0.2.0)

Subspace v0.2.0 (`83595fa`) rendered "mIRC formatting or a safe Markdown
subset through the shared rich text". The subset was aimed at bots and
agents. `markup::is_markdown` decided whether a message was Markdown with a
heuristic. The text was then rewritten by `markup::to_markdown` and shown with
`rich::rich_text_with_links`, which runs `prepare_markdown` and GPUI Kit's
`TextView` (GFM). Measured on `origin/feat/subspace-desktop` at `e8757cf`
(throwaway test, outputs fed to the same `markdown` parser `TextView` uses):

| Slack format | Typed | Before |
| --- | --- | --- |
| Bold | `*bold*` | **Italic**, not bold (CommonMark emphasis). |
| Bold | `**bold**` | Bold. |
| Italic | `_italic_` | Italic. |
| Strikethrough | `~strike~`, `~~strike~~` | Shown as typed: not detected as Markdown. In a message that was Markdown for another reason, GFM struck it through, except at the start of a line, where the first `~` was escaped and the rest showed as typed. |
| Inline code | `` `code` `` | Inline code. A tag inside it lost text: `` run `<script>` first `` showed "run `" because `prepare_markdown` read the tag as HTML and dropped the rest of the message. |
| Code block | ```` ``` ```` lines | A code block. Slack's ```` ```let x = 1``` ```` became inline code plus an empty block; ```` ```npm install ```` hid `npm install` as the language name. |
| Block quote | `> quoted` | Shown as typed (escaped). |
| Bulleted list | `- `, `* `, `• `, `+ ` | A list. |
| Numbered list | `1. ` | A list. |
| Link | bare `https://…` | A link. |
| Link | `[text](url)` | A link when the message contained `](http`. Any other scheme (`javascript:`, `file:`) rendered as a link that did nothing. |

Detection was separate from rendering, so the two could disagree: `*.rs and
*.toml` was not Markdown, but `2**3 and 4**5` was.

## Goal

- Slack's markers format as Slack does, and the standard Markdown forms still
  work.
- One rule decides what is formatting, used for both detection and
  rendering, and marks formatting only at word boundaries.
- Nothing typed is lost or turned into HTML, images or unsafe links.
- mIRC formatting, `#channel` and `@member` references and mention
  highlighting keep working.
