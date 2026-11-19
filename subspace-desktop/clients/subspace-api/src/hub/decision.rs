//! A single, issue-scoped detail read for the open decision panel.
use super::worker::{Job, Task};
use super::{
    Hub, HubError, IssueSnapshot, IssueStatus, ReadError, ReadPriority, Shared, State, lock,
};
use crate::{Error, model::DecisionDetail};
use std::sync::Arc;

#[cfg(test)]
mod reload_tests;

#[derive(Clone, Default)]
pub(super) struct Slot {
    generation: u64,
    read: Option<Arc<DecisionRead>>,
}

struct DecisionRead {
    id: String,
    status: DecisionStatus,
}

pub enum DecisionStatus {
    Loading,
    Loaded(Box<DecisionDetail>),
    Gone,
    Error(ReadError),
}

pub(super) struct DecisionRequest {
    generation: u64,
    id: String,
}

impl IssueSnapshot {
    pub fn decision(&self, id: &str) -> Option<&DecisionStatus> {
        self.decision
            .read
            .as_ref()
            .filter(|read| read.id == id)
            .map(|read| &read.status)
    }
}

impl Hub {
    pub fn open_decision(&self, id: &str) -> Result<(), HubError> {
        self.request_decision(id, false)
    }

    pub fn retry_decision(&self, id: &str) -> Result<(), HubError> {
        self.request_decision(id, true)
    }

    fn request_decision(&self, id: &str, retry: bool) -> Result<(), HubError> {
        crate::transport::validate_segments(std::iter::once(id))?;
        let mut state = lock(&self.0.state);
        state.running()?;
        enqueue(&self.0, &mut state, id);
        if retry
            && state.snapshot.issue.as_ref().is_some_and(|issue| {
                issue.can_read_decisions() && issue.status == IssueStatus::Error(ReadError::Network)
            })
        {
            self.0.reload_open_issue(&mut state);
        }
        Ok(())
    }

    pub fn close_decision(&self) {
        let mut state = lock(&self.0.state);
        if let Some(issue) = state.snapshot.issue.as_mut()
            && issue.decision.read.take().is_some()
        {
            issue.decision.generation = issue.decision.generation.wrapping_add(1);
            state.changed();
        }
    }
}

fn enqueue(shared: &Shared, state: &mut State, id: &str) {
    let Some(issue) = state
        .snapshot
        .issue
        .as_mut()
        .filter(|i| i.can_read_decisions())
    else {
        return;
    };
    let ready = issue.status == IssueStatus::Loaded;
    let slot = &mut issue.decision;
    slot.generation = slot.generation.wrapping_add(1);
    slot.install(id, DecisionStatus::Loading);
    let request = DecisionRequest {
        id: id.into(),
        generation: slot.generation,
    };
    state.changed();
    if ready {
        shared.enqueue(state, Task::Decision(request), ReadPriority::Visible);
    }
}

pub(super) fn resume_pending(shared: &Shared, state: &mut State) {
    let pending = state
        .snapshot
        .issue
        .as_ref()
        .and_then(|issue| issue.decision.read.as_ref())
        .cloned();
    if let Some(read) = pending
        && matches!(read.status, DecisionStatus::Loading)
    {
        enqueue(shared, state, &read.id);
    }
}

pub(super) fn current(state: &State, request: &DecisionRequest) -> bool {
    state.snapshot.issue.as_ref().is_some_and(|issue| {
        issue.status == IssueStatus::Loaded
            && issue.decision.generation == request.generation
            && issue.decision.read.is_some()
    })
}

pub(super) fn load(shared: &Shared, job: &Job, request: &DecisionRequest) {
    let status = match shared.client.decision(&request.id) {
        Ok(detail) => DecisionStatus::Loaded(Box::new(detail)),
        Err(Error::NotFound) => DecisionStatus::Gone,
        Err(error) => DecisionStatus::Error(error.into()),
    };
    let mut state = lock(&shared.state);
    if job.current(&state)
        && let Some(issue) = state.snapshot.issue.as_mut()
    {
        issue.decision.install(&request.id, status);
        state.changed();
    }
}

impl Slot {
    fn install(&mut self, id: &str, status: DecisionStatus) {
        self.read = Some(Arc::new(DecisionRead {
            id: id.into(),
            status,
        }));
    }
}
