# Public release ready to download

Tracker issue: SUB-60.

## Intent

The owner wants the public desktop release ready to be downloaded and tried.
The release stays as it is, with the API backend on and development sign-in
against `http://127.0.0.1:3000`, and gains the build with no API backend as a
second download. People who want a hosted API and their own sign-in point the
download at it with a small configuration file; people who want their own
name, logo or update source make their own build, from one page that says
how.

## Spec

- **Make your own build.** [Make your own build](../../build-your-own.md)
  lists every build config field (`build/branding.rs`), the build
  environment variables, the code constants a fork changes, and the signing
  and publishing variables, with WorkOS and Entra examples using placeholder
  values. The top-level README and `subspace-desktop/README.md` link it.
  Private build configurations stay out of the repository.
- **Both variants in every release.** `scripts/release-macos.sh` packages the
  app twice, first from `products/subspace/build-config.no-api.json` and
  then from `build-config.json`, ignoring any `SUBSPACE_BUILD_CONFIG` in the
  shell, and attaches `Subspace-NoAPI_<version>_darwin_arm64.zip`,
  `Subspace_<version>_darwin_arm64.zip` and their `.sha256` files to the same
  release. The names are those each variant's update source selects.
  `install-release.sh --no-api` and `install-release.ps1 -NoApi` (or
  `SUBSPACE_NO_API=1`) install the build with no API backend.
- **Channels, Issues and Decisions start on.** The descriptor no longer
  gates their default on a debug build. A saved choice in `config.json` still
  wins, so anyone who turned one off or on keeps it. With no API reachable,
  Issues and Decisions say `No API reachable at <address>. Start it, or set
  the app's API address in connection.json, then try again.` instead of only
  `Couldn't reach Subspace API`.
- **The connection file.** `connection.json` beside the app's `config.json`
  ([The connection file](../../subspace/connection-file.md) has the paths
  and the example) holds the settings the `SUBSPACE_*` run-time variables
  override, under their build config names, with the same rules
  (`src/connection.rs`, `launch::resolve_with`). A variable wins over the
  file, and the file over the built-in value; a setting the file leaves out
  falls through; no file changes nothing. It cannot change the backend,
  branding or update source, and names those settings when it tries. A build
  with no API backend refuses a file that would switch an API or sign-in on,
  as it refuses the variables. Development sign-in accepts only an API on
  this computer, from the file or a variable. The sign-in dialog rules are
  unchanged: `workos` or `entra` asks, `dev` does not.
- **A visible start-up failure.** A file or variable the app cannot use
  stops it before the shell, as before, but the desktop app now shows a
  window naming the file, the setting and the problem, with **Quit**
  (`subspace_framework::shell::report_startup_failure`). Commands still
  print to standard error.
- **Documentation fixes.** `docs/installation.md` describes the real release
  flow (no GitHub workflow builds the app; `make release-macos` builds,
  signs, notarizes and publishes both apps); `docs/issues.md` and the Issues
  guide say that only the `SUBSPACE_API_ACCESS_JWT_FILE` pairing is
  debug-only; `INSTALL.md` describes the standard release and the
  connection file for the fleet tunnel and a hosted API.

## Delivery

- Tests:
  - `products/subspace/src/connection.rs`: the file's names and types, a
    bad file naming the setting, a missing file, and launch messages in the
    file's words.
  - `products/subspace/src/launch.rs`: precedence (variable, file, build),
    no file changes nothing, the file held to the same rules and named when
    it breaks one, and a build with no API backend refusing a file that
    switches an API on.
  - `clients/subspace-api/src/backend.rs`: the unreachable message names the
    address.
  - `products/descriptor/src/lib.rs`: Channels, Issues and Decisions start
    on in every build.
  - `scripts/test-release-macos.sh`: both variants packaged from the
    repository's two build configs, the standard one last, all four files
    uploaded and checksummed, and a stray `SUBSPACE_BUILD_CONFIG` never
    reaching the packaging.
  - `scripts/test-install-releases.sh`: `--no-api`, `-NoApi` and
    `SUBSPACE_NO_API=1` install the no-API archive from the same release,
    with the shell and PowerShell installers.
  - `subspace-framework/host/app/src/startup_failure.rs`: the window says
    what failed and offers Quit.
- Screenshots: none. The start-up failure window opens before the shell, so
  the capture tooling, which drives the shell window, cannot reach it; and
  capture builds turn sign-in off, so they show Issues' sign-in state rather
  than the development sign-in's unreachable state.
- Not verified: a real signed and notarized two-variant release, the
  installers against a real release, the start-up failure window on
  Windows and Linux, and a first run against an unreachable API with
  development sign-in in a running app.

## Tasks

- [x] Make your own build page, linked from both READMEs.
- [x] Both variants in `make release-macos`; installer option.
- [x] Channels, Issues and Decisions on by default; unreachable message.
- [x] The connection file, its tests and the start-up failure window.
- [x] Installation, Issues and INSTALL.md corrections.
- [ ] packslip signs only `Subspace_*_darwin_arm64.zip`; signing the
      `-NoAPI` archive too needs the workflow's download pattern widened.
