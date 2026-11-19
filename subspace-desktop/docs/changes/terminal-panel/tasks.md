# Terminal panel: tasks

Back to the [spec](spec.md).

- [x] Board-kit: the product-neutral dock panel contract; remove the
      per-board dock and maximize hooks.
- [x] `framework/host/terminal`: move the Space's tabs, dialogs, config,
      font editor, help and quit warnings; implement the contract; delete
      `spaces/terminal`.
- [x] Shell: panel factory in `ProductBuild`; open state, placement, bottom
      span, maximize; compositions; ToggleTerminal (Ctrl+`),
      ToggleTerminalMaximized; toolbar button, menu item, palette commands;
      quit warnings; focus.
- [x] Panel header buttons with the text toggle (default off).
- [x] Settings > Layout: placement, bottom width, button labels; terminal
      font page.
- [x] Product: factory under the `terminal` feature; debug-only defaults in
      `make run`, `make install-macos`, `make capture`.
- [x] Architecture checks, mise tasks, audit paths, docs (`docs/terminal.md`,
      `docs/settings.md`, `docs/board-integration.md`,
      `docs/ui-components.md`, AGENTS.md structure notes).
- [x] Tests: compositions per placement and span, maximize, toggle and focus,
      persistence, label toggle; `make check` and `make check-terminal`.
- [x] Captures: each placement, both bottom spans, maximized, labels on.
