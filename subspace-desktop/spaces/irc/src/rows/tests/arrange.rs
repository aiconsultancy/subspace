//! How a network's groups are arranged: one fleet's network as one group
//! named after the fleet, several fleets with the network's own group last,
//! and that group never named like a fleet.

use serde_json::json;
use subspace_client_irc::Event;

use super::super::{group_key, layout};
use super::fleets::{initiative, outline, owned, rosters};
use crate::fixtures::{at, channel, config, network_id};
use crate::state::IrcState;

/// The fixture network, named `name`, with `channels` joined.
fn network_named(name: &str, channels: &[&str]) -> IrcState {
    let mut network = config();
    network.name = name.to_owned();
    let mut state = IrcState::new(&[network], Vec::new());
    for name in channels {
        state.apply(
            Event::Joined {
                buffer: channel(name),
            },
            at(0),
        );
    }
    state
}

fn two_fleets() -> super::super::Rosters {
    rosters(json!([
        initiative("aurora", "Aurora", &["lyra"], ""),
        initiative("borealis", "Borealis", &["vega"], ""),
    ]))
}

#[test]
fn a_network_with_several_fleets_ends_with_its_own_group_under_its_own_name() {
    let state = network_named("Acme IRC", &["#aurora-lyra", "#borealis-vega", "#lounge"]);
    let groups = layout(&state, &two_fleets(), &Default::default());
    assert_eq!(
        outline(&groups),
        owned(&[
            ("Aurora", &["lyra"]),
            ("Borealis", &["vega"]),
            ("Acme IRC", &["Server", "#lounge"]),
        ])
    );
    assert_eq!(groups.groups[2].key, group_key(&network_id()));
    assert!(!groups.captions.contains_key(&groups.groups[2].key));
}

#[test]
fn a_network_named_like_one_of_its_fleets_says_network_so_no_heading_shows_twice() {
    let state = network_named("aurora", &["#aurora-lyra", "#borealis-vega"]);
    let groups = layout(&state, &two_fleets(), &Default::default());
    let names: Vec<&str> = groups
        .groups
        .iter()
        .map(|group| group.name.as_ref())
        .collect();
    assert_eq!(names, ["Aurora", "Borealis", "aurora network"]);
    // With one fleet the network's rows join the fleet's group: one heading.
    let one = rosters(json!([initiative("aurora", "Aurora", &["lyra"], "")]));
    let groups = layout(&state, &one, &Default::default());
    let names: Vec<&str> = groups
        .groups
        .iter()
        .map(|group| group.name.as_ref())
        .collect();
    assert_eq!(names, ["Aurora"]);
}

#[test]
fn search_finds_a_channel_by_its_network_and_by_its_fleet_in_one_group() {
    let state = network_named("Acme IRC", &["#aurora-lyra", "#lounge"]);
    let one = rosters(json!([initiative("aurora", "Aurora", &["lyra"], "")]));
    let groups = layout(&state, &one, &Default::default());
    let index = super::super::search(&groups);
    for (query, title) in [
        ("Acme IRC lounge", "#lounge"),
        ("Aurora lounge", "#lounge"),
        ("Aurora lyra", "lyra"),
        ("Acme IRC lyra", "lyra"),
    ] {
        let found = index.search(query, 5);
        let titles: Vec<&str> = found.iter().map(|hit| hit.1.title.as_ref()).collect();
        assert!(titles.contains(&title), "{query}: {titles:?}");
    }
}
