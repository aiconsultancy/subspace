# Embedded fonts

## Intent

An app built from the Framework should be able to embed font files, so a
theme that names their families looks the same on macOS, Windows and Linux,
and in every screenshot from its capture build, without the fonts being
installed. The Subspace App embeds Inter and JetBrains Mono for its
**Twilight Two** theme. Themes that name no font must keep the platform's
fonts: the owner has not chosen new automatic fonts.

This is a breaking change to the shell's public API: `ProductBuild` has a new
public field, so every struct literal that builds one must add it. Mark the
commit `feat(framework)!` with a `BREAKING CHANGE:` footer that names
`ProductBuild::fonts` and says `fonts: &[]` keeps the old behaviour.

## Spec

- `subspace_framework::shell::BundledFont` is one embedded font file: its
  family name (the font's typographic family) and its bytes, usually from
  `include_bytes!`. `BundledFont::new` is a `const fn`.
- `ProductBuild::fonts: &'static [BundledFont]` lists an app's embedded
  fonts; `&[]` embeds none and behaves as before.
- At desktop startup the shell registers them with the GPUI text system
  (`TextSystem::add_fonts`, borrowing the bytes) and only then applies the
  saved theme (`theme::start`), so a theme that names them starts without a
  missing-font notice. The capture build starts the same way. If the platform
  refuses the files, the shell shows one notice and themes fall back as they
  do for any family that is not installed.
- `theme::font_names`, which Settings, the theme designer and theme
  resolution read, includes the registered families even where the text
  system does not list them (the test platform's).
- `theme::fonts::resolve` takes the families only the app provides (those
  the platform did not list before registration) and leaves them out of the
  automatic choices. A theme or Settings that names one resolves it; a theme
  that names no font keeps the platform's fonts. A family the platform also
  has stays an automatic choice.
- `test_support::registered_font_families` lets an app's tests check its
  font list against the shell's registration.
- The Framework itself embeds no font files.

## Delivery

Tests:

- `bundled_fonts`: each family is listed once and platform families are not
  bundled-only; `without` skips families without case; registered families
  count as installed; a refused registration reports it and registers
  nothing.
- `theme::fonts`: a bundled-only family answers a request but is never an
  automatic choice; a family the platform also has stays automatic. The
  existing tests pass the new argument.
- `theme::preview_tests::bundled`: Twilight Two warns that Inter is not
  installed before registration, and after `theme::start` with registered
  fictional font data it starts with no notices in Inter and JetBrains
  Mono; for Solarized Light, the registered families are listed as installed
  but are not the automatic body or code font.
- The README's example sets `fonts: &[]`.

These tests passed on 21 November 2026 in plain
`cargo test --workspace --locked --no-fail-fast` on the branch's working
tree (69 test binaries, 2,396 passed, 4 ignored); the `bundled_fonts`,
`theme::fonts` and `theme::preview_tests::bundled` tests are in the host app
suite (`subspace-shell`, 591 passed). On the final working tree, on
21 November 2026, `make check` passed in the Framework and `mise run quality`
passed in the Framework with no new findings against the baseline.

Screenshots, and what has not been checked on Windows and Linux, are recorded
in the Subspace App's change record,
`subspace-desktop/docs/changes/bundled-fonts/README.md`.

## Tasks

- [x] `BundledFont`, registration and the installed and automatic lists.
- [x] `ProductBuild::fonts` and startup order.
- [x] Tests, README example, appearance guide.
- [x] `cargo test --workspace --locked --no-fail-fast`.
- [x] `make check` and `mise run quality` (run by the main session on
      21 November 2026; both passed).
