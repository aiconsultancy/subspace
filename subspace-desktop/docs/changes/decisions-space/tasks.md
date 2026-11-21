# Decisions Space checklist

Back to the [change record](intent.md).

- [x] Add the `spaces/decisions` crate (`subspace-decisions-board`) with its
      artwork, guide and tests.
- [x] Register it in `products/subspace` and `products/descriptor`, on in
      debug builds and off in release builds, as Issues is.
- [x] Record the declared contract role and the Space in the
      [Spaces audit](../../spaces-audit.md), and add the
      [developer page](../../decisions.md) to the documentation index.
- [x] Build and test (`cargo test --workspace --locked --no-fail-fast`).
- [x] `make check` and `mise run quality`, on the final tree
      (21 November 2026): both passed.
- [ ] `mise run audit`. Run on 21 November 2026 and fails on nine advisories
      against `heapless` 0.9.3 (RUSTSEC-2026-0574 to RUSTSEC-2026-0582, all
      "unsound"); `main` depends on the same version and this change adds no
      third-party crate, so `main` fails it too.
- [x] Add `"decisions"` to the expected boards in
      `scripts/test-product-interfaces.sh`.
- [x] Run `make interfaces` (21 November 2026, exit 0).
- [x] Link the guide from the board table in the README.
- [ ] Link the guide from `docs/subspace/boards.md`.
- [x] Capture the list with the peek, in Twilight Two
      ([delivery](delivery.md#capture)).
- [ ] Capture the hidden navigator and a page with its banner, and embed
      the captures in the pull request with commit-pinned URLs.
- [ ] Both model reviews clear the final head. Not run.

### Board checklist: decisions

Identity and registration
- [x] ID `decisions`, title **Decisions**, description "Subspace API
      decisions", Lucide `scale` icon under the Space's own key
      (`icons/decisions.svg`).
- [x] Product descriptor (`board("decisions", cfg!(debug_assertions))`) and
      `BoardRegistration` added; artwork declared with `ARTWORK`, order 14,
      after the gallery's.
- [x] `RETAIN_WHEN_DISABLED`: not set; the Space owns no processes.

Layout and UI
- [x] Workspace layout: views, makers and projects in the navigator; the
      view and an open decision in the tab strip; search, Tag and the count
      in the toolbar; the peek or the page's facts in the inspector; no Space
      heading or context line.
- [x] One text edge from `workspace::text_edge` (also the list's
      `list_side_inset`); the page is no wider than
      `workspace::READING_WIDTH_PX`.
- [x] Navigator and inspector switches stored as `decisions.navigator` and
      `decisions.inspector`; a view menu leads the toolbar while the
      navigator is hidden.
- [x] List board hooks (`ListDashboard`): grouped rows, lanes, group glyphs,
      accessible row names, row menu, empty words, click to select and
      double click or Enter to open.
- [x] Components: GPUI Kit and shared Framework components only (workspace,
      list, tabs, fields, menus, action buttons, feedback, rich text, links,
      toolbar label). No new shared component.
- [x] Status (service, account, count, read time) and busy; no badge.
- [x] Palette commands for every navigator row, the Tag menu, the search,
      Open or Close, the linked issues and the dock switches.
- [x] Own keys bound in `view_keys(ID)` (`1`–`5`, `/`); `keys` matches.
- [x] Menus reported with `modal` and closed by `dismiss`.
- [x] Loading (skeleton), empty (search or choices) and error states with
      Retry; no Configure, as the Space has no settings of its own.
- [x] `quit_warnings`: not applicable because the Space starts nothing that
      quitting ends.

Settings and configuration
- [x] `settings()` description; notifications and background updates off.
- [x] Service connections: not applicable because the Space uses the shared
      account under Settings › Account.
- [x] Feature flags: none.

Data caching and refresh
- [x] Stored choices in `prefs`: view, maker, project and both docks.
- [x] Framework store: not applicable because decision content stays in
      memory, as issue content does.
- [x] Store-first display: not applicable for the same reason.
- [x] Reads run in `cx.background_spawn`; showing the Space reads a list
      over a minute old; no scheduled refresh.

Search
- [x] Item search (`/`) and `Opened`: not applicable because `/` focuses the
      Space's own server-side search, as in Issues, and decision content is
      kept out of the shell's recent items.
- [x] Full-text index: not applicable, under the same privacy boundary as
      Issues.
- [x] Cross-board references with `index_metadata`: not applicable.

Agent tools, MCP and CLI
- [x] No tools, because decision content stays out of agent context, as
      issue content does.
- [x] App-only tools: not applicable.
- [x] `docs/mcp.md`: nothing to list.

Chat context
- [x] `agent_context` is empty and row and text menus leave chat out.

Notifications and attention
- [x] Not applicable: the Space raises no notices.
- [x] Not applicable: no process-owned work.

Help and docs
- [x] `help.md` follows the board guide template; no images.
- [ ] Guide linked from `docs/subspace/boards.md`: outside this change's
      allowed paths. The developer page is linked from `docs/README.md`.

Communicating with other boards
- [x] Requests `issues@1/current-scope` to open linked issues; listed under
      Declared contracts.
- [x] No dependency on another Space's crate, globals or stored choices.

Links, logging and diagnostics
- [x] Evidence links go through `links::external_marked` and the shared
      safe-link policy; Markdown uses the shared rich text.
- [x] Nothing logged with content; no config file, so no diagnostic path.

Testing and evidence
- [x] Pure logic tested; interaction and layout invariants tested headless.
- [ ] Screenshots from the capture build with fixture data: the list with
      the peek is taken; the hidden navigator and a page with its banner are
      not.
