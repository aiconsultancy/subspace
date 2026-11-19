# Tasks: drag selection across the IRC transcript

- [x] Headless drag tests on the board: Markdown rows created out of reading
      order, and mixed Markdown, plain, mIRC and membership rows.
- [x] Kit-level tests of `TextView`, `InteractiveText` and `SelectableText`
      interleaved (`tests/selection/kit.rs`).
- [x] Findings with Kit `file:line` citations ([findings](findings.md)).
- [x] Literal Markdown for plain and mIRC-coded text, with parser tests for
      Markdown-significant text.
- [x] Every row's text through the selectable `TextView`, with tones, links,
      references and the shared Copy menu.
- [x] mIRC colours as theme tints; the decision recorded.
- [x] `docs/irc.md` and the IRC Help guide.
- [x] Evidence from the debug capture build on Linux, before and after.
- [x] Gate: `make check`, `mise run quality`.
- [x] Reviews: CodeRabbit CLI, read-only reviewer, final Opus review (no
      Codex pass: paused by the owner).
- [x] Merge the current desktop base, preserving unread highlighting (#37)
      and issue-reference navigation and cards.
- [x] Test drag selection and the Copy action across the unread divider,
      including unread plain, reference and mIRC text.
- [x] Check unread and selection fills remain distinct.
- [x] Capture the unread state and divider-crossing selection at 2× in both
      Cupertino themes.
- [x] One final local Claude review of the pushed merged PR, with the valid
      URL-style P2 and two documentation P3s addressed and verified.
