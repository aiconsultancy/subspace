//! A row's progress lane: a tracker's strip or bar with *n of m*, or an
//! ordinary parent's sub-issue count (design §8, departure 7).

use gpui_kit::component::progress::Progress;
use gpui_kit::component::{Icon, Sizable as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{AnyElement, Div, IntoElement, ParentElement as _, Styled as _, div, px};
use subspace_api_client::model::IssueListItem;
use subspace_framework::dashboard::{Tokens, Tone};
use subspace_framework::lucide::IconName;

/// Up to this many sub-issues draw as a strip of marks; more use a bar.
const STRIP_LIMIT: i32 = 16;
const STRIP_MARK_PX: f32 = 5.;
const STRIP_GAP_PX: f32 = 1.;
const BAR_WIDTH_PX: f32 = 56.;
const PERCENT: f32 = 100.;

/// How a row shows its progress.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Figure {
    /// A tracker with few sub-issues: one mark each.
    Strip,
    /// A tracker with many: a bar.
    Bar,
    /// An ordinary parent: its sub-issue count.
    Count,
}

/// A row's progress: its figure, counts, share done and words.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Shown {
    pub(crate) figure: Figure,
    pub(crate) done: i32,
    pub(crate) total: i32,
    pub(crate) percent: f32,
    pub(crate) text: String,
}

/// A tracker's *n of m*, else an ordinary parent's *n/m*, else nothing.
pub(crate) fn shown_progress(item: &IssueListItem) -> Option<Shown> {
    let progress = item.fields.progress.as_ref().filter(|p| p.total > 0)?;
    let figure = match (item.fields.kind.as_str(), progress.total) {
        ("tracker", total) if total <= STRIP_LIMIT => Figure::Strip,
        ("tracker", _) => Figure::Bar,
        _ => Figure::Count,
    };
    let (done, total) = (progress.done, progress.total);
    let text = if figure == Figure::Count {
        format!("{done}/{total}")
    } else {
        format!("{done} of {total}")
    };
    Some(Shown {
        figure,
        done,
        total,
        percent: done as f32 * PERCENT / total as f32,
        text,
    })
}

pub(crate) fn progress(item: &IssueListItem, tokens: &Tokens) -> Option<AnyElement> {
    let shown = shown_progress(item)?;
    let (done, total) = (shown.done, shown.total);
    Some(
        h_flex()
            .gap_1()
            .when(shown.figure == Figure::Strip, |row| {
                row.child(strip(done, total, tokens))
            })
            .when(shown.figure == Figure::Bar, |row| {
                row.child(bar(&item.fields.id, shown.percent, tokens))
            })
            .when(shown.figure == Figure::Count, |row| {
                row.child(
                    Icon::from(IconName::ListTree)
                        .xsmall()
                        .text_color(tokens.tone(Tone::Muted)),
                )
            })
            .child(shown.text)
            .into_any_element(),
    )
}

/// One mark per sub-issue: filled for done.
fn strip(done: i32, total: i32, tokens: &Tokens) -> Div {
    h_flex()
        .gap(px(STRIP_GAP_PX))
        .children((0..total).map(|index| {
            let filled = index < done;
            div()
                .size(px(STRIP_MARK_PX))
                .rounded_full()
                .border_1()
                .border_color(if filled { tokens.green } else { tokens.muted })
                .when(filled, |mark| mark.bg(tokens.green))
        }))
}

/// Kit's progress bar, for more sub-issues than a strip shows, in the
/// strip's done colour.
fn bar(id: &str, percent: f32, tokens: &Tokens) -> Div {
    div().w(px(BAR_WIDTH_PX)).child(
        Progress::new(format!("issues-progress-{id}"))
            .color(tokens.green)
            .value(percent),
    )
}
