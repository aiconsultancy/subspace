# Intent: IRC board

Give Subspace an IRC client that feels like the other boards: networks and
their buffers in the shared list, the open buffer in the detail panel, and the
same shared messaging pieces Teams uses (Kit `MessageScroller`, `Message`,
`ChatInput`). It should speak modern IRCv3 (server time, chat history, read
markers, typing, SASL) so it is useful on networks such as Libera.Chat and
Ergo, and follow Halloy's proven model: a server buffer per network, a topic
line, a member list, condensed joins and parts, highlights and nick
completion.

The protocol and connections live in `clients/irc`; the board only applies the
runtime's events and sends its commands, so the runtime stays testable without
GPUI and the board stays testable without sockets.
