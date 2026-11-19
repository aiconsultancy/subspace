# IRC issue previews

Show an issue's title and state beside the IRC message that mentions it so readers can decide whether to open it.

## Behaviour

- Show one tile per readable issue beneath each message. Show its identifier, title, state and tracker or archive markers. The existing card contract carries these fields.
- Recognise bare identifiers and supported Markdown and app links. Keep code literals plain and leave unresolved and unavailable references usable.
- Resolve visible rows through the existing Issues contract and keep cards scoped to the current viewer's session. Profile, organisation, or access changes remove the tiles.
- Selecting a tile or a resolved reference opens the issue in Subspace through the responding board. The Issues board checks the link's profile and organisation on receipt.
- The message row owns the tile and lookup. The replies pane can reuse that row.

## Board checklist

- **Layout and keyboard.** The transcript keeps the tile as part of the message row. The shared outline card provides a native Button. The transcript's existing reference keys open the same issue.
- **Contracts and navigation.** Reuse the release Issues contract for scope and batch resolution. The shell routes through the registered responder and the destination checks the scope again.
- **Data and privacy.** The IRC store and search retain only the existing messages. Cards and scope stay in the board's memory, and navigation requires an active responder.
- **Testing and evidence.** Use the offline Issues responder to cover formatting, deduplication, unavailable cards, scope changes, keyboard navigation and the tile click. Capture the native transcript with fictional data.
- **Existing integration.** This change adds no boards, configuration, agent tools or notifications. The IRC guide describes the tiles and navigation.

## macOS evidence

IRC messages with inline references.

The same messages with automatic issue tiles.
