//! The dated project rail and on-demand milestone previews.

use chrono::NaiveDate;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    StatefulInteractiveElement as _, Styled as _, div, px, relative,
};
use subspace_api_client::projects::{Milestone, MilestoneStatus, PlanningProject};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

use super::{date_label, failure, linked_issues};
use crate::{Projects, Record};

const MARKER_SIZE: Pixels = px(7.);
const TRACK_HEIGHT: Pixels = px(1.);
const TICK_HIT_SIZE: Pixels = px(17.);

pub(super) fn render(
    this: &Projects,
    record: &Record,
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> AnyElement {
    let rows = match &record.milestones {
        Ok(rows) => rows,
        Err(error) => return failure(&error.to_string(), tokens, cx),
    };
    let project = &record.project;
    let dates = timeline_dates(project, rows);
    let mut ordered: Vec<_> = rows.iter().collect();
    ordered.sort_by_key(|row| row.target_date.as_deref().unwrap_or("9999-12-31"));
    v_flex()
        .gap_3()
        .child(timeline_axis(&dates, rows, tokens, cx))
        .child(
            h_flex()
                .flex_wrap()
                .gap_2()
                .children(ordered.into_iter().map(|row| milestone_marker(row, cx))),
        )
        .children(
            this.milestone_pages
                .preview
                .as_deref()
                .and_then(|id| linked_issues::preview(this, id, tokens)),
        )
        .into_any_element()
}

fn milestone_marker(row: &Milestone, cx: &Context<Projects>) -> impl IntoElement {
    let id = row.id.clone();
    let hovered_id = id.clone();
    div()
        .id(format!("timeline-hover:{id}"))
        .on_hover(cx.listener(move |this, hovered, _, cx| {
            this.preview_milestone(&hovered_id, *hovered, cx);
        }))
        .child(
            action_button(
                format!("timeline-milestone:{id}"),
                if row.status == MilestoneStatus::Next {
                    Intent::Neutral
                } else {
                    Intent::Quiet
                },
                Some(IconName::Flag),
                milestone_label(row),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.open_milestone(&id, cx))),
        )
}

fn milestone_label(row: &Milestone) -> String {
    let prefix = if row.status == MilestoneStatus::Next {
        "Next · "
    } else {
        ""
    };
    format!(
        "{prefix}{} · {}",
        row.name,
        date_label(row.target_date.as_deref())
    )
}

fn timeline_dates(project: &PlanningProject, milestones: &[Milestone]) -> Vec<NaiveDate> {
    let mut dates: Vec<_> = [
        project.start_date.as_deref(),
        project.target_date.as_deref(),
    ]
    .into_iter()
    .chain(
        milestones
            .iter()
            .map(|milestone| milestone.target_date.as_deref()),
    )
    .flatten()
    .filter_map(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok())
    .collect();
    dates.sort_unstable();
    dates.dedup();
    dates
}

fn timeline_axis(
    dates: &[NaiveDate],
    rows: &[Milestone],
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> Div {
    let Some((&first, &last)) = dates.first().zip(dates.last()) else {
        return div().text_color(tokens.muted).child("No dates yet.");
    };
    let duration = (last - first).num_days().max(1) as f32;
    v_flex()
        .gap_1()
        .child(
            div()
                .relative()
                .h_5()
                .mr_4()
                .child(
                    div()
                        .absolute()
                        .top_2()
                        .w_full()
                        .h(TRACK_HEIGHT)
                        .bg(tokens.line),
                )
                .children(dates.iter().map(|date| {
                    axis_point(
                        *date,
                        (*date - first).num_days() as f32 / duration,
                        rows,
                        tokens,
                        cx,
                    )
                })),
        )
        .child(
            h_flex()
                .justify_between()
                .text_sm()
                .text_color(tokens.muted)
                .child(first.format("%-d %b %Y").to_string())
                .children((last != first).then(|| last.format("%-d %b %Y").to_string())),
        )
}

fn axis_point(
    date: NaiveDate,
    position: f32,
    rows: &[Milestone],
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> AnyElement {
    let date = date.to_string();
    let mut on_date = rows
        .iter()
        .filter(|row| row.target_date.as_deref() == Some(&date));
    let milestone = on_date.next();
    let color = if milestone.is_some() {
        tokens.accent
    } else {
        tokens.muted
    };
    let mark = div()
        .absolute()
        .top_0()
        .left(relative(position))
        .size(TICK_HIT_SIZE)
        .flex()
        .items_center()
        .justify_center()
        .child(div().size(MARKER_SIZE).rounded_full().bg(color));
    if let Some(row) = milestone.filter(|_| on_date.next().is_none()) {
        let id = row.id.clone();
        let hover_id = id.clone();
        return mark
            .id(format!("timeline-point:{id}"))
            .cursor_pointer()
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.preview_milestone(&hover_id, *hovered, cx);
            }))
            .on_click(cx.listener(move |this, _, _, cx| this.open_milestone(&id, cx)))
            .into_any_element();
    }
    mark.into_any_element()
}
