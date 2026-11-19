# IRC conversation polish: hover, Shift+Return, the member list and removing items

The owner used the IRC board in Subspace v0.2.0 and wanted it to behave more
like Slack. This change covers four of the five fixes in the Channels interaction improvements; drag
selection across the transcript is a separate change.

1. The transcript row under the pointer is shaded.
2. Shift+Return adds a line break and Return sends; the hint says so.
3. The member list fills its dock up to the window's edge.
4. A channel row's menu offers **Leave channel** and a private conversation's
   offers **Close conversation**. A removed item stays out of the list after a
   restart, a reconnect or a history replay.

- [Intent](intent.md): what the owner asked for and the choices made.
- [Spec](spec.md): behaviour, saved state and keys.
- [Delivery](delivery.md): tests, checks and before and after screenshots.
- [Tasks](tasks.md).
