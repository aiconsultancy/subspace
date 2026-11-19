//! API-to-contract projection. No display rules or additional reads.
use subspace_api_client::{
    hub::{IrcSnapshot, ReferenceCard},
    model::{LastActivity, Person},
};
use subspace_framework::contracts::issues::{
    IssuePerson, IssueProgress, IssueSummary, LatestActivity, PullRequestSummary,
};

const LOW_PRIORITY: u8 = 4;

pub(super) fn from_card(snapshot: &IrcSnapshot, card: &ReferenceCard) -> IssueSummary {
    let facts = card.facts();
    let assignee = facts.assignee.as_ref();
    super::base_summary(snapshot, card)
        .with_priority(
            u8::try_from(facts.priority)
                .ok()
                .filter(|priority| *priority <= LOW_PRIORITY),
        )
        .with_owner(facts.owner.as_ref().map(person))
        .with_assignee_id(assignee.map(|person| person.user_id.as_str()))
        .with_assignee_kind(assignee.map(|person| person.kind.as_str()))
        .with_assignee_rank(
            assignee
                .and_then(|person| person.agent.as_ref())
                .and_then(|agent| agent.rank.as_deref()),
        )
        .with_progress(
            facts
                .progress
                .as_ref()
                .map(|progress| IssueProgress::new(count(progress.done), count(progress.total))),
        )
        .with_pull_requests(facts.pull_requests.as_ref().map(|prs| {
            PullRequestSummary::new(
                count(prs.total),
                count(prs.open),
                count(prs.closed),
                count(prs.merged),
            )
            .with_checks(prs.checks_state.as_deref())
        }))
        .with_due_date(facts.due_date.as_deref())
        .with_latest_activity(facts.latest_activity.as_ref().map(activity))
}

fn count(value: i32) -> u32 {
    value.max(0) as u32
}
fn person(person: &Person) -> IssuePerson {
    IssuePerson::new(&person.user_id, &person.display_name, &person.kind)
}
fn activity(activity: &LastActivity) -> LatestActivity {
    let kind = serde_json::to_value(activity.kind()).unwrap_or_default();
    let actor = activity.actor();
    let kind = kind
        .as_str()
        .map_or_else(|| "other".to_owned(), str::to_lowercase);
    LatestActivity::new(&kind, activity.occurred_at()).with_actor(
        actor
            .person()
            .map(|person| person.display_name.as_str())
            .or(actor.label()),
    )
}

#[cfg(test)]
mod tests;
