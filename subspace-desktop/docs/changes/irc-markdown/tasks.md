# Tasks: write Markdown in IRC messages and render it like Slack

- [x] Survey what rendered before against Slack's set (intent).
- [x] One marker rule for detection and rendering: Slack's `*bold*`,
      `_italic_`, `~strike~` and Markdown's `**bold**`, `~~strike~~`, at word
      boundaries, outside code, links and URLs.
- [x] Slack's code fences (one-line, text after the opening fence, a fence
      ending a line) and a language word after the fence.
- [x] Quotes: `> ` lines and `>>>` for the rest of the message.
- [x] Links only for `http`, `https` and `mailto`; other link syntax, images,
      HTML, entities, math and backslashes shown as typed.
- [x] mIRC codes keep the IRC presentation; references, `nick:` addressing
      and highlights keep working inside formatting.
- [x] Show the rewrite with `rich::rich_text_prepared`, so code that shows a
      tag keeps its text (the shared function now comes from slice 3a).
- [x] Rows read the text as shown as their accessible name.
- [x] Unit tests for the translation; headless render tests for rows, the
      rich text and highlights.
- [x] Docs: `docs/irc.md` (Formatting messages), the IRC Help guide (Format
      messages), the component catalogue.
- [x] Before and after captures, and one capture per format, from fixture
      data.
- [ ] macOS check by the owner from the next build (see delivery).
- [x] Rebase onto the issue workspace implementation slice 3a and use its shared `rich_text_prepared`,
      dropping this branch's copy.
- [ ] When the issue workspace specification reference plugin lands, let it act on
      `Rendered::markdown`.
