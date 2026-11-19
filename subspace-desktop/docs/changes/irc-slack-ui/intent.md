# Intent: IRC conversations that read like Slack

The owner asked to make the IRC board "more like Slack". The transcript was a
terminal-style log: a time column, then `nick text` on one line, with bot
reports showing raw Markdown (`**bold**`, backticks, `- ` bullets). The panel
repeated the channel name as a large title under the bar, the topic floated
inside the log, and the member column was clipped on its left edge with no
clear boundary.

The goal is a conversation view that reads like a modern chat client while
staying a desktop GPUI Kit app and honest to IRC:

- messages grouped by sender under an avatar, a bold name and the time;
- bot and agent reports rendered in a safe Markdown subset, with mIRC
  formatting kept for everyone else;
- `#channel` and `@member` references that open what they name;
- compact, muted system lines and a date pill;
- a channel header with the topic and the member count, and a proper member
  list.

The conversation and the member list are self-contained views, so the next
phase can host them in GPUI Kit dock panels (tabs, splits, docks) instead of
the list board's detail panel.
