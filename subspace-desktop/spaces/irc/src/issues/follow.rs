//! The hub's side of the contract: it follows `changed` from its start, so
//! references stay current while the view is closed, and asks once per frame
//! for the references the transcript shows.

use gpui_kit::{App, Context, Entity};
use subspace_client_irc::BufferKey;
use subspace_framework::contracts::issues::{Changed, CurrentScope, ResolutionState};
use subspace_framework::dashboard::{BoardNavigation, DashboardEvent, contracts};

use super::IssueReferences;
use crate::hub::{self, Hub};
use crate::{ID, Irc, back};

/// Subscribes the process-owned hub to `changed` and asks for the scope.
/// Without the broker, as in headless runs, nothing is recognised.
pub(crate) fn attach(hub: &Entity<Hub>, cx: &mut App) {
    let attached = contracts::subscribe(
        ID,
        hub,
        |hub: &mut Hub, _: &Changed, cx| changed(hub, cx),
        cx,
    );
    if attached.is_ok() {
        hub.update(cx, refresh_scope);
    }
}

fn changed(hub: &mut Hub, cx: &mut Context<Hub>) {
    hub.issues.start_round();
    refresh_scope(hub, cx);
    cx.notify();
}

/// Asks for the scope; an unavailable answer is no scope.
fn refresh_scope(hub: &mut Hub, cx: &mut Context<Hub>) {
    let scope = contracts::request(ID, &CurrentScope, cx).ok().flatten();
    if hub.issues.set_scope(scope) {
        cx.notify();
    }
}

/// Notes the identifiers a row shows. The first new one of a frame defers
/// one request for all of them until the frame is drawn.
pub(crate) fn want(hub: &mut Hub, identifiers: Vec<String>, cx: &mut Context<Hub>) {
    if hub.issues.want(identifiers) {
        let hub = cx.weak_entity();
        cx.defer(move |cx| _ = hub.update(cx, resolve_wanted));
    }
}

fn resolve_wanted(hub: &mut Hub, cx: &mut Context<Hub>) {
    if hub
        .issues
        .resolve_wanted(|request| contracts::request(ID, request, cx))
    {
        cx.notify();
    }
}

/// Open the issue in its board, even if the hover summary has not loaded.
pub(crate) fn open(identifier: &str, from: &BufferKey, cx: &mut Context<Irc>) {
    let Some(hub) = hub::entity(cx) else {
        return;
    };
    if let Some(url) = navigation(&hub.read(cx).issues, identifier) {
        cx.emit(DashboardEvent::Navigate(BoardNavigation {
            responder: contracts::MessageKey::request::<CurrentScope>(),
            url: url.into(),
            // The conversation clicked, not the one last focused.
            origin: back::place(from),
        }));
    }
}

/// Bind the recognized identifier to the current board scope before routing.
pub(super) fn navigation(references: &IssueReferences, identifier: &str) -> Option<String> {
    let scope = references.scope.as_ref()?;
    if references.keys().leading(identifier) != Some(identifier.len()) {
        return None;
    }
    let url = match references.state(identifier) {
        Some(ResolutionState::Available(summary)) => summary.url.clone(),
        _ => format!(
            "{}/v1/issues/{identifier}",
            scope.api_origin.trim_end_matches('/')
        ),
    };
    scope.navigation_url(&url)
}
