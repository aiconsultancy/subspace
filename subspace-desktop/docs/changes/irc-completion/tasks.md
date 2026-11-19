# Tasks: IRC completion

- [x] Move the completion list's rendering and highlight from the agent
      chat's `SlashList` to `subspace_dashboard::chat_input::completion`, and
      keep the agent chat's behaviour.
- [x] Add `ChatInput::cursor`, `ChatInput::replace_range`, and an Enter
      binding in the completion context.
- [x] Add `COMMANDS` (`CommandHelp`, `FirstArgument`) beside the IRC parser,
      and take its usage errors from the same table.
- [x] Add the IRC provider (`compose/suggest.rs`) and the list wiring
      (`compose/offer.rs`): `@`, `/`, first arguments and `#`.
- [x] Make classic Tab completion edit the word before the cursor in place.
- [x] Add pure and interaction tests.
- [x] Add the `type:` and `key:` capture steps.
- [x] Update `docs/irc.md`, `spaces/irc/help.md`, `docs/ui-components.md`
      and `AGENTS.md`.
- [x] Capture Cupertino Dark evidence with fictional Ergo data.
