# Developer checklist skill

Back to the [change records](../README.md).

## Intent

The owner asked for a developer checklist that agents always run before
committing and pushing: the prek hooks are installed, and each kind of change
is routed to the rules it must follow (GPUI Kit guides and component rules for
UI, the board checklist for boards, the architecture rules for the framework,
and so on), instead of relying on agents to remember them.

## Spec

- A repository skill, `.agents/skills/developer-checklist/`, validated against
  the AgentSkills specification. `SKILL.md` is a short checklist in five
  steps: before you edit (sync, hooks), classify the change and read its
  rules, record and commit, gates before pushing, pull request. It links to
  the existing rules (AGENTS.md, docs/ways-of-working.md, docs/ui-components.md,
  docs/board-checklist.md, docs/board-integration.md) rather than copying them.
- `references/gotchas.md` records failures seen in this repository and their
  fixes (hooks run from worktrees, rejected pushes, `gh pr merge` deleting the
  head branch, signing over SSH, a full disk, zsh scripting, the Copilot CLI
  download).
- AGENTS.md gains "Before every commit and push", which tells agents to load
  the skill before every commit, push and pull request, including merges into
  `main`.

## Delivery

Docs only; no code changes. `uvx --from skills-ref agentskills validate`
passes, and every link anchor in the skill exists.

## Tasks

- [x] Write the skill and its gotchas reference.
- [x] Validate it.
- [x] Reference it from AGENTS.md.
