//! The captain's turns inside the Ship tab, opened from the card's
//! **Activity** action and left with Back, as a crew member's detail is.
//!
//! While it is open, the fleet's one event stream subscribes to the
//! captain's `agent-activity` events ([`FleetApi::watch_activity`]), and
//! the snapshot route is joined with them ([`ActivityJoin`]): no second
//! stream is opened. Once it closes, the handle goes and the stream drops
//! the subscription. It closes with Back, when the conversation in front
//! changes, when the inspector hides or shows another tab, and when the
//! ship leaves the live fleet or the channel's fleet changes. The snapshot
//! is fetched only once the stream carrying the captain has answered;
//! only a fleet polled without a stream, or a captain the stream does not
//! carry, is read again on a timer.
//!
//! A failed fetch keeps the last turns with the error and a **Try again**
//! action, and is retried after 1, 2, 4 … seconds up to 30 (or Stargate's
//! `Retry-After`, up to the same cap), and at once on a new connection.
//! A gateway without the route says so and is not retried.
//!
//! What the captain saw is shown in full: an item too large to send
//! inline loads on demand, page by page, then shows whole, a long text in
//! parts of 64 KiB. Items read in full are kept up to 64 MiB, the least
//! recently opened going first, and go with their turn. Closing the view
//! cancels any read: no page is asked for after it. The content lives in
//! this selection only: nothing is written to disk, cached or logged, and
//! none of it can be added to the chat.

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
use subspace_client_fleet::{ActivityWatch, Error, FleetApi, FleetClient, FleetState};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::panel::IconName;
use subspace_framework::dashboard::workspace;

use crate::Irc;
use fetch::{poll, start_fetch};
use load::Loaded;
use load::{load, prune};

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
    /// The part shown of each text long enough to show in parts, by the
    /// ID of its block.
    parts: HashMap<String, usize>,
    /// Counts each item opened, for evicting the least recently opened.
    opened: u64,
    fetch: Option<Task<()>>,
    /// Retries a failed fetch when its backoff is over.
    retry: Option<Task<()>>,
    _watch: Option<ActivityWatch>,
    _updates: Option<Subscription>,
    _poll: Option<Task<()>>,
    /// The client a test reads items with, in place of the service's.
    #[cfg(test)]
    client: Option<FleetClient>,
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
        parts: HashMap::new(),
        opened: 0,
        fetch: None,
        retry: None,
        _watch: watch,
        _updates: updates,
        _poll: poll,
        #[cfg(test)]
        client: None,
    });
    sync(board, cx);
    cx.notify();
}

/// Closes the turns; the stream drops the subscription, and every read
/// in flight is cancelled.
pub(crate) fn close(board: &mut Irc, cx: &mut Context<Irc>) {
    if board.inspector_activity.take().is_some() {
        cx.notify();
    }
}

/// The open turns' fleet, captain (by roster nick) and registration ID.
pub(crate) fn selected(board: &Irc) -> Option<(String, String, String)> {
    board.inspector_activity.as_ref().map(|selection| {
        (
            selection.fleet.clone(),
            selection.captain.clone(),
            selection.agent.clone(),
        )
    })
}

/// What the service holds of the captain's fleet for the join.
struct Service {
    state: FleetState,
    news: Option<subspace_client_fleet::activity::ActivityNews>,
    client: Option<FleetClient>,
}

fn service(api: &FleetApi, selection: &Selection) -> Service {
    Service {
        state: api.state(&selection.fleet),
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
    let changed = absorb(board, &service.state, service.news);
    if start_fetch(board, service.client, cx) || changed {
        cx.notify();
    }
}

fn absorb(
    board: &mut Irc,
    state: &FleetState,
    news: Option<subspace_client_fleet::activity::ActivityNews>,
) -> bool {
    let Some(selection) = board.inspector_activity.as_mut() else {
        return false;
    };
    selection.join.set_fleet_state(state);
    let changed = news.is_some_and(|news| selection.join.absorb(news));
    if changed {
        prune(selection);
    }
    changed
}

/// Fetches again after a failure, as **Try again** asks.
pub(super) fn retry(board: &mut Irc, cx: &mut Context<Irc>) {
    let Some(selection) = board.inspector_activity.as_mut() else {
        return;
    };
    if selection.join.retry() {
        selection.retry = None;
        sync(board, cx);
        cx.notify();
    }
}

/// Shows part `part` of the long text in the block `block`.
pub(super) fn show_part(board: &mut Irc, block: &str, part: usize, cx: &mut Context<Irc>) {
    if let Some(selection) = board.inspector_activity.as_mut() {
        selection.parts.insert(block.to_owned(), part);
        cx.notify();
    }
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
        .children(retry_action(selection, cx))
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

/// **Try again** beside a failed fetch, unless the gateway has no such
/// route or officer.
fn retry_action(selection: &Selection, cx: &Context<Irc>) -> Option<AnyElement> {
    selection.join.retry_in()?;
    Some(
        action_button(
            "irc-activity-retry",
            Intent::Neutral,
            Some(IconName::RefreshCw),
            "Try again",
        )
        .on_click(cx.listener(|board, _, _, cx| retry(board, cx)))
        .into_any_element(),
    )
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

/// The items `selection` read in full or is reading, by turn and item ID.
#[cfg(test)]
pub(crate) fn loaded_keys(selection: &Selection) -> Vec<(String, String)> {
    let mut keys: Vec<_> = selection.loaded.keys().cloned().collect();
    keys.sort();
    keys
}

/// Makes `selection` read items with `client` in place of the service's.
#[cfg(test)]
pub(crate) fn set_client(selection: &mut Selection, client: FleetClient) {
    selection.client = Some(client);
}

/// Whether a retry of `selection`'s failed fetch waits on its timer.
#[cfg(test)]
pub(crate) fn retry_scheduled(selection: &Selection) -> bool {
    selection.retry.is_some()
}

/// Answers the fetch the join started with `result`, as a read from the
/// service would.
#[cfg(test)]
pub(crate) fn finish_fetch(
    board: &mut Irc,
    result: Result<subspace_client_fleet::activity::AgentActivity, Error>,
    cx: &mut Context<Irc>,
) {
    fetch::finish_fetch(board, result, cx);
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
