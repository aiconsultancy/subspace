# Channels agent status lines: checklist

Back to the [change record](intent.md).

- [x] Read the protocol record, the Space's reducer, transcript and member
      list, the design guide and the component rules.
- [x] Expose the client's status cleaner as `subspace_client_irc::status_text`.
- [x] Hold statuses per network, cleaned again, never stored.
- [x] Clear on the runtime's clear, on any connection state but connected,
      and on the last held channel's departure; keep them through the nick's
      own messages.
- [x] Compose the line above the composer in a pure function; show it with
      an accessible name.
- [x] Show the status in the member row and its accessible name.
- [x] Write state, pure and render tests.
- [x] Write the capture fixture and its commands.
- [x] Update the Channels guide, `docs/irc.md` and the protocol record.
- [x] `mise run quality` on the source (exit 0, no new findings).
- [x] Build, the crates' tests and `make check` (exit 0).
- [x] Run the fixture against a local Ergo server and capture the evidence
      in Twilight Two at 1440 x 900, before and after, plus the narrowest
      window (1280 x 800).
- [ ] Owner: commit (signed), push and open the pull request.
- [ ] Embed the captures in the pull request with commit-pinned URLs.
- [ ] Both model reviews clear the final head. Not run.

### Board checklist: irc

Identity and registration
- [x] ID, title, description and icon: not applicable; unchanged.
- [x] Product descriptor, registration and artwork: not applicable; no
      Space added.
- [x] `RETAIN_WHEN_DISABLED`: not applicable; process ownership unchanged.

Layout and UI
- [x] Workspace layout: unchanged; the status shows in the content (the
      line above the composer) and the inspector (the member list), the
      zones that already hold typing and members.
- [x] One text edge and reading width: unchanged; the line keeps the typing
      line's inset.
- [x] Navigator and inspector choices: unchanged.
- [x] Follows the design guide; screenshots compared with the shell anatomy
      image and the review checklist (see the delivery notes' Evidence).
- [x] List hooks or custom layout: the existing conversation dock.
- [x] Components: GPUI Kit layout, the shared `row_button` and
      `truncated::styled`, theme tokens only; no new shared component, so no
      new gallery card.
- [x] Toolbar, status, badge and busy: not applicable; unchanged.
- [x] Palette commands: not applicable; no action added.
- [x] Keys and help foot: not applicable; no key or focus change.
- [x] Menus and dialogs: not applicable; none added.
- [x] Empty, loading and error states: no status shows nothing; the line
      keeps its height.
- [x] Quit warnings: not applicable.

Settings and configuration
- [x] Settings and notification opt-ins: not applicable; none added.
- [x] Service connections or config: not applicable; existing networks.
- [x] Feature flags: not applicable.

Data caching and refresh
- [x] Stored choices: not applicable; none added.
- [x] Framework store: not applicable; statuses are deliberately never
      stored.
- [x] Store-first loading: not applicable.
- [x] Automatic refresh and UI blocking: in-memory state from runtime
      events; no I/O in render.

Search
- [x] Item search, full-text index, cross-board references: not
      applicable; statuses are not searchable.

Agent tools, MCP and CLI
- [x] Read and write tools: not applicable; tools unchanged and do not
      report statuses.
- [x] App-only tools, tools docs: not applicable.

Chat context
- [x] `agent_context`: not applicable; unchanged.

Notifications
- [x] Not applicable; a status never notifies.

Help
- [x] `help.md` updated (the member list and the line above the input).
- [x] Guide and developer page links: unchanged.

Contracts and globals
- [x] Not applicable; no contract or global changes.

Links and logging
- [x] Not applicable; nothing opened or logged.

Tests and evidence
- [x] Pure logic tested; render tests for the line and the member row
      pass.
- [x] Screenshots from the capture build with fixture data: before, after
      and the narrowest window, in Twilight Two.
