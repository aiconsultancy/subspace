# Channels guide

<!--
Evidence: spaces/irc/src/{lib,view,rows,state,state/apply,state/counts,
state/removed,remove,lines,style,text,transcript,transcript/scroll,
transcript/fill,members,compose,compose/recall,compose/complete,
compose/suggest,compose/offer,join,listing,toolbar,hub,hub/running,config,setup,prompt,schema,editor,
editor/{draft,validate,form,view,network_view},agent_tools,
agent_tools/{bridge,read,write,read/search},notice,store,store/{mapping,links},
hub/persist,state/merge,state/directory,reveal,commands,context,issues,issues/{follow,tooltip},
markup/issue,conversation/keys,transcript/focus,transcript/line/marks}.rs;
subspace-framework/services/search
(records, migration); subspace-framework/board-kit/dashboard/src/
{lib,chat_input,chat_input/completion,notifications}.rs (LIST_KEYS, recall
and completion keys, notification defaults);
subspace-framework/host/app/src/notifications/runtime.rs (OS text and clicks);
clients/irc/src/session/input.rs (COMMANDS);
clients/irc/src/session/listing.rs (LIST); docs/irc.md;
docs/changes/irc-board; docs/changes/irc-mention-notifications;
docs/changes/irc-search-history; docs/changes/irc-issue-references;
docs/changes/channel-list. The channel list was checked against a local Ergo
server's LIST reply. Linux OS
notification delivery checked against a local Ergo server. Not checked in
this pass: a live public network, SASL against a real account and macOS
notification delivery.
-->

## Overview

The Channels Space connects to the IRC networks in its own settings file,
`irc/config.json` beside Subspace's `config.json`, and shows
each network with its server messages, channels and private conversations.
You can read channels, send messages and `/commands`, open private messages,
and get notifications when someone mentions you or writes to you privately.
The board speaks IRCv3: server time, chat history, read markers, typing
notices, away and account changes, and SASL sign-in. It keeps your channel and
private messages on this computer, shows them again when Subspace starts, and
makes them searchable with **Search** (`Cmd+Shift+F`, `Ctrl+Shift+F` on Linux
and Windows).

Channels ships off by default in release builds and on by default in debug builds.
If it is missing from the switcher, turn on **Enabled** under **Settings >
Channels** and save.

Open the board from the dashboard switcher. Press `?` while board navigation
has focus, choose **Help** in the command palette, or use the **Help (?)**
toolbar button to show or hide this guide.

## Before you start

- Add at least one network under **Settings > Channels**. While no network is
  configured, the board offers **Add network…**, which opens that page.
- To let a coding agent set up networks instead, choose **Copy agent prompt**
  on the board or in **Settings > Channels** and paste the prompt into the agent.
  It names this computer's `irc/config.json`, the exact format and how to
  keep passwords private.
- A SASL password or a server password stays in `irc/config.json`. The file
  is readable only by you (mode `0600`), in a folder only you can open (mode
  `0700`); Subspace ignores the file when others can read it. Settings never
  shows a saved password and never copies it into an input or into
  `state.json`.
- Network changes saved in Settings apply without a restart. Edits made to
  `irc/config.json` by hand or by an agent take effect after a restart.
- For local testing, `scripts/irc-test-server.sh` starts an Ergo server on
  `127.0.0.1:16667` with chat history. See
  [Test with a local server](../../docs/irc.md#test-with-a-local-server).

## Main view

The controls below **Channels** put the initiative picker on the left and
**Connect** or **Disconnect**, **Join…**, **Members** and **More** on the
right. In a narrow window, use **More toolbar actions** to reach buttons
that do not fit.

The board is a dock: the list of buffers on the left, each open channel,
private conversation or server buffer as a panel in the middle, and the
member list of the channel you are working in on the right. Drag a dock's
edge to resize it. The layout, including which conversations are open, how
they are split and the docks' widths, is kept for the next launch.

### Conversations, tabs and splits

Choose a buffer in the list, or press `Enter` on it, to open it. An open
buffer is brought to the front; another opens as a tab next to the
conversation you are working in. Right-click a buffer and choose **Open to
the right** or **Open below** to show it beside or under that conversation.
Drag a tab onto another group or the edge of a panel to move or split it,
and drag the line between panels to resize them. The × on a tab closes the
panel; the buffer stays in the list. To take it out of the list, see
[Remove channels and conversations from the list](#remove-channels-and-conversations-from-the-list). A tab shows how many messages arrived while it was not the
conversation you were working in, highlighted when they mention you.

### Networks and buffers

Each configured network is a group. Its header shows a coloured dot and the
connection state in words: **Disconnected**, **Connecting…**, **Signing in…**,
**Connected**, **Reconnecting in N s** or **Failed**. Under it are:

1. **Server**: server notices, the message of the day, numerics and errors.
2. Channels, sorted by name. A channel you left with `/part`, or were kicked
   from, stays in the list with a muted title until you join it again or
   choose **Remove from list**.
3. Private conversations, sorted by nick.
4. The server's other channels, the ones you are not in, sorted by name.
   They are greyed out and show how many people are in each, such as
   `7 users`. See [Join channels from the list](#join-channels-from-the-list).

A row's trailing number counts unread messages. It uses the accent colour and
the row title turns bold when the unread messages mention you. In a private
conversation any unread message makes the title bold. Joins, parts and other
membership lines never count as unread.

### Transcript

A message with replies shows their count and the last reply's local time
beneath it. This counts only replies loaded on this computer, including
replies to replies. Loading older history can increase the count. It works
in channels and private conversations. Replies still appear inline; the
count does not open a pane yet.

Opening a row shows its conversation. Messages are grouped by sender: the
first message shows an avatar with the sender's initials, the nick in a colour
that stays the same for that nick (your own nick uses the strong text colour)
and the local time (`HH:MM`). Further messages from the same person within
five minutes follow without repeating them; point at one to see its time.
The message or line under the pointer is shaded, as in Slack.
`/me` actions are italic and notices are muted and marked **Notice**. Lines
that mention you have a highlighted background. A pill on a rule starts each
day: **Today**, **Yesterday** or the date.

Joins, parts, quits and nick changes are condensed into one muted line, such
as `alice, bob joined · carol left`. Topic changes, modes, server lines and
errors stay as single compact lines. The **Server** buffer shows every line
this way.

mIRC bold, italic, underline, strikethrough and monospace formatting show as
sent. mIRC colours show as a light tint of the theme's matching colour behind
the text. Background colours and the extended colours 16 to 98 are not shown. Messages formatted the way Slack
reads them, from anyone, show bold, italic and struck-through text, `code`,
code blocks, quotes, bullet and numbered lists and links (see
[Format messages](#format-messages)). Headings, tables and HTML show as typed,
and an image shows as a link, so nothing loads from the internet. `http`,
`https` and `mailto` links open in your browser.
`#channel` names are links: choosing one shows the channel, or joins it when it
is not in the list. `@nick`, or a member's nick and a colon at the start of a
message, is highlighted; choosing it opens a private conversation.

To copy part of the conversation, drag from any point in one line to any
point in another, as in Slack. Every message, server line and joined or left
line between is selected, and **Copy** (⌘C on macOS, Ctrl+C elsewhere) copies
their text in reading order, a blank line between lines. Message name headers,
times and day dividers are not copied; names within compact server or topic
lines are part of their text. Right-click a line and choose **Copy** to copy the
selection, or the whole line when nothing is selected.

While a board that knows your issues is on and signed in, such as the Issues
board, issue identifiers like `SUB-827` and links to its issues are
highlighted too (see [Issue references](#issue-references)).

The transcript keeps your place when you scroll up, also when you switch to
another buffer and back; **Jump to latest** returns to the newest line.

### Switch initiatives

If you have Subspace initiative memberships and a Subspace IRC connection,
open the initiative menu below **Channels**. Search for an initiative and
select it to join its channels. **Local channels** restores the autojoin
list in your IRC settings. The initiative choice also filters the Projects
menu in Issues. You can change or clear the selection from either Space.

One verified Subspace IRC connection uses the selected list. Other
networks, private conversations and manually joined channels stay open.
A channel you leave can still appear in the server's public channel list.
A selection lasts until you quit the app. The IRC server still controls
which channels you can join.

To show the menu when an initiative is available, configure Subspace IRC
with TLS and certificate verification. Use **Reload** to retry if the
Subspace API is temporarily unreachable.

## Toolbar and filters

| Control | What it does |
| --- | --- |
| **Connect** / **Disconnect** | Connects or disconnects the network of the selected row or group. Disabled until you select one. |
| **Join…** | Opens **Join a channel on** the selected network. |
| **Members** | Shows or hides the member list dock. |
| **More** | **Leave channel** leaves the selected channel and removes it from the list; for a channel you are no longer in, it reads **Remove from list**. **Close conversation** removes the selected private conversation from the list. Each is disabled for other kinds of row, as is **Leave channel** for a channel you are in while its network is not connected. |

The board has no filters. Reload (`r`) connects every network that is
disconnected or failed.

## Details

The dock tab names the conversation. Point at the name to read its network
and topic. The **Members** toolbar button opens the member list. It follows the
conversation you are working in and lists **Operators**, **Voiced** and
**Members**, each sorted by nick with a small avatar and the nick's prefix.
Away members are dimmed. Choose a member to open a private conversation with
them, or × to hide the list. The list fills the dock up to the window's
edge at any width you drag it to.

Point at the **Server** buffer's name for the connection status, any error,
your nick and the IRCv3 capabilities the server enabled.

When Subspace starts, each channel and private conversation shows the messages
kept from earlier sessions straight away. After connecting, it asks the server
only for the messages you missed since the newest kept one. Opening a channel
or private conversation with few lines asks the server for recent history when
it supports `draft/chathistory`. **Load older messages** at
the top of the transcript asks for the 100 lines before the oldest one shown.
The control disappears when the server has no more.

Above the message input, a line names who is typing, such as
`alice is typing…`. The empty input reads `Message #channel` or
`Message nick`.

## Actions

### Send a message

1. Open a channel or private conversation.
2. Type in the message input and press `Enter` (`Return` on a Mac keyboard).
   `Shift+Enter` (`Shift+Return`) adds a new line without sending, as the
   hint in the empty input says; a multi-line message is sent as
   `draft/multiline` batches (one when it fits the server’s limits), or in
   separate messages on other connections.

Drafts over 4,096 UTF-8 bytes, or over the line limit, stay in the composer.
The limit is 100 lines on a server that takes a multi-line message as one batch
(`draft/multiline`, as the Subspace Ergo server does), and 10 lines elsewhere and for
commands. Shorten the draft or choose **Save as file…** to save its text.

The input grows from two to twelve visible rows, then scrolls. In a small
window, its cap follows half the Space's height. Drag the top edge to resize;
focus the resize handle and use Up or Down, or Home to restore automatic
growth. The send hint fades as soon as you type and returns when you clear
the draft. Errors and **Save as file…** stay visible.

A pill at the input's top right shows the line count after the first line.
Above 2,048 bytes, it also shows the size in KB (1 KB is 1,024 UTF-8 bytes).
It uses a warning tone from 80% of either limit and a danger tone above a
limit. Commands use the 10-line cap; `//text` is literal text and can use
the 100-line cap on a batching connection.

Commands start with `/`. Type `/` at the start of the input to see them all,
each with its arguments and what it does: `/join`, `/part`, `/msg`,
`/query`, `/me`, `/notice`, `/topic`, `/nick`, `/away`, `/whois`, `/invite`,
`/kick`, `/mode`, `/quit` and `/raw` (also `/quote`). Commands also work in the
**Server** buffer. `/query nick` opens and shows that private conversation.
While you type a message, the channel sees a typing notice; commands send
none.

### Send files as attachments

Drop or paste files into the conversation, or choose **Send as attachment…**
to turn the text you typed into a Markdown attachment. Each file waits above
the composer as a small thumbnail; hover it for the name and size, and use its
**×** to remove it. Removing a text attachment puts its text back when the
composer is empty.

A message carries up to 10 attachments, 25 MB (25,000,000 bytes) in total.
Files are checked in the order they were added: Subspace keeps the files that
fit and says when it left some out. Supported files are Markdown, plain text,
PNG, JPEG, GIF, WebP, ZIP, PDF, DOCX and XLSX. Images must be at most 4096
pixels on each edge and 16 megapixels. ZIPs must be unencrypted, contain at
most 4,096 entries, expand to at most 250 MB and use relative paths. DOCX and
XLSX packages also have a 250 MB expansion limit.

Press **Enter** to send. Subspace uploads every attachment first, then sends
your text with one `UPL-… name (size)` reference per file. When everything fits
in one IRC line it is one line; otherwise your text goes first, followed by the
references packed into as few lines as fit. A reference is never split. You can
send attachments without text. Commands such as `/me` can't carry attachments.
If any upload fails, nothing is sent to IRC and your text and attachments stay
for another try. If you lose the connection after the uploads finish,
reconnect and press **Enter** again; the files aren't uploaded twice. While a
connection is still being set up, sending says "Not connected yet. Your
message is kept." If the connection drops while your message is being sent,
your text and attachments stay and the composer says some lines may have
gone: check the channel before you press **Enter** again.

The composer says “Visible to signed-in Subspace users for 30 days. Don’t
paste credentials into chat.” Put material worth retaining on an issue.

Received attachments show as a row of thumbnails under the message, in the
order they were sent; the reference text is hidden once it resolves. Images
show as square previews and other files as a chip with their name. Hover a
thumbnail for its name, size and expiry, and click it, or focus it with **Tab**
and press **Enter** or **Space**, to open the viewer, with **Save file…** for the
complete file. ZIP, PDF and Office documents show file details; use **Save
file…** to open them in another app. Very large text previews are shortened
with a notice. A thumbnail that couldn't load opens to try again. A missing or
expired upload shows **Attachment unavailable** with its key. Sign in under
**Settings > Account** to open uploads.

If an upload result is unclear, or you signed in again after uploading, use
**Check upload**: it asks Subspace about the same upload instead of uploading
again. Sent references are never resent automatically. After sending, the
attachments stay until the message comes back from the server; you can keep
typing and press **Enter** to send ordinary messages, or choose **Dismiss** to
clear them without sending anything again. Text typed in the meantime is kept.

### Format messages

Type the markers in the message input. The message is sent as typed, and
Subspace shows the formatting in channels and private conversations, for your
messages and everyone else's. Other IRC clients show the characters as typed.

| Type | To show |
| --- | --- |
| `*bold*` or `**bold**` | Bold |
| `_italic_` | Italic |
| `~strike~` or `~~strike~~` | Struck-through text |
| `` `code` `` | Inline code |
| ```` ```code``` ````, or ```` ``` ```` on the lines before and after the code | A code block |
| `> ` at the start of a line | A quoted line; `>>>` quotes the rest of the message |
| `- ` or `1. ` at the start of a line | A bulleted or numbered list |
| `[text](https://example.org)` | A link that shows your text |

- A marker formats only around whole words and only when it is closed on the
  same line, so `snake_case_names`, `2*3*4`, `src/*.rs` and `(*.md)` show as
  typed.
- Nothing inside `code` is formatted. To show a marker as typed, put it in
  backticks.
- One word right after an opening ```` ``` ````, such as `rust`, names the
  code's language and shows in the block's corner. When no code follows it,
  the word is the code.
- Use `Shift+Enter` (`Shift+Return`) for the lines of a code block, quote or
  list.
- A message with mIRC formatting, sent from another client, keeps that
  formatting, and its other characters show as typed.

### Mention people and complete names

- Type `@` and the start of a name to see the matching members, with recent
  speakers first. Choosing one inserts `@nick` and a space; the message shows
  it as a mention, and that person is notified.
- After `/join `, the list offers the network's channels. After `/part ` or
  `/topic `, it offers the channels you are in. After `/msg `, `/query ` and
  other commands that take a nick, it offers people on the network.
- Type `#` in a message to see the channels you are in.
- `Up` and `Down` move through the list, `Enter` or `Tab` inserts the
  highlighted choice, and `Esc` closes the list. You can also click a choice.

While no list is open, `Up` and `Down` recall lines you sent from this input
when the draft is empty or one line. They keep going through a recalled
message of several lines until you edit it; in any other draft of several
lines they move between its lines. `Tab` completes the nick before the cursor
from the channel's members. At the start of the line it adds `: `.

### Join channels from the list

After it connects, each network lists every public channel on the server
below your own rows, greyed out with its number of users. The list is kept
on this computer, so it shows straight away the next time Subspace starts,
and is replaced when the server sends a new one.

- To join a channel, right-click it and choose **Join**, double-click it, or
  select it and press `Enter`. The channel opens once the server lets you
  in, and its row becomes a normal channel. A single click only selects it.
- To fetch the list again, right-click the network's name and choose
  **Refresh channel list**, or choose **Refresh channel list** in the
  command palette.
- `/` finds channels you are not in as well; choosing one selects it.
- Joining from the list does not add the channel to **Channels to join**;
  add it in **Settings > Channels** to join it on every connect.
- Secret (`+s`) and private (`+p`) channels are not in the server's list
  unless you are already in them. Join one by name with **Join…** or
  `/join`.

### Join and leave channels

1. Select a network, or any of its rows.
2. Choose **Join…**, type the channel name (a leading `#` is added when
   missing) and an optional channel key, then choose **Join** or press
   `Enter`.

To leave, right-click the channel and choose **Leave channel**, or select it
and choose **More > Leave channel**: Subspace leaves the channel and takes it
out of the list (see below). Typing `/part` leaves the channel but keeps it in
the list with a muted title. Channels in the network's **Channels to join**
are joined on every connect, except one you left with **Leave channel**.

### Remove channels and conversations from the list

Right-click a row in the list:

- **Leave channel** leaves a channel you are in and removes it from the list.
  While a network is not connected, you cannot leave a channel you are in.
- **Remove from list** removes a channel you are no longer in, such as after
  a kick or `/part`.
- **Close conversation** removes a private conversation from the list. The
  other person is not told. Their notices, such as a service's replies, go to
  the **Server** row again.

The panel of a removed buffer closes. The item stays out of the list after a
restart, a reconnect and the server's replay of history, also when the
channel is in the network's **Channels to join**. A removed private
conversation comes back when a new message arrives in it, such as one from
that person, or when you open it again with `/query`, from the member list or
from one of its notifications. A left channel comes back when you join it
again, or when you add it to **Channels to join** in Settings; choosing an
earlier notification from it does nothing. The **Server** row cannot be
removed.

### Issue references

While the Issues board is on and connected to Subspace API, Channels
highlights identifiers with the loaded project keys, such as `SUB-827`.
Lowercase identifiers (`sub-827`), `UTF-8`, text in code and URLs stay plain
issue text or ordinary links. App links and Linear links are not issue references.

Readable issues always appear below the message as compact chips, even when
only one is available. The row wraps to fit the transcript. Click a chip or
press `Enter` on it to open the same issue in Issues.

Use the small chevron beside a chip to show its summary under the chip row.
Press it again to close the panel, or choose another chip’s chevron to switch
the one panel for that message. This keeps you in Channels. Older issues show
**No summary yet**. The panel grows and fades over 200 ms; reduced motion makes
the change instant. A transcript at the bottom stays at the bottom.

Point at a chip or an inline reference, or move the keyboard to a reference,
to see the same floating card. An ordinary issue shows its state edge,
identifier, urgent or high priority, title and state. A supplied summary sits
below the title in muted text, wrapped to at most three lines. When supplied, its footer
shows the assignee (an anchor for an agent, initials for a person), agent rank,
a different owner and the latest activity time. A green dot marks activity
within the last hour. Tracker cards show sub-issue progress, open pull requests
and known checks, merged pull requests and a due date. Missing facts are omitted.
The cards have no controls and do not take keyboard focus.

Until a summary arrives the reference shows `Loading SUB-827…`. An unavailable
reference turns grey and underlined, and its note says
`SUB-827 isn't available with Subspace API.` The note never says whether such
an issue exists. Signed-out and failed lookups keep their short status notes.
Choosing a reference opens the issue in the Issues board.

With the Issues board off or unavailable, identifiers are plain text and
links are ordinary links. Channels keeps issue titles in memory only: they
are not stored with your messages or searchable.

### Pull request references

Channels shows a GitHub pull request as an inline chip: the pull request glyph
and `repo#number`, such as `api#421`. The owner appears only when the same
message also names another owner's repository of that name
(`octo-org/api#421` beside `acme/api#5`). These become chips:

- the marker `[PR:owner/repo#number]`;
- a full `https://github.com/owner/repo/pull/number` link, including its
  `/files` and `/commits` pages, a query or a `#discussion…` anchor;
- a Markdown link whose label is the marker and whose destination is the same
  pull request.

A marker and a link to the same pull request are the same chip. A bare
`#123` or a marker without an owner (`[PR:repo#123]`) stays plain text:
Channels has no repository to read it in. A pull request inside code (between
backticks or in a code block) stays text too. Other GitHub links stay ordinary
links. No extra card appears under the message.

Point at a chip, or move the keyboard to it with `Tab`, to see its card. It shows the state (open, draft, merged or
closed), `repo#number`, the title, branches and changed lines when known, the
author, the review and checks state and how long ago it changed. The PRs Space
reads it with the GitHub account it uses, so you see only pull requests that
account can read. Until it answers the card says `Loading api#421…`. When the
account cannot read it, nobody is signed in, the PRs Space is off or the read
failed, the card says **Details unavailable**, and never whether the pull
request exists. Switching account, signing in or out, or reloading PRs forgets
every card.

Click a chip, or press `Enter` on it, to select the pull request in the PRs
Space. When the PRs Space is off, Channels opens the link in your browser and
says so; it never turns the Space on. Dragging across a chip selects text
instead of opening it. **Copy** keeps the text as written; **Copy link**
copies the link as written, or `https://github.com/owner/repo/pull/number` for
a marker. Card details stay in memory only: they are not stored with your
messages or searchable.

### Move through messages with the keyboard

In the message input, `Shift+Tab` moves to the newest message. `↑` and `↓`
move between messages, and `Tab` and `Shift+Tab` between the links and
references in the focused message, in reading order, formatted messages
included; a focused issue or pull request shows its card. `Enter` opens the
focused link or reference as a click does, and `Esc` returns to the message
input.

### Search messages

1. Open **Search** from the toolbar, or press `Cmd+Shift+F` (`Ctrl+Shift+F`
   on Linux and Windows).
2. Type words from the message and press `Enter`. Results from IRC show the
   channel and sender (`#channel · nick`), or the person in a private
   conversation, and **Channel message** or **Private message**.
3. Choose a result. Its conversation opens with the message selected. When the
   message is older than what the conversation holds, Subspace first asks the
   server for the messages around it; if the server no longer has it, a notice
   says so.

Search covers every kept message within the retention period, not only what
is on screen. A message that links a GitHub pull request or issue also shows
among that item's related items for agents (`related_items`).

In the board, `/` (or the command palette) finds a channel or private
conversation by name and opens it. The palette also offers **Join channel…**,
**Refresh channel list**,
**Connect to** or **Disconnect from** the selected network, **Mark … as
read**, and the selected row's **Leave channel**, **Remove from list** or
**Close conversation**.

### Find in the loaded transcript

Press **⌘F** on macOS or **Ctrl+F** elsewhere to find text in the focused
conversation's loaded transcript, including loaded messages outside the visible
rows. This does not fetch older history. Matching ignores case; every match is
highlighted and the bar shows the current position and total. **Enter / Shift+Enter**
and **⌘G / ⇧⌘G** (Ctrl+G / Ctrl+Shift+G elsewhere) move between matches and scroll
to them. **Escape** closes find and restores focus. App-wide Search keeps
**⌘⇧F / Ctrl+Shift+F** and can search kept history beyond the loaded transcript.

Drag within a message to select a range, double-click for a word or triple-click
for a line. Drag across messages to copy in reading order. The right-click text
menu offers **Copy**, **Copy link** on links, **Select all** for the block and
**Add to chat**. Opening it does not put text in chat.

### Keep or clear message history

Subspace keeps every channel and private message (yours included) in a
private store on this computer, readable only by you. Joins, parts and server
lines are not kept. Messages older than the retention period (60 days unless
you change it) are deleted, oldest first, as are the oldest when the store
reaches its size limit.

- To stop keeping messages, open **Search**, then **Cached sources**, and turn
  off **Cache Channels**. That deletes the kept messages; the board then holds
  lines only while it runs.
- To delete every kept message but go on keeping new ones, choose **Clear
  cache…** under **Cache Channels**.
- **Leave channel**, **Remove from list** and **Close conversation** delete
  that buffer's kept messages.

**Settings > Channels** says where these controls are.

### Notifications and read state

Opening a channel or private conversation marks it read. A **New** line and
a tinted background show which messages were unread when you opened it.
With `draft/read-marker`, the read position syncs with your other clients,
and reading elsewhere clears unread counts here.

When someone mentions you, for example with `@yournick`, or writes to you
privately, Subspace shows an OS notification, with a sound on macOS. It
names the person, the channel and the network, never the message. It does
not appear while that conversation is on screen with Subspace in front,
including beside or below another conversation in a split.

Choosing the notification brings Subspace to the front on that conversation.
Your own messages never notify. Kept messages shown again when Subspace
starts, history loaded when you connect or reconnect, and messages you already
read in another client, don't notify either. Change the delivery and the sound in **Settings > Notifications >
Channels**. The switcher badge counts mentions plus unread private messages.

### Notify ship captains

**Settings > Channels > Notify ship captains** is on by default. With automatic
board refresh on, Subspace checks your favourite repositories every ten
minutes while the app is open and connected to the selected initiative's IRC
server.

Subspace reads the issue key from the PR description, for example `Closes APL-7`.
The issue assignee identifies the ship and the project identifies the initiative.
Subspace posts to that initiative's `#<initiative>-<ship>` channel and mentions
the captain. You must have joined the matching channel. Subspace skips the
update if it cannot resolve one issue and one channel.

Updates include PR merges and deployments of the merge commit that GitHub
reports as started, failed or succeeded. Events before monitoring starts are
skipped. Choose **Save** to apply the switch.

### Agent tools

In the running app, Subspace chat and **Serve MCP over HTTP** offer `irc_read`
and `irc_write`. An agent can read networks, channels, recent messages and
members, search the messages the board holds and keeps, and, after you approve
each call, send messages, join or leave
channels and connect or disconnect networks. The approval lists every line it
will send. Messages go out as you and look exactly like your own. `subspace
mcp` and command-line tools cannot reach the board, so they have no IRC tools.
**Settings > MCP** can turn the IRC group off for MCP clients. **Add to chat**
(`Cmd+Shift+A`) attaches the conversation in front with its 40 newest
messages, marked as other people's text.

### Conversation tabs

The selected tab joins the conversation below it, with an accent line along
its top and stronger text. Other tabs stay quiet until you point at them.
An accent dot before a conversation name marks unread messages; the count
still shows how many, with mentions keeping their existing highlight.

The close button appears on the selected tab and while you point at another
tab. Keyboard focus also reveals it with a focus ring. Closing a tab does not
select it first. Drag tabs to arrange conversations or move them into splits.


## Keyboard shortcuts

Plain keys act only while no text field, menu or dialog has focus. Typing in
the message input is ordinary text input.

### Board keys

| Key | Action |
| --- | --- |
| `j` / `↓` | Next row |
| `k` / `↑` | Previous row |
| `Enter` | Open the buffer, or fold or unfold a network |
| `Enter` (`Return`) in the message input | Send, or insert the highlighted choice |
| `Shift+Enter` (`Shift+Return`) or `Option+Enter` | New line in the message input |
| `Up` / `Down` in the message input | Recall sent lines from an empty or one-line draft, move between a longer draft's lines, or move through the completion list |
| `Tab` in the message input | Complete a nick, or insert the highlighted choice |
| `Esc` in the message input | Close the completion list |
| `Esc` in **Join a channel** | Cancel |
| `Shift+Tab` in the message input | Move to the newest message |
| `↑` / `↓` on a message | Previous or next message |
| `Tab` / `Shift+Tab` on a message | Next or previous link or reference in it |
| `Enter` on a link or reference | Open it |
| `Esc` on a message | Back to the message input |

### Shared keys

`F1` to `F12` show the board at that position in the enabled-board order. `?`
shows or hides this guide. `r` or `Cmd+R` (`Ctrl+R` on Linux and Windows)
reconnects networks that are not connected. The command palette opens with `Cmd+K` on macOS,
`Ctrl+Shift+K` on Linux and `Ctrl+K` on Windows.

## Settings

Open **Settings** and choose **Channels** in the sidebar.

| Setting | Default | Notes |
| --- | --- | --- |
| **Enabled** | Off in release builds, on in debug builds | Turning the board off disconnects every network. |
| **Networks** | None | The networks to connect to. **Add network** adds one; **Edit…** and **Remove…** change one. |
| **Highlight words** | None | Words, separated by commas, that mark a message as mentioning you on every network, besides your nick. |
| **History** | — | Says that messages are kept on this computer and that **Search** turns this off (**Cache Channels**) or deletes them (**Clear cache…**). |

**Edit…** and **Add network** open the network's form. **Done** returns to the
list and names any value that still needs fixing.

| Field | Notes |
| --- | --- |
| **Name** | Shown as the network's group. A new network's id comes from it. |
| **Host**, **Port** | Required. The port is 1 to 65535; 6697 is the usual TLS port. |
| **Use TLS** | On by default. **Accept invalid certificates** is only for a local test server. |
| **CA certificate**, **Client certificate**, **Client key** | Full paths to PEM files. **Choose…** opens a file picker. |
| **Nickname**, **Alternative nicknames** | The nickname is required. Alternatives are tried when it is taken. |
| **Username**, **Real name** | Default to the nickname. |
| **Server password** | Only for servers that ask for one. |
| **Account sign-in (SASL)** | **None**, **Account and password (PLAIN)** or **Client certificate (EXTERNAL)**. PLAIN takes an account and a password, or **Read the password from a file**. EXTERNAL needs TLS and a client certificate. |
| **Channels to join** | Separated by commas. Each starts with `#` or `&`. |
| **Connect on startup** | On by default. |

Password fields are masked. A saved password shows **Password saved** and its
input stays read-only; **Clear saved password** lets you type a new one or
remove it on Save, and **Undo clear** keeps it. **Remove…** asks
**Remove "name"?** before the network leaves the list. Nothing is written until
Settings' **Save**; **Cancel** discards every change. After Save, changed
networks reconnect, removed networks disconnect and unchanged networks stay
connected.

**Copy agent prompt** copies setup instructions for a coding agent. **Show in
Finder** (**Show in File Explorer** on Windows, **Show in Folder** on Linux)
shows `irc/config.json`, or its nearest existing folder before the first save.

**Settings > Notifications** has a Channels source with **Delivery** (**OS** by
default, **Off**, **In-app** or **Both**), **In-app style** and **Sound**
(**Glass** by default on macOS; Linux has no sounds to choose).

At the top of that page, **Play notification sounds** mutes Channels, Copilot
and every other Space on Save, keeping Channels' own Sound choice for later. Cancel
discards the edit. **Preview sound** still plays explicitly; **Test
notification** follows the saved master choice.

## Troubleshooting

| Symptom | Check or action |
| --- | --- |
| `No IRC networks` | Choose **Add network…** and add one in Settings. A malformed entry in `irc/config.json` is named in red under the actions. A file that is not valid JSON, or that others can read, is named in the startup notices. |
| Save reports a network problem | The message names the network and the field. Choose **Edit…** to fix it, or remove the network. |
| A network shows **Failed** | Open its **Server** row: the error is the latest red line and in **Status**. Check `host`, `port` and `tls`, then choose **Connect**. |
| **Reconnecting in N s** repeats | The server closed the connection or could not be reached. The runtime retries with a growing delay. |
| SASL sign-in fails | The server's reply shows in the **Server** buffer. Check the account and password in **Settings > Channels**. |
| No older messages load | The server does not offer `draft/chathistory`; the **Server** panel's **Capabilities** list shows what it enabled. |
| Messages are gone after a restart | **Cache Channels** is off in **Search > Cached sources**, the store was cleared, or the messages are older than its retention period. |
| `This message is no longer on the server.` | The search result's message is older than what the conversation holds and the server's history no longer has it. |
| A channel you expected is not in the list | Secret (`+s`) and private (`+p`) channels are listed only to their members; join one by name with **Join…**. Choose **Refresh channel list** on the network for a newer list. Large networks show only their 500 busiest channels. A channel you left with **Leave channel** is no longer joined on connect, even if it is in **Channels to join**; join it again to undo that. |
| Unread counts do not sync with another client | The server does not offer `draft/read-marker`. |
| A notification does not appear | Check **Delivery** in **Settings > Notifications > Channels**, and that the operating system allows Subspace's notifications (on macOS, **System Settings > Notifications**). The conversation you are reading with Subspace in front, your own messages and loaded history do not notify. |

## Limits

- Per-network highlight words are kept but can be changed only in
  `irc/config.json`.
- Each mention or private message shows its own notification. A server that
  replays old messages as ordinary new ones, without chat history or read
  markers, can notify for them again. Through a ZNC bouncer, its `*status`
  messages are private messages and notify too.
- Each buffer keeps the latest 2,000 lines in memory and shows the newest
  2,000 kept messages after a restart. Joins, parts and server lines are not
  kept, so they come back only from the server's history.
- After a restart, at most 500 missed messages per channel are fetched, as the
  server allows; older missed ones remain on the server only. Without chat
  history on the server, nothing missed is fetched.
- History loads 100 lines per request.
- Background colours and extended mIRC colours are not shown, and an mIRC
  colour is a tint behind the text rather than the text's own colour. A
  message with mIRC formatting is not read as Markdown.
- A drag selection leaves out message name headers, times and day dividers.
  Names within compact lines are copied with their text. A drag that scrolls
  far past the lines on screen may lose
  where it started, because lines well off screen are not kept; select in
  shorter pieces.
- Formatting markers are not shown live in the message input. `_italic_`
  around your nick or a highlight word does not highlight the message,
  because `_` can be part of a nick; `*bold*` and `~strike~` do.
- Avatars show initials: IRC has no profile pictures. There are no reply
  threads or bot badges.
- `@` completion uses the channel's members and the conversation partner.
  Command arguments use people and channels the network already shows; there
  is no server-wide channel or user search. `Tab` without a list completes the
  first match without cycling.
- No DCC transfer, ignore list or per-channel notification settings.
  Temporary Subspace uploads use the authenticated API rather than IRC file transfer.
- The channel list keeps the 500 busiest channels of each network and does
  not show topics.
- An issue reference or preview tile opens in the Issues Space, including
  while its summary is loading. Without an Issues
  scope, identifiers stay text and ordinary links open in your browser. Issue
  references in formatted messages (bold, code, lists and the like) are
  links without a card or grey look, and only plain messages show the
  focused reference's highlight.
- The change record includes isolated fixture captures of upload cards, previews and viewers.
- Other IRC clients show long attachment messages as separate lines. Subspace
  does not rejoin them: plain IRC has no reliable marker for a split message.
