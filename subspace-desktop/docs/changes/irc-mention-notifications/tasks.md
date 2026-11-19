# Tasks: IRC mention notifications

- [x] Tests first for the decision: a tag and no tag, the person's own line,
      a private message, history with a tag, a line the read marker covers,
      and a **Server** buffer notice that names the person.
- [x] Tests first for the hub and board: the conversation in view with the
      app in front, the app in the background, another conversation in
      view, and choosing the notification.
- [x] Tests first for the shell: described and generic OS text, board
      defaults until saved, and an OS response routed to the board.
- [x] Notify only for unread lines; carry the sender in `Attention`.
- [x] `Notice` with optional OS body; the shell posts it.
- [x] `BoardSettings::notification_defaults`; IRC declares OS with Glass.
- [x] Activate the app on an OS notification click.
- [x] Help guide, `docs/irc.md` and `docs/ui-components.md`.
- [x] Linux capture with fixture data.
- [x] `make check` and `mise run quality`.
- [ ] macOS: jt checks the notification, its sound and the click from the
      next build.
