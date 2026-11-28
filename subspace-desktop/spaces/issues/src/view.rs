//! The Space in the shared workspace layout: the navigator lists the views,
//! projects and initiatives; the pane's tab strip names the open view and
//! its toolbar holds search, filters and actions over the grouped list (or
//! a state in its place); the inspector peeks at the selected issue. The
//! connection, account, count and read time are the status bar's.

pub(crate) mod issue;
mod peek;
mod render;
mod shell;
pub(crate) use issue::section_heading;
pub(crate) mod lanes;
pub(crate) mod progress;
pub(crate) mod reveal_focus;
mod states;

use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::{Tokens, feedback, list};

use crate::Issues;
use crate::model::{SubIssues, Tab};
use crate::scope::Body;

/// How many sub-issues the list hides, while it hides some: the Sub-issues
/// switch and the status bar say so.
pub(crate) fn hidden_subissues(issues: &Issues) -> Option<u32> {
    let hiding = issues.sub_issues == SubIssues::Hidden && issues.listing.content.is_some();
    issues
        .listing
        .hidden_subissues_count
        .filter(|count| hiding && *count > 0)
}

/// The Sub-issues switch's accessible name and tooltip. While sub-issues
/// are hidden, both say how many the list hides.
pub(crate) fn sub_issues_words(mode: SubIssues, hidden: Option<u32>) -> (String, String) {
    match (mode, hidden) {
        (SubIssues::Flat, _) => ("Sub-issues".into(), "Hide sub-issues".into()),
        (SubIssues::Hidden, Some(count)) => (
            format!("Sub-issues, {count} hidden"),
            format!("Show sub-issues ({count} hidden)"),
        ),
        (SubIssues::Hidden, None) => ("Sub-issues".into(), "Show sub-issues".into()),
    }
}

fn body_element(
    issues: &Issues,
    body: &Body,
    tokens: &Tokens,
    window: &mut Window,
    cx: &mut Context<Issues>,
) -> AnyElement {
    let frame = div().flex_1().min_h_0().w_full();
    match body {
        Body::List => frame
            .child(list::render(issues, window, cx))
            .into_any_element(),
        Body::Loading { .. } => frame
            .child(
                div()
                    .id("issues-loading")
                    .aria_label("Loading issues")
                    .test_support()
                    .child(feedback::skeleton(tokens))
                    .child(format!(
                        "The first load can take up to a minute while {} wakes.",
                        subspace_api_client::service_name()
                    )),
            )
            .into_any_element(),
        Body::Screen { screen, .. } => frame
            .child(states::render(issues, screen, tokens, cx))
            .into_any_element(),
    }
}

/// Subspace API's empty wording for the current filters, search or tab.
pub(crate) fn empty_message(tab: Tab, query: &str, filtered: bool) -> (SharedString, SharedString) {
    if !query.is_empty() {
        return (
            "No issues match your search".into(),
            "Try other words, or clear the search.".into(),
        );
    }
    if filtered {
        return (
            "No issues match your filters".into(),
            "Try changing your filters.".into(),
        );
    }
    (
        tab.spec().empty.into(),
        "Try another project, or reload.".into(),
    )
}
