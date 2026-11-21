//! A row's lanes after its title: whether it supersedes or is superseded,
//! its linked issues, who made it and when it was decided; and the status
//! glyph of a group header. The lanes have fixed widths so their columns
//! line up down the list.

use chrono::{Local, NaiveDate};
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement as _, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px,
};
use subspace_api_client::model::DecisionListItem;
use subspace_framework::dashboard::{Group, Row, Tokens, truncated};
use subspace_framework::lucide::IconName;

use crate::words::{self, People};
use crate::{Decisions, listing};

const ISSUES_LANE_PX: f32 = 36.;
const MAKER_LANE_PX: f32 = 168.;
const DATE_LANE_PX: f32 = 84.;

/// The id of a decision row's lanes, which the tests find a row by.
pub(crate) fn lanes_id(key: &str) -> SharedString {
    format!("decision-lanes:{key}").into()
}

pub(crate) fn trailing(decisions: &Decisions, row: &Row, cx: &App) -> Option<AnyElement> {
    let item = decisions.item(listing::decision_id(&row.key)?)?;
    let tokens = Tokens::current(cx);
    let today = Local::now().date_naive();
    Some(lanes(
        &row.key,
        item,
        (&decisions.session.people, today),
        &tokens,
    ))
}

fn lanes(
    key: &str,
    item: &DecisionListItem,
    (people, today): (&People, NaiveDate),
    tokens: &Tokens,
) -> AnyElement {
    h_flex()
        .id(lanes_id(key))
        .test_support()
        .flex_shrink_0()
        .gap_3()
        .text_size(tokens.secondary_size())
        .text_color(tokens.muted)
        .children(words::supersession(item))
        .child(issues_lane(item, tokens))
        .child(maker_lane(item, people, tokens))
        .child(
            h_flex()
                .flex_shrink_0()
                .w(px(DATE_LANE_PX))
                .justify_end()
                .child(words::decided_short(item, today)),
        )
        .into_any_element()
}

/// The linked issue count beside an issue glyph.
fn issues_lane(item: &DecisionListItem, tokens: &Tokens) -> AnyElement {
    h_flex()
        .flex_shrink_0()
        .w(px(ISSUES_LANE_PX))
        .gap_1()
        .child(
            Icon::from(IconName::CircleDot)
                .xsmall()
                .text_color(tokens.muted),
        )
        .child(item.issues.count.to_string())
        .into_any_element()
}

/// Who made the decision, after a glyph that says person or agent.
fn maker_lane(item: &DecisionListItem, people: &People, tokens: &Tokens) -> AnyElement {
    let maker = words::maker_short(item, people);
    h_flex()
        .flex_shrink_0()
        .w(px(MAKER_LANE_PX))
        .min_w_0()
        .gap_1()
        .text_color(tokens.fg)
        .child(
            div()
                .id(SharedString::from(format!(
                    "decision-maker-kind:{}",
                    item.id
                )))
                .flex_shrink_0()
                .aria_label(words::maker_kind(item))
                .child(
                    Icon::from(words::maker_icon(item))
                        .xsmall()
                        .text_color(tokens.muted),
                ),
        )
        .child(
            truncated::text(
                SharedString::from(format!("decision-maker:{}", item.id)),
                maker,
            )
            .flex_1(),
        )
        .into_any_element()
}

/// A status group's glyph in its tone, in place of the list's text glyph.
pub(crate) fn group_mark(group: &Group, cx: &App) -> Option<AnyElement> {
    let status = listing::group_status(&group.key)?;
    let tokens = Tokens::current(cx);
    Some(
        Icon::from(words::status_icon(status))
            .xsmall()
            .flex_shrink_0()
            .text_color(tokens.tone(words::status_tone(status)))
            .into_any_element(),
    )
}
