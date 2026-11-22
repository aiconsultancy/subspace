# Spec: Twilight Two for new installs

## Where the theme is kept

The chosen theme is `appearance.theme` in the product's state file,
`state.json`, not in `config.json`. On macOS the state file is in
`~/Library/Application Support/subspace/`; on Linux in
`$XDG_STATE_HOME/subspace/`; on Windows in `%LOCALAPPDATA%\subspace\`.
Pre-release channels use `subspace-<channel>`. The config file is
`config.json` in `~/.config/subspace/` or the platform's own folder
(Application Support on macOS, `%APPDATA%` on Windows,
`$XDG_CONFIG_HOME` on Linux), whichever exists.

With no `appearance.theme`, or one naming a theme that is no longer there,
the app shows the product's default theme. The Subspace App's default stays
**Cupertino Dark**, the Framework default, so every install that never saved
a theme looks as it did.

## Telling a new install

A new install is one where neither the product's config file nor its state
file exists when the desktop app starts. The product binary checks this
before anything else writes either file: before the built-in Fleet Console
adds itself to `config.json`, and before the shell loads `state.json`. Each
file is checked without following links, so a dangling link counts as a
file.

| At desktop start | What happens | What it shows |
| --- | --- | --- |
| No config file and no state file | A new state file is written with `appearance.theme` set to `Twilight Two` | Twilight Two |
| A state file with a saved theme | Nothing is written | The saved theme |
| A state file without a saved theme, or a config file without a state file | Nothing is written | Cupertino Dark, as before |
| A config or state file that is unreadable or malformed | Nothing is written; the app handles that file as before | Whatever it showed before: the saved theme if the state file can be read, otherwise Cupertino Dark |
| A folder the files would be in cannot be read | Nothing is written; the reason is printed to standard error | Cupertino Dark, as before |

Once recorded, Twilight Two is the user's saved choice like any other:
Settings shows it selected, and changing the theme replaces it.

The Framework does the check and the write in
`shell::record_new_install_theme(&context, theme)`, which also refuses a
theme that is not a variant of the product's catalogue. The Subspace App's
entrypoint calls it with `subspace_descriptor::NEW_INSTALL_THEME`
(`Twilight Two`) only when it starts the desktop app, at the same point as
the built-in web Spaces, never for a command-line or headless run. A build
that sets `SUBSPACE_DEFAULT_THEME` records nothing, so every install of that
build shows the build's default as before.

## Light and dark

Subspace has no setting that follows the system appearance, and the app
never reads it: each theme variant carries its own light or dark mode, and
applying the variant sets the window's mode. Twilight Two has only a dark
variant, so a new install on a light-mode system opens in Twilight Two,
dark, as the owner decided. A user who wants a light window chooses a light
variant, such as Cupertino Light, in **Settings > Appearance**.

## What does not change

- The Framework default, `subspace_product::DEFAULT_THEME`, stays Cupertino
  Dark, and products that do not call `record_new_install_theme` behave as
  before.
- The Subspace descriptor's `default_theme` stays Cupertino Dark.
- `make capture` writes both fixture files, so a capture is an existing
  install and shows Cupertino Dark without `CAPTURE_THEME`.
  `CAPTURE_FIRST_RUN=1` writes neither, for evidence of a first run, and the
  new `resize:<width>,<height>` capture step sizes a window that has no saved
  bounds.
