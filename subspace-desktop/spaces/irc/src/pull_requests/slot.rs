//! A pull request's tile slot under the message that keeps it: the tile
//! once the answering Space has given its details, otherwise a same-sized
//! loading or unresolved tile, so an answer never changes the row's height.
//! A click opens it as its inline reference does; the hover card is the
//! reference's.

use gpui_kit::component::Disableable as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::contracts::github::{PullRequestDetails, ResolutionState};
use subspace_framework::dashboard::reference_card::{self, Treatment};
use subspace_framework::dashboard::{Tokens, action::card_button};

use super::compact::{self, number};
use crate::markup::{PrRef, Ref};
use crate::state::Entry;
use crate::transcript::RowContext;

/// The tile for `found` under `entry`.
pub(crate) fn slot(entry: &Entry, found: &PrRef, context: &RowContext, cx: &App) -> AnyElement {
    let hub = context.hub.upgrade();
    let state = hub
        .as_ref()
        .and_then(|hub| hub.read(cx).pull_requests.state(&found.key));
    match state {
        Some(ResolutionState::Available(details)) => available(entry, found, details, context, cx),
        state => reserved(entry, found, state, context, cx),
    }
}

fn available(
    entry: &Entry,
    found: &PrRef,
    details: &PullRequestDetails,
    context: &RowContext,
    cx: &App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let id: SharedString = format!("irc-line:{}:pr-tile:{}", entry.seq, found.key).into();
    let reference = Ref::PullRequest(found.clone());
    let claim = context.claim.clone();
    let button = card_button(
        SharedString::from(format!("{id}:open")),
        compact::label(details),
        compact::tile(details, &tokens, cx),
    )
    .min_w_0()
    .on_click(move |_, window, cx| claim(&reference, window, cx));
    let surface = reference_card::surface(button, Treatment::Outline, &tokens, cx);
    crate::tiles::slot(hoverable(id, found, context, surface)).into_any_element()
}

/// A slot without details: loading until the answer comes, then the short
/// reason there are none, which never says whether the pull request
/// exists. Inert; the inline reference stays the way to open it.
fn reserved(
    entry: &Entry,
    found: &PrRef,
    state: Option<&ResolutionState>,
    context: &RowContext,
    cx: &App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let key = number(&found.key);
    let (kind, content) = match absence(state) {
        Some(reason) => (
            "unavailable",
            crate::tiles::unresolved(key, reason, &tokens, cx),
        ),
        None => ("loading", crate::tiles::loading(key, &tokens, cx)),
    };
    let id: SharedString = format!("irc-line:{}:pr-{kind}:{}", entry.seq, found.key).into();
    let label = super::tooltip::note(found.label(), state);
    let button = card_button(SharedString::from(format!("{id}:slot")), label, content)
        .min_w_0()
        .disabled(true);
    let surface = reference_card::surface(button, Treatment::Outline, &tokens, cx);
    crate::tiles::slot(hoverable(id, found, context, surface)).into_any_element()
}

/// The short reason an answered pull request has no details; `None` while
/// it has no answer yet.
pub(super) fn absence(state: Option<&ResolutionState>) -> Option<&'static str> {
    match state {
        Some(ResolutionState::Unavailable) => Some("Unavailable"),
        Some(ResolutionState::SignedOut) => Some("Signed out"),
        Some(ResolutionState::Failed) => Some("Couldn't load"),
        _ => None,
    }
}

/// A tile's surface with the reference's hover card or note.
fn hoverable(id: SharedString, found: &PrRef, context: &RowContext, surface: Div) -> AnyElement {
    let found = found.clone();
    let hub = context.hub.clone();
    div()
        .id(id)
        .test_support()
        .w_full()
        .min_w_0()
        .tooltip(move |window, cx| super::tooltip(&found, hub.clone(), window, cx))
        .child(surface)
        .into_any_element()
}
