//! A row's lanes in Subspace API's order: priority, identifier, state and tracker
//! marks before the title; progress or sub-issue count, labels, milestone,
//! project, event, due date, estimate and assignee after it. As the window
//! narrows, project goes first, then labels, as in Subspace API.

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Div, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div, px, rems,
};
use subspace_api_client::model::{IssueListItem, Label};
use subspace_framework::dashboard::{Glyph, Group, Row, Tokens, Tone, avatar, feedback, truncated};
use subspace_framework::lucide::{IconName, SvgIcon};

use super::progress::progress;
use crate::Issues;
use crate::lines::{self, Line};
use crate::listing::More;
use crate::model::{IssuePriority, StateKind};

/// Identifiers line up, as Subspace API's monospaced identifier column does.
const IDENTIFIER_WIDTH_PX: f32 = 68.;
/// The tracker mark's slot, kept on every row so titles line up.
const TRACKER_SLOT_PX: f32 = 14.;
const ASSIGNEE_SIZE_PX: f32 = 20.;
const LABEL_DOT_PX: f32 = 7.;
/// The widest a chip grows before its name is cut, so one long label cannot
/// push the row's other lanes aside. In rem, so it follows the text size.
const CHIP_MAX_WIDTH_REM: f32 = 12.;
/// Below these window widths, the project lane and then the labels go.
const LABELS_MIN_WIDTH_PX: f32 = 960.;
/// A window width before the first render, when every lane shows.
const UNKNOWN_WIDTH: f32 = 0.;
/// `#RRGGBB`, without the `#`.
const HEX_COLOUR_DIGITS: usize = 6;
const HEX_RADIX: u32 = 16;
/// `YYYY-MM-DD`, the date part of Subspace API's dates.
const DATE_CHARS: usize = 10;

/// Which optional lanes fit the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Lanes {
    pub(crate) labels: bool,
}

impl Lanes {
    /// Before the first render the width is unknown, and every lane shows.
    pub(crate) fn for_width(width: f32) -> Lanes {
        let known = width > UNKNOWN_WIDTH;
        Lanes {
            labels: !known || width >= LABELS_MIN_WIDTH_PX,
        }
    }
}

pub(crate) fn state_icon(kind: StateKind) -> (SvgIcon, Tone) {
    match kind {
        StateKind::Triage => (IconName::CircleDotDashed, Tone::Orange),
        StateKind::Backlog => (IconName::CircleDashed, Tone::Muted),
        StateKind::Unstarted => (IconName::Circle, Tone::Fg),
        StateKind::Started => (IconName::Contrast, Tone::Yellow),
        StateKind::Completed => (IconName::CircleCheck, Tone::Accent),
        StateKind::Canceled => (IconName::CircleX, Tone::Muted),
        StateKind::Duplicate => (IconName::CircleSlash, Tone::Muted),
        StateKind::Other => (IconName::Circle, Tone::Muted),
    }
}

fn mark((icon, tone): (SvgIcon, Tone), tokens: &Tokens) -> Icon {
    Icon::from(icon).xsmall().text_color(tokens.tone(tone))
}

/// The leading lane's id, by its row's key.
pub(crate) fn lead_id(key: &str) -> SharedString {
    format!("issues-lead:{key}").into()
}

/// Before the title: priority, identifier, state, and the tracker mark.
pub(crate) fn leading(issues: &Issues, row: &Row, cx: &App) -> AnyElement {
    let tokens = Tokens::current(cx);
    let lane = h_flex()
        .id(lead_id(&row.key))
        .test_support()
        .flex_shrink_0();
    let Some(item) = issues.issue(&row.key) else {
        return lane.into_any_element();
    };
    let fields = &item.fields;
    let priority = IssuePriority::from_value(fields.priority).look();
    let tracker = fields.kind == "tracker";
    lane.gap_2()
        .child(mark((priority.icon, priority.tone), &tokens))
        .child(identifier(&fields.identifier, cx))
        .child(mark(
            state_icon(StateKind::parse(&fields.state.kind)),
            &tokens,
        ))
        .child(div().w(px(TRACKER_SLOT_PX)).when(tracker, |slot| {
            slot.child(mark((IconName::Split, Tone::Muted), &tokens))
        }))
        .into_any_element()
}

pub(crate) fn identifier(text: &str, cx: &App) -> Div {
    div()
        .w(px(IDENTIFIER_WIDTH_PX))
        .font_family(cx.theme().mono_font_family.clone())
        .text_xs()
        .text_color(Tokens::current(cx).muted)
        .child(text.to_owned())
}

/// A state group's icon.
pub(crate) fn group_mark(issues: &Issues, group: &Group, cx: &App) -> Option<AnyElement> {
    let Some(Line::Group(key)) = lines::line(&group.key) else {
        return None;
    };
    let kind = issues
        .listing
        .content
        .as_ref()?
        .groups
        .iter()
        .find(|found| found.key == key)?
        .kind;
    Some(mark(state_icon(kind), &Tokens::current(cx)).into_any_element())
}

/// After the title: the lanes, or a failed *more* line's Retry.
pub(crate) fn trailing(issues: &Issues, row: &Row, cx: &App) -> Option<AnyElement> {
    let tokens = Tokens::current(cx);
    if let Some(Line::More(group)) = lines::line(&row.key) {
        return more_retry(issues, group);
    }
    let item = issues.issue(&row.key)?;
    Some(item_lanes(item, Lanes::for_width(issues.width), &tokens).into_any_element())
}

fn more_retry(issues: &Issues, group: &str) -> Option<AnyElement> {
    let failed = issues
        .listing
        .content
        .as_ref()?
        .groups
        .iter()
        .any(|found| found.key == group && found.more == More::Failed);
    let entity = issues.weak.clone();
    let group = group.to_owned();
    failed.then(|| {
        feedback::retry_action(more_retry_id(&group), move |_, _, cx| {
            _ = entity.update(cx, |this, cx| this.read_more(&group, cx));
        })
        .into_any_element()
    })
}

/// A failed *more* line's Retry, by its group's key.
pub(crate) fn more_retry_id(group: &str) -> SharedString {
    format!("issues-more-retry:{group}").into()
}

fn item_lanes(item: &IssueListItem, lanes: Lanes, tokens: &Tokens) -> Div {
    let fields = &item.fields;
    let id = fields.id.as_str();
    h_flex()
        .flex_shrink_0()
        .gap_3()
        .text_xs()
        .text_color(tokens.muted)
        .children(progress(item, tokens))
        .when(lanes.labels, |row| {
            row.children(
                fields
                    .labels
                    .iter()
                    .map(|label| label_chip(id, label, tokens)),
            )
        })
        .children(fields.due_date.as_deref().and_then(short_date))
        .children(fields.estimate.map(estimate))
        .child(assignee(item, tokens))
}

/// A label in Subspace API's own colour.
fn label_chip(issue: &str, label: &Label, tokens: &Tokens) -> AnyElement {
    let colour =
        label_rgb(&label.color).map_or(tokens.muted, |rgb| tokens.glyph(&Glyph::rgb("●", rgb)));
    let dot = div().size(px(LABEL_DOT_PX)).rounded_full().bg(colour);
    chip(
        ("Label", label.name.clone()),
        issue,
        dot.into_any_element(),
        tokens,
    )
}

/// A bordered chip, named for assistive technology as `Kind: name`.
fn chip(
    (kind, name): (&str, String),
    issue: &str,
    mark: AnyElement,
    tokens: &Tokens,
) -> AnyElement {
    let label = format!("{kind}: {name}");
    let name_id = SharedString::from(format!("issues-chip-name:{issue}:{label}"));
    h_flex()
        .id(SharedString::from(format!("issues-chip:{issue}:{label}")))
        .aria_label(label)
        .test_support()
        .flex_shrink_0()
        .min_w_0()
        .max_w(rems(CHIP_MAX_WIDTH_REM))
        .overflow_hidden()
        .gap_1()
        .px_1p5()
        .rounded(tokens.control_radius)
        .border_1()
        .border_color(tokens.line)
        .text_color(tokens.fg)
        .child(div().flex_shrink_0().child(mark))
        .child(truncated::text(name_id, name))
        .into_any_element()
}

fn estimate(points: f64) -> String {
    format!("{points}")
}

/// The assignee's initials, or an empty slot.
fn assignee(item: &IssueListItem, tokens: &Tokens) -> AnyElement {
    match &item.fields.assignee {
        Some(person) => avatar::member(person.display_name.clone(), tokens).into_any_element(),
        None => div()
            .size(px(ASSIGNEE_SIZE_PX))
            .rounded(tokens.control_radius)
            .border_1()
            .border_color(tokens.hair)
            .into_any_element(),
    }
}

/// `#RRGGBB` as a colour value.
pub(crate) fn label_rgb(colour: &str) -> Option<u32> {
    let hex = colour.strip_prefix('#')?;
    (hex.len() == HEX_COLOUR_DIGITS).then(|| u32::from_str_radix(hex, HEX_RADIX).ok())?
}

/// `2026-12-17` as `17 Dec`.
pub(crate) fn short_date(date: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let mut parts = date.get(..DATE_CHARS)?.split('-');
    let (_year, month, day) = (parts.next()?, parts.next()?, parts.next()?);
    let month = MONTHS.get(month.parse::<usize>().ok()?.checked_sub(1)?)?;
    let day: u32 = day.parse().ok()?;
    Some(format!("{day} {month}"))
}

#[cfg(test)]
mod tests;
