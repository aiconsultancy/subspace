# Terminal

The terminal runs local shell sessions in tabs, in a panel that the shell
docks beside every Space, as it does the chat. It is a Framework feature, not a
Space. Its user guide is the [terminal help page](../terminal/panel/help.md),
which the Help panel shows. The platforms are macOS arm64, Linux x64, and
Windows x64.

## Code layout

- `terminal/runtime` (`subspace-terminal`) runs sessions on a
  pseudo-terminal with libghostty and draws one session (`TerminalView`).
- `terminal/panel` (`subspace-terminal-panel`) is the panel: the tabs,
  New, Close, Next and Previous tab, Restart and Retry, the busy-tab close
  dialog, Ghostty configuration and theme loading, the font settings editor,
  the quit warnings, the header's layout buttons and the help guide. It
  implements board-kit's dock panel contract
  (`subspace_dashboard::dock_panel`) and never depends on `subspace-shell`.
- The shell (`subspace-shell`) stays Zig-free. It owns whether the terminal
  shows, its placement, bottom width, size and maximize, and reaches the panel
  only through the contract. A product passes the panel's registration in
  `ProductBuild::dock_panel`; `products/subspace` does so under its
  `terminal` feature. Without one the shell shows no terminal button, action,
  menu item, palette command or Settings page.

## Layout

`Ctrl+`` ` shows and hides the terminal on every platform (`ToggleTerminal`).
The toolbar's **Terminal** button, **View > Terminal** and the palette's
**Show terminal** do the same. Showing it focuses the current session and
starts the first session lazily; hiding it returns focus to the Space.
Sessions keep running while it is hidden.

| Placement | Composition |
| --- | --- |
| Bottom, beside the chat (default) | `[[Space / terminal], chat]`: the chat keeps the full body height |
| Bottom, full window width | `[[Space, chat] / terminal]` |
| Left | `[[terminal, Space], chat]` |
| Right | `[[Space, terminal], chat]`: the chat stays outermost |

The header's layout buttons (Move left, Move to bottom, Move right, Full
window width at the bottom, Maximize or Restore) are glyphs whose text shows
only when **Show button labels** is on (header context menu, or **Settings >
Layout**). The button for the current placement and the Full window width
state are latched. **Maximize** (`ToggleTerminalMaximized`, no default key)
fills the workspace body, covering the Space and the chat; hiding the terminal
or switching Space restores it. The sizes reuse the shell's dock frames,
clamps and 60% share, stored as `app.dock-bottom` and `app.dock-side`.

The shell stores `terminal.open`, `terminal.placement` (`left`, `bottom`,
`right`), `terminal.bottom-span` (`beside-chat`, `full-width`) and
`terminal.button-labels` in the state file. The header's buttons apply and
save at once; Settings applies on Save. Maximize is not stored.

## Enable and build

The Cargo feature is named `terminal`. Only debug builds enable it for now:
`make run` and `make install-macos` build with it by default (`FEATURES=`
builds without it), and `make capture` builds with it when Zig is available
through the Terminal mise environment. Release scripts do not enable it and do
not install Zig. The Terminal mise environment installs Zig 0.16.0:

```sh
MISE_ENV=terminal mise run build -- --features terminal
```

The Terminal crates are excluded from the normal workspace checks because they
need Zig. Run their gate with:

```sh
MISE_ENV=terminal mise run check-terminal
```

Normal `make check` stays Zig-free. `check-terminal` also builds the real
product binary with the feature enabled.

### Windows (experimental)

Windows support is experimental. CI tests it on a Windows runner, but it has
had little use on Windows desktops. Report problems with the Windows version,
the shell and the steps that caused them.

On Windows, sessions run on a ConPTY pseudo console. Windows 10 version 1809 or
later is required. The differences from macOS and Linux are:

- The shell is `SHELL` when it is set. Otherwise it is `pwsh.exe` from `PATH`,
  then `powershell.exe`, then `ComSpec`, then `cmd.exe`. Windows has no login
  shells, so the shell starts as given.
- The home directory is `HOME`, or `USERPROFILE` when `HOME` is unset. The
  Ghostty configuration is read from `%USERPROFILE%\.config\ghostty`, or from
  `XDG_CONFIG_HOME` when it is set.
- Copy is Ctrl+Shift+C and paste is Ctrl+Shift+V. Windows has no primary
  selection, so `copy-on-select = true` copies to the clipboard and the default
  copies nothing.
- A Windows session never reports a busy foreground program, so closing a tab
  does not ask for confirmation unless `confirm-close-surface = always`.
- Closing a session closes the pseudo console, which ends the attached
  programs. A shell that stays is stopped after a short wait.

On a Windows host, `cargo test --manifest-path terminal/runtime/Cargo.toml`
runs the ConPTY session tests. The GPUI view tests run on macOS and Linux only.

### Offline Zig dependencies

Machines behind a TLS-intercepting proxy can prefetch the Ghostty source and
all Zig packages with:

```sh
MISE_ENV=terminal mise run terminal-deps
```

The task downloads Ghostty commit
`3425025e585a3403340d4dc6d65132f42e05e605`, follows every `build.zig.zon`
manifest, and stores the packages under `target/zig-deps`. It prints export
commands for `GHOSTTY_SOURCE_DIR` and `GHOSTTY_ZIG_SYSTEM_DIR`. Evaluate those
commands before building offline. This is also the fallback when
`deps.files.ghostty.org` is unavailable.


## Keys

`Ctrl+`` ` shows and hides the terminal on every platform, also while a
session has focus. Tabs use these keys while a session has focus:

| Action | macOS | Linux |
| --- | --- | --- |
| New tab | Cmd+T | Ctrl+Shift+T |
| Close tab | Cmd+W | Ctrl+Shift+W |
| Next tab | Cmd+> | Ctrl+Page Down |
| Previous tab | Cmd+< | Ctrl+Page Up |

On macOS, Cmd+K opens the palette, Cmd+R reloads, Cmd+, opens Settings, Cmd+Q
quits, Cmd+Shift+C copies, and Cmd+V pastes. On Linux, app keys use
Ctrl+Shift: Ctrl+Shift+K, Ctrl+Shift+R, Ctrl+Shift+,, and Ctrl+Shift+Q.
Ctrl+Shift+C copies and Ctrl+Shift+V pastes. Super+Shift+C toggles chat on
Linux. Plain Ctrl keys go to the terminal program on Linux.

Cmd+1 through Cmd+9 on macOS, and Ctrl+1 through Ctrl+9 on Linux, select Space
positions. F1 through F12 select Spaces when the session does not have focus;
they go to the program when it does.

## Configuration

In **Settings > Terminal** (under Application), choose an installed fixed-width family and
enter a size from 8 to 48 points. **Save** updates all open sessions and new
tabs. Choose **Automatic** for the family or clear the size field to inherit
each value independently:

1. The saved Subspace override.
2. The layered Ghostty configuration.
3. On macOS, the family, weight and size from the default Terminal.app
   profile for new windows and tabs, when its font is available.
4. The code font and size in Subspace Appearance.

An unavailable Ghostty family falls back to the Terminal.app face and size
together. A Ghostty size on its own still overrides the profile size. Subspace
uses the same character-width check in the font picker and profile import.

A selected installed family applies to regular, bold, italic and bold italic
text. Automatic keeps any Ghostty style-specific families. An unavailable
Subspace family falls back to Automatic. These choices are saved as
`fontFamily` and `fontSize` in the config file's `terminal` object; Automatic
removes the key.

Subspace reads configuration files in this order. Later values override earlier
values:

1. `$XDG_CONFIG_HOME/ghostty/config` (or `~/.config/ghostty/config`).
2. On macOS, `~/Library/Application Support/com.mitchellh.ghostty/config`
   and its `config.ghostty` companion when present.
3. Subspace's `ghostty/config` and `ghostty/config.ghostty` layer.

`config-file` includes are followed. For a named `theme`, Subspace searches the
Subspace layer `themes` directory, the Ghostty config `themes` directory, and
the Ghostty resources `themes` directory. An absolute theme path is used
directly. `light:A,dark:B` selects a theme by the current Subspace appearance.
Theme colours are applied first; configuration colour keys always override
them. Missing colour values use the Subspace appearance defaults.

### Supported keys

| Key | Values and default |
| --- | --- |
| `background`, `foreground`, `cursor-color`, `cursor-text` | `#RRGGBB`; Subspace theme when unset |
| `palette` | `index=#RRGGBB` entries |
| `selection-background`, `selection-foreground` | `#RRGGBB`; derived when unset |
| `cursor-style` | `block`, `bar`, `underline`, `block_hollow`; `block` |
| `cursor-style-blink` | `true` or `false`; `true` |
| `font-family` and style variants | Font family values in order; inherited family when unset |
| `font-size` | Number in points; inherited size when unset |
| `adjust-cell-height` | Points or percentage; no adjustment |
| `scrollback-limit` | Bytes; 10,000,000 |
| `term` | Terminal name; `xterm-ghostty` when available, otherwise `xterm-256color` |
| `command` | Command string; login shell when unset |
| `working-directory` | `home`, absolute path, or `~/...`; home |
| `clipboard-read`, `clipboard-write` | `allow`, `deny`, or `ask`; read `ask`, write `allow` |
| `clipboard-paste-protection` | `true` or `false`; `true` |
| `clipboard-paste-bracketed-safe` | `true` or `false`; `true` |
| `copy-on-select` | `none`, `primary`, `clipboard`, `both`, or `true` |
| `confirm-close-surface` | `false`, `true`, or `always`; busy-tab rule |
| `mouse-scroll-multiplier` | Number; `1` |
| `window-padding-x`, `window-padding-y` | Number of points; `2` |
| `keybind` | `text:`, `esc:`, or `csi:` action |
| `theme` and `config-file` | Theme name/path and include path |

Subspace ignores unsupported keys. A supported key with an invalid value keeps
its default, keeps other settings, and adds a warning with the file and line.
Empty values clear optional values or restore the documented default. Invalid
theme names produce one warning and leave colour defaults in place.

Use Reload (`Cmd+R` on macOS or `Ctrl+Shift+R` on Linux) to reread configuration,
themes and the macOS default terminal profile. Reload applies live settings to every open session without
restarting Subspace. `command`, `working-directory`, and `term` affect only new
tabs. Editing a file does nothing until Reload. A changed Subspace appearance
also recomputes token-derived colours and fonts, including the selected half of
a `light:`/`dark:` theme pair.

If `$SHELL` is empty or unset, Subspace uses `/bin/sh`. If it names a missing or
non-executable file, Subspace reports the configured shell error and offers
Retry; it does not silently replace it with `/bin/sh`.

## Clipboard

Copy does nothing without a selection. It preserves line breaks and removes
padding added by wrapped lines. Paste sends clipboard text to the program and
uses bracketed-paste markers when the program enables that mode. Copy and paste
are also available from the session context menu.

While composing text through an input method, the pre-edit text is clipped to
the number of cells remaining on the cursor's row. The clipped text is only a
display limit; committed text is sent to the program normally.

`copy-on-select` chooses no automatic copy, the Linux primary selection, the
system clipboard, or both. The value `true` means primary selection on Linux
and clipboard on macOS. OSC 52 writes are allowed unless `clipboard-write` is
`deny` or `ask`. OSC 52 reads require `clipboard-read = allow`.

When `clipboard-paste-protection` is true, potentially unsafe pasted text asks
for confirmation. `clipboard-paste-bracketed-safe` permits bracketed paste
without that prompt when the terminal program has enabled bracketed paste. When
protection is false, paste is sent without that confirmation. The rule applies
to clipboard paste, not text typed directly into the session.
