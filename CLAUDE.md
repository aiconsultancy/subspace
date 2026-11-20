@AGENTS.md

## Pull request reviews from Claude Code

Every pull request needs both model reviews before it merges. See
[Pull request review clearance](AGENTS.md#pull-request-review-clearance).
From Claude Code, run the trusted copy of `scripts/review-pr.sh` through
Bash, as that section shows. It starts the
Claude Opus 5.5 reviewer as its own `claude -p` agent at high effort, and the
GPT-6.2 Sol reviewer with the Codex CLI at high effort. Both are isolated and
can only read.

Do not run the Opus review as an in-session subagent instead: a subagent
inherits this session's hooks and permissions. Verify each finding before
acting on it, fix critical and high findings before merging, and record both
reviews under **Reviews** in the pull request.
