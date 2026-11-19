# Tasks: ACP agent marks and names

- [x] Add the OpenAI, Claude, OpenCode and Gemini marks with their provenance
  and licences.
- [x] Map every engine and ACP agent to one mark in the chat crate.
- [x] Use the mapping on the toolbar's Chat button and in the chat header.
- [x] Show the mark before the chat state in the status bar.
- [x] Shorten ACP engine names to the product names: OpenCode, Claude, Codex,
  Gemini.
- [x] Remove the shell's duplicate Copilot mark.
- [x] Tests: the mapping covers every engine and agent; every mark is a
  tintable SVG that renders at 16 px; status text for each agent; a shell
  test that ACP Codex shows "Codex" with the OpenAI mark, and that the status
  bar mark matches the status glyph size.
- [x] Update `docs/agent-chat.md`, `docs/acp-chat.md` and
  `docs/ui-components.md`.
- [x] Cupertino Dark screenshots: Codex before and after, Claude and OpenCode
  after.
