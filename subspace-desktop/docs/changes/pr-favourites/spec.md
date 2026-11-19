# Spec: favourite repositories and All open

## Sections

- **All open** is the fourth pull request section, after **Involved**, key
  `4`. It comes last so the personal sections keep their places and keys, and
  the tabs read from the narrowest scope to the broadest.
- It searches `is:pr is:open archived:false` with one `repo:` qualifier per
  favourite. Without favourites it is empty, sends nothing, and its empty
  state (**No favourite repositories**) offers **Add repositories…**
  (`ListDashboard::empty_actions`), which opens the Repositories search.
- **Favourites only** adds the same `repo:` qualifiers to Mine, Review
  requested and Involved. It is off by default and saved as the preference
  `github.favourites-only`. With it on and no favourites, those sections show
  the same empty state.
- Counts, status line (`<n> open PRs · <n> repos`), saved results, background
  updates and reload work as for every section. Issues are not shown by this
  board, so the filter and All open apply to pull requests only.

## Query strategy (`subspace_client_github::scope`)

- `Client::section(login, id, repos)`: `None` searches everything the
  account can see; `Some(list)` limits to those repositories.
  `SectionDef::repos_only` marks All open.
- Each name must pass `check_repo`, so no name can add a qualifier, operator
  or quote. Names repeat once, without regard to case.
- Qualifiers are packed in order into searches of at most 256 characters.
  The searches run in parallel; counts are added and items merged by key,
  newest first, 50 kept, as every section does.
- `Client::repositories(login, text)`: `GET /search/repositories?q=<text>`,
  or with no text `GET /user/repos?sort=pushed&affiliation=owner,collaborator,organization_member`,
  30 at most. Serde names follow GitHub's JSON (`full_name`).

## The Repositories control (board)

- A toolbar control after the tabs: star glyph, **Repositories**, and a
  pressed look while Favourites only is on. `Shift+F` and the palette's
  **Repositories…** open it too; in overflow it is **Repositories…**.
- It is the shared `RepositoryPicker` (`subspace_dashboard::repositories`):
  a kept-open, owner-filtered `menu` with **Favourites only** (checked when
  on), **Favourites** (filled stars), then **Your repositories** (empty
  query) or **GitHub results**, outlined stars. Typing searches after a
  250 ms pause, in the background; **Searching…**, **No repositories found**
  and **Search failed: …** show under the options.
- A pick stars or unstars at once and the search stays open. Escape, Tab or
  a click outside closes it.
- Repository headers of favourites show a filled star. The context menu of a
  header or a pull request offers **Add `owner/name` to favourites** or **Remove
  `owner/name` from favourites**; `f` does the same for the selection; the palette
  lists it too.

## Saving and live propagation

- **Board (immediate):** `repositories::write` saves `repos` through the
  private config-file editor (`SettingsFile::update_repos`: only `repos`
  changes, unknown keys stay, the file stays owner-only, a file changed
  since it was read is refused as stale). Writes are serialised; a star made
  during a write is saved next. Then `repositories::apply` updates
  `Services::config`, and the board emits `DashboardEvent::ConfigChanged`.
- **Shell:** on `ConfigChanged` it rebuilds the board tool catalog for chat
  and the MCP catalog from the running config (the Settings board-choice
  path, `apply_board_selection`). MCP's target now includes `repos`, so an
  open listener swaps its catalog.
- **Boards:** PRs and Actions observe `Services`; a changed list reloads All
  open (and the filtered sections) and Actions. Copilot Agents and the
  agent tools take the list from the rebuilt catalog.
- **Settings (draft until Save):** **Favourite repositories** lists the
  draft with Remove; **Add repository…** opens the same search as the
  Settings account. Save writes the list with the other values and applies
  it like the board path, without a restart.
- A failed write keeps the saved list, reverts the board's stars and shows
  **Favourites not saved: …**.
