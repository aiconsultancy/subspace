# Actions board

Actions is an experimental board in the Subspace App. It is disabled by
default in debug and release builds. Enable **Settings > Actions > Enabled**,
and save. Alternatively set `{"boards":{"actions":true}}` in the
product's `config.json`. The board reads the favourite repositories (`repos`):
star them with **Repositories…** in the PRs board or add them in Settings.

## Use the board

- **Runs** groups run history by repository and workflow. Choose **Filters**
  to filter by status, branch, workflow or actor, or change grouping to status.
  Repository headers show which runs are still loading. When filters leave no
  matching runs, use **Clear filters** to see the loaded history.
- Open a run to load its jobs, steps and dependency graph. **Failed job log**
  reads a selected failed job's bounded log. Closing the detail panel discards it.
- **Deployments** shows each environment with its latest deployment and pending
  review records. Open a pending review to see reviewer availability and its run.
- **Releases** lists recent releases and tags without releases. A missing run
  marker means no matching completed run was found in the bounded loaded history.
- **Flow** shows the open run's jobs. Drag the background to pan, use the zoom
  controls or trackpad to zoom, and drag jobs to move them. Select a job for its
  steps. Fit and Reset restore the view. Arrow keys navigate jobs.
- Right-click a repository group header and choose **Refresh repository** to
  refresh that repository's workflow runs, deployments and releases.

The graph has one node per API job, including matrix variants. Dependencies
come from bounded workflow YAML. Ambiguous names, unreadable definitions and
invalid graphs produce an explained graph without dependency edges.

Trigger, Re-run, Re-run failed, Cancel run, Approve and Reject open a confirmation
for the selected run or pending deployment. No request is sent before Confirm.
The trigger form reads declared workflow inputs, including defaults, required
values, choices and booleans. If the definition is unavailable, it accepts only
a flat JSON object of strings and states that the inputs are unvalidated.
Reject requires a comment. Write errors keep the dialog open.

## Credentials and lifetime

Actions uses the existing GitHub credential priority: `GH_TOKEN` or
`GITHUB_TOKEN`, `github.token`, then the selected account's sign-in through
Subspace, then its gh CLI token. Read access
requires Actions, Contents and Deployments permissions. Writes require Actions
write or the corresponding classic token scopes. Reviews also require deployment
write permission and permission to review that environment.

The account picker keeps in-memory records separate per login and discards late
results after an account switch. Automatic updates load Actions at startup and
refresh runs, deployments and releases about every 10 minutes, even when the
board is hidden. PRs uses the same default background cadence. Active runs also
poll while the board is visible. Poll errors
back off from 30 seconds to at most 120 seconds. The board does not write a
record cache, index content for full-text search, or expose agent tools.
`/` searches loaded rows.

## Offline graph example

From `subspace-desktop`:

```bash
mise exec -- cargo run -p subspace-actions-board --example flow-graph -- dark
mise exec -- cargo run -p subspace-actions-board --example flow-graph -- light
```

The example uses fixture data and the same graph component. It makes no service
calls. The prototype can change or be removed.
