# An idiomatic menu bar

## Intent

The macOS menu bar did not look like a Mac app's. The app menu held the
command palette, both searches, the log view, Help and Keyboard Shortcuts
beside About, Settings and Quit, and there was no Edit, Window or Help menu,
so the system's Services, Hide, Minimize, Zoom, window list and tiling
commands were missing. Lay the menu bar out as Apple's apps do, and give
Windows and Linux the layout their apps use, from one shared model.

## Spec

The menus and their gating are in [Menu bar](../../menu-bar.md).

- `host/app/src/app/menu_bar.rs` builds the menu bar for a `Layout` (`Mac`,
  `Windows` or `Linux`) from a `MenuBar` (the app's name, whether it docks a
  terminal, whether Report an Issue is offered, and the Developer menu). It
  replaces `keys::menus`, and `install_menus` moves there with it.
- macOS: the app menu holds About, Settings…, Services, Hide, Hide Others,
  Show All and Quit. Edit holds GPUI Kit's `input` Undo, Redo, Cut, Copy,
  Paste and Select All, each tied to its `OsAction`. View holds the palette,
  searches, Chat, the terminal, the Space's navigator and inspector, vertical
  navigation, Space switching, Wrap Titles, Theme Designer and Reload.
  Window, titled exactly so that AppKit treats it as the windows menu, holds
  Minimize and Zoom. Help holds *name* Help, Keyboard Shortcuts, Show Logs and
  Report an Issue….
- Windows and Linux: File holds Settings… and Exit (Windows) or Quit (Linux);
  Help ends with About; View ends with Full Screen. There is no Window menu
  and no Hide items: GPUI's `hide_other_apps` panics on Windows and does
  nothing on Linux, and the title bar minimises and maximises.
- The Developer menu follows View, as Safari's Develop menu does, and Help is
  always last. Its items are now title case, like every other menu item.
- New actions: `ToggleFullScreen`, `HideApp`, `HideOtherApps`,
  `ShowAllApps`, `MinimizeWindow` and `ZoomWindow`. Every platform declares
  them, so the macOS layout builds and is tested anywhere, but only macOS
  handles the last five and binds their keys. Their handlers are app-wide
  (registered with the keys), so they act while a dialog has focus, and
  Hide, Hide Others and Show All act with no window open.
- New keys, macOS only: ⌘H, ⌥⌘H, ⌘M, ⌃⌘F, and ⇧⌘] and ⇧⌘[ for the next and
  previous Space. No shell or Space binding used them; Chat's own ⇧⌘] and
  ⇧⌘[ for its tabs win while Chat has focus, as the Space keys are bound
  outside Chat's key context (`!SubspaceChat`). The shortcut list gains Space
  switching and full screen on macOS.
- macOS lists no full screen item of its own: AppKit adds Enter Full Screen
  to the View menu, with the label for the window's state.
- Not built: an in-window menu bar for Windows and Linux. GPUI stores the
  menus there but draws nothing. GPUI Kit's `AppMenuBar` can draw them from
  the same model once the app sets them with `set_app_menus`
  (`vendor/gpui-base/src/global_state.rs`); that is a separate change with its
  own title bar layout and UI evidence.

## Delivery

- Tests in `host/app/src/app/menu_bar_tests.rs`: the exact macOS menu bar,
  the Windows and Linux layouts, the terminal's items only with a
  terminal, the Developer menu after View with Help last, Report an Issue
  only with the feature and a repository, the new macOS keys reaching
  their commands as macOS reports them, Chat's tab keys winning while Chat
  has focus, and the window and app commands being available with no
  focused window. Since [Review follow-ups](../review-follow-ups/README.md),
  every action item of each layout is checked against its command, by the
  action's type, with each Edit item's GPUI Kit action and its `OsAction`,
  and the Windows and Linux Full Screen item's.
- Not verified headlessly: what AppKit adds (Services' contents, Enter Full
  Screen, the window list, the macOS 15 tiling commands, Bring All to Front
  and the Help menu's search field). Check them in a built app.

## Tasks

- [x] Build the menu bar per platform in `menu_bar.rs`, with tests.
- [x] Add the window and app commands and their macOS keys.
- [x] Update the docs that named menu items, and add [Menu bar](../../menu-bar.md).
- [ ] Check the AppKit-supplied items in a built macOS app.
- [ ] Decide on an in-window menu bar for Windows and Linux.
