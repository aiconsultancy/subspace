//! A network's navigator groups: each fleet's, `People`, then the
//! network's own.

use gpui_kit::SharedString;
use subspace_client_irc::{BufferKey, NetworkId};
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use super::sections::{Held, Sections};
use super::{
    Layout, activity, fleet_group_key, group_key, listed_rows, people_group_key, row, status_label,
    status_tone, title,
};
use crate::fleet::{FleetPlace, Roster, short_name};
use crate::state::{Buffer, IrcState, Network};

/// One fleet on one network, for its channels' rows.
struct Fleet<'a> {
    network: &'a Network,
    roster: &'a Roster,
    place: &'a FleetPlace,
}

/// Adds `network`'s groups to `layout`: its fleets', `People`, then its own.
pub(super) fn network(state: &IrcState, network: &Network, roster: &Roster, layout: &mut Layout) {
    let sections = Sections::of(state, network, roster);
    fleet_groups(network, roster, sections.fleets, layout);
    people_group(state, network, sections.people, layout);
    network_group(state, network, sections.rest, layout);
}

/// Each fleet's group on `network`, for the fleets with a row.
fn fleet_groups(
    network: &Network,
    roster: &Roster,
    fleets: Vec<Vec<Held<'_>>>,
    layout: &mut Layout,
) {
    for (place, held) in roster.fleets().iter().zip(fleets) {
        if held.is_empty() {
            continue;
        }
        let fleet = Fleet {
            network,
            roster,
            place,
        };
        let rows = held
            .iter()
            .map(|(key, buffer)| fleet_row(key, buffer, &fleet, layout))
            .collect();
        let key = fleet_group_key(&network.id, &place.id);
        layout.fleets.insert(key.clone(), place.id.clone());
        let group = status_group(key, &place.name, network, rows);
        push_group(layout, &network.id, group);
    }
}

/// `People`: the private conversations with people on a network that
/// serves a fleet, when there are any.
fn people_group(state: &IrcState, network: &Network, people: Vec<Held<'_>>, layout: &mut Layout) {
    if people.is_empty() {
        return;
    }
    let rows = people
        .iter()
        .map(|(key, buffer)| person_row(state, key, buffer, layout))
        .collect();
    let group = Group {
        key: people_group_key(&network.id),
        icon: Glyph::new("@", Tone::Muted),
        count: None,
        name: people_name(state.networks().len(), &network.name).into(),
        rows,
    };
    push_group(layout, &network.id, group);
}

/// A person's row, with their presence recorded for its mark.
fn person_row(state: &IrcState, key: &BufferKey, buffer: &Buffer, layout: &mut Layout) -> Row {
    let row = keyed(row(key, buffer), key, layout);
    let presence = activity::presence(state, &key.network, title(key));
    layout.people.insert(row.key.clone(), presence);
    row
}

/// `People`, with the network's name when there are `networks` networks
/// to tell apart.
fn people_name(networks: usize, network: &str) -> String {
    if networks > 1 {
        format!("People · {network}")
    } else {
        "People".to_owned()
    }
}

/// The network's own group: its server, the channels of no fleet and the
/// private conversations not in `People`, then the listed channels.
fn network_group(state: &IrcState, network: &Network, rest: Vec<Held<'_>>, layout: &mut Layout) {
    let mut rows: Vec<Row> = rest
        .iter()
        .map(|(key, buffer)| keyed(row(key, buffer), key, layout))
        .collect();
    rows.extend(listed_rows(state, &network.id, &mut layout.listed));
    let group = status_group(group_key(&network.id), &network.name, network, rows);
    push_group(layout, &network.id, group);
}

fn push_group(layout: &mut Layout, network: &NetworkId, group: Group) {
    layout.networks.insert(group.key.clone(), network.clone());
    layout.groups.push(group);
}

/// Records which buffer `row` names.
fn keyed(row: Row, key: &BufferKey, layout: &mut Layout) -> Row {
    layout.buffers.insert(row.key.clone(), key.clone());
    row
}

/// A fleet channel's row: its name without the fleet's prefix, and for a
/// ship, its activity recorded for its dot.
fn fleet_row(key: &BufferKey, buffer: &Buffer, fleet: &Fleet<'_>, layout: &mut Layout) -> Row {
    let mut row = row(key, buffer);
    row.title = short_name(fleet.place.slug.as_deref(), title(key))
        .to_owned()
        .into();
    let ship = ship_activity(key, buffer, fleet).map(|ship| (row.key.clone(), ship));
    layout.ships.extend(ship);
    keyed(row, key, layout)
}

/// A ship channel's activity; `None` for any other channel.
fn ship_activity(
    key: &BufferKey,
    buffer: &Buffer,
    fleet: &Fleet<'_>,
) -> Option<activity::Activity> {
    let captain = activity::captain(fleet.roster.channel(title(key))?)?;
    Some(activity::ship(
        captain,
        &fleet.network.statuses,
        &buffer.members,
    ))
}

/// A group header with `network`'s connection state: a network's own, or
/// one of its fleets'.
fn status_group(key: SharedString, name: &str, network: &Network, rows: Vec<Row>) -> Group {
    Group {
        key,
        icon: Glyph::new("●", status_tone(&network.status)),
        name: name.to_owned().into(),
        count: Some(status_label(&network.status).into()),
        rows,
    }
}
