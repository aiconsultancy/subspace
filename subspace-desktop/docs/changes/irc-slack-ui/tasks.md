# Tasks: IRC conversations that read like Slack

- [x] Group messages by sender within 5 minutes; break groups on system
      lines and day changes; keep the server buffer compact.
- [x] Message rows with avatar, bold nick in its tone, notice mark and time;
      hover time for continued rows; mention fill kept.
- [x] Safe Markdown subset through the shared rich text, with escaping of
      everything outside it; mIRC text unchanged.
- [x] `#channel` and member references in plain text and Markdown; open or
      join a channel, open a private conversation.
- [x] Compact system lines and membership runs with the time in the gutter;
      day pill with Today, Yesterday or the date.
- [x] Conversation header with topic and member count; network name in the
      panel bar; no repeated title or topic.
- [x] Member list rebuilt: header, sections, avatars, away state, own
      scrolling; clipping regression test.
- [x] Composer placeholder.
- [x] `Conversation` and `MemberList` as self-contained views with per-buffer
      scroll position and draft.
- [x] Shared additions documented in `docs/ui-components.md`.
- [x] Docs: `docs/irc.md`, `spaces/irc/help.md`.
- [ ] After screenshot in Cupertino Dark (blocked by the shared build).
- [ ] Follow-ups: reply threads once the client tracks `+draft/reply`; a bot
      badge once it parses the bot mode; open a channel after joining it from
      a reference; host the views in GPUI Kit docks.
