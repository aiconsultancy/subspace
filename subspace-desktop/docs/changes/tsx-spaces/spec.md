---
status: accepted
author: Coding agent (Claude Opus 5.5) for the owner
created: "2026-11-26"
approval:
  by: coordinator on the owner's behalf, for owner review
  date: "2026-11-26"
intent: intent.md
work-items: []
---

# Spec: TSX Spaces

Back to [change records](../README.md). Inputs: [intent](intent.md),
[research](research.md). Paths are relative to `subspace-desktop/`.

Requirement IDs (`R-…`) are referenced by the [delivery plan](delivery.md)
and the task files.

## 1. Build

- **R-BUILD-1.** The app product has a Cargo feature `tsx-spaces`, included
  in `unreleased-spaces` and never in `default`. It enables the Space crate
  `subspace-tsx-board` (`spaces/tsx`) and the GPUI Fast backend that nsh
  needs.
- **R-BUILD-2.** A build without `tsx-spaces` has no `nsh-*`, `quickjs-jit*`
  or `gpui-fast` crate in its normal dependency tree
  (`cargo tree -e normal -i <crate>` finds none). Release packaging refuses
  `SUBSPACE_BUILD_FEATURES` naming `tsx-spaces`, as it does for the other
  unreleased Spaces.
- **R-BUILD-3.** In a build with `tsx-spaces`, `cargo tree -i gpui-fast`
  shows one version, and the app links and starts with both `subspace-charts`'
  QuickJS and nsh's QuickJS in the binary (superseded below). A test that runs a script in
  each engine in one process proves it.
- **R-BUILD-4.** The GPUI route is the smallest that passes: `gpui-kit`
  0.9.1 with `gpui-fast` first; an upgrade to the `gpui-kit` version nsh
  tests only if 0.9.1 fails, and then measured against existing Spaces. The
  delivery record names the route taken and why.
- **R-BUILD-5.** Existing checks pass: `make check` in the Framework and the
  desktop, which builds the workspace with `unreleased-spaces` and the app in
  its release configuration.

## 2. The TSX Space

- **R-SPACE-1.** Board ID `tsx`, name **TSX Spaces**, marked experimental.
  Like Space Lab it starts off; the user turns it on in Settings or from
  **Add a Space**.
- **R-SPACE-2.** The Space uses the Framework's workspace layout. The
  navigator lists installed packages by name, with a status (Running,
  Needs grant, Error, Unsigned). The content pane holds the selected
  package's nsh `AppView`. The toolbar has a Reload action on the trailing
  side. The status bar shows the package version and signer state
  (Signed by a trusted root or Unsigned, developer mode).
- **R-SPACE-3.** A package draws only the content pane. It defines no
  navigator rows, toolbar actions or inspector in this version.
- **R-SPACE-4.** Empty, loading and error states use the Framework's shared
  components: no packages installed (with an action that opens
  Settings › TSX Spaces), loading a package, a package that failed to load
  (naming the failing check or the nsh error kind), and a package that needs
  a grant (with an action that opens the grant review).
- **R-SPACE-5.** Theme hand-off: the host maps the active GPUI Kit theme to
  nsh's shadcn/ui tokens (background, foreground, card, muted, primary,
  border, ring, chart colours, radius, fonts) and sets light or dark
  appearance; it does so again on every theme change. It also sets the
  custom token `text-edge` to `workspace::text_edge` in points, which
  packages use for their side inset (`px-(--text-edge)`).
- **R-SPACE-6.** Each running package has its own `AppView` and nsh VM with
  the embedded limits (heap 256 MB unless the package asks for less). A
  fatal nsh error stops that package and shows the error state; other
  Spaces carry on.
- **R-SPACE-7.** The embedded view is one Tab stop in the shell's order
  (nsh's own Tab handling hands focus back).

## 3. Packages, trust and installation

- **R-PKG-1.** Packages are `.nshpkg` files. Installed packages and nsh's
  trust records live under the app's data folder in `tsx-spaces/`.
- **R-PKG-2.** Install from a local file (file picker in Settings). Install
  from an HTTPS URL and from a GitHub release asset URL are optional for
  this delivery; when built, they download to a temporary file and use the
  same path. Plain HTTP is refused.
- **R-PKG-3.** Verification runs off the UI thread with nsh's
  `verify_package` against the trusted roots: root certificate, revocation,
  signature, app ID, minimum runtime, file hashes, rollback. A failure
  refuses the install, names the failing check and keeps nothing from the
  package.
- **R-PKG-4.** No signing root is trusted by default. Settings › TSX Spaces
  lists trusted roots and adds one from a public key (pasted text or file),
  showing its fingerprint; it can remove one. Removing a root stops packages
  signed under it on next load and marks them Error.
- **R-PKG-5.** Developer mode, off by default, in Settings. When on, the
  user may install an unsigned package or a built JS bundle; it runs as JS
  source only (never bytecode), is marked Unsigned everywhere and still
  needs grants. Turning developer mode off stops unsigned packages.
- **R-PKG-6.** Remove: Settings removes a package, its grants and its nsh
  data folder.
- **R-PKG-7.** Updating a package with the same app ID replaces it after
  verification (rollback protection applies).

## 4. Capabilities and grants

- **R-CAP-1.** The host exposes three host channel names:
  `subspace.system.metrics`, `subspace.system.facts` and
  `subspace.system.apps`. A package declares the names it needs in
  `capabilities.host` in `nsh.json`.
- **R-CAP-2.** Before first run, and whenever the declared set grows, the
  user reviews the declared capabilities in plain words and grants any
  subset. Undeclared or unknown capabilities are never granted. The host
  always sets an explicit grant on the bundle (`Bundle::policy.granted`),
  so a package without a declaration gets nothing beyond the nsh defaults
  minus every host name.
- **R-CAP-3.** A grant is stored per app ID and bound to the release key
  fingerprint (or `unsigned`) and the declared set. A different key or a
  larger declared set asks again; an update with the same key and set
  keeps the grant.
- **R-CAP-4.** A call to an ungranted name is refused by nsh
  (`ERR_ACCESS_DENIED`) and also by the host handler. The package shows an
  explained empty state for that tier.
- **R-CAP-5.** Revoking a grant in Settings takes effect at once: the host
  stops serving and pushing that tier to the package, and reloads it with
  the narrower grant.

## 5. System capability

- **R-SYS-1.** Crate `subspace-tsx-system` in `spaces/tsx/crates/system`,
  read-only, on `sysinfo` (workspace version). No process control and no
  system tools are run.
- **R-SYS-2.** Metrics (`subspace.system.metrics`), JSON:
  `{ at, cpu, cores[], memory: { used, total, swapUsed, swapTotal },
  network: { rx, tx } }`; CPU in percent, memory in bytes, network in bytes
  per second over non-loopback interfaces. `null` means unavailable, never
  zero; `cores` is empty while `cpu` is `null`, and network rates are
  `null` until a second sample gives a baseline. `at` is milliseconds
  since the Unix epoch in every tier. Sampled at 1 Hz.
- **R-SYS-3.** Apps (`subspace.system.apps`): top 10 apps by resident
  memory, `{ at, apps: [{ id, name, memory, processes }] }`. Processes
  group into apps as sysmon does (helper bundles into the outer `.app` on
  macOS). `id` is opaque and stable only for the app session. Sampled
  every 2 s with a memory-only process refresh.
- **R-SYS-4.** Facts (`subspace.system.facts`), read on load: OS name and
  version, kernel, architecture, machine model, CPU model, physical and
  logical cores, GPU model, total memory, uptime, displays (size and
  scale), shell name (file name only), Subspace version. Each fact the
  platform cannot give is `null`. JSON: `{ at, os, osVersion, kernel,
  architecture, machineModel, cpuModel, physicalCores, logicalCores,
  gpuModel, memoryTotal, uptime, displays: [{ width, height, scale }],
  shell, subspaceVersion }`; memory in bytes, uptime in seconds. The host
  supplies `displays` and `subspaceVersion` (`FactsInput`).
- **R-SYS-5.** Privacy: no output contains a PID, command line, environment
  variable, executable or working folder path, user ID, host name, user
  name, serial number, MAC or IP address. Unit tests feed values that hold
  such data and assert it does not appear.
- **R-SYS-6.** One shared sampler thread serves all views. It samples only
  the tiers some visible, granted view needs, and stops when no granted
  TSX Space is visible. Readings stay in memory.
- **R-SYS-7.** Delivery: `host.call(name)` returns the latest reading
  (sampling once if none); the host also pushes each new reading with
  `host.send` on the same name to views granted that tier. A call for a
  tier the view is not granted fails with `ERR_ACCESS_DENIED` where nsh
  gates the name, or else the handler rejects it with the exact message
  `not granted`; packages treat both as a refusal.

## 6. Demo package: system monitor

- **R-DEMO-1.** Source in `tsx-spaces/system-monitor/` (desktop root): TSX,
  `nsh.json` with app ID `dev.subspace.system-monitor` and the three host
  capabilities, nothing else declared.
- **R-DEMO-2.** It shows CPU overall and per core, memory, and network up
  and down, each with a 60-sample graph; the top apps by memory; and a
  facts panel. It states the sample rate and time of the last sample, and
  shows an explained empty state for any tier not granted.
- **R-DEMO-3.** It uses only theme token classes so it follows the
  Subspace theme, draws no heading of its own and keeps the design guide's
  edge (`text_edge`) spacing.
- **R-DEMO-4.** `nsh test` runs headless tests with stubbed host calls:
  rendering with data, with each tier refused, and history trimming.
- **R-DEMO-5.** A script (`scripts/tsx-package.sh`, also a mise task)
  generates a local development root and release key under a gitignored
  folder if absent, certifies the key, and publishes a signed `.nshpkg`.
  No private key is committed.
- **R-DEMO-6.** Code adapted from sysmon names its source in a file
  comment and keeps sysmon's MIT notice ("Copyright (c) 2026 Jason Lee")
  in the package's `NOTICE`, under `licenses/` and in the release notices.

## 7. Settings and help

- **R-SET-1.** Settings › TSX Spaces (only in builds with the feature):
  installed packages (name, version, signer, granted capabilities, Remove),
  Install from file, trusted roots (add, remove, fingerprint), developer
  mode switch. Switches use `toggle::toggle_row`; changes follow the
  Settings Save boundary, except install, remove and grant review, which
  apply at once with confirmation.
- **R-SET-2.** A help guide for the Space (`spaces/tsx/help.md`), linked
  from the documentation index: what TSX Spaces are, installing, trust,
  grants, developer mode, and the authoring loop from `nsh init` to a
  signed package.

## 8. Platforms

- **R-PLAT-1.** macOS is required. Windows and Linux should keep compiling
  with the feature off; with it on they are best effort and recorded.

## Acceptance

The delivery is accepted when R-BUILD-1 to 5 pass, the demo installs from a
signed file in an isolated debug build, the grant review appears, and the
running Space shows live data, with screenshots in `evidence/` in Twilight
Two light and dark, and the capability's privacy tests pass.

## Amendments during delivery (27 November 2026)

Decided by the implementing agent under the coordinator's delegation, for
owner review.

- **R-BUILD-2 and R-BUILD-4, superseded.** The owner asked to move the whole
  app to GPUI Fast. Every build now uses GPUI Kit 0.11.0 with `gpui-fast`
  and links one QuickJS (`quickjs-jit`, see below). A build without
  `tsx-spaces` still has no `nsh-*` crate. See the
  [delivery results](delivery.md#results).
- **R-BUILD-3.** Two QuickJS copies link without an error but resolve to one
  set of C symbols, so `subspace-charts` now uses `quickjs-jit` (nsh's fork,
  imported as `rquickjs`) in every build.
- **R-PKG-2 and R-SET-1.** Install, Reload and Remove are in the Space's
  toolbar and empty state, not in Settings. Settings holds the trusted roots
  and the grants, under its Save boundary, through the board field source
  that web Spaces use. Installing from a URL is not built.
- **R-PKG-1.** Packages, nsh's trust records, unpacked assets, each app's
  storage and the install records live beside the config file in `tsx/`.
  Storage is kept per app ID and release key, so a build signed by another
  publisher starts with nothing.
- **R-PKG-3, source only (review finding).** QuickJS does not validate
  bytecode, and a signature says who published a package, not that its
  bytecode is safe to load. A package runs from its verified JS source:
  verification asks nsh for bytecode of a version no runtime compiles for,
  so package bytecode is never read, and the bundle is built from source.
  A test installs a signed package that carries bytecode and checks the
  bundle is source.
- **R-PKG-5.** Developer mode is not built. Only signed packages run.
- **R-PKG-6.** Remove deletes the package, its unpacked assets, its storage
  and its install record. Grants are bound to the install, so a package
  installed again asks for its capabilities again; the old grant keys stay
  in the config file, unused.
- **R-PKG-7.** A replacement is staged beside the installed package and
  renamed over it, under one process-wide lock with the trust records, so a
  failure leaves the installed build whole and concurrent accepts keep every
  build record. A failure to record a build is shown.
- **R-CAP-5.** A changed grant takes effect when Settings closes, whether or
  not the Space shows, when the Space next shows, or on Reload: the Space
  reads Settings again, drops any older read, and restarts the package
  unless it already runs the same package hash with the same grants. A
  package that no longer verifies stops.
- **R-SET-1.** Settings builds its page from the inventory the Space writes
  after verifying packages off the UI thread; it verifies nothing itself.
- **R-SYS-3.** An app's opaque ID is a keyed hash with a random key for the
  process: stable while the app runs, different in the next session.
- **R-SYS-7.** Host calls are answered on a worker thread, not the nsh
  engine thread. Only a call's reply is queued, never its data, and at most
  64 calls wait: a package that calls faster is answered `busy` at once,
  and posts to the capability are dropped.
- **R-PKG-8 (re-review).** An install writes its install record before it
  renames the package into place and puts the earlier record back if the
  package cannot be saved. Remove cleans up by the app ID the store named
  the file after, so a package that no longer verifies loses its storage
  and grants too. A package found without an install record gets one.
  Settings' inventory is written only by the latest refresh, staged and
  renamed under the store lock.
- **R-PKG-9 (third review).** Each app has one file in the store, named
  after its verified app ID; another file holding the same app is refused.
  Remove deletes an app's assets, storage and install only for that file,
  and only by the app ID the package verified as or, if it no longer
  verifies, an app ID the install records hold. An ID used for a recursive
  deletion must be a single folder name. Any other file is removed alone.
  A copy refused under another app's name is removed alone too: its
  verified app ID, not its name, decides (round 4).
- **R-PKG-10 (round 5), amends R-PKG-6 and R-PKG-9.** Remove cleans up an
  app's assets, storage and install only by an app ID a trusted root
  authenticated: the package verified, or was refused only as an older
  build (nsh checks the signature before the build). A package that does
  not verify at all, such as one whose root is no longer trusted, loses
  only its file; its storage and install record stay until it is removed
  from a trusted state. The install records no longer decide a deletion.
  Reason: no identity anyone could forge, such as a file name or a header
  read without trust, is ever used for a recursive deletion.
- **R-SYS-8 (third review).** An ungranted call is refused on the engine
  thread before the queue, so it cannot crowd out granted calls.
- **R-DEMO-5 and R-DEMO-6.** `mise run tsx-package` also writes a zip
  archive with the package, its `NOTICE` and the sysmon licence, and checks
  that it holds them. Another package's archive holds its own `NOTICE`
  and `licenses/` files, whichever it has. Release archives ship `licenses/sysmon/`, which
  the app's `NOTICE` lists.
