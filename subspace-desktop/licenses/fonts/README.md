# Bundled font licences

The Subspace App embeds two font families in its executable and registers
them with the text system at startup, so every platform, and every
screenshot, can draw the same faces without the fonts being installed. Each
family is licensed under the SIL Open Font License, Version 1.1. Neither
licence names a Reserved Font Name. The fonts are unmodified copies of the
upstream release files: no glyph, name or table has been changed.

| Family | Licence text | Copyright |
| --- | --- | --- |
| Inter | [LICENSE-INTER](LICENSE-INTER) | The Inter Project Authors |
| JetBrains Mono | [LICENSE-JETBRAINS-MONO](LICENSE-JETBRAINS-MONO) | The JetBrains Mono Project Authors |

Both licence files keep the upstream bytes, so the repository's whitespace
hooks skip them.

The font files are in the source tree under
`products/subspace/assets/fonts/`. They are not shipped as separate files:
the executable contains them.

## Inter

- Source: <https://github.com/rsms/inter>, release
  [v4.1](https://github.com/rsms/inter/releases/tag/v4.1).
- Archive: `https://github.com/rsms/inter/releases/download/v4.1/Inter-4.1.zip`,
  SHA-256 `9883fdd4a49d4fb66bd8177ba6625ef9a64aa45899767dde3d36aa425756b11e`.
- Files: the static TrueType fonts in the archive's `extras/ttf/`. Each
  reports internal version 4.001 (`git-9221beed3`).
- Licence: the archive's `LICENSE.txt`, copied as `LICENSE-INTER`,
  SHA-256 `262481e844521b326f5ecd053e59b98c8b2da78c8ee1bdbb6e8174305e54935a`.

| File | Weight | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `Inter-Regular.ttf` | 400 | 411,640 | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| `Inter-Italic.ttf` | 400 italic | 417,388 | `bbc051dd204b5019a1aa0bc0ae2aa8a05ab13e7a3f979fa357631dc7feb6833a` |
| `Inter-Medium.ttf` | 500 | 417,300 | `97ad806f526e41546d46365bb3a393145f75b7b1568913db74549ad8b8dba872` |
| `Inter-SemiBold.ttf` | 600 | 419,744 | `78a843fade9d4612a5567302fb595b56976eb5fcebf4fea5a5912d638bafcde3` |
| `Inter-Bold.ttf` | 700 | 420,428 | `288316099b1e0a47a4716d159098005eef7c0066921f34e3200393dbdb01947f` |
| `Inter-BoldItalic.ttf` | 700 italic | 425,296 | `948405a16cdc62701da5f4005ed068ca5f4d27061d98f7974ccfc37831d9581d` |

## JetBrains Mono

- Source: <https://github.com/JetBrains/JetBrainsMono>, release
  [v2.304](https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304).
  This is the upstream font, not a Nerd Font build.
- Archive: `https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip`,
  SHA-256 `6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf`.
- Files: the static TrueType fonts in the archive's `fonts/ttf/`, with
  ligatures (not the `JetBrainsMonoNL` files).
- Licence: the archive's `OFL.txt`, copied as `LICENSE-JETBRAINS-MONO`,
  SHA-256 `30f0c136e3c88e422d0791acd97238870f9054a9729bc34cf2ff0d4ed8cac4ad`.

| File | Weight | Bytes | SHA-256 |
| --- | --- | --- | --- |
| `JetBrainsMono-Regular.ttf` | 400 | 273,900 | `a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f` |
| `JetBrainsMono-Italic.ttf` | 400 italic | 276,840 | `9d0a1f7a708e6af183f1193b7e81d40da294f5c67682c085d8401c60aac8ded4` |
| `JetBrainsMono-Medium.ttf` | 500 | 273,860 | `31c92d01a8a08528b718a43addf0ad3df0af2ca4b7b3290a452f70f358e14d3d` |
| `JetBrainsMono-SemiBold.ttf` | 600 | 277,092 | `1b3bfa1ed5665a4ce3f9feb68d2d4e40e70bf8b4b7d9a3edd418f321b4e166a0` |
| `JetBrainsMono-Bold.ttf` | 700 | 277,828 | `5590990c82e097397517f275f430af4546e1c45cff408bde4255dad142479dcb` |
| `JetBrainsMono-BoldItalic.ttf` | 700 italic | 279,832 | `4039d5ce0ed225bf9c8b2c8c6436290ae2f356b7e90d70fa666227238324aa3b` |

## Updating

Download the new release archive from the project's own release page,
take the same static files, check that each is a TrueType font, and update
the version, archive and file hashes above in the same change. Keep the
licence files byte for byte as released.
