//! Project facts, Markdown, and progress.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, IntoElement as _, ParentElement as _, Pixels,
    Styled as _, div, px,
};
use subspace_api_client::projects::PlanningProject;
use subspace_framework::dashboard::Tokens;

use super::{date_label, failure, prose, timeline};
use crate::{Projects, Record, project_editor::STAGES};

const FACT_MIN_WIDTH: Pixels = px(96.);
const STAGE_HEIGHT: Pixels = px(3.);

pub(super) fn properties(project: &PlanningProject) -> [(&'static str, String); 6] {
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
}

/// Keep the properties in the content when the inspector is hidden.
pub(super) fn facts(project: &PlanningProject, tokens: &Tokens) -> Div {
    h_flex()
        .flex_wrap()
        .gap_x_7()
        .gap_y_3()
        .py_3()
        .border_y_1()
        .border_color(tokens.line)
        .children(properties(project).map(|(label, value)| {
            v_flex()
                .min_w(FACT_MIN_WIDTH)
                .gap_1()
                .child(div().text_sm().text_color(tokens.muted).child(label))
                .child(div().text_color(tokens.fg_strong).child(value))
        }))
}

fn stage(status: &str) -> &str {
    STAGES
        .iter()
        .find(|(key, _)| *key == status)
        .map_or(status, |(_, label)| *label)
}

pub(super) fn stages(project: &PlanningProject, tokens: &Tokens) -> Div {
    let stages = ["backlog", "planned", "started", "completed"];
    let special = matches!(project.status.as_str(), "paused" | "canceled");
    let current = stages.iter().position(|status| *status == project.status);
    h_flex().flex_wrap().gap_2().children(
        stages
            .into_iter()
            .chain(special.then_some(project.status.as_str()))
            .enumerate()
            .map(|(index, status)| {
                let active = status == project.status;
                let reached = current.is_some_and(|current| index <= current);
                v_flex()
                    .flex_1()
                    .min_w(FACT_MIN_WIDTH)
                    .gap_2()
                    .text_sm()
                    .text_color(if active { tokens.accent } else { tokens.muted })
                    .child(
                        div()
                            .w_full()
                            .h(STAGE_HEIGHT)
                            .rounded_full()
                            .bg(if reached || active {
                                tokens.accent
                            } else {
                                tokens.line
                            }),
                    )
                    .child(stage(status).to_owned())
            }),
    )
}

pub(super) fn render(
    this: &Projects,
    record: &Record,
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> AnyElement {
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
            "Timeline",
            timeline::render(this, record, tokens, cx),
            tokens,
        ))
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
        .child(section("Latest update", update, tokens))
        .into_any_element()
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
