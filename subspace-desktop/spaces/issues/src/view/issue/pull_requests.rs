use super::{
    paging::{self, Rows},
    panel,
};
use crate::Issues;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, Div, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::hub::{IssueSnapshot, IssueStatus};
use subspace_api_client::model::{PullRequest, PullRequestSummary};
use subspace_framework::dashboard::action::{Intent, action_button};
use subspace_framework::dashboard::{Tokens, links};
use subspace_framework::lucide::IconName;

#[cfg(test)]
mod tests;

const UNAVAILABLE: &str = "Pull requests are not available from this server yet";

pub(super) fn render(
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let title = heading(
        open.detail()
            .and_then(|detail| detail.fields.pull_request_summary.as_ref()),
    );
    let filled = open
        .pull_requests()
        .is_some_and(|page| !page.items.is_empty());
    div()
        .id("issue-pull-requests")
        .aria_label(title.clone())
        .test_support()
        .child(
            panel::section(title, IconName::GitPullRequest, filled, tokens)
                .child(content(open, tokens, cx)),
        )
        .into_any_element()
}

fn content(open: &IssueSnapshot, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let page = open.pull_requests();
    let empty = page.is_none_or(|p| p.items.is_empty());
    v_flex()
        .gap_1()
        .when(open.pull_requests_loading() && empty, |col| {
            col.child(panel::empty("Loading pull requests…", tokens))
        })
        .when(open.pull_requests_unavailable(), |col| {
            col.child(
                div()
                    .id("issue-pull-requests-unavailable")
                    .aria_label(UNAVAILABLE)
                    .test_support()
                    .child(panel::empty(UNAVAILABLE, tokens)),
            )
        })
        .when(
            empty
                && !open.pull_requests_loading()
                && !open.pull_requests_unavailable()
                && !open.pull_requests_failed(),
            |col| col.child(panel::empty("No pull requests", tokens)),
        )
        .children(
            page.into_iter()
                .flat_map(|p| &p.items)
                .map(|pr| row(pr, tokens, cx)),
        )
        .when(
            *open.status() == IssueStatus::Loaded
                && !open.pull_requests_unavailable()
                && (open.pull_requests_failed() || page.is_some_and(|p| p.next_cursor.is_some())),
            |col| col.child(paging::render(Rows::PullRequests, open, tokens, cx)),
        )
}

fn row(pr: &PullRequest, tokens: &Tokens, cx: &mut Context<Issues>) -> Div {
    let status = state_label(if pr.is_draft { "draft" } else { &pr.state });
    let reference = format!("{}#{}", pr.repository.full_name, pr.number);
    let title = format!("{reference} · {}", pr.title);
    let url = pr.url.clone();
    let button = action_button(
        format!("issue-pr:{}", pr.id),
        Intent::Quiet,
        Some(IconName::GitPullRequest),
        title,
    )
    .tooltip("Open in PRs")
    .accessibility_label(format!("Open {reference} in PRs: {}, {status}", pr.title))
    .on_click(cx.listener(move |_, _, _, cx| {
        crate::navigation::open_pull_request(&url, cx);
    }));
    v_flex()
        .gap_1()
        .border_b_1()
        .border_color(tokens.hair)
        .py_1()
        .child(
            h_flex()
                .min_w_0()
                .flex_wrap()
                .gap_2()
                .child(button.max_w_full())
                .child(
                    div()
                        .id(format!("issue-pr-browser:{}", pr.id))
                        .test_support()
                        .flex()
                        .text_size(tokens.secondary_size())
                        .child(links::external_marked(
                            format!("issue-pr-link:{}", pr.id),
                            "Open on GitHub",
                            pr.url.clone(),
                            tokens,
                        )),
                ),
        )
        .child(
            div()
                .id(format!("issue-pr-status:{}", pr.id))
                .aria_label(status.to_owned())
                .test_support()
                .text_color(tokens.muted)
                .text_size(tokens.secondary_size())
                .child(pr_facts(pr)),
        )
}

fn pr_facts(pr: &PullRequest) -> String {
    let status = state_label(if pr.is_draft { "draft" } else { &pr.state });
    let mut facts = format!(
        "{status} · {} · {}",
        pr.base_branch,
        if pr.reference_active {
            "Current association"
        } else {
            "Historical association"
        }
    );
    if pr.reference_kind == "closes" {
        facts.push_str(" · Closes this issue");
    }
    let disposition = match pr.completion.as_str() {
        "completed" => Some("Issue completed"),
        "skippedTracker" => Some("Tracker status is calculated from sub-issues"),
        "skippedArchived" => Some("Archived issue kept unchanged"),
        "skippedTerminal" => Some("Issue already in a terminal state"),
        _ => None,
    };
    if let Some(disposition) = disposition {
        facts.push_str(&format!(" · {disposition}"));
    }
    facts
}

fn state_label(state: &str) -> String {
    let mut label = String::with_capacity(state.len());
    let mut start = true;
    for character in state.chars() {
        let character = if matches!(character, '_' | '-') {
            ' '
        } else {
            character
        };
        if start {
            label.extend(character.to_uppercase());
        } else {
            label.extend(character.to_lowercase());
        }
        start = character.is_whitespace();
    }
    label
}

fn heading(summary: Option<&PullRequestSummary>) -> String {
    let mut title = "Pull requests".to_owned();
    if let Some(summary) = summary {
        for (count, state) in [
            (summary.open, "open"),
            (summary.merged, "merged"),
            (summary.closed, "closed"),
        ] {
            if count > 0 {
                title.push_str(&format!(" · {count} {state}"));
            }
        }
    }
    title
}
