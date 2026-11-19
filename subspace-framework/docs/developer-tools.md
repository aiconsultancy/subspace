# Developer tools

Debug builds of every Subspace app, such as `make run` and the capture build,
include an element inspector, a **Developer** menu and a **Developer** page in
Settings. Release builds include none of them. Back to the
[documentation index](README.md).

## Build version

The product's build script stamps a `BuildInfo` (commit, dirty flag,
branch, build time, kind and host) and passes it, with the repository About
names, through `ProductBuild::build`. The shell runs no git. A debug build
marks the version it shows in the status bar, About, `--version` and
`--about`, so it is never mistaken for a release:

- `make run` and other local debug builds: `0.2.1-dev-<host>-<unix time>`,
  such as `0.2.1-dev-studio-1763290000`. The host is the building machine's
  short host name.
- Capture builds (`make capture`): `0.2.1-test-capture-<unix time>`.
  Capture builds use `capture`, so no host name appears in screenshots. A
  context without a stamped build, as in the shell's unit tests, is a test
  build of host `unknown`.

The time is when Cargo last ran the product's build script. Release builds show the bare
version. Only display text changes: update checks, `__product_metadata`, the
MCP server and the IRC client version still report the real version.

The app and the Framework have their own versions. About and `--about` show
both: the app's version line, then the Framework's under its fixed name,
then the build details:

```text
Harbor v1.2.0
Subspace Framework v0.1.0

Repository: https://example.com/harbor
Git commit: abc1234
…
```

The Framework fills its own version in; `ProductBuild` carries only the
app's. `--version` prints the app's version line alone, for scripts.
`__product_metadata` has both, as `version` and `subspace_version`, and Copy
diagnostics and Report issue name both. The MCP server, from the desktop or
`<binary> mcp`, reports the app's name and version. The status bar shows the
app's version only.

## Element inspector

The inspector shows the bounds of the element under the pointer, like the
element picker in a browser's developer tools, and names the shared component
it belongs to.

### Open and close it

- Press **⌘F12** on macOS (**Ctrl+F12** elsewhere). GPUI Kit's own key,
  **⌘⌥I** (**Ctrl+Shift+I**), opens the same inspector.
- Choose **Developer > Inspect elements** in the menu bar.
- Choose **Inspect elements** in the command palette. While the inspector is
  open, the command reads **Close inspector**.

Press **Esc**, the same key or command, or **Close** in the panel to close it.
Plain **F12** still shows the twelfth Space: F1 to F12 are reserved for
Spaces.

### Inspect an element

GPUI gives the inspector the right-hand 30rem of the window for its panel and
lays the application out in the rest.

1. Point at an element. It gets an accent outline, its padding is shaded green
   and its margin orange, and a tag above it gives its component name and size,
   such as `icon_button::IconButton  28 × 28`. The panel shows the same
   element. Scroll to reach an element hidden under another one.
2. Click to pin the element. Clicks do not reach the application while you
   pick, so pinning never activates a control.
3. Read and copy the details in the panel. **Pick** starts picking again.

| Panel row | Shows |
| --- | --- |
| Component > Name | The component's catalogue name; Copy copies it |
| Component > Inside | Every component around the element, outermost first |
| Element > ID | The nearest element ID, as tests find it with `window.find` |
| Element > Source | Where the element was built, `file:line:column` |
| Element > ID path | GPUI's full ID path |
| Layout | Size, origin, content size, and the explicit padding, border and margin |

**Copy all** copies every row as text, for a bug report or a chat.

### Component names

GPUI scopes each `RenderOnce` component's elements with the component's type
name, so the element's ID path lists every component around it. The inspector
names the innermost Subspace shared component, written relative to
`subspace_dashboard` as in the [UI component catalogue](ui-components.md) and
the Component gallery, such as `icon_button::IconButton`. A type declared in
that component's own module, such as `icon_button::content::Glyph`, is part of
it and does not replace its name. Without a shared component the inspector
names the innermost board component, then the innermost GPUI Kit one, such as
`gpui_component::switch::Switch`.

Shared helpers that are functions, such as `segmented::control`, have no type
of their own. For their elements the inspector names the module that built
them, from the source location, such as `segmented`. To give a helper its own
name, make it a `RenderOnce` type, as `IconButton` is.

**Source** is where GPUI's `div()` or other element constructor was called,
which for a shared component is inside the component. Search for the name or
the ID to find the call site in a board.

## Developer mode

**Settings > Developer > Developer mode** is on by default. Turn it off to
hide the Developer menu and the palette commands and to ignore the inspector
keys, for example while recording a demo from a debug build. Save applies the
choice without a restart. It is stored as `app.developer-mode` in `state.json`.

## Release builds

GPUI compiles its inspector only into debug builds or with its `inspector`
Cargo feature. That feature applies to the whole GPUI build: it records a
source location in every element and adds style reflection, and GPUI Kit's
own inspector, with its style editors, comes with it. Zed measured about 5 MB
for its inspector in a release build and reverted the flag that shipped it.
Subspace therefore keeps the inspector, the Developer menu and the Developer
page out of release builds.

## How it works

The code is in [`host/app/src/inspector`](../host/app/src/inspector.rs):

- GPUI's inspector (`Window::toggle_inspector`) picks the element, keeps its
  `InspectorElementId` (ID path and source location) and reports its
  `DivInspectorState` (bounds and style).
- `model.rs` derives the report: component names, the ID, the compact source
  and the box model.
- `panel.rs` draws the panel with the shared tokens and buttons; `overlay.rs`
  draws the box and the tag without hitboxes of their own.
- `layers.rs` is a GPUI Kit `Root` plugin. While picking, every element gets a
  hitbox, and the Kit's transparent window layers for dialogs, sheets and
  notifications sit above the application, so GPUI would always pick a layer.
  Before each frame the plugin scrolls GPUI's pick depth past them. It learns a
  new transparent, window-sized Kit layer the first time it is picked, so that
  layer can be outlined for one frame.

The inspector replaces GPUI Kit's built-in inspector panel and the
`gpui-devtools` panel that the removed `subspace-shell/devtools` feature
installed, so their temporary style editors and keyboard log are not
available.
