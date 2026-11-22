//! A chip slot for every recognised issue, reserved from the first frame,
//! and one controlled summary panel within the viewer's scope.
mod disclosure;
use super::{tile, tooltip};
use crate::{markup::Ref, state::Entry, transcript::RowContext};
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_framework::contracts::issues::{IssueSummary, ResolutionState};
use subspace_framework::dashboard::{Tokens, action::card_button, reference_card};

/// One slot per issue `identifiers` names, from the first frame the scope
/// recognises them: a chip once available, otherwise a same-sized reserved
/// chip, so an answer never changes the row's height.
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
    let slots: Vec<_> = identifiers
        .iter()
        .map(|id| (id.as_str(), issues.state(id)))
        .collect();
    let cards = available(&slots);
    (!slots.is_empty()).then(|| {
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
                    .children(slots.iter().map(|&(id, state)| match state {
                        Some(ResolutionState::Available(summary)) => {
                            chip(entry, summary, context, tokens, cx)
                        }
                        state => reserved(entry, id, state, context, cx),
                    })),
            )
            .children(disclosure::panel(entry, &cards, context, cx))
            .into_any_element()
    })
}

/// The slots' available summaries, in order.
fn available<'a>(slots: &[(&str, Option<&'a ResolutionState>)]) -> Vec<&'a IssueSummary> {
    slots
        .iter()
        .filter_map(|(_, state)| match state {
            Some(ResolutionState::Available(summary)) => Some(summary),
            _ => None,
        })
        .collect()
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
    let button = card_button(
        SharedString::from(format!("{id}:slot")),
        label,
        tile::reserved(identifier, state, &tokens, cx),
    )
    .min_w_0()
    .disabled(true);
    hoverable(
        id,
        identifier,
        context,
        reference_card::surface(button, reference_card::Treatment::Chip, &tokens, cx),
    )
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
        .min_w_0()
        .tooltip(move |window, cx| tooltip::tooltip(&identifier, hub.clone(), window, cx))
        .child(surface)
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
    h_flex()
        .min_w_0()
        .gap_0p5()
        .child(hoverable(
            id,
            &summary.identifier,
            context,
            reference_card::surface(button, reference_card::Treatment::Chip, tokens, cx),
        ))
        .child(disclosure::toggle(entry, &summary.identifier, context, cx))
        .into_any_element()
}
