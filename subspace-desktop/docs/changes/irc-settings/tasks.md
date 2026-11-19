# Tasks: IRC settings

- [x] Move the saved-secret row from the shell to `subspace_dashboard::secret`
      with token and password wording; the shell delegates to it.
- [x] Share the Settings form treatment in `fields`: `form_field`, `heading`,
      `note`, `field_error`, and `path_input` with the native file picker.
- [x] Add `reveal` (platform-labelled Show in Finder) and
      `copy_details::copy_text`; add `feedback::empty_with_actions`.
- [x] Settings > General: reveal button beside the configuration file.
- [x] IRC schema source of truth and agent prompt, with drift tests.
- [x] IRC settings editor: list, form, validation, ids, secrets, Done and
      inline Remove confirmation, highlight words, agent setup actions.
- [x] Apply saved settings without a restart: hub observes `Services`,
      `IrcState::reconfigure`; board highlight words no longer go to the
      runtime.
- [x] Redesign the empty board.
- [x] Tests: draft round trip with unknown keys and secrets, secret edits,
      validation, ids, editor interaction, secrets never in inputs, removal,
      empty-state actions and copied prompt, hot reconfigure, General reveal.
- [x] Docs: help guide, `docs/irc.md`, `docs/settings.md`,
      `docs/ui-components.md`.
- [x] Evidence screenshots in One Dark, Cupertino Light and Cupertino Dark.
