//! A row's lanes, as the mock-up lays them out: the status glyph first, then
//! after the title the branch as a chip, who started the run and why, the
//! status in its tone and the age. Fixed widths keep the columns aligned
//! across rows and groups. A record with no run shows only its status.
//!
//! The lanes never take more than [`LANES_MAX_SHARE`] of the row, so the
//! title keeps the rest. In a narrow list the branch and the actor columns
//! yield first, then the status word is cut; the age keeps its width.

use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, Div, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, TestSupportExt as _, div, relative, rems,
};
use subspace_framework::dashboard::list::Row;
use subspace_framework::dashboard::{Tokens, truncated};

use crate::{Actions, model::Record, rows};

/// The branch chip's column, so a long branch is cut, not the row.
const BRANCH_WIDTH_REM: f32 = 9.;
/// The actor and event column.
const BY_WIDTH_REM: f32 = 9.;
/// Wide enough for "in progress".
const STATUS_WIDTH_REM: f32 = 5.5;
/// Wide enough for "11mo".
const AGE_WIDTH_REM: f32 = 2.5;
/// The most of a row the lanes take; the title keeps the rest.
pub(crate) const LANES_MAX_SHARE: f32 = 0.6;
/// How readily the status word gives way, beside the branch and actor
/// columns' 1: only once they have all but gone.
const STATUS_SHRINK: f32 = 0.05;

/// The record a list row draws.
fn record<'a>(this: &'a Actions, row: &Row) -> Option<&'a Record> {
    this.records()
        .iter()
        .find(|record| record.key == row.key.as_ref())
}

/// A status's glyph in its tone.
pub(crate) fn status_icon(status: &str, tokens: &Tokens) -> Icon {
    let (icon, tone) = rows::status_look(status);
    Icon::from(icon).xsmall().text_color(tokens.tone(tone))
}

/// The status glyph in place of the list's dot.
pub(crate) fn leading(this: &Actions, row: &Row, cx: &App) -> Option<AnyElement> {
    let record = record(this, row)?;
    let tokens = Tokens::current(cx);
    Some(
        div()
            .flex_shrink_0()
            .child(status_icon(&record.status, &tokens))
            .into_any_element(),
    )
}

/// Who started the run and why, such as "ann · push".
pub(crate) fn by_line(actor: &str, event: &str) -> String {
    [actor, event]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// After the title: branch, actor and event, status and age for a run; the
/// status alone for anything else.
pub(crate) fn trailing(this: &Actions, row: &Row, cx: &App) -> Option<AnyElement> {
    let record = record(this, row)?;
    let tokens = Tokens::current(cx);
    let lanes = h_flex()
        .id(SharedString::from(format!("actions-lanes:{}", row.key)))
        .test_support()
        .flex_shrink_1()
        .min_w_0()
        .max_w(relative(LANES_MAX_SHARE))
        .overflow_hidden()
        .gap_3()
        .text_xs();
    let status = status_lane(record, &tokens);
    let Some((run, facts)) = record.run.as_ref().zip(record.facts.as_ref()) else {
        return Some(lanes.child(status).into_any_element());
    };
    let mono = cx.theme().mono_font_family.clone();
    let by = by_line(&run.actor, &facts.event);
    let by_id = SharedString::from(format!("actions-by:{}", row.key));
    Some(
        lanes
            .child(branch_chip(&row.key, &run.branch, &tokens, cx))
            .child(
                yielding(BY_WIDTH_REM)
                    .text_color(tokens.muted)
                    .child(truncated::text(by_id, by)),
            )
            .child(status)
            .child(
                column(AGE_WIDTH_REM)
                    .justify_end()
                    .font_family(mono)
                    .text_color(tokens.muted)
                    .child(facts.age.clone()),
            )
            .into_any_element(),
    )
}

/// One fixed-width column of the lanes.
fn column(width: f32) -> Div {
    h_flex().flex_shrink_0().min_w_0().w(rems(width))
}

/// A column that is `width` wide while the row has room, and gives way
/// down to nothing when it has not.
fn yielding(width: f32) -> Div {
    column(width).flex_shrink_1().overflow_hidden()
}

/// The status as a word in its tone, so it reads without the colour too.
fn status_lane(record: &Record, tokens: &Tokens) -> Div {
    let (_, tone) = rows::status_look(&record.status);
    column(STATUS_WIDTH_REM)
        .flex_shrink(STATUS_SHRINK)
        .overflow_hidden()
        .font_weight(FontWeight::MEDIUM)
        .text_color(tokens.tone(tone))
        .child(
            div()
                .min_w_0()
                .truncate()
                .child(rows::status_word(&record.status)),
        )
}

/// The branch as a quiet monospace chip, cut to its column.
fn branch_chip(key: &str, branch: &str, tokens: &Tokens, cx: &App) -> Div {
    let slot = yielding(BRANCH_WIDTH_REM);
    if branch.is_empty() {
        return slot;
    }
    let name = SharedString::from(format!("actions-branch:{key}"));
    slot.child(
        h_flex()
            .min_w_0()
            .max_w_full()
            .overflow_hidden()
            .px_1p5()
            .rounded(tokens.control_radius)
            .border_1()
            .border_color(tokens.line)
            .font_family(cx.theme().mono_font_family.clone())
            .text_color(tokens.muted)
            .child(truncated::text(name, branch.to_owned())),
    )
}

#[cfg(test)]
mod tests {
    use super::by_line;

    #[test]
    fn the_by_line_names_who_and_why_and_skips_what_is_missing() {
        assert_eq!(by_line("ann", "push"), "ann · push");
        assert_eq!(by_line("", "schedule"), "schedule");
        assert_eq!(by_line("ann", ""), "ann");
    }
}
