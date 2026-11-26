# Merge a pull request from Subspace

The PR page and inspector refresh the visible pull request every 25 seconds.
The Merge button follows GitHub's current mergeability and the selected
account's repository permissions. After that account approves, Merge takes
the emphasis when available. The confirmation shows the merge method
and base branch. GitHub receives the reviewed head SHA so it can refuse a
merge if newer commits arrive before submission.

The native tests exercise the polling cadence, state changes, confirmation
and merge request against a fixture server. See the [PRs guide](../../../spaces/github/help.md#merge-a-pull-request)
for the workflow.

## Captures

Twilight Two against the fixture GitHub server.

- [Before](evidence/before.png) shows the PR toolbar without a Merge button.
- [Ready](evidence/ready.png) shows Merge enabled.
- [Confirmation](evidence/confirm.png) shows the chosen method and base branch.
- [Blocked](evidence/blocked.png) shows Merge disabled while a check is failing.
- [Approved](evidence/approved.png) shows Merge highlighted after the signed-in account approves.
