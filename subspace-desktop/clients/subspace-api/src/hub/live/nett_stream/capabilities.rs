//! What the organisation serves: its capability list read again, and the
//! Spaces told to read their initiatives again when it may have changed.
use super::super::{SLICE, StreamScope};
use crate::hub::{Shared, lock};
use std::sync::Weak;

pub(super) fn scope_epoch(weak: &Weak<Shared>) -> u64 {
    weak.upgrade()
        .map_or(0, |shared| lock(&shared.state).snapshot.scope_epoch)
}

/// Until the followed scope changes, the capability list is read again for
/// the stream, or any read finds it changed from `revision`, as an explicit
/// reload that finds access back may.
pub(super) async fn wait_for_scope_epoch_or_revision(
    weak: &Weak<Shared>,
    scope: &StreamScope,
    epoch: u64,
    (client, revision): (&crate::Client, Option<u64>),
) {
    while super::super::scope(weak).as_ref() == Some(scope)
        && scope_epoch(weak) == epoch
        && client.nett_served_revision() == revision
    {
        tokio::time::sleep(SLICE).await;
    }
}

/// What ended a wait while the stream is off.
pub(super) enum Woken {
    /// The followed scope changed, or the capability list was read again
    /// for the stream.
    Moved,
    /// The capability list is due to be read again.
    Due,
    /// A read elsewhere, such as an explicit reload's first team read,
    /// found the capability list changed.
    Changed,
}

/// Until the followed scope changes, the capability list is read again, the
/// list is due to be read again, or any read finds it changed from
/// `revision`, the revision the stream last saw.
pub(super) async fn wait_for_scope_epoch_or_capabilities(
    weak: &Weak<Shared>,
    scope: &StreamScope,
    epoch: u64,
    client: &crate::Client,
    revision: Option<u64>,
) -> Woken {
    // Without an organisation there is no list to read.
    let organisation = client.organisation().is_some();
    while super::super::scope(weak).as_ref() == Some(scope) && scope_epoch(weak) == epoch {
        if organisation && !client.nett_served_current() {
            return Woken::Due;
        }
        if client.nett_served_revision() != revision {
            return Woken::Changed;
        }
        tokio::time::sleep(SLICE).await;
    }
    Woken::Moved
}

/// Reads the organisation's capability list again, as it is due, and tells
/// the Spaces to read their initiatives again if it is not the revision the
/// stream saw, `seen`. Another read may have found the change first, in
/// which case this one finds none of its own.
pub(super) fn recheck_capabilities(weak: &Weak<Shared>, client: &crate::Client, seen: Option<u64>) {
    if client.nett_refresh_served().is_err() {
        client.nett_served_unreadable();
    }
    announce_if_changed(weak, client, seen);
}

/// Tells the Spaces to read their initiatives again if the capability list
/// is no longer the revision `seen`, whichever read changed it. A first
/// list is no change.
pub(super) fn announce_if_changed(weak: &Weak<Shared>, client: &crate::Client, seen: Option<u64>) {
    if seen.is_some() && client.nett_served_revision() != seen {
        announce(weak);
    }
}

/// Reads the organisation's capability list again and tells the Spaces to
/// read their initiatives again.
pub(super) fn refresh_scope(weak: &Weak<Shared>, client: &crate::Client) {
    let _ = client.nett_refresh_served();
    announce(weak);
}

/// Tells the Spaces what the organisation serves may have changed, so they
/// read their initiatives again.
pub(super) fn announce(weak: &Weak<Shared>) {
    let Some(shared) = weak.upgrade() else {
        return;
    };
    let mut state = lock(&shared.state);
    if state.is_running() {
        state.snapshot.scope_epoch = state.snapshot.scope_epoch.wrapping_add(1);
        state.changed();
    }
}

#[cfg(test)]
mod capabilities_tests;
