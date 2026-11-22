use subspace_contracts::issues::{
    IssuePerson, IssueProgress, IssueState, IssueSummary, LatestActivity, PullRequestSummary,
    StateType,
};
fn summary() -> IssueSummary {
    IssueSummary::new(
        "SUB-28",
        "Fixture issue",
        "https://example.test/SUB-28",
        IssueState::new("In progress", StateType::Started),
    )
}
#[test]
fn older_callers_keep_optional_facts_absent() {
    let issue = summary().with_assignee(Some("Rosa Fixture"));
    assert_eq!(issue.assignee.as_deref(), Some("Rosa Fixture"));
    assert!(
        issue.priority().is_none()
            && issue.owner().is_none()
            && issue.assignee_kind().is_none()
            && issue.assignee_rank().is_none()
    );
    assert!(
        issue.progress().is_none()
            && issue.pull_requests().is_none()
            && issue.due_date().is_none()
            && issue.latest_activity().is_none()
            && issue.ready_for_agents().is_none()
    );
}
#[test]
fn all_additive_builders_preserve_their_facts() {
    let issue = summary()
        .with_priority(Some(2))
        .with_owner(Some(IssuePerson::new("human", "Rosa Fixture", "human")))
        .with_assignee(Some("protostar"))
        .with_assignee_id(Some("agent"))
        .with_assignee_kind(Some("agent"))
        .with_assignee_rank(Some("captain"))
        .with_progress(Some(IssueProgress::new(3, 7)))
        .with_pull_requests(Some(
            PullRequestSummary::new(5, 1, 0, 4).with_checks(Some("success")),
        ))
        .with_due_date(Some("2026-11-20"))
        .with_latest_activity(Some(
            LatestActivity::new("comment.created", "2026-11-16T12:00:00Z")
                .with_actor(Some("Rosa Fixture")),
        ))
        .with_ready_for_agents(Some(true));
    assert_eq!(issue.priority(), Some(2));
    assert_eq!(issue.ready_for_agents(), Some(true));
    assert_eq!(issue.owner().unwrap().id, "human");
    assert_eq!(issue.assignee_id(), Some("agent"));
    assert_eq!(issue.assignee_kind(), Some("agent"));
    assert_eq!(issue.assignee_rank(), Some("captain"));
    let progress = issue.progress().unwrap();
    assert_eq!((progress.done, progress.total), (3, 7));
    let prs = issue.pull_requests().unwrap();
    assert_eq!((prs.total, prs.open, prs.closed, prs.merged), (5, 1, 0, 4));
    assert_eq!(prs.checks.as_deref(), Some("success"));
    assert_eq!(issue.due_date(), Some("2026-11-20"));
    let activity = issue.latest_activity().unwrap();
    assert_eq!(
        (
            activity.kind.as_str(),
            activity.occurred_at.as_str(),
            activity.actor.as_deref()
        ),
        (
            "comment.created",
            "2026-11-16T12:00:00Z",
            Some("Rosa Fixture")
        )
    );
}
#[test]
fn facts_can_be_removed_and_progress_is_bounded() {
    let issue = summary()
        .with_priority(Some(1))
        .with_priority(None)
        .with_assignee_id(Some("agent"))
        .with_assignee_id(None)
        .with_assignee_kind(Some("agent"))
        .with_assignee_kind(None)
        .with_assignee_rank(Some("captain"))
        .with_assignee_rank(None)
        .with_progress(Some(IssueProgress::new(9, 7)))
        .with_progress(None)
        .with_owner(Some(IssuePerson::new("id", "name", "human")))
        .with_owner(None)
        .with_pull_requests(Some(PullRequestSummary::new(0, 0, 0, 0)))
        .with_pull_requests(None)
        .with_due_date(Some("2026-11-20"))
        .with_due_date(None)
        .with_latest_activity(Some(LatestActivity::new("issue.created", "invalid")))
        .with_latest_activity(None)
        .with_ready_for_agents(Some(false))
        .with_ready_for_agents(None);
    assert!(
        issue.priority().is_none()
            && issue.assignee_kind().is_none()
            && issue.assignee_rank().is_none()
            && issue.owner().is_none()
    );
    assert!(
        issue.progress().is_none()
            && issue.pull_requests().is_none()
            && issue.due_date().is_none()
            && issue.latest_activity().is_none()
    );
    assert!(issue.assignee_id().is_none() && issue.ready_for_agents().is_none());
    assert_eq!(
        summary()
            .with_ready_for_agents(Some(false))
            .ready_for_agents(),
        Some(false),
        "not ready is a fact, not an absence"
    );
    assert_eq!(IssueProgress::new(9, 7).done, 7);
}
