# Tasks: IRC conversation polish

- [x] Reproduce Shift+Return in the real IRC composer with a headless test:
      it added the line break and sent the draft at once.
- [x] Send only on a plain Enter in the shared `ChatInput`; keep Up and Down
      for cursor movement in a multi-line draft; name Return and
      Shift+Return in the message input's hint.
- [x] Reproduce the strip beside the member list with a layout test, and let
      the list fill its dock at any saved width.
- [x] Tint the transcript row under the pointer; keep a mention's fill.
- [x] Add the saved removals (`irc.removed`), the event filter that keeps
      removed items out and brings them back, and `Hub::remove`.
- [x] Add **Leave channel** and **Close conversation** to the row menus and
      use the same items in the toolbar's **More** menu.
- [x] Keep left channels out of the runtime's configured autojoin at startup
      and across reconnects, without restarting a connection.
- [x] Add the `right-click:` and `move:` capture steps and the
      `IRC_TEST_LISTEN` port override.
- [x] Update `docs/irc.md`, `spaces/irc/help.md`, `docs/ui-components.md` and
      `AGENTS.md`.
- [x] Capture before and after evidence in Cupertino Dark with a fictional
      crew on a local Ergo server.
