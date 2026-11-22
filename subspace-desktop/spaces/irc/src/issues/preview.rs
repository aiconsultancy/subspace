//! An issue's tile under the message that keeps it, its size reserved from
//! the first frame, and one controlled summary panel within the viewer's
//! scope.
mod disclosure;
use super::{tile, tooltip};
use crate::{markup::Ref, state::Entry, transcript::RowContext};
use gpui_kit::component::{Disableable as _, h_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::contracts::issues::{IssueSummary, ResolutionState};
use subspace_framework::dashboard::avatar::AgentIcons;
use subspace_framework::dashboard::{Tokens, action::card_button, reference_card};

/// The tile for issue `identifier` under `entry`, the message that keeps
/// it: the tile once available, otherwise a same-sized loading or
/// unresolved tile, so an answer never changes the row's height.
pub(crate) fn slot(entry: &Entry, identifier: &str, context: &RowContext, cx: &App) -> AnyElement {
    let hub = context.hub.upgrade();
    let state = hub
        .as_ref()
        .and_then(|hub| hub.read(cx).issues.state(identifier));
    match state {
        Some(ResolutionState::Available(summary)) => chip(entry, summary, context, cx),
        state => reserved(entry, identifier, state, context, cx),
    }
}

/// The summary panel a tile's disclosure opened, under the tiles of
/// `entry`, which shows `identifiers`.
pub(crate) fn panel(
    entry: &Entry,
    identifiers: &[&str],
    context: &RowContext,
    cx: &App,
) -> Option<AnyElement> {
    let hub = context.hub.upgrade()?;
    let issues = &hub.read(cx).issues;
    let cards: Vec<&IssueSummary> = identifiers
        .iter()
        .filter_map(|id| match issues.state(id) {
            Some(ResolutionState::Available(summary)) => Some(summary),
            _ => None,
        })
        .collect();
    disclosure::panel(entry, &cards, context, cx)
}

/// A slot without a summary: loading until the answer comes, then the reason
/// there is none. The same chip surface as an available issue, inert, so
/// the row keeps its height; the inline reference stays the way to open it.
fn reserved(
    entry: &Entry,
    identifier: &str,
    state: Option<&ResolutionState>,
    context: &RowContext,
    cx: &App,
) -> AnyElement {
    let tokens = Tokens::current(cx);
    let kind = if tile::absence(state).is_some() {
        "unavailable"
    } else {
        "loading"
    };
    let id: SharedString = format!("irc-line:{}:issue-{kind}:{identifier}", entry.seq).into();
    let profile = context
        .hub
        .upgrade()
        .and_then(|hub| hub.read(cx).issues.profile_name().map(str::to_owned));
    let label = tooltip::note(
        identifier,
        state,
        profile.as_deref().unwrap_or(tooltip::TRACKER),
    );
    let content = match tile::absence(state) {
        Some(reason) => crate::tiles::unresolved(identifier.to_owned(), reason, &tokens, cx),
        None => crate::tiles::loading(identifier.to_owned(), &tokens, cx),
    };
    let button = card_button(SharedString::from(format!("{id}:slot")), label, content)
        .min_w_0()
        .disabled(true);
    crate::tiles::slot(hoverable(
        id,
        identifier,
        context,
        reference_card::surface(button, reference_card::Treatment::Outline, &tokens, cx),
    ))
    .into_any_element()
}

/// A chip's surface with the issue's hover card or note.
fn hoverable(
    id: SharedString,
    identifier: &str,
    context: &RowContext,
    surface: Div,
) -> impl IntoElement {
    let identifier = identifier.to_owned();
    let hub = context.hub.clone();
    div()
        .id(id)
        .test_support()
        .flex_1()
        .min_w_0()
        .tooltip(move |window, cx| tooltip::tooltip(&identifier, hub.clone(), window, cx))
        .child(surface)
}
fn chip(entry: &Entry, summary: &IssueSummary, context: &RowContext, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let seq = entry.seq;
    let reference = Ref::Issue(summary.identifier.clone());
    let claim = context.claim.clone();
    let id: SharedString = format!("irc-line:{seq}:issue-preview:{}", summary.identifier).into();
    let now = context
        .hub
        .upgrade()
        .map(|hub| hub.read(cx).now)
        .unwrap_or_default();
    let icons = AgentIcons::current(cx);
    let button = card_button(
        SharedString::from(format!("{id}:open")),
        tile::label(summary, now),
        tile::tile(summary, (now, icons), &tokens, cx),
    )
    .min_w_0()
    .on_click(move |_, window, cx| claim(&reference, window, cx));
    crate::tiles::slot(
        h_flex()
            .w_full()
            .min_w_0()
            .gap_0p5()
            .child(hoverable(
                id,
                &summary.identifier,
                context,
                reference_card::surface(button, reference_card::Treatment::Outline, &tokens, cx),
            ))
            .child(disclosure::toggle(entry, &summary.identifier, context, cx)),
    )
    .into_any_element()
}
