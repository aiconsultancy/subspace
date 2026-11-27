---
status: accepted
author: Coding agent (Claude Opus 5.5) for the owner, research spike
created: "2026-11-26"
approval:
  by: coordinator on the owner's behalf, for owner review
  date: "2026-11-26"
work-items: []
---

# Intent: TSX Spaces

Back to [change records](../README.md). Evidence: [research](research.md).

## Problem and reason to act

The owner's request, 26 November 2026:

> Add support for plugin boards using either the gpui-kit shell features or
> even better https://nsh.dev. I would like to call this new experimental
> space 'TSX Spaces'. We should be able to create Spaces using TSX and nsh
> and distribute them and load them like plugins.

Today a new native Space is Rust code compiled into the app. Web Spaces load
at run time but draw in a web view, not with the app's renderer. Space Lab
builds native dashboards from JSON and workers, on macOS ARM64 only. No path
lets someone write a native Space in a few files, sign it and give it to
another user without a new app release.

**Reason to act now.** The owner sees nsh as the successor to GPUI Kit and
expects Subspace to move towards it over time. TSX Spaces is a contained
first step on that path: one new Space type, embedded beside GPUI Kit, that
shows what nsh costs and gives in this app before anything else moves. nsh
documents this route for GPUI Kit apps: "screens can move one at a time
instead of in a rewrite", and "embed one NSH panel first … before moving
the surrounding workspace" ([research §1](research.md#1-adding-nsh-to-an-existing-gpui-kit-app)).

## Owner direction (26 November 2026)

- **Name.** TSX Spaces, an experimental Space type.
- **Tool.** nsh. Authors write TSX, test and package with the `nsh`
  toolchain, and distribute signed `.nshpkg` packages that Subspace loads
  like plug-ins.
- **Incremental adoption.** nsh is embedded alongside GPUI Kit in the
  existing app. No rewrite of existing Spaces or the shell.
- **Alternative considered.** GPUI Kit's shell feature, GPUI Shell (plain
  JavaScript, no TSX): fewer dependencies, less flexible, and not the
  direction the owner wants. Recorded in
  [research §10](research.md#10-the-considered-alternative-gpui-shell).
- **Demo.** A system monitor, not the stock ticker first suggested:
  real-time CPU (overall and per core), memory and network throughput with
  live graphs; the apps using the most memory; and a fastfetch-style panel
  of system facts. The market-data demo and its research are dropped.
- **Prior art.** [huacnlee/sysmon](https://github.com/huacnlee/sysmon) by the
  author of GPUI Kit and nsh is the primary reference for the demo
  ([research §9](research.md#9-prior-art-sysmon)).

## Purpose

1. Let people build native Spaces in TSX, ship them as signed packages and
   load them into Subspace without an app release.
2. Take **a contained first step towards nsh**: prove the embedding, its
   costs and its security model in one Space type, with every existing Space
   untouched.
3. Show the model with a demo that needs a host-granted capability: a live
   system monitor.

## Affected users and systems

- **Space authors**, people and coding agents: write TSX, run `nsh test`
  headless, sign with `nsh publish`.
- **App users**: install a package, review and grant its capabilities, use
  the Space, revoke or remove it.
- **Maintainers**: a new dependency family (nsh, GPUI Fast, a second
  QuickJS), a capability with privacy weight, and more build combinations.
- **Downstream apps** that vendor this repository: they inherit the build
  options and must be able to leave TSX Spaces out entirely or trust only
  their own signing roots.
- **Systems**: the desktop product, the Framework's workspace shell (as
  host), Settings, release notices and licences.

## Desired outcome

An experimental TSX Spaces build of Subspace in which a user installs a
signed TSX package from Settings, sees what it asks for, grants it, and gets
a native Space drawn by GPUI inside the normal workspace shell. The first
package is the system monitor, which reads live metrics only through a
read-only capability the user granted to that package.

## Success measures

Numbers marked *budget* are set in the spec, from the baseline measured in
the first delivery task.

1. **No regression from the nsh dependency.** In the TSX Spaces build, every
   existing check passes in both projects (Framework and desktop
   `make check`, web Space tests, terminal check), and existing Spaces look
   and behave as before. Build time, binary size, start-up time and idle
   memory stay within budgets. A build without TSX Spaces has no nsh crate
   in its dependency tree.
2. **Signed install.** The system monitor installs as a signed `.nshpkg`
   from Settings. A tampered, revoked, unsigned or older-build package is
   refused with the failing check named, and nothing from it runs.
3. **Granted-capability prompt.** Before first use, Settings lists each
   capability the package declares, in plain words. Nothing is sampled
   before a grant. A denied tier shows an explained empty state. Revoking a
   grant stops the data at once.
4. **Live metrics at a stated rate.** CPU overall and per core, memory and
   network up and down update at 1 Hz (proposed), with graphs of the last
   60 seconds. The Space states the rate and the time of the last sample.
5. **Top memory apps.** The top 10 apps (proposed) by resident memory, with
   name, memory and process count, refreshing every 2 seconds (proposed).
6. **System facts.** A fastfetch-style panel: OS name and version, kernel,
   machine model, CPU model and core counts, GPU where available, total
   memory, uptime, displays and resolution, shell, Subspace version. Facts
   the platform cannot give are shown as unavailable, not left out.
7. **Design guide.** The Space passes the Framework's design checklist in
   both light and dark Twilight Two, follows theme changes live, and the
   embedded content keeps Tab order with the shell.
8. **Cost of the demo.** Time to first frame, memory per open TSX Space and
   sampler CPU stay within budgets. Measured reference: 1.4 ms of CPU per
   metrics sample and 28 ms per process scan on one Mac
   ([research §8](research.md#measured-sampling-cost)).
9. **Authoring loop.** The demo's headless `nsh test` suite passes with
   stubbed host calls, and the authoring guide takes a new author from
   `nsh init` to an installed signed package.

## Constraints

- **No rewrite.** Existing Spaces and the workspace shell keep their code.
  nsh is embedded alongside GPUI Kit, in the content pane of one Space type.
- **One GPUI (blocker).** nsh 0.7.1 draws with GPUI Fast (`gpui-fast`
  ^0.1.19). Subspace uses upstream GPUI (`gpui-pre` 0.3.12) through
  `gpui-kit` 0.9.1. nsh requires GPUI Kit's `gpui-fast` feature so that both
  share one GPUI, and Cargo applies that feature to the whole build. The
  routes: turn the feature on at `gpui-kit` 0.9.1 (semver-compatible,
  untested); upgrade to `gpui-kit` 0.11.0, the pairing nsh tests; or wait
  for nsh on `gpui-pre`, which is unlikely. Until a spike build passes the
  full checks, TSX Spaces is not feasible in this repository
  ([research §2](research.md#2-version-check-the-blocker)).
- **Experimental and off by default.** A build option leaves TSX Spaces
  out, as `unreleased-spaces` does for Space Lab. Inside a build that has
  it, TSX Spaces starts off and no package runs until the user installs and
  grants it.
- **Sandboxed.** Every package runs in its own nsh VM with limits. The host
  always sets a grant, because a package without a capabilities
  declaration may otherwise do everything.
- **Signed packages.** Packages verify against trusted roots, revocation
  lists and rollback records. Unsigned code runs only in an explicit
  developer mode, as JS source, never as bytecode.
- **Read-only, narrow system capability.** No process control, command
  lines, environment, paths, working folders, user IDs, host name, user
  name or network addresses. Off unless the user grants it to one package.
- **Subspace API rule.** TSX Spaces and the system capability need no
  Subspace API and must not take a dependency on it. If a later capability
  needs the API, it lives in the product or a Space and turns itself off
  without it ([working rules](../../../../AGENTS.md#working-rules)).
- **Design guide.** The shell zones stay GPUI Kit. The package draws only
  its content, on Subspace's theme tokens.
- **Public repository.** No private keys or private signing roots. The demo
  may ship a public root key only.
- **Licences.** nsh is Apache-2.0. Code adapted from sysmon keeps its MIT
  notice, "Copyright (c) 2026 Jason Lee", in the crate `NOTICE`, under
  `licenses/` and in the release notices.
- **Platforms.** nsh's GPUI backend supports macOS, Windows and Linux with
  Vulkan. macOS comes first; Windows and Linux follow once measured.
- **Release Please** owns versions and changelogs.

## Exclusions

- A store, marketplace, catalogue or update service for packages.
- Mobile, web and terminal targets.
- Moving any existing Space or the shell to nsh.
- Rust code inside packages (nshm crates): packages hold TSX only.
- In the demo: disk, Docker, Bluetooth, battery, listening ports, coding
  agent workloads, and anything that runs system tools.
- Remote telemetry. Readings stay on the device and in memory.
- GPUI Shell as an implementation.

## Trust and data flow

```text
Author's machine
  App.tsx + nsh.json (declares capabilities)
     │  nsh check · nsh test (headless, host calls stubbed)
     ▼
  nsh publish ── release key certified by an offline root key
     │
     │  .nshpkg: file, HTTPS URL or GitHub release asset
     ▼
Subspace, TSX Spaces build
  Settings › TSX Spaces › install
     │
     ▼
  Verify off the UI thread ── a check fails ──► refuse; nothing runs
  root · revocations · signature · app ID ·
  minimum runtime · file hashes · build number
     │
     ▼
  Grant prompt: the user grants some, none or all declared capabilities
     │  grant is at most the declaration; kept per app ID and signing key
     ▼
  TSX Space
  ┌──────────────────────────────────────────┐
  │ Workspace shell (GPUI Kit, unchanged)    │
  │ navigator · toolbar · status bar         │
  │ ┌──────────────────────────────────────┐ │
  │ │ Content pane: nsh AppView            │ │
  │ │ QuickJS VM · limits · Subspace theme │ │
  │ │ host.call("subspace.system.*") ──────┼─┼──► System capability (Rust)
  │ └──────────────────────────────────────┘ │    sysinfo · read-only
  └──────────────────────────────────────────┘    one shared sampler thread
```

## Evidence

From the [research](research.md):

- nsh documents embedding in GPUI Kit apps: `nsh-platform-gpui`'s `AppView`
  is a GPUI view placed in the host's element tree; the host sets limits,
  grants, theme tokens and appearance; Tab order and GPUI Kit overlays work
  across the boundary (§1).
- The GPUI version mismatch and its routes (§2). A second QuickJS beside
  `subspace-charts`' QuickJS NG is a link risk to test (§3).
- Signed packages, the sandbox's limits and capabilities, and the `host`
  channel as a gated way to expose a host capability (§4).
- `sysinfo` 0.40.3 is already in the Framework and covers the demo's
  metrics and most facts; machine model, displays and shell come from
  elsewhere (§8).
- sysmon's data model, aggregation, sampling loop, charts and tests are
  reusable under MIT; its extras, workloads and visual design are not (§9).
- GPUI Shell runs on Subspace's current GPUI but is JavaScript only, at
  milestone M0, with no plug-in signing or authorisation interface (§10).

## Open questions and proposed defaults

Accepted on 26 November 2026 by the coordinator on the owner's behalf, for
owner review: every proposed default below is the decision. Install from an
HTTPS URL or a GitHub release asset (13) is in scope only if time allows; a
local file comes first.

| # | Question | Proposed default |
| --- | --- | --- |
| 1 | Which route resolves the GPUI mismatch? | Spike `gpui-kit/gpui-fast` at 0.9.1 first; if it fails, upgrade to 0.11.0 with `gpui-fast`. Do not wait for nsh. |
| 2 | Does GPUI Fast stay limited to TSX Spaces builds, or become the default for every build? | A product feature `tsx-spaces` within `unreleased-spaces`; release builds unchanged. Making GPUI Fast the default is a separate owner decision. |
| 3 | Where does the code live? | `subspace-desktop/spaces/tsx`, with the capability in `spaces/tsx/crates/`. Nothing in the Framework until a second user appears. |
| 4 | What does the package draw? | The content pane only. The navigator header shows the package name; no package-defined navigator rows, toolbar actions or inspector in the first version. |
| 5 | Which signing roots are trusted? | None by default. The user adds root public keys in Settings. The demo's root public key ships with the example; its secret key stays outside the repository. Downstream apps supply their roots at build time. |
| 6 | Developer mode? | Off. When on: install from a folder as JS source and connect to `nsh dev` on loopback, marked Unsigned; grants still required. |
| 7 | Capability names and tiers? | `subspace.system.metrics`, `subspace.system.facts`, `subspace.system.apps`, each declared and granted separately. |
| 8 | What is a grant bound to? | App ID, release key and declared set. A build that declares more asks again; an update with the same set keeps the grant. |
| 9 | Sampling rates? | Metrics 1 Hz, apps every 2 s, facts on load and on display change. One sampler for all granted views; it stops when none is visible. |
| 10 | Apps list content? | Top 10 by resident memory: name, memory, process count, opaque per-session ID, grouped per app as sysmon does. |
| 11 | Host name and user name in facts? | Left out. |
| 12 | Data contract? | sysmon's `Reading` and `AppUsage` shapes, plus per-core CPU and a facts object; the host pushes readings with `host.on` events. |
| 13 | Install sources? | Local file, HTTPS URL, GitHub release asset; folders only in developer mode. |
| 14 | Two QuickJS engines? | Link test in the spike; if they conflict, move `subspace-charts` to `quickjs-jit`. |
| 15 | GPUI grant API? | Set `Bundle::policy.granted` before loading, and ask nsh upstream for `AppView::set_granted_capabilities`. |
| 16 | Platforms? | macOS first; Windows and Linux after the spike measures them. |
