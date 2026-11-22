# Channels (IRC)

The Channels Space (`spaces/irc`, board id `irc`) is an IRC client with IRCv3
support. It was called IRC until 0.4.0; its id, crates, settings file and
stored data keep the `irc` name. The protocol, connections and session state live in
`clients/irc` (`subspace-client-irc`); the board applies the runtime's events
and sends it commands. The in-app guide is
[spaces/irc/help.md](../spaces/irc/help.md).

## Initiative channel sets

The process Hub reads `GET /api/v1/me/initiatives` with the same Subspace API
client as Issues. Only the caller's latest successful memberships are used;
an access error, verified user change or empty result falls back to Local channels. The picker is
shared across windows through the Hub and resets when you quit.

The selected list replaces autojoin on exactly one connection: the one on the
selected initiative's IRC server. An initiative assigned to a fleet uses that
fleet's `ircHost`, `ircPort` and `ircTls` from Subspace API, so switching to an
initiative of another fleet moves its channels to that fleet's connection;
another initiative on the same server stays on the current one. Initiatives
without a fleet use `SUBSPACE_IRC_ENDPOINT` (`irc://host:port` or
`ircs://host:port`, by default `irc://127.0.0.1:6667`). While an assigned
fleet's server is unknown, for example because its fleet could not be read,
the initiative's autojoin stays off rather than using that default: the
header says so and the Hub reads the memberships again, waiting 5 seconds at
first and doubling the wait after each failure, up to 5 minutes. A TLS connection must
verify certificates unless its host is loopback. When no connection matches,
the header names the server to add in **Settings > Channels**; the desktop
does not create connections, because IRC needs that server's SASL account.
Other IRC connections never receive initiative channel names. Capture can
use a numeric loopback IRC address when the debug API is also a validated
loopback fixture. The network task retains the selected list while it
reconnects. The board closes tabs retired by a switch and preserves cached
message history and manually joined channels.

See the [change record](changes/subspace-api-accounts/README.md).

## Fleet roles

Channels reads fleet roles from the same memberships (`fleet.rs`). A
connection carries a fleet's roles when it is the one an initiative of that
fleet would use, by the rule above; any other connection, such as a
local-only one, has none. A fleet that several initiatives share counts once,
by its id. The hub builds each network's `Roster` once and forgets it when
the catalogue or the configured networks change.

An agent's rank comes from the Subspace API, never from its name. The
fleet's `agents` give each agent's `rank` (`rearAdmiral`, `captain` or
`crew`) and, when the API knows it, its `ircNick`, which fleet IRC servers
tie to the account of the same name. An agent without a known nick is not
marked, and only active agents are: one the API reports as not `available`,
such as a retired captain, has no rank and is treated as a person. A sender, member or completion row has a rank only when its server
account equals that nick, ignoring ASCII case (`Roster::verified`), the same
rule as request status trust; a missing account is unknown. A transcript
group of one sender breaks where the server account changes, so a
continuation never sits under an avatar and rank the account no longer
earns. Nicks and channels compare ignoring ASCII case, as Ergo's `ascii`
casemapping does. The roster indexes agents and channels by case-folded
name and each fleet's captains by ship.

The fleet's channels come from its record: `starbaseChannel`,
`commandChannel` and each ship's `channel` (repeated on its captain as
`shipChannel`). Without a record the standard names stand in:
`#<slug>-starbase`, `#<slug>-command` and, for a captain, `#<nick>` (the
installer names a captain's nick and ship channel both `<slug>-<id>`). A
crew member talks in its ship's recorded channel, else in that of its ship's
one active captain (a ship with several has none), and the rear admiral in
the Starbase. An initiative joins its fleet's recorded channels, then its own
(`initiative_channels`); a guessed standard name is never joined. Choosing
an initiative shows its own first channel, else its fleet's recorded
Starbase, else the first channel it joins (`focus_channel`), so two
initiatives of one fleet each land in their own place.

Nothing is guessed. A nick or channel that two roles claim, such as the rear
admiral and a captain with one nick, or a ship whose channel is the
Starbase's, is unknown and draws as a plain person or channel, and a ship
channel whose captain is unknown is a plain channel.

- **Transcript.** A captain or the rear admiral has the shared agent avatar
  (`avatar::agent`) in the gutter and a name in blue or the accent, followed
  by a `CAPTAIN` or `REAR ADMIRAL` pill (`badge::Pill::hue`) in the same tone,
  named *captain* or *rear admiral*. Crew keep a person's avatar and tone
  and a muted `CREW` pill.
- **Members and completion.** The member list shows the small agent avatar
  as the app's **Icons** setting asks (`avatar::agent_icon`: as drawn for
  Large, at a row glyph's size for Small, none for Off, the name closing
  up) and its note, and the `@` completion row's trailing mark, says `captain`,
  `rear admiral` or `crew`.
- **Heading.** A ship's channel leads with its captain's small agent avatar,
  the Starbase and the command channel with a glyph, and a ship's channel
  shows its captain's status (see [Design](#design)).

### The navigator by fleet

The navigator groups a network's buffers by fleet (`rows.rs`,
`rows/sections.rs`, `rows/groups.rs`). The roster carries each fleet on the
network (`fleet/places.rs`, `FleetPlace`): its API id and name, its slug, and
the channels it claims, being its Starbase, command and ship channels as the
roster names them and the channels its initiatives name. A channel that two
fleets on one network claim belongs to neither.

- **One group per fleet**, named from the API, keyed
  `irc␟<network>␟f␟<fleet id>`, holding that fleet's channels: the Starbase,
  the command channel, the ships, then the rest, each by the name the row
  shows. A fleet's rows drop the `#` and the `<slug>-` prefix
  (`#aurora-lyra` shows as `lyra`); search still finds the full name. A
  fleet with no buffer open has no group. The header carries the network's
  connection state, as the network's own header does.
- **People**, keyed `irc␟<network>␟p`, follows the fleets on a network that
  serves one and holds the private conversations with people. A private
  conversation with a nick the roster knows as an agent stays in the
  network's group, so history stays reachable but `People` never offers an
  agent. With more than one network it reads `People · <network>`.
- **The network's group** keeps its key and holds the server row, the
  channels of no fleet, those older private conversations and the listed
  channels. A network that serves no fleet has only this group, as before.
- Row keys are the buffers' own (`rows::row_key`), so selection, the dock
  layout (`dock/layout.rs`, which stores buffers) and search results restore
  as before; only group keys are new. `Irc::group_networks` maps each group
  key to its network for the toolbar, Join and the list's menu.
- Rows are plain, as the owner asked: every channel row, the Starbase and
  the command channel included, leads with the muted `#` (`view::kind_icon`),
  and a person in `People` with a presence dot (`avatar::activity_dot`,
  filled while they are in a channel we share and not away, a muted ring
  while away or in none; `rows::activity::presence`). A ship's row leads
  with its captain's agent icon as the app's **Icons** setting asks
  (`avatar::agent_icon`): **Large** the small agent avatar
  (`avatar::agent(...).small()`), **Small**, the default, the same avatar at
  the `#`'s size, **Off** the `#`.
- A ship's row ends with one dot (`view/navigator.rs`, `ship_dot`): filled
  green while the ship is working, a muted ring while not, named
  *aurora-lyra: working* or *aurora-lyra: idle* (the row's spoken name ends
  with the same words), and showing Kit's tooltip on hover: the captain's
  status today. Whether a ship is working comes from one function,
  `rows::activity::ship`: today the captain's nick status while the server
  vouches for it, by the heading pill's rule (`conversation::vouched_status`:
  a member with the captain's nick signed in to its account), so nothing is
  invented without one. Crew activity joins `Activity::working` there once
  the Space can read it, and the tooltip then lists the crew at work.
- Every row with unread messages is emphasised. A mention adds an `@` mark
  in `Tone::Red` before the count, which keeps the accent.
- A fleet group that is not the selected initiative's fleet starts folded
  the first time it appears (`view/navigator.rs`, `fold_new_fleets`); with no initiative
  selected every fleet starts open. Revealing a row in a folded group
  unfolds it, and later syncs leave it as the user left it.
- The shared list has no caption inside a group, so `People` is a group of
  its own and the ships have no `Ships` caption.

See the [change record](changes/channels-navigator-and-requests/README.md).

### Direct messages are for people only

All communication with a fleet's agents happens in channels, where everyone
can see it (an owner decision). When a private conversation would start with
a nick the roster knows as an agent, Channels opens, uploads and sends
nothing and shows a notice above the composer (`compose/agents.rs`) naming
the agent's channel, with **Dismiss** and, when the API records that channel
or the user's channel list holds it, **Go to** that channel; a standard name
standing in is only named, as joining it would create it. The composer
checks what a draft would send, through the IRC client's
`private_recipients`, the same parse the session sends it with: every nick
of a `/msg`, `/notice` or `/query` target list, the PRIVMSG or NOTICE that
`/raw`, `/quote` or an unknown command carries, and text, `/me` or
attachments in an older private conversation; a channel target is never
refused. The member list, a nick reference and a notification for a closed
conversation check the nick, and `irc_write` refuses a message to an
agent's nick. When nothing of the agent's network is in front, as after a
notification click while another network shows, Channels shows the agent's
channel if the list holds it, else the network's server buffer, with the
notice there. An older private conversation with an agent stays readable.
This is a client-side rule: the server still delivers a private message
sent another way. Stargate
answers one unless its registration sets `dmPolicy: "redirect"`, which makes
every connection answer a direct message with a notice pointing to its
first channel and start no turn.

See the [change record](changes/channels-fleet-roles/README.md).


## Configuration

The `captainNotifications` switch is on by default. See
[Notify ship captains](../spaces/irc/help.md#notify-ship-captains).

Networks live in the board's own settings file, `irc/config.json` in the
folder of Subspace's `config.json`. With the config file at
`~/.config/subspace/config.json`, it is `~/.config/subspace/irc/config.json`.
The file's top-level object holds the IRC settings. Whether the board shows
(`boards.irc`) stays in the main `config.json`. There are three ways to
change the networks:

- **Settings > Channels** lists the networks with **Add network**, **Edit…** and
  **Remove…**, and has the board-wide **Highlight words**. Saving the network
  form preserves per-network `highlights` values. Settings keeps edits in a
  draft until you choose **Save**. It validates each network and names the
  network and field that need fixing.
- **Copy agent prompt**, on the IRC Settings page and on the board while no
  network is configured, copies a self-contained prompt for a coding agent:
  the absolute paths of this computer's IRC settings file and of the main
  config file that enables the board, the rules for editing it (valid JSON,
  keep other keys, keep mode `0600` and the folder's `0700`, ask for passwords
  and never echo or commit them), the schema below and how to verify the
  result. The
  prompt is written from `spaces/irc/src/schema.rs`; a test keeps that file
  equal to the serde names of `NetworkConfig` and `Sasl`.
- Edit the file yourself. **Show in Finder** (**Show in File Explorer** on
  Windows, **Show in Folder** on Linux), on the same two surfaces, shows the
  IRC settings file, or the nearest existing folder above it before the first
  save. **Settings > General** shows the main config file.

A Settings save applies at once: the board's runtime hub observes the saved
`Services` config, sends the runtime the new networks, and the runtime
restarts only networks whose configuration changed. Unchanged networks stay
connected and keep their buffers; removed networks disconnect and their
buffers leave the list. A file edited outside Settings is read at the next
start.

For the local stack (`subspace-irc/` in this repository), add a network for
`127.0.0.1`, port `6667`, TLS off, with SASL set to the Ergo account and
password the stack provisioned; Ergo forces the nickname to the account
name, and only an operator can create channels. A shared or remote fleet
server uses its host name, port `6697` and TLS with certificate checks.

```json
{
  "networks": [
    {
      "id": "local",
      "name": "Local Ergo",
      "host": "127.0.0.1",
      "port": 16667,
      "tls": false,
      "nickname": "subspace",
      "sasl": { "username": "subspace", "password": "…" },
      "channels": ["#subspace"]
    }
  ],
  "highlights": ["subspace"],
  "captainNotifications": true
}
```

Each network accepts:

| Field | Default | Meaning |
| --- | --- | --- |
| `id` | required | Stable key, such as `libera`. Settings derives a new network's id from its name (`Example Network` becomes `example-network`, with `-2` and so on when taken) and never changes a saved id. |
| `name` | the `id` | Name shown as the network's group. |
| `host` | required | Server host name or address. |
| `port` | `6697` | Server port, 1 to 65535. |
| `tls` | `true` | Use TLS. |
| `acceptInvalidCerts` | `false` | Accept any certificate. Only for local test servers. |
| `caCert` | none | PEM CA certificate(s) trusted for this server besides the system roots, such as a private network's own CA. |
| `nickname` | required | Preferred nick. |
| `altNicks` | none | Nicks to try when the preferred one is taken; else an underscore is appended. |
| `username`, `realname` | the nick | Sent at registration. |
| `password` | none | Server password (`PASS`), not the account password. |
| `sasl` | none | `{ "username": …, "password": … }` for SASL PLAIN, `{ "username": …, "passwordFile": … }` to read the PLAIN password from a file at each connect (used when `password` is empty), or `{ "username": …, "external": true }` for SASL EXTERNAL. |
| `channels` | none | Channels joined after every connect. Each starts with `#` or `&`. |
| `autoConnect` | `true` | Connect at startup. |
| `highlights` | none | Extra highlight words for this network only. |
| `clientCert`, `clientKey` | none | PEM client certificate and private key paths for TLS and SASL EXTERNAL. The key defaults to the certificate file. |

Settings checks the same rules before it saves: a host and a nickname are
required, the port is 1 to 65535, nicks start with a letter or one of
`` [ ] \ ` _ ^ { | } `` and have no spaces, channel names start with `#` or
`&`, file paths are absolute, PLAIN needs an account and a password or a
password file, and EXTERNAL needs TLS and a client certificate. Keys the form
does not show, in the network object, its `sasl` object or the file's
top-level object, are kept as they are.

The top-level `highlights` lists extra words that highlight a message on every
network besides your nick. The board matches them itself, so changing them
reconnects nothing. Words match whole words, ignoring case and formatting.
Topic, mode and membership lines never highlight.

A malformed network entry is skipped and named on the board's empty state;
the other networks still connect. A file that is not a JSON object is ignored
with a startup notice naming it, and Settings names it instead of showing the
IRC page's editor. A file that other users can read or write is ignored with
a startup notice that says to run `chmod 600`; a save from Settings rewrites
it owner-only.

`irc/config.json` is owner-only (`0600`) in an owner-only folder (`0700`);
Settings writes both that way. Passwords stay in it and go only to the
runtime. Settings never copies a saved password into an input: a saved
password is kept until you clear or replace it, and removing a network removes
its passwords on Save. Passwords never enter UI state or `state.json`.

## IRCv3 support

The runtime negotiates with `CAP LS 302` and requests, when the server offers
them: `cap-notify`, `sasl` (PLAIN, EXTERNAL), `server-time`, `message-tags`,
`batch`, `echo-message`, `labeled-response`, `away-notify`, `account-notify`,
`account-tag`, `extended-join`, `multi-prefix`, `userhost-in-names`,
`invite-notify`, `setname`, `chghost`, `standard-replies`,
`draft/chathistory` (with `draft/event-playback`), `draft/read-marker` and
`draft/multiline` and `draft/metadata-2` (requires `batch`), plus the
`+typing` client tag and reactions (`+draft/react` and `+draft/unreact`
with `message-tags`; see [Reactions protocol](#reactions-protocol)). It
reads ISUPPORT
(`CHANTYPES`, `PREFIX`, `NETWORK`, `CASEMAPPING`, `CHATHISTORY`), answers
`PING`, falls back to another nick on collision and reconnects with backoff.
Opening the TCP connection and the TLS handshake each time out after 15
seconds and are retried; **Disconnect** abandons an attempt in progress.

Channels and private conversations follow the server's `CASEMAPPING`: `#Room`
and `#room` (and, under `rfc1459`, `#[x]` and `#{x}`) are one buffer, shown with
the first spelling the runtime saw, which it keeps across reconnects. Member
updates carry each member's stored spelling.

What the board does with them:

| Capability | Board behaviour |
| --- | --- |
| `server-time` | Line times and day separators use the server's time. |
| `draft/chathistory` | Joining a channel, or registering with a stored private conversation, asks only for what was missed since the newest stored line (`LATEST <target> msgid=…`, up to 500 lines), else the latest 100. Opening a buffer with fewer than 50 lines loads recent history; **Load older messages** loads 100 more. Opening a search result the buffer does not hold asks for the 50 lines around it (`AROUND`). Without the capability, the first request reports that there is no history and the row disappears. |
| `draft/read-marker` | Opening a buffer sends `MARKREAD`; markers from other clients clear unread counts. |
| `+typing` | Typing notices are sent at most every 3 seconds while you type and `done` when you clear the input; others' notices show above the input and expire after 6 (active) or 30 (paused) seconds. |
| `draft/metadata-2` | The client subscribes to nickname `status` after registration and emits `Event::NickStatus { network, nick, status }`. The Space holds each network's statuses and shows them above the input and in the member list; see [Agent status lines](#agent-status-lines). |
| `multi-prefix` | The member column groups members by their highest prefix. |
| `away-notify` | Away members are muted in the member column. |
| `echo-message` | Your own lines appear once, as the server delivered them. |
| `message-tags` (`msgid`) | A line that live delivery, history and the store all carry is shown and stored once; lines without a `msgid` match by time, sender and text. |

The Server panel lists the capabilities the server enabled.

### Nickname status protocol

After `draft/metadata-2` and `batch` are acknowledged and registration
completes, the runtime sends `METADATA * SUB status` once per capability
activation. Without those capabilities it sends no metadata commands.
It reads Ergo 2.19.1's `761 <recipient> <nick> status * :<value>` and
`766 <recipient> <nick> status :<explanation>`, and the draft's
`METADATA <nick> status <visibility> [:<value>]` (no value clears).
Channel metadata and other keys do not change nickname statuses.

`metadata` batches stream normally without blocking other traffic. Their
target can be a channel even when the enclosed values belong to nicknames.
Subscription numerics 770–773, metadata error numerics and `FAIL METADATA`
produce diagnostics without conversation lines. A 774 sync-later reply
requests `METADATA <target> SYNC` after its delay, including Ergo's
`METADATA *ALL SYNC` for the initial snapshot. Positive delays run on the
connection's existing ten-second tick; removing the capability cancels them.

Each network holds current values using its own `CASEMAPPING`. Only a set,
change or clear emits `Event::NickStatus`; repeated equal values do not.
Events use the held value's first nickname spelling. Values clear on QUIT,
NICK, deletion, loss of the final known shared channel (PART or KICK by
either side), capability removal and connection end. Each clear is emitted
to consumers, and reconnect carry data contains no statuses. Values learned
outside known channel membership are retained until another clear rule
applies; no extra membership tracking is introduced.

At ingestion, a pure function strips IRC formatting and colour arguments,
removes control and bidi override characters, replaces line separators with
spaces, trims and limits text to 80 Unicode characters including a trailing
ellipsis when cut. The workspace has no segmentation dependency, so this
uses characters rather than grapheme clusters. Empty cleaned text clears.
Markdown, URLs, mentions and issue references remain literal plain text;
consumers must not interpret them as markup or interactive content.

See [the status protocol change record](changes/irc-agent-status/README.md)
for sources, tests and delivery gates.

### Agent status lines

The Space shows what the runtime reports. `Event::NickStatus` sets, replaces
or clears a nick's status in `Network::statuses` (`state/statuses.rs`): one
map per network, because a status describes the whole nick, never a buffer
or a message. It is never stored: the hub's store keeps lines only, and the
reconnect carry data has no statuses. The value is untrusted, so the
reducer cleans it again with the client's own cleaner,
`subspace_client_irc::status_text`, rather than relying on the runtime: one
plain line of at most 80 characters, where nothing left means cleared.
Nicks compare ignoring ASCII case, as `members::note` does, because the
Space does not know the network's case mapping and the runtime reports a
status under the first spelling it saw.

- **Above the input.** `transcript/activity.rs` composes the line that used
  to show only who is typing. A conversation shows the statuses of the nicks
  in it: a channel's members, or a private conversation's peer, never our
  own nick and never anyone else. The status texts come first, verbatim and
  joined with ` · `, then the typing notice for the nicks without a status,
  so an agent that has a status and is also typing shows its status only.
  The line keeps its height, shows plain text (no Markdown, links, mentions
  or issue references), is cut with an ellipsis and offers the whole text
  in a tooltip (`truncated::text`). Its accessible name says whose each
  status is, such as `Status of galactica: Galactica is thinking…`. With no
  status, the line reads exactly as the typing line did.
- **In the member list.** A member with a status has a second, quiet line
  under the nick in the same row, one line cut with an ellipsis, and the
  row's accessible name adds it: `Open private messages with galactica,
  Galactica is thinking…`, then `, away` as before.
- **Clearing.** A status goes when the runtime clears it (the gateway ends
  the turn, or the nick quits, changes nick, leaves the last channel we
  share or the capability goes), and the Space also drops:
  - every status of a network as soon as its connection is anything but
    connected, so none survives a reconnect even if the runtime's clears
    were missed;
  - a nick's status when it leaves a channel (`MemberLeft`, or the old nick
    of `MemberChanged`) and no channel of that network we hold still lists
    it. This matches the runtime's rule for the last shared channel. A
    private conversation's peer that shares no channel with us has no
    channel departure, so it keeps its status there.

  A line from the nick ends its typing notice but not its status, which the
  gateway clears itself. See
  [the agent status change record](changes/channels-agent-status/intent.md).

### Reply protocol foundation

The runtime preserves `+draft/reply` as `Line::reply_to` on `PRIVMSG`,
`NOTICE` and actions, in channels and private conversations, live and in
history. Multiline messages take the reference exclusively from the opening
`BATCH`, as required for client tags by the
[multiline specification](https://ircv3.net/specs/extensions/multiline).
The existing deployed-server proof uses this draft wire tag; this slice
continues to use `+draft/reply`.

`Line::thread_root` follows the relationships in loaded lines of the same
buffer. A reply to a reply resolves to the earliest known ancestor; cycles
return no root. Resolving again after loading more history can complete the
chain. The original relationship survives the local store, and older stored
lines without the optional field still load. There is no count or server
lookup in this foundation; the reply-search follow-up can later supply server results.

`Line::reply(text, loaded, message_tags)` prepares `Command::Reply` only for
a conversation message with a nonempty server `msgid` and `message-tags`
enabled. The command sends literal text, using the resolved root as its
`+draft/reply` value. The session rechecks `message-tags` and a nonempty ID,
escapes the value through the protocol serializer and rejects values that
cannot fit in an IRC tag. Multiline replies tag only the opening `BATCH`;
without multiline, each split message carries the reference. Local echoes
retain it. Ordinary composer and agent sends are unchanged.

See the [protocol change record](changes/irc-reply-tags/README.md) for tests
and proof commands.

The runtime also keeps a short allow-list of client-only tags (`+` prefix)
of a `PRIVMSG` or `NOTICE` as `Line::client_tags`, live and in history, so
a Space can read a machine-readable marker that another client added:
`subspace_client_irc::KEPT_CLIENT_TAGS`, today only `+stargate/progress`
(`PROGRESS_TAG`), with values of at most 64 bytes. Any sender can add
client-only tags, so every other one, and a longer value, is dropped rather
than held and stored. A multiline message takes them from its opening
`BATCH` only. They are stored with the line.

### Reactions protocol

The runtime sends and reads IRCv3 reactions (the work-in-progress
[react client tag](https://ircv3.net/specs/client-tags/react)): a `TAGMSG`
to the conversation with `+draft/react=<reaction>` or
`+draft/unreact=<reaction>`, and `+draft/reply=<msgid>` naming the message.
The Space does not show them yet; this is the client layer only.

- **Support.** Reactions need `message-tags`, which the runtime already
  requests, and no `CLIENTTAGDENY` that blocks `draft/react`,
  `draft/unreact` or `draft/reply`. The runtime reports `Update::ReactionSupport { network,
  supported }` with every capability report and when `CLIENTTAGDENY`
  changes, so a UI can hide reactions. `reactions::reactions_supported`
  answers from the capability list alone.
- **Sending.** `Command::React` and `Command::Unreact` (checked first with
  `reactions::check_reaction`, which returns the same typed error) send
  exactly one `TAGMSG`, such as
  `@+draft/react=👍;+draft/reply=<msgid> TAGMSG #channel`. Without a
  registered connection, support, one valid destination (a channel the
  server calls a channel, or a nick that is not one, without spaces or
  commas), a valid `msgid` (at most 128 bytes, no spaces or controls) or a
  valid reaction (1 to 32 characters, no controls or surrounding spaces)
  nothing is sent, and the runtime answers `Update::ReactionRefused` with a
  `ReactionError`. A reaction for a network that is not configured is
  refused as `NotConnected`. Without `echo-message` the runtime echoes our
  reaction locally, but only after the write succeeds; a failed write is
  answered with `ConnectionLost` (it may or may not have arrived).
- **Receiving.** A reaction is a `Reaction` (message, reaction, added or
  removed, nick, account, ours, time, live or history, and the network's
  case mapping), reported as
  `Update::Reaction`. Updates come in one order with the events through
  `Runtime::drain_updates`; `Runtime::drain` still returns events only, so
  existing owners are unchanged (it drops the updates, so use one drain or
  the other). Reactions are not new `Event` variants because the Channels
  Space matches `Event` exhaustively. Network input is untrusted: a `TAGMSG`
  with both tags, without a nick or reply `msgid`, or with an invalid value
  is ignored. A reaction is ours when its account is ours, both being
  known (our account comes from `RPL_LOGGEDIN`), else when its nick is our
  current nick. `+reply` is read when `+draft/reply` is missing, as Stargate
  does; unprefixed `+react` is not read, as the draft says.
- **Aggregation.** `reactions::ReactionBook` holds, per buffer and
  `msgid`, each reaction and who reacted (nick, account, ours) in
  first-reacted order: `reactions(&buffer, msgid)` and `reacted(&buffer,
  msgid, reaction)`. It keeps the latest operation per message, reaction
  and person by time, so an older replay never overwrites a newer change,
  and remembers removals for that. Each person's place is when their
  current addition started; an older replayed addition can only move a
  shown person's place earlier. When a message's places for remembered
  removals run out, the oldest group holding only removals is dropped. It follows nick changes through
  `observe` (`Event::MemberChanged`) or `rename`, skipping entries with
  another known account and merging nobody, and keeps reactions to
  messages not loaded yet. Each reaction's `stops_agent` marks Stargate's
  default stop reactions. People are the same when both are ours, when both
  have the same account, or when neither has an account and their nicks
  match under the network's case mapping. It keeps at most 2,048 messages
  (dropping the least recently changed), 20 shown reactions per message and
  100 shown people per reaction, with as many again remembered removals.
  Local echoes carry the local clock, so a large clock skew against the
  server can misorder our own operations.
- **Agent stop reactions.** `reactions::agent_stop` describes, as data only,
  the reactions that stop an agent in Stargate (`main` at `da242f8`, the
  same behaviour as v0.43.0): ✋ and 🛑 by default, ignoring skin tones and
  the variation selector, as a `TAGMSG` naming the message with
  `+draft/reply` (or `+reply`), from a person with an account, on the
  request or an agent message answering it, while the turn is active and
  the gateway's `reactions` feature is on. History reactions never stop a
  turn, and an unreact does not undo a stop. A fleet can configure other
  stop reactions in `irc[].stopReactions`, which the client cannot see.

Ergo 2.19.1 relays a `TAGMSG` only to clients with `message-tags`, and
echoes it only with `echo-message` as well. It stores a `TAGMSG` in history
only when it carries a tag listed in `history.tagmsg-storage.whitelist`,
and replays it in `chathistory` only to clients with
`draft/event-playback`, which the runtime requests. Our `ircd.yaml.tmpl`
lists `+draft/react`, `+draft/unreact` and `+react`, so removals are
replayed too. History recorded before `+draft/unreact` was listed holds
additions without their removals, so a reaction taken back then can
return when that history loads, unless the client saw the removal.

See the [reactions change record](changes/irc-client-reactions/README.md).

### Loaded reply counts

A root message with loaded replies shows their count and the latest reply's
local time below its text, in channels and private conversations. Replies to
replies count on the root. The count covers only the messages this client
currently holds; loading older history or restoring the store rebuilds it.
The reply-search follow-up will add the server lookup. An unloaded root gets
no count until it loads, and cyclic relationships get no count.

`ThreadRoots` indexes a buffer's parents once for all its counts, preserving
`Line::thread_root`'s cycle and missing-parent behaviour. Each conversation
retains the resulting summaries and shares them across its virtual rows.

Replies remain inline, so every message stays readable. Counts are read-only;
the pane, reply action, composer and hiding replies from the room come in PR 3.
Unread and notification behaviour are unchanged in this slice. See the
[count change record](changes/irc-reply-count/README.md).

**Join…** is available only while the selected network is connected; its
menu entry says why otherwise. Commands that need the server while a network is
disconnected or still connecting (input, joins, leaving a channel) leave a
**Not connected** error line instead of being dropped. Your user modes read as
`Your user modes are +Zi` (`RPL_UMODEIS`) or `Your user modes changed: +w`.

### Request status lines

Stargate reports progress on a request as a `NOTICE` carrying
`+stargate/progress=<stage>` (`thread_status::PROGRESS_TAG`) and
`+draft/reply=<the request's msgid>`. Channels folds trusted progress into
one line under that request, such as `astra · Using a tool`, updated in
place, instead of a reply row per notice. It never reads the notice's
prose. The contract, the trust rule and its sources, and the stage mapping
are in the [change record](changes/irc-thread-status/spec.md).

- **Trust.** A notice folds only when it replies to a message or action
  this buffer holds (in a private conversation, our own), and its
  server-set `account` tag equals the nick that request asks, ignoring
  ASCII case. The request asks a ship: the nick its first word addresses
  (`nick:`, `nick,` or `@nick`), or a private conversation's partner. A
  request whose sender's nick, or sender's `account` tag, is the addressed
  nick asks no one. Member lists play no part, so whoever holds the nick
  later, only the account named after it folds. Owner decision, 21
  November 2026: this overrides reviewer findings on the trust model.
  Fleet IRC servers are private and tie each nick to the account of the
  same name (`force-nick-equals-account`), so only the agent itself can
  send as its nick there. On other servers, an agent whose account is not
  named after its nick gets no status lines. Untrusted, unrecognised and
  uncorrelated notices, questions, answer instructions and final replies
  stay ordinary messages, counted and notified as before.
- **One line per request**, keyed by the request's own message ID, so two
  requests in one thread each have one. The newest trusted notice by
  server time (of two at the same time, the later one held) sets
  **Receipt unconfirmed**, **Accepted**, **Queued**, **Thinking**, **Using a
  tool**, **Waiting to retry**, **Waiting for your answer**, **Answer
  received** or **Writing a reply**; an update to one request leaves the
  others alone.
  The agent is the nick on the newest trusted notice, shown as plain
  text on one line and cut to 32 characters.
- **Clock and stage meter.** While a request runs, its line also shows the
  time since the request was sent by the server (`0:35`, `1:02:03` past an
  hour) in quiet monospace, and Kit's `Progress` as a five-step stage meter
  with the stage's word. Stargate's fixed stages map onto the meter:
  `received`, `submitting`, `accepted`, `queued` and `delivered` are
  *accepted*; `model`, `reasoning`, `tool` and `retry` *working*; `waiting`
  and `clarified` *waiting*; `reply` *reply*; `completed` and `failed`
  *done* (`thread_status::Stage`). The meter is named *Stage N of 5:
  <stage>*. Nothing else is shown: Stargate's data has no delegation or
  pull request stage. The conversation redraws once a second
  (`Conversation::follow_clock`) only while a line runs and is not stale,
  and drops the timer once none does.
- **Endings.** `completed` or `failed` settles its request's line, for good:
  no later or older notice reopens it. The settled line is quiet: a check
  and **Done in 42 m**, or a cross and **Failed after 3 m**, from the request's
  server time to the ending notice's (`35 s`, `42 m` or `2 h 5 m`), named
  *<agent>: Done in 42 m*. The reply count above it is unchanged. Owner
  decision, 22 November 2026: a finished request keeps a settled line, where
  the line used to disappear. The ending is kept apart from the notice with
  its outcome and server time (`thread_status::Ending`), and stored with
  them (`store::Ended`'s `outcome` and `at`), so it holds after the notice
  is dropped by the line cap, after a restart, and when a partial or Search
  reload brings back earlier progress without it. An ending record stored
  before outcomes were kept loads as **Done** without a duration; a held
  ending notice later fills it in once. A main-room final reply without a
  reply tag clears nothing.
- **Stale.** The line shows the last stage, its time and a **Stale** pill
  when the notice carried `,disconnected`, while the network is not
  connected, or when the notice was already held when the network's current
  connection began (`Network::held_at_connect`): restored from the store or
  seen before a reconnect. Anything received on the current connection,
  live or as history, is current.
- **Counting.** `Buffer::folded` holds the trusted notices
  (`state/folding.rs`). A live line is taken in by itself, without reading
  the buffer again; a history page, backfill or restore folds its buffer
  again; member list changes fold nothing again. A line stays folded while
  it and its request are held: when the line cap drops the request, its
  notices show as ordinary notices again, counted as history is: unread and
  highlighted only when newer than a known read marker. Folded
  notices make no row, are not replies, never count as unread and never
  notify, live or from history. They are stored and searchable; opening one
  from Search shows its request.
- **Reopening.** Statuses are rebuilt from the held lines on every change,
  so a restart or a reopened thread reads the same.
- The composer's typing line is unchanged and stays conversation-wide.
- This status belongs to one request, by its message ID. It is not the
  nick-wide `status` metadata from `draft/metadata-2`
  (`Event::NickStatus`), which Channels shows above the input and in the
  member list through the separate `channels-agent-status` change (see
  [Agent status lines](#agent-status-lines)); the two are kept apart.

## Message history and search

The board keeps channel and private messages on this computer, in its own
SQLite full-text store: the Framework's search store for the `irc` board
(`<state dir>/search/<app>/<board>.sqlite`, folder `0700`, file `0600`). There
is no other history file, and nothing goes into `state.json`. The design note
is the [change record](changes/irc-search-history/spec.md).

- **What is kept**: each `PRIVMSG`, `ACTION` and `NOTICE` in a channel or
  private conversation, yours included, live, replayed by a bouncer or sent as
  chat history. Joins, parts, quits, nick, topic, mode and server lines are
  not kept. Each message is one record: its `msgid`, else a hash of where,
  when, who and what, so copies dedupe; its text without formatting; the
  network as the `project` filter; and an exact `ReferenceIdentity::github`
  (`mentioned-pull-request` or `mentioned-issue`) for each GitHub pull
  request or issue URL in it, so `related_items` for that item lists the
  message. Beside them, each agent request that completed or failed is one
  record of kind `Channels request ending` under the same buffer, holding
  only the buffer and the request's message ID, with no searchable text
  (see [Request status lines](#request-status-lines)). Endings are written
  in the same batches as the lines waiting with them, before those lines.
  The restore loads each buffer's newest 20,000 endings, also when none of
  its lines survive.
- **When it is written**: in order on the background executor, in batches of
  at most 100, about a second after new lines arrive and when the board stops
  (`hub/persist.rs`).
- **Restore**: when the board starts, the hub reads each stored buffer's newest
  2,000 lines before it configures the runtime. They show at once, as history:
  no unread count, no mention count, no notification, no badge. It then sends
  `Command::Anchors` with each buffer's newest stored line, so joining asks the
  server only for what came after.
- **Search**: IRC is a Search view source with one local scope (endpoint and
  account `local`); the network is the `project` filter for `search_content`.
  A result's title is `#channel · nick`, or the nick in a private
  conversation; its kind is `Channel message` or `Private message`. Opening
  one shows the buffer's panel, scrolls to the line and selects it; when the
  buffer does not hold the line, the board first asks for the lines around it.
- **Control**: the Search view's **Cache Channels** switch, on by default
  (`BoardSettings::store_by_default`), keeps messages; off erases the store and
  IRC keeps lines in memory only. **Clear cache…** deletes every stored
  message. Retention (60 days by default) and the storage limit apply to IRC's
  store alone and drop the oldest messages first. Leaving a channel, removing
  it from the list or closing a private conversation deletes its stored
  messages.

## Removing channels and conversations from the list

A joined channel's row menu offers **Leave channel**, a channel we are no
longer in (after a kick or a typed `/part`) offers **Remove from list**, and a
private conversation's offers **Close conversation**. The toolbar's **More**
menu has the same items for the selected row, after **Connect** or
**Disconnect** and **Join…**. The server row offers none
(`spaces/irc/src/remove.rs`).

- **Leave channel** sends `Command::Part`, then `Command::Forget`, and
  removes the buffer. It is unavailable while the network is not connected.
- **Remove from list** and **Close conversation** send only
  `Command::Forget` and remove the buffer.
- `Command::Forget` sends nothing to the server. For a private conversation
  the session forgets the nick, so the other side's notices go to the server
  buffer again, as before the conversation was opened.
- Each closes the buffer's panel. The conversation in front stays in front,
  or the network's server buffer comes forward when the removed buffer was
  in front, so the list keeps a selection.
- A typed `/part` parts without removing the row, and a reconnect joins a
  configured channel again as before.

`IrcState::remove` drops the buffer and records it in `IrcState::removed`
(`state/removed.rs`) with the time of the newest message someone else sent in
it, a time the server set, or no time when there is none. Our own lines and
local error lines are left out, because their times come from our clock. The
hub saves that record under `irc.removed` in the state file whenever it
changes and loads it at startup, so removals survive a restart. Names match
ignoring ASCII case, because the runtime's spelling of a buffer may differ
after a restart; they do not use RFC 1459 folding or Unicode case, which on a
network with ASCII case mapping would join two different names. While an item
is removed, `IrcState::apply` drops every event for it, including a bouncer's
replay (`Event::Replayed`), except those that bring it back, which also forget
the removal:

| Removed | Comes back on |
| --- | --- |
| Private conversation | A message, action or notice in it (ours too, such as `/msg` from another channel) newer than the recorded time, or any one when no time was recorded, because the local clock cannot be compared with the server's. Not replayed history, typing, quits or nick changes. Also `Event::Joined` from `/query`, and opening it from the member list, a reference or a notification (`IrcState::open_query`) |
| Channel | Our own join (`Event::Joined` or our join line), from `/join`, **Join…**, a channel reference or the server; adding the channel to the network's `channels` in Settings |

A notification for a channel left since does nothing when chosen.

A left channel stays out after a reconnect and a restart, also when it is in
the network's `channels`:

- The hub configures the runtime with each network's `channels` minus the
  channels the user left (`hub/running.rs`). A later Settings save keeps the
  configuration of every network whose settings did not change, so leaving a
  channel never restarts a connection; a network whose settings changed starts
  without the channels left.
- `Command::Forget` puts the channel in the network task's `Carry` (`left`),
  whether a session runs, is connecting or the network is disconnected. Each
  connection skips those channels, matched ignoring ASCII case, in the
  configured autojoin and in the channels it rejoins after a reconnect, until
  we join them again.
- Restoring the dock layout drops removed conversations.

## Channel list

Each network shows every public channel on the server, not only the ones
joined or named in its `channels` setting (the
[change record](changes/channel-list/README.md)).

- **Fetch**: each connection sends `LIST` once, at the end of the message of
  the day (`RPL_ENDOFMOTD` 376 or `ERR_NOMOTD` 422), and collects
  `RPL_LIST` (322) until `RPL_LISTEND` (323) into one
  `Event::ChannelList` with each channel's name, visible user count and
  topic (`clients/irc/src/session/listing.rs`). `RPL_LISTSTART` (321) and
  the replies show no server lines. Names that do not start with a
  `CHANTYPES` prefix, such as the `*` some servers send for a private
  channel, are left out. `Command::ListChannels` asks again: the network
  header's **Refresh channel list** and the palette send it. A connection
  runs at most one `LIST`; a request while one runs, or before
  registration, sends nothing. A numeric naming `LIST` (such as
  `RPL_TRYAGAIN` 263 or an unknown command) or `FAIL LIST` ends it without
  an event, so the Space keeps its previous list. A `LIST` typed with
  `/raw` also replaces the list when it ends.
- **Cache**: `IrcState::directories` (`state/directory.rs`) holds each
  configured network's list with the time it arrived, the 500 busiest
  channels kept, sorted by name. The hub reads it from
  `irc-channel-list.json` beside the state file at start, so the list shows
  at once, and writes it there (`prefs::write_cache`, owner-only) when a new
  list arrives or a network is removed. Nothing goes into `state.json`.
- **Rows** (`rows.rs`): after a network's server row, channels and private
  conversations, the listed channels that have no buffer follow in name
  order, matched ignoring ASCII case: greyed (`Row::done`), with their user
  count (`7 users`) and no unread count. Their row keys use the kind `l`, in
  `Irc::listed`, apart from buffer rows. A parted or kicked channel keeps
  its own greyed buffer row; after **Remove from list** it returns as a
  listed row. A network with no list yet shows nothing extra. `/` and the
  palette search find listed channels too (kind `channel not joined`);
  choosing one selects its row.
- **Join** (`listing.rs`): a click selects a listed row
  (`ListDashboard::activates_on_click` is false for it); a double click,
  Enter or the row menu's **Join** sends `Command::Join` through
  `IrcState::join_listed`, which remembers the channel. When the server's
  `Event::Joined` names it, the state hands it to the board like a
  `/query`, and the board selects and opens it. **Join** is unavailable
  while the network is not connected; Enter and a double click then leave
  the runtime's **Not connected** line in the server buffer. The menu of a
  listed row offers **Join** then **Copy title**, without **Add to chat** or
  **Open**; while the network is not connected, **Join** is disabled with
  the reason under it. Joining does not add the channel to the network's
  `channels` setting.
- **Hidden channels**: the server leaves secret (`+s`) and private (`+p`)
  channels out of `LIST` unless you are in them, so they appear only once
  joined.

## Formatting messages

Messages are sent as typed; other IRC clients show the characters as they
are. The board shows formatting the way Slack reads it, in channels and
private conversations and for messages from anyone, people and bots alike:

| Written | Shown |
| --- | --- |
| `*bold*` or `**bold**` | bold |
| `_italic_` | italic |
| `~strike~` or `~~strike~~` | strikethrough |
| `` `code` `` | inline code |
| ```` ``` ```` lines around the code, or ```` ```code``` ```` | a code block |
| `> text`, or `>` alone | a quoted line; `>>>` quotes the rest of the message |
| `- `, `* `, `• ` or `+ `; `1. ` or `1) ` | bulleted and numbered lists |
| `https://…`, `[text](https://…)` | links |

The rules, in order of precedence:

1. **mIRC codes.** A message with mIRC formatting codes keeps the IRC
   reading: its Markdown characters show as typed. Its bold, italic,
   underline, strikethrough and monospace show; a colour shows as a tint of
   its theme tone behind the text (see [Selecting and copying](#selecting-and-copying)).
2. **Code.** Fenced blocks and code spans (a run of backticks closed by a run
   of the same length on the line) show their text as typed: nothing inside
   is formatted, linked or referenced. Slack's fences work too: text right
   after an opening fence is the first line of code, unless it is one word
   such as `rust` and code follows, which names the language; a fence at the
   end of a line closes the block; text after a closing fence is a line of
   its own. An unclosed block is closed at the end of the message.
3. **Links and URLs.** `[text](url)` is a link only when the link policy
   opens its URL (`http`, `https` or `mailto`, through `links::safe_url`);
   otherwise it shows as typed. Its text shows as typed, and the typed URL
   ends at the `)` that balances its parentheses, at most 32 deep, as the
   parser allows. Bare `http` and `https` URLs are links, unless they hold
   `<` or `>`. Nothing inside a link or URL is formatted. A link always opens
   exactly the URL the policy returned, never the typed text: brackets,
   backslashes and `&` are written with a backslash, which Markdown decodes,
   and anything that cannot be in a destination is percent-encoded.
4. **Markers.** A marker opens before text, after a space, the line's start,
   an opening bracket or quote, or another marker. It closes after text,
   before a space, punctuation or the line's end. A marker between two
   punctuation characters, as in `(*.md)` or `"*"`, is neither. A closing
   marker pairs with the nearest open marker of the same character and length
   on the same line; markers opened after that one show as typed. So formats
   nest (`*_both_*`), `*a _b* c_` is bold `a _b` then ` c_`, and
   `snake_case_names`, `__init__`, `2*3*4`, `src/*.rs`, `**/*.rs`,
   `(*.rs) and (*.md)` and `~/notes` show as typed. Runs of `__`, `***` or
   `~~~` and longer, and unpaired markers, show as typed.

Everything else shows as typed: a backslash is a character, not an escape;
headings, rules, tables, HTML, entities, math, footnotes and task lists are
escaped, and `<` is written as `&lt;`, so no message can start HTML; an image
`![alt](url)` shows as `!` and a link, so nothing remote loads. `:`, `@` and
the `.` of `www.` are escaped so the renderer's GFM autolink literals never
form: they would run on through the following syntax, such as a code span's
opening backtick. So `www.` and email addresses are text, as in plain
messages. A list item's text follows the paragraph rules, and a list keeps
its first item's indentation as the left edge, so an indented list stays a
list. A list marker with nothing after it shows as typed, so an empty
nested bullet cannot turn the item above it into a heading. A lone carriage
return ends a line. `markup/markdown/parsed.rs` parses
the rewrite with the same parser and options as `TextView` to check all of
this. `#channel` and `@member` references stay clickable inside formatting.
Your nick and highlight words still highlight inside `*bold*`, `**bold**` and
`~strike~`, but not `_italic_`, because `_` is a nick character. A
multi-line batch (`draft/multiline`) is one message, so a code block, quote or
list can span its lines. A row's accessible name is the text as shown,
without markers.

## Selecting and copying

A drag from any point in the transcript to any other selects the text of
every row between, and Copy puts the selected text on the clipboard in
reading order. Each row's text is one selectable GPUI Kit `TextView`
(`text.rs`): formatted messages as the safe Markdown above, and plain
messages, mIRC-coded messages, server lines and membership lines as literal
Markdown (`markup/markdown/literal.rs`). Kit numbers `TextView`s in the order
they paint, which is reading order, and copies participants by that number,
so the rows copy in order. The other selectable texts Kit offers do not mix
with them: `InteractiveText` takes no part in a selection, and
`SelectableText` defaults to order 0, which copies before every text view.
`tests/selection/kit.rs` records this, and
`docs/changes/irc-transcript-drag-selection/findings.md` explains it with
Kit's source.

Literal Markdown shows the text exactly as sent: every ASCII punctuation
character is escaped with a backslash (`<` as `&lt;`), spaces and tabs at a
line's start or end become character references, and each line ending, a
lone carriage return included, is a hard break. mIRC bold, italic, underline and strikethrough become the inline
`<b>`, `<i>`, `<u>` and `<s>` tags Kit's Markdown reads, monospace a code
span, URLs links the link policy opens and references links the board opens.
A `TextView` has no text colour for a run, so an mIRC colour becomes a
`<mark>` tint of its theme tone at 16 % opacity; white and black, which
follow the text colour, add none, and a compact line's sender name is bold
without its tone. These generated tags are the only HTML in a row, and no
message text can open a tag. A muted or error row keeps its tone through
`rich::tinted`, as a `TextView` ignores its container's text colour while the
app leaves Kit's `inherit_text_color` off.

Kit separates rows in the copied text with a blank line, as each `TextView`
ends its paragraph with a line break. Not copied: message sender headers,
times and day dividers, which are labels, not text views. Sender names within
compact server and topic lines are copied with their text. The `MessageScroller` keeps only
rows near the viewport, so a drag that auto-scrolls far past them can lose
its start; a fix needs a Kit selection adapter for virtual lists. Right-click
**Copy** on a row copies the selection, or the row's whole text.

The unread divider, **NEW SINCE YOU LEFT · N** with the number of messages
that arrived since the conversation was last read (spoken as *N new
messages since you left*), is also a label: it does not interrupt selection
or add its label to copied text. Unread row fills remain visible behind a
text selection. Literal links and references retain their native look and
cards through Kit inline objects. Right-click **Copy** on an issue link
copies the selection or the link's displayed text. A primary click opens
the issue in the current Issues board, including while the preview loads.
## Oversized drafts

The shared composer grows by whole visual rows, including wrapped text, from
two rows up to twelve. Half the measured Space height, after the toolbar and
counter, can lower that cap. The native textarea owns both its row minimum
and scrolling; the old fixed wrapper height could let its rows extend into
the toolbar. Drag and keyboard resizing now set matching native row bounds.

The empty draft shows the send hint. Kit's fast opacity transition hides it
as soon as the draft contains text and restores it when cleared; reduced
motion switches immediately. Send errors remain visible.

Channels opts into `ChatInput::set_limits` with `DraftLimits`. The client
exports `draft_line_limit` and `MAX_INLINE_BYTES`; validation and the counter
use the same rule, including the 10-line command exception and literal `//`.
Capabilities changing during an open draft also update its counter. The
shared `badge::Pill` is a labelled status at the frame's top right, in its
own layout row above the editor. It shows `n / max lines` above one line and
`x.x / 4 KB` strictly above 2,048 UTF-8 bytes. KB uses 1,024 bytes. The higher
of line and byte usage selects muted below 80%, warning from 80% through
100%, and danger above 100%. Exact counts are exposed in the status label.

The composer keeps drafts over the line limit or 4,096 UTF-8 bytes available
for editing. The line limit is 100 for plain text on a connection with
`draft/multiline`, which sends it as one batch, and 10 otherwise, including for
slash commands. **Save as file…** writes the draft text to a local file. The
IRC session checks the same limit before it splits text or creates a multiline
batch, including text sent through slash commands and agent tools; the agent
`message` tool keeps its own 10-line limit. Text that doesn't fit one batch,
because the server advertises a lower limit or long lines split into pieces,
goes as several batches, so on a server like the Subspace Ergo server a draft the composer
accepts is sent. Only a server whose batches are so small that more than 10
would be needed refuses it, after the composer has cleared it (it stays in
recall), rather than sending it line by line. Actions and notices over 10
lines are refused too.

A single paragraph that wraps into several wire messages also uses one
`draft/multiline` batch when it fits the negotiated byte and physical-line
limits. Continuations keep spaces and UTF-8 characters and use concat tags,
so receivers see the whole paragraph without inserted newlines. A paragraph
that cannot fit a batch keeps the existing standalone splitting fallback.
See the [wrapped paragraph record](changes/subspace-api-accounts/README.md).

## Issue references

IRC shows issue references through the `issues` contract
([Board-to-board contracts](../../subspace-framework/docs/board-integration.md#board-to-board-contracts),
[record](changes/irc-issue-references/spec.md)). It never depends on
`clients/subspace-api` or the Issues board, and works with whichever board answers.

- **Declared roles** (`issues::CONTRACTS`): subscribes to `issues@1/changed`,
  requests `issues@1/current-scope` and `issues@1/resolve`.
- **Scope**: the process-owned hub (`issues/follow.rs`) subscribes from
  `hub::start`, so it follows the contract while the view is closed. It asks
  for the scope at start and on every `changed`. `None` or `Unavailable`
  (Issues disabled, absent or without a ready API connection) means
  nothing is recognised: identifiers stay text and links stay links.
- **Recognition** (`markup/issue.rs`) uses the contract's recogniser
  (`subspace_contracts::issues::recognition`) with the scope's project keys
  and API origin, inside `markup::classify`, so plain and formatted lines
  share the same identifier rule (`[A-Z][A-Z0-9]{1,9}`). The scope carries
  `profile_id`, `profile_name`, `api_origin` and `project_keys`. Links to
  other trackers stay links. Code spans stay plain.
  An issue link is a reference, not a link (`style/marks.rs`).
- **Resolving** (`issues.rs`): each message row the transcript draws reports
  its identifiers; the first new one of a frame defers one request until the
  frame is drawn, so a frame's references go in one `resolve` of at most 50
  distinct identifiers (more are split). An identifier is asked once per
  round; `changed` starts a new round, keeps the old answers on screen until
  they are asked again, and forgets them when the profile, API origin or
  project keys change. A refused `resolve` drops the scope until the next `changed`.
- **Looks** (`text/references.rs`, `transcript/line/marks.rs`): references are accent on
  a tint, as `#channel` references are; `Unavailable` is muted and
  underlined; the keyboard's reference is accent on a strong tint. An
  available issue leads with a dot for its state category (`text/look.rs`)
  in the tone of the hover card's `state_glyph`: started yellow (a started
  state named for review blue), completed the accent, cancelled or duplicate
  muted, and a hollow ring before work starts (triage orange, backlog muted,
  unstarted the foreground). A
  reference still loading or unresolved has no dot. The dot is a child of
  the inline object, so selection and Copy keep the reference as written. Formatted
  (Markdown) lines show issue references as links (`subspace-irc:issue/…`)
  without an inline muted look.
- **Hover cards** (`issues/tooltip.rs`) share one observed view for chip hover,
  inline-reference hover and the keyboard's focused reference. The view reads
  the hub on each draw, so `Pending` (*Loading SUB-42…*) becomes the current
  summary after `changed`. Available summaries use the A accent or B tracker
  composition from `issues/tile/`, at `400 / REFERENCE_REM` rem. The shared
  `reference_card::frame` supplies the card surface, state edge or outline,
  padding and radius, with Kit's themed popover elevation. It is the tooltip
  view itself, with no surrounding Kit tooltip frame and no interactive
  children or focus. Pending, Unavailable, SignedOut and Failed keep plain
  Kit tooltip notes. The keyboard card floats through `anchored::floating`.
- **Issue chips** (`issues/preview.rs`, `issues/tile/`) use the same batched
  lookup as the hover card. Every available issue is a compact chip in its
  reserved slot (below), including
  a message with one available issue; the row always wraps. Chips retain their
  full accessible names and native activation. A and B cards appear only on
  hover or reference keyboard focus. Ordinary hover cards have a state edge,
  mono key, urgent or high priority pill, state pill, two-line title and
  optional assignment and latest-activity footer. Tracker hover cards show a
  progress bar and available pull request and due-date facts.
  Code spans and blocks do not request chips. The additive summary builders
  carry priority, owner, assignee kind/rank, progress, pull requests, due date
  and latest activity without coupling Channels to the API client.
  `reference_card` supplies shared chip and passive card treatments, pills
  and avatars; Kit supplies icons and progress. Colours come from `Tokens`.
  Pure UTC formatting uses `hub.now`: minutes, hours, days, then a British date
  after seven days; future activity says *just now* without a recent dot.
  Activity within an hour has a success dot. Missing or malformed optional
  dates are omitted. See the [change record](changes/subspace-api-accounts/README.md).
- **Reserved chip slots** (`issues/preview.rs`, `issues::tile::reserved`):
  every issue reference the scope recognises outside code gets a slot in the
  chip row from the message's first frame, before any answer, so an answer
  never changes the row's height and nothing above or below it moves. The
  slot is the chip's own surface (`card_button` in `reference_card::surface`),
  inert and named by the tooltip note. With no answer yet or `Pending`, it
  shows the known key between static skeleton marks in `Tokens::line`, the
  shared list skeleton's colour; nothing animates, so reduced motion needs no
  case. `Unavailable`, `SignedOut` and `Failed` keep the slot as a muted
  `CircleSlash` chip with the key and **Unavailable**, **Signed out** or
  **Couldn't load**; the inline reference still opens the issue, and the
  hover note gives the reason. An answer already held, including one from an
  earlier round, draws its chip on the first frame. Slots wrap in the same
  row as chips; a loading slot is as wide as a chip with a full-length title,
  so a short-titled chip can be narrower than its slot. Only recognition
  changes the reservation: a scope that arrives later, changes or is
  withdrawn still adds or removes slots. See the
  [change record](changes/channels-reserve-issue-tile-space/README.md).
- **Opening an issue** works from a chip, resolved reference click or `Enter` on a resolved
  reference. The shell opens the issue in the answering board after it checks
  the profile on the navigation request.
  A reference without a summary keeps its browser destination while the
  current profile can recognise it.
- **Privacy**: scope and answers live in the hub's memory only; nothing goes
  to IRC's store, search, state file or logs.
- **Keyboard** (`conversation/keys.rs`, `transcript/focus.rs`): `Shift+Tab` in
  the message input focuses the newest message (`IrcConversation > Input`,
  ahead of the text area's outdent); in `IrcTranscript`, `↑`/`↓` move between
  messages, `Tab`/`Shift+Tab` between a message's links and references in
  reading order, `Enter` opens and `Esc` returns to the input. A ring marks
  the focused message. For a formatted message the references are
  `Rendered::refs` (issues and pull requests outside code, in reading order,
  a pull request with its URL as written, each with its range in the message
  text). `transcript::targets` keeps the plain text's links and channel and
  nick references, drops a link a rendered reference covers, and sorts all of
  them by those real ranges, so Tab follows reading order and Tab and a click
  agree.
- **Evidence fixture**: debug capture builds with `SUBSPACE_CAPTURE_ISSUES`
  route a stand-in responder (`subspace-framework/host/app/src/capture/issues.rs`)
  with fictional answers; release builds never contain it.

## Pull request references

IRC shows GitHub pull request references through the `github` contract
([Board-to-board contracts](../../subspace-framework/docs/board-integration.md#board-to-board-contracts),
[change record](changes/subspace-api-accounts/README.md)). It never depends on the
GitHub client or the PRs board.

- **Declared roles** (`issues::CONTRACTS`): subscribes to `github@1/changed`,
  requests `github@1/resolve-pull-requests` and `github@1/open-pull-request`.
- **Recognition** (`markup/pull_request.rs`): `[PR:owner/repo#n]`, a full
  pull request URL with GitHub's `/files`, `/commits`, query or fragment
  suffixes, and a Markdown link whose label is the marker and whose
  destination is the same pull request. Each gives one `PrRef`: the
  lower-cased `owner/repo#n` identity and the URL as written (the canonical
  URL for a marker). Repo-only markers and bare `#123` stay text: Channels has
  no repository context. Recognition runs before ordinary links, so a pull
  request URL is a chip rather than a link. In a formatted message, code spans
  and fenced code stay text, and `Rendered::refs` lists the chips outside
  code for the keyboard and the hub.
- **Chip** (`text/references.rs`): the `GitPullRequest` glyph, tinted by an
  available answer's status (open blue, merged magenta, closed or draft
  muted; `text/look.rs`), and `repo#n`,
  with the owner only beside another owner's same-named repository in the
  message (`PrRef::label_among`). The node text stays as written, so Copy
  keeps it; Copy link gives `PrRef::url`. No preview row is added under the
  message.
- **Answers** (`pull_requests.rs`, `pull_requests/follow.rs`): the hub asks
  for the distinct keys a frame shows in one request of at most 50, keeps the
  answers in memory and forgets them, and the wanted keys, on every `changed`.
  A refused request records `Unavailable`. Nothing reaches IRC's store,
  search, state file or logs.
- **Hover tile** (`pull_requests/tooltip.rs`, `pull_requests/tile.rs`): a
  passive 320 px `reference_card` (accent treatment) with state edge and pill,
  `repo#n`, title, branches and changed lines when known, author, review and
  check marks and the time since update from `hub.now`. Every other state is
  a note: `Loading repo#n…` while pending, otherwise **Details unavailable**.
- **Opening** (`pull_requests/follow.rs`): a click, or `Enter` on a chip the
  keyboard focused (which shows the same card), navigates to
  the running PRs Space (`contracts::navigation_target`), from the clicked
  conversation. With no running responder the URL as written opens in the
  browser with the info toast *PRs is unavailable. Opening in browser.*; a
  disabled Space is never enabled. Part 1's first-click and drag rules apply.

## Agent tools

The board advertises `irc_read` and `irc_write` (`spaces/irc/src/agent_tools`).
They act on the board's hub: the same connections, buffers and nick you see.
A tool's message is sent as you and appears in the transcript exactly like a
line you typed, without a marker. The hub logs each write at info level with
the action, network and target, never the text or a channel key.

Only the running app serves them: Subspace chat (Copilot, Pi and ACP agents,
which get the app's own tools over a per-session loopback endpoint) and the
app's own **Serve MCP over HTTP** server. They are built only for
`ToolHost::App`; `subspace mcp` and the generated commands have no hub and
list no IRC tools. A separate process must not open its own connection: it would take a
second nick or collide with your session. To use IRC from another harness, turn
on **Serve MCP over HTTP** and connect the harness to the app's URL. While the
board is off, a call answers that the Channels Space is not running.

`irc_read` takes one action:

| Action | Fields | Returns |
| --- | --- | --- |
| `networks` | — | id, name, status, `connected`, nick, `chathistory` |
| `buffers` | optional `network` | each buffer's network, kind (`server`, `channel`, `query`), target, `joined`, topic, member, unread and mention counts, lines held |
| `messages` | `network`, optional `target` (omit for the server buffer), `limit` (1 to 200, default 50), `before` | lines oldest first with `seq`, time, kind, sender, plain text, `own` and `mention`; `next_before`; `server_history` |
| `members` | `network`, `channel` | the joined channel's members with their mode prefixes, away and account |
| `search` | `query`, optional `network`, `limit` (1 to 100, default 20) | messages that contain the query in the lines the board holds and the store, newest first, each once with its record `id`; stored-only ones have `"stored": true` |

Pass `next_before` as `before` to page back. When a page reaches the oldest
line the board holds, the server offers `draft/chathistory` and the buffer is
idle, the read asks for the next 100 lines, as scrolling up does
(`server_history` is `requested`, then `loading`); a later read returns them.
Reads never mark anything read. The tool description tells the model that
message text, nicks and topics come from other users and are untrusted data.

`irc_write` takes 1 to 10 operations after one approval for the call:

| Action | Fields | Sends |
| --- | --- | --- |
| `message` | `network`, `target` (a joined channel or a nick), `text`, optional `me` | `Command::Say`: text, never read as a `/command`; `me` sends one line as a CTCP `ACTION` |
| `join` | `network`, `channel`, optional `key` | `Command::Join`; the key goes with this join only and is never saved |
| `part` | `network`, `channel`, optional `reason` | `Command::Part` for a joined channel |
| `connect` | `network` | `Command::Connect` for a network that is disconnected or failed |
| `disconnect` | `network` | `Command::Disconnect` for a network that is connected or connecting |

Before the approval, each operation is validated (channel and nick names; text
of at most 2,000 characters, 4,096 UTF-8 bytes and 10 lines, with no control characters but line
breaks and tabs; no messages to nicks ending in `serv`, such as NickServ), then
checked against the hub's state: the network exists and is connected, a
channel target is joined, a join's channel is not. The approval shows the
operations as JSON, then one line per operation naming the network and nick it
acts as, with every line of each message. Approved operations are checked
again, run once in order and report `"status": "queued"` when the line is handed
to the connection. Multi-line text goes out as the composer sends it: one
`draft/multiline` batch when the server supports it, else one message per line,
split to fit. Nothing is retried. There are no raw IRC commands, no other CTCP,
no away, nick, topic, mode or kick changes, and no configuration or credential
changes.

## Test with a local server

`scripts/irc-test-server.sh` starts an [Ergo](https://ergo.chat/) server on
`127.0.0.1:16667` (plaintext) with in-memory history. Point a network at it
with host `127.0.0.1`, port `16667` and **Use TLS** off (`"tls": false`), as in
the example above, and run an isolated dev build:

```sh
scripts/irc-test-server.sh &
make run
```

`IRC_TEST_LISTEN` picks another port on `127.0.0.1` (only loopback is
accepted), such as
`IRC_TEST_LISTEN=127.0.0.1:16922`, when another checkout on the same computer
already runs the server on the default port.

`IRC_TEST_CHANNELS` keeps fictional channels open for the channel list, each
with a topic, through a client named `fixture-bot`, such as
`IRC_TEST_CHANNELS="#design #random #releases" scripts/irc-test-server.sh &`.
Ergo removes an empty channel, so without it `LIST` shows only the channels
someone is in.

`scripts/irc-agent-status-fixture.py` (Python standard library only) keeps
fictional agents with status lines, and a person typing, in `#agent-status`
until it is stopped, and writes the matching Channels configuration for
`CAPTURE_IRC_CONFIG` or a dev build. The
[agent status change record](changes/channels-agent-status/delivery.md#evidence)
has the commands.

`scripts/irc-thread-status-fixture.py --config <file>` plays a fictional
ship, `astra`, answering three requests in `#thread-status` with tagged
progress: two in one thread, a question and its answer, a lookalike notice
from another account (`nova`), a lost event stream, a main-room final reply
and one request completing, timed from when the capture build joins. Both
register throwaway accounts on the local server. Once the server has
answered, it writes the Channels configuration for `CAPTURE_IRC_CONFIG`
(mode `0600`, never through a symbolic link) and prints `ready`. Its capture
command is in the [change record](changes/irc-thread-status/delivery.md).

The runtime's integration test against the same server runs when
`IRC_TEST_SERVER` is set (`IRC_TEST_SERVER=127.0.0.1:16667 cargo test -p
subspace-client-irc`) and is skipped otherwise. With the same variable,
`cargo test -p subspace-irc-board agent_tools` also checks that an
`irc_write` message arrives in a channel under the tool's nick, and the
runtime's `anchors` test checks that an anchored join receives only the
messages after its anchor.

## Design

- `hub.rs` owns the runtime for the whole process, from
  `Dashboard::start_background` until Settings turns the board off. A 50 ms UI
  timer drains events into one state entity and notifies the board. It
  observes the `Services` global and applies changed IRC settings with
  `Command::Configure` and `IrcState::reconfigure`.
- `store.rs` maps stored lines to records (`store/mapping.rs`, GitHub links in
  `store/links.rs`) and runs writes, deletes and the restore against the
  store. `hub/persist.rs` queues them in order. `state/merge.rs` merges
  restored lines, missed lines (`Event::Backfill`) and the lines around a
  message by time, deduplicating by `msgid`, then by time, sender and text.
  `reveal.rs` opens a search result.
- `state/directory.rs` holds and caches each network's channel list;
  `listing.rs` joins a listed channel and asks for the list again (see
  [Channel list](#channel-list)).
- `config.rs` reads the settings the shell loaded from the board's file
  (`Services::board_config_file("irc")`).
- `editor.rs` is the board's `Dashboard::settings_editor`: retained native
  inputs per network inside Settings' Save/Cancel draft. `editor/draft.rs`
  reads and writes the JSON over the saved object, and `editor/validate.rs`
  holds the rules. Saved passwords stay in the saved object and are written
  back only when untouched.
- `state.rs` is a pure reducer over runtime events: networks, buffers, topics,
  members, typing, nick statuses, read markers and unread and mention counts. Live lines keep
  a buffer at 2,000 lines, dropping the oldest. Loaded history raises that
  buffer's limit by the lines it added, up to 10,000; a page that would pass
  10,000 keeps its newest lines that fit and ends **Load older messages**.
  Notifications are decided there: a live line that counts as unread and
  mentions us or is in a private conversation raises `Attention`. History
  (`chathistory` batches, with their `draft/event-playback` events), a
  bouncer's replay (`Event::Replayed`, from ZNC's `znc.in/playback` batch),
  our own lines, the **Server** buffer, lines the read marker covers and
  folded request progress never do. The hub raises it for conversations not
  on screen (the focused one and each tab group's displayed tab, published
  by the board), as a
  `Notice::described` from `notice.rs`: who and where, with the network as
  the OS body, never the message. IRC declares OS delivery with the Glass
  sound as its notification defaults. See
  [the change record](changes/irc-mention-notifications/spec.md).
- Each opened buffer has a `Conversation` view (`conversation.rs`): its
  transcript and composer, with its own scroll position and draft. The dock
  tab carries the buffer name and exposes the topic and network on hover;
  `Conversation::heading` gives the pane's toolbar the name in strong text
  and the topic in quiet text (`HeaderFacts::quiet`: the network, or the
  server buffer's status, while there is no topic); its accessible name
  still says the network. A fleet channel (see [Fleet roles](#fleet-roles))
  leads with its captain's agent avatar (a ship) or a glyph for its kind,
  and a ship's channel ends with a pill holding its captain's current nick
  status (`header::live`), at most 40 characters, named
  *Captain <nick>: <status>*, and nothing while the captain has none or no
  member with its nick is signed in to its account. A channel also has a `MemberList` view
  (`members.rs`), which draws the rank groups under captions such as
  `Operators · 1`, each member with a small avatar, the nick and a quiet
  note (`members::note`: `you`, a fleet's `captain`, `rear admiral` or `crew`,
  `owner`, `admin` or `half-operator`), with
  the member's status line, if any, under the nick, and
  leaves the heading and the scrolling to its host. Both observe
  the hub and report navigation as events.
- `view/frame.rs` composes the Framework's workspace shell
  (`dashboard::workspace`). The navigator is titled **Channels**, has Join in
  its header and holds the buffer list, drawn by the shared list
  (`list::render`) so the rows keep their selection, keys, folding and
  menus; `NavRow` has no menu or key hooks. The list reads as navigator
  rows: `list_side_inset` returns `workspace::text_edge` (kept in
  `Irc::text_edge`, refreshed each render), rows keep the theme's row
  height (not `comfortable_rows`), and `row_trailing` draws unread and user
  counts at caption size, as `NavRow`'s count. The pane's toolbar leads with the initiative picker and the heading
  of the conversation in front, and trails with a search button, which
  opens find in the conversation (`find_in_page::Open`), and **More**
  (`toolbar.rs`). **More** holds **Connect** or **Disconnect** and **Join…**
  for the network, then the selected buffer's removal. Join is also in the
  navigator's header, and the command palette offers Join, Connect and
  Disconnect. The inspector is titled **Members**, with the count for a
  channel, and holds the focused channel's `MemberList`. While the navigator
  is hidden, `view/conversations.rs` adds a menu of every network's
  conversations at the start of the toolbar. Before a network is configured
  the pane shows the setup state with no toolbar and no inspector.
- `dock.rs` hosts the conversations in a GPUI Kit `DockArea`, the pane's
  content: centre panels in tabs and splits (`dock/open.rs`). The tabs are
  the dock area's own, one strip per group, so the pane has no tab strip of
  its own. List and conversation events reach it without a window, so dock
  edits are deferred to the board's window (`dock::with_window`).
- `dock/layout.rs` is the layout in buffer terms (splits, tab groups, and
  whether the navigator and the inspector show), saved under
  `irc.dock-layout` after every change and restored once the hub is bound
  (`dock/persist.rs`). The workspace owns the two docks' widths, as density
  tokens shared by every Space; a layout stored with widths still reads.
  Conversations on networks that are no longer configured are dropped with
  the groups they leave empty. Without a stored layout the first network's
  **Server** buffer opens.
- `Dashboard::status` carries what the Space's heading used to
  (`view::status_line`): `IRC`, the networks and how many are connected, then
  the initiative hint (`initiatives::hint`), such as the connection an
  initiative still needs.
- The transcript uses Kit's `MessageScroller` with stable row identities, so
  new lines and prepended history splice in without losing the reading place.
  `lines.rs` groups a sender's messages within five minutes and condenses
  membership lines; `markup.rs` finds `#channel` and member references and
  rewrites formatted text to a safe Markdown subset for the shared rich text
  (`markup/markdown/`: `blocks.rs` sorts lines and Slack's fences and
  quotes, `emphasis.rs` pairs markers, `spans.rs` reads code spans, links and
  URLs as units, `inline.rs` and `lines.rs` rewrite each line). The result
  has no HTML and no images, so the row shows it with
  `rich::rich_text_prepared`, without `prepare_markdown`. Plain text, text
  with mIRC codes, server lines and membership lines go through the same
  view as literal Markdown (`style.rs`, `markup/markdown/literal.rs`,
  `text.rs`; see [Selecting and copying](#selecting-and-copying)).
  A day starts with quiet text between two hairlines. Times are in quiet
  monospace: beside the sender's name, and in the avatar-wide gutter of
  compact lines, so they read as a column. Senders' names are semibold in
  their tone.
  Every row but the day divider is tinted under the pointer with the foreground
  at low opacity; a row that mentions us keeps its `select` fill
  (`transcript/fill.rs`). Message and compact-line text is capped at
  `workspace::READING_WIDTH_PX`; the row's fill runs the pane's width.
  `thread_status.rs` reads the progress marker and `thread_status/trust.rs`
  decides which notices the server vouches for; `state/folding.rs` keeps
  each buffer's folded set, which `lines.rs` leaves out of the rows.
  `thread_status/model.rs` rebuilds each request's status from the folded
  notices on every conversation sync, beside the reply counts, and
  `transcript/status.rs` draws it under the request with GPUI Kit's
  `Marker`, with Kit's `Progress` as the stage meter while it runs, and a
  quiet settled line once it ends (see
  [Request status lines](#request-status-lines)).
  See [GPUI-Kit messaging reuse](../../subspace-framework/docs/ui-components.md#gpui-kit-messaging-reuse).
- The composer (`compose.rs`) is the shared `ChatInput::message` in plain-text
  mode with recall keys, under the shared completion list. `Enter` sends and
  `Shift+Enter` breaks the line, which the hint under the input names
  (`Return` on macOS). Under the input, a slim row (`compose/hints.rs`,
  `irc-composer-hints`) names the keys with Kit's `Kbd`: `@` mention, `/`
  commands, `↑` recall (IRC cannot edit a sent message) and `⇧⏎` new line;
  with uploads on, the retention notice ends that row in a shorter form,
  “Kept 30 days. Don’t paste credentials.”, named in full. With uploads on,
  one quiet line above the input holds the paperclip
  (`irc-send-attachment`, **Send as attachment…**,
  `uploads/outgoing/view.rs`). `ChatInput` draws its frame
  edge to edge, so the field is not rounded. Up and Down recall sent lines while the draft is empty,
  one line or a recalled line not yet edited (`ChatInput::set_recalled`), so
  a multi-line message does not stop the walk; they move between the lines of
  any other longer draft. The completion list
  (`chat_input::completion`). After each edit, `compose/suggest.rs` reads
  the word before the cursor and offers choices for it:
  - `@` offers the buffer's members, or the other person of a private
    conversation. Choosing one inserts `@nick ` with a space after it, which
    the transcript shows as a mention pill. The nick in it still highlights
    for the recipient, because `@` is not part of a nick.
  - `/` at the start of the draft offers the commands that the client parser
    (`subspace_client_irc::COMMANDS`, beside `parse_input`) handles, with
    their arguments and a description. Choosing one inserts `/name `. In a
    ship's channel or a Starbase the channel's own command comes first:
    `/ask <text>` (`compose/ask.rs`), *Ask <agent> and track the answer*,
    named as a command of this channel. The shared list has no section
    caption, so nothing heads it. `/ask` sends `<agent>: <text>` as
    `Command::Say` to the ship's captain or the Starbase's rear admiral
    (`Roster::asked_in`), so the request status line tracks it; anywhere
    else, or with no text, the draft stays and the input shows why. `ask`
    names no client command, which a test checks.
  - The first argument of `/join` offers the network's channels. `/part` and
    `/topic` offer the channels you are in. `/msg`, `/query`, `/notice`,
    `/whois`, `/invite` and `/kick` offer everyone on the network.
  - `#` elsewhere in the text offers the channels you are in.

  People are listed by who spoke recently in the buffer, newest first, then
  by nick; your own nick is left out. Rows show an initials avatar and the
  mode prefix, muted at the end as in the member list, and away members are
  dimmed. Matching ignores case
  using RFC 1459 folding, which treats `[]\~` as `{}|^`. The server's own
  casemapping is not sent to the board, and RFC 1459 folding matches at
  least as much. Up and Down move the highlight, Enter or Tab inserts the
  choice, Escape closes the list and a click chooses a row. While no list is
  open, Tab completes a bare nick before the cursor (`compose/complete.rs`),
  adding `: ` at the start of the line.

## Inline issue summaries

Each available issue chip has a separate compact shared `IconButton` chevron.
Its accessible name is **Show summary of SUB-2** or **Hide summary of SUB-2**,
and Kit receives expanded state. Activation stops propagation; the chip’s
other button still navigates to Issues. Hover cards A and B show a supplied
summary below the title in muted text, clamped to three lines; null is omitted.

`Conversation::summaries` retains one selection per message, keyed by message
sequence and issue identifier. Redraws keep the selection. The panel sits under
the wrapping chip row; null uses **No summary yet**. The dashboard kit’s
`animated_collapsible::AnimatedCollapsible` composes a GPUI animation clock and
standard GPUI layout. `DISCLOSURE_DURATION` is 200 ms with `ease_in_out`;
height and opacity follow the same progress in both directions. The animation ID
includes the message, issue and transition revision. The owner retains the
measured and sampled geometry, so a reversal starts at the current height.
Content height changes, including switching to a different summary, animate
from that height to the newly measured target.

Progress changes defer remeasurement of the message’s virtual row until the
scroller releases its list borrow. Kit’s tail-following state keeps a transcript
at the bottom anchored while the row changes height. `App::reduce_motion()`
uses the endpoint immediately. The gallery has collapsed, mid-way, open and
interactive examples. See the [change record](changes/subspace-api-accounts/README.md).
