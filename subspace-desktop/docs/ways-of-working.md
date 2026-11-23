# Ways of working

How changes reach `main` in this repository. Agents also follow
[AGENTS.md](../AGENTS.md), which stays authoritative for agent-specific rules.

## Commits

Write every commit message as a
[conventional commit](https://www.conventionalcommits.org/en/v1.0.0/):

```text
type(scope)!: summary

Body: why the change is needed and what it affects.

BREAKING CHANGE: what changed and what callers or users must do.
```

- **Summary**: imperative mood, lower case, no full stop, about 72 characters
  at most: `fix(irc): keep the topic when a channel rejoins`. Add the issue key
  in parentheses at the end when the issue is public, for example `(#42)`.
- **Body**: explain why, not how. The diff shows how.
- **One logical change per commit.** A reviewer should be able to revert a
  commit without losing unrelated work. Keep formatting, renames and
  behaviour changes in separate commits.
- **No merge noise.** Sync a branch by rebasing, not by repeated merges from
  its base. Merge commits come from merging pull requests.

### Types

| Type | Use it for | Release effect |
| --- | --- | --- |
| `feat` | A new capability people or agents can use: a board, control, setting, command or tool | Minor |
| `fix` | A bug fix in shipped behaviour | Patch |
| `perf` | A faster or leaner implementation with the same behaviour | Patch |
| `refactor` | A code change that neither fixes a bug nor adds a feature | None |
| `docs` | Documentation only, including change records and board guides | None |
| `test` | Adding or correcting tests and fixtures only | None |
| `build` | Build, packaging, signing, dependencies, Makefile and mise tasks | None; use `fix` or `feat` for a change people notice in the shipped app |
| `ci` | CI configuration and hooks | None |
| `style` | Formatting with no code change | None |
| `chore` | Maintenance that fits no other type, such as a release commit | None |
| `revert` | Reverting an earlier commit; quote its summary and hash in the body | Patch |

A `fix` that also adds behaviour is a `feat`. When in doubt between `feat` and
another type, ask whether release notes would list it as new.

### Scopes

The scope names the area, usually a board, client or Framework crate, as in
the history: `subspace` (the app, workspace, releases), `irc`, `irc-client`,
`github`, `nett`, `issues`, `chat`, `chat-input`, `agents`, `rich`, `host`,
`capture`, `scripts`. Use an existing scope when one fits; add a new one only
for a new area. Omit the scope only for repository-wide changes.

### Breaking changes

Mark a breaking change with `!` after the scope and a `BREAKING CHANGE:`
footer. Breaking changes include removed or renamed settings, configuration
keys, CLI commands, MCP tools and stored-state formats. This repository does
not keep compatibility layers (see the engineering principles), so say in the
footer what a person must do after upgrading.

## Branches and pull requests

- Never commit to `main`. The `no-commit-to-branch` hook refuses it.
- Work on a branch, open a pull request and merge it once the gates pass.
- Agents commit, push or add remotes only when the owner asks
  ([AGENTS.md](../AGENTS.md#conventions)).
- Record each change under `docs/changes/<slug>/`; see
  [change records](changes/README.md).
- A change that alters what Subspace shows needs screenshots in the pull
  request; follow [UI change evidence](../AGENTS.md#ui-change-evidence).

## Before opening or merging a pull request

Run through this list before opening a pull request and again before merging
it. The repository's pull request template
(`.github/pull_request_template.md` at the repository root) repeats it.

- [ ] Commits are conventional (`type(scope)!: summary`), one logical change
  each, rebased with no merge noise. See [Commits](#commits).
- [ ] Pre-push checks pass for the changed projects. See [Gates](#gates).
- [ ] The change has a record under `docs/changes/<slug>/` with intent, spec,
  delivery and tasks. See [change records](changes/README.md).
- [ ] New or moved pages under `docs/` are linked from the
  [documentation index](README.md). Board guides, [MCP server](../../subspace-framework/docs/mcp.md) and
  [Command-line interface](../../subspace-framework/docs/cli.md) match the behaviour.
- [ ] Every pull request that changes what Subspace shows embeds its
  screenshots in the description, with commit-pinned URLs, as
  [UI change evidence](../AGENTS.md#ui-change-evidence) describes. This
  includes integration merges into `main`, which embed each included
  change's key images or link the pull requests that carry them. Without
  them a pull request is not ready to merge. `gh pr create --body` skips the
  template, so copy its sections into the body.
- [ ] Before merging, independent reviews from Claude Opus 5.5 (high) and
  GPT-6.2 Sol (high) clear the final head. Record each review and its method
  under **Reviews**. Read-only subagents and restricted CLI reviewers count.
  See [Pull request review clearance](../../AGENTS.md#pull-request-review-clearance)
  for the requirements and exceptions.
- [ ] The screenshots are compared with the shell anatomy image and the
  checklist in the [design guide](../../subspace-framework/docs/design-guide.md#reviewing-a-ui-change):
  the same zones in the same places, one text edge, aligned strips, views in
  the navigator and state in the status bar. Content differs per Space; the
  layout does not.
- [ ] UI follows the [component rules](../../subspace-framework/docs/ui-components.md#rules): a GPUI Kit
  component first, then a shared component built from GPUI Kit components,
  custom drawing last with its reason in the catalogue. Every new or changed
  component has its Component gallery card and catalogue name.
- [ ] Breaking changes are marked with `!` and a `BREAKING CHANGE:` footer.
  See [Breaking changes](#breaking-changes).
- [ ] A dependency change passes `mise run audit`. See
  [Dependency audit](../AGENTS.md#dependency-audit).
- [ ] No commit changes a version or a changelog by hand: Release Please's
  pull requests do. See [Releases](#releases).
- [ ] A change that adds or changes a board completes the
  [board checklist](../../subspace-framework/docs/board-checklist.md) in its change record, with each item
  implemented or "not applicable because …". See
  [What the Framework gives a board](../../subspace-framework/docs/board-integration.md).

## Gates

No CI builds or tests the code. The GitHub workflows release and sign (see
[Releases](#releases)); the prek hooks are the gate; install them with
`make hooks`.

| Hook | Runs |
| --- | --- |
| pre-commit | File hygiene, `mise run fmt-check` and gitleaks |
| pre-push | The checks below, selected from the pushed diff |

- Framework and desktop source or build input changes run `mise run check` in
  the affected project. Rust changes and changes to its quality configuration
  also run `mise run quality`.
- Markdown changes run the Framework or desktop board guide and documentation
  index checks for the affected project. A Markdown-only push skips code tests
  and builds.
- API code changes run Bun type checking, lint and tests. Database-backed API
  tests use the [local Compose stack](../../subspace-api/README.md).
- Added or modified release tooling, workflows, Cargo manifests and Cargo
  lockfiles run the root `make release-check`.

Do not bypass the hooks with `--no-verify`. If a hook fails, fix the cause.
See [Commands](../AGENTS.md#commands) and
[Code quality rules](../AGENTS.md#code-quality-rules) for what each check
enforces.

## Versioning

Releases follow [semantic versioning](https://semver.org) with the `0.x`
convention. Release Please derives each project's bump from the
conventional commits that touched its folder since its last release tag
(`subspace-desktop-vX.Y.Z`, `subspace-framework-vX.Y.Z`), configured to
match this table (`bump-minor-pre-major`, no
`bump-patch-for-minor-pre-major`; `scripts/check-release-config.py` keeps it
so):

| Commits since the last release | Bump | Example |
| --- | --- | --- |
| Any breaking change (`!` or `BREAKING CHANGE`) | Minor while below 1.0; the changelog lists it first | 0.3.0 → 0.4.0 |
| Any `feat` | Minor | 0.2.0 → 0.3.0 |
| Only `fix`, `perf` or `revert` | Patch | 0.3.0 → 0.3.1 |
| Nothing else releases (`docs`, `refactor`, `build`, `chore` and the rest) | No release | |

The highest row that applies wins. 0.2.1 contained new features and should
have been 0.3.0. It stays as published; this rule exists so that does not
recur. To preview the next versions, run `release.yml` by hand
(`gh workflow run release.yml`): its dry run changes nothing.

## Releases

[Release Please](https://github.com/googleapis/release-please) runs in
`.github/workflows/release.yml` from `release-please-config.json` and
`.release-please-manifest.json` at the repository root, with one release
pull request per project against `main`:

| Project | Pull request | Versions it moves | After it merges |
| --- | --- | --- | --- |
| Framework | `chore(framework): release X.Y.Z` | `[workspace.package] version`, the internal `=X.Y.Z` pins, and the app's requirement on the Framework in `Cargo.toml` and both terminal manifests | Merged by the workflow every night when one is open. Tagged, released and published to crates.io by `publish-framework.yml` ([publishing](../../subspace-framework/docs/publishing.md)) |
| App | `chore(subspace): release X.Y.Z` | `[workspace.package] version` and both terminal manifests | Merged by hand. Tagged, with a draft release; `make release-macos` publishes it |

Each pull request also updates its project's `CHANGELOG.md` and carries the
four lockfiles: Release Please cannot run Cargo, so the workflow commits
what `scripts/sync-lockfiles.py` changes (only local packages' versions)
to the pull request's branch after Cargo accepts it with `--locked`, then
starts the `lockfiles` check (`.github/workflows/lockfiles.yml`, which runs
`scripts/sync-lockfiles.py --check`) on that branch. Every other pull request
gets the check from its own `pull_request` run; a release pull request does
not, because the release workflow's token starts no other workflow. Do not
merge a release pull request without a passing `lockfiles` check. When
one release merges, the other's pull request falls behind `main` and the
workflow has Release Please rebuild it (`scripts/release-rebuild.sh`).

Each app release also carries the fleet installer, which pins Stargate by
commit, and every Subspace release pins Stargate's latest release. The same
script starts the `stargate-pin` check (`.github/workflows/stargate-pin.yml`,
which runs `scripts/stargate-pin.py --check`) on each release branch, beside
`lockfiles`, on every push to `main` and nightly. On the app's branch it fails
while the pin is not Stargate's latest release, and when GitHub's API cannot
be reached, so a pin it could not check never passes; on the Framework's
branch, which does not carry the installer, it is skipped. Do not merge the
app's release pull request without a passing `stargate-pin` check. When it
fails, merge the Stargate bump pull request first, after its checklist, and
Release Please rebuilds the release pull request on the new `main`. The
`stargate-bump` workflow opens that pull request daily when Stargate has a
newer release, with the release notes, the installer's test results and what
to check. When a release cannot be adopted yet, a reviewed hold file lets
the check pass with its reason as a warning until a date or a newer release.
See [Stargate pin](../../subspace-installer/README.md#stargate-pin). This
check needs the network, so `make release-check` and the push hooks, which
work offline, do not run it.

1. Merge commits to `main` as usual. Release Please opens or updates the
   release pull requests on each push.
2. The Framework's is merged nightly. Review it during the day; close it, or
   push a commit to `main`, to change what goes out.
3. To release the app, merge its pull request. When the release workflow has
   run, check out the new tag and run `make release-macos` on the owner's
   Mac (any shell: it moves into the desktop session itself). It attaches the
   signed app to the draft and publishes it, which starts packslip. The
   release workflow has already started `windows-release`, which builds,
   signs and attaches the Windows archive
   ([Sign Windows releases](installation.md#sign-windows-releases)).

Never change a version, a changelog entry or a release tag by hand. Signing,
notarization and options such as `DRAFT=1` are in
[Publish a macOS release](installation.md#publish-a-macos-release).

## Integration merges into main

Release pull requests merge into `main` directly. After a release, the
release workflow fast-forwards `feat/subspace-desktop` to `main` when it
can; when it cannot (the branch has commits `main` lacks), its job summary
says so: rebase or merge the branch onto `main` before the next integration
merge.

Merge `feat/subspace-desktop` into `main` through a temporary branch: create
`merge/feat-to-main` at the integration head, open the pull request from it
and merge with a merge commit. The repository deletes a pull request's head
branch on merge, so merging the integration branch directly deletes it and
retargets the pull requests based on it. Do not run `gh pr merge` from a
checkout with the head branch checked out; it switches that checkout to the
base branch.
