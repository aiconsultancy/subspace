# Linux offscreen gallery proof

`linux_offscreen.rs` owns the opt-in capture entry point and five pure parsing
and height tests. Its test-only `support/` modules separate settings validation,
theme fixtures, GPU rendering and manifest output. This split does not change
the fixture names, viewport rules, capture count or PNG output.

`linux_offscreen_gallery_pngs` renders the existing catalogue cards in
Subspace One Dark and Cupertino Dark. It calls
`ComponentGallery::render_test_item`, which accepts every exact canonical
catalogue name, including the GPUI Kit cards. It does not replace samples or
change the production view. Without a selection, the original three cards
(`action::action_button`, `fields::input`, `feedback::error_box`) still produce
six PNGs with their original filenames and 800 × 600 logical viewports.

For the full 180-card catalogue, pass generated selections in bounded batches
of at most 12 cards. Each process captures both themes, at most 24 PNGs.
Fifteen complete batches produce 360 PNGs. The caller owns catalogue export,
batch scheduling and verification of complete coverage across batches. The
native test does not keep a second 180-entry catalogue.

This opt-in integration test uses GPUI's `TestPlatform`, not an application
launcher. `HeadlessAppContext::with_platform` receives the native Linux text
system and `gpui_kit::platform::current_headless_renderer`. In GPUI
pre-0.3.12, that public function supplies `WgpuHeadlessRenderer` on Linux.
`Window::render_to_image` reads the rendered scene back to RGBA pixels.

## Requirements

- Linux native build dependencies required by the pinned GPUI Kit.
- The Framework's pinned Rust 1.99.0 and Mr Boxington 1.19.0 through mise.
  Do not install or change tools for capture. Use one Cargo build job and
  one test thread. Wait for the coordinator to release any other native
  build or pre-push check before running these commands.
- A working WGPU adapter. A Vulkan software driver is sufficient; no display
  server, installed application or screen-capture permission is needed.
- Installed font families named exactly `Inter` and `JetBrainsMono Nerd Font`.
  The test refuses missing families rather than accepting a fallback.
- An explicit fixture directory containing `one-dark.json` and `cupertino.json`.
  These are copies of the tracked theme documents, not application configuration.
- An absolute output path which does not exist yet. Its parent must exist.

## Selection and filenames

`SUBSPACE_GALLERY_CASES` is an optional JSON array, supplied as an environment
value, not a file path. Each entry has exactly two string fields:

```json
[
  { "item": "action::action_button", "slug": "action-button" },
  { "item": "gpui_kit::component::button::Button", "slug": "kit-basic-gpui-kit-component-button-button" }
]
```

`item` is the exact, case-sensitive production name. Unknown names fail when
the production selector renders them. `slug` is the published web card ID.
Preserve the existing `action-button`, `input` and `error-box` aliases.
For other cards, the web catalogue uses the section ID, a hyphen, then the
lowercase canonical name with each run of non-ASCII-alphanumeric characters
replaced by a hyphen and edge hyphens removed. Keep the full
`gpui-kit-component` portion of GPUI Kit names. Pass the exported web slug
instead of deriving another native alias.

The test rejects empty or oversized batches, duplicate names, duplicate slugs,
unsafe filenames and extra fields. Slugs contain only lowercase ASCII letters,
digits and single hyphens, with no leading or trailing hyphen. Each PNG is
`${theme}-${slug}.png`; `theme` is `one-dark` or `cupertino-dark`.
Each manifest record retains both the exact `item` and its web `slug`.
The selection cannot override paths, fonts, themes or scale. A separate,
explicit `SUBSPACE_GALLERY_MIN_HEIGHT` may raise the full-frame logical height
from 600 to at most 4096 for a matched visual-review viewport. Invalid values
fail before output creation. It does not scale or restyle the card. The original
six fixed-size references remain unchanged; this option is only for new
versioned full-gallery visual evidence. The matched chart pair uses height 1088
at DPR 2; measured native list heights are 659 and 658 by theme.

## Run after native resource release

From `subspace-framework`, prepare isolated theme input and output locations:

```sh
export VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json
export RUST_FONTCONFIG_DLOPEN=1
export CARGO_TARGET_DIR="$HOME/.cache/component-gallery-cargo-target"
export CARGO_BUILD_JOBS=1
export RUST_TEST_THREADS=1
mise exec -- rustc --version # must report 1.99.0
mise exec -- mbx --version   # must report 1.19.0
proof=$(mktemp -d "${TMPDIR:-/tmp/opencode}/gallery-proof.XXXXXX")
mkdir "$proof/themes"
cp host/app/themes/one-dark.json host/app/themes/cupertino.json "$proof/themes/"
SUBSPACE_GALLERY_THEME_FIXTURES="$proof/themes" \
SUBSPACE_GALLERY_PNG_OUTPUT="$proof/png" \
SUBSPACE_GALLERY_SOURCE_REVISION="$(git rev-parse HEAD)" \
mise exec -- cargo test -p subspace-component-gallery-board \
  --features test-support --test linux_offscreen --locked --jobs 1 \
  linux_offscreen_gallery_pngs -- --ignored --exact --nocapture --test-threads=1
```

To capture all cards, the coordinator first writes `all-cases.json` in the
shape above from the web catalogue's exact canonical-name/slug export. Split
that mapping, without copying card definitions, and run one batch at a time:

```sh
python3 - "$proof" <<'PY'
import json
import pathlib
import sys

proof = pathlib.Path(sys.argv[1])
cases = json.loads((proof / "all-cases.json").read_text())
assert len(cases) == 180
assert all(set(case) == {"item", "slug"} for case in cases)
assert len({case["item"] for case in cases}) == 180
assert len({case["slug"] for case in cases}) == 180
(proof / "cases").mkdir()
for start in range(0, len(cases), 12):
    batch = proof / "cases" / f"batch-{start // 12:03}.json"
    batch.write_text(json.dumps(cases[start:start + 12]))
PY

for batch in "$proof"/cases/batch-*.json; do
  name=$(basename "$batch" .json)
  SUBSPACE_GALLERY_CASES="$(cat "$batch")" \
  SUBSPACE_GALLERY_THEME_FIXTURES="$proof/themes" \
  SUBSPACE_GALLERY_PNG_OUTPUT="$proof/$name" \
  SUBSPACE_GALLERY_SOURCE_REVISION="$(git rev-parse HEAD)" \
  mise exec -- cargo test -p subspace-component-gallery-board \
    --features test-support --test linux_offscreen --locked --jobs 1 \
    linux_offscreen_gallery_pngs -- --ignored --exact --nocapture --test-threads=1 \
    || break
done
```

Keep every batch's manifest. Verify that the union has the 180 expected names
in both themes, with no duplicate `(item, theme)` pair, and 360 decoded PNGs.
A partial batch or failed command does not establish complete capture. Use a
new output directory for a retry; never merge stale files into new evidence.

The five non-ignored selection and height tests need no GPU capture. Run them
with the same toolchain and resource limits, also after native resource release:

```sh
mise exec -- cargo test -p subspace-component-gallery-board \
  --features test-support --test linux_offscreen --locked --jobs 1 \
  -- --test-threads=1
```

If this fails because no adapter exists, stop. The host operator must supply
a working Vulkan ICD or EGL/OpenGL driver before PNG capture can run. Do not
install graphics packages as part of the test.

The test parses the supplied documents with GPUI Kit's public `ThemeSet`
and `Theme::apply_config`. It also applies the documents' Subspace density
and tone metadata through the public dashboard globals. Font overrides set
Inter for body text and JetBrainsMono Nerd Font for the mono role. No saved
preferences or service configuration are loaded. No theme assets are copied
into this crate or imported from a host crate.

Each capture starts at 800 × 600 logical pixels at the test platform's pinned
2× scale. Most PNGs are therefore 1600 × 1200. Some production cards contain
many samples: `toggle::switch` has seven surface rows with six switch states
each, and `gpui_kit::component::chart::BarChart` has seven 300 × 180 chart
samples. At 800 logical pixels wide, their natural height can exceed 600.

The fixture measures the actual card in a non-shrinking wrapper. If required,
it reopens the offscreen window at the natural card height plus the original
bottom inset, rounded up. Height is bounded at 4096 logical pixels. It never
rescales, crops, removes or restyles samples to fit. Width remains 800, and a
card that exceeds the height ceiling or horizontal viewport fails. Each window
closes before the next card, so a batch does not retain 24 GPU windows.

`manifest.json` records themes, exact card names, web slugs, font roles and
card bounds. **Use each card record's `width`, `height` and `scaleFactor` for
the matching web capture**, not the manifest's default `height` of 600. Height
can differ between themes because theme density changes text and wrapping.
The original six captures remain fixed-size if their natural bounds still fit.

The test checks that each named production card exists, has positive bounds
and fits inside its final viewport. It rejects uniform images, decodes each
saved PNG and compares its pixels with GPU readback. An existing output
directory fails to prevent stale images from being mistaken for new evidence.

## Limits

This proves fixture cards at rest on Linux. It does not prove native window
chrome, pointer or keyboard workflows, installed-app behaviour, real-data
integration, macOS, or WASM. Inspect the PNGs for visual defects. The pixel
and bounds checks are not a visual baseline or an accessibility audit.
Card containment does not prove that every internal sample avoids overflow;
inspect complete PNGs for that defect rather than treating outer bounds as a
complete clipping audit. No maximum measured catalogue height is claimed until
all batches have actually run.

Theme parsing uses public configuration APIs rather than the host's private
theme-selection code. It does not test host font fallback, custom theme
storage, preference migration or the theme designer.
