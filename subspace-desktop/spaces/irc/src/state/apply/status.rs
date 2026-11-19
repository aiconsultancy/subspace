use chrono::{DateTime, Utc};
use subspace_client_irc::{BufferKey, ConnectionStatus, Line, LineKind, NetworkId};

use super::{IrcState, append, network_mut};
use crate::state::server_key;

/// A status change, with a local error line in the server buffer when the
/// connection failed, so the reason stays readable after it changes again.
pub(super) fn apply(
    state: &mut IrcState,
    id: &NetworkId,
    status: ConnectionStatus,
    now: DateTime<Utc>,
) {
    let error = failure(&status);
    let Some(network) = network_mut(state, id) else {
        return;
    };
    if let ConnectionStatus::Connected { nick } = &status {
        network.nick = nick.clone();
    }
    network.status = status;
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
