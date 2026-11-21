# Channels agent status lines

Back to the [change records](../README.md).

Agents publish a nick-wide status line while they work, through the IRCv3
`draft/metadata-2` key `status`: a gateway sets it to a rotating line such
as "Galactica is thinking…" while a turn runs and clears it when the turn
ends. The [earlier protocol slice](../irc-agent-status/README.md) taught the
IRC client to read it and report `Event::NickStatus`, but the Channels
Space ignored the event, so nobody could see what an agent was doing.

The owner asked for the status to show in two places:

- **Above the composer**, where "alice is typing…" shows today. While an
  agent in the open conversation has a status, its status takes that place
  for that nick; typing notices keep working for everyone else.
- **In the members inspector**, with the member.

The status describes the nick, not a request: it must never be presented
as being about a particular message, and never go into the transcript. It
is untrusted text: show only what was sent, as one plain line of limited
length, with no Markdown, link, mention or issue-reference rendering.

## Supersedes the 17 November 2026 decision

The protocol slice recorded the settled points of 17 November 2026:
"typing gates the future row" and "no members-panel status". On
21 November 2026 the owner superseded both:

- A status shows whether or not its nick is typing. A status takes the
  place of that nick's typing notice; it is not gated by it.
- The members inspector shows each member's status.

The other points stand: people and agents follow the same rules (any nick
with a status shows it), and there is no status editor or Stop button.

See the [behaviour](spec.md), [delivery](delivery.md) and
[checklist](tasks.md).
