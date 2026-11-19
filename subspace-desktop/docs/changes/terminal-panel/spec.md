# Terminal panel: spec

Back to the [intent](intent.md).

## Layout of the code

- **Delete the Terminal Space.** `spaces/terminal` (`subspace-terminal-board`)
  goes. No board id `terminal`, no `BoardRegistration`, no board settings
  page, no board artwork, no `known.rs` board entry. No compatibility shims.
- **New Framework crate `framework/host/terminal`**, package
  `subspace-terminal-panel`. It is an excluded Zig workspace like the old
  Space (own `Cargo.lock`, explicit fields), because it depends on
  `framework/runtimes/terminal` (`subspace-terminal`, libghostty). It owns
  what the Space owned on top of the runtime: the tab list and tab bar, New /
  Close / Next / Previous tab, Restart and Retry, the busy-tab close dialog,
  Ghostty config and theme loading and their warnings, the font settings
  editor, `quit_warnings`, help text and help images. It depends on
  board-kit, services, runtimes and lucide, never on `subspace-shell`.
- **The shell stays Zig-free.** `subspace-shell` does not depend on the
  terminal crates. Board-kit (`subspace-dashboard`) gains a small,
  product-neutral contract for one shell-docked panel, for example
  `subspace_dashboard::dock_panel`: a trait object or handle that gives the
  shell the panel's `AnyView`, its focus handle, its quit warnings, its
  minimum size, and events (`Hide`, `ToggleMaximized`, `Move(Placement)`,
  `SetBottomSpan(..)`, `Resize`). The terminal crate implements it.
- **The product composes it.** `ProductBuild` gains an optional dock panel
  factory. `products/subspace` passes the terminal panel's factory when its
  `terminal` feature is on; otherwise `None`, and the shell shows no terminal
  button, action, menu item or palette command.
- **Remove the unused per-board hooks**: `Dashboard::workspace_dock`,
  `workspace_dock_minimum`, `workspace_maximized`,
  `DashboardEvent::ResizeWorkspaceDock` and their handle forwarding and test
  fixtures. Reuse the shell's dock layout (`app/shell/dock.rs`,
  `shell/layout.rs`, dock sizes `app.dock-bottom` / `app.dock-side` and their
  clamps, `MAX_DOCK_SHARE`) for the new panel.
- Update `Cargo.toml` `exclude`, the architecture checks
  (`scripts/check-architecture.py` manifests, `scripts/test-architecture.py`
  fixtures, `scripts/architecture_source.py` exemptions), `mise.toml`
  `check-terminal`, the `make` targets and `deny`/audit paths from
  `spaces/terminal` to `framework/host/terminal`.

## Builds

- Debug builds only, for now. `make run` and `make install-macos` build with
  the terminal (`MISE_ENV=terminal`, `FEATURES=terminal`) by default;
  `FEATURES=` turns it off. Release scripts do not enable it and do not
  install Zig. `make check` stays Zig-free; `make check-terminal` checks the
  excluded crates and builds the product with the feature.
- `make capture` builds with the terminal when Zig is available through the
  terminal mise environment, so evidence can show it.

## Behaviour

- **Show and hide**: action `subspace_app::ToggleTerminal`, bound to
  `ctrl-backtick` on every platform, globally (it also hides the terminal
  when the terminal has focus). A toolbar button (Terminal glyph, label
  following the existing per-control toolbar label preference, like Chat),
  a View menu item and a palette command. Showing it focuses the current
  session; hiding it returns focus to the Space. Open state persists as
  `terminal.open`. Sessions keep running while hidden; the first show starts
  the first session lazily.
- **Placement**: Left, Right or Bottom, stored in prefs as
  `terminal.placement` (default Bottom). The old `terminal.placement` config
  key and its `LogTab` value are removed. Settings > Layout keeps a
  "Terminal placement" select (Left, Bottom, Right) and adds "Bottom
  terminal width": "Beside the chat" (default) or "Full window width".
  Settings edits apply on Save, like other Layout choices; the panel's own
  buttons apply at once and persist at once.
- **Composition** (the chat panel and its canvas are "chat" below):
  - Bottom, beside the chat (default): `h_flex[ v_flex[Space, terminal],
    chat ]`. The chat keeps the full body height.
  - Bottom, full width: `v_flex[ h_flex[Space, chat], terminal ]`.
  - Left: `h_flex[ terminal, Space, chat ]`.
  - Right: `h_flex[ Space, terminal, chat ]`; the chat stays outermost.
  - Sizes and drag handles reuse the existing dock frames and clamps; the
    Space keeps its minimum main size.
- **Maximize**: the panel fills the whole workspace body, covering the Space
  and the chat (the title bar, rail and status bar stay). Toggled by the
  panel's Maximize / Restore button and by action
  `subspace_app::ToggleTerminalMaximized` (no default shortcut, palette
  command). Hiding the terminal, or switching Space, restores it. Not
  persisted. Focus stays in the terminal.
- **Panel header**: the existing terminal tab bar with New tab, then, at the
  trailing end, glyph buttons: Move left (PanelLeft), Move to bottom
  (PanelBottom), Move right (PanelRight), Full window width (shown only at
  the bottom; latched when on), Maximize / Restore (Maximize2 / Minimize2),
  and Hide (X or ChevronDown). The button for the current placement is
  latched (`IconButton::latched`). Each button has an accessible label and a
  tooltip naming its action.
- **Button text toggle**: every repositioning button (left, bottom, right,
  full width, maximize) is a glyph-plus-text shared button whose text is
  hidden by default. One preference, `terminal.button-labels` (default off),
  shows the text on all of them. It is set from the header's right-click
  menu ("Show button labels") and from Settings > Layout, following the
  pattern of the toolbar label preferences. Use shared components only
  (`action::toolbar_button` or `IconButton` with a label); add a gallery
  card if a new shared component is needed.
- **Quit**: the shell's quit confirmation includes the panel's busy
  programs, as it did for the Space.
- **Settings**: the terminal font editor moves from the Space's board page to
  a Framework page in Settings > Application (for example "Terminal"),
  shown only when the product has a terminal. Keys are stored as before but
  not under board settings: `terminal.fontFamily`, `terminal.fontSize` in the
  config file's `terminal` object. No migration of the old board settings.
- **Help**: the Space's help guide becomes a Framework help page for the
  terminal, linked from Help, with its existing images.

## Out of scope for this change

- Dragging the panel to an edge (next layer).
- Release builds with the terminal.
- More than one terminal panel, or splitting sessions.

## Decisions made while building it

- **Contract shape.** `subspace_dashboard::dock_panel` has `Placement`,
  `BottomSpan`, `PanelLayout` (placement, span, maximized, labels, pushed to the
  panel for its latched buttons), `DockPanelEvent` (`Hide`, `ToggleMaximized`,
  `Move`, `SetBottomSpan`, `SetLabels`), the `DockPanel` trait (`show`,
  `set_layout`, `quit_warnings`, `status`, `minimum_size`, `reload`), the
  type-erased `DockPanelHandle` and `DockPanelRegistration` (`create`,
  `bind_keys`, help guide and images, key help, font settings editor).
  `ProductBuild::dock_panel` takes the registration.
- **No `Resize` event.** The shell's dock frames own the drag handles and the
  stored sizes, so the panel never resizes itself. The keyboard resize path
  went with `DashboardEvent::ResizeWorkspaceDock`; no key resizes the dock.
- **`status` and `reload`.** The Space showed Ghostty configuration warnings
  in the status bar and reread the configuration on Reload. The panel keeps
  both: the status bar joins the Space's status and the terminal's, and Reload
  reloads the terminal while it has focus.
- **Side compositions nest.** Left is `[[terminal, Space], chat]` and right is
  `[[Space, terminal], chat]`. The visual order is the spec's, but the chat
  takes its width share of the whole body and the terminal its share of the
  rest, so the Space is never squeezed to nothing on a narrow window.
- **Canvas gallery.** While the chat's canvas gallery replaces the Space there
  is no Space column, so a bottom terminal "beside the chat" spans the full
  width until the gallery closes.
- **Maximize covers boards.** A maximized terminal counts as a cover for
  `Dashboard::covered`, so a web Space hides its native view.
- **Help.** **Terminal help** (`ShowTerminalHelp`, palette) and Help opened
  while the terminal has focus show the terminal's guide in the Help panel.
- **Stored keys.** `terminal.open`, `terminal.placement`,
  `terminal.bottom-span` (`beside-chat`, `full-width`) and
  `terminal.button-labels` in the state file.
- **Toolbar.** The Terminal control is a new `ToolbarControl` with its own
  label preference (`app.toolbar-label.terminal`), shown only for a product
  with a terminal.
- **Gallery.** The header composes existing shared components
  (`action::toolbar_button` with `latched`, `action::icon_button`), so there is
  no new component and no gallery card; the gallery cannot build the Zig
  panel.
