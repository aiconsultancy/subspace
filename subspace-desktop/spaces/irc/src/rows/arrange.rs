//! How a network's groups are arranged: one fleet's channels and the
//! network's own in one group, or a group per fleet and the network's own
//! group last, named so it cannot read as a fleet.

use subspace_framework::dashboard::{Glyph, Group, Row, Tone};

use super::{group_key, groups, kind_name, other_group_key};
use crate::fleet::FleetPlace;
use crate::state::Network;

/// The group for channels on a one-fleet network that no fleet claims.
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
/// fleet is followed by the network's Other group, then People.
/// Otherwise each fleet's group, `People`, then the network's own group,
/// named by [`own_name`]; with no fleet that is the only group, as before.
pub(super) fn groups(network: &Network, places: &[FleetPlace], parts: Parts) -> Vec<Group> {
    let Parts {
        fleets,
        people,
        rest,
    } = parts;
    if let [_] = places {
        let other = Group {
            key: other_group_key(&network.id),
            icon: Glyph::new("", Tone::Muted),
            name: OTHER.into(),
            count: None,
            rows: server_last(rest),
        };
        return fleets.into_iter().chain([other]).chain(people).collect();
    }
    let name = own_name(&network.name, places);
    let own = groups::status_group(group_key(&network.id), &name, network, rest);
    fleets.into_iter().chain(people).chain([own]).collect()
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
