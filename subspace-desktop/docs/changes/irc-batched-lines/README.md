# Longer Channels drafts

Connections that negotiate IRCv3 multiline can send plain-text drafts of up
to 100 lines and 4,096 bytes. The client divides the draft into batches that
respect the server's advertised limits. Connections without multiline, slash
commands, notices, actions and the agent message tool retain their ten-line
limit. Escaped commands beginning with `//` are plain text.

The focused IRC client and Channels suites passed. On macOS, the loopback Ergo
fixture showed the old overflow warning for a 12-line draft and delivered the
same draft as one multiline message after the change.

| Before | Draft ready to send | Delivered message |
| --- | --- | --- |
| [Blocked draft](evidence/before-draft.png) | [Ready draft](evidence/after-draft.png) | [Message](evidence/after-sent.png) |
