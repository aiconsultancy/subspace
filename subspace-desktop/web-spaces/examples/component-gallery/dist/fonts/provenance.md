# Bundled font provenance

These are the same complete fonts used by the Linux GPUI reference fixture.
No glyph was removed. Font names and outlines are unchanged.

## Inter

- Family: Inter Regular, version 4.001, git `9221beed3`.
- Author: Rasmus Andersson and the Inter Project Authors.
- Download: Inter-4.1 release archive,
  <https://github.com/rsms/inter/releases/tag/v4.1>.
  Its static `Inter-Regular.ttf` reports internal version 4.001.
- `Inter-Regular.ttf` is an unmodified copy of the fixture's installed font.
- SHA-256: `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82`.
- Licence: `Inter-OFL.txt`, copied from that upstream release.

## JetBrainsMono Nerd Font

- Family: JetBrainsMono Nerd Font Regular.
- JetBrains Mono version 2.304, patched by Nerd Fonts version 3.5.1.
- Base authors: Philipp Nurullin, Konstantin Bulenkov and JetBrains.
- Patch project: Ryan L McIntyre and Nerd Fonts contributors.
- Source: <https://github.com/ryanoasis/nerd-fonts/releases/tag/v3.5.1>.
- Original TTF SHA-256: `1c680e8cde9fcf8b88a5605ce8d1fb94dd3fb15841f7ca7bf4c55664855e5611`.
- Bundled WOFF2 SHA-256: `1cbaa2b824a6cb1ae5c7910832062df0367b417cd58cf564162df16686475e13`.
- Lossless full-font WOFF2 conversion with FontTools 4.68.0 and Brotli 1.2.1.
  The WOFF2 is 1,046,472 bytes, below the repository's 1,024 KiB file limit.
- The original font's OFL notice is `JetBrainsMono-OFL.txt`.
- `NerdFonts-LICENSE` and `NerdFonts-license-audit.md` retain upstream patch
  licensing and glyph-source attribution. Files in `glyph-licences/` retain
  the additional notices shipped with those glyph sources at Nerd Fonts 3.5.1.
  The Nerd Fonts MIT licence covers its patcher source, not every glyph.

Reproduction, with the fixture's original TTF and FontTools installed:

```sh
fonttools ttLib.woff2 compress JetBrainsMonoNerdFont-Regular.ttf \
  -o JetBrainsMonoNerdFont-Regular.woff2
```

Neither font is licensed as application source code. Keep these notices when
redistributing the built web Space.
Licence text whitespace follows the repository's formatting hooks; font
bytes and licence wording are unchanged.
