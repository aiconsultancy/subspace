//! Reconcile the ship feed's bounded task previews with fetched detail.

use subspace_client_fleet::model::Task;

use super::roster::task_revision;

// The Fleet ship feed caps these fields in code points, including the ellipsis.
const SUMMARY_LIMIT: usize = 180;
const PHASE_LIMIT: usize = 80;
const PROGRESS_LIMIT: usize = 400;

fn is_projection(full: &str, preview: &str, limit: usize) -> bool {
    full == preview
        || preview.strip_suffix('…').is_some_and(|prefix| {
            full.starts_with(prefix)
                && prefix.chars().count() == limit - 1
                && full.chars().nth(limit).is_some()
        })
}

pub(super) fn diverged(old: &Task, new: &Task, full: Option<&Task>) -> bool {
    let full = full.filter(|task| task.id == new.id);
    let fields = [
        (
            &old.summary,
            &new.summary,
            full.map(|task| &task.summary),
            SUMMARY_LIMIT,
        ),
        (
            &old.phase,
            &new.phase,
            full.map(|task| &task.phase),
            PHASE_LIMIT,
        ),
        (
            &old.progress,
            &new.progress,
            full.map(|task| &task.progress),
            PROGRESS_LIMIT,
        ),
    ];
    fields.into_iter().any(|(old, new, full, limit)| {
        old != new && !full.is_some_and(|text| is_projection(text, new, limit))
    })
}

pub(super) fn overlay_text(shown: &mut Task, old: &Task, new: &Task) {
    if task_revision(Some(shown)) == task_revision(Some(new)) {
        // The response may already include the revision that arrived in flight.
        return;
    }
    for (shown, old, new, limit) in [
        (
            &mut shown.summary,
            &old.summary,
            &new.summary,
            SUMMARY_LIMIT,
        ),
        (&mut shown.phase, &old.phase, &new.phase, PHASE_LIMIT),
        (
            &mut shown.progress,
            &old.progress,
            &new.progress,
            PROGRESS_LIMIT,
        ),
    ] {
        if old != new && !is_projection(shown, new, limit) {
            shown.clone_from(new);
        }
    }
}
