//! Names in tool calls, resolved against the hub's state.

use subspace_client_irc::{BufferKey, ConnectionStatus, NetworkId, Target};

use crate::state::{IrcState, Network};

/// The configured network with the id `id`.
pub(super) fn network<'a>(state: &'a IrcState, id: &str) -> Result<&'a Network, String> {
    let networks = state.networks();
    if let Some(network) = networks.iter().find(|network| network.id.0 == id) {
        return Ok(network);
    }
    let known: Vec<&str> = networks
        .iter()
        .map(|network| network.id.0.as_str())
        .collect();
    Err(match known.is_empty() {
        true => "No IRC networks are configured.".to_owned(),
        false => format!(
            "No IRC network has the id {id}. Configured networks: {}.",
            known.join(", ")
        ),
    })
}

/// The buffer of the channel or nick `name` on `network`, compared without
/// ASCII case, with the spelling the runtime reported.
pub(super) fn buffer(state: &IrcState, network: &NetworkId, name: &str) -> Option<BufferKey> {
    state
        .buffers()
        .map(|(key, _)| key)
        .find(|key| {
            &key.network == network
                && target_name(&key.target).is_some_and(|known| known.eq_ignore_ascii_case(name))
        })
        .cloned()
}

/// The channel or nick of a buffer; `None` for the server buffer.
pub(super) fn target_name(target: &Target) -> Option<&str> {
    match target {
        Target::Channel(name) | Target::Query(name) => Some(name),
        Target::Server => None,
    }
}

pub(super) fn is_connected(network: &Network) -> bool {
    matches!(network.status, ConnectionStatus::Connected { .. })
}

/// `Acme IRC (local) as subspace`, for previews and errors.
pub(super) fn label(network: &Network) -> String {
    format!("{} ({}) as {}", network.name, network.id.0, network.nick)
}
