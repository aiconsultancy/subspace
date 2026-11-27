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

The Cargo feature is named `terminal`. `make run`, `make run-unreleased` and
`make install-macos` build with it by default (`FEATURES=` builds without
it), and `make capture` builds with it when Zig is available through the
Terminal mise environment. The packaging targets (`package`, `package-macos`,
`release-macos` and `package-macos-ci`) build without it unless
`SUBSPACE_BUILD_FEATURES` names `terminal`, which their scripts pass to
Cargo. No script installs Zig: it comes from the Terminal mise environment,
which installs Zig 0.16.0:

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

`MISE_ENV=terminal mise run terminal-deps` fills an offline cache with the
pinned Ghostty source and every Zig package its build needs, downloaded with
curl. Builds then use the cache instead of Zig's own downloader, which fails
behind TLS-intercepting proxies with `TlsInitializationFailed` or
`HttpConnectionClosing`, and they keep working through a
`deps.files.ghostty.org` outage.

The make targets that build the terminal run the task first: `run`,
`run-unreleased` and `install-macos` unless `FEATURES` leaves out
`terminal`, and `capture` and `check-terminal`. If it fails, because there is
no network or GitHub is down, the target prints `offline Ghostty cache could
not be filled; building without it` and builds as before, from Cargo's and
Zig's own caches or your `GHOSTTY_*` variables. The task run by itself fails
with the reason. A command that goes through mise directly, such as
`mise run install-macos`, uses the cache but does not fill it.

`build` does not build the terminal. The packaging targets (`package`,
`package-macos`, `release-macos` and `package-macos-ci`) leave it out unless
`SUBSPACE_BUILD_FEATURES` names `terminal`, and they never run the task, even
then. To package with the terminal from the cache, fill it first and build
in the terminal environment:

```sh
MISE_ENV=terminal mise run terminal-deps
MISE_ENV=terminal SUBSPACE_BUILD_FEATURES=terminal make package-macos
```

**The pin.** `scripts/ghostty-pin` holds three lines: the libghostty-rs rev,
`d2036baeb605d31d09a9e2ba6d0c923e04c34d54`; the Ghostty commit that rev's
`libghostty-vt-sys` build script builds (`GHOSTTY_COMMIT`),
`3425025e585a3403340d4dc6d65132f42e05e605`; and the SHA-256 of GitHub's
archive of that commit. When you bump libghostty-rs, update all three, the
"This rev pins Ghostty" comment in `terminal/runtime/Cargo.toml` and the
commit in this page and `licenses/terminal/README.md`.
`scripts/test-ghostty-offline-env.sh` fails when the pin's rev differs from
the terminal manifests or lockfiles or when the commit differs from that
comment. Take the digest from two downloads that agree:

```sh
curl -fsSL https://github.com/ghostty-org/ghostty/archive/<commit>.tar.gz | shasum -a 256
```

The task checks the archive against the digest before it unpacks it, also
when it reuses one it downloaded earlier. On a mismatch it deletes the
archive and fails with both digests. GitHub generates these archives on
demand, so a change on its side can change their bytes. If the digest
changes without a new commit, check that the new archive's files match the
commit (for example against a `git clone` at it) before you record the new
digest.

**The cache** is `~/.cache/subspace/zig-deps`
(`$XDG_CACHE_HOME/subspace/zig-deps` when set; `SUBSPACE_ZIG_DEPS_DIR`
overrides it). It must be an absolute path on a file system with hard
links: the task refuses a relative path, builds ignore it, and the task
stops before downloading anything where hard links fail. It is outside `target/`, which a build cache can sweep,
and every checkout and worktree shares it. Everything a pin produces is
named by its commit and archive digest, so checkouts pinning different
digests of one commit, as while a regenerated archive's digest is recorded,
never share a path. It holds:

- `ghostty-<commit>-<sha256>.tar.gz`, the archive;
- `ghostty-<commit>-<sha256>`, the source tree, unpacked from the verified
  archive;
- `zig-packages`, the Zig packages, each in a directory named by its Zig
  package hash, which covers its contents, so pins share them safely and a
  build never picks up another pin's packages. `tarballs` and `cache` (Zig's
  own) hold what they were unpacked from. A download that is not an
  archive at all, such as an error page a proxy served, is removed so that
  the next fill downloads it again;
- `.complete-<commit>-<sha256>`, the marker the task writes last. Its first
  line names the archive digest, and each further line one package in
  `zig-packages` that the source's manifests name, directly or through
  another package.

Only this version of the task writes the archive, the tree, the marker and
`zig-packages`, and the archive and the tree only from bytes with the
pinned digest, so what an earlier version left, such as an unverified tree
or a half-unpacked package in its `system` store, is never used. The task
takes its own hard link to the cached archive, verifies that and unpacks
that, so the bytes it unpacks are the bytes it checked whatever another run
does meanwhile. A cached archive without its digest is damaged, since the
task publishes none: it is removed and downloaded again. A source tree
without `build.zig` is damaged too, but another run may be using it, so the
task fails and names it; remove it and run the task again. A downloaded
archive without the pinned digest fails the task with both digests.

The cache is ready for the pin when the marker names the pinned digest, the
source tree has its `build.zig` and every package the marker lists is in
`zig-packages`. Anything else, such as a missing package or a malformed
line, means not ready. The task does nothing when the cache is ready and
fills it again otherwise, from the archive when it is still there.

Several checkouts can run the task at once and the cache stays whole. Each
run unpacks and downloads into `.stage-<pid>-…` names of its own and
publishes each archive with a hard link and each tree and package with a
rename, neither of which replaces or nests in what another run published
first; the loser keeps the other run's copy, which holds the same verified
bytes. A marker is replaced only
when it does not show the cache ready. A run removes, as far as it can, only
staging whose process has exited. A lock, `.lock` in the cache holding its
holder's pid, saves a second run the same downloads: it waits up to a minute
(`SUBSPACE_ZIG_DEPS_LOCK_WAIT` seconds) and then fills the cache alongside
the first, and it takes over a lock whose process has gone without waiting.
Publishing uses Perl, which macOS and the usual Linux distributions
include. The offline cache is off on Windows for now: the task stops at once
and builds fetch as before. To start again, or after a damaged source tree,
remove the cache, `rm -rf ~/.cache/subspace/zig-deps`, or just the tree the
task names.

**Using the cache.** `mise.terminal.toml` sources
`scripts/ghostty-offline-env.sh` for every command run with
`MISE_ENV=terminal`. When the cache is ready for the pinned commit it sets
`GHOSTTY_SOURCE_DIR` to the cached source and `GHOSTTY_ZIG_SYSTEM_DIR` to
the package store, unless you set it. It never uses another commit's source,
because the crate's bindings are generated for the pinned one. If you set
`GHOSTTY_SOURCE_DIR` yourself it sets nothing: the cached packages belong to
the pinned source, not yours.


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
