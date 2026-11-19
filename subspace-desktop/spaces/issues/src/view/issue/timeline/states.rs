use crate::detail::timeline::TimelineMode;
use gpui_kit::{Div, ParentElement as _, Styled as _, div};
use subspace_api_client::hub::{IssueStatus, ReadError, TimelineSnapshot};
use subspace_framework::dashboard::{Tokens, feedback};

pub(super) fn empty(
    snapshot: &TimelineSnapshot,
    mode: TimelineMode,
    status: &IssueStatus,
    tokens: &Tokens,
) -> Div {
    if snapshot.loading() || (snapshot.idle() && *status == IssueStatus::Loaded) {
        return div().child(feedback::skeleton(tokens));
    }
    if let Some(error) = snapshot.error() {
        return div()
            .text_color(tokens.muted)
            .child(timeline_failure(error));
    }
    let complete = snapshot
        .page()
        .is_some_and(|page| page.next_cursor.is_none());
    let copy = match (mode, complete) {
        (TimelineMode::Comments, true) => "No comments yet",
        (TimelineMode::Comments, false) => "No comments in the loaded activity",
        (TimelineMode::All | TimelineMode::Decisions, _) => "No activity yet",
    };
    div().text_color(tokens.muted).child(copy)
}
fn timeline_failure(error: &ReadError) -> &'static str {
    match error {
        ReadError::Network => crate::scope::NETWORK_MESSAGE,
        ReadError::SignInNeeded => crate::scope::SIGN_IN_MESSAGE,
        _ => "Couldn’t load this timeline",
    }
}
