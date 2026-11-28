# Native Space toolbars and inspectors

## Intent

Use the Issues layout as the reference for native list and detail toolbars.
Keep record commands in the detail toolbar and give Projects the same
navigator and inspector layout. Preserve the conversation as the Back
destination for pull requests opened from Channels.

## Specification

Open the [interactive mockups](mockups.html) in a browser and choose a Space.
Compare shows the list and detail together. List and Detail show each view at
a larger size. The mockups use fictional records and run locally.

| Space | List | Detail |
| --- | --- | --- |
| Issues | Search, filters and New issue | Back and the issue path, with Edit issue and More |
| Decisions | Search and Tag, with Accept taking the main slot for an eligible selection | Back and the decision reference, with revision history, available record actions and Refresh. Accept leads when eligible. |
| PRs | Search and repository filters | Back and the repository path, with compact secondary actions. Approve leads when a review is available. Merge leads when available after review or when review is unavailable. |
| Actions | Filters and Trigger | Back and the run reference, with Refresh and run commands in the inspector |
| Projects | Initiative, Overview and Milestones, with project properties in the inspector | Back to the view that opened the milestone, with milestone properties and issue progress in the inspector |
| Copilot Agents | Task filters | Back and the task's GitHub action when available |
| Channels | Conversation header, transcript and composer | Follow the PR in the transcript and use the detail toolbar's Back to return to the conversation. |

Inspector excerpts use the same size as property values. Decision link rows
start from a shared edge, including when their titles truncate.

## Delivery

The desktop checks cover the native and feature-gated Spaces. Headless
interaction tests cover contextual decision approval, inspector alignment,
project and milestone switching, inspector visibility and the pull request
return point. The capture build exercises the Channels round trip at the
shell level as well.

### Native captures

The comparisons show `main` on the left and the updated Space on the right.

- [Issues](evidence/issues-before-after.png)
- [Decisions](evidence/decisions-before-after.png)
- [PRs](evidence/prs-before-after.png)
- [Actions](evidence/actions-before-after.png)
- [Projects and milestone detail](evidence/projects-before-after.png)
- [Copilot Agents](evidence/copilot-agents-before-after.png)
- [PR Back destination from Channels](evidence/channel-return-before-after.png)

These macOS fixture captures use Twilight Two. The linked PR was opened from
the channel transcript, checked at an 880 px window width, then returned
to the same conversation via Back.

- [Proposed decision detail](evidence/decision-detail.png)
- [PR opened from Channels](evidence/pr-from-channels.png)
- [PR at a narrow width](evidence/pr-narrow.png)
- [Back in the originating conversation](evidence/back-in-channels.png)
- [Project properties in Overview](evidence/project-inspector.png)
- [Milestone properties and issue progress at a narrow width](evidence/milestone-inspector-narrow.png)

## Tasks and board checklist

Applies to Projects and PRs, which gain a new dock switch or a return
point integration. Issues, Decisions, Actions and Copilot Agents use their
existing shell integration for the presentation changes in this batch.

- [x] Identity and registration. Existing Space IDs, names, artwork, descriptors and registration are used. Process retention does not apply to these changes.
- [x] Workspace layout. The navigator, toolbar, pane and inspector use the shared shell. Reading columns use the shared width and text edge. The Projects toolbar uses the strip height to meet both dock headers.
- [x] Dock settings. Projects reports its inspector switch through the shell and saves visibility in preferences. Its project picker is the persistent navigator. PRs uses its existing dock settings and hidden-navigator view menu.
- [x] Components and actions. Existing buttons, facts, rows, menus and list hooks supply the UI. Projects uses its planning timeline and milestone list. No shared component was added. Existing shortcuts, status, busy state and modal handling apply. Projects shows project or milestone properties, and the inspector handles loading and unavailable records.
- [x] Settings, storage and refresh. No connection, feature flag, data cache or background policy changes apply. Projects uses the loaded record for its inspector and restores properties to the page when the inspector is hidden.
- [x] Search, tools, chat and notifications. Their scopes and interfaces are unchanged because the changes apply to visible toolbar, navigation and inspector state.
- [x] Help and reference navigation. The Projects and Decisions guides describe the new layout. PR Back uses the existing shell return-point contract. No Space depends on another Space's crate or reads its preferences.
- [x] Diagnostics and writes. The existing record states, handlers and error paths apply. No new logging or API writes are involved.
- [x] Tests and evidence. Desktop checks, headless interaction tests and fictional Twilight Two captures cover the changed paths.
