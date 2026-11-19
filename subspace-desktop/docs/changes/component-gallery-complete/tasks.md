# Tasks: a complete component gallery and clear component rules

Delivered in three separable parts.

## Part 1: rules and enforcement

- [x] Canonical Rules section in `docs/ui-components.md`, with what
      `make check` enforces; Ownership rule trimmed to where code lives.
- [x] AGENTS.md: the two scattered component bullets replaced by one
      Components section that summarises and links the rules.
- [x] Pull request checklist in `docs/ways-of-working.md` and the
      repository's `.github/pull_request_template.md`; a board checklist item.
- [x] "Catalogue entries without a card" table; `chat_bubble` catalogued;
      the `IconButton` entry named `icon_button::IconButton`.
- [x] Tests: public dashboard modules catalogued; catalogue entries have
      cards or reasons; names table equals the Subspace and shell cards;
      gpui-component modules have GPUI Kit cards; used marks match the source.

## Part 2: gallery and Subspace coverage

- [x] Catalogue model: libraries, `Kit` facts, `Values` for controlled
      samples.
- [x] View: GPUI Kit Sidebar navigation by library, one section at a time,
      filter, copyable names (GPUI Kit `Clipboard`), used marks, "In
      Subspace, use" links.
- [x] Toolbar **Hide unused** (latched, stored) and the used count status.
- [x] `/` item search that reveals a card.
- [x] New cards: messaging (7), navigation and chrome (5), inputs (8), menus
      (3), `list::render`; 15 shell surface cards.
- [x] Tests: every section renders every card; copy puts the exact name on
      the clipboard; hiding removes exactly the unused cards; search reveals
      a hidden card; the filter matches across sections; builders caption
      every state; shell links resolve.

## Part 3: GPUI Kit components

- [x] 84 cards in basic (37), form (13), layout (17) and advanced (17).
- [x] Interactive samples with fixture data; popups through `anchored`,
      switches through `toggle::switch`.
- [x] Provenance note for samples that follow GPUI Kit's stories.

## Board checklist: component-gallery

Identity and registration
- [x] ID, title, icon unchanged; description updated to say it covers shell
      surfaces and GPUI Kit.
- [x] Product descriptor and registration unchanged.
- [x] `RETAIN_WHEN_DISABLED`: not set; the board owns no processes.

Layout and UI
- [x] Custom layout: a GPUI Kit Sidebar and a page of cards, not a list board.
- [x] Components follow the rules: GPUI Kit `Sidebar`, `Clipboard` and `Tag`
      as shipped; shared `fields::input`, `action` buttons and
      `IconButton::latched`.
- [x] Toolbar: **Hide unused**; status: the used count; no badge or busy.
- [x] Palette commands: not applicable because the one toolbar control is a
      view toggle with its own stored state; `/` search covers navigation.
- [x] Keys: none of its own; `keys` is empty.
- [x] Menus and dialogs: sample popups close on `dismiss` and `deactivate`.
- [x] Empty states: a Kit section with every card hidden says so; the filter
      says when nothing matches.
- [x] `quit_warnings`: not applicable because the board starts no work.

Settings and configuration
- [x] `settings()` description updated; no notifications or background work.
- [x] Service connections: not applicable because it reads no services.
- [x] Feature flags: not applicable; dev builds enable it.

Data caching and refresh
- [x] Stored choices in `prefs`: the section and **Hide unused**.
- [x] Framework store: not applicable because it has fixture data only.
- [x] Refresh: not applicable; reload only redraws.

Search
- [x] Item search (`/`) over every card.
- [x] Full-text index: not applicable because the cards are not content.
- [x] Cross-board references: not applicable.

Agent tools, MCP and CLI
- [x] No tools because the gallery is a developer view with fixture data.

Chat context
- [x] `agent_context`: the default; no row subject.

Notifications and attention
- [x] Not applicable; the notification samples are in-app only.

Help and docs
- [x] `help.md` rewritten to the template's headings.
- [x] Already linked from docs/README.md and docs/subspace/boards.md.

Communicating with other boards
- [x] Not applicable because the gallery publishes and receives no contracts.
- [x] No board's crates, globals or stored choices are used.

Links, logging and diagnostics
- [x] Links open through `links` (the fixture `example.com` URL).
- [x] No writes or failures to log.

Testing and evidence
- [x] Pure logic (filtering, counts, usage scan) and interaction tested.
- [x] Screenshots from the capture build with fixture data.
