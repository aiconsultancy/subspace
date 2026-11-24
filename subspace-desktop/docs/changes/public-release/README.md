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
  gates their default on a debug build. A choice saved in `config.json`
  still wins, but the app saved a choice only when it differed from the old
  default (off in a release build): someone who turned Channels, Issues or
  Decisions on and back off has nothing saved, so after upgrading it starts
  on and they turn it off again in Settings. With no API reachable,
  Issues and Decisions say `No API reachable at <address>. Start it, or set
  the app's API address in connection.json, then try again.` instead of only
  `Couldn't reach Subspace API`.
- **The connection file.** `connection.json` beside the app's `config.json`
  ([The connection file](../../subspace/connection-file.md) has the paths
  and the example) holds the settings the `SUBSPACE_*` run-time variables
  override, under their build config names, with the same rules
  (`src/connection.rs`, `launch::resolve_with`). A variable wins over the
  file, and the file over the built-in value, so a variable can complete or
  correct the file, and an empty `SUBSPACE_IDENTITY_PROVIDER=` turns sign-in
  off for the run; a setting the file leaves out falls through; no file
  changes nothing. When the merged settings fail and the file alone fails
  too, the problem is reported as the file's; otherwise as the variable's.
  When the app's config folder cannot be found, no file is read. It cannot
  change the backend, branding or update source, and names those settings
  when it tries. A build with no API backend refuses a file that would
  switch an API or sign-in on, as it refuses the variables, so moving from
  the standard app to the `-NoAPI` one with a hosted `connection.json` in
  place stops the `-NoAPI` app at start-up with the file named (both apps
  share the settings folder); this is intended and the connection file
  page says to move the file aside first. The sign-in dialog rules are
  unchanged: `workos` or `entra` asks, `dev` does not.
- **Sign-in settings checked at start-up.** The identity provider must be
  `dev`, `workos`, `entra` or empty, and the selected provider's
  configuration is built before the shell (`launch::workos_config`,
  `launch::entra_config`, the same builders sign-in uses), so `"WorkOS"` or
  a malformed `workos_api_url` stops the app naming the setting instead of
  leaving sign-in silently off.
- **Development sign-in on a hosted API opens.** Development sign-in still
  reaches only an API on this computer, a rule kept in the dev provider. A
  launch whose `api_url`, from the file or a variable, is on another
  computer opens as it did before this change, with no account; Issues and
  Decisions say **Sign-in unavailable** and why
  (`subspace_api_client::sign_in_state`) instead of pointing to an Account
  page the app does not have.
- **A visible start-up failure.** A file or variable the app cannot use
  stops it before the shell, as before, but the desktop app now shows a
  window naming the file, the setting and the problem, with **Quit**; closing
  the window quits too
  (`subspace_framework::shell::report_startup_failure`). Commands, and
  capture runs, print to standard error and exit non-zero instead. About,
  `--version` and `upgrade` read no settings
  (`subspace_framework::shell::reads_configuration`), so a broken file or
  variable never blocks them.
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
    no file changes nothing, a variable completing or correcting the file
    and an empty one turning sign-in off, the file held to the same rules
    and named when it breaks one (including an unknown provider, a
    malformed WorkOS API URL and unusable Entra IDs), a provider variable
    sign-in would refuse named by its variable, development sign-in on a
    hosted API opening with sign-in unavailable, and a build with no API
    backend refusing a file that switches an API on.
  - `clients/subspace-api/src/backend.rs`: the unreachable message names the
    address; the sign-in state says why sign-in is unavailable when the
    product recorded a reason.
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
    what failed and offers Quit; `startup.rs`: the routes that read no
    configuration.
  - By hand, with a malformed `connection.json` in a temporary home: a debug
    build's `--version` and `about` exit 0, a command prints the problem
    and fails, and a capture run (`SUBSPACE_CAPTURE_DIR`) prints it and exits
    1 without a window.
- Screenshots: none. The start-up failure window opens before the shell, so
  the capture tooling, which drives the shell window, cannot reach it; and
  capture builds turn sign-in off, so they show Issues' sign-in state rather
  than the development sign-in's unreachable state.
- Not verified: a real signed and notarized two-variant release, the
  installers against a real release, the start-up failure window on
  Windows and Linux, closing it from its title bar, and a first run against
  an unreachable API with development sign-in in a running app.
- Known limits:
  - When the local API is down at start-up, development sign-in has no
    session, and Retry in Issues or Decisions reloads without signing in
    again: the account's restore is the shell's, which a Space cannot reach,
    so making Retry restore it needs a new Framework hook through the API
    client into both Spaces. **Settings › Account › Retry** (or reopening
    the app) signs in once the API is up.
  - A Space someone turned on and back off in an earlier release has no
    saved choice, so it comes back on after upgrading.
  - On macOS, quitting the start-up failure window exits 0; scripts read
    standard error.

## Tasks

- [x] Make your own build page, linked from both READMEs.
- [x] Both variants in `make release-macos`; installer option.
- [x] Channels, Issues and Decisions on by default; unreachable message.
- [x] The connection file, its tests and the start-up failure window.
- [x] Installation, Issues and INSTALL.md corrections.
- [x] Review fixes: window close quits, merged settings first, sign-in
      settings checked, dev sign-in on a hosted API opens, settings-free
      commands and capture runs, installer and locate fixes.
- [ ] packslip signs only `Subspace_*_darwin_arm64.zip`; signing the
      `-NoAPI` archive too needs the workflow's download pattern widened.
