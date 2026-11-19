use crate::{Issues, requests::Request};
use gpui_kit::component::h_flex;
use gpui_kit::{
    Context, Div, InteractiveElement as _, ParentElement as _, StatefulInteractiveElement as _,
    Styled as _, TestSupportExt as _, div,
};
use subspace_api_client::hub::IssueSnapshot;
use subspace_framework::dashboard::Tokens;
use subspace_framework::dashboard::action::{ActionState, Intent, action_button};
use subspace_framework::lucide::IconName;

#[derive(Clone, Copy)]
pub(super) enum Rows {
    Relations,
    PullRequests,
}

pub(super) fn label(loading: bool, failed: bool, remaining: Option<usize>) -> String {
    match (loading, failed) {
        (true, _) => "Loading more…".into(),
        (_, true) => "Retry".into(),
        _ => remaining.map_or_else(|| "Show more".into(), |n| format!("Show {n} more")),
    }
}

pub(super) fn render(
    rows: Rows,
    open: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> Div {
    let (id, kind, loading, failed, request) = match rows {
        Rows::Relations => (
            "issue-more-relations",
            "relations",
            open.relations_loading(),
            open.relations_failed(),
            Request::MoreRelations,
        ),
        Rows::PullRequests => (
            "issue-more-pull-requests",
            "pull requests",
            open.pull_requests_loading(),
            open.pull_requests_failed(),
            Request::MorePullRequests,
        ),
    };
    let button = action_button(
        id,
        Intent::Neutral,
        Some(if failed {
            IconName::RotateCw
        } else {
            IconName::ChevronDown
        }),
        label(loading, failed, None),
    )
    .accessibility_label(format!("{} {kind}", label(loading, failed, None)))
    .on_click(cx.listener(move |this, _, window, cx| {
        this.focus_list(window, cx);
        this.request(request.clone(), cx);
    }));
    h_flex()
        .gap_2()
        .children(
            failed.then(|| failure_notice(id, failure_label(kind, has_rows(rows, open)), tokens)),
        )
        .child(if loading {
            ActionState::Busy.apply(button)
        } else {
            button
        })
}

fn has_rows(rows: Rows, open: &IssueSnapshot) -> bool {
    match rows {
        Rows::Relations => open
            .detail()
            .and_then(|d| d.relations.as_ref())
            .is_some_and(|p| !p.items.is_empty()),
        Rows::PullRequests => open.pull_requests().is_some_and(|p| !p.items.is_empty()),
    }
}

fn failure_notice(id: &str, message: String, tokens: &Tokens) -> impl gpui_kit::IntoElement {
    div()
        .id(format!("{id}-error"))
        .aria_label(message.clone())
        .test_support()
        .text_color(tokens.muted)
        .child(message)
}

fn failure_label(kind: &str, has_rows: bool) -> String {
    let more = if has_rows { "more " } else { "" };
    format!("Couldn't load {more}{kind}")
}

#[cfg(test)]
mod tests {
    use super::{failure_label, label};

    #[test]
    fn failure_distinguishes_first_page_from_continuation() {
        assert_eq!(
            failure_label("pull requests", false),
            "Couldn't load pull requests"
        );
        assert_eq!(
            failure_label("pull requests", true),
            "Couldn't load more pull requests"
        );
    }

    #[test]
    fn cursor_pages_without_totals_do_not_invent_a_remaining_count() {
        assert_eq!(label(false, false, None), "Show more");
        assert_eq!(label(false, false, Some(5)), "Show 5 more");
        assert_eq!(label(true, false, Some(5)), "Loading more…");
        assert_eq!(label(false, true, Some(5)), "Retry");
    }
}
