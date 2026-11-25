//! The compact timeline preview and the full issue list on a milestone.

use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, FontWeight, IntoElement as _, ParentElement as _, Styled as _, div,
};
use subspace_api_client::model::{IssueListItem, StateSummary};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    feedback,
};
use subspace_framework::lucide::IconName;

use crate::Projects;

const PREVIEW_LIMIT: usize = 5;

pub(super) fn preview(this: &Projects, id: &str, tokens: &Tokens) -> Option<Div> {
    let milestone = this.milestone(id)?;
    let body = match this.milestone_pages.issues.get(id) {
        Some(Ok(rows)) if !rows.is_empty() => v_flex()
            .gap_1()
            .children(rows.iter().take(PREVIEW_LIMIT).map(|row| {
                h_flex()
                    .gap_3()
                    .justify_between()
                    .child(div().min_w_0().truncate().child(issue_label(row)))
                    .child(status(row, tokens))
            }))
            .children((rows.len() > PREVIEW_LIMIT).then(|| {
                div().text_color(tokens.muted).child(format!(
                    "{} more in the milestone",
                    rows.len() - PREVIEW_LIMIT
                ))
            })),
        Some(Ok(_)) => muted("No linked issues", tokens),
        Some(Err(_)) => muted("Linked issues unavailable", tokens),
        None => muted("Loading linked issues…", tokens),
    };
    Some(
        v_flex()
            .w_full()
            .gap_2()
            .p_3()
            .rounded_md()
            .border_1()
            .border_color(tokens.line)
            .text_sm()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(milestone.name.clone()),
            )
            .child(body),
    )
}

pub(super) fn detail(this: &Projects, id: &str, tokens: &Tokens, cx: &Context<Projects>) -> Div {
    let body = match this.milestone_pages.issues.get(id) {
        Some(Ok(rows)) if !rows.is_empty() => v_flex()
            .children(rows.iter().map(|row| issue_row(row, tokens, cx)))
            .into_any_element(),
        Some(Ok(_)) => muted("No linked issues", tokens).into_any_element(),
        Some(Err(error)) => retry(id, error.to_string(), tokens, cx),
        None => {
            feedback::loading_line(tokens, "milestone-issues-loading", "Loading linked issues…")
                .into_any_element()
        }
    };
    v_flex()
        .gap_2()
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .child("Linked issues"),
        )
        .child(body)
}

fn muted(label: &'static str, tokens: &Tokens) -> Div {
    div().text_color(tokens.muted).child(label)
}

fn retry(id: &str, message: String, tokens: &Tokens, cx: &Context<Projects>) -> AnyElement {
    let id = id.to_owned();
    v_flex()
        .gap_2()
        .child(div().text_color(tokens.muted).child(message))
        .child(
            action_button(
                "retry-linked-issues",
                Intent::Neutral,
                Some(IconName::RotateCw),
                "Reload issues",
            )
            .on_click(cx.listener(move |this, _, _, cx| this.retry_milestone_issues(&id, cx))),
        )
        .into_any_element()
}

fn issue_row(row: &IssueListItem, tokens: &Tokens, cx: &Context<Projects>) -> Div {
    let identifier = row.fields.identifier.clone();
    h_flex()
        .py_1()
        .gap_3()
        .min_w_0()
        .border_t_1()
        .border_color(tokens.line)
        .child(
            h_flex().min_w_0().flex_1().justify_start().child(
                action_button(
                    format!("milestone-issue:{identifier}"),
                    Intent::Quiet,
                    Some(IconName::CircleDot),
                    issue_label(row),
                )
                .on_click(
                    cx.listener(move |this, _, _, cx| this.open_linked_issue(&identifier, cx)),
                ),
            ),
        )
        .child(status(row, tokens))
}

fn issue_label(row: &IssueListItem) -> String {
    format!("{} · {}", row.fields.identifier, row.fields.title)
}

fn status(row: &IssueListItem, tokens: &Tokens) -> Div {
    let StateSummary { name, kind, .. } = &row.fields.state;
    let color = match kind.as_str() {
        "completed" => tokens.green,
        "started" => tokens.accent,
        _ => tokens.muted,
    };
    div()
        .flex_shrink_0()
        .text_sm()
        .text_color(color)
        .child(name.clone())
}
