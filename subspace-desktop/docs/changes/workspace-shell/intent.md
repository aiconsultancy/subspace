# Intent: workspace shell

> Board Lab has since been renamed Space Lab; see [Space Lab rename](../space-lab-rename/intent.md).

Back to [change records](../README.md).

Every Space lays its content out for itself today: a large heading with a
line of service context, a row of view tabs, a row of filters, then the
content, all running to the window's edges. Things bunch against the sides.
An earlier attempt gave each Space an outer padding, which framed the Space
without fixing the cause and was dropped.

## Owner direction (20 November 2026)

- **More like Zed.** Panels tile to the window's edges and each other. The
  breathing room is inside each panel, on one shared text edge, not around the
  Space.
- **One layout for every Space.** The same zones in the same places: somewhere
  to choose what you are looking at, a consistent header, one place for
  filters, the content, and a side panel for the detail of the selection.
- **Issue properties in a right sidebar.** On the issue page, Properties,
  Relations and Record move out of the body into the side panel.
- **Peek, then open.** In the issue list, one click loads the issue in the
  side panel. The selected row shows an Open button, and a double-click or
  Enter opens the issue as well.
- **Views move into the navigator.** My issues, Active, Backlog, All issues
  and Activity become navigator rows, and the tab strip is kept for what is
  open.

The owner approved the mock-ups of the anatomy, the Issues list, the issue
page, Channels, Pull requests, Actions, Board Lab and a web Space on
20 November 2026. [spec.md](spec.md) records what they show.

## Added during the build

The branch that carries this change took in more work once the first
Spaces were on the shell. These parts belong to this record:

- **The deferred inspector, done.** Pull requests, Actions and Copilot
  Agents drop their old detail panel for the shell's inspector: one click
  selects a row into the inspector, and Open, Enter or a double click opens
  the item as a closable tab beside the view.
- **A repository filter in Pull requests.** The navigator's Repositories
  rows filter the list to one repository. It is the one new feature here;
  the reason for taking it in is not recorded.
- **The Issues page restyled** to the shell: plain property rows, a
  segmented workflow bar, the path as the toolbar, pencils that show only on
  hover or focus, and a one-line comment box.
- **The Channels content restyled** to the shell: the conversation's name
  and topic in the toolbar, a reading width for messages and quieter server
  lines.
- **The shell restyled to the mock-ups**: the Space's name in the accent,
  section labels in capitals, full-height pane tabs that join the content,
  plain filter buttons, a filled borderless search, one primary action per
  toolbar, and a shorter Issues toolbar.
- **Dock switches in the status bar.** The navigator's and inspector's
  switches moved from the toolbar's ends to the status bar's corners, as the
  mock-ups draw them; the owner confirmed this on 21 November 2026.
- **The Twilight Two theme**, a first-party theme with the mock-ups'
  colours and sizes, which the screenshots are taken in.

These landed on the same branch with records of their own:

- [The Decisions Space](../decisions-space/intent.md), built on the shell.
- [The standard right-click menu](../standard-context-menu/intent.md).
- [Add a Space from the title bar](../add-space-from-title-bar/intent.md).
- [Inter and JetBrains Mono embedded](../bundled-fonts/README.md) in the
  Subspace App, so Twilight Two renders in them wherever it runs.

## Out of scope

- New features in any Space, apart from the Pull requests repository
  filter above. This moves what exists; it adds the peek, the double click
  and opened-item tabs.
- Other themes' colours, sizes and fonts. They keep their own tokens; the
  mock-ups' look is the new Twilight Two theme.
- The vertical navigation rail, Settings, Search and Logs.
