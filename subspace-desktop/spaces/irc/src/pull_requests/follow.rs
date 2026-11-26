//! The hub's side of the `github` contract: it follows `changed` from its
//! start and asks once per frame for the pull requests the transcript shows.
//! A click selects the pull request in the PRs Space; without a running PRs
//! Space it opens the original link in the browser and says so.

use gpui_kit::{App, Context, Entity};
use subspace_client_irc::BufferKey;
use subspace_framework::contracts::github::recognition::PullRequestLink;
use subspace_framework::contracts::github::{AccessChanged, Changed};
use subspace_framework::dashboard::{DashboardEvent, Toast, contracts, links};

use crate::hub::Hub;
use crate::markup::PrRef;
use crate::{ID, Irc, back};

/// What the toast says when the PRs Space is not running.
const UNAVAILABLE: &str = "PRs is unavailable. Opening in browser.";
const CANNOT_OPEN: &str = "This link cannot be opened.";

/// Subscribes the process-owned hub to `changed`. Without the broker, as
/// in headless runs, chips simply have no details.
pub(crate) fn attach(hub: &Entity<Hub>, cx: &mut App) {
    _ = contracts::subscribe(
        ID,
        hub,
        |hub: &mut Hub, _: &AccessChanged, cx| {
            hub.pull_requests.changed();
            cx.notify();
        },
        cx,
    );
    _ = contracts::subscribe(
        ID,
        hub,
        |hub: &mut Hub, _: &Changed, cx| {
            hub.pull_requests.refreshed();
            cx.notify();
        },
        cx,
    );
}

/// Notes the pull requests a row shows. The first new one of a frame
/// defers one request for all of them until the frame is drawn.
pub(crate) fn want(hub: &mut Hub, keys: Vec<String>, cx: &mut Context<Hub>) {
    if hub.pull_requests.want(keys) {
        let hub = cx.weak_entity();
        cx.defer(move |cx| _ = hub.update(cx, resolve_wanted));
    }
}

fn resolve_wanted(hub: &mut Hub, cx: &mut Context<Hub>) {
    if hub
        .pull_requests
        .resolve_wanted(|request| contracts::request(ID, request, cx))
    {
        cx.notify();
    }
}

/// Selects the pull request in the running PRs Space, or opens the link as
/// written in the browser. A disabled Space is never enabled for it.
pub(crate) fn open(found: &PrRef, from: &BufferKey, cx: &mut Context<Irc>) {
    let navigation = navigation_url(found)
        .and_then(|url| contracts::github::pull_request_navigation(&url))
        .filter(|navigation| contracts::navigation_target(ID, navigation.responder, cx).is_some());
    match navigation {
        Some(navigation) => cx.emit(DashboardEvent::Navigate(
            subspace_framework::dashboard::BoardNavigation {
                // The conversation clicked, not the one last focused.
                origin: back::place(from),
                ..navigation
            },
        )),
        None => fallback(&found.url, cx),
    }
}

fn fallback(url: &str, cx: &mut Context<Irc>) {
    let toast = if links::open(url, cx) {
        Toast::info(UNAVAILABLE)
    } else {
        Toast::error(CANNOT_OPEN)
    };
    cx.emit(DashboardEvent::Toast(toast));
}

/// The canonical URL the PRs Space opens: the link as written without
/// GitHub's tab, query or fragment, keeping the owner's case.
pub(super) fn navigation_url(found: &PrRef) -> Option<String> {
    PullRequestLink::parse_lenient(&found.url).map(|link| link.url())
}
