# Agent instructions for Subspace

Subspace is the communications layer for autonomous, collaborative coding
agents. It brings together IRC and a bespoke issue tracker so agents can
communicate and coordinate their work. The desktop app gives humans access to
both. Start with the [repository README](README.md).

**If the user wants to install, set up or try Subspace, follow
[INSTALL.md](INSTALL.md).** It lists the questions to ask them first and the
rules that keep the fleet VM private. If the user wants their local agents to
use the tracker or the fleet's IRC, see [agent-skills/](agent-skills/README.md).
When you act through those skills you use the user's login, so always say you
are an agent acting for them, with your model and version.

## Repository layout

Each folder is a self-contained project with its own toolchain, checks and
docs. Read the nearest `AGENTS.md` before editing a project, then use its
README and documentation index to find the pages relevant to your change.

| Project | Role | Guidance |
| --- | --- | --- |
| [`subspace-framework/`](subspace-framework/README.md) | Reusable Rust crates exposed through `subspace-framework`, published on crates.io. | [AGENTS.md](subspace-framework/AGENTS.md) |
| [`subspace-desktop/`](subspace-desktop/README.md) | Rust and GPUI desktop app with Spaces, service clients, a terminal and web Spaces. | [AGENTS.md](subspace-desktop/AGENTS.md) |
| [`subspace-irc/`](subspace-irc/README.md) | Ergo IRCd packaged as a container. | |
| [`subspace-api/`](subspace-api/README.md) | TypeScript HTTP API for the issue tracker, initiatives and fleets. | |
| [`subspace-installer/`](subspace-installer/README.md) | Installer for fleet VMs. | |

## Toolchains

- **Framework and desktop.** Both Rust workspaces use edition 2024 and pin
  stable Rust 1.99.0 through mise. Use GPUI through `gpui-kit` (0.9.1 in the
  workspace manifests). The desktop terminal also pins Zig 0.16.0. Follow the
  pins in the [Framework mise config](subspace-framework/mise.toml),
  [desktop mise config](subspace-desktop/mise.toml) and
  [terminal mise config](subspace-desktop/mise.terminal.toml).
- **API.** Bun and strict TypeScript, with Effect HttpApi and Schema at the
  HTTP boundary and Drizzle ORM on PostgreSQL. WorkOS is the default identity
  provider, with optional Entra and an explicitly enabled local development
  provider. Use oxlint and `bun test`.
- **IRC.** Ergo IRCd runs under Docker Compose.
- **Installer.** Bash for Ubuntu LTS VMs. It sets up stargate, OpenCode (the
  default agent harness), CLIProxyAPI and agentsview.

## Glossary

| Term | Meaning |
| --- | --- |
| Space | A dashboard module, called a board in much of the code. Prefer *Space* in new text. |
| Framework | The reusable crates that apps are composed from, exposed through the `subspace-framework` facade crate. |
| Channels | The IRC Space for networks, channels and private messages. Its board ID is `irc`. |
| Fleet | A VM running one rear admiral and a set of captains. Fleet and agent names are configurable. |
| Rear admiral | The agent that coordinates work across a fleet's captains. |
| Captain | An autonomous coding agent that carries out work in a fleet. |
| Initiative | A grouping of work that can be assigned to a fleet. Switching initiative in the desktop also switches fleet context when the new initiative belongs to a different fleet. |

See the [Framework glossary](subspace-framework/docs/glossary.md) and
[desktop glossary](subspace-desktop/docs/glossary.md) for product terminology.

## Working rules

- Keep changes small and focused. Include tests for changed behaviour and run
  the relevant project checks before reporting work as done.
- Use conventional commits (`type(scope)!: summary`), one logical change per
  commit. Work on a branch.
- Release Please owns release versions and changelogs. Never bump a version,
  edit a release changelog or tag a release by hand.
  [Release Please configuration](release-please-config.json)
- Keep prek hooks installed and passing. Never use `--no-verify`. Review the
  staged diff before committing. The pre-push hook runs code checks only for
  changed projects. Markdown changes run the applicable documentation checks.
  [Repository hook configuration](prek.toml)
- Before the first commit in a clone, check that `.git/hooks/pre-commit` and
  `.git/hooks/pre-push` exist, and run `make hooks` if either is missing. A
  fresh clone has neither, and without them nothing checks a commit or a push.
- Open a pull request only after the hooks have run on every commit in it and
  on the push. A commit made another way, such as through the GitHub API or
  the web editor, skips them. Then, with the branch checked out, a clean
  working tree and `base=$(git merge-base origin/main HEAD)`, run the commit
  hooks on each commit and the push hooks on the whole range, and say in the
  pull request that you did:

  ```sh
  git rebase --exec 'prek run --last-commit' "$base"
  prek run --stage pre-push --from-ref "$base" --to-ref HEAD
  ```

  `git rebase --exec` checks out each commit in turn, leaves commits that
  pass unchanged, and stops at the first failure; run `git rebase --abort`,
  then fix that commit. A single `prek run --from-ref "$base"` is not
  enough: it checks the files as they are in the current checkout, so it
  misses a problem, such as a private key, added in one commit and removed in
  a later one.
- Write prose in British English. Keep existing code and API identifiers.
- Keep this public repository free of secrets, tokens, real tenant or client
  IDs, internal hostnames, personal data, and organisation-specific branding
  or configuration. Use fictional fixtures and placeholders such as
  `example.com` in docs and `.env.example` files. Configure identity
  providers and service endpoints at build or run time.
- Keep generated build output out of commits.
- Build a new or changed Space to the
  [design guide](subspace-framework/docs/design-guide.md): the same
  workspace shell, zones and rules in every Space.
- Merge a pull request only after both model reviews clear it. See
  [Pull request review clearance](#pull-request-review-clearance).

## Pull request review clearance

Before merging, obtain two independent, read-only reviews of the PR changes.

- **GPT-6.2 Sol** at high effort.
- **Claude Opus 5.5** at high effort.

Each reviewer must actually run the named model at the specified effort and
inspect the PR diff. A read-only subagent or a restricted CLI reviewer counts.
Both reviewers must be independent of the implementation agent.

**The gate is the two model verdicts.** [`scripts/review-pr.sh`](scripts/review-pr.sh)
is an optional helper. If the helper cannot launch a model, run that model
through another read-only method. A launch error, timeout or missing receipt
affects only the helper's own run. Completed reviews recorded in the PR still
count, and any critical or high findings still need resolution.

### Run and record the reviews

- Give each reviewer the diff from the pinned commits and relevant files.
  Disable external diff tools and text conversion when preparing a Git diff.
  Have a person inspect any binary or submodule changes the reviewers cannot
  read. Check for secrets before sharing review material.
- Reviewers read and report findings only. Disable write tools and external
  actions where the launcher supports those controls. Use an isolated session
  if the current one could run hooks or settings from the change.
- Reviews can run before or after opening the PR. Confirm that a local review
  covers the PR's target branch, merge base and head.
- Verify each model and effort from the launch settings or session output.
  Under **Reviews**, record the model, effort, method, target branch, merge base,
  head, reviewed range, verdict and how each finding was resolved. Check review
  text for secrets before posting it. Flag changes to agent instructions, hooks,
  workflows, task runners or the helper for a maintainer to read before merging.
- The required **Fleet app review** check uses P0 to P3. P0 is critical, P1
  is high, P2 is medium and P3 is low.
- A review is clear when the reviewer inspected the change and found no
  remaining P0 or P1 findings. Verify each finding. Fix or refute each P0
  or P1 in the PR and review any amended code before merging.
- Use judgement on P2 and P3 findings, including findings from fix rounds.
  Fix those worth addressing in this PR. Under **Reviews**, record each
  finding with its fix commit, the reason for leaving it open, or a link to
  the issue tracking separate work. Once the remaining P2 and P3 findings
  have a recorded decision, complete the merge checks below.
- Have both reviewers cover any changes to the reviewed diff. Run full reviews
  after a rebase or rewrite that changes commit trees, or when the target
  branch or merge base changes. If only commit metadata changes, the reviewers
  may carry forward their verdicts after verifying each commit tree in order
  against the reviewed commits. Record that verification against the new head.
- Immediately before merging, confirm both clear verdicts cover the current
  head, target branch and merge base, and that project checks pass. When
  merging with `gh`, use `--match-head-commit` with the reviewed head.

### Optional review helper

If using the helper, read these instructions on canonical `main` and fetch
the script from there. Run it from the repository root with `pr` set to the
pull request number.

```sh
review_pr() (
  helper=$(mktemp) && trap 'rm -f "$helper"' EXIT &&
  gh api --hostname github.com -H 'Accept: application/vnd.github.raw' \
    'repos/aiconsultancy/subspace/contents/scripts/review-pr.sh?ref=refs/heads/main' > "$helper" &&
  test -s "$helper" && bash "$helper" "$@"
)
review_pr --pr "$pr" --repo aiconsultancy/subspace
```

The helper verifies its own copy, scans for secrets and isolates its reviewers.
Use `review_pr --help` for local and incremental runs. Its receipts apply to
its own incremental mode.

### Exceptions

- **Release Please.** Its pull requests and the release workflow's automated
  merges are exempt. If the branch gains any other commit, both reviews and a
  manual merge are required.
- **Integration merges** (`merge/feat-to-main`) reuse clearances for changes
  already reviewed in their own PRs. Review additional changes, including
  conflict resolutions, direct commits and hand rebases.

## Commands

Plain `make` or `make help` lists the root targets. From the repository root:

- `make build`, `make test`, `make check`, `make format`, `make fmt-check`,
  `make lint`, `make quality` and `make audit` run in the Framework first,
  then the desktop.
- `make release-check` checks release tooling and is also part of root
  `make check`. `make hooks` installs the prek hooks on a fresh checkout.
- `make stack-up` runs PostgreSQL, the API and IRC from the root
  `compose.yaml` on loopback with demo data, and `make stack-reset` removes
  them. See [Run the whole system locally](README.md#run-the-whole-system-locally).
- Other targets are forwarded to the desktop Makefile, so `make run` also
  works from the root.

Run project commands from the owning folder.

| Project | Everyday commands |
| --- | --- |
| Framework | `mise install`, `make build`, `make test`, `make check`, `make quality`. Run `make audit` after dependency changes. |
| Desktop | `mise install`, `make run`, `make test`, `make check`, `make quality`. Use `make interfaces` for the headless product interface and `MISE_ENV=terminal make check-terminal` for the terminal. Run `make audit` after dependency changes. |
| IRC | `docker compose config --quiet` validates the configuration. `docker compose up --build -d` starts the server. |
| API | `bun install --frozen-lockfile` installs pinned dependencies. Run `bun test` and use the scripts in `package.json` for linting, type checking and running the API. `bun run` lists them. |
| Installer | Use `bash -n` and `shellcheck` on changed scripts, including the root `install.sh`. Run `python3 -B subspace-installer/tests/policy_test.py` (Python 3.10 or later). Follow the installer README for the fleet setup commands and VM checks, and INSTALL.md for the host installer. |

See the project guidance for prerequisites, integration checks and release
commands. For changes across projects, run the checks in each affected folder
and exercise the shared behaviour with the local stack.

## Deeper guidance

- [Desktop AGENTS.md](subspace-desktop/AGENTS.md) and
  [documentation index](subspace-desktop/docs/README.md).
- [Framework AGENTS.md](subspace-framework/AGENTS.md) and
  [documentation index](subspace-framework/docs/README.md).
- [Design guide](subspace-framework/docs/design-guide.md): how every Space
  is laid out, the shell anatomy image, and the checklist reviewers use on
  UI screenshots.
- [Desktop agent skills](subspace-desktop/.agents/skills) and
  [Framework agent skills](subspace-framework/.agents/skills) hold the local
  checklists and specialised workflows.
