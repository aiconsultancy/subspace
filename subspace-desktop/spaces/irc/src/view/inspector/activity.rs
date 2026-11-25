//! The captain's turns inside the Ship tab, opened from the card's
//! **Activity** action and left with Back, as a crew member's detail is.
//!
//! While it is open, the fleet's one event stream subscribes to the
//! captain's `agent-activity` events ([`FleetApi::watch_activity`]), and
//! the snapshot route is joined with them ([`ActivityJoin`]): no second
//! stream is opened. Once it closes, the handle goes and the stream drops
//! the subscription. Without a live stream the snapshot is read again on
//! a timer.
//!
//! What the captain saw is shown in full: an item too large to send
//! inline loads on demand, page by page, then shows whole. The content
//! lives in this selection only: nothing is written to disk, cached or
//! logged.

mod fetch;
mod item;
mod load;
mod paged;
mod parts;
mod text;
mod turn;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use gpui_kit::component::v_flex;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _,
    Styled as _, Subscription, Task, TestSupportExt as _,
};
use subspace_client_fleet::activity::ActivityJoin;
use subspace_client_fleet::{ActivityWatch, Error, FleetApi, FleetClient, FleetState, Link};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::workspace;

use crate::Irc;
use fetch::{poll, start_fetch};
use load::Loaded;
use load::load;

/// How often the snapshot is read again while no live stream carries the
/// captain's events.
const POLL: Duration = Duration::from_secs(5);

/// The captain whose turns the Ship tab shows, and what is known of them.
pub(crate) struct Selection {
    fleet: String,
    /// The captain's roster nick, as the channel names its ship.
    captain: String,
    /// The captain's registration ID ([`crate::live::ShipLive::id`]).
    agent: String,
    join: ActivityJoin,
    /// Turns opened or closed by hand, against the default: only a
    /// running turn is open.
    toggled: HashSet<String>,
    /// Items read page by page, by turn and item ID.
    loaded: HashMap<(String, String), Loaded>,
    fetch: Option<Task<()>>,
    _watch: Option<ActivityWatch>,
    _updates: Option<Subscription>,
    _poll: Option<Task<()>>,
}

/// Opens the turns of the captain `captain` of `fleet`, whose
/// registration ID is `agent`.
pub(super) fn open(
    board: &mut Irc,
    fleet: &str,
    captain: &str,
    agent: &str,
    cx: &mut Context<Irc>,
) {
    let api = FleetApi::current(cx);
    let watch = api
        .as_ref()
        .map(|api| api.update(cx, |api, cx| api.watch_activity(fleet, agent, cx)));
    let updates = api
        .as_ref()
        .map(|api| cx.observe(api, |board, _, cx| sync(board, cx)));
    let poll = api.as_ref().map(|_| poll(cx));
    board.inspector_activity = Some(Selection {
        fleet: fleet.to_owned(),
        captain: captain.to_owned(),
        agent: agent.to_owned(),
        join: ActivityJoin::new(),
        toggled: HashSet::new(),
        loaded: HashMap::new(),
        fetch: None,
        _watch: watch,
        _updates: updates,
        _poll: poll,
    });
    sync(board, cx);
    cx.notify();
}

/// Closes the turns; the stream drops the subscription.
pub(crate) fn close(board: &mut Irc, cx: &mut Context<Irc>) {
    if board.inspector_activity.take().is_some() {
        cx.notify();
    }
}

/// What the service holds of the captain's fleet for the join.
struct Service {
    polling: bool,
    news: Option<subspace_client_fleet::activity::ActivityNews>,
    client: Option<FleetClient>,
}

fn service(api: &FleetApi, selection: &Selection) -> Service {
    Service {
        polling: api.state(&selection.fleet) != FleetState::Followed(Link::Live),
        news: api.activity_news(&selection.fleet, &selection.agent, selection.join.next()),
        client: api.client(&selection.fleet).ok(),
    }
}

/// Takes the captain's news from the service and starts a wanted fetch.
pub(super) fn sync(board: &mut Irc, cx: &mut Context<Irc>) {
    let Some(api) = FleetApi::current(cx) else {
        return;
    };
    let Some(selection) = board.inspector_activity.as_ref() else {
        return;
    };
    let service = service(api.read(cx), selection);
    let changed = absorb(board, service.polling, service.news);
    if start_fetch(board, service.client, cx) || changed {
        cx.notify();
    }
}

fn absorb(
    board: &mut Irc,
    polling: bool,
    news: Option<subspace_client_fleet::activity::ActivityNews>,
) -> bool {
    let Some(selection) = board.inspector_activity.as_mut() else {
        return false;
    };
    selection.join.set_polling(polling);
    news.is_some_and(|news| selection.join.absorb(news))
}

/// Opens or closes a turn.
pub(super) fn toggle(board: &mut Irc, turn: &str, cx: &mut Context<Irc>) {
    if let Some(selection) = board.inspector_activity.as_mut() {
        if !selection.toggled.remove(turn) {
            selection.toggled.insert(turn.to_owned());
        }
        cx.notify();
    }
}

/// The turns of the captain the Ship tab shows, while they are open.
pub(super) fn body(
    board: &Irc,
    fleet: Option<&str>,
    captain: &str,
    cx: &Context<Irc>,
) -> Option<AnyElement> {
    let selection = board.inspector_activity.as_ref().filter(|selection| {
        fleet == Some(selection.fleet.as_str()) && selection.captain.eq_ignore_ascii_case(captain)
    })?;
    let back = action_button(
        "irc-activity-back",
        Intent::Quiet,
        Some(IconName::ArrowLeft),
        "Back to Ship",
    )
    .on_click(cx.listener(|board, _, _, cx| close(board, cx)));
    let section = workspace::section(format!("Activity · {}", selection.captain), cx)
        .children(notes(selection, cx))
        .children(turn::list(selection, cx));
    Some(
        v_flex()
            .id("irc-turns")
            .test_support()
            .w_full()
            .min_w_0()
            .items_start()
            .gap_4()
            .child(back)
            .child(section)
            .into_any_element(),
    )
}

/// What the view says above the turns: why it shows none, or that it is
/// not live.
fn notes(selection: &Selection, cx: &Context<Irc>) -> Vec<AnyElement> {
    note_words(selection)
        .into_iter()
        .map(|(id, words)| parts::caption(id, &words, cx))
        .collect()
}

/// Each note's ID and words.
pub(super) fn note_words(selection: &Selection) -> Vec<(&'static str, String)> {
    let activity = selection.join.activity();
    let error = selection.join.error().map(|error| match error {
        Error::NotFound => "Activity isn't available on this gateway.".to_owned(),
        error => error.to_string(),
    });
    let uncollected = activity
        .filter(|activity| !activity.agent.collected)
        .map(|_| "This officer's backend isn't collected yet.".to_owned());
    let loading = (activity.is_none() && error.is_none()).then(|| "Loading turns…".to_owned());
    let empty = activity
        .filter(|activity| activity.agent.collected && activity.turns.is_empty())
        .map(|_| "No turns yet.".to_owned());
    let refused = selection
        .join
        .refused()
        .then(|| "Live updates aren't available here, so turns refresh every few seconds.".into());
    [
        ("irc-activity-error", error),
        ("irc-activity-uncollected", uncollected),
        ("irc-activity-loading", loading),
        ("irc-activity-empty", empty),
        ("irc-activity-refused", refused),
    ]
    .into_iter()
    .filter_map(|(id, words)| words.map(|words| (id, words)))
    .collect()
}

#[cfg(test)]
impl Selection {
    /// The join, for a test to feed a snapshot or events to.
    pub(crate) fn join_mut(&mut self) -> &mut ActivityJoin {
        &mut self.join
    }
}

/// The card's **Activity** action: the captain's turns, inside this tab.
/// The captain's registration ID is the ship's ([`ShipLive::id`]).
pub(super) fn action(fleet: &str, captain: &str, agent: &str, cx: &Context<Irc>) -> AnyElement {
    let (fleet, captain, agent) = (fleet.to_owned(), captain.to_owned(), agent.to_owned());
    action_button(
        "irc-inspector-activity",
        Intent::Neutral,
        Some(IconName::Activity),
        "Activity",
    )
    .on_click(cx.listener(move |board, _, _, cx| {
        open(board, &fleet, &captain, &agent, cx);
    }))
    .into_any_element()
}
