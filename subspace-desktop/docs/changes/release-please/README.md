# Release Please, a nightly Framework release and releases without Terminal

Back to the [change records](../README.md). Follows
[Publish the Framework from CI](../framework-publish-ci/README.md).

## Intent

The owner asked to:

- move to Release Please: conventional commits drive the versions and
  changelogs, with one release pull request for the Framework and one for
  the desktop app;
- merge the Framework's release pull request every night when there are
  changes, and publish it to crates.io. The desktop app's release pull
  request is merged by hand for now, because its signed, notarized build
  only works on the owner's Mac;
- stop releases opening Terminal windows. They were opened to reach the
  desktop login session, where the signing key is usable.

## Spec

### Release Please

- Manifest mode at the repository root (`release-please-config.json`,
  `.release-please-manifest.json`), two components with separate pull
  requests: `subspace-framework` (path `subspace-framework`, 0.1.0) and
  `subspace-desktop` (path `subspace-desktop`, 0.8.0). Tags keep today's
  form: `subspace-framework-vX.Y.Z` and `subspace-desktop-vX.Y.Z`.
- Version rule, as in ways of working: below 1.0 a breaking change or a
  `feat` bumps the minor version, anything else the patch.
- Each release pull request updates every place its version lives:
  - Framework: `[workspace.package] version`, the internal pins
    (`version = "=X"`), and, because the app builds the Framework by path,
    the app's requirement on it in `subspace-desktop/Cargo.toml` and the two
    terminal manifests.
  - App: `[workspace.package] version` and the two terminal manifests.
  - Lockfiles: Release Please cannot run Cargo, so the release workflow
    refreshes the affected lockfiles on the release pull request's branch
    (the Framework's, the app's and the two terminal ones), changing only
    the local packages' versions.
- Changelogs: `subspace-framework/CHANGELOG.md` and
  `subspace-desktop/CHANGELOG.md`, from the commits that touch each folder.

### Workflows

- `release.yml`, on every push to `main`: runs Release Please, which opens
  or updates the two release pull requests and, after one is merged, tags
  it and creates its GitHub release; then refreshes the lockfiles on the
  release branches.
- `release.yml`, nightly: merges the Framework's release pull request when
  one is open, then runs Release Please in the same run (a merge made by a
  workflow does not start other workflows).
- When a Framework release is created, `release.yml` starts
  `publish-framework.yml` on the new tag with `publish` set. That workflow
  keeps its file name, which crates.io trusts. It verifies on Linux
  (packaging, and building each crate from its packaged copy) and publishes
  with a short-lived token. Started by hand without `publish`, it uploads
  nothing. The tag-push trigger goes: Release Please is the one way a
  release starts.
- App: merging its release pull request creates the tag and a draft GitHub
  release with the changelog as notes. `make release-macos` builds, signs
  and notarizes the app for that version, attaches it to the draft and
  publishes it, which starts the packslip workflow as today. It never
  overwrites a published release.
- After a release pull request merges, `release.yml` fast-forwards
  `feat/subspace-desktop` to `main` when it can.

### Releases without Terminal

- `scripts/desktop-session.sh` runs a command in the macOS desktop login
  session through a one-off launchd agent in the `gui/<uid>` domain, with
  no window: it passes the working directory and a named set of environment
  variables, streams the output, returns the command's exit code and
  removes the agent.
- `make release-macos` and `make package-macos` use it automatically when
  they are started outside the desktop session, so they work from any
  shell.
- `make package-macos` with a keychain of its own (`SUBSPACE_MACOS_KEYCHAIN`,
  as `package-macos-ci.sh` sets) does not need the session and stays where
  it is.

### Docs

Ways of working (versioning, releases, integration merges), both AGENTS.md
files, the installation guide and the Framework's publishing guide describe
the new flow; the manual tag steps go.

## Decisions

- **`simple` with extra files, not `rust` or `cargo-workspace`.** Release
  Please's `rust` strategy reads each `[workspace] members` entry as a
  literal folder (the Framework's are globs such as `services/*`), writes
  `[package] version` rather than `[workspace.package] version`, cannot
  reach the excluded terminal manifests or the other project's folder, and
  would drop the `=` from the internal pins. The `cargo-workspace` plugin
  versions each crate separately. So both packages use the `simple` type,
  whose only own edit is `CHANGELOG.md` (it updates `version.txt` only if
  one exists), plus `extra-files`:
  - `toml` entries with a JSONPath for single values: each project's
    `$.workspace.package.version`, the terminal manifests'
    `$.package.version`, and the app's `subspace-framework` requirement in
    `subspace-desktop/Cargo.toml` and both terminal manifests (paths with a
    leading `/` are relative to the repository root). Release Please edits
    the value in place and keeps comments and layout.
  - one `generic` entry for the Framework's internal pins, between
    `# x-release-please-start-version` and `# x-release-please-end` in
    `subspace-framework/Cargo.toml`; it keeps the `=`.
  Generic annotations stay out of files both packages edit: Release Please
  applies every annotation in a file to each package that lists the file, so
  an annotation in `subspace-desktop/Cargo.toml` would receive the app's
  version in a Framework release, or the reverse. Annotations also stay off
  the `version = "…"` lines, because `stamp-prerelease-version.sh` matches
  those lines exactly. `scripts/check-release-config.py` enforces all of
  this and that every value matches the manifest.
- **Lockfiles by a script, checked by Cargo.** `scripts/sync-lockfiles.py`
  (repository root) reads every workspace member's name and version with
  `cargo metadata --no-deps --offline` (no network, no registry cache) and
  rewrites only the `version` line of lockfile entries without a `source`
  whose name a workspace declares. Registry and git packages and the
  patched `markdown` are untouched. `release.yml` commits the result only
  after `cargo metadata --locked` accepts all four lockfiles. A dry run on a
  copy of the repository with the Framework at 0.2.0 and the app at 0.9.0
  changed 24, 39, 16 and 15 entries and Cargo accepted all four offline with
  `--locked`.
- **Release pull requests are rebuilt when they fall behind `main`.** Both
  release pull requests edit `.release-please-manifest.json` and the app's
  lockfiles, so merging one leaves the other in conflict, and Release Please
  does not touch a release pull request whose description would not change.
  The first app release (0.9.0) left the Framework's 0.2.0 pull request
  unmergeable this way. `scripts/release-rebuild.sh` runs before each
  Release Please run and changes the description of a release pull request
  that is behind `main`, which makes Release Please rebuild its branch on
  `main`; the lockfile refresh then runs on the rebuilt branch, also after
  the nightly merge.
- **The app's release stays the repository's latest.** GitHub marks each
  new release Latest and Release Please has no setting for it, so the first
  Framework release (0.2.0) took the badge from Subspace 0.9.0. After a
  Framework release, `release.yml` marks the newest published
  `subspace-desktop-v*` release Latest again. The updater and
  `install-release.sh` choose by tag prefix and were not affected.
- **The 0.11.0 changelog was completed by hand, once.** Release Please's
  0.11.0 section listed 57 of the 89 releasable commits under
  `subspace-desktop/` since 0.10.0: it left out the 32 fixes that came in
  through two pull requests merged with a merge commit, and listed 11 merge
  commits beside the commits they merged. On the owner's instruction the
  section was corrected by hand after the release, with the same entries as
  the published release notes: every releasable commit once, no merge
  commits. This is the one exception to "Release Please owns the changelog".
  The cause appears to be how Release Please reads pull requests merged with
  a merge commit; it is not confirmed and not fixed. Until it is, compare
  the next app release pull request's notes with the `feat`, `fix`, `perf`
  and `revert` commits, and the breaking commits of any type (`!` or a
  `BREAKING CHANGE:` footer), in
  `git log --no-merges <last subspace-desktop-v* tag>..main -- subspace-desktop/`.
- **Root-level release tooling.** The configuration, the sync and its tests
  span both projects, and each project's scripts may not read the other's
  folder, so they live at the repository root (`scripts/`), with
  `make release-check` in the root Makefile (also run by the root
  `make check`) and a root prek pre-push hook for release files, manifests
  and lockfiles.
- **`make bump-version` removed.** Release Please's pull request is the one
  way a version moves; a manual bump would leave the manifest behind.
- **Draft releases with `force-tag-creation`.** The app's release is a draft
  so `make release-macos` can attach the signed archive before anyone can
  download it. GitHub creates a draft's tag only when it is published, and
  Release Please finds a component's last release by its tag commit, so
  without the tag the next run would miss the release and repeat its
  changelog. `force-tag-creation` (release-please 17.2.0 and later; the
  pinned action v5.0.0 bundles 17.6.0) creates the tag at the merge commit
  when the draft is created. `make release-macos` therefore runs on that
  tag, refuses unless the release exists, is a draft and its tag is `HEAD`,
  uploads the files (`--clobber`, safe because it is still a draft), then
  publishes with `gh release edit --draft=false --latest`, using the
  owner's `gh` token, so the `release: published` event starts packslip.
  The Framework's releases are not drafts.
- **Pre-releases keep the old path.** An integration build, a non-stable
  channel or a pre-release version is never a Release Please release, so
  `release-macos.sh` still creates those itself as pre-releases.
- **Pull request titles** keep the history's form:
  `chore(framework): release X.Y.Z` and `chore(subspace): release X.Y.Z`.
  Release Please identifies its pull requests by branch, so the titles need
  no component.
- **Nightly merge.** The nightly run first lets Release Please update the
  pull requests and refreshes the lockfiles, then squash-merges the
  Framework's with `--match-head-commit` (so a later push is not merged
  unseen) and runs Release Please again in the same job to tag and release
  it: a merge made with the workflow's token starts no workflow.
  `release.yml` then starts `publish-framework.yml` with
  `gh workflow run --ref <tag> -f publish=true` (`actions: write`), the one
  kind of event a workflow token may start. The ruleset on `main` asks for
  a pull request with no approvals, which the token can merge.
- **The integration branch.** After any release, `release.yml` moves
  `feat/subspace-desktop` to `main` with a non-forced ref update, which
  GitHub refuses unless it is a fast-forward; a refusal is reported in the
  job summary and the branch is left alone.
- **Desktop session through launchd.** `desktop-session.sh` writes a plist
  with `plistlib` (any argument is valid XML), runs `/bin/bash` on a small
  runner in the state folder, polls for a status file and streams the log
  by byte offset, so no background process is left behind. It forwards
  `PATH`, `HOME`, the locale, the signing and release variables, `MISE_*`,
  `SUBSPACE_*`, `CARGO_*` and `RUSTUP_*`, and drops any name containing
  `TOKEN`, `SECRET`, `PASSWORD`, `CREDENTIAL` or ending in `_KEY`. Cleanup
  (`launchctl bootout`, which stops the command, and removing the folder)
  runs on exit, `INT` and `TERM`.

## Delivery

- `release-please-config.json`, `.release-please-manifest.json`
  (Framework 0.1.0, app 0.8.0), `CHANGELOG.md` starting points in both
  projects, and the pin markers in `subspace-framework/Cargo.toml`.
- `.github/workflows/release.yml` (push to `main`, nightly at 02:23 UTC, a
  manual dry run) and `publish-framework.yml` (dispatch only, `publish`
  input, Linux verify). Every action is pinned by commit; `actionlint`
  passes.
- `scripts/sync-lockfiles.py`, `scripts/check-release-config.py`,
  `scripts/release-lockfiles.sh`, `scripts/release-rebuild.sh` and their
  tests (`scripts/test-release-tooling.py`,
  `scripts/test-release-lockfiles.sh`, `scripts/test-release-rebuild.sh`)
  in `make release-check`.
- `subspace-desktop/scripts/desktop-session.sh` and
  `test-desktop-session.sh` (fake `launchctl`, in the macOS script
  fixtures); `release-macos.sh` and `package-macos.sh` hop through it;
  `test-release-macos.sh` covers the draft flow, the refusals and the hop.
- Checked by:
  - the release-please 17.6.0 updaters applied to copies of the manifests
    (Framework 0.2.0, app 0.9.0): exactly the intended lines changed;
  - a read-only `Manifest.buildPullRequests()` from release-please 17.6.0
    against the real repository, with files read from this branch (the CLI
    dry run reads the configuration from `main`, where it does not exist
    yet): it found `subspace-desktop-v0.8.0` and the
    `subspace-framework-v0.1.0` tag, proposed `chore(subspace): release
    0.9.0` with the expected changelog, updating `CHANGELOG.md`, the three
    app manifests and the manifest file, and no Framework release (no
    releasable commits on `main` since the tag);
  - a real probe of `desktop-session.sh` from a non-Aqua shell
    (`launchctl managername` printed `Aqua` inside, `security find-identity`
    saw the Developer ID identity, exit status 3 came back, an interrupt
    stopped the command and left no agent or files).
- Not testable from here: the workflows themselves (first runs), the Linux
  packaging job, and a real signed release.

## Tasks

- [x] Release Please configuration, manifest, changelogs and pin markers.
- [x] `release.yml` and the dispatch-only `publish-framework.yml`.
- [x] Lockfile sync, configuration check and their tests in
  `make release-check`.
- [x] `desktop-session.sh` with tests; `release-macos.sh` and
  `package-macos.sh` use it.
- [x] `release-macos.sh` publishes Release Please's draft.
- [x] Docs: ways of working, both AGENTS.md files, the installation guide,
  the Framework's publishing guide and README, the developer checklist.
- [ ] First runs on GitHub after merge: `publish-framework.yml` by hand
  without `publish`, `release.yml` by hand (dry run), then the first push
  run opens the release pull requests.
- [ ] The first app release through `make release-macos` on a draft.
