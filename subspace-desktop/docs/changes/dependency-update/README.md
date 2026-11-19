# Change: update every Cargo dependency to its latest release

## Intent

Use the latest release of every package in all three Cargo workspaces: the
main workspace and the excluded Terminal workspaces `framework/runtimes/terminal`
and `spaces/terminal`. Raise each direct requirement to its latest release,
including semver-major bumps, refresh the three lockfiles, and migrate the code
to the new APIs instead of keeping compatibility shims.

## Spec

`cargo update` ran in all three workspaces, then every direct requirement was
raised with `cargo upgrade --incompatible --pinned`. Exact (`=`) pins stay
exact at the new version.

### Direct dependencies bumped

| Dependency | Where | Old | New |
| --- | --- | --- | --- |
| `gpui-kit` | workspace, both Terminal crates | `0.7.3` | `0.9.1` |
| `gpui-kit-assets` | workspace | `=0.7.3` | `=0.9.1` |
| `gpui-devtools` | workspace (`devtools` feature) | `=0.5.1` | `=0.6.0` |
| `gpui-wry` → `gpui-webview` | workspace, `spaces/web` | `gpui-wry =0.7.3` | `gpui-webview =0.9.1` |
| `rmcp` | workspace | `3.9` | `3.10` |
| `rusqlite` | workspace | `0.41` | `0.42` |
| `sysinfo` | workspace | `0.31.4` | `0.40.3` |
| `zip` | workspace | `9.1.3` | `9.5.1` |
| `github-copilot-sdk` | `framework/runtimes/agent` | `=1.0.28` | `=1.0.34` |
| `usage-lib` | `host/update`, `host/mcp`, `host/board-command` | `=7.0.1` | `=7.3.0` |
| `fastembed` | `services/search-onnx` | `=7.1.1` | `=7.7.0` |
| `toml` | `spaces/board-lab` | `0.8` | `1.2` |
| `rustix` | `framework/runtimes/terminal` | `1.1.5` | `1.1.6` (matches the workspace pin) |

Every other direct requirement was already at its latest release, including
`lb-wry =0.53.3`, `rustix =1.1.6`, `mermaid-rs-renderer =0.3.1` and
`mxc-sdk =1.0.1`. After the update `cargo update --verbose` reports only five
transitive crates one release behind (`generic-array`, `proc-macro-crate`,
`webkit2gtk`, `wgpu`, `winnow`), held by their dependents' requirements.

### Pins kept

- `libghostty-vt` stays on git rev `d2036bae`. Its comment explains the pin:
  the latest crates.io release (0.2.2) still requires Zig 0.15.2, which cannot
  link on macOS 26 with only the Command Line Tools. The Ghostty commit used by
  `mise run terminal-deps` is tied to the rev.
- `github-copilot-sdk` stays exact, as its comment requires, so the tested SDK
  and its bundled CLI move together. `failure::SDK_VERSION`, README and
  `docs/agent-chat.md` follow the new pin.
- The GPUI Kit group (`gpui-kit-assets`, `gpui-webview`) and `lb-wry` stay
  exact so the asset `links` metadata and the web view stay on one GPUI build.

### GPUI Kit 0.7.3 → 0.9.1

The upgrade crosses 0.8.0, 0.8.1, 0.9.0 and 0.9.1 and moves the default
backend from gpui-pre 0.3.9 to 0.3.12. The breakage was modest, so the upgrade
is done: about 35 compile errors and denied deprecations across 20 files, most
of them from one `KeyContext` signature change, and four failing tests.

- `gpui-wry` is renamed `gpui-webview` (0.8.0). The web board imports
  `gpui_webview::WebView`. Subspace stays on gpui-pre, so neither crate enables
  `gpui-fast`; the native page still covers GPUI content within its bounds.
- `KeyContext::new_with_defaults` takes the host `OperatingSystem`. Every key
  context passes `cx.platform_info().os`.
- `KeyBinding::new` takes `&'static str` and no longer resolves `secondary-`;
  such a binding would panic at startup. The new `subspace_dashboard::secondary!`
  macro expands `secondary!("shift-c")` to `cmd-shift-c` on macOS and
  `ctrl-shift-c` elsewhere. The shell, composer, chat input and native grid
  bindings use it. Dashboard switching keys are static arrays.
- `ScrollWheelEvent`, `on_scroll_wheel` and `PlatformInput::ScrollWheel` are
  renamed `ScrollEvent`, `on_scroll` and `PlatformInput::Scroll`. The capture
  build's scroll step gives `source: ScrollSource::Pointer`.
- Popup menus now skip disabled rows when navigating by keyboard and refuse to
  activate them. The overflow toolbar's guard for the 0.7.3 behaviour is
  removed, along with the `disabled` state it kept for that guard.
- Animations read the executor clock. The chat drop test advances the test clock
  past the panel's reveal before it drops a file.
- The dialog surface's snapshot ID is `surface`, not `0`; the Settings geometry
  test looks the surface up by that ID. The Help dock menu test presses Down
  once, because the disabled current side is skipped.
- The Board Lab grid test uses Kit's new `click_with_modifiers` instead of a
  hand-written Shift-click.
- Comments and docs that describe Kit behaviour, which was checked against the
  0.9.1 sources, now name 0.9.1. The vendored theme catalogue is byte-identical
  to the 0.9.1 `themes/` directory; `PROVENANCE.md` records that.

### Other migrations

- `github-copilot-sdk` 1.0.34: `UserInputHandler::handle` receives the
  runtime's `request_id` (unused; Subspace routes answers by its own IDs), and
  `agent.get_current()` returns `Option<AgentInfo>`. The default agent (`None`)
  reports "The SDK has not confirmed an active custom agent.", the same message
  as an empty name. Before, the non-optional field failed to deserialize and
  reported "Cannot confirm the current SDK agent."
- `sysinfo` 0.40: `System::os_version()` returns `Result`.
- `toml` 1.x, `rusqlite` 0.42, `rmcp` 3.10, `zip` 9.5, `fastembed` 7.7,
  `usage-lib` 7.3 and `gpui-devtools` 0.6 needed no code changes.

### Behaviour changes users may notice

From GPUI Kit, not Subspace code:

- keyboard navigation in popup menus skips disabled entries
- tree rows select on click rather than on mouse-down
- menus size their items and default width in rems
- rendered Markdown tables highlight the hovered row

The Component gallery's three Kit sample cards now show
`gpui-component 0.9.1: …` as their source. No layout or colour of Subspace's
own components changed, so this record has no screenshots.

## Delivery

Checks, run before an unrelated change being made in the same working tree at
the same time:

- `cargo fmt --all` and `make check` pass: architecture, board docs, format,
  cargo-shear, clippy with `-D warnings`, the workspace tests, and the script,
  web-bridge and Weather tests.
- `cargo clippy -p subspace-shell --features devtools,capture --all-targets`
  passes.
- `make interfaces` passes.
- `mise run quality`: findings 0 → 0, score unchanged at 100%.
- `MISE_ENV=terminal mise run check-terminal` with Zig 0.16.0 and the
  prefetched Ghostty dependencies: format, clippy and tests (270) pass for both
  Terminal crates, and `cargo build -p subspace --features terminal` links.
- `cargo build -p subspace` passes.
- The IRC smoke example connected to the local Ergo server, registered and
  joined `#test`. The `ergo` integration test passes.
- Windows and Linux targets were not compiled, because only the macOS target
  is installed.

Audit (`mise run audit`): all three lockfiles report `advisories ok`, with no
new advisories. No ignored advisory was resolved: every one is still reachable
through the latest releases of GPUI Kit, lb-wry or serde_yaml_ng. `deny.toml`
keeps all 13 entries. Their blocking chains now name gpui-kit 0.9.1,
gpui-pre 0.3.12 and gpui-webview 0.9.1, the stale "0.9 is a major UI framework
release" note is gone, and the review date is 2027-02-14.

## Tasks

- [x] `cargo update` in all three workspaces.
- [x] Raise every direct requirement to its latest release.
- [x] Upgrade the GPUI Kit group to 0.9.1 and rename `gpui-wry` to `gpui-webview`.
- [x] Migrate the Copilot SDK, sysinfo and GPUI Kit call sites.
- [x] Remove the overflow toolbar's 0.7.3 disabled-row guard.
- [x] Update the tests that depended on 0.7.3 behaviour.
- [x] Update the version notes in comments, docs and theme provenance.
- [x] Refresh `deny.toml` chains and review dates.
- [x] Run the checks, the Terminal check, the audit and the IRC smoke test.
