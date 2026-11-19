# Desktop defect repairs: delivery

## Verification

- Original/capture base: `c9ecbf678019bb0343d8357258465ac58f0017f3`.
- Rebased the five fixes onto integration head
  `a0a8f252f379e5e801ebfb4f51adb86bee34de14` after its theme changes landed.
- Four client regression tests cover chosen-account priority with case folding,
  unknown/absent choice falling back to the first EMU login, no match, and
  credential-viewer verification. They pass against offline synthetic inputs.
- All five `view::lanes::tests` passed before and after the byte-for-byte move.
- The Actions `saved_account` context test passed: absent, shared, then
  board-specific preference. No live GitHub calls were made.
- `cargo shear` reports no issues after the module move.
- The four-fix preparation's `make check` passed, including architecture, documentation, formatting,
  cargo-shear, clippy with warnings denied, workspace tests and script fixtures.
  The existing artwork/theme-path and title-bar headless tests pass.
- `mise run quality` passed at 100% with the fifth fix too; no new findings in touched code.
  `mise run quality-baseline` changes only the function-count `total`, from
  16,075 to 16,532 across the complete rebased five-fix tree.
  All scores and finding counters are unchanged. The tool prints 86 existing
  chart magic-number advisories, also present in the base scan; its scored
  findings remain zero. No quality configuration or suppressions were added.
- `mise run audit` passed for all three lockfiles, with existing
  advisory-not-detected warnings for documented exceptions.
- `make interfaces` passed against the real product's headless interfaces.
- Full pre-commit and full pre-push hooks passed. The first pre-push attempt
  encountered a missing Copilot SDK build artifact during concurrent capture
  compilation and detected an in-progress record edit; the complete serial
  rerun passed without concurrent builds or edits.
- Main remains at the capture base; integration advanced to the rebase head
  above. The Mac capture instructions deliberately retain the requested
  original revision for the before images.
- The captain authorised five conventional commits and signed publication
  while Mac captures are pending. The PR must remain unopened. Final-head
  `make check`, quality, interfaces, audit and full pre-push results are
  recorded in the prepared PR body after publication; the four-fix preparation
  results above are not a substitute for those gates.
- The shared repository's local config has no `core.worktree`, `user.name`
  or `user.email` overrides.

## Evidence-pass verification

Rebased onto `c58a31de198510586e80fe5b329dcc8f967e64b3` on 16 November 2026,
including release 0.4.0, the notification sound switch and the ACP Stop fix.
There were no conflicts; `git range-diff` confirms all five repair patches
are unchanged. This base includes freshly fetched `origin/main`.

Before the evidence commit, `make check`, `mise run quality`,
`make interfaces`, `mise run audit` and the full
`prek run --stage pre-push --all-files` passed. Workspace tests used
`RUST_TEST_THREADS=4` with no test-exclusion flags. The standard suite retains
its existing ignored public-model download probe and platform fixture skips.
Quality is 100%, with zero scored findings and only the same 86 existing chart
advisories outside touched code. The current scan counts 16,588 functions;
the previously recorded baseline update remains function-count-only, with no
new baseline, configuration or suppression changes in this evidence commit.
Final evidence-head gates and signed publication are recorded in the PR body.

## macOS title-bar evidence

jt captured the four fixture-only images on macOS with the canonical
`make capture`: before at `c9ecbf678019bb0343d8357258465ac58f0017f3`, after at
`63bf98574271b8dfcd66a4feccf4a56c103b05bc`, in Cupertino Dark and Cupertino
Light. These replace the earlier Linux capture blocker (`plutil` unavailable).
The supplied originals are 2560 × 1600 at 2× and remain unchanged outside
the committed evidence set.

Each committed PNG is the same native-pixel rectangle, `1600x76+0+0`, cropped
with mise's pinned ImageMagick 7.1.2_31, without resizing. Its 76-pixel height
matches the Cupertino title bar's 38-point height at 2×; it retains the logo,
tabs and the complete left window-control area, with no board content. The
canonical captures themselves omit the native traffic-light buttons. All
four crops were inspected at native resolution: no real accounts, credentials
or internal hostnames; the after marks have continuous strokes and retain
their pale/dark tones.

![Before: Cupertino Dark title bar at c9ecbf6.](evidence/before-dark/titlebar-crop.png)
![After: Cupertino Dark title bar at 63bf985.](evidence/after-dark/titlebar-crop.png)
![Before: Cupertino Light title bar at c9ecbf6.](evidence/before-light/titlebar-crop.png)
![After: Cupertino Light title bar at 63bf985.](evidence/after-light/titlebar-crop.png)

These 2× captures cannot show the 1× improvement. The
[specification](spec.md#title-bar-mark) describes its rendering mechanism:
draw a 14-device-pixel SVG mask instead of shrinking a 512-pixel raster about
36:1. No physical 1× display result is claimed. The subsequent rebase onto
`c58a31de1985` was conflict-free and does not change the logo implementation;
the captions deliberately identify the revisions actually captured.

## Tool verification

The initial repair checks disabled ImageMagick auto-installation. The captain
subsequently installed the pinned `conda:imagemagick@7.1.2_31` through mise and
verified version 7.1.2-31 and `make check-icons` on Linux. This evidence pass
used that managed installation for the crops and repeated `make check-icons`:
all icon signatures, sizes and alpha pass. No global package was installed.

Installation on macOS and icon generation remain unverified. Generate icons
only on macOS with `rsvg-convert` and `iconutil` available; generation rewrites
the checked-in icons and is unnecessary for these repairs.

Back to [intent](intent.md); see [specification](spec.md) and [tasks](tasks.md).
