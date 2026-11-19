//! Available chips and one controlled summary panel within the viewer's scope.
mod disclosure;
use super::{tile, tooltip};
use crate::{markup::Ref, state::Entry, transcript::RowContext};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::contracts::issues::{IssueSummary, ResolutionState};
use subspace_framework::dashboard::{Tokens, action::card_button, reference_card};

pub(crate) fn tiles(
    entry: &Entry,
    identifiers: &[String],
    context: &RowContext,
    tokens: &Tokens,
    cx: &App,
) -> Option<AnyElement> {
    let hub = context.hub.upgrade()?;
    let hub = hub.read(cx);
    let issues = &hub.issues;
    issues.scope()?;
    let cards: Vec<_> = identifiers
        .iter()
        .filter_map(|id| match issues.state(id) {
            Some(ResolutionState::Available(summary)) => Some(summary),
            _ => None,
        })
        .collect();
    (!cards.is_empty()).then(|| {
        v_flex()
            .w_full()
            .min_w_0()
            .mt_1()
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .gap_1p5()
                    .items_start()
                    .flex_wrap()
                    .children(
                        cards
                            .iter()
                            .map(|summary| chip(entry, summary, context, tokens, cx)),
                    ),
            )
            .children(disclosure::panel(entry, &cards, context, cx))
            .into_any_element()
    })
}
fn chip(
    entry: &Entry,
    summary: &IssueSummary,
    context: &RowContext,
    tokens: &Tokens,
    cx: &App,
) -> AnyElement {
    let seq = entry.seq;
    let reference = Ref::Issue(summary.identifier.clone());
    let claim = context.claim.clone();
    let id: SharedString = format!("irc-line:{seq}:issue-preview:{}", summary.identifier).into();
    let now = context
        .hub
        .upgrade()
        .map(|hub| hub.read(cx).now)
        .unwrap_or_default();
    let button = card_button(
        SharedString::from(format!("{id}:open")),
        tile::label(summary, now),
        tile::chip(summary, tokens, cx),
    )
    .min_w_0()
    .on_click(move |_, window, cx| claim(&reference, window, cx));
    let identifier = summary.identifier.clone();
    let hub = context.hub.clone();
    h_flex()
        .min_w_0()
        .gap_0p5()
        .child(
            div()
                .id(id)
                .test_support()
                .min_w_0()
                .tooltip(move |window, cx| tooltip::tooltip(&identifier, hub.clone(), window, cx))
                .child(reference_card::surface(
                    button,
                    reference_card::Treatment::Chip,
                    tokens,
                    cx,
                )),
        )
        .child(disclosure::toggle(entry, &summary.identifier, context, cx))
        .into_any_element()
}
