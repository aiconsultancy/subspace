# Board config files

## Intent

The owner asked that boards have their own configuration: "IRC should be in
`~/.config/subspace/irc/config.json`". Native board settings lived inside the
product config file under `boardSettings.<id>.settings`, mixed with service
settings, tokens and board choices. Each native board now owns one file, so
a person or an agent can edit a board's settings without touching the main
config, and the file's top-level object is the board's settings with no
wrapper.

## Spec

- A native board's settings file is `<directory of the config file>/<board
  id>/config.json` (`subspace_core::config::board_file`). With the config file
  at `~/.config/subspace/config.json`, IRC's is
  `~/.config/subspace/irc/config.json`. The file's top-level object is the
  board's settings (for IRC, `{ "networks": [...], "highlights": [...] }`).
- The `boardSettings` section of the config file is gone. It is not read, not
  migrated and not written; a leftover one is an unknown key and kept as is.
  Web boards (`webBoards`), board choices (`boards`), board order and every
  service setting stay in the config file.
- Loading: `config::load_with_boards(file, ids)` reads the config file and
  the file of each id that has one into `Config::board_settings`. The host
  passes every registered board of the product, at desktop startup and for
  headless `mcp`. A missing file or folder is no notice. A file that cannot be
  read, is not valid JSON or is not an object gives a config notice naming the
  file, and the board gets no settings. Like the config file's tokens, a board
  file that other users can read or write is ignored, with a notice that says
  to run `chmod 600`, because board files may hold passwords.
- Saving: Settings opens each settings-editor board's file
  (`SettingsFile::open_board_file`) inside its transaction. A file that cannot
  be opened is named in Settings' error and that board's editor is left out,
  so Save cannot overwrite it. Save (`ServiceSettings::board_files`) checks
  that the config file and every changed board file are unchanged since they
  were opened before writing any of them; then it writes the config file and
  each board file atomically through the private file writer: the file
  owner-only (`0600`), its folder owner-only (`0700`). Keys the board does not
  set stay in its file. Errors name the board file.
- After Save the shell mirrors the saved board files into the `Services`
  config, so boards that observe it (IRC's hub) apply the change without a
  restart. Reloading configured web boards keeps the board settings in memory.
- `Services::board_config_file(id)` gives boards their file without a host
  dependency.
- IRC: the board reads its settings from its file; **Show in Finder** on the
  empty board and on Settings > IRC reveals `irc/config.json`, or its nearest
  existing folder before the first save; **Copy agent prompt** names the IRC
  file and the config file (for `boards.irc`), and describes the file's
  top-level schema. Settings > General still reveals the config file.

## Delivery

- Core: `config/board_files.rs` (path and loader), `config::load_with_boards`,
  `settings/board_file.rs` (one board file in the Settings transaction),
  `SettingsFile::open_board_file`, `board_file_values`,
  `ServiceSettings::board_files` and `SettingsError::BoardFile`. The
  `boardSettings` parser and the `boardSettings` special case in the board
  field writer are removed.
- Board kit: `Services::board_config_file`.
- Host: `tool_environment` takes the registered boards; `board_editors` opens
  and saves board files; `mirror_services` applies the saved board files;
  `reload_configured_boards` keeps `board_settings`.
- IRC: `config::file`, `config::product_file`, the prompt's `Files`, editor
  and empty-state reveal, interface text naming `irc/config.json`, and the
  per-network channel hint now pointing to Settings > IRC.
- Docs: `docs/irc.md`, `docs/settings.md`, `spaces/irc/help.md`.
- Existing files are not migrated. Move `boardSettings.irc.settings` from the
  config file to `irc/config.json` by hand (`chmod 600` the file and
  `chmod 700` its folder).
- No interface layout changed; text changes are in the IRC help, the IRC
  Settings notes, the join hint and the agent prompt.

## Tasks

- [x] Load board files beside the config file with notices and the owner-only
  check.
- [x] Remove `boardSettings` from parsing and writing.
- [x] Save Settings board editors to their own files with stale checks and
  private permissions.
- [x] Expose the path to boards through `Services`.
- [x] IRC: read, reveal and prompt with the per-board file.
- [x] Tests: core loader and Settings transaction, host editor and startup
  loading, IRC prompt and setup.
- [x] Docs and this record.
