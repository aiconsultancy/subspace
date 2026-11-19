//! The issue-associated decision list, in the API's newest-logged order.
use super::super::paging::{self, Rows};
use crate::{Issues, decision::words, requests::Request};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::{
    hub::{IssueSnapshot, IssueStatus, ReadError},
    model::DecisionListItem,
};
use subspace_framework::dashboard::{
    Tokens,
    action::{Intent, action_button},
    feedback, truncated,
};
use subspace_framework::lucide::IconName;

pub(super) fn render(issue: &IssueSnapshot, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let list = v_flex().min_w_0();
    let offline = *issue.status() == IssueStatus::Error(ReadError::Network);
    if issue.decisions_unavailable() && !offline && !issue.decisions_loading() {
        return list.child(words::notice(
            tokens,
            "issue-decisions-unavailable",
            "You don’t have access to decisions for this issue.",
        ));
    }
    let (list, has_rows) = cached_rows(list, issue, tokens, cx);
    if offline {
        return list.child(
            v_flex()
                .gap_2()
                .child(words::offline(tokens, "issue-decisions-offline"))
                .child(retry("issue-decisions-offline-retry", cx)),
        );
    }
    if has_rows {
        return list;
    }
    list.child(empty(issue, tokens, cx))
}

fn cached_rows(
    list: Div,
    issue: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> (Div, bool) {
    let Some(page) = issue.decisions().filter(|page| {
        !page.items.is_empty() || page.next_cursor.is_some() || issue.decisions_refresh_failed()
    }) else {
        return (list, false);
    };
    let online = *issue.status() != IssueStatus::Error(ReadError::Network);
    let show_pager = page.next_cursor.is_some()
        || (online && (issue.decisions_loading() || issue.decisions_failed().is_some()));
    let list = list.children(page.items.iter().map(|item| row(item, tokens, cx)));
    (
        list.children(show_pager.then(|| paging::render(Rows::Decisions, issue, tokens, cx))),
        true,
    )
}

fn empty(issue: &IssueSnapshot, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    if let Some(error) = issue.decisions_failed() {
        return v_flex()
            .gap_2()
            .child(words::notice(
                tokens,
                "issue-decisions-error",
                "Couldn’t load decisions",
            ))
            .child(words::notice(
                tokens,
                "issue-decisions-error-hint",
                match error {
                    ReadError::Network => "Check your connection and retry.",
                    ReadError::SignInNeeded => crate::scope::SIGN_IN_MESSAGE,
                    _ => crate::scope::READ_ERROR_MESSAGE,
                },
            ))
            .child(retry("issue-decisions-retry", cx));
    }
    if issue.decisions_loading() || issue.decisions().is_none() {
        return div().child(
            div()
                .id("issue-decisions-loading")
                .aria_label("Loading decisions")
                .test_support()
                .child(feedback::skeleton(tokens)),
        );
    }
    words::notice(
        tokens,
        "issue-decisions-empty",
        "No decisions linked to this issue",
    )
}

fn retry(id: &'static str, cx: &mut Context<Issues>) -> Div {
    h_flex().child(words::retry(
        id,
        "Retry decisions",
        |this, cx| this.request(Request::RetryDecisions, cx),
        cx,
    ))
}

fn row(
    item: &DecisionListItem,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> impl gpui_kit::IntoElement + use<> {
    let id = item.id.clone();
    let title = item.title.clone();
    let label = format!(
        "Open decision {title}, {}, {}, project {}",
        words::status(item.status),
        words::date(item, true),
        item.project_key
    );
    v_flex()
        .id(format!("issue-decision-row:{id}"))
        .aria_description("Decisions are ordered by when they were logged, not by decided-on date.")
        .test_support()
        .min_w_0()
        .border_b_1()
        .border_color(tokens.line)
        .py_2()
        .child(
            h_flex()
                .flex_wrap()
                .min_w_0()
                .gap_2()
                .child(
                    action_button(
                        format!("issue-decision:{id}"),
                        Intent::Quiet,
                        Some(IconName::Scale),
                        "",
                    )
                    .max_w_full()
                    .min_w_0()
                    .child(truncated::text(
                        format!("issue-decision-title:{id}"),
                        title.clone(),
                    ))
                    .focus_on_click()
                    .accessibility_label(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        crate::decision::open(this, &id, &title, window, cx)
                    })),
                )
                .child(words::status_badge(item.status, tokens)),
        )
        .child(feedback::empty_line(
            tokens,
            format!("issue-decision-byline:{}", item.id),
            byline(item),
        ))
}

fn byline(item: &DecisionListItem) -> String {
    let suffix = match item.superseded_by.as_ref() {
        Some(next) => format!("Superseded by “{}”", next.title),
        None => words::maker_short(item),
    };
    format!(
        "{} · {} · {suffix}",
        item.project_key,
        words::date(item, false)
    )
}
