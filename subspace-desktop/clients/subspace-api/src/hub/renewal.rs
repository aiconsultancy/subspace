//! A NETT token refused before it expires. The hub's reads and its update
//! stream share one renewal: a refused read renews the token once, and once
//! it is renewed the list and the open issue are read again with it. Only a
//! 403, a renewed token refused again or a renewal that fails clears what
//! is shown. Subspace API keeps its own handling.

use super::{HubStatus, IssueStatus, ReadError, ScopeGeneration, Shared, State, lock};
use crate::Error;
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Renewal {
    #[default]
    Idle,
    Renewing,
    Renewed(Instant),
}

/// A renewed token refused again within this counts as a failed renewal.
const TRUSTED: Duration = Duration::from_secs(60);

/// Whether a read refused with `error`, sent at `sent`, stands: a 403, a
/// renewed token refused again or a renewal that failed. A 401 that a
/// renewal answers does not: the reads are queued again instead. Called
/// without the hub's state lock; it may wait for the renewal.
pub(super) fn stands(
    shared: &Shared,
    generation: ScopeGeneration,
    sent: Instant,
    error: Error,
) -> bool {
    if error != Error::SignInNeeded || shared.client.service() != crate::Backend::Nett {
        return true;
    }
    {
        let mut state = lock(&shared.state);
        if !state.current(generation) {
            return true;
        }
        match state.renewal {
            // Another read is renewing; its reads again cover this one.
            Renewal::Renewing => return false,
            // Sent with the token before the renewal: read again already.
            Renewal::Renewed(at) if sent < at => return false,
            Renewal::Renewed(at) if at.elapsed() < TRUSTED => return true,
            _ => state.renewal = Renewal::Renewing,
        }
    }
    let renewed = crate::native::renew_token();
    let mut state = lock(&shared.state);
    if !state.current(generation) {
        return true;
    }
    if renewed {
        state.renewal = Renewal::Renewed(Instant::now());
        reload(shared, &mut state);
        false
    } else {
        state.renewal = Renewal::Idle;
        true
    }
}

/// The update stream renewed the token: the list and the open issue are
/// read again with it.
pub(super) fn renewed(shared: &Shared, generation: ScopeGeneration) {
    let mut state = lock(&shared.state);
    if !state.current(generation) {
        return;
    }
    state.renewal = Renewal::Renewed(Instant::now());
    reload(shared, &mut state);
}

/// Reads the list and the open issue again. What is shown is kept and read
/// again in place; a first read that never arrived is read whole.
fn reload(shared: &Shared, state: &mut State) {
    if state.snapshot.me.is_none() {
        state.snapshot.status = HubStatus::Loading;
        state.snapshot.list_loading = false;
        state.changed();
        shared.enqueue(state, super::Task::Bootstrap, super::ReadPriority::Visible);
        return;
    }
    let refused = state.snapshot.status == HubStatus::Error(ReadError::SignInNeeded);
    if state.snapshot.list.is_some() && !refused {
        super::live::reread_list(shared, state);
    } else if let Some(query) = state.query.clone() {
        state.revision = state.revision.wrapping_add(1);
        state.snapshot.list_loading = true;
        state.snapshot.status = HubStatus::Loading;
        state.changed();
        shared.enqueue(
            state,
            super::Task::List(query),
            super::ReadPriority::Visible,
        );
    }
    let loaded = state
        .snapshot
        .issue
        .as_ref()
        .filter(|issue| issue.profile_id == crate::PROFILE_ID)
        .map(|issue| issue.status == IssueStatus::Loaded);
    match loaded {
        Some(true) => super::live::reread_issue(shared, state),
        Some(false) => shared.reload_open_issue(state),
        None => {}
    }
}
