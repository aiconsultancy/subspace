//! The channels an initiative's autojoin names.

use std::collections::HashSet;

use subspace_api_client::model::MemberInitiative;

const MAX_CHANNEL_BYTES: usize = 200;

/// The channels an initiative joins: its fleet's standard channels as the
/// fleet records them (the Starbase, the command channel, then each ship's),
/// then the initiative's own. A fleet that records none adds none: joining
/// a guessed name would create that channel on the server.
pub(crate) fn initiative_channels(initiative: &MemberInitiative) -> Vec<String> {
    let fleet = initiative.fleet.as_ref();
    let standard = fleet.into_iter().flat_map(|fleet| {
        let ships = fleet
            .ships
            .iter()
            .filter_map(|ship| ship.channel.as_deref());
        let agents = fleet
            .agents
            .iter()
            .filter_map(|agent| agent.ship_channel.as_deref());
        [
            fleet.starbase_channel.as_deref(),
            fleet.command_channel.as_deref(),
        ]
        .into_iter()
        .flatten()
        .chain(ships)
        .chain(agents)
    });
    let text: Vec<&str> = standard
        .chain(std::iter::once(initiative.channels.as_str()))
        .collect();
    channel_names(&text.join(","))
}

/// A JOIN accepts a single channel token. Store and show the first spelling,
/// with the same ASCII case-insensitive comparison as registration.
pub(crate) fn channel_names(text: &str) -> Vec<String> {
    let mut seen = HashSet::new();
    text.split(',')
        .flat_map(|entry| {
            let names = entry
                .trim()
                .split([' ', '\t'])
                .filter(|name| !name.is_empty());
            names
                .clone()
                .all(valid_channel)
                .then_some(names)
                .into_iter()
                .flatten()
        })
        .filter(|name| seen.insert(name.to_ascii_lowercase()))
        .map(str::to_owned)
        .collect()
}

fn valid_channel(name: &str) -> bool {
    // Leave room on the 512-byte IRC line for the JOIN command.
    (2..=MAX_CHANNEL_BYTES).contains(&name.len())
        && name.starts_with(['#', '&', '+', '!'])
        && !name.contains(|c: char| c.is_whitespace() || c.is_control() || c == ':')
}
