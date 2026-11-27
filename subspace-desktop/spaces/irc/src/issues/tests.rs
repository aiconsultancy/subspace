//! The hub's issue answers: scope, batching, rounds and what tooltips say.

use subspace_framework::contracts::MessageKey;
use subspace_framework::contracts::issues::{
    IssueScope, IssueState, IssueSummary, MAX_RESOLVE, Resolution, ResolutionState, StateType,
};
use subspace_framework::dashboard::contracts::ContractError;

use super::IssueReferences;
use super::follow::navigation;
use super::tooltip::note;

fn scope(profile: &str) -> IssueScope {
    IssueScope::new(profile, "Subspace API", "https://api.example.test/api")
        .with_project_keys(["TST", "OPS"])
}

fn summary(identifier: &str) -> IssueSummary {
    IssueSummary::new(
        identifier,
        "Fixture title",
        &format!("https://api.example.test/api/v1/issues/{identifier}"),
        IssueState::new("In Progress", StateType::Started),
    )
}

fn scoped() -> IssueReferences {
    let mut issues = IssueReferences::default();
    assert!(issues.set_scope(Some(scope("dev"))));
    issues
}

fn ids(range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|n| format!("TST-{n}")).collect()
}

#[test]
fn nothing_is_wanted_without_a_scope() {
    let mut issues = IssueReferences::default();
    assert!(!issues.want(ids(1..3)));
    assert!(issues.take_requests().is_empty());
    assert!(issues.keys().leading("TST-1").is_none());
}

#[test]
fn a_frame_s_rows_coalesce_into_one_deduplicated_request() {
    let mut issues = scoped();
    assert!(issues.want(ids(1..3)), "the first row schedules");
    assert!(!issues.want(ids(2..4)), "later rows join it");
    let requests = issues.take_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].identifiers(), ids(1..4).as_slice());
    assert_eq!(requests[0].profile_id(), "dev");
    assert!(issues.take_requests().is_empty(), "taken once");
}

#[test]
fn requests_hold_at_most_fifty_identifiers() {
    let mut issues = scoped();
    issues.want(ids(1..121));
    let sizes: Vec<usize> = issues
        .take_requests()
        .iter()
        .map(|request| request.identifiers().len())
        .collect();
    assert_eq!(sizes, [MAX_RESOLVE, MAX_RESOLVE, 20]);
}

#[test]
fn answered_identifiers_are_asked_again_only_after_changed() {
    let mut issues = scoped();
    issues.want(ids(1..2));
    issues.take_requests();
    let pending = vec![Resolution::new("TST-1", ResolutionState::Pending)];
    assert!(issues.record(pending.clone()), "pending is new");
    assert!(!issues.record(pending), "the same answer changes nothing");
    assert!(!issues.want(ids(1..2)), "pending waits for changed");
    issues.start_round();
    assert!(
        matches!(issues.state("TST-1"), Some(ResolutionState::Pending)),
        "the old answer stays shown"
    );
    assert!(issues.want(ids(1..2)), "a new round asks again");
    let available = vec![Resolution::new(
        "TST-1",
        ResolutionState::Available(summary("TST-1")),
    )];
    assert!(issues.record(available));
    assert!(matches!(
        issues.state("TST-1"),
        Some(ResolutionState::Available(_))
    ));
}

#[test]
fn pending_refreshes_preserve_the_latest_issue_and_keep_missing_issues_hidden() {
    let mut issues = scoped();
    let key = "TST-1";
    for state in [
        ResolutionState::Available(summary(key)),
        ResolutionState::Unavailable,
    ] {
        assert!(issues.record(vec![Resolution::new(key, state.clone())]));
        issues.start_round();
        assert!(!issues.record(vec![Resolution::new(key, ResolutionState::Pending)]));
        assert!(issues.state(key) == Some(&state));
        assert!(!issues.want([key.to_owned()]));
    }
}

#[test]
fn another_profile_forgets_every_answer() {
    let mut issues = scoped();
    issues.record(vec![Resolution::new("TST-1", ResolutionState::Unavailable)]);
    assert!(issues.is_unavailable("TST-1"));
    assert!(!issues.set_scope(Some(scope("dev"))), "the same scope");
    assert!(issues.is_unavailable("TST-1"));
    assert!(issues.set_scope(Some(scope("other"))));
    assert!(issues.state("TST-1").is_none());
}

#[test]
fn a_refused_request_stops_recognition() {
    let mut issues = scoped();
    issues.want(ids(1..3));
    let key = MessageKey::request::<subspace_framework::contracts::issues::Resolve>();
    let changed = issues.resolve_wanted(|_| Err(ContractError::Unavailable { key }));
    assert!(changed);
    assert!(issues.profile_name().is_none());
    assert_eq!(issues.keys(), &crate::markup::IssueKeys::default());
}

#[test]
fn resolving_records_every_batch() {
    let mut issues = scoped();
    issues.want(ids(1..61));
    let mut asked = Vec::new();
    let changed = issues.resolve_wanted(|request| {
        asked.push(request.identifiers().len());
        Ok(request
            .identifiers()
            .iter()
            .map(|id| Resolution::new(id, ResolutionState::Unavailable))
            .collect())
    });
    assert!(changed);
    assert_eq!(asked, [50, 10]);
    assert!(issues.is_unavailable("TST-60"));
}

#[test]
fn tooltips_say_why_there_is_no_summary() {
    assert_eq!(note("TST-1", None, "Subspace API"), "Loading TST-1…");
    assert_eq!(
        note("TST-1", Some(&ResolutionState::Pending), "Subspace API"),
        "Loading TST-1…"
    );
    assert_eq!(
        note("TST-1", Some(&ResolutionState::Unavailable), "Subspace API"),
        "TST-1 isn't available with Subspace API."
    );
    assert_eq!(
        note("TST-1", Some(&ResolutionState::SignedOut), "Subspace API"),
        "Sign in to Subspace API to see this issue."
    );
    assert_eq!(
        note("TST-1", Some(&ResolutionState::Failed), "Subspace API"),
        "Couldn't reach Subspace API."
    );
}

#[test]
fn navigation_does_not_wait_for_the_preview() {
    let mut issues = scoped();
    let expected = Some("https://api.example.test/api/v1/issues/TST-5?profile=dev");
    assert_eq!(navigation(&issues, "TST-5").as_deref(), expected);
    issues.record(vec![Resolution::new("TST-5", ResolutionState::Pending)]);
    assert_eq!(navigation(&issues, "TST-5").as_deref(), expected);
    assert!(navigation(&issues, "OPS-5/other").is_none());
    assert!(navigation(&issues, "OTHER-5").is_none());
    issues.set_scope(None);
    assert!(navigation(&issues, "TST-5").is_none());
}

#[test]
fn a_summary_can_only_open_in_its_current_scope() {
    let mut issues = scoped();
    let mut resolved = summary("TST-6");
    resolved.url = "https://api.example.test/api/v1/issues/TST-6?profile=old-profile".into();
    issues.record(vec![Resolution::new(
        "TST-6",
        ResolutionState::Available(resolved.clone()),
    )]);
    assert_eq!(
        navigation(&issues, "TST-6").as_deref(),
        Some("https://api.example.test/api/v1/issues/TST-6?profile=dev")
    );
    resolved.url = "https://elsewhere.example.test/issues/TST-6".into();
    issues.record(vec![Resolution::new(
        "TST-6",
        ResolutionState::Available(resolved),
    )]);
    assert!(navigation(&issues, "TST-6").is_none());
}
