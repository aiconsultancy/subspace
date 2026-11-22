//! The navigator grouped by fleet: each fleet's channels in its own group,
//! people's private conversations in `People`, and everything else under
//! the network as before.

use std::rc::Rc;

use serde_json::{Value, json};
use subspace_api_client::model::MemberInitiative;
use subspace_client_irc::{BufferKey, ConnectionStatus, Event, Member};
use subspace_framework::dashboard::Group;

use super::super::{Layout, Rosters, fleet_group_key, group_key, people_group_key, row_key};
use crate::fixtures::{at, channel, config, member, message, network_id, query};
use crate::fleet::{ChannelKind, Roster};
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
fn initiative(slug: &str, name: &str, ships: &[&str], channels: &str) -> Value {
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

fn rosters(initiatives: Value) -> Rosters {
    let initiatives: Vec<MemberInitiative> = serde_json::from_value(initiatives).unwrap();
    let held: Vec<&MemberInitiative> = initiatives.iter().collect();
    Rosters::from([(network_id(), Rc::new(Roster::for_initiatives(&held)))])
}

fn aurora() -> Rosters {
    rosters(json!([initiative(
        "aurora",
        "Aurora",
        &["lyra", "orion"],
        "#aurora-nav, #Aurora-Starbase"
    )]))
}

fn joined(buffers: impl IntoIterator<Item = BufferKey>) -> IrcState {
    let mut state = IrcState::new(&[config()], Vec::new());
    for buffer in buffers {
        state.apply(Event::Joined { buffer }, at(0));
    }
    state
}

/// Each group's name and its rows' titles.
fn outline(layout: &Layout) -> Vec<(String, Vec<String>)> {
    layout
        .groups
        .iter()
        .map(|group: &Group| {
            let titles = group.rows.iter().map(|row| row.title.to_string()).collect();
            (group.name.to_string(), titles)
        })
        .collect()
}

fn owned(groups: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
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
    let layout = super::super::layout(&state, &aurora());
    assert_eq!(
        outline(&layout),
        owned(&[
            ("Aurora", &["starbase", "command", "lyra", "orion", "nav"]),
            ("Local Ergo", &["Server", "#lounge"]),
        ])
    );
    let fleet = fleet_group_key(&network_id(), "aurora");
    assert_eq!(layout.groups[0].key, fleet, "only the group's key is new");
    assert_eq!(
        layout.fleets.get(&fleet).map(String::as_str),
        Some("aurora")
    );
    assert_eq!(layout.networks.get(&fleet), Some(&network_id()));
    assert_eq!(layout.groups[1].key, group_key(&network_id()));
    let lyra = row_key(&channel("#aurora-lyra"));
    assert_eq!(
        layout.groups[0].rows[2].key, lyra,
        "a row keeps its buffer's key, so selection and the dock restore keep working"
    );
    assert_eq!(layout.buffers.get(&lyra), Some(&channel("#aurora-lyra")));
    assert_eq!(
        layout.kinds.get(&lyra),
        Some(&ChannelKind::Ship {
            captain: "aurora-lyra".into()
        })
    );
    assert_eq!(
        layout.kinds.get(&row_key(&channel("#aurora-starbase"))),
        Some(&ChannelKind::Starbase)
    );
    assert_eq!(layout.kinds.get(&row_key(&channel("#aurora-nav"))), None);
}

#[test]
fn the_fleet_header_carries_the_connection_state_and_the_server_stays_under_its_network() {
    let mut state = joined([channel("#aurora-lyra")]);
    state.apply(
        Event::Status {
            network: network_id(),
            status: ConnectionStatus::Connected { nick: "me".into() },
        },
        at(1),
    );
    let layout = super::super::layout(&state, &aurora());
    let counts: Vec<Option<String>> = layout
        .groups
        .iter()
        .map(|group| group.count.as_ref().map(ToString::to_string))
        .collect();
    assert_eq!(
        counts,
        [Some("Connected".to_owned()), Some("Connected".to_owned())]
    );
    assert_eq!(layout.groups[1].rows[0].title.as_ref(), "Server");
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
    let layout = super::super::layout(&state, &rosters);
    assert_eq!(
        outline(&layout),
        owned(&[
            ("Aurora", &["lyra"]),
            ("Borealis", &["starbase", "vega"]),
            ("Local Ergo", &["Server", "#shared"]),
        ])
    );
    assert_ne!(layout.groups[0].key, layout.groups[1].key);
}

#[test]
fn a_fleet_with_no_channel_open_has_no_group() {
    let state = joined([channel("#lounge")]);
    let layout = super::super::layout(&state, &aurora());
    assert_eq!(
        outline(&layout),
        owned(&[("Local Ergo", &["Server", "#lounge"])])
    );
    assert!(layout.fleets.is_empty());
}

#[test]
fn people_hold_private_conversations_with_people_only() {
    let state = joined([
        query("mira"),
        query("aurora-lyra"),
        query("Anna"),
        channel("#aurora-lyra"),
    ]);
    let layout = super::super::layout(&state, &aurora());
    assert_eq!(
        outline(&layout),
        owned(&[
            ("Aurora", &["lyra"]),
            ("People", &["Anna", "mira"]),
            ("Local Ergo", &["Server", "aurora-lyra"]),
        ]),
        "an older conversation with an agent stays where it was"
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
    let layout = super::super::layout(&state, &Rosters::default());
    assert_eq!(
        outline(&layout),
        owned(&[("Local Ergo", &["Server", "#aurora-lyra", "mira"])])
    );
}

fn signed(nick: &str) -> Member {
    let mut signed = member(nick, "");
    signed.account = Some(nick.to_owned());
    signed
}

/// Lyra's nick status, or none.
fn lyra_status(status: Option<&str>) -> Event {
    Event::NickStatus {
        network: network_id(),
        nick: "aurora-lyra".into(),
        status: status.map(str::to_owned),
    }
}

/// The ship channel's members: only `member`.
fn crew(member: Member) -> Event {
    Event::Members {
        buffer: channel("#aurora-lyra"),
        members: vec![member],
    }
}

/// The ship row's trailing marks.
fn note(state: &IrcState) -> Vec<String> {
    super::super::layout(state, &aurora()).groups[0].rows[0]
        .trailing
        .iter()
        .map(|glyph| glyph.text.to_string())
        .collect()
}

#[test]
fn a_ship_row_says_working_only_while_its_vouched_captain_has_a_status() {
    let ship = channel("#aurora-lyra");
    let mut state = joined([ship.clone()]);
    let connected = Event::Status {
        network: network_id(),
        status: ConnectionStatus::Connected { nick: "me".into() },
    };
    for (minute, event) in [
        connected,
        lyra_status(Some("Reviewing NAV-7")),
        crew(member("aurora-lyra", "")),
    ]
    .into_iter()
    .enumerate()
    {
        state.apply(event, at(minute as i64));
    }
    assert!(note(&state).is_empty(), "no account, no state");
    state.apply(crew(signed("aurora-lyra")), at(4));
    assert_eq!(note(&state), ["working"]);
    state.apply(Event::Line(message(&ship, "mira", "hi", 5)), at(5));
    assert_eq!(note(&state), ["working", "1"]);
    state.apply(lyra_status(None), at(6));
    assert_eq!(note(&state), ["1"], "no idle state is invented");
}

#[test]
fn search_finds_a_fleet_channel_by_its_full_name() {
    let state = joined([channel("#aurora-lyra")]);
    let layout = super::super::layout(&state, &aurora());
    let index = super::super::search(&layout);
    let found = index.search("aurora-lyra", 5);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].1.title.as_ref(), "lyra");
}
