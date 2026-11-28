//! An optional wider search uses the list workers without replacing the visible list.
use super::{
    Hub, HubSnapshot, HubStatus, ReadPriority, ScopeGeneration, Shared, lock,
    worker::{Job, Task},
};
use crate::{ListQuery, model::IssueGroups};
use std::sync::Mutex;
use tokio::sync::oneshot;

pub(super) struct Probe {
    scoped: ListQuery,
    query: ListQuery,
    reply: Mutex<Option<oneshot::Sender<Option<WiderSearch>>>>,
}

/// Successful matches for a wider search, held until the caller adopts them.
pub struct WiderSearch {
    scoped: ListQuery,
    query: ListQuery,
    groups: IssueGroups,
    generation: ScopeGeneration,
    revision: u64,
}

impl Hub {
    /// Checks for wider matches while the specified scoped search is still settled and empty.
    pub async fn find_wider_matches(
        &self,
        scoped: &ListQuery,
        wider: &ListQuery,
    ) -> Option<WiderSearch> {
        if scoped == wider {
            return None;
        }
        let (reply, receive) = oneshot::channel();
        {
            let mut state = lock(&self.0.state);
            if state.query.as_ref() != Some(scoped) || !settled_empty(&state.snapshot) {
                return None;
            }
            let probe = Probe {
                scoped: scoped.clone(),
                query: wider.clone(),
                reply: Mutex::new(Some(reply)),
            };
            self.0
                .enqueue(&mut state, Task::SearchProbe(probe), ReadPriority::Visible);
        }
        receive.await.ok().flatten()
    }

    /// Installs those matches if the original search still owns the active list.
    pub fn adopt_wider_matches(&self, wider: WiderSearch) -> bool {
        let mut state = lock(&self.0.state);
        if !state.current(wider.generation)
            || state.revision != wider.revision
            || state.query.as_ref() != Some(&wider.scoped)
            || !settled_empty(&state.snapshot)
        {
            return false;
        }
        state.query = Some(wider.query.clone());
        state.revision = state.revision.wrapping_add(1);
        state.snapshot.page_loading.clear();
        state.snapshot.page_errors.clear();
        state.snapshot.hidden_subissues_count = None;
        state.install_list(wider.groups);
        state.changed();
        self.0.enqueue(
            &mut state,
            Task::ListCounts(wider.query),
            ReadPriority::LaterPage,
        );
        true
    }
}

fn settled_empty(snapshot: &HubSnapshot) -> bool {
    snapshot.status() == &HubStatus::Ready
        && !snapshot.list_loading()
        && snapshot
            .list()
            .is_some_and(|list| list.groups.iter().all(|group| group.count == 0))
}

pub(super) fn probe(shared: &Shared, job: &Job, probe: &Probe) {
    let Some(reply) = lock(&probe.reply).take() else {
        return;
    };
    if reply.is_closed() {
        return;
    }
    let result = shared.client.issue_groups(&probe.query);
    let found = result
        .ok()
        .filter(|list| list.groups.iter().any(|group| group.count > 0))
        .filter(|_| job.current(&lock(&shared.state)))
        .map(|groups| WiderSearch {
            scoped: probe.scoped.clone(),
            query: probe.query.clone(),
            groups,
            generation: job.generation,
            revision: job.revision,
        });
    _ = reply.send(found);
}
