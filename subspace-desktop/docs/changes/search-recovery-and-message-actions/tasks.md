# Tasks

- [x] Verify hidden-match recovery for Issues and Decisions.
- [x] Anchor the reaction and Stop menus and verify their wire commands with
  the IRC test transport.
- [x] Verify thread preview, pinning and unpinning from the transcript.
- [x] Verify the local Chat setting gates the native text menu.
- [x] Update the native and web component galleries and the Space guides.
- [x] Verify that Ship member initials stay visible in the native capture.


## Board checklist for Issues, Decisions and Channels

- [x] Identity, registration and artwork use the existing three Space
  descriptors. Their existing lifecycle governs disabling and retention.
- [x] Workspace layout keeps views in the navigator, searches in their
  toolbars and thread content in the inspector. Recovery uses the shared
  empty-state action. Menu and inspector controls use the native text
  menu, anchored popup and segmented tabs. The component galleries
  demonstrate the enclosing row menu.
- [x] Search recovery uses the corresponding list's loading and empty
  states. Toolbar and dock actions keep their existing keyboard routes.
  The row menu and reaction anchors dismiss through GPUI Kit.
- [x] Stored views and project choices use the existing preference keys.
  Search probes are background reads. Drafts stay bound to the selected
  thread's original message.
- [x] List search and exact identifiers use the existing API queries
  within the selected initiative. Item search and full-text indexing
  continue through the Spaces' existing integrations.
- [x] Chat menu actions use the shared selection attachment and running
  Chat engine availability.
- [x] Space help documents the interactions. The developer index links
  those guides and their developer pages.
- Settings and credentials, external reference handling, agent tools,
  MCP, CLI, notifications, board contracts and process lifecycles are
  outside this interaction change.
- [x] Native tests and fixture captures cover the changed interaction.
