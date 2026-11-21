# Workspace shell review findings

Back to the [change records](../README.md).

Pull request #29 put every Space in one workspace shell, with Twilight Two
and the embedded fonts ([Workspace shell](../workspace-shell/intent.md)). It
merged with its review findings open: one high and 24 medium and 17 low
findings from Claude Opus 5.5 and GPT-6.2 Sol, and a P3 and a nit from the
fleet review. The owner chose to merge, because Decisions is off by default
in release builds and there are no users yet, and to take every finding in a
follow-up pull request, with the Decisions account-switch fix first.

This is that follow-up. Each finding was checked against the current code,
not the diff slice the reviewer saw, and then fixed, refuted as not a
defect, or left with a reason. Each behavioural fix has a headless test that
fails without it.

First, the high finding (S12, the fleet review's F1 and Claude Opus 5.5's
O1): the Decisions Space kept the previous account's list, open page,
directory and tags after an account or organisation change or a sign-out,
until a reload. It now drops all of it as soon as the account changes, as
the Issues Space restarts its hub. While fixing it, a related gap turned up
in Issues: its Back state outlived a hub restart (N1).

The other fixes are in the Framework's shared kit and host app, Channels,
Pull requests, Actions, Copilot Agents, the release packaging scripts and the
agent guidance. The Framework's part has
[its own record](../../../../subspace-framework/docs/changes/workspace-shell-review-findings/README.md).

See the [behaviour](spec.md), [verification](delivery.md) and
[checklist](tasks.md).
