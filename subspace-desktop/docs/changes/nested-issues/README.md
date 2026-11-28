# Nested issues

SUB-89 makes the Issues list easier to scan and removes the old first-load
message from Issues and in-app Help.

## Behaviour

- **Show sub-issues** places returned children below their parents in the
  parent's workflow group. The indent and connector lines show the hierarchy;
  each row keeps its own status and keyboard actions. The heading counts
  top-level entries as pages arrive, and the status bar counts all matches.
- Search and filters keep matching children visible in their own state
  group when the parent is absent or its branch is folded. Groups retain
  their existing pagination, and keyboard navigation prefetches near the
  end of the displayed rows.
  In nested mode the first page of a folded group is also read so its
  children can appear with parents in open groups.
- **Hide sub-issues** switches back to the parent-only list. Existing saved
  preferences for showing sub-issues open in nested mode.
- The list and issue page use the shared skeleton and toolbar activity mark
  while loading. The read-only Issues settings use the shared spinner while
  connecting.

## Verification

- The list projection tests cover ordering, cross-state children, deeper
  descendants, and matching children without their parents.
- The Issues tests use a local fixture to check a finished child under an
  open parent, keyboard and toolbar toggling, and loading and recovery states.
- The toolbar uses the shared action button and list rows use the shared
  indent support. Connector lines belong to the Issues row's leading lane
  and use the theme's list spacing and muted colour.
- The workspace, palette, keyboard shortcuts, status bar and Help retain the
  existing Issues integration. Registration, contracts, tools and connections
  require no changes for this work.

## Fixture captures

Captured in Twilight Two with the isolated desktop fixture. The list and
detail reads were delayed for the loading captures. The fixture identity
read was delayed for Settings.

| State | Before | After |
| --- | --- | --- |
| Sub-issues hidden | [Before](evidence/before-hidden.png) | [Show sub-issues](evidence/after-hidden.png) |
| Sub-issues shown | [Before](evidence/before-shown.png) | [Nested under the parent](evidence/after-shown.png) |
| List loading | [Before](evidence/before-loading.png) | [List loading](evidence/after-loading.png) |
| Detail loading | [Before](evidence/before-detail-loading.png) | [Detail loading](evidence/after-detail-loading.png) |
| Settings connecting | [Before](evidence/before-settings.png) | [Connection spinner](evidence/after-settings.png) |
