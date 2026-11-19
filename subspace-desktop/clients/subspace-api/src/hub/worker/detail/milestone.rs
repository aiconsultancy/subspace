//! Resolve secondary names through the normal low-priority read queue.
use super::{Job, Shared, lock};
use crate::hub::{ReadPriority, State, worker::Task};

pub(super) fn queue(shared: &Shared, state: &mut State) {
    let selected = state
        .snapshot
        .issue
        .as_ref()
        .and_then(|issue| issue.detail.as_ref())
        .and_then(|detail| {
            detail
                .fields
                .milestone_id
                .as_ref()
                .map(|id| (detail.fields.project_key.clone(), id.clone()))
        });
    if let Some((project, id)) = selected {
        shared.enqueue(state, Task::Milestone(project, id), ReadPriority::LaterPage);
    }
}

pub(in crate::hub::worker) fn load(shared: &Shared, job: &Job, project: &str, id: &str) {
    if !job.current(&lock(&shared.state)) {
        return;
    }
    let result = shared.client.issue_milestone_name(project, id);
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if let Some(issue) = state.snapshot.issue.as_mut()
        && issue
            .detail
            .as_ref()
            .is_some_and(|detail| detail.fields.milestone_id.as_deref() == Some(id))
    {
        if let Ok(name) = result {
            issue.milestone_name = name;
        }
        issue.milestone_loading = false;
        state.changed();
    }
}

#[cfg(test)]
mod tests {
    use crate::hub::{
        lock,
        worker::{Job, Task},
    };
    use crate::tests::{common, connected};
    use subspace_framework::client_test_support::Mock;

    #[test]
    fn queued_names_are_retired_by_issue_reload_and_close() {
        let mock = Mock::new(common);
        let hub = connected(&mock);
        let job = Job::new(
            &lock(&hub.0.state),
            Task::Milestone("SUB".into(), "milestone-1".into()),
        );
        assert!(job.current(&lock(&hub.0.state)));
        hub.reload_issue().unwrap();
        assert!(!job.current(&lock(&hub.0.state)));
        let current = Job::new(
            &lock(&hub.0.state),
            Task::Milestone("SUB".into(), "milestone-1".into()),
        );
        hub.close_issue();
        assert!(!current.current(&lock(&hub.0.state)));
        hub.stop();
    }
}
