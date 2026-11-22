//! A network's navigator groups: each fleet's, `People`, then the
//! network's own.

use std::collections::HashMap;

use gpui_kit::SharedString;
use subspace_client_irc::{BufferKey, NetworkId};
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use super::sections::{Held, Sections};
use super::{
    Layout, activity, fleet_group_key, group_key, listed_rows, people_group_key, row, status_label,
    status_tone, title,
};
use crate::fleet::{ChannelKind, FleetPlace, Roster, short_name};
use crate::live::{FleetLive, Lives};
use crate::state::{Buffer, IrcState, Network};

/// The caption above a fleet's first ship channel.
pub(crate) const SHIPS: &str = "Ships";
/// The caption above a fleet's other channels, after its ships.
pub(crate) const CHANNELS: &str = "Channels";

/// One fleet on one network, for its channels' rows.
struct Fleet<'a> {
    network: &'a Network,
    roster: &'a Roster,
    place: &'a FleetPlace,
    /// What the fleet's Fleet API says, when the board shows the fleet.
    live: Option<&'a FleetLive>,
}

/// Adds `network`'s groups to `layout`: its fleets', `People`, then its own.
pub(super) fn network(
    state: &IrcState,
    network: &Network,
    (roster, lives): (&Roster, &Lives),
    layout: &mut Layout,
) {
    let sections = Sections::of(state, network, roster);
    fleet_groups(network, (roster, lives), sections.fleets, layout);
    people_group(state, network, sections.people, layout);
    network_group(state, network, sections.rest, layout);
}

/// Each fleet's group on `network`, for the fleets with a row.
fn fleet_groups(
    network: &Network,
    (roster, lives): (&Roster, &Lives),
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
            live: lives.get(&place.id),
        };
        let mut rows: Vec<Row> = held
            .iter()
            .map(|(key, buffer)| fleet_row(key, buffer, &fleet, layout))
            .collect();
        let full: Vec<&str> = held.iter().map(|(key, _)| title(key)).collect();
        unclash(&mut rows, &full);
        let key = fleet_group_key(&network.id, &place.id);
        let kinds = held.iter().map(|(key, _)| roster.channel(title(key)));
        let captions = captions(rows.iter().map(|row| row.key.clone()).zip(kinds));
        if !captions.is_empty() {
            layout.captions.insert(key.clone(), captions);
        }
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
    let presences = activity::presences(state, &network.id);
    let rows = people
        .iter()
        .map(|(key, buffer)| person_row(&presences, key, buffer, layout))
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
fn person_row(
    presences: &HashMap<String, activity::Presence>,
    key: &BufferKey,
    buffer: &Buffer,
    layout: &mut Layout,
) -> Row {
    let row = keyed(row(key, buffer), key, layout);
    let presence = activity::presence(presences, title(key));
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

/// A fleet channel's row: its name without the fleet's prefix (see
/// [`unclash`] for names that would read alike), and for a ship, its
/// activity recorded for its dot.
fn fleet_row(key: &BufferKey, buffer: &Buffer, fleet: &Fleet<'_>, layout: &mut Layout) -> Row {
    let mut row = row(key, buffer);
    row.title = short_name(fleet.place.slug.as_deref(), title(key))
        .to_owned()
        .into();
    let ship = ship_activity(key, buffer, fleet).map(|ship| (row.key.clone(), ship));
    layout.ships.extend(ship);
    keyed(row, key, layout)
}

/// Gives every row of `rows` whose title another row also has its channel's
/// full name from `full` (in the same order), so no two rows of a group read
/// alike: an initiative's `#lyra` beside the ship `#aurora-lyra`.
pub(super) fn unclash(rows: &mut [Row], full: &[&str]) {
    let titles: Vec<SharedString> = rows.iter().map(|row| row.title.clone()).collect();
    for (row, full) in rows.iter_mut().zip(full) {
        let shared = titles.iter().filter(|title| **title == row.title).count() > 1;
        if shared {
            row.title = (*full).to_owned().into();
        }
    }
}

/// A ship channel's activity; `None` for any other channel.
fn ship_activity(
    key: &BufferKey,
    buffer: &Buffer,
    fleet: &Fleet<'_>,
) -> Option<activity::Activity> {
    let captain = activity::captain(fleet.roster.channel(title(key))?)?;
    let live = fleet.live.and_then(|live| live.ship(captain));
    Some(activity::ship(
        captain,
        &fleet.network.statuses,
        &buffer.members,
        live.as_ref(),
    ))
}

/// A fleet group's captions, from each row's key and channel kind in
/// order: `Ships` above the first ship channel, and `Channels` above the
/// first channel of no role that follows a ship, so the ships read as one
/// run. A group with no ship has none.
pub(super) fn captions<'a>(
    rows: impl IntoIterator<Item = (SharedString, Option<&'a ChannelKind>)>,
) -> Vec<(SharedString, &'static str)> {
    let mut captions = Vec::new();
    for (key, kind) in rows {
        let ship = matches!(kind, Some(ChannelKind::Ship { .. }));
        let caption = match captions.len() {
            0 if ship => Some(SHIPS),
            1 if kind.is_none() => Some(CHANNELS),
            _ => None,
        };
        captions.extend(caption.map(|words| (key, words)));
    }
    captions
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
