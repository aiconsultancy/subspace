# IRC attachment delivery

## Intent

Receivers need the request and its attachment keys in the same IRC message.
Previously, once the composer ran out of space on one line, it sent the
draft and upload references as separate messages. A receiver could handle
the draft before seeing the references.

## Behaviour

Keep the draft and every upload reference in one PRIVMSG or negotiated IRCv3
multiline batch. The client refuses before writing if the server can't carry
them together. The composer retains the draft and uploaded files for retry.

## Delivery and checks

The attachment composer uses the same multiline transport as text sends,
with a one-message requirement. The tests check the client wire output for
a long draft with two files and verify draft retention on a server without
multiline support.
