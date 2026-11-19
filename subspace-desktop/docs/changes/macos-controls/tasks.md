# Tasks: macOS controls and theme designer

## 1. macOS controls

- [x] Add the `controls` density group: four tokens per button role,
      dialog padding and gap, board-tab height and `--liquid-glass`.
- [x] Read them through `Tokens::controls` and resolve them against the
      reference rem.
- [x] Give `IconButton` a role; route `action`, `dialog_button`, shell
      chrome, overflow, composer send and `CopyDetails` through it.
- [x] Scale toolbar rows, gaps and the overflow chevron with
      `icon_button::Metrics`.
- [x] Add `glass::control` and the tinted destructive treatment.
- [x] Migrate dialog footers (Settings, About, quit, theme designer, IRC
      join, Actions, Board Lab, gallery) to `dialog_button`.
- [x] Set macOS values in Cupertino Light, Cupertino Dark and Subspace One
      Dark; Cupertino body text to 13 px.
- [x] Live preview of designer edits in every window; Cancel restores.
- [x] Tests, docs and before/after screenshots.
- [ ] Terminal dialogs (excluded workspaces).

## 2. Theme designer

- [x] Sections with native disclosure and per-section reset.
- [x] Every token the app reads: density groups, Liquid glass switch,
      GPUI Kit radii and 24 colours; the unwired ones are listed in the spec.
- [x] Tests and screenshots of each section and of a live edit.
- [ ] Optional: colour picker instead of hex fields; a Component gallery
      playground scoped to one component.
