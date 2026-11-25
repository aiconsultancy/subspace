# Projects verification

The native Twilight Two capture uses an isolated Subspace API fixture and fictional project data.

- [Project overview before centering](evidence/overview-before.png) and [centered overview](evidence/overview.png)
- [Milestones before centering](evidence/milestones-before.png) and [centered milestones](evidence/milestones.png)
- [New milestone form](evidence/editor.png)
- [Switching initiatives in the early layout](evidence/initiative.png)

The live Subspace API returned the project list, project details, milestone list and latest update routes. Milestone changes are exercised against fixture HTTP endpoints for both API backends.

The native follow-up fixture saved Started → Paused, previewed linked issues on the timeline, opened a milestone and its linked issue, and returned through Back to the milestone and then the list. The client tests cover the versioned project PUT on both backends and paging through linked issues with their statuses.

- [Edit project](evidence/project-editor.png) and [saved status](evidence/project-paused.png)
- [Timeline preview](evidence/timeline-preview.png) and [milestone detail after Back](evidence/milestone-detail.png)
