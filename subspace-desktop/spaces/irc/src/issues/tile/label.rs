//! Accessible names reflect the supplied facts, including checks and activity.
use super::facts::{open_prs, owner, priority, rank};
use subspace_framework::contracts::issues::IssueSummary;
pub(in crate::issues) fn label(
    summary: &IssueSummary,
    now: chrono::DateTime<chrono::Utc>,
) -> String {
    let mut facts = vec![summary.state.name.clone()];
    if summary.ready_for_agents() == Some(true) {
        facts.push("ready for agents".into());
    }
    if summary.tracker {
        facts.push("Tracker".into());
    }
    if summary.archived {
        facts.push("Archived".into());
    }
    if let Some(priority) = priority(summary.priority()) {
        facts.push(format!("{} priority", priority.to_lowercase()));
    }
    if let Some(name) = &summary.assignee {
        let rank = rank(summary)
            .map(|rank| format!("{rank} "))
            .unwrap_or_default();
        facts.push(format!("assigned to {rank}{name}"));
    }
    if let Some(owner) = owner(summary) {
        facts.push(format!("for {owner}"));
    }
    facts.extend(tracker(summary));
    facts.extend(dates(summary, now));
    format!(
        "Open {} in Issues: {}, {}",
        summary.identifier,
        summary.title,
        facts.join(", ")
    )
}

fn dates(summary: &IssueSummary, now: chrono::DateTime<chrono::Utc>) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(due) = summary.due_date().filter(|value| {
        chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
            || chrono::DateTime::parse_from_rfc3339(value).is_ok()
    }) {
        facts.push(format!("due {due}"));
    }
    if let Some((last, when)) = summary
        .latest_activity()
        .and_then(|last| super::activity::relative(&last.occurred_at, now).map(|when| (last, when)))
    {
        facts.push(format!("latest activity {}", when.label));
        if let Some(actor) = &last.actor {
            facts.push(format!("by {actor}"));
        }
    }
    facts
}

fn tracker(summary: &IssueSummary) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(progress) = summary.progress().filter(|progress| progress.total > 0) {
        facts.push(format!("{} of {} done", progress.done, progress.total));
    }
    if let Some(prs) = summary.pull_requests() {
        if let Some((open, _)) = open_prs(prs) {
            facts.push(open.replace(" open", " open pull requests"));
        }
        if prs.merged > 0 {
            facts.push(format!("{} merged", prs.merged));
        }
    }
    facts
}
