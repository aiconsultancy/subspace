# Spec: Cupertino theme

## Theme

- `framework/host/app/themes/cupertino.json` defines **Cupertino Light** and
  **Cupertino Dark** with product theme id `cupertino`. It is in the Subspace
  catalogue after One Dark; One Dark stays the default.
- Materials: sidebar and title bar `#EDEDEA` / `#1B1B1A`, content `#F7F7F5` /
  `#20201F`, raised surfaces `#FDFDFC` / `#2C2C2A`, muted `#E8E8E5` /
  `#323230`, selection `#E3EBF6` / `#303B4C`, primary `#0A84FF` / `#4C8DFF`.
- Metadata: system UI font for headings and body, Menlo for code, 15 px
  headings, 14 px body, 12.5 px code, 32 px rows, 38 px top bar, 44 px
  toolbars, 720 px detail panel, 8 px row inset, 6 px control radius and 12 px
  surface radius. Switch colours meet the WCAG 1.4.11 3:1 rule.

## Tokens

- New density tokens: `--chrome-h` (top bar, default 30 px), `--row-inset`
  (default 0), `--radius` (default 4 px) and `--surface-radius` (default 6 px).
  `Tokens::control_radius` and `surface_radius` read the radius tokens.
- New colour tokens mapped from GPUI Kit's semantic theme: `chrome` and
  `chrome_line` (title bar), `sidebar`, `sidebar_line`, `nav_select` and
  `nav_select_fg` (sidebar and sidebar accent) and `surface` (popover).
  One Dark's theme file sets the matching GPUI Kit keys to its existing values.

## Shell and shared components

- The top bar uses `chrome`; the status bar uses GPUI Kit's `status_bar`.
- The vertical board rail uses `sidebar`; the current board, in both rail and
  tabs, uses `nav_select` and `nav_select_fg`.
- Dialogs, menus, the command palette, toasts, shortcuts, the theme designer,
  the help dock, the chat panel and the detail panel use `surface`. Dialogs add
  a shadow and a semibold title. Settings navigation uses `sidebar`.
- List rows never draw a leading selection bar (Design Guides). With
  `--row-inset` above 0, hover and selection are rounded fills inset by that
  much, rows have no separators, and group headers have no fill and muted
  titles.
- `.SystemUIFont` is always offered as an installed family, so automatic font
  choices use the platform UI font instead of falling back to Helvetica Neue.
- On macOS the toolbar is the window title bar for every theme: transparent
  title bar, traffic lights in its leading edge, drag and double-click on its
  background. Presses that a control claims never drag.

## Evidence capture

- The debug-only `subspace-shell/capture` feature renders the app's own windows
  to PNG with `SUBSPACE_CAPTURE_DIR` and `SUBSPACE_CAPTURE_STEPS` (`wait`,
  `action`, `click`, `scroll`, `shot`), with no screen-recording permission and
  no foreground activation.
- Debug builds honour `SUBSPACE_GITHUB_API_URL`, so captures can use a local
  fixture GitHub API.
