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

use std::collections::HashMap;
use std::rc::Rc;

use subspace_client_fleet::{FleetSnapshot, FleetState};

use crate::fleet::Roster;

pub(crate) mod ship;
pub(crate) mod watch;

#[cfg(test)]
pub(crate) mod tests;

pub(crate) use ship::{CrewLine, ShipLive};

/// What Channels knows of one fleet from its Fleet API.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FleetLive {
    pub state: FleetState,
    /// What the fleet's stream has delivered, while the board follows it.
    pub snapshot: Option<Rc<FleetSnapshot>>,
}

/// Each fleet the board shows, by its Subspace API id.
pub(crate) type Lives = HashMap<String, FleetLive>;

impl FleetLive {
    /// The live snapshot: only while the fleet's data arrives, so a stale
    /// snapshot never shows once the stream is down.
    fn live(&self) -> Option<&FleetSnapshot> {
        self.snapshot.as_deref().filter(|_| self.state.is_live())
    }

    /// The ship whose captain has the roster nick `captain`, while the
    /// fleet is live and its Fleet API lists that ship.
    pub(crate) fn ship(&self, captain: &str) -> Option<ShipLive> {
        self.live().and_then(|snapshot| ship::of(snapshot, captain))
    }

    /// Every ship of the live fleet whose captain `is_captain` accepts, in
    /// the Fleet API's order; `None` while the fleet is not live or before
    /// its first overview.
    pub(crate) fn ships(&self, is_captain: impl Fn(&str) -> bool) -> Option<Vec<ShipLive>> {
        self.live()
            .and_then(|snapshot| ship::all(snapshot, is_captain))
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
