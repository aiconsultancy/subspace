# Menu bar

Every app built from the Framework gets the same menu bar, laid out as each
platform's own apps lay theirs out. The shell builds it in
[`host/app/src/app/menu_bar.rs`](../host/app/src/app/menu_bar.rs) after it
binds its keys, so each item shows the key that runs the same command. Items
use title case, as native menu commands do on macOS and Windows.

## macOS

| Menu | Items |
| --- | --- |
| *App name* | About *name*; Settings… (⌘,); Services; Hide *name* (⌘H), Hide Others (⌥⌘H), Show All; Quit *name* (⌘Q) |
| Edit | Undo (⌘Z), Redo (⇧⌘Z); Cut (⌘X), Copy (⌘C), Paste (⌘V), Select All (⌘A) |
| View | Command Palette (⌘K), Search Items (/), Search Content (⇧⌘F); Chat (⇧⌘C), Terminal and Terminal Layout, Navigator, Inspector, Vertical Space Navigation; Next Space (⇧⌘]), Previous Space (⇧⌘[); Wrap Titles (W), Theme Designer (\`), Reload (⌘R) |
| Developer | Inspect Elements (⌘F12); Developer Settings… |
| Window | Minimize (⌘M), Zoom |
| Help | *Name* Help (?), Keyboard Shortcuts (⌘?); Show Logs (⇧⌘L), Report an Issue… |

- AppKit adds Enter Full Screen (⌃⌘F) to View, and the window list and, from
  macOS 15, the tiling commands to Window. ⌃⌘F is also the shell's own key,
  so it works while a text field has focus.
- Edit's items are GPUI Kit's text commands, tied to the system's cut, copy,
  paste and select-all actions, so a focused text field, selectable text or
  web page handles them. Undo and Redo reach GPUI's own fields only, not a
  web page, which keeps its own ⌘Z and ⇧⌘Z. The terminal keeps its own copy
  and paste keys.
- The window and app commands are handled app-wide, so they work while a
  dialog has focus, and Hide, Hide Others and Show All work with no window
  open.
- macOS reports a shifted punctuation key as the character it types, so
  ⇧⌘] arrives as ⌘}. The Space and Chat tab keys are bound both ways.
- Chat's own previous and next tab keys, ⇧⌘[ and ⇧⌘], win while Chat has
  focus: the Space keys are bound outside Chat's key context, because GPUI
  ranks a binding without a context above Chat's.

## Windows and Linux

| Menu | Items |
| --- | --- |
| File | Settings…; Exit on Windows, Quit on Linux |
| Edit | As on macOS |
| View | As on macOS, then Full Screen |
| Developer | As on macOS |
| Help | *Name* Help, Keyboard Shortcuts; Show Logs, Report an Issue…; About *name* |

Hiding apps and the Window menu are macOS-only: GPUI cannot hide apps
elsewhere, and the title bar minimises and maximises the window. GPUI keeps
this menu bar on Windows and Linux but does not draw it, so these layouts are
ready for an in-window menu bar.

## Which items appear

- Terminal and Terminal Layout appear only in an app that docks a terminal.
- Report an Issue… appears only with the `report-issue` feature and when the
  app names a repository ([Report an issue](report-issue.md)).
- Developer appears only in debug builds while Developer mode is on
  ([Developer tools](developer-tools.md)). Settings applies a change to it
  at once.
- An item whose command cannot act, such as Navigator in a Space without
  one, does nothing.
