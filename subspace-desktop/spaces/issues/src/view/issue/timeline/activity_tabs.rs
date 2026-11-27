//! The local Activity tabs use Kit's native selection and keyboard focus.
//! Kit's tab list makes the selected tab its one tab stop and moves focus
//! with the arrow keys; the wrapper's handle lets the board focus that tab.
use crate::{Issues, detail::timeline::TimelineMode};
use gpui_kit::{
    Context, InteractiveElement as _, IntoElement, ParentElement as _, div,
    prelude::FluentBuilder as _,
};
use subspace_api_client::hub::IssueSnapshot;
use subspace_framework::dashboard::{
    Tokens,
    tabs::{self, TabSpec, TabStatus},
};
use subspace_framework::lucide::IconName;

pub(super) fn render(
    issues: &Issues,
    issue: &IssueSnapshot,
    tokens: &Tokens,
    cx: &mut Context<Issues>,
) -> impl IntoElement {
    let selected = issues.issue_view.timeline.mode;
    let focus = issues.issue_view.timeline_focus.as_ref();
    let specs = [
        TabSpec::plain("Comments"),
        TabSpec::plain("All"),
        decisions(issue, tokens),
    ];
    let bar = tabs::tab_bar("issue-timeline-tabs")
        .selected_index(selected.tab_index())
        .children(specs.into_iter().map(tabs::tab))
        .on_click(cx.listener(|this, index: &usize, _, cx| {
            this.show_timeline(TimelineMode::at(*index), cx);
        }));
    div()
        .id("issue-timeline-tab-list")
        .when_some(focus, |wrapper, focus| wrapper.track_focus(&focus.tabs))
        .child(bar)
}

fn decisions(issue: &IssueSnapshot, tokens: &Tokens) -> TabSpec {
    TabSpec::new("Decisions", IconName::Scale).when(issue.decisions_loading(), |spec| {
        spec.status(TabStatus::busy(tokens.muted))
    })
}
