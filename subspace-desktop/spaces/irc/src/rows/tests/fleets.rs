//! The navigator grouped by fleet: each fleet's channels in its own group,
//! people's private conversations in `People`, and everything else under
//! the network as before.

use std::rc::Rc;

use serde_json::{Value, json};
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{BufferKey, ConnectionStatus, Event};
use subspace_framework::dashboard::Group;

use super::super::{Layout, Rosters, fleet_group_key, people_group_key, row_key};
use crate::fixtures::{at, channel, config, network_id, query};
use crate::fleet::Roster;
use crate::state::IrcState;

fn agent(nick: &str, rank: &str, ship: Option<&str>) -> Value {
    json!({
        "userId": format!("user-{nick}"), "name": nick, "rank": rank,
        "shipId": ship.map(|ship| format!("ship-{ship}")), "ship": ship,
        "ircNick": nick, "available": true,
    })
}

/// An initiative of fleet `slug` (named `name`), whose rear admiral is
/// `<slug>-admiral` and whose captains are `<slug>-<ship>` for each of
/// `ships`, naming its own `channels` too.
pub(super) fn initiative(slug: &str, name: &str, ships: &[&str], channels: &str) -> Value {
    let mut agents = vec![agent(&format!("{slug}-admiral"), "rearAdmiral", None)];
    agents.extend(
        ships
            .iter()
            .map(|ship| agent(&format!("{slug}-{ship}"), "captain", Some(ship))),
    );
    json!({
        "id": format!("initiative-{slug}"), "name": format!("{name} work"), "channels": channels,
        "fleetId": slug,
        "fleet": {"id": slug, "slug": slug, "name": name, "ircHost": "irc.example.test",
                  "ircPort": 6697, "ircTls": true,
                  "starbaseChannel": format!("#{slug}-starbase"),
                  "commandChannel": format!("#{slug}-command"),
                  "agents": agents}
    })
}

pub(super) fn rosters(initiatives: Value) -> Rosters {
    let initiatives: Vec<MemberInitiative> = serde_json::from_value(initiatives).unwrap();
    let held: Vec<&MemberInitiative> = initiatives.iter().collect();
    Rosters::from([(network_id(), Rc::new(Roster::for_initiatives(&held)))])
}

pub(super) fn aurora() -> Rosters {
    rosters(json!([initiative(
        "aurora",
        "Aurora",
        &["lyra", "orion"],
        "#aurora-nav, #Aurora-Starbase"
    )]))
}

pub(super) fn joined(buffers: impl IntoIterator<Item = BufferKey>) -> IrcState {
    let mut state = IrcState::new(&[config()], Vec::new());
    for buffer in buffers {
        state.apply(Event::Joined { buffer }, at(0));
    }
    state
}

/// Each group's name and its rows' titles.
pub(super) fn outline(layout: &Layout) -> Vec<(String, Vec<String>)> {
    layout
        .groups
        .iter()
        .map(|group: &Group| {
            let titles = group.rows.iter().map(|row| row.title.to_string()).collect();
            (group.name.to_string(), titles)
        })
        .collect()
}

pub(super) fn owned(groups: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
    groups
        .iter()
        .map(|(name, rows)| {
            let rows = rows.iter().map(|row| (*row).to_owned()).collect();
            ((*name).to_owned(), rows)
        })
        .collect()
}

#[test]
fn a_fleet_s_channels_come_starbase_command_ships_then_the_rest_without_its_prefix() {
    let state = joined([
        channel("#aurora-orion"),
        channel("#aurora-nav"),
        channel("#aurora-lyra"),
        channel("#aurora-command"),
        channel("#aurora-starbase"),
        channel("#lounge"),
    ]);
    let layout = super::super::layout(&state, &aurora(), &Default::default());
    assert_eq!(
        outline(&layout),
        owned(&[(
            "Aurora",
            &[
                "starbase", "command", "lyra", "orion", "nav", "#lounge", "Server"
            ]
        )])
    );
    let fleet = fleet_group_key(&network_id(), "aurora");
    assert_eq!(layout.groups[0].key, fleet, "only the group's key is new");
    assert_eq!(
        layout.fleets.get(&fleet).map(String::as_str),
        Some("aurora")
    );
    assert_eq!(layout.networks.get(&fleet), Some(&network_id()));
    let lyra = row_key(&channel("#aurora-lyra"));
    assert_eq!(
        layout.groups[0].rows[2].key, lyra,
        "a row keeps its buffer's key, so selection and the dock restore keep working"
    );
    assert_eq!(layout.buffers.get(&lyra), Some(&channel("#aurora-lyra")));
    assert_eq!(
        layout.ships.get(&lyra).map(|ship| ship.captain.as_str()),
        Some("aurora-lyra"),
        "a ship row has a dot for its captain"
    );
    for plain in [
        "#aurora-starbase",
        "#aurora-command",
        "#aurora-nav",
        "#lounge",
    ] {
        assert!(
            !layout.ships.contains_key(&row_key(&channel(plain))),
            "{plain} has no dot"
        );
    }
}

#[test]
fn a_one_fleet_network_is_one_group_with_the_connection_state_and_the_server_last() {
    let mut state = joined([channel("#aurora-lyra")]);
    state.apply(
        Event::Status {
            network: network_id(),
            status: ConnectionStatus::Connected { nick: "me".into() },
        },
        at(1),
    );
    let layout = super::super::layout(&state, &aurora(), &Default::default());
    let counts: Vec<Option<String>> = layout
        .groups
        .iter()
        .map(|group| group.count.as_ref().map(ToString::to_string))
        .collect();
    assert_eq!(counts, [Some("Connected".to_owned())], "named once");
    let rows = &layout.groups[0].rows;
    assert_eq!(rows.last().map(|row| row.title.as_ref()), Some("Server"));
    assert_eq!(
        layout.networks.get(&layout.groups[0].key),
        Some(&network_id())
    );
}

#[test]
fn two_fleets_on_one_network_each_have_a_group_and_a_shared_channel_has_none() {
    let rosters = rosters(json!([
        initiative("aurora", "Aurora", &["lyra"], "#shared"),
        initiative("borealis", "Borealis", &["vega"], "#shared"),
    ]));
    let state = joined([
        channel("#borealis-vega"),
        channel("#aurora-lyra"),
        channel("#shared"),
        channel("#borealis-starbase"),
    ]);
    let layout = super::super::layout(&state, &rosters, &Default::default());
    assert_eq!(
        outline(&layout),
        owned(&[
            ("Aurora", &["lyra"]),
            ("Borealis", &["starbase", "vega"]),
            ("Acme IRC", &["Server", "#shared"]),
        ])
    );
    assert_ne!(layout.groups[0].key, layout.groups[1].key);
}

#[test]
fn a_one_fleet_network_with_no_fleet_channel_open_is_still_named_after_its_fleet() {
    let state = joined([channel("#lounge")]);
    let layout = super::super::layout(&state, &aurora(), &Default::default());
    assert_eq!(
        outline(&layout),
        owned(&[("Aurora", &["#lounge", "Server"])])
    );
    assert_eq!(layout.fleets.len(), 1);
}

#[test]
fn people_hold_private_conversations_with_people_only() {
    let state = joined([
        query("mira"),
        query("aurora-lyra"),
        query("Anna"),
        channel("#aurora-lyra"),
    ]);
    let layout = super::super::layout(&state, &aurora(), &Default::default());
    assert_eq!(
        outline(&layout),
        owned(&[
            ("Aurora", &["lyra", "aurora-lyra", "Server"]),
            ("People", &["Anna", "mira"]),
        ]),
        "an older conversation with an agent stays with the network's rows"
    );
    assert_eq!(layout.groups[1].key, people_group_key(&network_id()));
    assert_eq!(
        layout.networks.get(&layout.groups[1].key),
        Some(&network_id())
    );
}

#[test]
fn a_network_that_serves_no_fleet_keeps_its_private_conversations() {
    let state = joined([query("mira"), channel("#aurora-lyra")]);
    let layout = super::super::layout(&state, &Rosters::default(), &Default::default());
    assert_eq!(
        outline(&layout),
        owned(&[("Acme IRC", &["Server", "#aurora-lyra", "mira"])])
    );
}

#[test]
fn search_finds_a_fleet_channel_by_its_full_name() {
    let state = joined([channel("#aurora-lyra")]);
    let layout = super::super::layout(&state, &aurora(), &Default::default());
    let index = super::super::search(&layout);
    let found = index.search("aurora-lyra", 5);
    assert_eq!(found.first().map(|hit| hit.1.title.as_ref()), Some("lyra"));
}

#[test]
fn search_still_finds_a_fleet_channel_and_a_person_by_their_network() {
    let state = joined([channel("#aurora-lyra"), query("mira")]);
    let layout = super::super::layout(&state, &aurora(), &Default::default());
    let index = super::super::search(&layout);
    for (query, title) in [("Acme IRC lyra", "lyra"), ("Acme IRC mira", "mira")] {
        let found = index.search(query, 5);
        assert_eq!(
            found.first().map(|(_, row)| row.title.as_ref()),
            Some(title),
            "{query}"
        );
    }
}

#[test]
fn channels_whose_short_names_clash_keep_their_full_names() {
    // The initiative names `#lyra`, which would read as the ship `#aurora-lyra`.
    let rosters = rosters(json!([initiative(
        "aurora",
        "Aurora",
        &["lyra"],
        "#lyra, #aurora-nav"
    )]));
    let state = joined([
        channel("#aurora-lyra"),
        channel("#lyra"),
        channel("#aurora-nav"),
    ]);
    let layout = super::super::layout(&state, &rosters, &Default::default());
    let (_, titles) = outline(&layout)
        .into_iter()
        .find(|(name, _)| name == "Aurora")
        .expect("the fleet's group");
    assert!(titles.contains(&"#aurora-lyra".to_owned()), "{titles:?}");
    assert!(titles.contains(&"#lyra".to_owned()), "{titles:?}");
    assert!(
        titles.contains(&"nav".to_owned()),
        "an unclashed name stays short"
    );
}

#[test]
fn ask_addresses_no_one_in_a_channel_two_fleets_claim() {
    // Borealis's initiative names Aurora's ship channel too, so the
    // navigator shows it in neither fleet, and /ask refuses it.
    let rosters = rosters(json!([
        initiative("aurora", "Aurora", &["lyra", "orion"], ""),
        initiative("borealis", "Borealis", &["vega"], "#aurora-lyra"),
    ]));
    let roster = &rosters[&network_id()];
    assert_eq!(roster.asked_in("#aurora-lyra"), None);
    assert_eq!(roster.asked_in("#aurora-orion"), Some("aurora-orion"));
    assert_eq!(
        roster.asked_in("#borealis-starbase"),
        Some("borealis-admiral")
    );
}
