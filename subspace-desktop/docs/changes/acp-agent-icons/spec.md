# Spec: ACP agent marks and names

## Mapping

One function, `engine_icon` in `framework/host/chat/src/agent_chat/engine.rs`,
maps the engine and the ACP agent to a glyph. `ChatPanel::engine_icon` exposes
it, and every surface that shows the engine uses it.

| Engine | Name | Mark |
| --- | --- | --- |
| Copilot | Copilot | Copilot (Octicons) |
| Pi | Pi, or Pi (Experimental) | Lucide Bot |
| ACP, `opencode` | OpenCode | OpenCode |
| ACP, `claude` | Claude | Claude |
| ACP, `codex` | Codex | OpenAI |
| ACP, `gemini` | Gemini | Gemini |
| ACP, `custom` | ACP agent | Lucide Plug |

The match is exhaustive over both enums, so a new engine or agent cannot
compile without a mark.

## Names

`AcpAgent::engine_label` in `subspace-core` returns the short names above. The
chat uses them everywhere it names the engine: the header, the composer, the
status line ("Claude is thinking…"), the waiting message and the status bar.
The "(ACP)" suffix is gone. **Settings > Chat** still lists the engine as
**ACP agent (Experimental)** and the agents by their full product names,
Claude Code and Gemini CLI, through `AcpAgent::label`.

## Placement

- **Toolbar Chat button:** the mark replaces the Plug glyph at the toolbar's
  glyph size. The accessible label stays **Chat**, and the tooltip keeps the
  activity indicator.
- **Chat header:** the mark before the engine's name, in muted text.
- **Status bar:** the mark before the state, such as "Codex ready". The mark
  is 0.75 rem with a 0.25 rem gap, the same as the compact status commands'
  glyphs, and takes the state's colour: muted, yellow when the chat needs the
  user, red when it is unavailable. The text alone carries the state, so the
  mark is decorative.

## Icons

The marks live in `framework/host/chat/assets/` and are embedded with
`include_bytes!` as `SvgIcon`s, like the Copilot mark already was. The copy of
the Copilot mark in `framework/host/app/assets/logos/` is removed; the toolbar
now uses the chat crate's mapping.

Each file is a single path with `fill="currentColor"` and the upstream viewBox,
so the theme's text colour tints it. `assets/PROVENANCE.md` records sources,
commits, hashes and licences:

- OpenAI: Lobe Icons (MIT), because Simple Icons no longer carries OpenAI.
- Claude, OpenCode, Google Gemini: Simple Icons (CC0-1.0).
- Copilot: Primer Octicons (MIT), unchanged.

The marks are trademarks of their owners and identify the products only.
