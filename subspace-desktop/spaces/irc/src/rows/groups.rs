//! A network's navigator groups. A network that holds one fleet is one
//! group named after the fleet: its channels, then under **Other** the
//! network's channels no fleet claims, with the Server row last; then
//! `People`. A network that holds several keeps a group per fleet,
//! `People`, then its own group, named so it cannot read as a fleet. A
//! network with no fleet keeps its one group.

use std::collections::HashMap;

use gpui_kit::SharedString;
use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use super::sections::{Entry, Sections};
use super::{
    Layout, activity, arrange, fleet_group_key, listed_row, people_group_key, row, status_label,
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

/// Adds `network`'s groups to `layout`, as the module says.
pub(super) fn network(
    state: &IrcState,
    network: &Network,
    (roster, lives): (&Roster, &Lives),
    layout: &mut Layout,
) {
    let sections = Sections::of(state, network, roster);
    let fleets = fleet_groups(network, (roster, lives), sections.fleets, layout);
    let rest = sections
        .rest
        .iter()
        .map(|entry| entry_row(entry, layout))
        .collect();
    let people = people_group(state, network, sections.people, layout);
    let parts = arrange::Parts {
        fleets,
        people,
        rest,
    };
    let groups = arrange::groups(network, roster.fleets(), parts, &mut layout.captions);
    push_groups(layout, network, groups);
}

/// Adds `groups`, all of `network`, to `layout`, with the network's name
/// for search.
fn push_groups(layout: &mut Layout, network: &Network, groups: Vec<Group>) {
    layout
        .network_names
        .insert(network.id.clone(), network.name.clone().into());
    for group in &groups {
        layout
            .networks
            .insert(group.key.clone(), network.id.clone());
    }
    layout.groups.extend(groups);
}

/// Each fleet's group on `network`: every fleet with a row, or with
/// `single` the network's one fleet even without one, as the network's
/// channels join it.
fn fleet_groups(
    network: &Network,
    (roster, lives): (&Roster, &Lives),
    fleets: Vec<Vec<Entry<'_>>>,
    layout: &mut Layout,
) -> Vec<Group> {
    let single = roster.fleets().len() == 1;
    let mut groups = Vec::new();
    for (place, held) in roster.fleets().iter().zip(fleets) {
        if held.is_empty() && !single {
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
            .map(|entry| fleet_row(entry, &fleet, layout))
            .collect();
        let full: Vec<&str> = held.iter().map(|entry| title(entry.key())).collect();
        unclash(&mut rows, &full);
        let key = fleet_group_key(&network.id, &place.id);
        let kinds = full.iter().map(|name| roster.channel(name));
        let captions = captions(rows.iter().map(|row| row.key.clone()).zip(kinds));
        if !captions.is_empty() {
            layout.captions.insert(key.clone(), captions);
        }
        layout.fleets.insert(key.clone(), place.id.clone());
        groups.push(status_group(key, &place.name, network, rows));
    }
    groups
}

/// `People`: the private conversations with people on a network that
/// serves a fleet, when there are any.
fn people_group(
    state: &IrcState,
    network: &Network,
    people: Vec<Entry<'_>>,
    layout: &mut Layout,
) -> Option<Group> {
    if people.is_empty() {
        return None;
    }
    let presences = activity::presences(state, &network.id);
    let rows = people
        .iter()
        .map(|entry| person_row(&presences, entry, layout))
        .collect();
    Some(Group {
        key: people_group_key(&network.id),
        icon: Glyph::new("@", Tone::Muted),
        count: None,
        name: people_name(state.networks().len(), &network.name).into(),
        rows,
    })
}

/// A person's row, with their presence recorded for its mark.
fn person_row(
    presences: &HashMap<String, activity::Presence>,
    entry: &Entry<'_>,
    layout: &mut Layout,
) -> Row {
    let row = entry_row(entry, layout);
    let presence = activity::presence(presences, title(entry.key()));
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

/// Builds a row and records its target for opening or joining.
fn entry_row(entry: &Entry<'_>, layout: &mut Layout) -> Row {
    let (row, keys) = match entry {
        Entry::Buffer(key, buffer) => (row(key, buffer), &mut layout.buffers),
        Entry::Listed(key, users) => (listed_row(key, *users), &mut layout.listed),
    };
    keys.insert(row.key.clone(), entry.key().clone());
    row
}

/// A fleet channel's row: its name without the fleet's prefix (see
/// [`unclash`] for names that would read alike), and for a ship buffer, its
/// activity recorded for its dot.
fn fleet_row(entry: &Entry<'_>, fleet: &Fleet<'_>, layout: &mut Layout) -> Row {
    let mut row = entry_row(entry, layout);
    let name = title(entry.key());
    row.title = match fleet.roster.channel(name) {
        Some(ChannelKind::Audit) => "fleet-audit".into(),
        _ => short_name(fleet.place.slug.as_deref(), name)
            .to_owned()
            .into(),
    };
    if let Entry::Buffer(_, buffer) = entry {
        let ship = ship_activity(name, buffer, fleet).map(|ship| (row.key.clone(), ship));
        layout.ships.extend(ship);
    }
    row
}

/// Gives every row of `rows` whose title another row also has, ignoring
/// ASCII case, its channel's full name from `full` (in the same order), so
/// no two rows of a group read alike: an initiative's `#lyra` or `#Lyra`
/// beside the ship `#aurora-lyra`. Titles are counted in one pass.
pub(super) fn unclash(rows: &mut [Row], full: &[&str]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for row in rows.iter() {
        *counts.entry(row.title.to_ascii_lowercase()).or_default() += 1;
    }
    for (row, full) in rows.iter_mut().zip(full) {
        if counts.get(&row.title.to_ascii_lowercase()) > Some(&1) {
            row.title = (*full).to_owned().into();
        }
    }
}

/// A ship channel's activity; `None` for any other channel.
fn ship_activity(name: &str, buffer: &Buffer, fleet: &Fleet<'_>) -> Option<activity::Activity> {
    let captain = activity::captain(fleet.roster.channel(name)?)?;
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
pub(super) fn status_group(
    key: SharedString,
    name: &str,
    network: &Network,
    rows: Vec<Row>,
) -> Group {
    Group {
        key,
        icon: Glyph::new("●", status_tone(&network.status)),
        name: name.to_owned().into(),
        count: Some(status_label(&network.status).into()),
        rows,
    }
}
