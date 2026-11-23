//! Live fleet state in Channels: what the Fleet API of each fleet the
//! navigator or the conversation in front shows says of its ships and
//! their crew, read through the app's one fleet service
//! (`subspace_client_fleet::FleetApi`).
//!
//! The board follows a fleet while it shows it and its state is ready (a
//! token saved for the address the Subspace API records), and lets go when
//! it no longer shows it ([`watch`]). Nothing here reads a token or opens a
//! connection itself: the service does, only to the address the token was
//! saved for.
//!
//! A ship is matched to the roster, never to a nick, a channel name or
//! message text: a ship channel's captain is the roster's agent of rank
//! captain (from the Subspace API), and the Fleet API ship is the one whose
//! registration ID is that captain's API nick, as the installer names both
//! (`<slug>-<id>`). Every name and task from the Fleet API is untrusted
//! text: [`ship`] bounds it before anything draws it, as text.

use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use subspace_client_fleet::{FleetSnapshot, FleetState};

use crate::fleet::Roster;

pub(crate) mod ship;
pub(crate) mod watch;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use ship::{CrewLine, ShipLive};

/// What Channels draws of one fleet from its Fleet API: its state and its
/// ships as [`ShipLive`] values, and nothing else the stream delivers
/// (activity lines, sample times, a ship's other fields), so news that
/// Channels does not draw compares equal and redraws nothing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FleetLive {
    pub state: FleetState,
    /// Every ship of the stream's latest overview, while the fleet is
    /// live; `None` while it is not, or before its first overview, so a
    /// stale ship never shows once the stream is down.
    ships: Option<Rc<[ShipLive]>>,
    /// Whether the fleet is live but its latest overview is one Stargate
    /// kept after a failed read: data kept, not current, so no ship shows.
    kept: bool,
}

/// Each fleet the board shows, by its Subspace API id.
pub(crate) type Lives = HashMap<String, FleetLive>;

/// Each Channels view's [`Lives`], by the view's entity id.
pub(crate) type Shares = BTreeMap<u64, Lives>;

/// Sets `view`'s share to `lives`, forgetting an empty one; whether it
/// changed.
pub(crate) fn set_share(shares: &mut Shares, view: u64, lives: Lives) -> bool {
    if shares
        .get(&view)
        .map_or(lives.is_empty(), |held| *held == lives)
    {
        return false;
    }
    if lives.is_empty() {
        shares.remove(&view);
    } else {
        shares.insert(view, lives);
    }
    true
}

/// Every view's fleets together. Views that show the same fleet read it
/// from the same service, so either's copy will do.
pub(crate) fn union(shares: &Shares) -> Lives {
    shares
        .values()
        .flat_map(|lives| lives.iter())
        .map(|(id, live)| (id.clone(), live.clone()))
        .collect()
}

impl FleetLive {
    /// The fleet in `state` with what its stream delivered, `snapshot`,
    /// reduced to what Channels draws.
    pub(crate) fn new(state: FleetState, snapshot: Option<&FleetSnapshot>) -> Self {
        let live = snapshot.filter(|_| state.is_live());
        let ships = live
            .and_then(|snapshot| ship::all(snapshot, |_| true))
            .map(Rc::from);
        let kept = live
            .and_then(|snapshot| snapshot.overview.as_ref())
            .is_some_and(|overview| overview.overview_stale);
        Self { state, ships, kept }
    }

    /// Whether the fleet is live but shows no ship because its latest
    /// overview is one Stargate kept, not a current one.
    pub(crate) fn kept(&self) -> bool {
        self.kept
    }

    /// Whether a ship's absence means the Fleet API does not list it: the
    /// fleet's current overview arrived.
    pub(crate) fn listed(&self) -> bool {
        self.ships.is_some()
    }

    /// The ship whose captain has the roster nick `captain`, matched to its
    /// registration ID ignoring ASCII case, while the fleet is live and its
    /// Fleet API lists that ship.
    pub(crate) fn ship(&self, captain: &str) -> Option<ShipLive> {
        self.ships
            .as_deref()?
            .iter()
            .find(|ship| ship.id.eq_ignore_ascii_case(captain))
            .cloned()
    }

    /// Every ship of the live fleet whose captain `is_captain` accepts, in
    /// the Fleet API's order; `None` while the fleet is not live or before
    /// its first overview.
    pub(crate) fn ships(&self, is_captain: impl Fn(&str) -> bool) -> Option<Vec<ShipLive>> {
        let ships = self.ships.as_deref()?;
        Some(
            ships
                .iter()
                .filter(|ship| is_captain(&ship.id))
                .cloned()
                .collect(),
        )
    }
}

/// The id of the fleet `roster` places `channel` in.
pub(crate) fn fleet_id<'a>(roster: &'a Roster, channel: &str) -> Option<&'a str> {
    roster
        .fleets()
        .iter()
        .find(|place| place.holds(channel))
        .map(|place| place.id.as_str())
}

/// What `lives` says of the fleet `roster` places `channel` in.
pub(crate) fn fleet_of<'a>(
    lives: &'a Lives,
    roster: &Roster,
    channel: &str,
) -> Option<&'a FleetLive> {
    fleet_id(roster, channel).and_then(|id| lives.get(id))
}
