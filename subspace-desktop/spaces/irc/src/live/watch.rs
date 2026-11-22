//! Following the fleets the board shows: one `FleetFollow` per fleet whose
//! group is in the navigator or whose channel is the conversation in front,
//! while the Space is in front and the fleet's state is ready, dropped as
//! soon as either stops. The service opens at most one stream per fleet
//! however many Spaces follow it, and closes it with the last handle.
//!
//! What the service then reports of each shown fleet goes to the hub
//! ([`crate::hub::Hub::fleets`]), where the navigator, the headers and the
//! inspector read it. A fleet with no address, no token, a changed address
//! or no answer is still listed, with its state, so each surface can say
//! why it shows what it showed before.

use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use gpui_kit::{App, Context, Entity, Subscription};
use subspace_client_fleet::{FleetApi, FleetFollow, FleetState};
use subspace_client_irc::Target;

use super::{FleetLive, Lives};
use crate::Irc;

/// The board's follows and what it last told the hub.
#[derive(Default)]
pub(crate) struct Watch {
    /// The fleets of the navigator's fleet groups, by id.
    pub navigator: BTreeSet<String>,
    follows: BTreeMap<String, FleetFollow>,
    service: Option<Entity<FleetApi>>,
    /// The service's revision and the fleets shown at the last sync.
    seen: Option<(u64, BTreeSet<String>)>,
    _observe: Option<Subscription>,
}

impl Watch {
    /// Tells the next sync to give a new hub what the service says, even
    /// when nothing changed since the last.
    pub(crate) fn forget_hub(&mut self) {
        self.seen = None;
    }

    /// The fleets followed now, by id.
    #[cfg(test)]
    pub(crate) fn following(&self) -> Vec<String> {
        self.follows.keys().cloned().collect()
    }
}

/// The fleet service as this build reaches it: the process's one in the
/// app; under test only one a test installed, so no test opens the token
/// store.
fn service(cx: &mut App) -> Option<Entity<FleetApi>> {
    #[cfg(not(test))]
    let service = Some(FleetApi::shared(cx));
    #[cfg(test)]
    let service = FleetApi::current(cx);
    service
}

/// Whether the board follows a fleet in `state`: once a token is saved for
/// the address the Subspace API records, and on while it is followed.
pub(crate) fn wanted(state: &FleetState) -> bool {
    matches!(state, FleetState::Ready | FleetState::Followed(_))
}

/// Follows the fleets the board shows, lets go of the rest, and gives the
/// hub what the service says of each shown fleet.
pub(crate) fn sync(board: &mut Irc, cx: &mut Context<Irc>) {
    bind(board, cx);
    let Some(fleets) = board.watch.service.clone() else {
        return;
    };
    let shown = shown(board, cx);
    let seen = Some((fleets.read(cx).revision(), shown.clone()));
    if board.watch.seen == seen {
        return;
    }
    follow(&mut board.watch, &fleets, &shown, cx);
    let lives = lives(fleets.read(cx), &shown);
    board.watch.seen = Some((fleets.read(cx).revision(), shown));
    publish(board, lives, cx);
}

/// Finds the service once, and syncs again whenever it has news.
fn bind(board: &mut Irc, cx: &mut Context<Irc>) {
    if board.watch.service.is_some() {
        return;
    }
    let Some(fleets) = service(cx) else {
        return;
    };
    board.watch._observe = Some(cx.observe(&fleets, |board, _, cx| sync(board, cx)));
    board.watch.service = Some(fleets);
}

/// The fleets on show: the navigator's and the front conversation's, while
/// the Space is in front; none otherwise.
fn shown(board: &Irc, cx: &App) -> BTreeSet<String> {
    if !board.active {
        return BTreeSet::new();
    }
    let mut shown = board.watch.navigator.clone();
    shown.extend(front_fleet(board, cx));
    shown
}

/// The fleet whose channel is the conversation in front, by the roster.
fn front_fleet(board: &Irc, cx: &App) -> Option<String> {
    let buffer = board.dock.focused.as_ref()?;
    let Target::Channel(channel) = &buffer.target else {
        return None;
    };
    let roster = board.hub.as_ref()?.read(cx).roster(&buffer.network, cx);
    super::fleet_id(&roster, channel).map(str::to_owned)
}

/// Keeps one follow for each shown fleet that is ready, and none other.
fn follow(
    watch: &mut Watch,
    fleets: &Entity<FleetApi>,
    shown: &BTreeSet<String>,
    cx: &mut Context<Irc>,
) {
    let ready: Vec<String> = shown
        .iter()
        .filter(|id| wanted(&fleets.read(cx).state(id)))
        .cloned()
        .collect();
    watch.follows.retain(|id, _| ready.contains(id));
    for id in ready {
        if let std::collections::btree_map::Entry::Vacant(entry) = watch.follows.entry(id) {
            let handle = fleets.update(cx, |fleets, cx| fleets.follow(entry.key(), cx));
            entry.insert(handle);
        }
    }
}

/// What the service says of each shown fleet.
fn lives(fleets: &FleetApi, shown: &BTreeSet<String>) -> Lives {
    shown
        .iter()
        .map(|id| {
            let live = FleetLive {
                state: fleets.state(id),
                snapshot: fleets.snapshot(id).map(Rc::new),
            };
            (id.clone(), live)
        })
        .collect()
}

/// Gives the hub `lives` when they differ from what it holds, so the
/// conversations and the navigator draw again only on news.
fn publish(board: &Irc, lives: Lives, cx: &mut Context<Irc>) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    if hub.read(cx).fleets == lives {
        return;
    }
    hub.update(cx, |hub, cx| {
        hub.fleets = lives;
        cx.notify();
    });
}
