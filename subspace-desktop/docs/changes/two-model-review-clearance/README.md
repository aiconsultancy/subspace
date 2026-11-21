# Two-model pull request review clearance

This record describes the original review helper. The
[current review rules](../../../../AGENTS.md#pull-request-review-clearance)
require both model verdicts and allow read-only subagents. The script is optional.

## Intent

Several pull requests were merged without an independent code review; the reviews run afterwards found a high-severity defect and more than twenty smaller ones. Every pull request should now be cleared by two different models before it merges.

## Spec

- Every pull request needs a clear review from Claude Opus 5.5 at high effort and from GPT-6.2 Sol at high effort, recorded in the pull request. Release Please pull requests and fully covered integration merges are exempt.
- The trusted copy of `scripts/review-pr.sh` (from the target branch) runs both reviews the same way from Claude Code and Codex:
  - GPT-6.2 Sol has no shell or file access and reviews the inline diff.
  - Claude Opus 5.5 is confined to the diff and a snapshot built from committed files.
  - Nothing from the change under review is executed or loaded as instructions: no checkout hooks, filters, external diff drivers, text conversion, attribute tricks, project or user settings, hooks, rules, MCP servers or inherited environment.
  - The title and author notes reach the reviewers only as untrusted data.
  - The script checks each review's model, completion and verdict, and Sol's effort.
- The pull request template and the ways-of-working checklist gain one merge-gate checkbox per review and a **Reviews** table.

## Delivery

- `AGENTS.md` holds the rule and how to run the script; `CLAUDE.md` tells Claude Code to run the script rather than an in-session subagent, which would inherit the session's hooks and permissions.
- Verified on this machine with Codex CLI 0.170.0 and Claude Code 2.1.325:
  - all flags exist, and briefs pass on stdin;
  - Claude in restricted mode reads only the review folders, with no Bash or MCP tools;
  - Codex runs with no inherited environment, and its log names the model and effort.
- A plain `claude -p` run inside a folder with a `.claude/settings.json` hook executed that hook. With the script's flags, the hook did not run.
- The rule was applied to its own pull request with this branch's script and `--bootstrap`, because the target branch has no copy yet. A person read the script; the clearances are recorded in the pull request.
- Two stale links in the pull request template now point at the Framework's component rules and board checklist.

## Tasks

- [x] Rule in `AGENTS.md`, Claude Code note in `CLAUDE.md`.
- [x] `scripts/review-pr.sh`.
- [x] Pull request template and ways-of-working checkboxes, and the **Reviews** table.
