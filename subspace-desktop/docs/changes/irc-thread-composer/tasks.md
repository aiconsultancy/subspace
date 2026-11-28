# Verification

## Layout and interaction

- [x] Verify both editors are visible with a thread open.
- [x] Send a reply and check the IRC channel and original message ID.
- [x] Write in both editors, switch roots and reopen the inspector.
- [x] Remove the original from loaded history and recover the unsent draft.
- [x] Check the compact composer on the Component gallery card.
- [x] Compare the two Twilight Two captures with the workspace shell.

### Board checklist — Channels (`irc`)

| Area | Status |
| --- | --- |
| Identity, descriptor, registration, artwork and retention | Existing Channels registration and retention policy apply. |
| Workspace, text edge, strips and docks | The shared workspace and existing dock switches own the layout. The channel uses the shared text edge. The inspector uses its existing scrolling body and pinned footer. |
| Design guide and components | The shared Kit-based `ChatInput`, buttons and popup menu supply the editor and controls. The gallery demonstrates the compact style. |
| List hooks, status and busy state | Channels retains its conversation dock and connection status. The upload tray keeps its progress and recovery. |
| Commands, modal state and keyboard | Existing channel actions and editor contexts still apply. The composer actions have keyboard-reachable buttons and use the existing Escape behaviour for menus and completion. |
| Empty, unavailable and quit states | The Thread tab keeps its empty state. It explains when replies cannot send and offers recovery for unsent drafts. The existing quit policy applies. |
| Settings, connections and flags | Not applicable because this changes the existing editors without adding configuration. |
| Data, caching and refresh | Existing history and preferences apply. The reply drafts live in the retained conversation and stay out of stored state. |
| Search, indexing and references | The transcript and its reference handling stay in place. **Jump to message** uses the existing transcript reveal. |
| Agent tools, CLI, contracts and chat context | Not applicable because the change is a human-facing editor. |
| Notifications, logging and diagnostics | Existing message delivery and attention behaviour apply. The editors add no log output. |
| Help and documentation | The guide and development notes document both editors and recovery. |
| Tests and evidence | Headless interaction tests and fixture captures cover the changed path. |
