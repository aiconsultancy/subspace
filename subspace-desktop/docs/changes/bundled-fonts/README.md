# Bundled Inter and JetBrains Mono

Back to the [change records](../README.md).

## Intent

The owner wants Subspace to look the same on macOS, Windows and Linux, and
every screenshot to use the same fonts, whatever is installed on the machine
that runs it. Subspace embeds two open-source families: **Inter** for
interface text and headings, and **JetBrains Mono** for code, logs, keys and
counts. Both are under the SIL Open Font License 1.1. The **Twilight Two**
theme names them, so it and every screenshot taken in it render in them on a
machine that has neither installed.

The owner chose to keep the platform's fonts for themes that name no font:
Inter and JetBrains Mono are not new automatic fonts.

The Framework part is a breaking change: `ProductBuild` gains the public
field `fonts`. Commit it as `feat(framework)!` with a `BREAKING CHANGE:`
footer (`ProductBuild::fonts` is required; `fonts: &[]` keeps the old
behaviour). The Framework's own record is
[Embedded fonts](../../../../subspace-framework/docs/changes/bundled-fonts/README.md).

## Spec

- The Subspace App embeds twelve static TrueType files, about 4.2 MB, in its
  executable: Inter and JetBrains Mono at 400, 400 italic, 500, 600, 700 and
  700 italic. Static files, not variable fonts, because the macOS text system
  registers a variable font as one face at its default weight. The files are
  in `products/subspace/assets/fonts/`; their licences and provenance, with
  every hash, are in `licenses/fonts/`, which every release archive and the
  macOS app bundle carry.
- The Framework's `ProductBuild::fonts` lists an app's embedded fonts
  (`BundledFont`, a family name and the file's bytes). The shell registers
  them with the GPUI text system (`TextSystem::add_fonts`) when the desktop
  window's application starts, before the theme is applied, so the capture
  build gets them too. Apps that list none behave as before.
- A registered family counts as installed when a theme, Settings or the theme
  designer asks for it by name, and appears in the font lists.
- Automatic font choices skip a family only the app provides, so themes that
  name no font keep the platform's fonts. A family the platform already had
  stays an automatic candidate.
- If the platform refuses the fonts, the window shows a notice and themes fall
  back as they do for any family that is not installed.

## Delivery

Tests in `products/subspace/src/fonts.rs`, each reading the embedded files'
own OpenType tables:

- every face is a static TrueType font of its declared family;
- each family has exactly 400, 400 italic, 500, 600, 700 and 700 italic;
- every file is under the repository's 1,024 KB limit, and all under 5 MiB;
- the shell's registration counts both families as installed.

The packaged-licence fixture test (`scripts/test-prerelease-products.sh`)
checks that `licenses/fonts/` reaches every archive byte for byte. The
Framework's tests are listed in its record.

These tests, and the Framework's, passed on 21 November 2026 in plain
`cargo test --workspace --locked --no-fail-fast` on the branch's working
tree: the app's in the Subspace App suite (`products/subspace`, 48 passed,
1 ignored), the Framework's in the host app suite (`subspace-shell`, 591
passed). The packaged-licence fixture test was not part of that run.

On the final working tree, on 21 November 2026, `make check` passed in the
Framework and in the desktop (including the packaging and script fixtures)
and `mise run quality` passed in both.

Screenshots: the workspace shell's captures in Twilight Two, taken with
`make capture` from the final build of this branch, render in Inter and
JetBrains Mono with no missing-font notice, for example
[the Issues list](../workspace-shell/evidence/issues.png). A capture of
Cupertino Dark from the same build,
[the Issues list in Cupertino Dark](../workspace-shell/evidence/issues-cupertino-dark.png),
shows that theme in the platform's own font. Not recorded: whether the
capture machine had either family installed.

Not done:

- The capture of Twilight Two without the embedded fonts, showing the
  missing-font notice. It was not taken, and there is no before capture of
  Cupertino Dark from a build of `main` to show it unchanged.
- Windows and Linux rendering of the embedded fonts, which has not been
  checked. Windows may list "JetBrains Mono Medium" as a family of its own,
  and on Linux an Inter installed on the system may be chosen over the
  embedded one.

## Tasks

- [x] Download the two approved release archives, choose the static files and
  record their provenance, licences and hashes.
- [x] Add the font files, `licenses/fonts/`, the NOTICE line, the Git
  attributes and the hook exclusions that keep the licence bytes.
- [x] Framework registration module and the product's font list, with tests.
- [x] Wire `ProductBuild::fonts` and registration into the shell's startup.
- [x] Count registered families as installed; keep automatic choices on the
  platform's fonts.
- [x] Update the appearance guide, the capture guide, the Framework README
  example and every `ProductBuild` literal, including the Harbor spike.
- [x] Unit tests in both projects.
- [x] Screenshots in Twilight Two and Cupertino Dark with the fonts
  embedded.
- [x] `make check` and `mise run quality` in both projects (21 November
  2026, all passed).
- [ ] A screenshot of Twilight Two without the embedded fonts, with the
  missing-font notice. Not taken.
- [ ] Windows and Linux rendering checked.
