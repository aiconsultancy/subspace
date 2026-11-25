# Channels guide

<!--
Evidence: spaces/irc/src/{lib,view,view/frame,view/conversations,
view/details,dock,dock/layout,rows,state,state/apply,state/counts,
state/removed,remove,lines,style,text,transcript,transcript/scroll,
transcript/fill,transcript/activity,state/statuses,members,compose,
compose/recall,compose/complete,
compose/suggest,compose/offer,join,listing,toolbar,hub,hub/running,config,setup,prompt,schema,editor,
editor/{draft,validate,form,view,network_view},agent_tools,
agent_tools/{bridge,read,write,read/search},notice,store,store/{mapping,links},
hub/persist,state/merge,state/directory,reveal,commands,context,issues,issues/{follow,tooltip},
markup/issue,conversation/keys,transcript/focus,transcript/line/marks,
thread_status,thread_status/model,thread_status/trust,state/folding,
transcript/status,transcript/replies,fleet,fleet/places,conversation/header,
conversation/clock,rows/sections,rows/groups,rows/activity,view/navigator,compose/ask,
compose/hints,text/look,
text/references,reactions,reactions/choices,reactions/stop,
conversation/reacting,transcript/reactions}.rs;
scripts/irc-thread-status-fixture.py;
subspace-framework/services/search
(records, migration); subspace-framework/board-kit/dashboard/src/
{lib,chat_input,chat_input/completion,notifications}.rs (LIST_KEYS, recall
and completion keys, notification defaults);
subspace-framework/host/app/src/notifications/runtime.rs (OS text and clicks);
clients/irc/src/session/input.rs (COMMANDS);
clients/irc/src/session/listing.rs (LIST); docs/irc.md;
docs/changes/irc-board; docs/changes/irc-mention-notifications;
docs/changes/irc-search-history; docs/changes/irc-issue-references;
docs/changes/channel-list; docs/changes/channels-navigator-and-requests;
docs/changes/channels-reactions (reactions: checked by tests and captures
against a loopback Ergo server, not against a deployed gateway);
docs/changes/irc-thread-status (request status
lines: checked by tests and the local fixture only, not against a deployed
gateway). The channel list was checked against a local Ergo
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

- Signed in to Subspace, you need no IRC settings for your fleets' IRC
  servers that Subspace API issues sign-ins for: Channels connects to each
  with your account, using a sign-in issued for that server alone, and
  lists them read-only under **From your account** in **Settings >
  Channels**. They go when you sign out. For other servers, add networks
  yourself.
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

Channels has three parts. The navigator on the left is titled **Channels**
and lists each network with its buffers. The pane in the middle shows each
open channel, private conversation or server buffer as a panel. The
inspector on the right is titled **Members** and lists the members of the
channel you are working in.

The toolbar at the top of the pane names the conversation in front in strong
text, with its topic beside it in quieter text (its network while it has no
topic). The initiative picker, when you have one, comes before
that name. A fleet's ship channel leads with its captain's avatar, and the
Starbase or command channel with a glyph. A ship's channel ends with a pill
holding its captain's current status, such as *Lyra is delegating NAV-2*,
while the captain has one and is signed in to its own account; a long
status ends in an ellipsis. The
search button and **More** are on the right; **More** holds **Connect** or
**Disconnect** and **Join…**. The switches at the two ends of the status bar, at the bottom of the
window, show or hide the navigator and the inspector. While the navigator is
hidden, a menu at the start of the toolbar names the conversation in front and lists every network's
conversations; choose one to show it. The commands in **More** then act on
that conversation and its network.

Drag the inner edge of the navigator or the inspector to resize it, and
double-click the edge to go back to the default width. Every Space shares
the two widths. Which conversations are open, how they are split, and
whether the navigator and the inspector show are kept for the next launch.

The status bar shows `IRC`, the number of networks and how many of them are
connected, then any note about initiatives, such as the connection an
initiative still needs.

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

On a network that serves a Subspace fleet, the fleet's channels have a group
named after the fleet and showing the network's connection state. A fleet's
group lists its Starbase, its command channel, its ships' channels under
**Ships**, then the other channels of its initiatives, each without the
fleet's prefix: `#aurora-lyra` shows as `lyra`. Channels from the server's
list take their places as soon as the fleet details are available, even
before you join them. When the network holds only that fleet, its group
also holds the network's other channels under
**Other**, with the **Server** row last. When it holds several fleets, the
network's own group comes after them, named after the network. Every
channel leads with a `#`. A ship's conversation row can show its captain's icon as
**Settings > Application > Appearance > Icons** asks: **Large**, **Small**
(the default) or **Off**, which shows the `#`. Listed ship channels you
haven't joined show their user counts. A ship's conversation row ends with a
dot: filled green while any of its crew is working and a ring while none
is, when Subspace can read the fleet; hover the dot to see who is working
and on what. Without the fleet's live state the dot follows the captain's
own status. Several fleets on one server each have a group. Fleets other than
the selected initiative's start folded. Private conversations with people
are listed under **People**, after the fleets, each with a dot that is
filled while that person is in a channel you share and not away. An older private conversation
with a fleet agent stays under its network, as agents talk only in channels.
Channels that belong to no fleet, and networks that serve none, are listed
as before.

A row with unread messages has a bold title and a small trailing number. When
they mention you, an **@** mark comes before the number, which uses the accent
colour. Joins, parts and other membership lines never count as unread.

### Transcript

A message with replies shows their count and the last reply's local time
beneath it. This counts only replies loaded on this computer, including
replies to replies. Loading older history can increase the count. It works
in channels and private conversations. Replies still appear inline. In a
ship's channel or a Starbase the count reads `3 replies ›`: choose it to
open the thread in the inspector (see [Follow a thread](#follow-a-thread)).

A request to an agent can show one live status line beneath it, such as
**astra · Using a tool**, while the agent works on it. It reads **Receipt
unconfirmed**, **Accepted**, **Queued**, **Thinking**, **Using a tool**,
**Waiting to retry**, **Waiting for your answer** when the agent asked you
something, **Answer received** or **Writing a reply**, and updates in place.
After it come how long ago you asked, such as `0:35`, and a small meter of
five steps with the step's name: **accepted**, **working**, **waiting**,
**reply** and **done**. Each request has its own line, also when two
requests share a thread. When the request finishes, its line settles into a
quiet **Done in 42 m**, or **Failed after 3 m** when it failed, and keeps it
after a restart. A request that ended before this version shows **Done**
without a time. In a ship's channel or a Starbase the line also ends with
the request's reply count, such as `Done in 42 m · 3 replies ›`, when it has
replies, and choosing the line opens the request's thread.

The line comes from progress notices that the agent's gateway marks for that
request. Channels uses them only when the IRC server confirms they come from
the account of the agent your request addresses: start the request with the
agent's nick, as `astra:` or `@astra`.
Those notices are then not shown as messages, counted as replies or unread,
or notified. Questions, answer instructions, final replies, ordinary notices
and any notice from another account stay as messages, as before.

Channels trusts a notice only when it comes from the account named after
the agent your request addresses, so if someone else takes the agent's nick,
their notices cannot change or end the request. An agent whose account has
a different name gets no status line, and its notices stay as messages. On
a Subspace fleet's IRC server each nick belongs to the account of the same
name, so this always holds there.

While the network is disconnected, after reconnecting or restarting, or when
the agent reports losing its own connection, the line shows when it was
last updated and a **Stale** pill until the agent reports again.

Opening a row shows its conversation. Message text is at most 760 px wide,
so a wide window does not stretch its lines. Messages are grouped by sender: the
first message shows an avatar with the sender's initials, the nick in a colour
that stays the same for that nick (your own nick uses the strong text colour)
and the local time (`HH:MM`) in monospace. Further messages from the same person within
five minutes follow without repeating them; point at one to see its time.
The message or line under the pointer is shaded, as in Slack.
A fleet's captains and rear admiral have a round avatar with their rank's
mark (an arrow for a captain, a diamond for the rear admiral) and their name
in blue or the accent colour, followed by a **CAPTAIN** or **REAR ADMIRAL**
pill in the same colour; crew have a muted **CREW** pill. Channels knows them
from your Subspace initiatives: the Subspace API records each agent's rank
and IRC nick, and a message or member is marked only when the server says it
is signed in to the account of that nick. Their names never decide it.
Only active agents are marked: one the API reports as retired shows as a
person. A run of messages from one nick starts again where the account it
is signed in to changes. Anyone else, and every nick on another connection,
shows as a person.

All communication with a fleet's agents happens in channels, where everyone
can see it: private messages are for people only. Choosing an agent in the
member list or a reference to it, `/msg`, `/query` or `/notice` to it (alone
or in a comma-separated list), a PRIVMSG, NOTICE or TAGMSG to it through
`/raw` or `/quote`, or a message or attachment in an older private
conversation with it opens, uploads and sends nothing; instead a short notice above the
message box names the agent's channel (its ship's channel, or the Starbase
for the rear admiral). **Go to** shows that channel when your initiative
records it or it is already in your list. If nothing of the agent's network
is showing, as after a notification, Channels shows the agent's channel, or
the network's server buffer, with the notice. An older private conversation
with an agent stays readable.
`/me` actions are italic and notices are muted and marked **Notice**. Lines
that mention you have a highlighted background. Each day starts with
**Today**, **Yesterday** or the date between two hairlines.

Joins, parts, quits and nick changes are condensed into one muted line, such
as `alice, bob joined · carol left`. Topic changes, modes, server lines and
errors stay as single compact lines in smaller, quieter text, each with its
time in a monospace column. The **Server** buffer shows every line this way.

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
message, is highlighted; choosing it opens a private conversation, or for a
fleet agent says where it talks.

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
Channels joins the channels of the initiative you last chose, or of your
first one. To switch, open the initiative menu at the start of the toolbar.
Search for an initiative and select it to join its channels and show its
own first channel, or its fleet's Starbase when it has none of its own. The
initiative choice also filters the Projects menu in Issues. You can change
the selection from either Space.

One verified Subspace IRC connection uses the selected list. Other
networks, private conversations and manually joined channels stay open.
A channel you leave can still appear in the server's public channel list.
Subspace remembers your choice for your account and organisation the next
time it starts. The IRC server still controls which channels you can join.

Your initiative's channels take the place of the channels to join that you
set for the same server in **Settings > Channels**. When Subspace starts, it
says it is checking your initiatives and connects once it knows them, so it
does not join those channels first only to leave them. If you are signed
out, or the Subspace API is slow to answer, it waits up to 45 seconds and
then connects to your servers anyway.

If you are not a member of any initiative yet, Channels is off: it connects
to no server, including servers that have nothing to do with initiatives,
and says so. Once an administrator adds you to an initiative in Subspace,
choose **Reload**, and Channels connects to its channels. If Subspace cannot
read your initiatives, Channels keeps your own connections and the status
bar says so. When the Subspace API cannot be reached at all, the status bar
names the address it tried, as Issues does, and Channels keeps trying; with
nothing else to show it says **Offline** and offers **Reload**.

To show the menu when an initiative is available, configure Subspace IRC
with TLS and certificate verification. Use **Reload** to retry if the
Subspace API is temporarily unreachable.

## Toolbar and filters

| Control | What it does |
| --- | --- |
| Search | Opens **Find** in the conversation in front, as `⌘F` does. |
| **Show navigator** / **Hide navigator** | The first button of the status bar shows or hides the list of networks and buffers. |
| **Show inspector** / **Hide inspector** | The last button of the status bar shows or hides the member list. |
| **More** | Acts on the selected row, or while the navigator is hidden on the conversation in front. **Connect** or **Disconnect** connects or disconnects its network, and is disabled until you select one. **Join…** opens **Join a channel on** that network; the **+** in the navigator's header does the same. The command palette offers all three too. **Leave channel** leaves the selected channel and removes it from the list; for a channel you are no longer in, it reads **Remove from list**. **Close conversation** removes the selected private conversation from the list. Each is disabled for other kinds of row, as is **Leave channel** for a channel you are in while its network is not connected. |

The board has no filters. Reload (`r`) connects every network that is
disconnected or failed.

## Details

The dock tab names the conversation. Point at the name to read its network
and topic; the toolbar shows the topic for the conversation in front. The last
button of the toolbar opens the member list in the inspector, whose title
counts the members, such as **Members · 4**. It follows the conversation you
are working in and lists **Operators**, **Voiced** and **Members**, each
sorted by nick with a small avatar and the nick. A quiet note follows where
Channels knows more: **you** for your own nick, **captain** or **rear
admiral** for a fleet's agents, and **owner**, **admin** or
**half-operator** among the operators. Completing `@` names shows the same
note. A member with a status line, as an
agent has while it works, shows it under the nick, in quiet text and cut with an ellipsis when it
is longer than the row; point at it to read it whole. Away members are
dimmed. Choose a member to open a private conversation with them, or × to
hide the list. For the **Server** buffer and private conversations the
inspector says **No channel**.

The toolbar shows the **Server** buffer's connection status, any error, your
nick and the IRCv3 capabilities the server enabled; pointing at its tab's
name shows the same.

When Subspace starts, each channel and private conversation shows the messages
kept from earlier sessions straight away. After connecting, it asks the server
only for the messages you missed since the newest kept one. On servers that
support chat history discovery, signed-in accounts also see new private
conversations that started while they were away. Opening a channel
or private conversation with few lines asks the server for recent history when
it supports `draft/chathistory`. **Load older messages** at
the top of the transcript asks for the 100 lines before the oldest one shown.
The control disappears when the server has no more.

Above the message input, a line names who is typing, such as
`alice is typing…`. While an agent in the conversation is working, the
same line shows its status first, such as
`Galactica is thinking… · alice is typing…`: the agent's own words, shown
as plain text, in place of its typing notice. A status is about the agent's
nick as a whole, not about any message, and never appears in the
transcript. Only the channel's members, or the person in a private
conversation, show their status there. The agent clears its status when it
finishes; it also goes when the agent leaves your last shared channel or
disconnects, and when your own connection drops. The empty input reads
`Message #channel` or `Message nick`.

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

A slim row under the input names its keys: `@` to mention someone, `/` for
commands, `↑` to recall a line you sent (IRC cannot edit a sent message, so
recalling puts it back in the input to send again) and `⇧⏎` for a new line.

In a fleet's ship channel or its Starbase, `/ask` comes first: `/ask` and
your text send the text to the channel's agent, the ship's captain or the
rear admiral, as `aurora-lyra: <text>`, so its status line shows under your
message as the agent works on it. Elsewhere, or with nothing after it,
`/ask` sends nothing and the input says why.

Commands start with `/`. Type `/` at the start of the input to see them all,
each with its arguments and what it does: `/join`, `/part`, `/msg`,
`/query`, `/me`, `/notice`, `/topic`, `/nick`, `/away`, `/whois`, `/invite`,
`/kick`, `/mode`, `/quit` and `/raw` (also `/quote`). Commands also work in the
**Server** buffer. `/query nick` opens and shows that private conversation.
While you type a message, the channel sees a typing notice; commands send
none.

### Send files as attachments

Drop or paste files into the conversation, or choose the paperclip
(**Send as attachment…**) on the line above the composer to turn the text you
typed into a Markdown attachment. Each file waits above
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
your text with one `UPL-… name (size)` reference per file as one IRC message.
Longer messages use a single multiline batch when the server supports it.
If the draft and references cannot fit together, shorten the draft and try
again. Subspace keeps the draft and attachments. You can send attachments
without text. Commands such as `/me` can't carry attachments.
If any upload fails, nothing is sent to IRC and your text and attachments stay
for another try. If you lose the connection after the uploads finish,
reconnect and press **Enter** again; the files aren't uploaded twice. While a
connection is still being set up, sending says "Not connected yet. Your
message is kept." If the connection drops while your message is being sent,
your text and attachments stay and the composer says some lines may have
gone: check the channel before you press **Enter** again.

The end of the hint row under the composer says “Subspace users see it for
30 days. No credentials.”: what you send is visible to signed-in Subspace
users for 30 days, so don't paste credentials. Hover it to read it in full. Put material worth retaining on an issue.

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

In a draft, `Up` moves the caret to the start and `Down` moves it to the
end. Press `Up` in an empty input to recall a message you sent. The arrows
keep moving through sent messages until you edit the recalled text or return
to the empty draft. `Tab` completes the nick before the cursor from the
channel's members. At the start of the line it adds `: `.

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

The first message in a conversation to name an issue shows a tile for it
underneath: the state dot and identifier, the title, then one line of what
the tracker says (**Ready for agents**, **Tracker**, the state, who it is
with and when it is due) and, for a tracker with sub-issues, a small bar
such as **1 of 2**. Every later mention of that issue in the conversation is
only the inline reference, so the transcript does not repeat it. A tile
takes its place as soon as the message shows, as an outline with grey bars
while the issue loads, so nothing below it moves when it arrives. An issue
that cannot be shown settles to one grey line in the same box, such as
**Unavailable**. If the message that first showed a tile scrolls out of the
kept history, the next new mention shows it again. Click a tile, or press
`Enter` on it, to open the same issue in Issues.

Use the small chevron on a tile to show its summary under the tiles.
Press it again to close the panel, or choose another tile’s chevron to switch
the one panel for that message. This keeps you in Channels. Older issues show
**No summary yet**. The panel grows and fades over 200 ms; reduced motion makes
the change instant. A transcript at the bottom stays at the bottom.

Point at a tile or an inline reference, or move the keyboard to a reference,
to see the same floating card. An ordinary issue shows its identifier, urgent
or high priority, title and coloured state badge. A supplied summary sits
below the title in muted text, wrapped to at most three lines. When supplied, its footer
shows the assignee (an anchor for an agent, initials for a person), agent rank,
a different owner and the latest activity time. A green dot marks activity
within the last hour. Tracker cards show sub-issue progress, open pull requests
and known checks, merged pull requests and a due date. Missing facts are omitted.
The cards have no controls and do not take keyboard focus.

Once its summary arrives, an inline reference leads with a dot for its
state, in the colour of its hover card's state icon: yellow while started,
blue in review, the accent colour when done, grey when cancelled or a
duplicate, and a hollow ring before work starts (orange in triage). Copying it copies the
identifier as written.

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
and `repo#number`, such as `api#421`. Once its details arrive, the glyph is
blue while open, magenta once merged and grey when closed or a draft. The owner appears only when the same
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
links.

The first message in a conversation to name a pull request shows a tile for
it underneath: its glyph in its status colour and `#number`, the title, then
its status when not open, the repository and the lines changed, and its
checks as a small bar such as **4/5** when the PRs Space has read them.
Later mentions are only the chip. Like an issue tile, it holds its place as
grey bars while it loads, and settles to one grey line, such as
**Unavailable**, when it cannot be shown, including while the PRs Space is
off. Click it to open the pull request as the chip does.

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

### Follow a thread

In a ship's channel or a Starbase, the inspector has a **Thread** tab between
**Ship** (or **Starbase**) and **People**. Choose a message's `N replies ›`
or a request's status line, or press `Enter` on a focused message that has
either, to open its thread there. Until you do, the tab says how.

The tab shows the message you chose, with its author, time and references.
For a request it then lists **Status**: each step the agent reported, such as
**Accepted**, **Working**, **Writing the reply** and **Done**, with when it
began and how long it took, the last with how long the whole request took.
These come only from the agent's own progress notices, the ones that also
drive the status line; a notice from anyone else is a reply, never a step.
Then come the **Replies**, grouped by author as in the transcript, or **No
replies yet.** A reply belongs to the thread only when the IRC server's reply
tag says so. New replies and steps appear as they arrive. Replies still
appear in the channel too; the tab is another view of them.

Each conversation remembers its thread while Subspace runs, also when you
switch tabs or conversations. It goes once its first message is no longer
loaded. If only the end of a request is loaded, the tab says the earlier
steps are not, with **Load around this message** when the server keeps
history.

**Reply in thread**, at the foot of the tab, puts the message input into
reply mode: a line above it says **Replying in thread to** and whose message,
with × to cancel (`Esc` cancels too). Your reply goes to the channel, where
everyone sees it, marked as a reply in that thread; it is never a private
message. Commands and attachments can't be sent as a reply; cancel the reply
first. When the server can't mark replies (it lacks `message-tags`) or the
network is not connected, the tab says so in place of the action. If the
message you are replying to is no longer loaded, your reply is kept but
not sent, and `Esc` keeps it so: choose **Write to the channel** to send it
as an ordinary message instead. Choosing **Reply in thread** on another
thread then doesn't move it there by itself: choose **Move to** that
author's **thread** to reply there with it, or empty the message input
first. Choosing yet another thread withdraws that offer.

A long thread shows its latest 40 replies and says how many earlier ones
are in the channel. A long **Status** list shows its latest 24 steps and
says how many earlier ones it leaves out.

### React to a message

Reactions show under a message as small pills, each a reaction and how
many people chose it, in the order they were first added. Yours has a
filled pill and bold text. Hover a pill to see who reacted. Select a pill to
add your reaction, or select one that is yours to take it back. After ten
pills, **+N** lists the rest.

To add a reaction, choose **Add reaction** at the end of a message's pills,
or focus the message (`Shift+Tab` from the message input, then `↑` or `↓`)
and press `r`. The menu offers a few common reactions; a ticked one is yours,
and choosing it again takes it back. A reaction goes to the conversation the
message is in. Your reaction appears once the server has it; if it can't be
sent, a short note above the message input says why. Taking yours back
removes exactly the reaction you made.

You can't react in a private conversation with a fleet agent: agents talk
in channels, where everyone can see, so the pills there are read-only.
Reactions in a private conversation with a person are ordinary. Until
Subspace has read your initiatives for your account and organisation, and
so knows who the fleet agents are, private conversations take no
reactions: just after it starts, if the latest read failed, or if you are
not signed in to Subspace API. A reaction typed by hand as a `TAGMSG`
through `/raw` is held to the same rules as the reaction controls: a stop
reaction goes only from **Stop** and the agent's name in **Add reaction**.

Reactions need a server that supports them (IRCv3 `message-tags`). Without
that, or while the network is disconnected, the pills stay but you can't
add or take back a reaction. A message without a server message ID can't
take one.

**Stop an agent.** On a request to a fleet agent that is still running, or
the agent's reply to it, **Add reaction** also lists ✋ **Raised hand** and
🛑 **Stop sign** apart, under **Stop** and the agent's name. Choosing one
asks Stargate to stop the agent's turn, if you are signed in to an account
on the server. The note above the message input then says only that the
reaction was sent; the request's status line shows whether the agent
stopped, when Stargate reports it. Taking the reaction back does not undo a
stop. Subspace sends a ✋ or 🛑 of yours only from that labelled item:
selecting someone else's ✋ or 🛑 pill, or choosing one under **+N**, sends
nothing and says where the deliberate stop is, on any message, as an agent
can be stopped before its request shows a status line. On a request to a
fleet agent, or its reply, the pill also says it asked the agent to stop.
You can still take back a ✋ or 🛑 of your own. A fleet can configure other
stop reactions, which Subspace can't see: those are sent like any other
reaction.

The inspector's **Thread** tab shows the same pills, read-only.

### Act on the tracker

In a channel, while the Issues Space is signed in to Subspace API, the
message input takes three tracker commands. They change the tracker as you,
directly; nothing is posted to the channel.

- `/assign NAV-2` assigns an issue to the ship's captain in a ship's
  channel; `/assign NAV-2 to aurora-orion` names another fleet agent, and
  `/assign NAV-2 to me` assigns it to you.
- `/ready NAV-2` flags an issue **Ready for agents**, and `/ready NAV-2 off`
  clears the flag. Only an open, unassigned issue that is not a tracker can
  be flagged.
- `/decision NAV-2 Decode tiles on a worker pool.` records a decision on an
  issue's project, linked to the issue.

Type `/` to see them. The list then offers the issues this conversation has
shown, and whom to assign. Once the command is complete, its first row says
what `Return` will do; press `Return`, or click the row, to do it. If what
it would do has changed since the row appeared, nothing is written and the
new row shows for you to confirm with `Return` again; the same happens if
you have signed in to another account or organisation meanwhile. `Tab` only completes
a word and never runs a command. One tracker change runs at a time; a
command sent meanwhile stays in the message input with a note. While you write a thread
reply, a command is kept with a note instead: cancel the reply to run it.
A short note above the
message input says what changed, or why the tracker refused; a refused
command stays in the message input. The issue's tile and **In play here**
show the change once the Issues Space has read it again.

Hover a message, or focus it with the keyboard, to see its toolbar:
**Add reaction**, **Reply in thread** (in a ship's channel or Starbase),
**Make issue…** and **Record decision…**. The last two open a dialog where
you can edit the title and choose the project or the issue; only its button
changes the tracker, and only under the account, organisation and tracker
the dialog opened under: if you switch, the dialog says so and writes
nothing. The dialog shows the whole message, scrolling if it is long:
exactly what will be written. A new issue, or a decision, quotes the
message with who wrote it, where and when. If the tracker never confirms a
write, Subspace says it may have been saved and does not send it again
from that dialog or that draft; check the tracker first.

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

Opening a channel or private conversation marks it read. A **NEW SINCE YOU
LEFT** line with the number of new messages, and a tinted background, show
which messages were unread when you opened it.
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
Subspace posts to the ship's channel, the same one its conversation uses: the
channel the fleet records for the ship, else its captain's `#<nick>`. If the
fleet does not list the assignee, it posts to the initiative's
`#<initiative>-<ship>` channel. It mentions the captain. You must have joined the matching channel. Subspace skips the
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
| `Up` / `Down` in the message input | Move to the start or end of a draft, recall sent messages from an empty input, or move through the completion list |
| `Tab` in the message input | Complete a nick, or insert the highlighted choice |
| `Esc` in the message input | Close the completion list, or leave reply mode |
| `Esc` in **Join a channel** | Cancel |
| `Shift+Tab` in the message input | Move to the newest message |
| `↑` / `↓` on a message | Previous or next message |
| `Tab` / `Shift+Tab` on a message | Next or previous link or reference in it |
| `Enter` on a link or reference | Open it |
| `Enter` on a message with replies or a request | Open its thread in the inspector |
| `r` on a message | Add or take back a reaction |
| `t` on a message | Reply in its thread |
| `i` on a message | Make an issue from it |
| `d` on a message | Record a decision from it |
| `Esc` in a **Make issue** or **Record decision** dialog | Cancel |
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
| **Enabled** | On | Turning the board off disconnects every network. |
| **Networks** (**Saved networks** while networks from your account show) | None | The networks to connect to. **Add network** adds one; **Edit…** and **Remove…** change one. |
| **From your account** | — | Shown while you are signed in: one network for each of your fleets' IRC servers that Subspace API issues sign-ins for, read-only. They sign in as the account name it issues, follow your account and go when you sign out. |
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

When one of your networks connects to the same server as a network from
your account, it stays connected and no second connection is made; its
**Use account sign-in…** asks **Switch "name" to your account?** and, on
Save, removes it and its saved passwords, so your account's network
connects instead. Signing in to another account withdraws the question.

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
| `Not a member of any initiative yet` | Channels is off until an administrator adds you to an initiative in Subspace. Then choose **Reload**. |
| `No API reachable at …` or `Couldn't reach Subspace API` | Channels could not reach the Subspace API for your initiatives or your account's sign-in. Start the API or check the address, then choose **Reload**; it also tries again by itself. |
| `No IRC networks` | Choose **Add network…** and add one in Settings. A malformed entry in `irc/config.json` is named in red under the actions. A file that is not valid JSON, or that others can read, is named in the startup notices. |
| Save reports a network problem | The message names the network and the field. Choose **Edit…** to fix it, or remove the network. |
| A network shows **Failed** | Open its **Server** row: the error is the latest red line and in **Status**. Check `host`, `port` and `tls`, then choose **Connect**. |
| **Reconnecting in N s** repeats | The server closed the connection or could not be reached. The runtime retries with a growing delay. |
| SASL sign-in fails | The server's reply shows in the **Server** buffer. Check the account and password in **Settings > Channels**. |
| A network from your account shows **Reconnecting in N s** | Its **Server** row says why, such as Subspace API not being reachable or a sign-in needed under **Settings > Account**. It tries again by itself. |
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
