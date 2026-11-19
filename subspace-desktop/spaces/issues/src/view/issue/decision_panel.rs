//! Shared decision detail: structure stays the same for either opener.
mod sections;

use crate::Issues;
use crate::decision::{self, DecisionPanel, words};
use gpui_kit::base::FocusTrapElement as _;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement as _, KeyContext,
    ParentElement as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    TestSupportExt as _, div, px,
};
use subspace_api_client::{
    hub::{DecisionReadStatus, IssueSnapshot, IssueStatus, ReadError},
    model::DecisionDetail,
};
use subspace_framework::dashboard::{
    MODAL_CONTEXT, Tokens,
    action::{Intent, action_button, icon_button},
    feedback,
    panel::clamp_width,
    rich::rich_text_prepared,
    truncated,
};
use subspace_framework::lucide::IconName;

pub(super) fn render(
    issues: &Issues,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Option<AnyElement> {
    let panel = issues.decision.as_ref()?;
    let entry = panel.stack.last()?;
    let mut keys = KeyContext::new_with_defaults(cx.platform_info().os);
    keys.add(MODAL_CONTEXT);
    keys.add(decision::CONTEXT);
    Some(
        v_flex()
            .id("decision-panel")
            .role(Role::Dialog)
            .aria_label(format!("Decision {}", entry.title))
            .test_support()
            .key_context(keys)
            .flex_shrink_0()
            .h_full()
            .min_h_0()
            .w(px(clamp_width(f32::from(tokens.panel_width), issues.width)))
            .max_w_full()
            .bg(tokens.surface)
            .border_l_1()
            .border_color(tokens.line)
            .child(header(panel, tokens, cx))
            .child(
                div()
                    .id("decision-panel-body")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&panel.scroll)
                    .px(tokens.pad_x)
                    .py_4()
                    .child(body(panel, issues.issue_view.snapshot.as_ref(), tokens, cx)),
            )
            .focus_trap("decision-panel", &panel.trap)
            .into_any_element(),
    )
}

fn header(panel: &DecisionPanel, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let back = panel.stack.iter().rev().nth(1).map(|entry| {
        icon_button(
            "decision-panel-back",
            Intent::Quiet,
            IconName::ArrowLeft,
            format!("Back to {}", entry.title),
        )
        .on_click(cx.listener(|this, _, window, cx| decision::back(this, window, cx)))
    });
    h_flex()
        .flex_shrink_0()
        .min_h(tokens.toolbar_height)
        .gap_2()
        .px(tokens.pad_x)
        .py_1()
        .border_b_1()
        .border_color(tokens.line)
        .child(
            truncated::text("decision-panel-heading", "Decision")
                .flex_1()
                .text_size(tokens.heading_font),
        )
        .children(back)
        .child(
            icon_button(
                "decision-panel-close",
                Intent::Quiet,
                IconName::X,
                "Close decision",
            )
            .track_focus(&panel.close)
            .on_click(cx.listener(|this, _, window, cx| {
                decision::close(this, window, cx);
            })),
        )
}

fn body(
    panel: &DecisionPanel,
    issue: Option<&IssueSnapshot>,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let status = issue
        .zip(panel.stack.last())
        .and_then(|(issue, entry)| issue.decision(&entry.id));
    if issue.is_some_and(|issue| *issue.status() == IssueStatus::Error(ReadError::Network))
        && !matches!(
            status,
            Some(DecisionReadStatus::Loaded(_)) | Some(DecisionReadStatus::Gone)
        )
    {
        return retry_state(words::offline(tokens, "decision-panel-offline"), cx);
    }
    match status {
        Some(DecisionReadStatus::Loaded(detail)) => content(panel, detail, tokens, cx),
        Some(DecisionReadStatus::Gone) => words::notice(
            tokens,
            "decision-panel-gone",
            "This decision is no longer available. It may have been removed or you may no longer have access.",
        ),
        Some(DecisionReadStatus::Error(error)) => retry_state(
            words::notice(
                tokens,
                "decision-panel-error",
                words::failure(error, "Couldn’t load decision"),
            ),
            cx,
        ),
        Some(DecisionReadStatus::Loading) | None => v_flex()
            .gap_2()
            .child(words::notice(
                tokens,
                "decision-panel-loading",
                "Loading decision…",
            ))
            .child(feedback::skeleton(tokens)),
    }
}

fn retry_state(notice: Div, cx: &mut Context<Issues>) -> Div {
    v_flex().gap_2().child(notice).child(
        h_flex().child(
            action_button(
                "decision-panel-retry",
                Intent::Neutral,
                Some(IconName::RotateCw),
                "Retry",
            )
            .accessibility_label("Retry decision")
            .on_click(cx.listener(|this, _, window, cx| decision::retry(this, window, cx))),
        ),
    )
}

fn content(
    panel: &DecisionPanel,
    detail: &DecisionDetail,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    v_flex()
        .min_w_0()
        .gap_4()
        .child(sections::summary(detail, tokens))
        .children(panel.markdown.as_ref().map(|fields| prose(fields, tokens)))
        .child(sections::provenance(detail, tokens))
        .children(sections::related(detail, tokens, cx))
}

const HEADINGS: [&str; 5] = ["Decision", "Context", "Why", "Implications", "Alternatives"];

fn prose(fields: &[SharedString; 5], tokens: &Tokens) -> Div {
    sections::section(tokens)
        .gap_3()
        .children(HEADINGS.into_iter().zip(fields).map(|(label, source)| {
            let id = format!("decision-prose:{label}");
            let text = if source.trim().is_empty() {
                feedback::empty_line(tokens, id, "Not recorded").into_any_element()
            } else {
                rich_text_prepared(id, source.clone(), |url, _, _| {
                    subspace_framework::dashboard::links::safe_url(url).is_none()
                })
                .into_any_element()
            };
            v_flex()
                .gap_1()
                .min_w_0()
                .child(sections::heading(label, tokens))
                .child(text)
        }))
}
