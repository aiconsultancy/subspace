use super::{Job, Shared, Task, lock};
use crate::{
    Error, PageRequest,
    hub::{HubStatus, IssueStatus, ReadPriority},
    model::{Project, WorkflowState},
};
use std::{collections::HashSet, time::Instant};

pub(super) fn bootstrap(shared: &Shared, job: &Job) {
    let started_at = Instant::now();
    let me = shared.client.me();
    let projects = me
        .as_ref()
        .map_err(|error| *error)
        .and_then(|_| shared.client.all_projects());
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if state
        .snapshot
        .identity
        .as_ref()
        .is_some_and(|(seen, _)| *seen > started_at)
    {
        resume_reads(shared, &mut state, job);
        return;
    }
    let identity = me
        .as_ref()
        .map(|me| me.user_id.clone())
        .map_err(|error| *error);
    let result = me.and_then(|me| projects.map(|projects| (me, projects)));
    let retire = state.retire_if_account_changed(&identity, result.as_ref().err().copied());
    state.snapshot.identity = Some((started_at, identity));
    match result {
        Ok((me, mut projects)) => {
            if matches!(job.task, Task::ReloadProjects) && !retire {
                retain_workflow_states(&mut projects, &mut state.snapshot.projects);
            }
            state.snapshot.me = Some(me);
            state.snapshot.projects = projects;
            state.project_refresh.replaced();
            state.snapshot.status = HubStatus::Ready;
            state.initial_access_retry = false;
            if retire {
                enqueue_open(shared, &mut state);
            } else {
                resume_reads(shared, &mut state, job);
            }
        }
        Err(error) => {
            let reason: crate::hub::ReadError = error.into();
            if let Some(issue) = state
                .snapshot
                .issue
                .as_mut()
                .filter(|issue| issue.detail.is_none())
            {
                issue.status = IssueStatus::Error(reason.clone());
            }
            state.snapshot.status = HubStatus::Error(reason);
            state.snapshot.list_loading = false;
        }
    }
    state.changed();
}

pub(super) fn refresh_projects(shared: &Shared, job: &Job, revision: u64) {
    let projects = shared.client.all_projects();
    let mut state = lock(&shared.state);
    if !job.current(&state) {
        return;
    }
    if state.project_refresh.finish(revision, Instant::now())
        && let Ok(mut projects) = projects
    {
        retain_workflow_states(&mut projects, &mut state.snapshot.projects);
        state.snapshot.projects = projects;
        state.project_refresh.replaced();
        state.changed();
    }
}

fn retain_workflow_states(fresh: &mut [Project], cached: &mut [Project]) {
    for project in fresh {
        if let Some(previous) = cached.iter_mut().find(|previous| {
            previous.id == project.id
                && previous.key == project.key
                && previous.default_state_id == project.default_state_id
                && previous.archived_at == project.archived_at
        }) {
            project.states = std::mem::take(&mut previous.states);
        }
    }
}

fn resume_reads(shared: &Shared, state: &mut super::State, job: &Job) {
    if state.snapshot.status != HubStatus::Ready {
        return;
    }
    if matches!(job.task, Task::Bootstrap) {
        enqueue_open(shared, state);
    } else {
        resume_activity(shared, state);
    }
}

fn enqueue_open(shared: &Shared, state: &mut super::State) {
    if let Some(query) = state.query.clone() {
        state.snapshot.list_loading = true;
        shared.enqueue(state, Task::List(query), ReadPriority::Visible);
    }
    if let Some(issue) = state
        .snapshot
        .issue
        .as_mut()
        .filter(|issue| issue.profile_id == crate::PROFILE_ID)
    {
        issue.status = IssueStatus::Loading;
        issue.decisions.pause();
        let identifier = issue.identifier.clone();
        shared.enqueue(state, Task::Issue(identifier), ReadPriority::Visible);
    }
    resume_activity(shared, state);
}

fn resume_activity(shared: &Shared, state: &mut super::State) {
    if let Some(query) = state.pending_activity.take() {
        shared.begin_activity(state, query);
    }
}

pub(super) fn ensure_states(shared: &Shared, job: &Job, project: &str) -> Result<(), Error> {
    if lock(&shared.state)
        .snapshot
        .projects
        .iter()
        .any(|p| p.key == project && !p.states.is_empty())
    {
        return Ok(());
    }
    let states = state_pages(&shared.client, project)?;
    let mut state = lock(&shared.state);
    if job.current(&state)
        && let Some(project) = state
            .snapshot
            .projects
            .iter_mut()
            .find(|p| p.key == project)
    {
        project.states = states;
        state.changed();
    }
    Ok(())
}

fn state_pages(client: &crate::Client, project: &str) -> Result<Vec<WorkflowState>, Error> {
    let mut request = PageRequest {
        limit: crate::query::MAX_LIST_PAGE_SIZE,
        cursor: None,
    };
    let mut states = vec![];
    let mut cursors = HashSet::new();
    loop {
        let page = client.workflow_states(project, &request)?;
        states.extend(page.items);
        let Some(cursor) = page.next_cursor else {
            return Ok(states);
        };
        if !cursors.insert(cursor.clone()) {
            return Err(Error::InvalidResponse);
        }
        request.cursor = Some(cursor);
    }
}
