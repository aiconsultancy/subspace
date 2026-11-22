//! Following the fleets the board shows: one `FleetFollow` per fleet whose
//! group is in the navigator while the navigator shows, or whose channel is
//! the conversation in front, while the Space is in front and the fleet's
//! state is ready, dropped as soon as either stops. Showing or hiding the
//! navigator and changing the conversation in front sync again. The service opens at most one stream per fleet
//! however many Spaces follow it, and closes it with the last handle.
//!
//! What the service then reports of each shown fleet goes to the hub, as
//! this view's share ([`crate::hub::Hub::fleet_views`]); the hub's
//! [`crate::hub::Hub::fleets`], where the navigator, the headers and the
//! inspector read it, is the union of every view's, so a hidden view never
//! empties what a shown one publishes. A view gives its share back when it
//! closes. A fleet with no address, no token, a changed address or no
//! answer is still listed, with its state, so each surface can say why it
//! shows what it showed before.
//!
//! The service is reached only once a fleet is on show: a view that shows
//! none never starts it, and so never reads a token or asks the keychain.

use std::collections::{BTreeMap, BTreeSet};

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
    /// Syncs again on the service's news, and gives this view's share
    /// back to the hub when the view closes.
    _subscriptions: Vec<Subscription>,
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

/// Whether the view's `watch` has reached the fleet service.
#[cfg(test)]
pub(crate) fn bound(watch: &Watch) -> bool {
    watch.service.is_some()
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
    let shown = shown(board, cx);
    if !shown.is_empty() {
        bind(board, cx);
    }
    let Some(fleets) = board.watch.service.clone() else {
        publish(board, Lives::default(), cx);
        return;
    };
    let seen = Some((fleets.read(cx).revision(), shown.clone()));
    if board.watch.seen == seen {
        return;
    }
    follow(&mut board.watch, &fleets, &shown, cx);
    let lives = lives(fleets.read(cx), &shown);
    board.watch.seen = Some((fleets.read(cx).revision(), shown));
    publish(board, lives, cx);
}

/// Finds the service once a fleet is on show, syncs again whenever it has
/// news, and gives the view's share back when the view closes: only a
/// view that reached the service ever has a share.
fn bind(board: &mut Irc, cx: &mut Context<Irc>) {
    if board.watch.service.is_some() {
        return;
    }
    let Some(fleets) = service(cx) else {
        return;
    };
    let view = cx.entity_id().as_u64();
    board.watch._subscriptions = vec![
        cx.observe(&fleets, |board, _, cx| sync(board, cx)),
        cx.on_release(move |board, cx| {
            if let Some(hub) = board.hub.clone() {
                hub.update(cx, |hub, cx| share(hub, view, Lives::default(), cx));
            }
        }),
    ];
    board.watch.service = Some(fleets);
}

/// The fleets on show: the navigator's while it shows and the front
/// conversation's, while the Space is in front; none otherwise.
fn shown(board: &Irc, cx: &App) -> BTreeSet<String> {
    if !board.active {
        return BTreeSet::new();
    }
    let navigator = board
        .dock
        .buffers_open()
        .then(|| board.watch.navigator.clone());
    let mut shown = navigator.unwrap_or_default();
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

/// What the service says of each shown fleet, reduced to what Channels
/// draws ([`FleetLive::new`]).
fn lives(fleets: &FleetApi, shown: &BTreeSet<String>) -> Lives {
    shown
        .iter()
        .map(|id| {
            let snapshot = fleets.snapshot(id);
            let live = FleetLive::new(fleets.state(id), snapshot.as_ref());
            (id.clone(), live)
        })
        .collect()
}

/// Gives the hub `lives` as this view's share when they differ from what
/// it holds, so the conversations and the navigator draw again only on
/// news they draw: a new activity line or sample time leaves `lives`
/// equal.
fn publish(board: &Irc, lives: Lives, cx: &mut Context<Irc>) {
    let Some(hub) = board.hub.clone() else {
        return;
    };
    let view = cx.entity_id().as_u64();
    hub.update(cx, |hub, cx| share(hub, view, lives, cx));
}

/// Sets `view`'s share of the hub's fleets to `lives` and announces the
/// union when it changed.
pub(crate) fn share(
    hub: &mut crate::hub::Hub,
    view: u64,
    lives: Lives,
    cx: &mut Context<crate::hub::Hub>,
) {
    if super::set_share(&mut hub.fleet_views, view, lives) {
        hub.fleets = super::union(&hub.fleet_views);
        cx.notify();
    }
}
