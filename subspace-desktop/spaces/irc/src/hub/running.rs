//! The networks the runtime runs: the saved settings without the channels
//! the user left, so a restart does not join them again. Pure, so it is
//! tested without a runtime.

use subspace_client_irc::{BufferKey, NetworkConfig, Target};

use crate::state::Removed;

/// `network` without the channels the user left on it.
fn without_left(network: &NetworkConfig, removed: &Removed) -> NetworkConfig {
    let mut network = network.clone();
    let id = network.id.clone();
    network
        .channels
        .retain(|channel| !removed.left(&id, channel));
    network
}

/// The networks to run at startup.
pub(crate) fn initial(networks: &[NetworkConfig], removed: &Removed) -> Vec<NetworkConfig> {
    networks
        .iter()
        .map(|network| without_left(network, removed))
        .collect()
}

/// The networks to run after the settings changed from `old` to `new`. A
/// network whose settings did not change keeps the configuration it runs,
/// so a channel left since does not restart its connection: the runtime
/// already stopped joining it.
pub(crate) fn next(
    (old, new): (&[NetworkConfig], &[NetworkConfig]),
    running: &[NetworkConfig],
    removed: &Removed,
) -> Vec<NetworkConfig> {
    new.iter()
        .map(|network| {
            let unchanged = old.contains(network);
            running
                .iter()
                .find(|run| unchanged && run.id == network.id)
                .cloned()
                .unwrap_or_else(|| without_left(network, removed))
        })
        .collect()
}

/// The channels a settings change from `old` to `new` adds to a network's
/// `channels`: the user wants them again, even when they left them.
pub(crate) fn added_channels(old: &[NetworkConfig], new: &[NetworkConfig]) -> Vec<BufferKey> {
    let mut added = Vec::new();
    for network in new {
        let before: Vec<String> = old
            .iter()
            .filter(|old| old.id == network.id)
            .flat_map(|old| old.channels.iter().map(|channel| channel_key(channel)))
            .collect();
        added.extend(
            network
                .channels
                .iter()
                .filter(|channel| !before.contains(&channel_key(channel)))
                .map(|channel| BufferKey {
                    network: network.id.clone(),
                    target: Target::Channel(channel_name(channel).to_owned()),
                }),
        );
    }
    added
}

/// The channel of a `channels` entry, `#ops` or `#ops key`.
fn channel_name(entry: &str) -> &str {
    entry.split_whitespace().next().unwrap_or_default()
}

fn channel_key(entry: &str) -> String {
    channel_name(entry).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use subspace_client_irc::NetworkConfig;

    use super::{added_channels, initial, next};
    use crate::fixtures::{channel, config};
    use crate::state::Removed;

    fn with_channels(channels: &[&str]) -> NetworkConfig {
        let mut network = config();
        network.channels = channels.iter().map(|name| (*name).to_owned()).collect();
        network
    }

    #[test]
    fn left_channels_are_not_joined_at_startup() {
        let mut removed = Removed::default();
        removed.insert(&channel("#Ops"), None);
        let networks = initial(&[with_channels(&["#ops secret", "#crew"])], &removed);
        assert_eq!(networks[0].channels, ["#crew"]);
        assert_eq!(networks[0].id, config().id, "nothing else changes");
    }

    #[test]
    fn an_unchanged_network_keeps_running_as_it_is() {
        let saved = [with_channels(&["#ops", "#crew"])];
        let running = initial(&saved, &Removed::default());
        let mut removed = Removed::default();
        removed.insert(&channel("#ops"), None);
        assert_eq!(
            next((&saved, &saved), &running, &removed),
            running,
            "leaving #ops since does not restart the connection"
        );
        let changed = [with_channels(&["#ops", "#crew", "#new"])];
        assert_eq!(
            next((&saved, &changed), &running, &removed)[0].channels,
            ["#crew", "#new"],
            "a changed network starts without the channels left"
        );
    }

    #[test]
    fn channels_added_in_settings_are_named() {
        let old = [with_channels(&["#ops"])];
        let new = [with_channels(&["#OPS key", "#crew"])];
        assert_eq!(added_channels(&old, &new), [channel("#crew")]);
        assert_eq!(added_channels(&[], &old), [channel("#ops")]);
    }
}
