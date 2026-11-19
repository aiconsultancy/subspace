//! Display decisions shared by rendering and accessible names.
use subspace_framework::contracts::issues::{IssueSummary, PullRequestSummary};
use subspace_framework::dashboard::Tone;

pub(super) fn priority(value: Option<u8>) -> Option<&'static str> {
    match value {
        Some(1) => Some("Urgent"),
        Some(2) => Some("High"),
        _ => None,
    }
}
pub(super) fn is_agent(summary: &IssueSummary) -> bool {
    summary.assignee_kind() == Some("agent")
}
pub(super) fn owner(summary: &IssueSummary) -> Option<&str> {
    summary
        .owner()
        .filter(|owner| match summary.assignee_id() {
            Some(id) => owner.id != id,
            None => Some(owner.name.as_str()) != summary.assignee.as_deref(),
        })
        .map(|owner| owner.name.as_str())
}
pub(super) fn has_footer(summary: &IssueSummary) -> bool {
    summary.assignee.is_some()
        || summary.owner().is_some()
        || summary
            .latest_activity()
            .is_some_and(|last| chrono::DateTime::parse_from_rfc3339(&last.occurred_at).is_ok())
}
pub(super) fn open_prs(prs: &PullRequestSummary) -> Option<(String, Tone)> {
    if prs.open == 0 {
        return None;
    }
    let (checks, tone) = match prs.checks.as_deref() {
        Some("success" | "passed" | "passing") => (" · checks pass", Tone::Green),
        Some("failure" | "failed" | "failing" | "error") => (" · checks failing", Tone::Red),
        _ => ("", Tone::Fg),
    };
    Some((format!("{} open{checks}", prs.open), tone))
}

/// The assignee's rank as words: the API's camelCase `rearAdmiral` reads as
/// "rear admiral".
pub(super) fn rank(summary: &IssueSummary) -> Option<String> {
    summary.assignee_rank().map(words)
}

fn words(raw: &str) -> String {
    let mut out = String::new();
    for char in raw.chars() {
        if char.is_uppercase() && !out.is_empty() {
            out.push(' ');
        }
        out.extend(char.to_lowercase());
    }
    out
}
