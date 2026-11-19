# Tasks: inspector mode

- [x] Audit what remained: the `devtools` feature, `devtools.rs`, the README
  note, GPUI Kit's built-in inspector and the dependency update.
- [x] Research GPUI's inspector, Zed's inspector, gpui-devtools 0.6 and GPUI
  Kit 0.9.1; record the findings and sources in the [spec](spec.md).
- [x] Decide release gating: debug builds only.
- [x] Replace the `devtools` feature and `gpui-devtools` with a Subspace
  renderer for GPUI's inspector (`framework/host/app/src/inspector`).
- [x] Derive component names, ID, source and box model from GPUI's inspector
  data (`inspector/model.rs`).
- [x] Draw the outline, padding, margin and name-and-size tag without hitboxes
  (`inspector/overlay.rs`).
- [x] Skip GPUI Kit's window layers while picking (`inspector/layers.rs`).
- [x] Panel with Pick, Close, copyable Name, ID, Source and ID path, and Copy
  all (`inspector/panel.rs`).
- [x] ⌘F12 / Ctrl+F12 and GPUI Kit's ⌘⌥I; Escape closes; F12 stays with the
  twelfth Space.
- [x] Developer menu, palette commands and **Developer settings…**.
- [x] Settings > Developer page with Developer mode, saved on Save.
- [x] Tests: naming, report, overlay geometry, hover, pin, copy, Escape,
  shortcut, Developer mode, menu and palette, the shell's status bar button,
  Developer settings and the Settings page.
- [x] Docs: [Developer tools](../../developer-tools.md), README, Settings and
  the documentation index; the run-app and package script tests no longer
  name the removed feature.
- [x] Evidence: Cupertino Dark captures of hover, pinned details and the
  Developer page.
- [x] `make check`, `make interfaces`, `mise run quality`.
