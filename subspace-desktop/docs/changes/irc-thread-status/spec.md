# Spec: IRC thread status

## Findings

### What Stargate sends today

Read from upstream Stargate at
[`9aab677`](https://github.com/aiconsultancy/stargate/tree/9aab67757e0e25ad7f677da6a3d43a3c25499b7b),
the merge of [stargate#92](https://github.com/aiconsultancy/stargate/pull/92)
(typing and request progress in IRC and Teams), which follows
[stargate#90](https://github.com/aiconsultancy/stargate/pull/90) (live
request status and background questions) and
[stargate#91](https://github.com/aiconsultancy/stargate/pull/91) (crew
question handoff). Paths below are in that tree. Both progress features are
opt-in per captain (`irc.features.liveProgress`, `irc.features.fleetTyping`;
`src/config.ts` lines 575–581).

Every request is a `PRIVMSG` from a person. Stargate stores its `msgid` as
the request's thread, and, when the request is itself a reply, the thread
root as its reply parent (`src/irc/routing.ts` lines 505–519).

| What | Command and target | Tags Stargate sets | Text |
| --- | --- | --- | --- |
| Live progress (`liveProgress`) | `NOTICE` to the channel, or to the requester's verified nick | `+draft/reply=<reply parent, else the request's msgid>` when `message-tags` is on | First, after at most 3 s: `Stargate received your request. Waiting for backend activity.` if nothing was observed, else the stage text below. Then the stage text again whenever the stage or the disconnected flag changes, checked at most every 30 s (`src/irc/delivery.ts` lines 178–210). |
| Default progress (`turnStatus` without `liveProgress`) | the same `NOTICE` | the same | `Checking that now…` at once, then after 5 minutes the stage text, or `This request is still open.` without OpenCode (lines 211–241). |
| Stage text | — | — | `[OpenCode event stream disconnected. ]<description> Observed at HH:MM UTC.` (`src/turn-progress.ts` lines 26–31). The descriptions are fixed per stage (lines 2–16), for example `tool`: `OpenCode started a tool call.`, `waiting`: `Waiting for a clarification answer.`, `queued`: `OpenCode queued the request. Delivery to the model is unconfirmed.`, `submitting`: `OpenCode has not confirmed receipt.`, `completed`: `OpenCode finished this request.` Upstream's text function can name `Claude` or `Codex` instead of `OpenCode`; another build of the same feature cannot. |
| Typing (`fleetTyping`, alongside the notices) | `TAGMSG` to the channel or nick | `+typing=active` every 3 s, `+typing=done` when the turn ends; no reply tag | — (`delivery.ts` lines 172–175 and 251–272, `src/irc/client.ts` lines 524–538) |
| Clarification question | `PRIVMSG`, as a `draft/multiline` batch where offered | `+draft/reply=<reply parent, else the request's msgid>` | `OpenCode needs clarification (not a permission approval):`, the fields and numbered options, then answer instructions such as `Only the original requester may reply in this thread: <account>: answer <token> <choice number or answer>` (`src/questions.ts` lines 297–337, `src/irc/ship.ts` lines 452–466) |
| Status on request (`<captain>: status`) | `PRIVMSG` | none in a channel: it answers in the main room | `Reply to your original request to check its status.`, `Stargate request is <status>. Backend activity is unconfirmed.` or the stage text (`ship.ts` lines 918–985) |
| Final reply or failure | `PRIVMSG` lines or `draft/multiline` batches | in a channel, **none** for a request in the main room; the root for a request inside a thread; the request's msgid in a private conversation | the agent's answer, or a fixed failure text (`delivery.ts` lines 419–503; upstream `docs/irc.md` lines 646–662) |

The progress notice is sent at low priority, and a queued notice is dropped
once the turn ends, so none follows the final reply (`delivery.ts` lines
282–302). The server adds `msgid`, `time` and `account`, and `bot` only when
the operator turns on bot mode. Stargate sends no other tag on any of these
messages.

### How Channels shows them today

- The client keeps `+draft/reply` on `PRIVMSG` and `NOTICE` as
  `Line::reply_to` and drops every other client-only tag
  (`clients/irc/src/session/lines.rs`).
- `ThreadRoots` resolves each reply to its thread root. Replies stay inline,
  and the root shows `N replies · Last reply HH:MM`
  (`spaces/irc/src/transcript/replies.rs`).
- Each progress notice is therefore one more inline **Notice** row from the
  agent, one more counted reply, one more unread line and, in a private
  conversation, one more notification.
- `TAGMSG +typing` becomes `Event::Typing { buffer, nick }`, shown as
  `<nick> is typing…` above the composer. It names no message, so it cannot
  say which request is running.
- The nickname `status` metadata from `draft/metadata-2`
  (`Event::NickStatus`) is nick-wide too. Channels shows it above the
  composer and in the members inspector through a separate change,
  `channels-agent-status`, merged into `main` before this one. The two are
  deliberately different: that status belongs to a nick and says what the
  agent is doing overall; this change's status belongs to one request, by
  its message ID. This change does not change the nick status handling, and
  never presents one as the other.

### Verdict on the wire before the marker

Before the agreed marker, Channels could not reliably tell a Stargate
progress notice from an ordinary notice:

1. Nothing machine-readable marked progress. The notice carried only
   `+draft/reply`, which every reply in the thread also carries: questions,
   answer instructions, ordinary notices and threaded final replies.
2. The only distinguishing feature was English prose, which is not a
   contract: [stargate#90](https://github.com/aiconsultancy/stargate/pull/90)
   changed it, and two builds of the same feature word it differently.
3. Client-only tags and text are the sender's to choose, so any member can
   send an identical notice.
4. In a channel, the final reply to a request in the main room carries no
   reply tag, so nothing ties it to its request.

Channels therefore reads no prose. Notices from today's release, which carry
no marker, stay ordinary messages exactly as before.

## The agreed contract

Stargate sends request progress as a `NOTICE` carrying two client-only
tags, from both of its repositories from its next release:

- `+stargate/progress=<value>`, where `<value>` is one of `received`,
  `submitting`, `accepted`, `queued`, `delivered`, `model`, `reasoning`,
  `tool`, `retry`, `waiting`, `clarified`, `reply`, `completed` or `failed`.
  A stage before the end (anything but `completed` and `failed`) may carry
  the suffix `,disconnected` when the agent's own event stream was lost.
- `+draft/reply=<the original request's msgid>`. The reply ID names the
  individual request, also when two requests share one thread, so Channels
  keys each status by that request's message ID, never by the thread root.

Channels reads the tag from one constant, `thread_status::PROGRESS_TAG`
(the client's `PROGRESS_TAG`). Any other value, any suffix other than
`,disconnected` (an empty one, as in `tool,` or `completed,`, too), a suffix
on `completed` or `failed`, the tag on a `PRIVMSG` or action, or a tagged
notice without a reply ID is not a marker: the line stays an ordinary
message and changes no status.

## Trust

Owner decision, 21 November 2026: a notice is trusted when its server
account equals the addressed nick. This overrides reviewer findings on the
trust model.

Anyone can send a client-only tag, so Channels folds a marked notice only
when the server vouches for its sender. Every condition must hold:

1. **A request Channels knows.** The buffer holds the line whose `msgid` the
   notice's `+draft/reply` names, and it is a `PRIVMSG` or action (not a
   notice). In a private conversation it is our own message.
2. **The ship that request asks.** A leading `@nick` or `nick:` addresses the
   channel request directly. Otherwise, one distinct `@nick` elsewhere takes
   precedence over a leading `nick,`. In a private conversation it is the conversation's
   partner. A self-addressed request has no expected ship: its sender's nick
   is the addressed nick, or whose sender's `account` tag, when it has one,
   is the addressed nick (compared case-insensitively in ASCII). It is not a
   request, so a member cannot fold their own tagged notices, from this nick
   or another nick on the same account.
3. **The notice comes from the account named after that nick.** The
   notice's `account` tag, which only the server sets (`account-tag`),
   equals the addressed nick, compared case-insensitively in ASCII. Who
   holds the nick, and what the member lists say, play no part.

**Why the account and the nick must match.** Account names are unique on
an IRC server, and only the server sets the `account` tag. Fleet IRC
servers tie each nick to the account of the same name (Ergo's
`force-nick-equals-account` with strict nick reservation, in
`subspace-irc/ircd.yaml.tmpl`), so only the agent's own account can use its
nick there, and they are reached only over a private network. A notice
from the account named after the addressed nick is therefore from the
agent, live, from history, from a bouncer replay or after a restart, and
no other account can fold, change or end a request's status, even one that
holds the agent's nick on a server without that tie.

**Where else the expected account could come from.** Channels has no other
trustworthy source. The Subspace API's fleet record
(`Fleet::rear_admiral_name`, `Fleet::captain_names`) and the issue
assignee's `agent.ship` give agent *names*, not IRC accounts. Captain
notifications address `@<ship>` by name and prove nothing about accounts.
If the API later publishes each agent's IRC account, the rule can also
require the account to belong to the initiative's fleet.

**When the rule does not hold, nothing folds.** Examples: the server does
not offer `account-tag`, the agent is not logged in, its account is not
named after its nick (on servers other than the fleet's, such an agent gets
no status lines, and its progress stays as ordinary notices), the request
addresses no one or itself, or the request is not loaded.

**Endings.** A request that completed or failed, by a trusted notice, is
kept as ended in the Space's state, per buffer (`thread_status::Requests`,
in `Buffer::requests`), apart from its lines, so it stays ended after the
notice is dropped. Each buffer remembers up to twice `HISTORY_MAX` endings,
the oldest forgotten first.

**Endings are stored.** Each ending is written to the Channels store as one
record (kind `Channels request ending`, id `ended:<network>:<msgid>`,
payload `{"buffer", "request", "outcome", "at"}`, the last two being
`completed` or `failed` and the ending notice's server time; see
[Settled lines](#settled-lines)) under its buffer's record parent, so
removing the buffer deletes it with the lines. The record has no title,
body or metadata, so no search finds it. Endings go in the same write as
the lines waiting with them, before those lines, in batches of up to 100
records, each one store transaction: a write cut short between batches can
leave an ending without its notice, never a notice without an ending
written before it. Should an ending still be missing, a finished request
only shows its last stage again until its completed notice reloads. On
start, each buffer's newest twice-`HISTORY_MAX` endings load into
`Requests`, also when none of its lines survive: the record names its
buffer, and endings for a buffer that holds no lines wait
(`IrcState::parked`) until it does, without adding it to the list. A
finished request therefore stays finished after a restart, a partial
reload or a Search reload. An older store without these records, or with
the records of an earlier draft of this change, loads with none; Subspace
has no users or installs yet, so there is no migration. With **Cache
Channels** off nothing is stored, and endings last only while Channels
runs. Ending records follow the store's retention like lines, counted from
their write.

**Stability.** A live line is taken in by itself, without reading the
buffer again. A history page, backfill or restore folds its buffer again
from the held lines (`state/folding.rs`). Member list changes fold nothing
again. A notice stays folded while it and its request are held: when the
line cap drops the request, its notices stop folding, show, and can be
found from Search as ordinary notices, counted as history is, unread and
highlighted only when newer than a known read marker, so restored progress
never reads as new activity; they fold again if the request loads again.
Untrusted progress notices are remembered by the request ID they name only
while they are held.

**Counting.** A trusted notice never counts as unread and never notifies,
live or from history. A live notice is checked before it is counted. History
and backfill count as before and then take any notice that turns out to be
trusted back out of the unread count once folding runs. Untrusted, unrecognised and
uncorrelated notices stay ordinary notices, counted and notified exactly as
before.

## Stage mapping

| Value | Line | Meaning |
| --- | --- | --- |
| `received`, `submitting` | Receipt unconfirmed | The runtime has not confirmed receipt. |
| `accepted`, `delivered` | Accepted | Accepted; no work seen yet. |
| `queued` | Queued | Waiting behind other work. |
| `model`, `reasoning` | Thinking | |
| `tool` | Using a tool | |
| `retry` | Waiting to retry | |
| `waiting` | Waiting for your answer (our request) or Waiting for an answer | A person must answer. |
| `clarified` | Answer received | |
| `reply` | Writing a reply | |
| `completed`, `failed` | Done in …, Failed after … | The request ended; the line settles. |

The line reads `<agent> · <label>`, the agent being the nick on its newest
trusted notice. The label is fixed text; the notice's own text is never
shown. The nick comes from the network, so the line shows it as plain text
on one line, without control characters or whitespace, cut to 32
characters with an ellipsis (`thread_status::model::agent_name`).

## Behaviour

- **One line per request**, under that request's message, after its
  reply count, while it runs and, settled, once it ended. Two requests in
  one thread each have their own line.
- **Newest wins.** The newest trusted notice by server time sets the
  stage; of two at the same time, the later one in the buffer. An older
  notice replayed later, such as from a bouncer, never replaces a newer
  stage. An update to one request never changes another.
- **Endings are final.** `completed` or `failed` settles only the matching
  request, and no other notice, later or loaded from older history, reopens
  it. The ending is kept apart from the notice (`thread_status::Requests`)
  and stored (see **Endings are stored** under Trust), so it holds after the
  notice is dropped by the line cap, after a restart, and when a partial
  or Search reload brings back earlier progress without it. A final
  answer links to its thread when Stargate supplies the reply tag, and the
  completion notice settles its status.
- **Stale.** The line shows the last known stage with `Last update HH:MM`
  and a **Stale** pill when the notice carried `,disconnected`, while the
  network is not connected, or when the notice was already held when the
  network's current connection began (`Network::held_at_connect`): restored
  from the store or seen before a reconnect. Anything received on the
  current connection, live or as history, is current.
- **Reopening.** Statuses are rebuilt from the held lines on every change,
  so a restart or a reopened thread reads the same: an ended request never
  looks active, and unfinished work from an earlier session reads as stale.
- **Messages stay messages.** Questions, answer instructions, ordinary
  notices, final replies and every untrusted notice remain visible. A
  trusted notice makes no transcript row and does not count as a reply. It
  is still stored and searchable; opening it from Search shows its request.
- **Typing** above the composer is unchanged and stays conversation-wide.
  As before, any line from a nick ends its typing indicator until the next
  refresh; a folded notice does too.
- **Appearance.** GPUI Kit's `Marker` (the row Kit offers for conversation
  status) at the caption size with a decorative Bot glyph; when stale, an
  Unplug glyph and the shared warning `Pill`. Each line is a status region
  labelled `<agent>: <label>`, with `. Stale: the last update was at HH:MM`
  when stale. It has no focus, click or keyboard behaviour.
- **Client.** The IRC client keeps only the client-only tags Channels
  reads (`subspace_client_irc::KEPT_CLIENT_TAGS`: `+stargate/progress`)
  with values of at most 64 bytes, besides `+draft/reply` in
  `Line::reply_to`, as `Line::client_tags`, live and in history, and
  stores them with the line. Every other client-only tag, from any sender,
  is dropped. A multiline message takes them from its opening `BATCH`
  only.

## Settled lines

Amended on 22 November 2026 by
[Channels navigator, request lines and composer hints](../channels-navigator-and-requests/README.md).
Owner decision: a finished request keeps a settled line, reversing the
earlier behaviour in which the line disappeared when the request ended.

- **While it runs** the line also shows the time since the request was
  sent, by the server's time, as a clock (`0:35`, `1:02:03`), and a
  five-step stage meter (Kit's `Progress`) with its word: `received`,
  `submitting`, `accepted`, `queued` and `delivered` are *accepted*;
  `model`, `reasoning`, `tool` and `retry` *working*; `waiting` and
  `clarified` *waiting*; `reply` *reply*; `completed` and `failed` *done*.
  The conversation redraws once a second only while a line runs and is not
  stale. A stale line shows its last update and the pill instead of the
  clock.
- **Once it ends** the line is quiet: **Done in 42 m** or **Failed after
  3 m**, from the request's server time to the ending notice's. The ending
  keeps its outcome and time (`thread_status::Ending`) in memory and in its
  stored record, so a restart reads the same. A record stored before then
  has neither and reads **Done**; an ending notice still held fills it in
  once, and the store is written again.
- The trust rule is unchanged: only a trusted notice ends a request.

## Limits

- Requests without an identifiable addressee, such as unaddressed messages
  in a channel where Stargate wakes on every human message, fold no notices.
- An agent whose account is not named after its nick gets no status lines
  on servers other than the fleet's: its progress stays as ordinary
  notices. The owner accepts this.
- A progress notice for a request that is not loaded stays a notice until
  the request loads and folding runs again.
- Nicks and accounts are compared in ASCII, not by the network's
  `CASEMAPPING`: the Space has no case mapping, as the agent status change
  also decided, and the owner's decision above settles it. A request to
  `bot{` on an RFC 1459 network does not match an account `bot[`, so its
  progress stays visible as notices.
- Progress received on the current connection counts as current, however
  old, also when it comes from history replayed on join: that is how
  Channels learns the current progress after a restart, and the fixture's
  captures rely on it. Staleness comes from `,disconnected` and our own
  disconnect.
- The Subspace API's update stream is not used.
