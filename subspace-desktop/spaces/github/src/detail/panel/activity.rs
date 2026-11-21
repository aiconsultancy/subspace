//! The activity events of the detail panel.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use subspace_framework::client_github::model::{GhEvent, display_name};
use subspace_framework::dashboard::rich::{Format, rich_text};
use subspace_framework::dashboard::{Tokens, links};

use super::{
    EVENT_BODY_INDENT_PX, EVENT_GAP_PX, OnAction, PanelInput, SECTION_GAP_PX, action_bar, glyph,
    heading, note,
};
use crate::model::age;
use crate::rows::{person_glyph, reviewer_badge};

/// The activity events and the second action bar below them, in the panel
/// over the diff view.
pub fn activity(
    body: Div,
    input: &PanelInput<'_>,
    tokens: &Tokens,
    key: &str,
    on: &OnAction,
) -> Div {
    if input.detail.is_none() {
        return body;
    }
    events(body, input, tokens, key).child(
        div()
            .mt(px(SECTION_GAP_PX))
            .child(action_bar(input, "bottom", tokens, on)),
    )
}

/// The activity events under their heading, once the details have loaded.
pub fn events(body: Div, input: &PanelInput<'_>, tokens: &Tokens, key: &str) -> Div {
    let Some(detail) = input.detail else {
        return body;
    };
    body.child(heading(
        format!("Activity ({})", detail.events.len()),
        tokens,
    ))
    .children(
        detail
            .events
            .iter()
            .enumerate()
            .map(|(index, event)| event_row(index, event, input, tokens, key)),
    )
}

fn event_row(
    index: usize,
    event: &GhEvent,
    input: &PanelInput<'_>,
    tokens: &Tokens,
    key: &str,
) -> impl IntoElement {
    v_flex()
        .mt(px(EVENT_GAP_PX))
        .gap_1()
        .child(event_header(index, event, input, tokens))
        .when(!event.body_html.trim().is_empty(), |el| {
            el.child(div().pl(px(EVENT_BODY_INDENT_PX)).child(rich_text(
                ElementId::Name(format!("gh-event-{key}-{index}").into()),
                Format::Html,
                &event.body_html,
            )))
        })
}

fn event_header(
    index: usize,
    event: &GhEvent,
    input: &PanelInput<'_>,
    tokens: &Tokens,
) -> impl IntoElement {
    let name = event
        .author
        .as_ref()
        .map_or("ghost".to_string(), |user| display_name(user).to_string());
    let url = event
        .url
        .clone()
        .filter(|url| links::safe_url(url).is_some());
    let when = format!("{} ago", age(&event.created_at, input.now));
    h_flex()
        .gap_2()
        .child(glyph(
            person_glyph(event.author.as_ref(), input.account),
            tokens,
        ))
        .child(div().text_color(tokens.fg_strong).child(name))
        .when_some(event.review_state, |row, state| {
            let (badge, tag) = reviewer_badge(state);
            row.child(glyph(badge, tokens)).child(note(tag, tokens))
        })
        .child(div().flex_1())
        .child(
            div()
                .id(("gh-event-time", index))
                .text_color(tokens.muted)
                .when(url.is_some(), |el| el.cursor_pointer())
                .child(when)
                .on_click(move |_, _, cx| {
                    if let Some(url) = &url {
                        links::open(url, cx);
                    }
                }),
        )
}
