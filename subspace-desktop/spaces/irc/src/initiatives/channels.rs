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

/// The channel to show when `initiative` is chosen: its own first channel,
/// so two initiatives of one fleet each land in their own place; else its
/// fleet's recorded Starbase; else the first channel it joins.
pub(crate) fn focus_channel(initiative: &MemberInitiative) -> Option<String> {
    let starbase = || {
        let fleet = initiative.fleet.as_ref()?;
        channel_names(fleet.starbase_channel.as_deref()?)
            .into_iter()
            .next()
    };
    let joined = || initiative_channels(initiative).into_iter().next();
    channel_names(&initiative.channels)
        .into_iter()
        .next()
        .or_else(starbase)
        .or_else(joined)
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

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_api_client::model::MemberInitiative;

    use super::focus_channel;

    fn initiative(channels: &str, fleet: serde_json::Value) -> MemberInitiative {
        serde_json::from_value(json!({
            "id": "one", "name": "One", "channels": channels, "fleetId": "aurora", "fleet": fleet
        }))
        .unwrap()
    }

    #[test]
    fn choosing_an_initiative_shows_its_own_first_channel_else_the_starbase() {
        let fleet = json!({"id": "aurora", "slug": "aurora",
                           "starbaseChannel": "#aurora-starbase", "commandChannel": "#aurora-command"});
        assert_eq!(
            focus_channel(&initiative("#aurora-nav #aurora-map", fleet.clone())).as_deref(),
            Some("#aurora-nav"),
            "its own, not the fleet's first"
        );
        assert_eq!(
            focus_channel(&initiative("#aurora-relay", fleet.clone())).as_deref(),
            Some("#aurora-relay"),
            "a second initiative of the fleet lands in its own place"
        );
        assert_eq!(
            focus_channel(&initiative("", fleet)).as_deref(),
            Some("#aurora-starbase")
        );
        let command_only =
            json!({"id": "aurora", "slug": "aurora", "commandChannel": "#aurora-command"});
        assert_eq!(
            focus_channel(&initiative("", command_only)).as_deref(),
            Some("#aurora-command"),
            "without a Starbase, the first channel it joins"
        );
        let bare = json!({"id": "aurora", "slug": "aurora"});
        assert_eq!(
            focus_channel(&initiative("", bare)),
            None,
            "nothing to show"
        );
    }
}
