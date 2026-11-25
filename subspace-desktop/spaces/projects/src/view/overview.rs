//! Read-only project facts, Markdown and milestone dates.

use chrono::NaiveDate;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, IntoElement as _, ParentElement as _, Pixels,
    Styled as _, div, px, relative,
};
use subspace_api_client::projects::{Milestone, MilestoneStatus, PlanningProject};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
};
use subspace_framework::lucide::IconName;

use super::{date_label, failure, prose};
use crate::{Projects, Record, Tab};

const FACT_MIN_WIDTH: Pixels = px(96.);
const MARKER_SIZE: Pixels = px(7.);
const TRACK_HEIGHT: Pixels = px(1.);

pub(super) fn facts(project: &PlanningProject, tokens: &Tokens) -> Div {
    let status = if project.archived {
        "Archived"
    } else {
        stage(&project.status)
    };
    let health = match project.health.as_deref() {
        Some("onTrack") => "On track",
        Some("atRisk") => "At risk",
        Some("offTrack") => "Off track",
        _ => "No update",
    };
    let priority = match project.priority {
        1 => "Urgent",
        2 => "High",
        3 => "Normal",
        4 => "Low",
        _ => "None",
    };
    h_flex()
        .flex_wrap()
        .gap_x_7()
        .gap_y_3()
        .py_3()
        .border_y_1()
        .border_color(tokens.line)
        .children(
            [
                ("Status", status.to_owned()),
                ("Health", health.to_owned()),
                ("Priority", priority.to_owned()),
                (
                    "Lead",
                    project
                        .lead_name
                        .clone()
                        .unwrap_or_else(|| "Unavailable".into()),
                ),
                ("Start date", date_label(project.start_date.as_deref())),
                ("Target date", date_label(project.target_date.as_deref())),
            ]
            .map(|(label, value)| {
                v_flex()
                    .min_w(FACT_MIN_WIDTH)
                    .gap_1()
                    .child(div().text_sm().text_color(tokens.muted).child(label))
                    .child(div().text_color(tokens.fg_strong).child(value))
            }),
        )
}

fn stage(status: &str) -> &str {
    match status {
        "backlog" => "Backlog",
        "planned" => "Planned",
        "started" => "Started",
        "paused" => "Paused",
        "completed" => "Completed",
        "canceled" => "Cancelled",
        _ => status,
    }
}

pub(super) fn stages(project: &PlanningProject, tokens: &Tokens) -> Div {
    let stages = ["backlog", "planned", "started", "completed"];
    let special = matches!(project.status.as_str(), "paused" | "canceled");
    h_flex().flex_wrap().gap_4().children(
        stages
            .into_iter()
            .chain(special.then_some(project.status.as_str()))
            .map(|status| {
                let active = status == project.status;
                let color = if active { tokens.accent } else { tokens.muted };
                h_flex()
                    .gap_2()
                    .text_sm()
                    .text_color(color)
                    .child(
                        div()
                            .size(MARKER_SIZE)
                            .rounded_full()
                            .border_1()
                            .border_color(color)
                            .bg(if active { color } else { tokens.bg }),
                    )
                    .child(stage(status).to_owned())
            }),
    )
}

pub(super) fn render(record: &Record, tokens: &Tokens, cx: &Context<Projects>) -> AnyElement {
    let description = record
        .project
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty());
    let update = match &record.update {
        Ok(Some(update)) => v_flex()
            .gap_2()
            .child(
                div()
                    .text_sm()
                    .text_color(tokens.muted)
                    .child(timestamp_day(&update.created_at)),
            )
            .children(
                update
                    .body
                    .as_ref()
                    .map(|body| prose("project-update", body)),
            )
            .into_any_element(),
        Ok(None) => div()
            .text_color(tokens.muted)
            .child("No project updates yet.")
            .into_any_element(),
        Err(error) => failure(&error.to_string(), tokens, cx),
    };
    v_flex()
        .gap_5()
        .child(section(
            "Description",
            description.map_or_else(
                || {
                    div()
                        .text_color(tokens.muted)
                        .child("No description.")
                        .into_any_element()
                },
                |text| prose("project-description", text),
            ),
            tokens,
        ))
        .child(section("Timeline", timeline(record, tokens, cx), tokens))
        .child(section("Latest update", update, tokens))
        .into_any_element()
}

fn timeline(record: &Record, tokens: &Tokens, cx: &Context<Projects>) -> AnyElement {
    let rows = match &record.milestones {
        Ok(rows) => rows,
        Err(error) => return failure(&error.to_string(), tokens, cx),
    };
    let project = &record.project;
    let dates = timeline_dates(project, rows);
    let Some(first) = dates.first().copied() else {
        return div()
            .text_color(tokens.muted)
            .child("No dates yet.")
            .into_any_element();
    };
    let last = dates.last().copied().unwrap_or(first);
    let next = rows.iter().find(|row| row.status == MilestoneStatus::Next);
    v_flex()
        .gap_3()
        .child(timeline_axis(&dates, first, last, tokens))
        .children(next.map(|row| {
            div().child(format!(
                "Next · {} · {}",
                row.name,
                date_label(row.target_date.as_deref())
            ))
        }))
        .children((!rows.is_empty()).then(|| {
            h_flex().child(
                action_button(
                    "timeline-see-milestones",
                    Intent::Neutral,
                    Some(IconName::Flag),
                    format!("Milestones · {}", rows.len()),
                )
                .on_click(cx.listener(|this, _, _, cx| {
                    this.tab = Tab::Milestones;
                    cx.notify();
                })),
            )
        }))
        .into_any_element()
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

fn timeline_axis(dates: &[NaiveDate], first: NaiveDate, last: NaiveDate, tokens: &Tokens) -> Div {
    let duration = (last - first).num_days().max(1) as f32;
    v_flex()
        .gap_1()
        .child(
            div()
                .relative()
                .h_4()
                .mr_2()
                .child(
                    div()
                        .absolute()
                        .top(MARKER_SIZE)
                        .w_full()
                        .h(TRACK_HEIGHT)
                        .bg(tokens.line),
                )
                .children(dates.iter().map(|date| {
                    div()
                        .absolute()
                        .top_1()
                        .left(relative((*date - first).num_days() as f32 / duration))
                        .size(MARKER_SIZE)
                        .rounded_full()
                        .bg(tokens.accent)
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

fn section(title: &'static str, body: AnyElement, tokens: &Tokens) -> Div {
    v_flex()
        .gap_2()
        .border_t_1()
        .border_color(tokens.line)
        .pt_3()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(title),
        )
        .child(body)
}

fn timestamp_day(value: &str) -> String {
    date_label(value.split_once('T').map(|(date, _)| date))
}
