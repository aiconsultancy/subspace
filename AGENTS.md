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
  staged diff before committing. [Repository hook configuration](prek.toml)
- Write prose in British English. Keep existing code and API identifiers.
- Keep this public repository free of secrets, tokens, real tenant or client
  IDs, internal hostnames, personal data, and organisation-specific branding
  or configuration. Use fictional fixtures and placeholders such as
  `example.com` in docs and `.env.example` files. Configure identity
  providers and service endpoints at build or run time.
- Keep generated build output out of commits.
- Merge a pull request only after both model reviews clear it. See
  [Pull request review clearance](#pull-request-review-clearance).

## Pull request review clearance

Every pull request needs clearance from two independent model reviews before
it merges: Claude Opus 5.5 at high effort and GPT-6.2 Sol at high effort. They
come on top of the project checks, never instead of them. Review the diff
before opening the pull request: fetch the target branch from the remote the
pull request will target, run both reviews on the local branch against it, and
deal with what they find first, so the pull request opens already reviewed.
The clearances also gate the merge.

Run both reviews with the **trusted** copy of
[`scripts/review-pr.sh`](scripts/review-pr.sh), the one on the canonical
repository's `main`, never the copy in the change under review. Take the
launcher below from the canonical `main` too, read straight from GitHub rather
than from any local branch, tag or checkout, which could hold a changed copy:

```sh
gh api --hostname github.com -H 'Accept: application/vnd.github.raw' \
  'repos/aiconsultancy/subspace/contents/AGENTS.md?ref=refs/heads/main'
```

The launcher fetches the script into a private scratch repository with no Git
configuration, so no URL rewrite can substitute another copy:

```sh
review() (
  d=$(mktemp -d) && trap 'rm -rf "$d"' EXIT &&
  tg() { env -i PATH="$PATH" HOME="$HOME" GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CONFIG_NOSYSTEM=1 git "$@"; } &&
  tg init -q --bare --template= "$d/r.git" &&
  tg -C "$d/r.git" -c credential.helper= -c 'credential.helper=!gh auth git-credential' \
    fetch -q --depth=1 https://github.com/aiconsultancy/subspace.git refs/heads/main &&
  tg -C "$d/r.git" show FETCH_HEAD:scripts/review-pr.sh > "$d/review-pr.sh" &&
  test -s "$d/review-pr.sh" && bash "$d/review-pr.sh" "$@"
)
git fetch "$remote" "+refs/heads/$target:refs/remotes/$remote/$target" &&
  review --local "refs/remotes/$remote/$target"     # this clone's HEAD, before opening
review --local "refs/remotes/$remote/$target" --since "$cleared"  # after a local review
review --pr "$pr"                                   # the pushed head
review --pr "$pr" --since "$cleared"                # commits added after a --pr review
```

These commands are for the person or agent preparing the pull request. Here
`$remote` is the name of the remote the pull request targets (for example
`origin` or `public`), `$target` its branch, `$pr` the pull request number and
`$cleared` the last cleared head. Each step of the launcher must succeed before
the next runs, so a failed fetch never runs a stale or empty launcher. The
launcher passes only `PATH` and `HOME` to Git, so a `gh` signed in through
`GH_TOKEN` alone, a proxy, or a custom CA bundle needs those variables added to
`tg`. A proxy or CA bundle can then see or alter the fetch, so use only ones
you control.

It needs `git`, `gh`, `codex`, `claude`, `python3` and `gitleaks`, and runs
from Claude Code and from Codex alike. It:

- does all Git work in a fresh scratch repository with no user, system or
  repository configuration, so no URL rewrites, hooks, filters or attributes
  apply. It fetches the pull request from the repository GitHub names and
  checks that both the head and base commits match;
- refuses to run unless it matches the canonical `main` copy;
- clears inherited `GIT_*` variables, and scans every commit in the range
  (merges against each parent) plus everything the reviewers would see with
  gitleaks before anything is sent to a model, stopping on a possible secret.
  It uses gitleaks' built-in rules only, so no `gitleaks:allow` comment, config
  or ignore file in the change can exempt a line;
- stops on binary changes other than images, PDFs and fonts, which a person
  looks at, and on submodule pointer changes, since the reviewers cannot read
  either;
- writes the diff and a snapshot straight from the committed files;
- runs GPT-6.2 Sol with no shell or file access at all, on the diff passed
  inline with 25 lines of context;
- runs Claude Opus 5.5 in restricted mode with only Read, Grep and Glob,
  confined to the diff and the snapshot;
- starts both reviewers from an empty folder with a fixed minimal environment,
  and with user and project settings, hooks, rules and MCP servers switched off;
- passes the pull request's title and any author notes only as untrusted data,
  after the instructions;
- checks the Sol session header for the model and high effort, and that Opus
  was the only model used. `--effort high` is passed to Claude, but its result
  does not report the effort;
- requires each report to end with a JSON summary of its findings and what it
  checked, then a highest-severity line and a verdict. The verdict is worked out
  from the summary, the findings' tags and the footer together, so a report
  that lists a high finding, or checks nothing, cannot clear;
- keeps Sol's transcript out of Claude's reach, keeps only the two reports and
  session metadata unless `--keep` is given, and saves a clear receipt (pull
  request, target, merge base and head) for `--since` to check.

Edits to the script are reviewed by `main`'s copy like any other change. Only
the change that first adds the script, while `main` has no copy, may run with
`--bootstrap`; a person reads the script before running it, and that run saves
no receipt.

It exits 0 when both reviews are clear, 1 when either is not clear, and 2
when a review did not run or could not be checked.

**Clearance.** A review clears when it finished, read the change, and ended
with `Verdict: clear`, meaning no critical or high finding remains. Critical
and high findings block the merge: fix each one, or show that it is wrong.

**Lesser findings.** Medium and low findings do not block, so the agent decides
which ones are worth fixing, and should lean towards fixing them in the same
pull request before it merges. Listing findings in the pull request as
"follow-ups" does not track them; it only records technical debt. Leave a
finding unfixed only when you have decided it is not worth fixing, and record
that reason under **Reviews**, never a plan to fix it later.

**After changes.**
- When new commits are added on top of a cleared head, run both reviews with
  `--since <cleared head>` in the same mode as the cleared review. The script
  refuses unless this clone holds a clear receipt for that head, pull request
  (or local branch), target and merge base. A local receipt does not carry
  over to `--pr`: after the pull request opens, keep reviewing locally on the
  same branch, or run a full `--pr` review. Before counting a local review as
  the pull request's clearance, check against GitHub's view of the pull
  request (`gh pr view "$pr" --repo aiconsultancy/subspace --json
  headRefOid,baseRefName,baseRefOid`) that the reviewed head is exactly the
  pull request's head, that the reviewed target is its base branch (the remote
  you fetched must be the pull request's base repository, and the branch names
  must match), and that `git merge-base <headRefOid> <baseRefOid>` is the merge
  base that was reviewed, after fetching the base so that `baseRefOid` exists
  locally. This is a check you run by hand; when in doubt, run a full `--pr`
  review, which checks the same things itself.
- Any reused clearance, from a local or a `--pr` review, needs the merge base
  between the head and the pull request's base to be unchanged. If the base
  branch's history was rewritten, run both full reviews again.
- Run both full reviews again after any rebase or rewritten history, after a
  change that does more than fix findings, and whenever the pull request's
  target branch changes.
- Any other new head, including one that only re-signs or rewords commits,
  needs a review of its own, so that the script scans and reviews exactly what
  merges.

**Recording and merging.**
- Record every review under **Reviews** in the pull request: the range and
  target reviewed, each verdict, and how each finding was resolved.
- Before copying review text into the pull request, check it for secrets.
  The script scans both reports with gitleaks and says if it found anything.
- When the script names changed agent instructions, hooks, workflows, task
  runners (`prek.toml`, `mise.toml`, `Makefile`) or the script itself, a person
  reads those changes as well.
- Tick both review checkboxes only when both reviews of the final head are
  clear. Just before merging, check again (`gh pr view "$pr" --repo
  aiconsultancy/subspace --json headRefOid,baseRefName,baseRefOid`) that the
  base branch is the one reviewed and that the merge base between the head and
  the base is the one reviewed: it can move even without a force push, for
  example when the base gains a commit the head already contains. If either
  changed, review again. Then merge with `gh pr merge "$pr" --repo
  aiconsultancy/subspace --match-head-commit <final head>`. GitHub cannot bind
  the merge to a merge base, so keep that check immediately before the merge.
- Push commits in the form they will merge in (signed, reworded) before the
  final review, so the reviewed head is the pull request's head.

**Exceptions.**
- **Release Please.** Its pull requests and the release workflow's automated
  merges are exempt. A Release Please branch that gains any other commit loses
  the exemption: it needs both reviews and a merge by hand, not the nightly
  workflow.
- **Integration merges** (`merge/feat-to-main`) need no new review when every
  change they bring was cleared in its own pull request. Review anything not
  covered, such as conflict resolutions, direct commits or hand rebases.

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
- [Desktop agent skills](subspace-desktop/.agents/skills) and
  [Framework agent skills](subspace-framework/.agents/skills) hold the local
  checklists and specialised workflows.
