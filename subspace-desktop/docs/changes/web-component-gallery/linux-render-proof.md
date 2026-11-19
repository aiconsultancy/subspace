# Linux headless render spike

## Resolved on 18 November 2026

After Jack's approval in message `1795020911.188469`, the captain installed
Mesa Vulkan drivers 26.0.8, Vulkan loader and XCB/xkbcommon development
libraries. The direct pinned renderer now initialises. Using only
`VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.json`, the fixture test
renders action buttons, inputs and error boxes in both approved themes.
All six PNGs are byte-identical on an independent repeat capture.

The test needed two corrections revealed by execution: initialise Inter and
JetBrainsMono Nerd Font before GPUI probes its virtual default system font,
and use the test display's real 2× scale instead of assuming 1×. The viewport
is 800 × 600 logical pixels and each PNG is 1600 × 1200 pixels. No production
component or theme was changed to obtain these images.

Current logs: `.local/feasibility/gpui-lavapipe-renderer-probe.log`,
`linux-lavapipe-gallery-test.log` and `linux-lavapipe-repeat.log`.
The earlier blocked result below remains as historical evidence only.

The captain separately supplied OpenSSL development files and pkg-config
after the normal full-repository gate found those missing prerequisites.
No further system packages were installed by the worker.

18 November 2026. Slice 1 of the web component gallery work. The approved decisions are in
[plan.md](plan.md). This records observed prerequisites, not image parity.

## Historical result before provisioning

**Blocked on this host.** A standalone program invoked the exact pinned
`gpui-pre-wgpu` 0.3.12 `WgpuHeadlessRenderer::new()`. It compiled and ran,
returned `No usable headless GPU adapter found`, and exited 2. The program
depends on the renderer directly, so the result does not depend on the
desktop's missing X11 linker libraries. No reference PNGs exist.

The full gallery image test type-checks successfully. Attempting to run its
renderer probe stopped at linking, because the host lacks linkable `xcb`,
`xkbcommon` and `xkbcommon-x11` libraries. Runtime `.so.1`/`.so.0` files exist
for the first two; the xkbcommon X11 runtime is absent. No stubs or fake
libraries were used to bypass these requirements.

Stopped before the web scaffold, Chromium capture, image diff, visual review,
commit, push or draft PR. Those steps are conditional on reference PNGs.

## Existing APIs

The pinned upstream crates contain a real offscreen image path:

- `gpui-pre-platform` 0.3.12, `src/gpui_platform.rs`, exports
  `current_headless_renderer()` under `test-support` and returns Linux's
  `WgpuHeadlessRenderer`.
- `gpui-pre-wgpu` 0.3.12, `src/wgpu_renderer.rs`, implements
  `PlatformHeadlessRenderer::render_scene_to_image` and GPU readback.
- `gpui-pre` 0.3.12, `src/platform/test/window.rs`, attaches that renderer to
  test windows. `src/window.rs::Window::render_to_image()` returns an RGBA image
  from the rendered scene. Without a renderer the test platform cannot supply
  real component pixels.
- `gpui-pre-wgpu/src/wgpu_context.rs::create_headless` enumerates Vulkan and GL
  adapters and fails with `No usable headless GPU adapter found` when none work.

This supports the requested approach in principle. It does not establish that
the current host has a working renderer. Xvfb supplies windowing, not a missing
Vulkan ICD or EGL implementation.

An opt-in integration test was retained at
`subspace-framework/spaces/component-gallery/tests/linux_offscreen.rs`, with
instructions in its adjacent `README.md`. A separate scratch probe invoked the
public renderer factory directly. The capture test uses the real gallery's action
button, input and error-box cards in both themes. It checks card bounds,
nonblank pixels and lossless PNG readback. It does not use mock raster output
and is not enabled in normal test runs because it requires a graphics adapter.

## Host checks

- Architecture: aarch64 Linux. No DISPLAY or WAYLAND_DISPLAY.
- Rust 1.99.0 matches the repository pin.
- Native gallery `cargo check --features test-support --locked` passed after
  using the dependency's supported `RUST_FONTCONFIG_DLOPEN=1` switch. The first
  attempt failed because this host has no `pkg-config` executable. The switch
  uses the already-installed `libfontconfig.so.1`; it does not disable rendering
  or replace text with a mock.
- Vulkan's loader library exists, but direct `vkCreateInstance` returned
  `-9`, `VK_ERROR_INCOMPATIBLE_DRIVER`. `/etc/vulkan/icd.d` is empty and
  `/usr/share/vulkan/icd.d` is absent.
- Neither `libEGL.so.1` nor `libGLESv2.so.2` can be loaded.
- No apt, sudo, driver installation, permission change or service change ran.

Local evidence: `.local/feasibility/gallery-linux-check.log`,
`gallery-linux-dlopen-check.log`, `linux-renderer-prerequisites.txt` and
`font-sha256.txt`. Cargo build output is under
`~/.cache/component-gallery-cargo-target`, not `/tmp`.

Final executable checks:

| Check | Result | Log |
| --- | --- | --- |
| Native gallery check with test-support | Passed | `.local/feasibility/gallery-linux-dlopen-check.log` |
| New Linux image integration test, cargo check | Passed | `.local/feasibility/linux-offscreen-compile-check.log` |
| Full gallery renderer test execution | Link failed, exit 101; test did not run | `.local/feasibility/linux-offscreen-renderer-test.log` |
| Direct pinned WGPU renderer construction | Ran, exit 2, no usable adapter | `.local/feasibility/gpui-direct-renderer-probe.log` |
| Rustfmt, Framework documentation-index links, diff whitespace | Passed | Direct local commands |

The direct probe source and lockfile remain under
`.local/feasibility/renderer-probe/`. Reproduce from the repository root:

```sh
CARGO_TARGET_DIR="$HOME/.cache/component-gallery-cargo-target" \
mise exec rust@1.99.0 -- cargo run \
  --manifest-path .local/feasibility/renderer-probe/Cargo.toml --offline
```

These are focused spike checks. Full `make check` and `mise run quality` did
not run, and no PR readiness or successful capture is claimed.

## Authorised fonts

Installed under `~/.local/share/fonts/component-gallery/` and refreshed with `fc-cache`.
`fc-match` resolves both exact requested families.

| Font | Release | File SHA-256 |
| --- | --- | --- |
| Inter Regular | rsms/inter v4.1 | `40d692fce188e4471e2b3cba937be967878f631ad3ebbbdcd587687c7ebe0c82` |
| JetBrainsMono Nerd Font Regular | ryanoasis/nerd-fonts v3.5.1 | `1c680e8cde9fcf8b88a5605ce8d1fb94dd3fb15841f7ca7bf4c55664855e5611` |

At the blocked checkpoint, no fonts had been added to the repository. The
subsequent web bundle includes the exact Inter TTF and a lossless full-font
WOFF2 of the Nerd font, with their licences and glyph-source notices.

## If no adapter is available

Stop before the web scaffold. Ask the owner to choose one Linux repair:

1. Authorise installation of an ARM64 software Vulkan implementation such as
   Mesa Lavapipe, with a matching ICD and its dependencies. Prefer a reversible
   user-local installation if available; do not invoke apt/sudo under this task.
2. Supply an existing isolated Linux render runner with a usable Vulkan/GL
   adapter and the same font files. This changes the agreed reference host and
   needs owner approval.
3. Authorise an EGL/GLES software stack and, only if it requires windowing,
   an isolated Xvfb or headless Wayland session.

Re-run the exact GPUI image test after provisioning. Do not treat a successful
Vulkan enumeration or layout-only GPUI test as a substitute for nonblank PNGs.
No React-only screenshot can satisfy the GPUI reference requirement.
