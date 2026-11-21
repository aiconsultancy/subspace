@AGENTS.md

## UI work

UI work in any Space follows the
[design guide](subspace-framework/docs/design-guide.md): the workspace
shell's zones and rules, built from the shared components. Compare your
screenshots with its shell anatomy image before asking for review.

## Pull request reviews from Claude Code

Before merge, obtain and record the independent Claude Opus 5.5 and GPT-6.2 Sol
reviews described in [Pull request review clearance](AGENTS.md#pull-request-review-clearance).
Use high effort for both. A subagent counts when it runs the named model and
reviews read-only. Use an isolated reviewer if the current session would run
hooks or settings from the change under review.

The repository's review script is optional. If its launcher cannot start a
model, use another read-only route for that model and record the actual review
under **Reviews** in the PR.
