//! Milestone statuses, issue progress and row actions.

use gpui_kit::component::progress::Progress;
use gpui_kit::component::{Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, IntoElement, ParentElement as _, Styled as _, div,
};
use subspace_api_client::projects::{
    CompletionSource, IssueProgress, Milestone, MilestoneChange, MilestoneStatus, Move,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button, icon_button},
    feedback,
};
use subspace_framework::lucide::IconName;

use super::{date_label, failure, linked_issues, prose};
use crate::{Projects, Record, Write};

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
    let done = rows
        .iter()
        .filter(|row| row.status == MilestoneStatus::Done)
        .count();
    v_flex()
        .gap_2()
        .child(
            div()
                .text_sm()
                .text_color(tokens.muted)
                .child(format!("{done} of {} done", rows.len())),
        )
        .children(
            this.error
                .as_ref()
                .filter(|_| this.editor.is_none())
                .map(|error| failure(error, tokens, cx)),
        )
        .children(
            this.write_task
                .is_some()
                .then(|| feedback::loading_line(tokens, "project-writing", "Saving milestone…")),
        )
        .children(
            rows.is_empty()
                .then(|| div().text_color(tokens.muted).child("No milestones yet.")),
        )
        .children(rows.iter().enumerate().map(|(index, row)| {
            let actions = this
                .may_write()
                .then(|| milestone_actions(row, index, rows.len(), cx));
            milestone_row(row, actions, tokens, cx)
        }))
        .into_any_element()
}

fn milestone_row(
    row: &Milestone,
    actions: Option<Div>,
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> Div {
    let id = row.id.clone();
    v_flex()
        .gap_2()
        .py_3()
        .border_t_1()
        .border_color(tokens.line)
        .child(
            h_flex()
                .flex_wrap()
                .gap_3()
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .gap_1()
                        .child(
                            action_button(
                                format!("milestone-open:{id}"),
                                Intent::Quiet,
                                Some(IconName::Flag),
                                row.name.clone(),
                            )
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.open_milestone(&id, cx);
                                },
                            )),
                        )
                        .child(status_line(row, tokens)),
                )
                .children(actions),
        )
        .children(
            row.description
                .as_deref()
                .filter(|text| !text.is_empty())
                .map(|text| prose(&format!("milestone-description:{}", row.id), text)),
        )
        .children(
            row.issue_progress
                .as_ref()
                .map(|progress| issue_progress(&row.id, progress, tokens)),
        )
}

pub(super) fn detail(
    this: &Projects,
    id: &str,
    project: &str,
    tokens: &Tokens,
    cx: &Context<Projects>,
) -> AnyElement {
    let Some(row) = this.milestone(id) else {
        return div()
            .text_color(tokens.muted)
            .child(if this.record_task.is_some() {
                "Loading milestone…"
            } else {
                "Milestone unavailable."
            })
            .into_any_element();
    };
    v_flex()
        .gap_4()
        .child(
            div()
                .text_color(tokens.muted)
                .text_sm()
                .child(project.to_owned()),
        )
        .child(
            div()
                .text_xl()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(tokens.fg_strong)
                .child(row.name.clone()),
        )
        .child(status_line(row, tokens))
        .children(
            row.description
                .as_deref()
                .map(|text| prose("milestone-detail-description", text)),
        )
        .children(
            row.issue_progress
                .as_ref()
                .map(|progress| issue_progress(id, progress, tokens)),
        )
        .child(linked_issues::detail(this, id, tokens, cx))
        .into_any_element()
}

pub(super) fn status_line(row: &Milestone, tokens: &Tokens) -> Div {
    let (status, color) = match row.status {
        MilestoneStatus::Unstarted => ("Not started", tokens.muted),
        MilestoneStatus::Next => ("Next", tokens.accent),
        MilestoneStatus::Overdue => ("Overdue", tokens.orange),
        MilestoneStatus::Done => ("Done", tokens.green),
    };
    let from_issues = matches!(
        row.completion_source,
        Some(CompletionSource::Issues | CompletionSource::ManualAndIssues)
    );
    h_flex()
        .flex_wrap()
        .gap_3()
        .text_sm()
        .child(div().text_color(color).child(status))
        .child(
            div()
                .text_color(tokens.muted)
                .child(date_label(row.target_date.as_deref())),
        )
        .children(from_issues.then(|| {
            div()
                .text_color(tokens.muted)
                .child("Completed from issues")
        }))
}

fn issue_progress(id: &str, progress: &IssueProgress, tokens: &Tokens) -> Div {
    let (percent, summary, excluded) = progress_labels(progress);
    h_flex()
        .flex_wrap()
        .gap_3()
        .text_sm()
        .text_color(tokens.muted)
        .child(
            div()
                .w_20()
                .child(Progress::new(format!("milestone-progress:{id}")).value(percent)),
        )
        .child(summary)
        .children(excluded)
}

fn progress_labels(progress: &IssueProgress) -> (f32, String, Option<String>) {
    const FULL_PERCENT: f32 = 100.0;
    let percent = FULL_PERCENT * progress.done as f32 / progress.total.max(1) as f32;
    let summary = format!("{} of {} issues done", progress.done, progress.total);
    let excluded = progress.canceled + progress.duplicate;
    let note = (excluded > 0).then(|| format!("{excluded} cancelled or duplicate"));
    (percent, summary, note)
}

fn milestone_actions(row: &Milestone, index: usize, count: usize, cx: &Context<Projects>) -> Div {
    let edit = row.clone();
    let delete = row.clone();
    let complete = row.clone();
    let reopen = row.status == MilestoneStatus::Done;
    h_flex()
        .flex_wrap()
        .gap_1()
        .children(
            (!reopen || row.completion_source == Some(CompletionSource::Manual)).then(|| {
                action_button(
                    format!("complete:{}", row.id),
                    Intent::Neutral,
                    Some(IconName::Check),
                    if reopen { "Reopen" } else { "Mark as done" },
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.send(
                        Write::Change(complete.clone(), MilestoneChange::Complete(!reopen)),
                        window,
                        cx,
                    );
                }))
            }),
        )
        .child(
            icon_button(
                format!("edit:{}", row.id),
                Intent::Quiet,
                IconName::Pencil,
                "Edit milestone",
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_editor(Some(edit.clone()), window, cx)
            })),
        )
        .child(move_button(row, Move::Up, index == 0, cx))
        .child(move_button(row, Move::Down, index + 1 == count, cx))
        .child(
            icon_button(
                format!("delete:{}", row.id),
                Intent::Quiet,
                IconName::Trash,
                "Delete milestone",
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.confirm_remove(delete.clone(), window, cx)
            })),
        )
}

fn move_button(
    row: &Milestone,
    direction: Move,
    disabled: bool,
    cx: &Context<Projects>,
) -> impl IntoElement {
    let (icon, label) = match direction {
        Move::Up => (IconName::ArrowUp, "Move milestone up"),
        Move::Down => (IconName::ArrowDown, "Move milestone down"),
    };
    let row = row.clone();
    icon_button(format!("{label}:{}", row.id), Intent::Quiet, icon, label)
        .disabled(disabled)
        .on_click(cx.listener(move |this, _, window, cx| {
            this.send(
                Write::Change(row.clone(), MilestoneChange::Move(direction)),
                window,
                cx,
            );
        }))
}
