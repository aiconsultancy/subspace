# Actions board

Actions is an experimental board in the Subspace App. It is disabled by
default in debug and release builds. Enable **Settings > Actions > Enabled**,
and save. Alternatively set `{"boards":{"actions":true}}` in the
product's `config.json`. The board reads the favourite repositories (`repos`):
star them with **Repositories…** in the PRs board or add them in Settings.

## Layout

Actions uses the shared workspace shell
(`subspace_framework::dashboard::workspace`), composed in
`spaces/actions/src/view.rs`:

| Zone | Source | Holds |
| --- | --- | --- |
| Navigator | `navigator.rs` | The title **Actions**; **Views** (Runs, Deployments, Releases, Flow) as rows that call `set_view`; the groups of the list on show (**Workflows**, **Statuses** or **Repositories**) with their counts, where a click unfolds, selects and scrolls to the group; the signed-in account as the footer. |
| Tab strip | `open::tab_bar` | A tab named after the current view, then, once an item is opened, a closable tab for it (`Run <id>`). |
| Toolbar | `toolbar::render` | Leading: the view menu (only while the navigator is hidden), **Filters**, the account picker and **Failed job log**. Trailing: **Trigger…**. The dock switches are the status bar's (`Dashboard::docks`). Leading controls wrap in a narrow pane; there is no overflow menu. |
| Content | `view.rs`, `lanes.rs`, `open.rs` | The shared list, drawn with `list::render_under` under `workspace::pane_head`, with run rows' lanes from `ListDashboard::row_leading` and `row_trailing` (`lanes.rs`); the Flow graph in a `workspace::Pane`; or, while its tab shows, the opened item's page (`open::page`) at `workspace::READING_WIDTH_PX`. |
| Inspector | `inspector.rs` | A `workspace::Inspector` in `Workspace::inspector`. For the selection: `Run <id>` with **Open**, **Open on GitHub** and **Close inspector**; the status (glyph, words and tone), title and what a waiting run waits on; **Details**; **Jobs**; **Environment** with reviewers; and a footer with `Operation::FOOTER` (**Approve**, primary while a review waits, **Reject**, **Re-run**, **Re-run failed**, **Cancel run**), enabled by `writes::unavailable`. In Flow a selected job takes its place. The shared list's own detail panel is unused: `ListDashboard::detail` returns `None`. |
| Status bar | `Actions::status_text` | `GitHub · <account> · <n> runs · <n> active · <status filter> · <age>`, with `run history truncated` when a repository's history is cut. |

There is no large heading. `docks.rs` keeps whether the navigator and the
inspector show, as the preferences `actions.navigator` and
`actions.inspector` (`shown` or `hidden`, shown by default). Group headers
start at `workspace::text_edge`, the edge the toolbar and the navigator use,
and rows are indented from it. The list keeps 320 px beside the inspector
(`ListDashboard::min_list_width`).

### Select and open

`ListDashboard::activates_on_click` is `false`, so one click on a row only
selects it; a double click or Enter activates it. Group headers fold on one
click, as the shared list always does.

- **Select** (`on_selection_changed`): a record with a run calls
  `load_detail`, which reads the run's jobs and its workflow definition in the
  background, the two reads opening a run made before. The result fills
  `Actions::detail`, which the inspector and Flow show.
- **Open** (`on_open`, reached from Enter, a double click, the inspector's
  **Open** through `list.reveal`, or a search result): closes the list's open
  flag again at once, so moving the selection opens nothing more, and calls
  `open_tab`. The tab (`open::OpenTab`) keeps its own copy of the run's
  detail; `load_detail` fills every copy whose target matches. Opening reads
  nothing new unless the selection's jobs were never read.
- **Failed job log** reads one job's log (`load_job_log`) for the selected run,
  opens that run's tab and shows the log there. Closing the tab discards it.

Every write is also a Commands entry while it applies to the selected item
(`keys::write_commands`), so the writes stay reachable when the inspector is
hidden. **Re-run failed** and **Reject** have actions but no key.

## Use the board

- Choose a view in the navigator, or with the keys `1` to `4`.
- **Runs** groups run history by repository and workflow. Choose **Filters**
  to filter by status, branch, workflow or actor, or change grouping to status.
  Repository headers show which runs are still loading. When filters leave no
  matching runs, use **Clear filters** to see the loaded history.
- Select a run to load its jobs and dependency graph into the inspector and
  Flow; open it for its full page in a tab. **Failed job log** reads a selected
  failed job's bounded log into that tab. Closing the tab discards it.
- **Deployments** shows each environment with its latest deployment and pending
  review records. Select a pending review to see its environment, reviewers and run.
- **Releases** lists recent releases and tags without releases. A missing run
  marker means no matching completed run was found in the bounded loaded history.
- **Flow** shows the selected run's jobs. Drag the background to pan, use the zoom
  controls or trackpad to zoom, and drag jobs to move them. Select a job for its
  steps. Fit and Reset restore the view. Arrow keys navigate jobs.
- Right-click a repository group header and choose **Refresh repository** to
  refresh that repository's workflow runs, deployments and releases.

The graph has one node per API job, including matrix variants. Dependencies
come from bounded workflow YAML. Ambiguous names, unreadable definitions and
invalid graphs produce an explained graph without dependency edges.

Trigger, Re-run, Re-run failed, Cancel run, Approve and Reject open a confirmation
for the selected run or pending deployment. No request is sent before Confirm.
**Trigger…** is in the toolbar; the others are in the inspector's footer for
the selected run, and all six are in Commands and (except Re-run failed and
Reject) on a key.
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
