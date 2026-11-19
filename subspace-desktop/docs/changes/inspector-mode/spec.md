# Spec: inspector mode

## Research

### What remained in the repository

- `subspace-shell` had an optional `devtools` feature that pulled in
  `gpui-devtools` and, only in a debug build with that feature, installed its
  inspector panel without a key and added **Developer > Toggle GPUI inspector**
  (`framework/host/app/src/devtools.rs`). The README said to start it with
  `make run FEATURES=subspace-shell/devtools`.
- The history before `b9392bc` (the squashed import) is not in this
  repository. Since then, `ea67b8b` updated GPUI Kit 0.7.3 → 0.9.1 and
  `gpui-devtools` 0.5.1 → 0.6.0 with no code change to the inspector.
- GPUI Kit 0.7.3 and 0.9.1 both compile their own inspector into debug builds
  (`gpui-component/src/inspector.rs`), bound to ⌘⌥I (Ctrl+Shift+I), with a
  Rust and JSON style editor. Nothing in Subspace mentioned it.

### Why it seemed removed

1. Plain `make run` and every release build had no Developer menu: it needed
   the opt-in Cargo feature and a debug build.
2. GPUI Kit's ⌘⌥I inspector was undocumented.
3. Neither picker worked in a GPUI Kit window. While picking, GPUI gives every
   element a hitbox and selects the topmost. The Kit `Root` mounts transparent,
   window-sized containers above the application (its plugin overlay,
   `WindowState` and `WindowStateLayers`, and the notification list), so the
   picked element was always a layer, never the control under the pointer. The
   scroll wheel was the only way down, one layer per notch, and it reset on
   every hover change.

### GPUI, GPUI Kit and gpui-devtools

- GPUI's inspector is compiled with `any(feature = "inspector",
  debug_assertions)`. `Window::toggle_inspector` reserves the window's last
  30rem for a panel drawn by `App::set_inspector_renderer`; picking records an
  `InspectorElementId` (the `GlobalElementId` path plus the
  `#[track_caller]` source location of the element's constructor) and fills
  `DivInspectorState` (bounds, content size, base style) for the active element
  only. `App::register_inspector_element` renders per-type state. Clicks select
  and stop picking; GPUI handles no other mouse input while picking.
  Sources: `gpui-pre` 0.3.12 `src/inspector.rs`, `src/window.rs`
  (`toggle_inspector`, `handle_inspector_mouse_event`,
  `hovered_inspector_hitbox`), `src/elements/div.rs`.
- Every `RenderOnce` component's subtree is scoped with
  `ElementId::from(type_name::<C>())` (`gpui-pre` `src/view.rs`,
  `request_layout_component`), so the element's ID path names every component
  around it, in every build.
- Zed's inspector (`crates/inspector_ui`) is debug-only. A flag to ship it in
  release builds added about 5 MB and was reverted
  ([zed#62920](https://github.com/zed-industries/zed/pull/62920), reverted by
  [zed#64248](https://github.com/zed-industries/zed/pull/64248);
  [README](https://github.com/zed-industries/zed/blob/main/crates/inspector_ui/README.md)).
- [gpui-devtools](https://github.com/joris-gallot/gpui-devtools) 0.6 targets
  `gpui-pre` 0.3, enables GPUI's `inspector` feature, and draws a fixed panel
  (source, global ID, box model, computed values, temporary style edits and a
  Keyboard tab). It has no extension point for extra sections, does not name
  components, draws no labelled overlay, and advises keeping it out of release
  builds.
- GPUI Kit 0.9.1 exposes an `inspector` Cargo feature and
  `gpui_kit::base::Root::register_plugin`, whose `RootPlugin::prepare` runs
  before the window's elements are built.

## Decisions

- **Debug builds only.** Shipping it would turn on GPUI's `inspector` feature
  for the whole GPUI build (a source location in every element, style
  reflection, GPUI Kit's inspector and editors), which Zed measured at about
  5 MB and withdrew. Release builds compile none of the inspector, the
  Developer menu, the palette commands or the Developer page.
- **Wrap GPUI's inspector.** GPUI picks, selects and reports; Subspace
  supplies the renderer (panel and overlay) and the names. The renderer
  replaces GPUI Kit's and gpui-devtools', so the `devtools` feature and the
  `gpui-devtools` dependency are removed.
- **Names come from GPUI.** Component names are read from the type-name
  scopes in the ID path; no annotation is added to the shared components.
- **⌘F12 / Ctrl+F12**, not F12: F1 to F12 show Spaces 1 to 12 and
  `FUNCTION_KEYS` reserves all twelve. GPUI Kit's ⌘⌥I (Ctrl+Shift+I) is
  rebound to the same action.
- **A Developer menu** after View, as Safari places Develop, rather than a
  submenu of View.

## Behaviour

### Opening and closing

- `ToggleInspector` (`subspace_app::ToggleInspector`) opens or closes the
  inspector in the active window, or the first window when none is active.
  It is bound to `secondary-f12` and GPUI Kit's inspector key, offered as
  **Developer > Inspect elements**, and in the command palette as
  **Inspect elements**, or **Close inspector** while it is open.
- Escape closes the inspector before anything under it receives the key. The
  panel's **Close** button and the toggle close it too.
- With Developer mode off, the action opens nothing (it still closes an open
  inspector), the Developer menu and palette commands are hidden.

### Picking and the overlay

- Opening starts picking. The element under the pointer gets an accent
  outline, its explicit padding shaded green and margin orange, and a tag
  above it (below when there is no room, kept left of the panel) reading
  `<name>  <W> × <H>`. The overlay paints above popups and registers no
  hitbox, so it cannot pick itself.
- A Root plugin (`inspector/layers.rs`) skips the Kit's window layers: before
  each frame, while the active element is a Kit root container (`src/root.rs`
  in `gpui-component` or `gpui-base`) or a transparent, window-sized Kit
  element seen before, it scrolls GPUI's pick depth down one layer, as the
  scroll wheel does. A new layer of that kind can show for one frame the first
  time it is picked.
- While picking, a shield over the panel blocks the pointer, so crossing the
  panel keeps the last application element.
- A click pins the element; GPUI does not deliver the click to the
  application. **Pick** (`inspector-pick`, latched while picking) starts
  picking again.

### Panel

| Section | Rows |
| --- | --- |
| Component | Name (copy), Inside (outermost first) |
| Element | ID (copy), Source (copy), ID path (copy) |
| Layout | Size, Origin, Content, Padding, Border, Margin |

**Copy all** copies the summary text. Copies confirm with an in-app
notification. With nothing picked yet, the panel shows "Point at an element".

### Names

- A type-name scope is a `Name` ID of the form `crate::module::Type` (generic
  arguments dropped).
- The name is the innermost shared component (`subspace_dashboard::…`),
  written without the crate as in the catalogue (`icon_button::IconButton`).
  A component whose module is inside the named one's module
  (`icon_button::content::Glyph`) is a part and does not replace it. Without a
  shared component: the innermost other Subspace component, then the innermost
  GPUI Kit component, written with its crate and with repeated module names
  collapsed (`gpui_component::button::Button`).
- Without any component, an element built in
  `framework/board-kit/dashboard/src/<module>.rs` is named by that module, for
  function helpers such as `segmented`.
- The ID is the last non-type, non-view ID in the path. The source is
  `file:line:column`; registry sources start at the crate directory.

### Developer settings

- Debug builds add **Settings > Application > Developer** (`app:developer`,
  square-dashed pointer glyph) after Experiments, with **Developer mode**
  (`settings-developer-mode`, a `toggle::switch` in a `toggle_row`) and the
  inspector's keys and use.
- Developer mode is stored as `app.developer-mode` in `state.json`; only `"0"`
  turns it off. The switch is a draft until Save; Save writes a change and
  rebuilds the native menus without a restart.
- **Developer > Developer settings…** and the palette's **Developer settings**
  (`subspace_app::OpenDeveloperSettings`) open Settings at that page.
