//! How a network's groups are arranged: one fleet's channels and the
//! network's own in one group, or a group per fleet and the network's own
//! group last, named so it cannot read as a fleet.

use std::collections::HashMap;

use gpui_kit::SharedString;
use subspace_framework::dashboard::{Group, Row};

use super::{group_key, groups, kind_name};
use crate::fleet::FleetPlace;
use crate::state::Network;

/// The caption above the channels on a one-fleet network that no fleet
/// claims, and its Server row.
pub(crate) const OTHER: &str = "Other";

/// A network's groups before they are arranged.
pub(super) struct Parts {
    /// Each fleet's group, in the roster's order.
    pub fleets: Vec<Group>,
    pub people: Option<Group>,
    /// The network's own rows, the Server row first.
    pub rest: Vec<Row>,
}

/// `network`'s groups in order. Holding exactly one fleet (`places`), the
/// fleet's group takes the network's rows ([`merged`]), then `People`.
/// Otherwise each fleet's group, `People`, then the network's own group,
/// named by [`own_name`]; with no fleet that is the only group, as before.
pub(super) fn groups(
    network: &Network,
    places: &[FleetPlace],
    parts: Parts,
    captions: &mut HashMap<SharedString, Vec<(SharedString, &'static str)>>,
) -> Vec<Group> {
    let Parts {
        mut fleets,
        people,
        rest,
    } = parts;
    if let ([_], [_]) = (places, fleets.as_slice()) {
        let fleet = fleets.remove(0);
        return std::iter::once(merged(fleet, rest, captions))
            .chain(people)
            .collect();
    }
    let name = own_name(&network.name, places);
    let own = groups::status_group(group_key(&network.id), &name, network, rest);
    fleets.into_iter().chain(people).chain([own]).collect()
}

/// The fleet's group with the network's `rest` after its channels, under
/// an `Other` caption, the Server row last.
pub(super) fn merged(
    mut fleet: Group,
    rest: Vec<Row>,
    captions: &mut HashMap<SharedString, Vec<(SharedString, &'static str)>>,
) -> Group {
    let rest = server_last(rest);
    if let Some(first) = rest.first() {
        captions
            .entry(fleet.key.clone())
            .or_default()
            .push((first.key.clone(), OTHER));
    }
    fleet.rows.extend(rest);
    fleet
}

/// `rows` with the Server row moved to the end.
fn server_last(rows: Vec<Row>) -> Vec<Row> {
    let (server, others): (Vec<Row>, Vec<Row>) = rows
        .into_iter()
        .partition(|row| kind_name(&row.key) == "server");
    others.into_iter().chain(server).collect()
}

/// The network's own group's name beside its fleets' groups: its name, or
/// `<name> network` when a fleet has the same name ignoring case, so the
/// same heading never shows twice.
pub(super) fn own_name(network: &str, places: &[FleetPlace]) -> String {
    let clash = places
        .iter()
        .any(|place| place.name.eq_ignore_ascii_case(network));
    if clash {
        format!("{network} network")
    } else {
        network.to_owned()
    }
}
