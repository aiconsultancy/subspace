//! A row's lanes, in the rhythm of the pull request list: the status glyph
//! before the title; then, after it, where the row belongs, its pull
//! request, whether it is monitored and a right-aligned age. The lanes have
//! fixed widths, so their columns line up down the list and across groups.

use chrono::Utc;
use gpui_kit::StatefulInteractiveElement as _;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement as _, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, div, px,
};
use subspace_framework::dashboard::{Group, Tokens, Tone, truncated};
use subspace_framework::lucide::IconName;

use crate::dates;
use crate::model::{AgentRow, StateGroup};
use crate::peek;

/// Where the row belongs: its repository, or "Local chat".
const PLACE_LANE_PX: f32 = 144.;
/// The pull request's glyph and number.
const PULL_LANE_PX: f32 = 56.;
/// The monitored mark's slot, kept on every row.
const WATCH_LANE_PX: f32 = 14.;
/// The age, right-aligned.
const AGE_LANE_PX: f32 = 36.;

/// The id of a row's lanes after its title, which the tests find it by.
pub(crate) fn lanes_id(key: &str) -> SharedString {
    format!("github-copilot-agents-lanes:{key}").into()
}

/// The status glyph in its tone, named by the state in words.
pub(crate) fn leading(row: &AgentRow, cx: &App) -> Option<AnyElement> {
    let tokens = Tokens::current(cx);
    let status = peek::status(&row.state);
    Some(
        div()
            .id(SharedString::from(format!(
                "github-copilot-agents-status:{}",
                row.key
            )))
            .flex_shrink_0()
            .aria_label(status.words.clone())
            .child(
                Icon::from(status.icon)
                    .xsmall()
                    .text_color(tokens.tone(status.tone)),
            )
            .into_any_element(),
    )
}

/// The quiet facts after the title, then the age.
pub(crate) fn trailing(row: &AgentRow, cx: &App) -> Option<AnyElement> {
    let tokens = Tokens::current(cx);
    let age = dates::age(&row.updated_at, Utc::now());
    Some(
        h_flex()
            .id(lanes_id(&row.key))
            .test_support()
            .flex_shrink_0()
            .gap_3()
            .text_size(tokens.secondary_size())
            .text_color(tokens.muted)
            .child(place_lane(row))
            .child(pull_lane(row, &tokens))
            .child(watch_lane(row, &tokens))
            .child(
                h_flex()
                    .flex_shrink_0()
                    .w(px(AGE_LANE_PX))
                    .justify_end()
                    .child(age),
            )
            .into_any_element(),
    )
}

fn place_lane(row: &AgentRow) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .w(px(PLACE_LANE_PX))
        .min_w_0()
        .child(
            truncated::text(
                SharedString::from(format!("github-copilot-agents-place:{}", row.key)),
                peek::place(row),
            )
            .flex_1(),
        )
        .into_any_element()
}

fn pull_lane(row: &AgentRow, tokens: &Tokens) -> Div {
    let lane = h_flex().flex_shrink_0().w(px(PULL_LANE_PX)).gap_1();
    match peek::pull(row) {
        Some(pull) => lane
            .child(
                Icon::from(pull.status.icon)
                    .xsmall()
                    .text_color(tokens.tone(pull.status.tone)),
            )
            .child(format!("#{}", pull.number)),
        None => lane,
    }
}

fn watch_lane(row: &AgentRow, tokens: &Tokens) -> AnyElement {
    let lane = div()
        .id(SharedString::from(format!(
            "github-copilot-agents-watch:{}",
            row.key
        )))
        .flex_shrink_0()
        .w(px(WATCH_LANE_PX));
    match row.monitor {
        true => lane
            .aria_label("Monitored")
            .child(
                Icon::from(IconName::Bell)
                    .xsmall()
                    .text_color(tokens.tone(Tone::Accent)),
            )
            .into_any_element(),
        false => lane.into_any_element(),
    }
}

/// A state group's glyph in its tone, in place of the list's text glyph.
pub(crate) fn group_mark(group: &Group, cx: &App) -> Option<AnyElement> {
    let state = group
        .key
        .strip_prefix("group:")
        .and_then(StateGroup::from_id)?;
    let (icon, tone) = match state {
        StateGroup::NeedsInput => (IconName::CircleQuestionMark, Tone::Yellow),
        StateGroup::Active => (IconName::LoaderCircle, Tone::Blue),
        StateGroup::Finished => (IconName::CircleCheck, Tone::Muted),
    };
    let tokens = Tokens::current(cx);
    Some(
        Icon::from(icon)
            .xsmall()
            .flex_shrink_0()
            .text_color(tokens.tone(tone))
            .into_any_element(),
    )
}
