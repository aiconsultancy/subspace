//! The hub's pull request answers: batching, rounds, clearing on `changed`,
//! what tooltips say and where a click goes.

use subspace_framework::contracts::MessageKey;
use subspace_framework::contracts::github::{
    MAX_RESOLVE, PullRequestDetails, PullRequestStatus, Resolution, ResolutionState,
    ResolvePullRequests,
};
use subspace_framework::dashboard::contracts::ContractError;

use super::PullRequestReferences;
use super::follow::navigation_url;
use super::tooltip::note;
use crate::markup::PrRef;

fn keys(range: std::ops::Range<usize>) -> Vec<String> {
    range.map(|n| format!("octo-org/api#{n}")).collect()
}

fn details(key: &str) -> PullRequestDetails {
    let (repo, number) = key.split_once('#').unwrap();
    PullRequestDetails::new(
        key,
        "Fixture title",
        &format!("https://github.com/{repo}/pull/{number}"),
        PullRequestStatus::Open,
    )
}

fn unavailable(keys: &[String]) -> Vec<Resolution> {
    keys.iter()
        .map(|key| Resolution::new(key, ResolutionState::Unavailable))
        .collect()
}

#[test]
fn a_frame_s_rows_coalesce_into_one_deduplicated_request() {
    let mut prs = PullRequestReferences::default();
    assert!(prs.want(keys(1..3)), "the first row schedules");
    assert!(!prs.want(keys(2..4)), "later rows join it");
    let requests = prs.take_requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].keys(), keys(1..4).as_slice());
    assert!(prs.take_requests().is_empty(), "taken once");
}

#[test]
fn requests_hold_at_most_fifty_pull_requests() {
    let mut prs = PullRequestReferences::default();
    prs.want(keys(1..121));
    let sizes: Vec<usize> = prs
        .take_requests()
        .iter()
        .map(|request| request.keys().len())
        .collect();
    assert_eq!(sizes, [MAX_RESOLVE, MAX_RESOLVE, 20]);
}

#[test]
fn answered_pull_requests_are_asked_again_only_after_changed() {
    let mut prs = PullRequestReferences::default();
    prs.want(keys(1..2));
    prs.take_requests();
    let pending = vec![Resolution::new("octo-org/api#1", ResolutionState::Pending)];
    assert!(prs.record(pending.clone()), "pending is new");
    assert!(!prs.record(pending), "the same answer changes nothing");
    assert!(!prs.want(keys(1..2)), "pending waits for changed");
    prs.changed();
    assert!(prs.state("octo-org/api#1").is_none(), "nothing is kept");
    assert!(prs.want(keys(1..2)), "a new round asks again");
}

#[test]
fn changed_forgets_every_detail_so_nothing_private_outlives_the_account() {
    let mut prs = PullRequestReferences::default();
    prs.want(keys(1..2));
    prs.take_requests();
    prs.record(vec![Resolution::new(
        "octo-org/api#1",
        ResolutionState::Available(details("octo-org/api#1")),
    )]);
    assert!(matches!(
        prs.state("octo-org/api#1"),
        Some(ResolutionState::Available(_))
    ));
    prs.changed();
    assert!(prs.state("octo-org/api#1").is_none());
}

#[test]
fn a_pending_refresh_keeps_the_summary_until_a_new_answer() {
    let mut prs = PullRequestReferences::default();
    let key = "octo-org/api#1";
    prs.record(vec![Resolution::new(
        key,
        ResolutionState::Available(details(key)),
    )]);

    prs.refreshed();
    assert!(prs.want([key.to_owned()]));
    prs.take_requests();
    assert!(!prs.record(vec![Resolution::new(key, ResolutionState::Pending)]));
    assert!(matches!(
        prs.state(key),
        Some(ResolutionState::Available(_))
    ));

    prs.refreshed();
    assert!(prs.want([key.to_owned()]));
    prs.take_requests();
    assert!(prs.record(vec![Resolution::new(key, ResolutionState::Unavailable)]));
    assert!(matches!(prs.state(key), Some(ResolutionState::Unavailable)));
}

#[test]
fn answers_are_recorded_by_canonical_identity() {
    let mut prs = PullRequestReferences::default();
    prs.record(vec![Resolution::new(
        "Octo-Org/API#7",
        ResolutionState::SignedOut,
    )]);
    assert!(matches!(
        prs.state("octo-org/api#7"),
        Some(ResolutionState::SignedOut)
    ));
}

#[test]
fn a_refused_request_leaves_chips_without_details_until_changed() {
    let mut prs = PullRequestReferences::default();
    prs.want(keys(1..3));
    let key = MessageKey::request::<ResolvePullRequests>();
    let changed = prs.resolve_wanted(|_| Err(ContractError::Unavailable { key }));
    assert!(changed);
    assert!(matches!(
        prs.state("octo-org/api#1"),
        Some(ResolutionState::Unavailable)
    ));
    assert!(!prs.want(keys(1..3)), "not asked again every frame");
    prs.changed();
    assert!(prs.want(keys(1..3)));
}

#[test]
fn resolving_records_every_batch() {
    let mut prs = PullRequestReferences::default();
    prs.want(keys(1..61));
    let mut asked = Vec::new();
    let changed = prs.resolve_wanted(|request| {
        asked.push(request.keys().len());
        Ok(unavailable(request.keys()))
    });
    assert!(changed);
    assert_eq!(asked, [50, 10]);
    assert!(matches!(
        prs.state("octo-org/api#60"),
        Some(ResolutionState::Unavailable)
    ));
}

#[test]
fn tooltips_say_details_are_unavailable_without_saying_why_it_is_private() {
    assert_eq!(note("api#1", None), "Loading api#1…");
    assert_eq!(
        note("api#1", Some(&ResolutionState::Pending)),
        "Loading api#1…"
    );
    for state in [
        ResolutionState::Unavailable,
        ResolutionState::SignedOut,
        ResolutionState::Failed,
    ] {
        assert_eq!(note("api#1", Some(&state)), "Details unavailable");
    }
}

#[test]
fn navigation_uses_the_canonical_url_and_keeps_the_owner_s_case() {
    let found = PrRef::from_url("https://github.com/Octo-Org/API/pull/7/files?w=1#r1").unwrap();
    assert_eq!(found.key, "octo-org/api#7");
    assert_eq!(
        navigation_url(&found).as_deref(),
        Some("https://github.com/Octo-Org/API/pull/7")
    );
    let marker = PrRef {
        key: "octo-org/api#8".into(),
        url: "https://github.com/octo-org/api/pull/8".into(),
    };
    assert_eq!(
        navigation_url(&marker).as_deref(),
        Some("https://github.com/octo-org/api/pull/8")
    );
}
