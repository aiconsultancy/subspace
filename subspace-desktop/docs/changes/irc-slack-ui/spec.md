# Spec: IRC conversations that read like Slack

## Messages

- A message from a person in a channel or private conversation (`PRIVMSG`,
  `/me`, `NOTICE` with a sender) is a message row: a 32 px rounded-square
  avatar with the sender's initials on Kit's theme-aware identity colour, then
  a header line (the nick in bold in its stable nick tone, our own nick in the
  strong text colour; **Notice** for a notice; the local time `HH:MM`), then
  the text. `/me` text is italic and notice text is muted.
- A message continues its sender's group, without avatar or header, when the
  previous row is a message from the same nick (case-insensitive), of the same
  kind (notices group only with notices), at most 5 minutes later. A day
  change, a membership run or any compact line ends the group. Its time shows
  in the avatar gutter on hover; the accessible name always carries the
  sender.
- A line that mentions us or a highlight word keeps the selection fill.
- Grouped messages are tight; a new group has more space above it.

## Text

- Text with mIRC codes keeps the IRC presentation: styled runs, mIRC colours
  mapped to theme tones, links.
- Otherwise text that uses the safe Markdown subset (`**bold**`, `*italic*`,
  `_italic_`, inline code, fenced code, `- `/`* `/`• `/`+ ` bullets, numbered
  lists, `[text](url)`) renders through the shared rich text. The board
  rewrites it first: headings, quotes, rules, setext underlines, tables and
  HTML are escaped; each line keeps its own line; an unclosed fence is closed;
  unbalanced backticks and brackets are escaped; a link that names the
  board's own reference scheme is escaped, so a message cannot forge one.
- Plain text renders as before.
- `#channel` (with a letter in it, so `#4541` is not one) is a reference.
  `@nick`, and `nick:` or `nick,` as the first word, are references when the
  nick is a channel member. In plain text they are accent-coloured on a light
  accent tint; in Markdown they are links. Choosing a channel shows it when it
  is in the list, else joins it; choosing a member opens a private
  conversation.

## Rows around messages

- Joins, parts, quits and nick changes stay condensed into one muted line,
  with the time in the avatar gutter. Topic, mode, info and error lines are
  compact lines with the time in the gutter. The **Server** buffer keeps every
  line compact.
- A day starts with a centred pill on a hairline: **Today**, **Yesterday**,
  else `Friday 13 November`, with the year when it is not this year.

## Header, members and composer

- The detail panel's bar reads the buffer kind and the network name; the
  panel title row is the conversation header: the channel or nick (with its
  avatar for a private conversation), the topic under it in muted text (two
  lines at most, links open), and for a channel the member count, which shows
  or hides the member list. The topic is no longer repeated in the log. The
  **Server** panel keeps its title and facts.
- The member list is a 200 px panel beside the transcript with its own
  hairline, header **Members · N**, sections **Operators**, **Voiced** and
  **Members** with counts, rows with a small avatar, the nick (truncated with
  an ellipsis) and the prefix, away members dimmed and named as away, and its
  own scrolling. The toolbar's **Members** and the header count toggle the
  same state.
- The composer's placeholder is `Message #channel`, `Message nick`, or
  `Type a /command` in the **Server** buffer. Enter still sends.

## Views

- `Conversation` (`spaces/irc/src/conversation.rs`) is one buffer's view: it
  owns the `MessageScroller` state (the reader's place) and the composer
  draft, observes the hub, and renders header, transcript and composer on its
  own. It emits `ConversationEvent::Open(Ref)` and `ToggleMembers`.
- `MemberList` (`spaces/irc/src/members.rs`) is one channel's member list. It
  observes the hub and emits `MemberListEvent::OpenQuery(nick)`.
- The board keeps one of each per opened buffer and shows their parts in the
  detail panel.

## Not included

- No avatars from `draft/metadata-2`, no bot badge, no reply threads: the
  client does not parse avatar metadata, the bot mode or `+draft/reply`.
- The list layout and the detail panel's width are unchanged; the dock phase
  owns panes and their sizes.
