# Intent: IRC thread status

The owner asked for one live agent status line inside each IRC request
thread in Channels. This is a separate Desktop enhancement; the agent
gateway ships independently.

Today Channels shows `<agent> is typing…` above the composer. Stargate, the
agent gateway that connects a fleet's agents to IRC, also sends progress
notices as IRC replies to the original request. Someone who opens that
thread should see what the agent is doing now, without reading a list of
status messages.

What the owner asked for, condensed:

- First inspect the messages Stargate actually sends and how Channels
  groups them. Use the IRC reply's message ID to associate progress with the
  correct request. Typing is channel- or conversation-wide, so it cannot say
  which request is running. Prefer the existing IRC path over the Subspace
  API's update stream.
- Show one compact line in the open thread, such as `<agent> · Thinking`,
  `<agent> · Using a tool` or `<agent> · Waiting for your answer`, updated in
  place as confirmed progress arrives. Keep routine progress from appearing
  twice in the thread.
- Keep queued, receipt unconfirmed and working distinct. When the connection
  drops, show that the last observation is stale. Clear the active status
  when the final reply arrives. Reopening a completed thread must not make it
  look active again.
- Keep native questions, answer instructions, ordinary notices and final
  replies visible as messages, and keep the composer's typing line.
- Fold only notices Desktop can reliably identify as Stargate progress. If
  the wire format needs a small machine-readable marker, explain the gap
  before expanding the change into Stargate.
- Start with a disposable fixture showing two requests to the agent in one
  channel, proving that an update to one leaves the other alone, and cover a
  clarification, a disconnect and a final reply.

The agent is whoever the IRC nick is: nothing here names a particular agent,
fleet or server.

## The agreed marker

After the findings, the owner agreed the marker with Stargate, which sends
it from its next release, and set the final Desktop behaviour:

- Progress is a `NOTICE` with `+stargate/progress=<stage>` and
  `+draft/reply=<the original request's msgid>`. The reply ID names the
  individual request, also when two requests share one thread.
- Fold a notice only when it comes from the expected ship's
  server-authenticated account and its reply ID matches a request Desktop
  knows; without a trustworthy source for that account, do not fold.
  Untrusted, unrecognised or uncorrelated notices stay ordinary notices,
  counted and notified as today. Never infer state from text, a nickname,
  typing or an untagged final reply.
- Keep receipt unconfirmed, queued and waiting for a person apart; show the
  last stage as stale on a disconnect; let `completed` or `failed` clear only
  its own request, for good; fold live and replayed progress out of unread
  counts, reply counts and notifications; one compact line per request.
