# React component gallery web Space: phase 1 plan

## Slice 1 resumption, 18 November 2026

Jack approved Linux prerequisites in Slack message `1795020911.188469`.
The captain installed Mesa Vulkan drivers 26.0.8, Vulkan loader, and XCB/
xkbcommon development libraries. Use only the pinned Lavapipe ICD through
`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json` for CPU references.
The direct GPUI renderer now initialises successfully. Continue the fixture
PNG proof, then the previously approved bounded web and diff work. No further
system packages are authorised.

Continuation task: `task_e16a4906-ed7c-49c5-8498-c32093340963`, Adama;
branch the feature branch in the retained isolated clone.

## Approved phase 2 decisions, 18 November 2026

Jack approved slice 1 in thread `1795017294.971179`, message
`1795019557.224699`. These decisions supersede the exploratory options below.

- Target all 180 current gallery cards, including shared helpers and shell
  examples. Slice 1 proves representative cards, not full catalogue coverage.
- Themes are **Subspace One Dark** and **Cupertino Dark**.
- Drop WASM entirely. Compare GPUI reference PNGs captured offscreen on this
  Linux host against headless Chromium screenshots of React/shadcn examples.
- Spike GPUI Linux headless/test rendering and its screenshot API first. If
  that cannot produce real rendered images, stop and report evidence/options.
  No macOS runner is available and no host-CSP change is in scope.
- Pixel acceptance is zero differences except anti-aliased edges, on this host.
  The previous proposed 0.5% tolerance is rejected. Report every remaining
  non-edge difference, without hiding layout, colour or text mismatches.
- Fonts are Inter and JetBrainsMono Nerd Font. User-level font installation
  under `~/.local/share/fonts` and `fc-cache` are authorised; no apt or sudo.
- Once Linux rendering works, build the web Space scaffold, both theme presets
  and screenshot/diff harness. Review with one Opus 5.5 visual child plus normal
  repository checks. Commit/push a feature branch and open a **draft** PR for
  slice 1 only. Return sample image paths and stop after slice 1.

Phase 2 task: `task_2ce66e8c-0839-48e6-a501-017156ccc47c`, originating captain
Adama. Feature branch: the feature branch. The isolated clone
and durable session/worktree arrangement are unchanged.

Change record: [intent](intent.md), [slice 1 specification](spec.md),
[delivery](delivery.md), [Linux proof task](task-01-linux-proof.md), and
[Linux spike evidence](linux-render-proof.md).

## Phase 1 research retained below

18 November 2026. Planning only for the web component gallery issue.
Owner request: Jack, Slack thread `1795017294.971179`, message
`1795017673.613439`, channel `C0CEQ7JRZND`.
Source baseline: `aiconsultancy/subspace` commit
`42b07e3377fd2a3ed892d7413bf15ca0199b8f24`.
Branch: a planning branch. No implementation, commit, push or PR.

## Recommendation

Inventory: **180 cards in 16 sections**, comprising 81 shared Subspace cards,
15 shell cards and 84 GPUI Kit cards. The Kit catalogue marks 39 used and 45
unused. Full card/sample/state references and token values are in
[Inventory and theme mapping](inventory-and-themes.md).

Build a React/shadcn web Space that mirrors the existing gallery, including
shared Subspace components and shell examples, not just the upstream controls.
Keep catalogue IDs, example text, navigation, filtering and interactions equal.
Use a generated coverage manifest to prevent missing cards or states.

Do not rule out GPUI WASM. The pinned `gpui-kit` 0.9.1 explicitly exposes a web
platform. The gallery and its Framework dependencies still need a portability
check. Separate browser rendering from native web-Space integration acceptance.
Linux currently cannot embed a web Space, even when a display server exists.

Start with one representative button/input/popover row in both themes. Prove
fonts, WASM loading and image comparison before implementing the full catalogue.
If the full GPUI gallery cannot run in WASM, show native reference images next
to interactive React examples and label them as images. Never claim interactive
GPUI parity for a screenshot. Native capture remains the authority for matching
the desktop library; a WASM renderer can differ from desktop text rendering.

## Source locations and scope

Paths below are relative to the repository root. The ticket's
`docs/web-spaces-guide.md` and `docs/changes/component-gallery-complete/intent.md`
actually live under `subspace-desktop/`. There is no root `AGENTS.md` at this
baseline; read `subspace-desktop/AGENTS.md` and `subspace-framework/AGENTS.md`.

- Gallery: `subspace-framework/spaces/component-gallery/src/`, especially
  `catalog.rs`, `samples.rs`, `samples/`, `state.rs`,
  `view.rs`, `view/`, `browse.rs` and `lib.rs`.
- Existing intent:
  `subspace-desktop/docs/changes/component-gallery-complete/intent.md`.
- Component contract: `subspace-framework/docs/ui-components.md`.
- Web Space guide: `subspace-desktop/docs/web-spaces-guide.md`.
- Packaging: `subspace-desktop/scripts/package-web-space.py` and Weather's
  package tests under `subspace-desktop/web-spaces/examples/weather/test/`.
- Native web view: `subspace-desktop/spaces/web/src/page.rs` and
  `page/native.rs`; asset MIME handling: `spaces/web/src/app.rs`.
- Desktop capture: `subspace-desktop/scripts/capture.sh` and
  `subspace-framework/host/app/src/capture.rs`.

The companion inventory records every catalogue card and its sample dispatch.
Implementation expands each card's demonstrated variants into explicit case IDs;
unsupported states remain visible failures in the coverage report, not omissions.

## Web Space build and loading

Proposed folder: `subspace-desktop/web-spaces/examples/component-gallery/`.
Use React, TypeScript, Vite, Tailwind and locally owned shadcn components.
Adapt shadcn composition, tokens and dimensions to GPUI rather than accepting
shadcn defaults. Use Lucide SVGs from the same icon family as the native gallery.
No CDN scripts, external fonts or runtime package loading.

Minimum source layout:

```text
component-gallery/
  package.json
  index.html
  public/subspace-board.json
  src/main.tsx
  src/themes.css
  src/catalogue.ts
  src/components/
  tests/
  dist/                    built installable package
    index.html
    subspace-board.json
    assets/
```

Manifest: `{"name":"Component gallery","version":"0.1.0","permissions":[]}`.
Set Vite `base: './'`; build external JS/CSS files without inline handlers or
eval. Copy the manifest into the output. Subspace serves built assets itself;
it does not run Node or Vite. Validate `dist/` with the existing packager's
`--check`, then create a ZIP outside `dist/`. Enforce existing entry/size limits.

Install using Settings > Application > Web Spaces > Add board… > Local folder,
select `dist/`, Validate source, Install now. Installed folders are snapshots;
rebuild and Replace source after changes. A development `webBoards` path can
serve the live output with Reload page. Do not edit a real user's settings for
tests. Use fixture settings only. Standalone browser tests use a loopback static
server on an available port. They feature-detect `window.subspace`.

Add package tests modelled on Weather, a dedicated mise test task and packaging
validation. No permission is needed for local theme selection or examples.

## GPUI WASM feasibility

### Verified upstream support

The lockfile pins `gpui-kit` and `gpui-component` 0.9.1, with `gpui-pre`,
`gpui-pre-platform` and `gpui-pre-web` 0.3.12. Published crate sources were
downloaded from `https://static.crates.io/crates/<name>/<name>-<version>.crate`
into `.local/feasibility/upstream/` for inspection.

- `gpui-kit/src/lib.rs` re-exports `web` for `target_family = "wasm"`.
- `gpui-pre-platform/src/gpui_platform.rs` provides `web_init`,
  `single_threaded_web`, `application_with_web_backend` and WASM platform
  selection. Start a dedicated WASM library entry point, not the desktop binary.
- `gpui-pre-web/src/gpui_web.rs` documents one document-owned canvas and one
  top-level window. Renderer prefers WebGPU and falls back to WebGL2 through
  wgpu. Second or reopened top-level windows return `WebWindowError`.
- `gpui-pre-web/src/text_system.rs` uses Cosmic text without system fonts,
  accepts added font bytes and supports browser Canvas font fallback.
  `platform.rs` defaults to `IBM Plex Sans`.
- `gpui-component/Cargo.toml` gates native cpal, smol and tree-sitter dependencies
  away from WASM. This is evidence of deliberate browser support, not proof that
  every component and this gallery work in the browser.

### Integration blockers and bounded follow-up

The host currently sends `script-src 'self'` and `frame-src 'none'` in
`subspace-desktop/spaces/web/src/page/native.rs`. This does not permit normal
WASM compilation under CSP or iframe isolation. Although `.wasm` already has
`application/wasm` MIME handling, loading the file alone does not prove execution.
Do not add unsafe-eval or relax policy globally. Prove a narrowly scoped
`wasm-unsafe-eval` policy and tests, or leave GPUI in a standalone comparison
browser and use reference PNGs inside the installed web Space pending approval.

The web backend appends a full-size canvas to document.body. First try a
controlled single-document canvas container that does not require a backend
fork; verify resize, focus and event ownership. If isolation requires an iframe,
that needs a separate scoped host policy decision. A screenshot comparison works
under the existing policy and avoids this dependency.

The gallery imports `subspace-dashboard`, `subspace-core`, `subspace-charts` and
`subspace-lucide`. Core includes keyring, filesystem preferences and native HTTP
dependencies. Keep fixture samples; do not compile the desktop host or real
credentials into a browser. Probe the existing gallery unchanged first, then
isolate fixture-only rendering dependencies if the compiler identifies a native
dependency. `kit_usage.rs` is test-only, so its filesystem scan is not itself a
production WASM blocker.

Explicit risk cases, not a confirmed unsupported list: audio playback, native
file dialogs, window chrome/traffic lights, multi-window examples, clipboard and
IME permissions, syntax highlighting without native tree-sitter, persistent
preferences and links that open external applications. Test each applicable
gallery example and record supported, emulated or native-image-only. Ordinary
in-window overlays are not automatically multi-window failures.

### Compilation spike

Installed the authorised user-level repo-pinned Rust 1.99.0 and the
`wasm32-unknown-unknown` standard library. A 180-second-bounded check returned
exit 101, before reaching gallery code. `aws-lc-sys` 0.47.0 reported
`No target-specific source found: unknown-wasm32` and failed because `clang`
was absent. This does not establish whether installing clang would make that
dependency or the gallery portable. No system package was installed.

Full output: `.local/feasibility/gallery-wasm-check.log`. Reverse dependency
evidence: `.local/feasibility/aws-lc-chain.txt`. The chain includes gallery →
subspace-core → ureq/rustls → aws-lc-rs → aws-lc-sys; dashboard also brings core,
GitHub authentication and agent-tools. Browser fixture extraction must address
these transitive native services, not only replace the gallery's direct import.

The actual check used the clone's default `subspace-framework/target`, which is
under `~/.cache/tmp/`, never the RAM-backed `/tmp`. Future checks should use the
explicit reusable target directory below:

```sh
export CARGO_TARGET_DIR="$HOME/.cache/component-gallery-cargo-target"
mise exec -- cargo check --manifest-path subspace-framework/Cargo.toml \
  -p subspace-component-gallery-board --target wasm32-unknown-unknown --locked
```

A successful check only proves compilation. The next implementation gate must
produce wasm-bindgen JS/WASM output, render the representative row, deliver
pointer/keyboard events, and run under both browser and installed-host CSP.
No renderer or component is declared unsupported solely because this host lacks
a toolchain, display, browser or compatible runtime.

## Native and headless evidence

Research host is aarch64 Linux, without DISPLAY or WAYLAND_DISPLAY. Xvfb and
xvfb-run exist; weston and browser executables were not found on PATH. A display
server alone cannot provide the Rust toolchain, fonts or a usable GPU adapter.

The authorised capture path is `make capture`, which builds fixture-isolated
state and launches a macOS .app through `open -g -n -W`. It cannot run unchanged
on this Linux host. Do not bypass its account/credential isolation by launching
the installed app or writing an ad hoc capture launcher.

Preferred fallback: use an authorised macOS capture runner, exact source commit,
`app.dashboard = component-gallery`, each requested `CAPTURE_THEME`, fixture
state and scripted steps. Capture per card/state, crop to component bounds and
retain a whole-gallery overview. Keep original 2x PNGs and metadata; publish
review images at no more than 1600 px wide, as the app guide requires.

Upstream `gpui-pre-platform::current_headless_renderer` has a Linux
`WgpuHeadlessRenderer` implementation under test-support/bench-support. This is
a plausible off-screen Linux path, not a verified capture solution here.
Headless gallery tests already open production views, but layout assertions
alone do not prove rendered PNG parity. If macOS is unavailable, authorise a
Linux extension to the existing isolated capture path, probe off-screen wgpu
with a software adapter first, then Xvfb/X11 or a headless Wayland compositor
if windowing is required. Do not install system packages or change services
without approval. Browser screenshots can run headlessly, but acceptance inside
Subspace still needs macOS WKWebView or Windows WebView2. The current Linux
`page/unsupported.rs` cannot become a web view through Xvfb or weston.

## Pixel comparison contract

Generate one case per catalogue ID, demonstrated state and theme, including
default, hover, focus, pressed, disabled, selected, error, busy and open overlays
where the source actually shows them. Include compound shell examples and the
gallery navigation/search/hide-unused interactions. Avoid meaningless Cartesian
products of states a control cannot have.

Pin source SHA, package versions, OS, renderer/backend, browser build, viewport,
device scale, font file hashes, locale and clock. Set equal logical bounds and
2x output scale. Use identical fixture text, SVGs, font files, sizes, weight and
line height. Await font loading, image decode and two stable render frames.
Freeze animation/caret/blink and chart time, seed random data, place pointers
deterministically and restore focus. Never mask a whole component or text block
to manufacture a pass. Exact macOS system-font parity requires licensed access;
do not redistribute SF fonts to Linux or browser packages without checking terms.

Produce native, React and optional GPUI-WASM images plus overlay and difference
images. Fail missing baselines/cases, differing bounds, text wrapping, clipping
or state mismatches separately from colour differences. Proposed initial image
gate, subject to calibration: pixelmatch threshold 0.1, ignore anti-aliased edge
pixels, at most 0.5% changed pixels per crop, and no displacement greater than
1 logical pixel. Report exact-diff percentage as well. These are engineering
proposals, not achieved results or a definition of literal pixel perfection.
Calibrate using repeat captures and one deliberate colour/spacing regression;
do not increase tolerances to conceal a real mismatch. Font/OS differences get
separate baselines, never a shared silently widened tolerance.

Write JSON rows with case ID, theme, source SHA, reference mode, dimensions,
changed-pixel count/percentage, result and artifact paths. Generate an HTML
report grouped by theme/component with side-by-side images, overlay control,
heatmap and failure reason. Label image-only GPUI references clearly. Reports
include interactive coverage separately from visual coverage. CI returns nonzero
for missing or failing cases. Jack approves initial baselines and any waiver.

## Implementation slices

Indicative engineering days; refine after the first spike. No delivery dates
are promised. Slices are ordered so missing platform evidence stops wasted work.

| Slice | Estimate | Deliverable and acceptance |
| --- | --- | --- |
| 1. Runtime proof | 1–2 days | Compile gallery or identify exact native blockers; render representative fixtures in WASM; probe CSP and fonts; agree capture runner and fallback. |
| 2. Package and tokens | 1–2 days | React/shadcn build, valid web Space ZIP, two theme presets, sidebar/search/filter/copy names; install on supported native host. |
| 3. Controls | 2–3 days | Basic/form catalogue cards, all source variants, keyboard and pointer tests, both-theme captures. |
| 4. Composite examples | 3–5 days | Data, navigation, overlays, editors, charts/media and shared Subspace helpers; explicit per-case fallback list. |
| 5. Shell and comparison | 2–3 days | Shell examples and complete catalogue coverage; live GPUI pane where proved, native reference otherwise; deterministic case navigation. |
| 6. Acceptance | 1–2 days | Full two-theme image report, package/CSP tests, supported-host load, interaction coverage, usage docs and reviewed reference images. |

Likely total 10–17 engineering days, excluding platform provisioning and any
substantial Framework WASM extraction. Split a large extraction into separately
approved work instead of assuming it is part of a web-space rebuild.

## Decisions needed from Jack

1. Confirm current catalogue is the parity target, including shared helpers and
   shell examples, rather than a different upstream component demo app.
2. Choose which Subspace dark preset to copy. Sources register both
   `Subspace One Dark` and `Subspace Channel Dark`, plus `Cupertino Dark`.
   There is no registered `Subspace Dark`. The appendix maps all three to avoid
   guessing; implementation still delivers only the two approved themes.
3. Can phase 2 use an authorised macOS capture runner? Linux cannot currently
   run the approved capture script or embed the resulting web Space.
4. Is live GPUI in a standalone browser acceptable while native images appear
   in the web Space, or must scoped WASM CSP support ship in the same change?
5. Approve the font source/licensing and proposed diff calibration. Does
   "pixel-perfect" require zero non-edge differences on one reference platform?
6. If native dependencies prevent the complete gallery from compiling, approve
   a bounded fixture-only extraction or choose native images for those cases.

## Phase 1 checks and limits

Read repository/scoped instructions and requested docs; inspected pinned crate
sources rather than assuming old GPUI platform limitations. No app was launched,
no real configuration was loaded, and no native/browser screenshots were made.
The permanent Indra worktree and durable session directory remain untouched.
Only the explicitly authorised user-level Rust installation changes host setup.
No full application test suite is warranted for this planning artifact.
