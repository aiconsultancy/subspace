# TSX Spaces: research spike

A research spike on a new experimental Space type, **TSX Spaces**: Spaces
written in TSX with [Native Shell](https://nsh.dev) (nsh), tested and
packaged with the `nsh` toolchain, distributed as signed packages and
loaded by Subspace like plug-ins. Written 26 November 2026 against `main`
at `ac273f49`, on the branch `feat/tsx-spaces`. It informs the
[intent](intent.md). It records what is feasible; it does not choose the
implementation design.

Paths are relative to `subspace-desktop/` unless they start with
`subspace-framework/`. "Space" and "board" mean the same thing.

## Owner decisions during the spike (26 November 2026)

- **nsh is the chosen tool.** Authors write TSX, test and package with
  `nsh`, and publish signed `.nshpkg` packages.
- **Incremental adoption, not a rewrite.** nsh is embedded beside GPUI Kit
  in the existing app. Existing Spaces and the workspace shell stay as
  they are. The owner sees nsh as the successor to GPUI Kit and expects
  Subspace to move towards it over time; TSX Spaces is a first, contained
  step on that path.
- **GPUI Shell is the considered alternative.** GPUI Kit's own JavaScript
  runtime has fewer dependencies but less flexibility. It stays in this
  record as the alternative ([section 10](#10-the-considered-alternative-gpui-shell)).
- **The demo is a system monitor.** The Magnificent 7 stock ticker and the
  market-data comparison are dropped. Question 6 is now a host-granted,
  read-only system metrics capability ([section 8](#8-system-metrics-capability)).
- **sysmon is the primary reference** for the demo
  ([section 9](#9-prior-art-sysmon)).

## Sources

| Source | Version or commit read |
| --- | --- |
| [nsh.dev documentation](https://nsh.dev/llms.txt) (Markdown at `<url>.md`) | Read 26 November 2026 |
| [longbridge/nsh](https://github.com/longbridge/nsh), including `docs/dev/embedding.md` | `main` at `b6eaf385` (workspace 0.7.2, not on crates.io yet) |
| [`nsh-platform-gpui`](https://crates.io/crates/nsh-platform-gpui) | 0.7.1, published 25 November 2026 |
| [GPUI Shell documentation](https://gpui-kit.com/shell.md) | Read 26 November 2026 |
| [`gpui-shell`](https://crates.io/crates/gpui-shell) | 0.9.1 and 0.11.0 manifests |
| [huacnlee/sysmon](https://github.com/huacnlee/sysmon) | `main` at `8defd3c0`, release v0.1.7 |
| [`sysinfo`](https://crates.io/crates/sysinfo) | 0.40.3 (in the lockfiles), 0.40.4 latest |
| `subspace-desktop/Cargo.lock`, `subspace-framework/Cargo.lock` | At `ac273f49` |
| `gpui-kit`, `gpui-component`, `gpui-base`, `gpui-webview` 0.9.1 manifests | Local Cargo registry |

## Summary

**Feasible as an embedded, contained step, with one version blocker and
two build risks.**

- **nsh supports incremental adoption in GPUI Kit apps.** Its comparison
  page says: "NSH's GPUI backend embeds NSH apps in existing GPUI and GPUI
  Kit apps, so screens can move one at a time instead of in a rewrite",
  and advises GPUI Kit users to "embed one NSH panel first and verify its
  document state and keyboard behavior before moving the surrounding
  workspace". An `AppView` is one GPUI view that the host places in its
  own element tree. Longbridge's own GPUI Kit desktop apps embed nsh this
  way.
- **Blocker: nsh needs GPUI Fast; Subspace uses upstream GPUI.**
  `nsh-platform-gpui` 0.7.1 draws with `gpui-fast` ^0.1.19. Subspace uses
  `gpui-pre` 0.3.12 through `gpui-kit` 0.9.1's default backend. nsh's
  embedding guide is explicit: "GPUI Kit … must use it too: its `gpui-fast`
  feature. `cargo tree -i gpui-fast` must show one version, else the app
  has two GPUIs and NSH's views are not the app's elements." Cargo features
  unify, so this switches the whole app build, Framework included, to GPUI
  Fast. It changes no Space's code (imports stay `gpui_kit::*`), but every
  Space runs on a different renderer. [Section 2](#2-version-check-the-blocker)
  lists the ways to resolve it.
- **Risk: two QuickJS engines.** `subspace-charts` links QuickJS NG through
  `rquickjs-sys` 0.14.0; nsh links `quickjs-jit-sys` 0.14.2. Neither sets
  Cargo `links` or prefixes its C symbols. This needs a link test.
- **Risk: release cadence.** `nsh-platform-gpui` went from 0.1.0 to 0.7.1 in
  39 days (11 releases); GPUI Kit from 0.9.1 to 0.11.0 in 14 days. nsh
  promises that packages keep running on later runtimes, but the Rust
  embedding API is 0.x.
- **The plug-in model fits.** Signed `.nshpkg` packages (Ed25519, offline
  root, revocation, rollback protection), a per-view sandbox with limits,
  capabilities declared by the package and narrowed by the host, a host
  channel gated by the `host` capability, host-set theme tokens, headless
  tests with stubbed host calls, and charts.
- **System metrics** come from a read-only host capability in Rust, on
  `sysinfo` 0.40.3, which the Framework already uses. Measured on one Mac:
  1.4 ms of CPU per CPU, memory and network sample; 28 ms per memory-only
  process scan.
- **Prior art.** sysmon is the nsh author's TSX system monitor with a Rust
  collector, MIT licence. Its data model, app aggregation, sampling loop,
  charts and tests are the starting point; its visual design is not.
- **GPUI Shell**, the alternative, runs on Subspace's current GPUI (no
  switch), but is plain JavaScript with builder chains (no TSX), at
  milestone M0, with no signed package format and no plug-in authorisation
  interface yet.

## 1. Adding nsh to an existing GPUI Kit app

### Where nsh documents it

| Page | What it says |
| --- | --- |
| [Comparing approaches › Challenging GPUI](https://nsh.dev/docs/comparison.md) | Screens "can move one at a time instead of in a rewrite"; "Embed one NSH panel first and verify its document state and keyboard behavior before moving the surrounding workspace." |
| [Comparing approaches › Evaluating it: start with a pilot](https://nsh.dev/docs/comparison.md) | "No rewrite is needed to find out … start with one screen": build it in nsh alongside the current one, then compare start time, memory, CPU, frame rate, input, and for delivered UI, signature checks, capability limits and rollback. |
| [Embedding in an existing app](https://nsh.dev/docs/embedding.md) | "An existing native app can show an NSH app in part of its own UI: one screen, one tab, a card under a native toolbar." For a GPUI app: `AppView` (a GPUI view) runs a Rust root, TSX bundles and signed packages, from `nsh-platform-gpui`. |
| [GPUI › Embedding in a GPUI app](https://nsh.dev/docs/gpui.md#embedding-in-a-gpui-app) | The `AppView` API, package loading, the host channel, limits, theme overrides, errors. |
| [`docs/dev/embedding.md` › GPUI and GPUI Kit apps](https://github.com/longbridge/nsh/blob/main/docs/dev/embedding.md) (repository) | The integration rules: one GPUI, mounting, focus, frame rate, overlays, theme hand-off, storage, caches, errors. Names Longbridge Pro and Allsum Desktop as GPUI Kit apps that embed nsh. |
| [`examples/embed-gpui`](https://github.com/longbridge/nsh/tree/main/examples/embed-gpui) | A GPUI Kit window with two embedded TSX order tickets, a host module, GPUI Kit widgets inside the TSX apps, GPUI Kit dialogs over them, shared Tab order, and theme mapping. |

### What it requires

**Crates.**

| Crate | Version | Notes |
| --- | --- | --- |
| `nsh-platform-gpui` | 0.7.1 | `AppView`, `TrustStore`, `verify_package`, `accept_package`. Turn off unused default features (web view, shaders, editors, terminal, maths, emoji, zlib, crypto, WebSocket, updater, SVG). Keep `js`, `net` if packages may `fetch`, and `plot` if they use `nsh/base/plot`. |
| `gpui-kit` | With the `gpui-fast` feature | nsh's example uses 0.11.0. Subspace is on 0.9.1. |
| `gpui-fast`, `gpui-fast-platform` | ^0.1.19 | Must resolve to one version (`cargo tree -i gpui-fast`). |
| `nsh-base` | 0.7.x | `nsh_base::registry()` for `set_ui_sets`. |
| `dev` feature | Development builds only | `connect_dev_server` for hot reload from `nsh dev`. |

**Host set-up, once per process.** Call `set_ui_sets(vec![nsh_base::registry(), …])`
before the first view, with any host UI sets. Open host windows with
`inactive_frame_interval: None`, or GPUI draws unfocused windows at 30 fps
and halves the embedded animations.

**Mounting in the element tree.** `cx.new(|cx| AppView::new(options, window, cx))`
with `options.inline_root = true`. "The host puts the entity wherever it
wants as a child element. The surface fills the space its parent gives it
and follows it when resized." Each view has its own engine thread.
Dropping the last handle, or `AppView::stop`, stops the engine and joins
its thread. For a Space, the content pane of the Framework's workspace
layout would hold the entity; the navigator, toolbar, inspector and status
bar stay GPUI Kit.

**Package loading.** `TrustStore::open(dir)`, `add_root(PublicKey)`, then
off the main thread `verify_package(&trust, &bytes, &packages_dir)`, then
`view.run_package(&package)`, then `accept_package` once the package shows a
frame without a fatal error. `on_app_error` reports `Load`, `Uncaught`,
`Stopped`, `HotUpdate` and `Asset` errors with `is_fatal()` and the package
identity, so the host can fall back to the previous build.

**Capabilities.** Limits per view through `AppOptions::limits` or
`set_limits` (default: 256 MB heap). Grants: iOS and Android `AppView`s
have `set_granted_capabilities`; GPUI's does not. nsh-engine's
`Bundle::policy.granted` is a public field, so a GPUI host can set it on the
verified bundle before loading (source reading, not documented API).
Storage and caches go under per-app folders the host can choose
(`AppOptions::data_dir`, `storage`, `cache`). Notifications stay the host's
unless the host hands them over.

**Theme hand-off.** `set_theme(Some(theme))` or `set_theme_json` replaces
the app's tokens with the host's ("the app renders with its tokens instead
of its own theme's"); `set_appearance` gives light or dark. `embed-gpui`'s
`src/theme.rs` (117 lines) maps GPUI Kit's active `Theme` to shadcn/ui
token names (`background`, `primary`, `muted-foreground`, `border`,
`ring`, radius, `font-sans`, `font-mono`) and runs again on every theme or
mode change through `cx.observe_global::<Theme>`. The surface draws on the
`background` token.

**Focus and overlays.** `AppView` is `Focusable` and a Tab stop. Its key
context `NshAppView` binds Tab and Shift+Tab deeper than GPUI Kit's `Root`,
moves through the app's fields, then hands focus back to the host with
`focus_next` and `focus_prev`. Keys reach the view only while it has focus.
GPUI Kit's dialogs, sheets, popovers and notifications draw above the
surface and take the input.

**What does not change.** Existing Spaces, the workspace shell and
`gpui_kit::*` imports keep their code. The renderer under them changes
(section 2).

## 2. Version check: the blocker

| Crate | Subspace (`Cargo.lock`) | nsh 0.7.1 needs |
| --- | --- | --- |
| `gpui-kit` | 0.9.1, default backend | `gpui-fast` feature; nsh's example uses 0.11.0 |
| GPUI | `gpui-pre` 0.3.12 | `gpui-fast` ^0.1.19 |
| `gpui-fast` | 0.1.15 in the lockfile, not built | 0.1.19 |
| `lb-wry` | 0.53.3 | 0.53.3 |
| `wgpu` | 30.0.1 | 29, only with nsh's `shader` feature |
| QuickJS | `rquickjs` 0.14.0 (QuickJS NG) | `quickjs-jit` 0.14.2 |
| `taffy` | 0.15.1 | 0.16 |

Facts:

- `gpui-kit` 0.9.1 has an optional `gpui-fast` feature (`gpui-fast`
  ^0.1.14). Its crate docs: "Enable `gpui-fast` to select GPUI Fast for the
  core, platform, and enabled layers without changing imports. Upstream
  remains the default backend." Its README: "upstream packages remain build
  dependencies while Kit exposes only Fast types".
- `gpui-component`, `gpui-base` (including the Framework's vendored copy)
  and `gpui-webview` 0.9.1 each have a matching `gpui-fast` feature.
- nsh's embedding guide says nsh "builds with GPUI Fast 0.1.14 or later,
  which GPUI Kit is built on from 0.9.1". The published 0.7.1 manifest is
  stricter: ^0.1.19. With `gpui-kit` 0.9.1 the two requirements unify on
  0.1.19 by semver. No one has built `gpui-component` 0.9.1 against
  `gpui-fast` 0.1.19.
- Subspace chose `gpui-pre`: "Subspace stays on gpui-pre, so neither crate
  enables `gpui-fast`" ([dependency update](../dependency-update/README.md)).
- A second GPUI beside the first is not an option: `AppView` is an
  `Entity` of GPUI Fast, and two GPUI platforms in one process would each
  want the main thread.

Ways to resolve it:

| Path | What it takes | Assessment |
| --- | --- | --- |
| Turn on `gpui-kit/gpui-fast` at 0.9.1 | Product-level feature; full checks of Framework and desktop, web Spaces, terminal, capture builds | Smallest version change; pairing untested |
| Upgrade to `gpui-kit` 0.11.0 with `gpui-fast` | A GPUI Kit upgrade (0.10.0 and 0.11.0) like the [last one](../dependency-update/README.md), then the switch | The pairing nsh tests; larger change |
| Wait for an nsh release on `gpui-pre` | Nothing to do | Unlikely: nsh is built on GPUI Fast, Longbridge's fork, by the same team; no sign it will target `gpui-pre` |
| Keep TSX Spaces in a separate build | Product feature `tsx-spaces` that turns on `gpui-fast`; release builds unchanged | Contains the change while experimental; every Space then runs on two renderers across builds, which doubles testing |

Because Cargo features unify, "off by default" can be a run-time setting
only inside a build that already uses GPUI Fast. A build without the
feature carries no nsh code at all.

## 3. Cost

nsh's measurements ([performance report](https://nsh.dev/docs/performance-report.md)),
not this spike's:

| macOS arm64, LTO, stripped | Executable |
| --- | --- |
| Rust hello world (no QuickJS) | 12.4 MB |
| TSX hello world (QuickJS and nsh's JS modules) | 19.8 MB |

QuickJS and the JS side add about 7.4 MB there. A TSX counter uses about
20 MB of memory standalone on macOS. `nsh-platform-gpui` 0.7.1 alone is
43,733 lines of Rust (crates.io), before `nsh-core`, `nsh-engine`,
`nsh-base` and QuickJS's C sources. GPUI Fast also leaves `gpui-pre` as a
build dependency. Binary size, build time and start-up for Subspace were
not measured; 10–20 MB more on macOS is plausible but unverified.

**QuickJS twice.** `subspace-charts` builds QuickJS NG from `rquickjs-sys`
0.14.0 ([charts licences](../../../licenses/charts/README.md)); nsh builds a
patched QuickJS from `quickjs-jit-sys` 0.14.2. Both are static C builds
with the same un-prefixed API names (`JS_NewRuntime` and the rest), and
neither sets `links`, so Cargo lets both in. The linker either reports
duplicate symbols or resolves some calls to the other engine. A link and
smoke test is needed. If it fails, move `subspace-charts` to
`quickjs-jit`, or compile Flint charts through nsh's engine.

## 4. Plug-in model

### Signed packages

[Signed packages](https://nsh.dev/docs/signed-packages.md):

- **Keys.** Ed25519. An offline **root** certifies **release** keys for app
  ID patterns and a time window (365 days by default). Host apps trust
  roots only; several roots allow rotation. A platform can sign other
  packages again with its own key, so a downstream app can trust only its
  own root.
- **Package.** One `.nshpkg`: a JSON header (app ID, name, version, build,
  declared capabilities, bytecode version, minimum runtime, signing time,
  certificate, file table with SHA-256), the JS source, optional bytecode,
  source map and assets, and the release key's signature.
- **Checks, in order.** Certificate signed by a trusted root; release key
  not revoked; package signature; app ID covered by the certificate at
  signing time; minimum runtime; file hashes; package not revoked; build not
  older than the highest accepted build (rollback protection,
  `nsh-trust.json`).
- **Revocation.** Root-signed lists with a rising sequence number.
- **Bytecode.** Runs only from the app's files, a dev server or a verified
  package. Anything else must be JS source.

nsh verifies and runs packages; it does not deliver them.

### Sandbox

[Sandbox and capabilities](https://nsh.dev/docs/sandbox-and-capabilities.md),
embedded defaults:

| Limit | Default | When hit |
| --- | --- | --- |
| JS heap | 256 MB | VM stops |
| Interpreter stack | 4096 KB | `RangeError` |
| Evaluate the bundle | 10 s | Cut off |
| Any other call into JS | 2 s (CPU time on macOS and Linux, wall time on Windows) | Call cut off, app goes on |
| Nodes | 200,000 | VM stops |
| Queued operations | 8,000,000 values | VM stops |
| Canvas operations per drawing | 1,000,000 | VM stops |

Release builds are hardened: no `eval`, no `Function` constructor, frozen
shared prototypes.

### Capabilities

A package declares capabilities in `nsh.json`; they travel in the signed
header. With `"capabilities"`, it may do only what it lists. **Without it,
it may do everything** except host functions, raw sockets, web-view input
and media, and overlay windows. A host must therefore always set a grant.
Capabilities: `network` (hosts, or method, scheme, port and path rules),
`webview`, `statusBar`, `clipboard`, `picker`, `share`, `speech`,
`notifications`, `audio`, `application`, `homeDir`, `host`, `openUrl`,
`windows`, and `fs` and `process` for the standard modules. Storage needs
none and is per app. A request must pass the declaration and the host's
grant. Refusals reject with `ERR_ACCESS_DENIED`, `permission` and
`resource`.

### How a host adds its own capability

| | Host channel (`host`) | Host UI set (`nativeModule`) |
| --- | --- | --- |
| TSX | `await host.call(name, data)`, `host.on`, `host.post` | `nativeModule<T>("host/name")` |
| Rust | `AppView::host().handle(name, \|call\| …)`, `send`; handlers run on the engine thread | `#[module]`, `#[native_module]`, `set_ui_sets` |
| Gated by nsh | **Yes**: only names in `capabilities.host`, narrowed by the grant; never granted by default | **No**: sets are process-wide and "capabilities do not apply"; the host would check `engine_context` per view itself |
| Data | JSON | Typed values |
| Headless tests | `host.handlers` | `modules` stand-ins |

The host channel is the safer default for a capability that must be off
unless granted to one package.

## 5. Authoring, testing and distribution

[CLI](https://nsh.dev/docs/cli.md), [Testing](https://nsh.dev/docs/testing-and-trace.md):

| Step | Command |
| --- | --- |
| Scaffold | `nsh init my-space --template ts` |
| Run in the GPUI host | `nsh run App.tsx --target gpui`, with `--limits` for embedded limits |
| Check | `nsh check`, `nsh build` |
| Test headless | `nsh test`, stubbing network, storage and host calls; `--target gpui` for a real window |
| Sign | `nsh keygen`, `nsh certify`, `nsh publish App.tsx --key … --cert …` |
| Verify | `nsh verify pkg.nshpkg --root root.pub` |
| Revoke | `nsh revoke --root … --extend list.json --package <sha256>` |

Installation by analogy with Web Spaces
([reference](../../web-spaces-reference.md#install-sources)), which install
from a URL, folder, ZIP, GitHub repository or GitHub release ZIP, review
requested permissions, and bind grants to the folder and exact manifest
bytes. A `.nshpkg` is one file, so a local file, an HTTPS URL and a GitHub
release asset cover it; a folder is for development.

**Developer mode.** Unsigned code can run as JS source (`Bundle::source`),
never as bytecode. `connect_dev_server` (`dev` feature) gives hot reload
and the error overlay.

**Compatibility.** "An app built for a compatible release runs on every
later release, unchanged" ([compatibility](https://nsh.dev/docs/compatibility.md)).
A package names its minimum runtime.

## 6. UI fit

- **Components.** `nsh/base` gives unstyled behaviour and accessible
  structure: buttons, tabs, toolbars, tables, `DataTable`, lists, menus,
  dialogs, `Tree`, `Command`.
- **Charts.** `nsh/base/plot` (`Line`, `Area`, `Bar`, `Rule`, `Grid`…,
  axes, pointer hit-testing, "no look of their own") and `Canvas` with
  native animated values. sysmon uses `Canvas`.
- **Theme.** Tailwind classes against shadcn/ui tokens, set by the host
  (section 1). A package that uses only token classes follows the
  Subspace theme.
- **Design guide.** The [design guide](../../../../subspace-framework/docs/design-guide.md)
  puts the navigator, tab strip, toolbar, inspector and status bar in the
  workspace shell. The host can keep them and give the package the
  content pane. nsh does not stop a package drawing its own heading or edge
  padding; a review checklist has to.
- **Accessibility.** Through AccessKit; "custom actions and the
  context-menu action are not passed on yet".
- **GPUI backend differences.** No `filter: blur()`, `backdropFilter` or
  `mixBlendMode`; opacity per colour; rounded clips do not cut text.

## 7. Platforms and maturity

- **GPUI backend.** macOS (Metal), Windows (DirectX), Linux (Vulkan, with
  Wayland or X11). Linux web views need X11.
- **Subspace.** Ships on macOS; Linux and an experimental Windows build
  exist. Space Lab is macOS ARM64 only.
- **Maturity.** Apache-2.0. 3.5k stars, 262 forks, 2,633 commits, 35 open
  issues and 19 open pull requests on 26 November 2026. 0.1.0 on
  17 October 2026, 0.7.1 on 25 November 2026.

## 8. System metrics capability

### Why the host must provide it

A package runs in QuickJS with no OS access beyond nsh's capabilities, and
nsh has no system metrics capability. "A package holds TSX. The host app's
Rust code … is part of the host app itself." sysmon links its collector
crate into its own runner with full trust. A TSX Space cannot, so Subspace
compiles a collector in and exposes it through gated `host` names.

### Data sources

`sysinfo` (MIT) 0.40.3 is in the Framework's tree: `subspace-shell`
(`subspace-framework/host/app`) uses it with the `system` feature for the
OS version in diagnostics. Features include `system`, `network`, `disk`,
`gpu`, `component` and `user`. Supported: Android, FreeBSD, NetBSD, iOS,
Linux, macOS, Raspberry Pi, Windows. 0.40 constructors return `Result`.

| Fact or metric | `sysinfo` 0.40.3 | Gap or other source |
| --- | --- | --- |
| CPU overall and per core | `global_cpu_usage()`, `cpus()[i].cpu_usage()` | Two refreshes at least 200 ms apart |
| Memory total, used, swap | `refresh_memory`, `total_memory`, `used_memory` | |
| Network up and down | `Networks::refresh`, `received`, `transmitted` | Needs a baseline; exclude loopback |
| Processes by memory | `refresh_processes_specifics(All, true, ProcessRefreshKind::nothing().with_memory())` | Resident memory; summed per app, shared pages count more than once |
| OS, kernel | `name`, `os_version`, `long_os_version`, `kernel_version` | |
| CPU model, cores, architecture | `cpus()[0].brand()`, `physical_core_count`, `cpu_arch` | |
| Uptime | `uptime`, `boot_time` | |
| Host name | `host_name` | Personal data; see privacy |
| GPU model and use | `Gpus` (`gpu` feature) | Checked on macOS only ("Apple M5 Max", use 0.0, no memory total) |
| Machine model | None | macOS `hw.model`, Linux `/sys/class/dmi/id/product_name`, Windows WMI |
| Display and resolution | None | The host's GPUI displays |
| Shell | None | `SHELL` or `ComSpec` |
| App version | None | Product descriptor |

### Measured sampling cost

A throwaway release program (not committed), `sysinfo` 0.40.3, Apple M5
Max, 18 cores, macOS 27.2, 1,316 processes. CPU time per sample over 20
samples taken 250 ms apart:

| Work per sample | CPU per sample | At 1 Hz |
| --- | --- | --- |
| CPU (all cores) + memory + network | 1.43 ms | 0.14 % of one core |
| All processes, memory only | 28.3 ms | 2.8 % of one core |
| All processes, memory + executable path | 30.1 ms | 3.0 % of one core |
| `refresh_all` (sysmon's approach) | 266 ms | 27 % of one core |
| GPUs | 0.12 ms | Negligible |

One machine and one run: orders of magnitude only. Process scans dominate
and grow with the process count. Windows and Linux were not measured.
This favours CPU, memory and network at 1 Hz; a memory-only process scan
every 2–5 s; facts once and on display change; one sampler thread shared by
all granted views, stopped when none is visible; never `refresh_all`.

### Privacy

Process names show what the user runs. Command lines, working folders,
environment variables and executable paths can hold secrets and customer
names. Host and user names identify the person and machine. A proposal for
the spec, one `host` name per tier so each is declared and granted
separately:

| Tier | Contains | Never contains |
| --- | --- | --- |
| Metrics | CPU overall and per core; memory used, total and swap; network rates, combined and per interface without addresses | |
| Facts | OS name and version, kernel, CPU model and cores, GPU model, total memory, uptime, display size and scale, shell name, Subspace version | Host name, user name, serial numbers, MAC or IP addresses |
| Apps | Top N apps by resident memory: display name, memory, process count, opaque per-session ID | PIDs, executable paths, command lines, environment, working folders, user IDs, ports |

Grouping helper processes into apps needs executable paths inside the
host, as sysmon does; only the derived name and the opaque ID leave it.

### Platform coverage

| | macOS | Windows | Linux |
| --- | --- | --- | --- |
| nsh GPUI backend | Yes | Yes | Yes, Vulkan |
| `sysinfo` CPU, memory, network, processes | Yes | Yes | Yes |
| GPU model (`sysinfo` `gpu`) | Checked | Unchecked | Unchecked |
| Measured cost | Above | Not measured | Not measured |

## 9. Prior art: sysmon

[huacnlee/sysmon](https://github.com/huacnlee/sysmon), by Jason Lee, who
also leads GPUI Kit and nsh. "A system monitor build by NSH". v0.1.7 on
25 November 2026.

### What it is

- **nsh and TSX, not GPUI.** `App.tsx` mounts the interface in `src/`.
  Collectors are a Rust nshm crate, `ext/sysmon-native`, imported as
  `sysmon-native`. Desktop builds use nsh's native backends (AppKit, Win32,
  GTK 4), not GPUI. It also builds for iOS and Android, where it reads only
  its own process.
- **Releases.** macOS arm64 and Linux x86_64 archives with `SHA256SUMS`;
  Windows from source.
- **Licence.** MIT, "Copyright (c) 2026 Jason Lee". `assets/LICENSE.nsh` is
  nsh's Apache-2.0 notice for the bundled runtime.

### How it works

- **Collector.** `sysinfo` 0.37.2 (`system`, `disk`, `network`). One sampler
  thread keeps rate baselines (`lib.rs`). `Sysmon.sample()` (async) returns
  a `Reading` as JSON; `extras()` returns slower providers; `device()` and
  `uptime()` are synchronous.
- **Data model** (`sampler.rs`, `src/model.ts`). `Reading { at, cpu, cores,
  memory, total, disk, diskIo, network, gpu, volumes, interfaces, apps,
  workloads }`. `None` means unavailable, never zero. `AppUsage { id, name,
  cpu, memory, processes }`; `id` "contains no executable or bundle path".
  No per-core CPU.
- **App aggregation** (`apps.rs`). Sums processes into apps; on macOS,
  helper bundles count toward the outer `.app`.
- **Extras** (`providers.rs`). Battery, audio, GPU model, Bluetooth, Docker
  and listening ports, mostly by running `system_profiler`, `lspci`,
  PowerShell, `lsof`, `ss` and `docker`.
- **Workloads** (`workloads.rs`). Reads command lines (`cmd()[1]`) and
  working folders to find coding agents and projects.
- **Sampling** (`src/monitor.ts`). Core every 2 s, extras every 30 s, 60
  samples of history in memory, pause, a `Source` interface between native
  module and store.
- **Charts** (`src/chart.tsx`). `Canvas` with native animated values: real
  capture times, gaps for pauses, pointer read-out, per-unit value ranges.
- **Tests.** `tests/*.test.tsx` headless with `nsh/test`, faking
  `sysmon-native` from `tests/fixtures.ts`.
- **Theme.** `theme.json` with shadcn/ui tokens, dark, own gradients.

### What to reuse

| Part | Reuse | Change |
| --- | --- | --- |
| `Reading` and `AppUsage` | Adopt as the capability's JSON contract | Add per-core CPU and facts; drop disk, volumes and workloads unless asked |
| Sampler thread with baselines | Adopt the pattern | Rates per tier; targeted refreshes; shared across views |
| App aggregation (`apps.rs`) | Adapt | Port to `sysinfo` 0.40; expose only the apps tier |
| GPU use (`gpu.rs`) | Compare with `sysinfo` `Gpus` first | Keep only if `sysinfo` falls short |
| Extras and workloads | Do not reuse | Running system tools and reading command lines is too wide for a Space capability |
| `Source` and `monitor.ts` | Adopt in the demo | `host.call` instead of `nativeModule`; the host may push with `host.on` |
| `chart.tsx` | Adapt | Subspace tokens; compare with `nsh/base/plot` |
| Tests and fixtures | Adopt the approach | `host.handlers` instead of `modules` |
| Visual design | Do not reuse | Subspace's design guide decides |

### Licence terms

MIT allows copying and adapting into this Apache-2.0 repository, binaries
included, if "the above copyright notice and this permission notice shall
be included in all copies or substantial portions of the Software". For
adapted code:

- Keep the MIT notice with "Copyright (c) 2026 Jason Lee" in the crate's
  `NOTICE` and beside adapted files, as
  `subspace-framework/services/charts/NOTICE` does for Flint.
- Add the licence text under `licenses/` and to the release notices, as
  `licenses/charts/` does.
- Name the source in each adapted file's module comment.
- Our own changes may be Apache-2.0; the MIT notice stays on the derived
  parts.

Data shapes and sampling rates are ideas, not protected expression, but
credit sysmon as prior art anyway.

### Effect on the capability design

- The capability can return sysmon's shapes, extended with per-core CPU
  and facts, so the demo keeps sysmon's model, store and chart code.
- sysmon's opaque app `id` already matches the apps tier.
- sysmon's module is ungated and fully trusted; the Space capability is
  gated per package and narrower.
- sysmon's 2 s `refresh_all` costs too much beside the rest of Subspace.

## 10. The considered alternative: GPUI Shell

### What "GPUI Kit's shell feature" is

[GPUI Shell](https://gpui-kit.com/shell.md), crate `gpui-shell` (0.9.1
matches Subspace's GPUI Kit; 0.11.0 latest), in the GPUI Kit repository at
`crates/shell`. Its own words: it "exists to make a Rust GPUI application
extensible in JavaScript", "rendered by GPUI itself — no WebView, no DOM",
and "the primary goal is plugin extension".

- **Language.** Plain JavaScript ES modules. Views are classes with
  `init` and `render(cx)`; UI is Rust-style builder chains
  (`v_flex().size_full().child(Button.new("save")…)`) recorded into an
  arena and replayed into GPUI elements. No JSX or TSX; no signals ("Change
  state, then call `cx.notify()`"). A generated `gpui-kit.d.ts` gives
  editor types.
- **Rendering.** On `gpui-base`, beside `gpui-component`. A script View
  sits under a `ShellRoot`, which owns dialogs, sheets, toasts and Tab
  order "the same role `Root` plays for a `gpui-component` window".
  Presentation belongs to the script: `gpui-base` controls have no style.
- **GPUI version.** `gpui-shell` 0.9.1 depends on `gpui-pre` 0.3.12 and
  `gpui-kit` 0.9.1, the same as Subspace, with `gpui-fast` optional. Its
  default `webview` feature turns `gpui-fast` on, so a host would use
  `default-features = false, features = ["quickjs", "components"]`.
- **Engine.** `quickjs-jit` 0.14.1, so the two-QuickJS risk with
  `subspace-charts` applies to it as well.
- **Sandbox.** `Capabilities::default()` is empty; the host grants files,
  storage, clipboard, processes, HTTP, TCP and WebSocket. Heap 256 MiB,
  stack 1 MiB, 50 ms per `render`. No `eval`, frozen prototypes.
- **Host functions.** `HostModule` lends Rust functions to scripts as an
  importable module, with plain-data arguments. "The list is the grant":
  each plug-in's `Policy` carries its own module registry.
- **Distribution.** Dependencies are Git repositories ("shell packages").
  `gpui-shell bundle` packages standalone apps, signed with the platform's
  code signing. There is no signed plug-in package format.
- **Cost.** "+13.5 MiB of binary and +14 MiB of memory" in GPUI Kit's
  measured build.
- **Status.** "Milestone M0: a feasibility baseline, not a stable
  interface … the script API remains experimental." "What is still missing
  is the rest of the contribution registry (`gpui.command`,
  `gpui.keymap`), the authorization UI, and a CLI that uses
  `PluginManager`. What runs end to end today is the standalone path."

nsh's comparison page calls GPUI Shell "the idea NSH grew from".

### Comparison

| | nsh TSX Spaces | GPUI Shell |
| --- | --- | --- |
| GPUI | Needs GPUI Fast (switch or upgrade) | Runs on Subspace's `gpui-pre` |
| Dependencies | nsh core, engine, base, platform crate, QuickJS | `gpui-shell`, QuickJS; reuses `gpui-base` |
| Authoring | TSX, JSX, Tailwind classes, signals, `nsh/base` components | JavaScript builder chains, manual `cx.notify()` |
| Testing | `nsh test` headless with stubs | `gpui-shell check` without a window, `dev` with hot reload |
| Packages | Signed `.nshpkg`, offline root, revocation, rollback | Git repositories; no signing for plug-ins |
| Capabilities | Package declares, host narrows; `host` names gated | Host grants; `HostModule`s granted per plug-in through each `Policy`'s registry ("the list is the grant") |
| Charts | `nsh/base/plot`, `Canvas` | Paths drawn with `PathBuilder` and `window.paint_path`; no chart component found in the script API |
| Platforms beyond desktop | iOS, Android, web, terminal | Desktop |
| Plug-in readiness | Embedding, signing and grants shipped | Authorisation UI and plug-in manager missing |
| Direction | The owner's chosen successor to GPUI Kit | Same team; nsh "grew from" it |

### Why nsh

- **The owner's direction.** The owner sees nsh as GPUI Kit's successor
  and expects Subspace to move towards it. TSX Spaces is the first
  contained step: one Space type, embedded beside GPUI Kit, with no change
  to existing Spaces. GPUI Shell would be a step in a different direction.
- **Plug-in parts exist.** Signed packages, revocation, rollback and
  declared-then-granted capabilities ship in nsh today. GPUI Shell's
  plug-in authorisation and manager do not yet.
- **Authoring.** TSX with signals and `nsh/base` components is closer to
  what Space authors and coding agents already write than builder chains.
- **Cost of the choice.** nsh needs the GPUI Fast switch and more
  dependencies; GPUI Shell needs neither. That is the trade the owner
  accepted.

### Other options considered

GPUI Kit's `RootPlugin` (`gpui-base`) is a compile-time hook for window
roots, not run-time loading.

| Option | Safety | Authoring | Distribution | Effort |
| --- | --- | --- | --- | --- |
| Rust dynamic libraries | Full process trust; no stable ABI | Rust and GPUI Kit | Per-platform binaries per app release | High to maintain |
| WASM components | Strong sandbox | Any WASM language; UI protocol to design | One `.wasm` | High: rebuilds what nsh is |
| Space Lab (JSON and workers) | Data only; MXC workers | JSON plus Python, Node or Bun | Folders | Exists; macOS ARM64 only |
| Web Spaces | Web view, bridge permissions | Any web stack | Folder, ZIP, GitHub | Exists; not GPUI drawing |

## Open items for delivery

1. Spike build with `gpui-kit/gpui-fast`: at 0.9.1, and if that fails,
   after an upgrade to 0.11.0. Run `make check` in both projects, the web
   Space tests and the terminal check. Record which path works.
2. Link test: `rquickjs-sys` and `quickjs-jit-sys` in one binary.
3. Measure binary size, build time, start-up time, memory per open TSX
   Space and first-frame time on macOS, with and without the feature.
4. Ask upstream for a GPUI `AppView::set_granted_capabilities`, or confirm
   `Bundle::policy.granted` is stable.
5. Check `sysinfo` GPU and process coverage, and sampling cost, on Windows
   and Linux.
