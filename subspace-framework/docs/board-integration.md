# What the Framework gives a board

A board, now called a **Space** (the terms are interchangeable and Space is
preferred; see the [glossary](glossary.md)), is one crate under
`spaces/<id>/` that implements `Dashboard` for one GPUI view. The Framework does the rest: it puts the board in the switcher,
gives it a Settings page, serves its agent tools to chat, MCP and the CLI,
indexes its content for search, shows its notifications and help, saves its
choices and carries the little that boards share with each other. None of that happens unless the board opts in. This page lists
every hook a board can implement and everything the Framework hands it, in
plain terms, so you can decide for each one whether a board needs it.

Use the [board checklist](board-checklist.md) to record those decisions for a
new or changed board. An app can keep an audit of which of its boards use
what.

The interface lives in `board-kit/dashboard/src/lib.rs` (`Dashboard`)
and `board-kit/dashboard/src/list.rs` (`ListDashboard`). The host
reads it through `BoardRegistration` in `host/app/src/registry.rs`
and `DashboardHandle` in `board-kit/dashboard/src/handle.rs`. The
[developer overview](../README.md#the-dashboard-interface) has the short
member table; this page explains when and why to use each one.

## How the Framework sees a board

The Framework meets a board at three moments, and each needs different
things from it:

1. **Registration, without a view.** `BoardRegistration::new::<D>()` reads
   the board's constants and static functions: `ID`, `TITLE`, `DESCRIPTION`,
   `ICON`, `ARTWORK`, `HELP_MARKDOWN`, `HELP_IMAGES`, `SEARCHABLE`,
   `RETAIN_WHEN_DISABLED`, `settings()`, `settings_editor()`,
   `connection_settings()`, `contracts()` and `agent_tools`. Nothing here may create a view,
   touch GPUI or load credentials. This is what lets the headless `subspace mcp`
   command and the CLI serve a board's tools with no window, and lets Settings
   show a disabled board's page.
2. **Process lifetime, without a view.** `start_background` runs at startup
   (and when Settings enables the board) and `stop_background` when Settings
   disables it. Use it for work that must outlive the window, such as IRC's
   connections and mention notifications.
3. **The view.** `new` creates the view once; the app keeps it while the board
   is enabled and the window is open, so the board keeps its state across
   switches. `activate` and `deactivate` run as the board becomes visible or
   hidden, `background_refresh` runs on the shell's automatic schedule and
   `reload` runs on `r` or Cmd/Ctrl+R.

A board talks back to the shell with `DashboardEvent` (`cx.emit(...)`): `Toast`,
`OpenPalette`, `Opened`, `Chat(ChatRequest)` and `ConfigChanged`. It reads what the shell provides from globals: `Services`,
`Wrap`, `Density` (through `Tokens`), `CurrentProduct`, `chat_sessions` and
`SearchNavigation`. Boards never call each other: they exchange typed
messages through contracts and the Framework's broker, as
[Communicating with other boards](#communicating-with-other-boards) describes.

## Native resources for product adapters

An app adds a native account with `ProductBuild::account` (an
`AccountRegistration` naming its identity provider; see the README's
Accounts section). A product adapter, or a Space crate the app adds, can call
`shell::account_session` to snapshot the active account. Resolve an
explicitly registered resource on a background thread. The result belongs to
the snapshot's generation and consumer only. Keep resource tokens in native
service code and reject stale work before applying its results. A web page or
bridge cannot request a token.

A product with a long-lived API client can supply
`AccountRegistration::configure`. The shell calls it before Spaces start. The
product binds the `AccountTokenSource` to its registered service. An adapter
pins each client to an account generation and discards that client's work and
cached data on account replacement. From the start of a sign-in or sign-out
until the shell records its outcome, the source refuses every lease with
`Error::Cancelled`, and a committed account gets a new generation, so a client
created during the switch is stale once the switch completes. A provider
advances its generation in the same step as it installs a new identity, and
keeps a rotated refresh credential even when the request that rotated it went
stale.

## Identity and registration

### ID, title and description

- **What**: `Dashboard::ID` is the stable lower-case identifier. It is also
  the view's key context, the prefix of its stored choices (`<id>.<name>`),
  the notification source, the search board name, the MCP tool group, the
  board's config folder (`<config dir>/<id>/config.json`) and the owner prefix
  of its feature flags (`board.<id>.`). `TITLE` is the switcher label and
  window title. `DESCRIPTION` is the navigation tooltip; a product can
  override it with `BoardRegistration::description`.
- **Use it when**: always. Choose the ID once: renaming it is a breaking change
  for stored state, config files, tool names and search records.
- **Example**: `spaces/irc/src/lib.rs` (`ID = "irc"`, `TITLE = "IRC"`).
- **Tests**: `host/app/src/registry.rs` rejects duplicate IDs and
  feature keys outside `board.<id>.`.

### Registration and the product descriptor

- **What**: a product lists its boards twice: a `BoardDescriptor` (ID, aliases,
  whether it is enabled by default) in
  `services/product/src/known.rs`, and a `BoardRegistration` in its
  entrypoint, `products/subspace/src/main.rs`. Descriptor order is switcher
  order and gives the function key (F1 to F12).
- **How**: add the crate to the product's `Cargo.toml`, then
  `BoardRegistration::new::<subspace_name_board::Name>().description("…")`.
  Configured boards, such as web boards, use `BoardRegistration::configured`
  with `with_configuration` and `with_field_source`; they get no agent tools,
  search, features or background work of their own. A field source's
  `BoardFields` lists the fields, notes and grants Settings shows;
  `with_app_secrets()` adds a read-only count of the secrets the board's app
  saved itself in its section's `secrets`, beside the declared fields, with
  **Clear**, which applies on Save. Settings never reads their values.
- **Use it when**: always. Do not add per-board code to the shell.
- **Tests**: `products/subspace/src/main.rs` checks icons, artwork, help
  contents, the documented tool table and which boards are retained.

### Icon and artwork

- **What**: `ICON` is a monochrome SVG path for the switcher and Settings.
  `ARTWORK` embeds the board's own assets (`BoardArtwork::new(&[(path,
  include_bytes!(…))])`), captured at registration. `with_order` sets catalog
  precedence when two boards ship the same path.
- **Use it when**: always, for the icon. Keep the files in `spaces/<id>/assets/`;
  never put board artwork in host or product assets.
- **Example**: `spaces/irc/src/lib.rs`.
- **Tests**: `current_boards_have_unique_registered_icons` and
  `composed_artwork_keeps_keys_bytes_and_legacy_listing_order` in
  `products/subspace/src/main.rs`; add the new path's fingerprint there.

### Retain when disabled

- **What**: `RETAIN_WHEN_DISABLED = true` keeps the view alive, without a tab
  or refresh, when Settings disables the board, so shells or workers it owns
  do not end silently.
- **Use it when**: the board owns processes (Space Lab). Otherwise
  leave it false.
- **Tests**: `only_boards_that_own_processes_stay_hidden_when_disabled`.

## Layout and UI

Every UI change follows the [Design and Coding Guides](../AGENTS.md#canonical-design-and-coding-guides)
and uses shared controls from the [UI component catalogue](ui-components.md).

### Workspace layout

Every Space uses the same layout, built from `subspace_dashboard::workspace`.
Panels tile to the window's edges and to each other, separated by hairlines;
the room is inside each panel. A Space draws no heading of its own: its name
is the navigator's title. The [design guide](design-guide.md) is the full
statement of the layout, its rules and how reviewers check it; this section
is the summary.

| Zone | Build it with | It holds |
| --- | --- | --- |
| Navigator, the leading dock | `workspace::Navigator`, `workspace::nav_section` and `workspace::NavRow` | Where the person is looking: the Space's name in the header, then its views, projects, networks or repositories. |
| Tab strip, at the top of the pane | `workspace::Pane::tabs` with a `tabs::pane_tab_bar` | What is open: the list, and an opened item beside it. A Space's views are navigator rows, not tabs. |
| Toolbar, under the tab strip | `workspace::Toolbar` | Search and filters on the leading side, actions on the trailing side. |
| Content | The children of `workspace::Pane` | Lists run the full width. Reading content, such as an issue's body or a conversation, is no wider than `workspace::READING_WIDTH_PX`. |
| Inspector, the trailing dock | `workspace::Inspector`, `workspace::section` and `workspace::fact`; `Inspector::tabs` with `workspace::InspectorTab` for the [approved exception](design-guide.md#inspector-tabs) | The detail of the selection, with the actions that act on it in the footer. |
| Status bar, the shell's | `Dashboard::status`, `Dashboard::docks` and `Dashboard::toggle_dock` | Connection, account, counts and sync time; the navigator's switch at its leading end and the inspector's at its trailing end. |

`workspace::Workspace::new(id).navigator(..).pane(..).inspector(..)` places
the three columns and owns the docks' widths and drag handles. Leave a dock
out to hide it. A shared list keeps its own detail panel as the trailing
dock: draw it with `list::render_under`, passing `workspace::pane_head` for
the tab strip and toolbar, so the panel's header lines up with the tab strip.

- Put service, account and status context in `Dashboard::status`, not in a
  line above the content.
- Start text at `workspace::text_edge(&tokens)` in every panel, not at a
  side inset of the Space's own. Dock headers, navigator rows, the toolbar
  and shared list rows already start there.
- Keep two stored choices in `prefs`, both shown by default: whether the
  navigator shows and whether the inspector shows. Report them from
  `Dashboard::docks` as a `workspace::SpaceDocks` (a dock is `None` where
  the Space has none, such as an inspector with nothing to inspect), and
  flip and store one in `Dashboard::toggle_dock`. The shell draws the
  switches in the status bar (`workspace::dock_switch`) and redraws it after
  a toggle. While the navigator is hidden, start the toolbar with an
  `action::menu_button` that names the current view, so the views stay
  reachable.
- Show selection through the row's own fill, never a leading bar.
- A toolbar has at most one primary action, the Space's main one, at its
  trailing end, built with `action::main_action`; its other actions are
  neutral or quiet. Use `fields::toolbar_search` for the toolbar's search.

Check the Space at the normal window size and in a narrow window, with both
docks shown. The toolbar's leading items wrap onto another line and its
trailing items stay on the first. The Component gallery shows each part in
its states: `workspace::Navigator`, `workspace::Toolbar`,
`workspace::dock_switch`, `workspace::Pane` and `workspace::Inspector`.

### List boards

- **What**: `ListDashboard` and `list::render` give grouped rows, folding,
  keyboard selection, the detail panel, row menus and empty states. Hooks:
  `list`/`list_mut`, `detail`, `on_open` (start loading the panel's data here,
  never in `render`), `on_selection_changed`, `on_panel_toggled`,
  `on_activate`, `row_menu`, `row_commands` (the board's own menu commands,
  each in its [group](design-guide.md#context-menus)), `extend_row_menu`,
  `row_submenus`, `group_menu`, `group_indicators`, `group_captions` (quiet
  labels inside a group, such as SHIPS and PEOPLE, that are not rows),
  `row_leading`, `row_emphasis`, `row_accessible_label`,
  `empty_message`, `empty_actions`, `panel_extra`, `panel_footer`,
  `panel_scroll_handle`, `panel_content_owns_scroll`, `min_list_width` and
  `can_navigate`. Change selection from outside the list's keys with
  `list::update`, so the hooks still run.
- **Use it when**: the board shows items with details. Boards with another
  layout (Space Lab, Web) draw their own view with the same widgets.
- **Example**: `spaces/github/src/lib.rs`; `spaces/irc/src/view.rs` for avatars
  and attention rows.

### Toolbar, status, badge and busy

- **What**: `render_toolbar` draws controls after the switcher (use
  `overflow_toolbar` when they may not fit). `status` is the short text in the
  bottom status bar: the connection, the account, counts and sync time. A
  Space shows none of these above its content. `docks` and `toggle_dock`
  give the status bar its dock switches (see
  [Workspace layout](#workspace-layout)); the defaults report none. `badge` marks the switcher
  tab, such as an unread count. `busy` drives the reload button's spinner.
  `window_title` adds context after the board title.
- **Use it when**: status and busy for any board that loads data; badge only
  for something waiting for the person.
- **Example**: `spaces/irc/src/lib.rs` (`status`, `badge`); `spaces/web/src/lib.rs`
  (`window_title`).
- **Tests**: `subspace_dashboard::test_support::ToolbarTestAppContext` mounts a
  real toolbar at a given width.

### Palette commands, keys and key help

- **What**: `commands` returns `PaletteCommand`s for ⌘K: an action, a callback
  or a sub-list (`SubList`). `subject` names the selected row in the palette
  header. `bind_keys` binds the board's keys once, in
  `view_keys(Self::ID)` for plain keys. `keys` and `help_foot` fill the key
  help; the default is the list keys (`LIST_KEYS`), so a board that is not a
  list must override `keys`. F1 to F12 are reserved for boards.
- **Use it when**: every action reachable from a toolbar or menu should also
  be a palette command; the palette is how keyboard users and new users find
  things.
- **Example**: `spaces/github/src/lib.rs` (`commands`); `spaces/irc/src/lib.rs`
  (`keys`).

### Menus, dialogs and toasts

- **What**: `modal` tells the shell a board menu or dialog is open, so shell
  and board keys stand down; `dismiss` closes it before the palette opens.
  `DashboardEvent::Toast` shows a short message (`Toast::info`, `Toast::error`,
  with an optional https link).
- **Use it when**: the board opens its own menu or dialog.
- **Example**: `spaces/irc/src/lib.rs` (`modal`, `dismiss` for the join dialog).

### Covered

- **What**: `covered` tells a board with a native child view (a web view) to
  hide it while a shell overlay, or a maximized terminal, covers the board.
- **Use it when**: required for any native child view.
- **Example**: `spaces/web/src/lib.rs` (`covered`).

A board has no dock or maximize hooks. The one panel the shell docks beside
every Space, the terminal, is product-composed through
`subspace_dashboard::dock_panel` (`DockPanel`, `DockPanelRegistration`,
`ProductBuild::dock_panel`), not through a board. The Subspace App's
terminal panel is an example.

### Quit warnings

- **What**: `quit_warnings` names running programs before the app quits.
- **Use it when**: quitting would end something the person started.
- **Example**: the dock panel contract has the same hook
  (`host/terminal/src/panel.rs`); no current board needs it.

### Theme tokens, density and shared widgets

- **What**: `subspace_dashboard::tokens` maps the active theme, and its
  `ThemeId` global names the bundled theme file the active variant comes from
  (`one-dark`, `cupertino`, or `custom` for a user's theme); `Density`,
  `Typography` and the shared widgets (`workspace`, `feedback`, `panel`,
  `menu`, `composer`, `chat_input`, `rich`, `code`, `dates`, `toggle`,
  `icon_button`, `action`, `chart`, `native_grid`) keep boards consistent.
- **Use it when**: always. No hard-coded colours; `Glyph::rgb` only for colours
  the service defines.
- **Reference**: [UI component catalogue](ui-components.md),
  [shared widgets](../README.md#shared-widgets), [Appearance](appearance.md).

## Settings and configuration

### The board's Settings page

- **What**: `settings()` returns static `BoardSettings`: an `icon`, a Markdown
  `description`, nested `sections`, `features` and `live_features`, and two
  opt-ins that make the shell draw shared controls: `notifications` (with
  `notification_defaults`) and `background_updates`. Every included board gets
  an Enabled switch without shell code.
- **Use it when**: always, at least for the description.
- **Example**: `spaces/irc/src/lib.rs` (notifications on, with OS delivery and
  the Glass sound by default).
- **Tests**: registration validates section IDs and feature specs.

### Service connections

- **What**: `connection_settings()` lists the shared service editor's fields the
  board needs: `ConnectionField::GitHubCredentials`, `Token(…)`,
  `Repositories` or a `Note`. Tokens stay in the private config editor; never
  copy them into UI state.
- **Use it when**: the board uses a shared service account (GitHub today).
- **Example**: `spaces/github/src/lib.rs`, `spaces/actions/src/dashboard.rs`.
- **Reference**: [`ConnectionField`](../board-kit/dashboard/src/settings.rs) and the shell's Settings dialog.

### The board's own config file and editor

- **What**: each native board can keep its settings in
  `<config dir>/<id>/config.json`. The host loads it into
  `Config::board_settings` at startup; `Services::board_config_file(id)` gives
  the path. `settings_editor()` supplies board-owned controls inside Settings'
  Save and Cancel draft. Files are owner-only and ignored when others can read
  them.
- **Use it when**: the board has its own settings, such as IRC's networks.
- **Example**: `spaces/irc/src/config.rs` and `spaces/irc/src/editor.rs`.
- **Reference**: `Services::board_config_file` in the board kit.

### Feature flags

- **What**: `FeatureSpec`s in `BoardSettings::features` or a section's
  `features`, with IDs under `board.<id>.`, appear on the board's page and
  under Experiments. List live ones in `live_features`; others need a restart.
- **Use it when**: shipping an experiment or a capability that people can turn
  off.
- **Example**: `spaces/space-lab/src/lib.rs`.
- **Reference**: [Features and experiments](feature-flags.md#register-a-board-feature).

### What `Services` holds

`subspace_dashboard::services(cx)` returns the `Services` global: `config`
(the loaded `Config`, including `repos` and `board_settings`), `credentials`
(GitHub tokens by priority and `github_accounts`), `tools` (the selected
boards' tool catalog), `search` (the full-text service, when a board is
searchable) and `config_file`. Take `Arc` clones into background tasks. When a
board writes the config file itself, as `repositories::apply` does, it updates
`Services::config` and emits `DashboardEvent::ConfigChanged` so the shell
rebuilds the tools and the MCP catalog. Service changes otherwise need a
restart.

`CurrentProduct` holds the product's immutable context, for a board that needs
the product's name or files.

## Data caching and refresh

The pattern: **a board persists its data in the Framework's SQLite full-text
store** (the search service's per-board database) and reads it back through
the [board storage API](#board-storage), so the same records restore the board
and answer global search. `prefs` is for small UI choices. A cache file is only
for a small view snapshot that is not searchable; new boards should not add
one, and existing ones are candidates to move to the store.

| Store | For | API | Survives restart |
| --- | --- | --- | --- |
| Board store | The board's data: messages, items, logs, anything a person would see again or search | [Board storage](#board-storage) and [Search](#search) | Yes, while the board's source is on in the Search view |
| Stored choices | Small UI choices: filters, layout, folded groups | `prefs::get`, `set`, `get_list`, `set_list`, `remove` (keys `<id>.<name>`, `state.json`) | Yes |
| Cache files | A small, non-searchable view snapshot to show at once on the next launch | `prefs::read_cache(name)`, `prefs::write_cache(name, text, cx)`; `subspace_core::state::sibling` and `write_atomic` for a board-owned file | Yes, owner-only files beside `state.json` |

### Board storage

- **What**: the board's own SQLite database with an FTS5 index
  (`<state dir>/search/<app>/<board>.sqlite`, folder `0700`, file `0600`),
  with its own retention, byte quota and one active scope. Write with
  `Service::upsert` and `index_metadata_batch`; read back with
  `Service::records` (facet filters, newest first, cursor pages, payloads) and
  `Service::groups` (each `parent_id` with its count); delete with
  `Service::remove_matching` (by facet) or `clear`. Retention and quota drop a
  board's oldest records only.
- **How**: set `SEARCHABLE = true`; put what restores an item in the
  document's `payload` and what opens it in `navigation`; use `parent_id` for
  the container (a conversation, a repository) and the `project` facet for a
  filter. Run every call on the background executor, in order. Read back on
  start (`start_background` or `on_open`), then ask the service only for what
  changed. `BoardSettings::store_by_default` turns the source on until the
  person chooses; the Search view's switch is then the board's one "keep this
  data on this computer" control, and **Clear cache…** deletes it.
- **Use it when**: the board loads or receives data the person would expect to
  see again after a restart or to find later.
- **Example**: `spaces/irc/src/store.rs` and `hub/persist.rs` (messages:
  restore, catch-up, forget).
- **Tests**: a round trip against a temporary `Service::open`, the store off
  writing nothing, and deletion; `spaces/irc/src/store/tests.rs`.

### Stored choices

- **What**: `prefs` keeps choices in memory and the app writes `state.json`
  shortly after a change and on quit. Without a state file, as in tests,
  choices last for the run.
- **Use it when**: any choice the person would expect to survive a restart.
  Never store credentials or large data here.
- **Example**: `spaces/irc/src/dock/persist.rs` (dock layout),
  `spaces/irc/src/hub.rs` (removed buffers).

### Cache files

- **What**: a whole text file beside the state file, written off the UI thread
  with an atomic, owner-only replace.
- **Use it when**: the board loads data that is slow or costly to fetch and
  that the person expects to see straight away. Show the cache first, then
  refresh.
- **Example**: `spaces/github/src/store.rs` (`github-cache.json`, every
  account's sections); `spaces/github-copilot-agents/src/logs.rs` (session
  logs). Both predate the [board store](#board-storage).
- **Tests**: a round trip of the cache format and a corrupt or missing file
  reading as empty.

### API-backed views

- Keep the last usable result and the request status in separate fields.
  Use a loading placeholder for a first read or a new scope. During a
  same-scope refresh, keep the result visible and use a small status
  indicator if feedback is needed.
- Run the request in the background and capture its account, organisation,
  record, filters and request generation. Apply the latest matching result
  in place, keeping selection and scroll position where they still apply.
- Keep the current result on transient errors and allow a retry. Clear
  data when the account or organisation changes or access is lost.
  Remove a record when the API confirms it is unavailable.
- Test a delayed refresh with data already visible and a response that
  arrives after the user switches scope.

### Automatic refresh and background updates

- **What**: `background_refresh` is a data-only refresh the shell calls on its
  schedule; it must not activate views, start runtimes or ask for credentials.
  A board that polls by itself reads `refresh::enabled(id, cx)` and
  `refresh::interval(id, default, cx)`, and `refresh::Schedule` for timing.
  `BoardSettings::background_updates = true` adds the per-board interval and
  Off controls; Off must also pause the board's monitoring and notifications.
- **Use it when**: the board reads a service that changes without the person
  acting. A live connection (IRC) does not need it.
- **Example**: `spaces/github/src/lib.rs` (`background_refresh`),
  `spaces/actions/src/poll.rs`.

### Blocking work

Client calls block. Run them with `cx.background_spawn`, never in `render`,
and start panel loads from `ListDashboard::on_open`. Agent tool refreshes use
`subspace_agent_tools::refresh::Refresher`, which coalesces concurrent
refreshes and refuses repeats within its cooldown.

## Search

The Framework has two kinds of search. Read [Full-text search](search.md) for
the person's view and the index API.

### Item search (`/` and the palette)

- **What**: `search` returns a `Search` with an `ItemSearch` of `(SearchDoc,
  Row)` pairs held in memory; `open_search_result(id)` reveals one. Emit
  `DashboardEvent::Opened(id)` when an item's details open, so the shell lists
  it under recent items.
- **Use it when**: the board has a list of named things the person may want to
  jump to: items, channels, conversations.
- **Example**: `spaces/github/src/lib.rs`, `spaces/actions/src/dashboard.rs`,
  `spaces/irc/src/rows.rs` (`search`: channels and conversations).

### Full-text index

- **What**: a persistent, local, BM25 index shared by the Search view
  (Cmd/Ctrl+Shift+F), chat and MCP (`search_content`, `read_search_result`,
  `related_items`). The host opens it only for boards with
  `SEARCHABLE = true`, under the product's state folder. The person enables
  caching per board in the Search view, unless the board declares
  `BoardSettings::store_by_default`; each board starts with 60 days of
  retention and a 2,000,000,000-byte budget. The index is the
  [board store](#board-storage): what a board persists, global search finds.
- **How**:
  1. Set `const SEARCHABLE: bool = true`.
  2. Get the service with `full_text_service(cx)`; do nothing when it is
     `None` or `service.policy(Self::ID)` is not enabled.
  3. Bind a verified scope on the background executor:
     `search_adapter::bind` or `schedule` (or `prefetch::Source::bind`) with an
     `Identity { endpoint, account, key }` that comes from the service, never
     from a display name. A board has one active scope at a time.
  4. Publish `Document`s (id, title, body, kind, parent id, navigation,
     `refreshed_at`, optional payload) in batches of at most 100 through
     `prefetch::Work::publish` or `Service::upsert`. Bound long text with
     `search_adapter::bounded_text`.
  5. Optionally call `index_metadata` with a project facet and exact
     `ReferenceIdentity` links (repository, branch, commit, issue or pull
     request, Copilot task), so `related_items` can join boards.
  6. Implement `open_indexed_result` (or `open_indexed_result_fenced`) to
     reveal a hit from its `navigation`, checking the scope and permit. Keep a
     retained result valid with `retained::watch_retained`.
- **Use it when**: the board holds text a person would search for later:
  bodies, comments, logs, messages.
- **Example**: `spaces/github/src/search_index.rs` (prefetch, scope, retained
  results); `spaces/github-copilot-agents/src/logs/load.rs` and
  `search_index.rs` (session logs); `spaces/irc/src/store/mapping.rs` and
  `reveal.rs` (one local scope, the network as `project`, GitHub links as
  references, opening a hit at its line).
- **Tests**: `spaces/github/src/search_index/tests/` covers binding, prefetch,
  cross-board references and retained results against a temporary
  `Service::open`.

`SearchNavigation` and `ChatRequest::OpenInSubspace` route local
`subspace-search://` citations to the board; boards do not open them as URLs.

## Agent tools, MCP and CLI

- **What**: `agent_tools(&ToolEnvironment)` returns the board's
  `Arc<dyn AgentTool>`s. One declaration serves Subspace chat (Copilot, Pi and
  ACP agents), the app's MCP server, `subspace mcp` and the generated CLI
  (`subspace <board> <action>`). The Framework adds `board_help`, the search
  tools and `web_read`.
- **How**: wrap a `ReadService` in a `ReadTool` (one action per call) and a
  `WriteService` in a `WriteTool` (1 to 20 operations, validated and
  preflighted, one approval, never retried). Write tools must use
  `ApprovalPolicy::PerCall`. Name them `<id with underscores>_read` and
  `_write`; names must be unique in the product. `ToolEnvironment` gives
  `config`, `credentials`, `search`, `config_file` and `host`. Build tools
  that act on live app state, such as connections, only for `ToolHost::App`.
  Mark service text as untrusted in the tool description and never return
  credentials.
- **Use it when**: the board reads or changes something an agent could help
  with. A board with nothing to offer advertises no tools and says so in
  [MCP server tools](mcp.md#tools).
- **Example**: `spaces/github/src/agent_tools.rs`; `spaces/irc/src/agent_tools.rs`
  (app-only tools over a bridge to the hub).
- **Tests**: tool tests with `subspace_agent_tools::test_support` (`environment`,
  `app_environment`, `invocation`); `the_documented_tool_inventory_matches_the_compiled_catalog`
  fails until [MCP server tools](mcp.md#tools) lists the tools and actions.
- **Reference**: [MCP server](mcp.md), [Command-line interface](cli.md),
  [Agent chat](agent-chat.md#tools).

## Chat context

- **What**: `agent_context` returns the `AgentContext` (`ContextRef`: key,
  label, content) for the visible selection. Auto context, **+ Current** and
  **Add to chat** (Cmd/Ctrl+Shift+A) all use it. The default sends only the
  board ID, row key and title. `context_text` builds labelled text from a
  `Detail` (`ContextText`, `add_detail`) and caps it at 16 KB.
  `set_selection` holds text selected in a pane for **Add selection to chat**.
- **How**: override `agent_context` to include what the detail panel shows,
  as data rather than instructions, without credentials.
- **Chat requests**: a board reads `chat_sessions::sessions(cx)` and asks the
  chat pane to act with `DashboardEvent::Chat(ChatRequest::…)`: list or open a
  saved session, replay a cloud task, open a citation, or refresh the visible
  board. It never owns the runtime.
- **Use it when**: always consider it for a board with a selection. "Add to
  chat" that attaches only a title is rarely useful.
- **Example**: `spaces/github/src/lib.rs` and `agent_context.rs`;
  `spaces/github-copilot-agents` for `ChatRequest`.

## Notifications and attention

- **What**: `notifications::request(Self::ID, target, Notice, cx)` posts an
  in-app toast and, by the person's choice, an OS notification with a sound.
  `Notice::described(text, body)` lets the OS banner name who and where; never
  put message content in it. The board keeps its own `target` numbers and
  resolves them in `open_notification`, which the shell calls when the
  notification is clicked. `BoardSettings::notifications = true` adds the
  shared controls; `notification_defaults` sets the board's defaults (Off
  unless declared). The person's saved choice wins.
- **Use it when**: something needs the person while they are elsewhere: a
  mention, a failed run, a review request. Raise notices from process-owned
  work (`start_background`) so they arrive while the board is hidden, and
  pair them with `badge`.
- **Example**: `spaces/irc/src/hub.rs` (`raise`), `spaces/irc/src/notice.rs`,
  `Dashboard::open_notification` in `spaces/irc/src/lib.rs`.
- **Tests**: `spaces/irc/src/tests/notifications.rs`.
- **Older API**: `alerts` (sound and macOS notification with `alerts.*` keys)
  predates this and bypasses the per-board controls and click-through. Do not
  use it in new boards.

## Help and docs

- **What**: `HELP_MARKDOWN = include_str!("../help.md")` is the board's guide.
  The shell shows it, and `board_help` returns it to agents. `HELP_IMAGES`
  bundles up to three PNG or WebP illustrations (512 KiB each), referenced as
  `subspace-help-image:<id>`. A developer page under `docs/` covers
  configuration and internals.
- **How**: follow the [board guide template](board-guide-template.md): ten
  fixed H2 headings, evidence for every claim. Link the guide and developer
  page from the app's documentation index.
- **Tests**: `scripts/check-board-docs.py` and `scripts/check-docs-index.py`
  (both in `make check`), `registered_multi_section_board_guides_have_useful_contents`.

## Links and navigation

- **What**: `links::open` and `links::safe_url` let only `http`, `https` and
  `mailto` leave the app; `links::copy` copies; `ServiceLink` puts a service
  link on the detail panel; `reveal` shows a local file. Search citations go
  through `SearchNavigation`, never the OS.
- **Cross-board references**: today the only shared reference model is the
  search index's `ReferenceIdentity` with `related_items`. There is no
  Framework call to open a GitHub item in the PRs board from another board.
  See [Communicating with other boards](#communicating-with-other-boards).
- **Use it when**: always use `links` for URLs from a service or a person.
- **Example**: `spaces/irc/src/markup/link.rs` (IRC's link policy).

## Communicating with other boards

Two kinds of traffic leave a board, and they work differently.

- **Board and Framework.** A board exposes agent tools so data can be written
  in, and the Framework gives it hooks to send data back out:
  `DashboardEvent`s (toasts, chat requests, `Opened`, `ConfigChanged`),
  notifications, the full-text index and help. Sending a selection to chat is
  this kind: chat is a Framework feature, not a board.
  [Framework paths](#framework-paths) lists them.
- **Board and board.** One board tells others that something happened, or
  asks another board a question, without knowing whether that board is in the
  product or enabled. This goes only through
  [contracts](#board-to-board-contracts) and the broker. Boards never depend
  on each other's crates (`scripts/check-architecture.py` refuses it), never
  read each other's globals or stored-choice keys, and hold no handle to each
  other.

### Framework paths

| Mechanism | Direction | Who sends | Who receives | How it is routed | Declared? |
| --- | --- | --- | --- | --- | --- |
| `DashboardEvent` | Board to shell | Any board, `cx.emit(…)` | The shell only | `SubspaceApp::dashboard_subscriptions` subscribes to every board and `on_dashboard_event` (`host/app/src/app.rs`) acts on it, tagged with the board ID | The enum is fixed in board-kit; which board emits which variant is implicit |
| Shell-called hooks | Shell to board | The shell | One board, by ID | `DashboardHandle` (`open_search_result`, `open_indexed_result_fenced`, `open_notification`, `reload`, `activate`) | Trait methods with empty defaults |
| Shared state in globals | Framework to many | Settings Save; `repositories::apply`; the chat panel | Any view or process-owned entity that calls `cx.observe_global` | GPUI wakes observers when the global is set | No |
| Search navigation | Shell, chat, rich text to the owning board | Search view, chat citations, `subspace-search://` links in `rich` text | The board named by the result's scope | `SearchNavigation` global, installed by `install_search_navigation` (`host/app/src/app/search.rs`), then `open_indexed_result_fenced` | Implicit: any board with indexed content |
| Search references | Stored data join | Boards that call `index_metadata` | The `related_items` agent tool | Exact `ReferenceIdentity` keys in the index (`services/search/src/service.rs`) | No; nothing is told when another board indexes |
| Notification click-through | Board, through the shell, back to itself | `notifications::request(source, target, …)` | The same board's `open_notification(target)` | `Route::Board(source, target)` in `host/app/src/notifications/runtime.rs` (at most 256 targets, 24 hours) | `BoardSettings::notifications` opts in to the controls |
| Agent tools | Agent to boards | A person, through chat or an MCP client | Any board's read and write tools | The model calls tools in turn; writes need per-call approval | Tools are declared; the workflow between them is not |
| Contracts | Board to board | A board that declares `publishes` or `requests` | Running boards that declare `subscribes` or `responds` | The broker in `subspace_dashboard::contracts` | Yes, at registration: [Board-to-board contracts](#board-to-board-contracts) |

The Issues board answers IRC through the `issues` contract. The NETT client
retains only a weak live-hub handle for `Hub::start` reuse; it exposes no
process-wide snapshot or resolution path to other boards.

Shared state is the Framework's observer pattern for latest values. Three
real examples:

- **Favourite repositories.** Starring a repository in the PRs board calls
  `repositories::apply` (`board-kit/dashboard/src/repositories.rs`),
  which replaces `Services::config`, and emits `DashboardEvent::ConfigChanged`
  so the shell rebuilds the agent tools (`spaces/github/src/favourites.rs`).
  The PRs board follows `Services` in `follow_config`, and the Actions board
  follows it in `follow_repos` (`spaces/actions/src/load.rs`), reloading its
  runs. Settings Save takes the same path.
- **IRC settings.** IRC's process-owned hub observes `Services` and applies
  saved settings without a restart (`follow_settings` in
  `spaces/irc/src/hub.rs`).
- **Chat sessions.** The host chat panel publishes `ChatSessions`
  (`board-kit/dashboard/src/chat_sessions.rs`) only when the list
  changes. Copilot Agents observes it (`spaces/github-copilot-agents/src/dashboard.rs`)
  and asks the panel to act with `DashboardEvent::Chat(ChatRequest::…)`.

Things that look like board-to-board communication but are not:

- IRC's `ChatInput::message` (`spaces/irc/src/compose.rs`) reuses the shared
  composer widget; IRC sends nothing to chat.
- The shell handles `ChatRequest::OpenInSubspace` and `RefreshVisibleBoard`
  (`host/app/src/app/chat.rs`), but no board emits them today.
- The web board bridge (`spaces/web/src/bridge/protocol.rs`: `info`,
  `settings.get`, `badge.set`, `context.set`, `notify`) connects a web page to
  its own board, not to other boards.
- IRC's `Process` and Space Lab's `ProjectLifecycle` globals are private to
  their boards.

`DashboardEvent::Navigate` asks the shell to open a link in the board that
answers the named contract request. The shell routes only from the visible
board to an active responder that accepts the URL through `Dashboard::open_link`.
Routing takes the first matching responder in registration order. Product
validation allows one responder per request.
IRC uses this to open a resolved issue. Its destination checks the link's
origin, profile and organisation against the current issue scope.

### Using the Framework paths

- **Ask the shell to act**: emit a `DashboardEvent`. Tests assert the
  emitted event: `spaces/github/src/favourites_tests.rs` (`ConfigChanged`),
  `spaces/github-copilot-agents/src/load/refresh_tests.rs`
  (`ChatRequest::ListSessions`).
- **Change shared configuration**: write it through `repositories::apply` or
  Settings, emit `ConfigChanged`, and let other boards follow
  `cx.observe_global::<Services>`. A follower compares the new value with
  what it applied before doing work, as `follow_repos` does.
- **Read host-published state**: read the snapshot (`chat_sessions::sessions`)
  and observe its global. A publisher sets the global only when the value
  changes, so observers are not woken for nothing
  (`publishing_identical_snapshots_does_not_wake_global_observers`).
- **Let other boards find your items**: record exact `ReferenceIdentity`
  links with `index_metadata` ([Full-text index](#full-text-index)). Tests:
  `spaces/github/src/search_index/tests/cross_board.rs`.
- **Open your item from elsewhere**: today only through an indexed result
  ([Search](#search)) or your own notification target.
- **Tell or ask another board**: use a contract, below.

### Board-to-board contracts

The `decisions@1/open` capability routes a decision reference to an answering
Space. The requesting Space can give the shell a return location, and the
answering Space reads the decision in its own account.

**Contracts.** A contract is an interface that no board owns. Contracts live
in `services/contracts` (`subspace-contracts`), which has no GPUI,
so boards, clients and hubs can all build their messages. A contract has a
stable lower-case name and a version (`Contract::NAME`, `Contract::VERSION`)
and two kinds of typed message, each with a key such as `issues@1/resolve`:

- an **event** (`Event`) announces that something happened, to every running
  board that subscribes, with no answer;
- a **request** (`Request`) asks the one running board that answers it, and
  returns its typed `Response` at once.

Messages carry identities (issue identifiers, `ReferenceIdentity`, framework
store document IDs) and short summaries, never credentials or bulk content. A
breaking change is a new version with its own types.

**Declaring.** A board declares its part in each message as a fixed fact,
read without a view:

```rust
use subspace_contracts::issues::{Changed, CurrentScope, Resolve};
use subspace_dashboard::contracts::{BoardContracts, Declaration};

fn contracts() -> BoardContracts {
    BoardContracts::new(&[
        Declaration::subscribes::<Changed>(),
        Declaration::requests::<CurrentScope>(),
        Declaration::requests::<Resolve>(),
    ])
}
```

The roles are `publishes` and `subscribes` for events, `requests` and
`responds` for requests. `BoardRegistration::new` captures the declarations
with the other static facts, and `BoardRegistration::contracts` lists them
for Settings, docs and tests. Configured boards (`BoardRegistration::configured`)
declare their type's `contracts()` under each configured ID, so a configured
type may publish, subscribe and request but never respond: two instances
would be two responders. Registration
rejects malformed names, a role declared twice and a request that more than
one included board answers.

**Attaching receivers.** At run time the board attaches the entity that
handles what it declared:

```rust
// In start_background, with the board's process-owned hub:
contracts::subscribe(Self::ID, &hub, |hub: &mut Hub, _: &Changed, cx| hub.refresh(cx), cx)?;
// On the answering board:
contracts::respond(Self::ID, &hub, |hub: &mut Hub, ask: &Resolve, _| hub.resolutions(ask), cx)?;
```

The broker holds each receiver and responder by weak handle
(`WeakEntity`). It never keeps a board alive, and it skips and forgets one
that has been dropped. Attach a process-owned entity from `start_background`
to receive while the view is closed, or the view in `new` to receive only
while it exists. The same entity subscribing again replaces its handler; a
later responder replaces the board's earlier one.

**Publishing and asking.**

```rust
contracts::publish(Self::ID, Changed, cx)?;
match contracts::request(Self::ID, &Resolve::new(profile, identifiers), cx) {
    Ok(resolutions) => { /* show summaries */ }
    Err(ContractError::Unavailable { .. }) => { /* no answering board: plain text */ }
    Err(error) => { /* undeclared: a bug, already logged */ }
}
```

**Routing.** The host installs the broker (`contracts::install`) with the
running boards' declarations, in product order, whenever the running set
changes. It does so after stopping removed boards' background work and before
starting new boards', so a stopping board can still announce that it stops
and a starting board can announce that it starts.

- Only running boards take part: those between `start_background` and
  `stop_background`. A disabled, stopped or absent board receives nothing and
  answers nothing, and that is not an error. Headless processes
  (`subspace mcp`, the CLI) install no broker and route nothing.
- Events run on the UI thread. Delivery waits until the publisher's update
  ends (`App::defer`), so a handler never runs inside the publisher. Each
  publisher's events arrive in order, to subscribers in product board order
  and then in attach order. A board does not receive its own events.
  Delivery is at most once, in process, with no persistence or replay: a
  subscriber that needs current state asks for it when it starts, and uses
  events for changes. An event published from a chain of handlers deeper
  than eight is refused (`ContractError::TooDeep`) and logged, so a cycle
  stops.
- Requests are synchronous and go to the single running board that declares
  `responds`. The answer is `ContractError::Unavailable` when there is none,
  when its responder entity is gone, or when that board is already answering
  further up the same chain. A responder answers from memory and never
  blocks; it starts slow work itself and announces the result with an event,
  as the `issues` contract's `Pending` and `Changed` do.
- Publishing, asking, subscribing or responding without the matching
  declaration is refused with `ContractError::Undeclared` and logged under
  `contracts`, with board ID and message key only, never payloads.

**Relation to the rest.** Globals stay the way the Framework shares a latest
value (`Services`, `ChatSessions`). Navigation (search results, notification
targets and `BoardNavigation`) shows something; contracts never
switch boards or take focus. The framework store (search service) holds each
board's data and joins it by reference; contracts store nothing and carry the
same identities, so a reference means the same thing everywhere. Agents do
not use contracts: a write tool goes through the board's own service, which
publishes the same event the interface would. Web boards and MCP clients do
not take part yet.

**Defining a contract.** Add a module to `services/contracts/src/`:
a type implementing `Contract`, one type per message implementing `Event` or
`Request`, `#[non_exhaustive]` records with constructors or builders, and the
responder's obligations in the module documentation. Types that carry content,
such as titles or names, do not derive `Debug`, so nothing logs them by
accident. Add a row to the app's contract list ([contracts and declarations](#contracts-and-declarations)).

**Testing.**

- Declarations: `BoardRegistration::new::<D>().contracts` needs no window.
  A product test can compare the app's documented list of roles with the
  registrations ([contracts and declarations](#contracts-and-declarations)).
- Publishing: `subspace_dashboard::test_support::contracts::Recorder::<E>::install(&[(D::ID, D::contracts())], cx)`
  routes the board and a recorder that subscribes to `E`;
  `recorder.events(cx)` returns what was delivered after the update.
- Receiving and answering: `contracts::install` the board with a fixture
  board that publishes or requests, attach the board's entity, then publish or
  request from the fixture's ID.
- The mechanism: `board-kit/dashboard/src/contracts/tests.rs` and
  its `events` and `requests` modules;
  routing with background work and no view:
  `host/app/src/registry/background/contracts_tests.rs`.

### Contracts and declarations

A board declares its contract roles in `Dashboard::contracts`. An app lists
the roles its boards declare as `| board | role | message |` rows, and a
product test compares that list with the registrations.

## Logging and diagnostics

- **What**: `subspace_core::log::Logger::new(target)` writes records to the app
  log, which redacts and bounds them. Use the board ID as the target and
  `<id>.<area>` for parts, such as `irc` for tool writes. Every client request
  through `subspace_core::http::agent()` is logged already.
  `diagnostic_config_path` offers the board's config path, never its contents,
  for an issue report.
- **Use it when**: writes, state changes and failures someone will need to
  diagnose. Never log message text, tokens or channel keys.
- **Example**: `spaces/irc/src/agent_tools/write.rs`.
- **Reference**: [Logging](logging.md), [Report an issue](report-issue.md).

## Testing and evidence

- Keep pure logic (grouping, selection, filters, caches, reducers) out of
  render code and test it; test observable interaction and critical layout,
  not incidental rendering. Every test asserts something.
- Use the shared fixtures: `subspace_dashboard::test_support` (feature
  `test-support`) for toolbars, `subspace_agent_tools::test_support` for tools,
  `clients/test-support` for offline HTTP, and `scripts/irc-test-server.sh` for
  a local IRC server.
- A change that alters what the app shows needs screenshots from the app's
  capture build, as [UI change evidence](../AGENTS.md#ui-change-evidence)
  describes.
- `make check` and `mise run quality` must pass in each project the change
  touches; see [Checks](../AGENTS.md#commands).
