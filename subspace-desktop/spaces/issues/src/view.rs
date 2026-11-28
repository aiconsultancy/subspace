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
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window, div,
};
use subspace_framework::dashboard::{Row, Tokens, feedback, list};

use crate::Issues;
use crate::lines;
use crate::model::{IssuePriority, SubIssues, Tab};
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
                    .role(Role::Status)
                    .aria_label("Loading issues")
                    .test_support()
                    .child(feedback::skeleton(tokens)),
            )
            .into_any_element(),
        Body::Screen { screen, .. } => frame
            .child(states::render(issues, screen, tokens, cx))
            .into_any_element(),
    }
}

/// Subspace API's empty wording for the current filters, search or tab.
pub(crate) fn empty_message(tab: Tab, query: &str, filtered: bool) -> (SharedString, SharedString) {
    if filtered {
        return (
            "No issues match your filters".into(),
            "Try changing your filters.".into(),
        );
    }
    if !query.is_empty() {
        return (
            "No issues match your search".into(),
            "Try other words, or clear the search.".into(),
        );
    }
    (
        tab.spec().empty.into(),
        "Try another project, or reload.".into(),
    )
}

/// A row's accessible name: identifier, state, priority and title, so the
/// icon-only lanes are never the only way to read them.
pub(crate) fn row_label(issues: &Issues, row: &Row) -> SharedString {
    let Some(item) = issues.issue(&row.key) else {
        return row.title.clone();
    };
    let fields = &item.fields;
    let mut label = format!(
        "{}, {}, {}, {}",
        fields.identifier,
        fields.state.name,
        IssuePriority::from_value(fields.priority).look().label,
        fields.title
    );
    let parent = fields.parent_id.as_deref().filter(|_| row.indent > 0);
    if let Some(parent) = parent.and_then(|id| issues.issue(&lines::issue_key(id))) {
        label.push_str(", sub-issue of ");
        label.push_str(&parent.fields.identifier);
    }
    label.into()
}
