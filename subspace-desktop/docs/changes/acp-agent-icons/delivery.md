# Delivery: ACP agent marks and names

These images come from isolated debug capture builds in Cupertino Dark, the
default theme. Each uses a temporary `HOME`, no accounts, a 1280 × 800 window,
the default boards and a fixture `config.json` with
`"chat": {"engine": "acp", "acp": {"agent": "<agent>"}}` and
`"features": {"app.acp-chat": true}`. The crops show the right end of the
title bar and the status bar. In each pair, before is on top.

The chat panel stays closed, so no agent process starts. The runtime starts
only when the panel first opens, so "ready" is the state the status bar shows
for every engine at launch, before and after this change.

![Codex status bar, before and after: "Codex (ACP) ready" becomes the OpenAI mark and "Codex ready"](evidence/codex-status-before-after.png)

![Codex toolbar, before and after: the Chat button's Plug glyph becomes the OpenAI mark](evidence/codex-toolbar-before-after.png)

![Claude status bar after: the Claude mark and "Claude ready"](evidence/after-claude-status.png)

![Claude toolbar after: the Chat button shows the Claude mark](evidence/after-claude-toolbar.png)

![OpenCode status bar after: the OpenCode mark and "OpenCode ready"](evidence/after-opencode-status.png)

![OpenCode toolbar after: the Chat button shows the OpenCode mark](evidence/after-opencode-toolbar.png)

## Results

- Headless tests: `every_engine_and_agent_is_shown_with_its_own_mark`,
  `every_mark_is_a_tintable_svg_that_renders_at_glyph_size`,
  `acp_agents_are_reported_by_their_product_names` and
  `acp_codex_is_shown_by_name_with_the_openai_mark` pass.
- Unit tests pass for `subspace-chat` (178), `subspace-shell` (443) and
  `subspace-core` (260). Clippy with warnings denied, `cargo fmt --check` and
  `cargo shear` are clean for these packages. The architecture, board-docs
  and script checks pass.
- `make check` did not pass as a whole while this change was made. The
  in-progress IRC board work in the same tree had format differences and a
  Clippy error in `spaces/irc`. Concurrent builds also removed rlibs during
  the doctests. Neither involves this change.
- `mise run quality` reports no findings in the files this change touches. The
  three new findings were `SRP_MODULE` in `spaces/irc`.
