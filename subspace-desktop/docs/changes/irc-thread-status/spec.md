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
  (`Event::NickStatus`) is parsed but not shown on this branch. It is
  nick-wide too. A separate change, `channels-agent-status`, shows it above
  the composer and in the members inspector. The two are deliberately
  different: that status belongs to a nick and says what the agent is doing
  overall; this change's status belongs to one request, by its message ID.
  This change neither shows nor changes the nick status handling, and never
  presents one as the other.

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

Channels reads the tag from one constant, `thread_status::PROGRESS_TAG`. Any
other value, a suffix on `completed` or `failed`, the tag on a `PRIVMSG` or
action, or a tagged notice without a reply ID is not a marker: the line
stays an ordinary message and changes no status.

## Trust

Anyone can send a client-only tag, so Channels folds a marked notice only
when the server vouches for its sender. Every condition must hold:

1. **A request Channels knows.** The buffer holds the line whose `msgid` the
   notice's `+draft/reply` names, and it is a `PRIVMSG` or action (not a
   notice). In a private conversation it is our own message.
2. **The ship that request asks.** In a channel, the request's first word
   addresses one nick: `nick:`, `nick,` or `@nick`, the forms the composer
   completes. In a private conversation it is the conversation's partner.
   A request that addresses no one, or only further on in its text, has no
   expected ship.
3. **That ship's server-authenticated account.** The account the IRC server
   reports for that nick in the member lists: the channel's own, or, for a
   private conversation, those of the network's channels. The client fills
   them from `WHOX` after each join, `extended-join` and `account-notify`
   (`Member::account`); only the server sets them.
4. **The notice comes from that account.** The notice's `account` tag, which
   only the server sets (`account-tag`), names the same account, compared
   case-insensitively in ASCII.

**Where the expected account comes from.** Channels has no other trustworthy
source. The Subspace API's fleet record (`Fleet::rear_admiral_name`,
`Fleet::captain_names`) and the issue assignee's `agent.ship` give agent
*names*, not IRC accounts; the fleet installer happens to derive accounts as
`<fleet slug>-<name>`, but that is installer configuration, not data the
API publishes, so Channels does not assume it. Captain notifications address
`@<ship>` by name and prove nothing about accounts. The request's addressee
is therefore the only per-request indication of the expected ship, and only
the server's account data turns it into an identity. The nick is used to
look the account up, never as proof of identity. If the API later publishes
each agent's IRC account, the rule can also require the account to belong to
the initiative's fleet.

**When there is no trustworthy source, nothing folds.** Examples: the
server does not offer `account-tag`, the ship is not in the channel (or, for
a private conversation, in any channel we share), `WHOX` has not answered
yet, the request addresses no one, or the request is not loaded.

**Stability.** Folding is re-evaluated after every new line, history page,
backfill, restore and member list change (`state/folding.rs`). A notice
folds as soon as the server vouches for it and stays folded while its line
is held, so a ship leaving the channel later does not bring its progress
back as messages.

**Counting.** A trusted notice never counts as unread and never notifies,
live or from history. A live notice is checked before it is counted. History
and backfill count as before and then take any notice that turns out to be
trusted back out of the unread count once folding runs, including when the
member list arrives after the history page. Untrusted, unrecognised and
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
| `completed`, `failed` | no line | The request ended. |

The line reads `<agent> · <label>`, the agent being the nick on its newest
trusted notice. The label is fixed text; the notice's own text is never
shown. The nick comes from the network, so the line shows it as plain text
on one line, without control characters or whitespace, cut to 32
characters with an ellipsis (`thread_status::model::agent_name`).

## Behaviour

- **One line per active request**, under that request's message, after its
  reply count. Two requests in one thread each have their own line.
- **Newest wins.** The newest trusted notice in the buffer, in order, sets
  the stage. An update to one request never changes another.
- **Endings are final.** `completed` or `failed` clears only the matching
  request, and no other notice, later or loaded from older history, reopens
  it. A final reply without a reply tag in the main room, or any untagged or
  `PRIVMSG` reply, clears nothing.
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
- **Client.** The IRC client keeps the client-only tags of a `PRIVMSG` or
  `NOTICE`, except `+draft/reply`, as `Line::client_tags`, live and in
  history, and stores them with the line. A multiline message takes them
  from its opening `BATCH` only.

## Limits

- Requests that do not address the ship in their first word, such as
  messages in a channel where Stargate wakes on every human message, fold
  nothing: Channels has no expected account for them.
- The expected account is the one the server reports now, not when the
  request was sent. If the ship's nick later belongs to another account,
  that account's notices on the ship's open requests would be trusted.
  Servers that tie nicks to accounts, as Subspace's Ergo does with
  `force-nick-equals-account`, rule this out.
- A progress notice for a request that is not loaded stays a notice until
  the request loads and folding runs again.
- The Subspace API's update stream is not used.
