use std::ops::Range;

use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, ConnectionStatus, Line, LineKind, NetworkId, Target};

use super::{IrcState, append, network_mut};
use crate::state::server_key;

/// A status change, with a local error line in the server buffer when the
/// connection failed, so the reason stays readable after it changes again.
/// Any state but connected drops every nick status of the network, whether
/// or not the runtime's clears arrived. Channel membership belongs to the
/// connection as well.
pub(super) fn apply(
    state: &mut IrcState,
    id: &NetworkId,
    status: ConnectionStatus,
    now: DateTime<Utc>,
) {
    let error = failure(&status);
    let held = state.first_seq..state.next_seq;
    let Some(network) = network_mut(state, id) else {
        return;
    };
    if let ConnectionStatus::Connected { nick } = &status {
        network.nick = nick.clone();
    } else {
        network.statuses.clear();
    }
    network.held_at_connect = held_at_connect(network.held_at_connect.take(), &status, held);
    network.status = status;
    if !matches!(network.status, ConnectionStatus::Connected { .. }) {
        for (key, buffer) in &mut state.buffers {
            if &key.network == id && matches!(key.target, Target::Channel(_)) {
                buffer.joined = false;
            }
        }
    }
    if let Some(error) = error {
        let key = server_key(id);
        let repeated = state
            .buffers
            .get(&key)
            .and_then(|buffer| buffer.entries.back())
            .is_some_and(|entry| entry.line.kind == LineKind::Error && entry.line.text == error);
        if !repeated {
            append(state, local_error(key, error, now));
        }
    }
}

/// The lines held when the connection `status` reports began: kept while it
/// stays connected, `held` (every sequence held now) when it has just
/// connected, `None` otherwise.
fn held_at_connect(
    current: Option<Range<i64>>,
    status: &ConnectionStatus,
    held: Range<i64>,
) -> Option<Range<i64>> {
    match status {
        ConnectionStatus::Connected { .. } => Some(current.unwrap_or(held)),
        _ => None,
    }
}

fn failure(status: &ConnectionStatus) -> Option<String> {
    match status {
        ConnectionStatus::Failed { error } | ConnectionStatus::Reconnecting { error, .. }
            if !error.is_empty() =>
        {
            Some(error.clone())
        }
        _ => None,
    }
}

fn local_error(buffer: BufferKey, text: String, now: DateTime<Utc>) -> Line {
    Line::new(buffer, now, LineKind::Error, text)
}
