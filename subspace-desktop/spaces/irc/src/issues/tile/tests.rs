use super::{Variant, activity, facts, variant};
use chrono::{DateTime, Duration, Utc};
use subspace_framework::contracts::issues::{
    IssuePerson, IssueProgress, IssueState, IssueSummary, PullRequestSummary, StateType,
};
fn issue() -> IssueSummary {
    IssueSummary::new(
        "SUB-28",
        "Live issue updates",
        "https://example.test/SUB-28",
        IssueState::new("In progress", StateType::Started),
    )
}
fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-16T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}
#[test]
fn hover_card_variant_follows_issue_type() {
    let ordinary = issue();
    let tracker = issue().with_markers(true, false);
    assert_eq!(variant(&ordinary), Variant::Ordinary);
    assert_eq!(variant(&tracker), Variant::Tracker);
}
#[test]
fn absent_optional_data_has_no_pills_footer_or_pr_facts() {
    let issue = issue();
    assert!(facts::priority(issue.priority()).is_none());
    assert!(!facts::has_footer(&issue));
    assert!(facts::owner(&issue).is_none());
    assert!(facts::open_prs(&PullRequestSummary::new(0, 0, 0, 0)).is_none());
    assert_eq!(
        super::label(&issue, now()),
        "Open SUB-28 in Issues: Live issue updates, In progress"
    );
    for priority in [0, 3, 4, 5] {
        assert!(facts::priority(Some(priority)).is_none());
    }
}
#[test]
fn labels_include_assignment_rank_owner_and_tracker_facts() {
    let issue = issue()
        .with_priority(Some(2))
        .with_assignee(Some("protostar"))
        .with_assignee_kind(Some("agent"))
        .with_assignee_rank(Some("captain"))
        .with_owner(Some(IssuePerson::new("owner", "Rosa Fixture", "human")))
        .with_markers(true, true)
        .with_progress(Some(IssueProgress::new(3, 7)))
        .with_pull_requests(Some(PullRequestSummary::new(5, 1, 0, 4)))
        .with_due_date(Some("2026-11-20"));
    let label = super::label(&issue, now());
    for fact in [
        "Tracker",
        "Archived",
        "high priority",
        "assigned to captain protostar",
        "for Rosa Fixture",
        "3 of 7 done",
        "1 open pull requests, 4 merged",
        "due 2026-11-20",
    ] {
        assert!(label.contains(fact), "{label}");
    }
    assert!(facts::is_agent(&issue));
    let admiral = issue.clone().with_assignee_rank(Some("rearAdmiral"));
    assert_eq!(facts::rank(&admiral).as_deref(), Some("rear admiral"));
    assert!(super::label(&admiral, now()).contains("assigned to rear admiral protostar"));
    let same = issue.with_assignee(Some("Rosa Fixture"));
    assert!(facts::owner(&same).is_none());
}
#[test]
fn checks_are_only_shown_for_open_prs_and_recognised_states() {
    for (state, suffix) in [
        ("success", " · checks pass"),
        ("failure", " · checks failing"),
        ("pending", ""),
        ("future-value", ""),
    ] {
        let prs = PullRequestSummary::new(1, 1, 0, 0).with_checks(Some(state));
        assert_eq!(facts::open_prs(&prs).unwrap().0, format!("1 open{suffix}"));
    }
    assert_eq!(
        facts::open_prs(&PullRequestSummary::new(1, 1, 0, 0))
            .unwrap()
            .0,
        "1 open"
    );
}
#[test]
fn relative_time_is_pure_and_handles_thresholds_and_clock_skew() {
    for (elapsed, expected, recent) in [
        (0, "0m ago", true),
        (4 * 60, "4m ago", true),
        (3599, "59m ago", true),
        (3600, "1h ago", false),
        (3 * 3600, "3h ago", false),
        (86400, "1d ago", false),
        (2 * 86400, "2d ago", false),
        (7 * 86400, "09 Nov 2026", false),
        (-60, "just now", false),
    ] {
        let at = (now() - Duration::seconds(elapsed)).to_rfc3339();
        let relative = activity::relative(&at, now()).unwrap();
        assert_eq!(relative.label, expected);
        assert_eq!(relative.recent, recent);
    }
    assert!(activity::relative("invalid", now()).is_none());
    assert_eq!(
        activity::relative("2026-11-16T13:56:00+02:00", now())
            .unwrap()
            .label,
        "4m ago"
    );
}
#[test]
fn due_dates_have_weekday_and_date_forms_and_invalid_dates_are_omitted() {
    assert_eq!(
        activity::due("2026-11-20", now().date_naive()).as_deref(),
        Some("Due Fri")
    );
    assert_eq!(
        activity::due("2026-11-20T00:00:00Z", now().date_naive()).as_deref(),
        Some("Due Fri")
    );
    assert_eq!(
        activity::due("2027-01-01", now().date_naive()).as_deref(),
        Some("Due 01 Jan 2027")
    );
    assert!(activity::due("bad date", now().date_naive()).is_none());
}

#[test]
fn owner_identity_takes_precedence_over_equal_display_names() {
    let issue = issue()
        .with_assignee(Some("Rosa Fixture"))
        .with_assignee_id(Some("agent"))
        .with_owner(Some(IssuePerson::new("human", "Rosa Fixture", "human")));
    assert_eq!(facts::owner(&issue), Some("Rosa Fixture"));
    let same = issue.with_assignee_id(Some("human"));
    assert!(facts::owner(&same).is_none());
}
#[test]
fn the_label_reads_latest_activity_as_relative_time() {
    let at = (now() - Duration::minutes(4)).to_rfc3339();
    let issue = issue().with_latest_activity(Some(
        subspace_framework::contracts::issues::LatestActivity::new("issue.changed", &at),
    ));
    let label = super::label(&issue, now());
    assert!(label.contains("latest activity 4m ago"), "{label}");
    assert!(!label.contains(&at), "no raw timestamp: {label}");
}

#[test]
fn only_an_answer_without_a_summary_gives_a_reason() {
    use subspace_framework::contracts::issues::ResolutionState;
    assert_eq!(super::absence(None), None, "not asked yet");
    assert_eq!(super::absence(Some(&ResolutionState::Pending)), None);
    assert_eq!(
        super::absence(Some(&ResolutionState::Unavailable)),
        Some("Unavailable")
    );
    assert_eq!(
        super::absence(Some(&ResolutionState::SignedOut)),
        Some("Signed out")
    );
    assert_eq!(
        super::absence(Some(&ResolutionState::Failed)),
        Some("Couldn't load")
    );
}
