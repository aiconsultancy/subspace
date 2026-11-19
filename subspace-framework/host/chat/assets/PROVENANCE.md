# Chat engine mark provenance

Each chat engine is shown with its product's own mark. The marks are
trademarks of their owners. Subspace uses them only to identify the product
that runs the chat, not to suggest endorsement.

Every file is a single-colour SVG filled with `currentColor`, so the theme's
text colour tints it like any other glyph. Except for Copilot, which is copied
unchanged, each copy keeps the upstream path data and viewBox, removes the
`role`, `<title>`, inline `style` and `1em` size attributes, and adds
`width="16" height="16"`, `fill="currentColor"` and a one-line source comment.

| File | Engine | Source | Licence |
| --- | --- | --- | --- |
| `copilot.svg` | Copilot | [primer/octicons `icons/copilot-16.svg`](https://github.com/primer/octicons/blob/main/icons/copilot-16.svg) | MIT, [`LICENSE-OCTICONS`](LICENSE-OCTICONS) |
| `openai.svg` | Codex | [lobehub/lobe-icons `packages/static-svg/icons/openai.svg`](https://github.com/lobehub/lobe-icons/blob/55746b0b082258211121c70bd26e6878fd3bd292/packages/static-svg/icons/openai.svg) | MIT, [`LICENSE-LOBE-ICONS`](LICENSE-LOBE-ICONS) |
| `claude.svg` | Claude | [simple-icons `icons/claude.svg`](https://github.com/simple-icons/simple-icons/blob/cd796c98565fc466923fc1128b962e133fa3c612/icons/claude.svg) | CC0-1.0 |
| `opencode.svg` | OpenCode | [simple-icons `icons/opencode.svg`](https://github.com/simple-icons/simple-icons/blob/cd796c98565fc466923fc1128b962e133fa3c612/icons/opencode.svg), drawn from OpenCode's `packages/identity/mark.svg` | CC0-1.0 |
| `gemini.svg` | Gemini | [simple-icons `icons/googlegemini.svg`](https://github.com/simple-icons/simple-icons/blob/cd796c98565fc466923fc1128b962e133fa3c612/icons/googlegemini.svg) | CC0-1.0 |

Simple Icons does not list a separate licence for these three icons, so they
fall under the project's CC0-1.0 dedication. Its
[disclaimer](https://github.com/simple-icons/simple-icons/blob/develop/DISCLAIMER.md)
leaves the trademarks with their owners. OpenAI's mark is not in Simple Icons,
so it comes from Lobe Icons.

Retrieved 2026-11-14. SHA-256 of the upstream files:

- `openai.svg`: `a595df6b423920c67a7f8f73c063e4bfb72d415948097b6cac063a2366bb5186`
- `claude.svg`: `2d6fda79eb18ddccca35b799eeb3cece0dfabc22520ce3b10abd25668df9fa93`
- `opencode.svg`: `6f7d203200ba051cdbaa726c4de0a729cee3f1cec47953f189399d951af58c4c`
- `googlegemini.svg`: `a6228d8846040401e941a39ed17459785e8324dbb7b870ce84233c9b3e9863a0`

Pi and a custom ACP agent use the Lucide Bot and Plug glyphs.
