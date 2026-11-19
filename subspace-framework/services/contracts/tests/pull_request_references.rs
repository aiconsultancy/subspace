//! Pull request references in text: lenient URL identity, `[PR:…]` marker
//! keys and the summary request that resolves them.

use subspace_contracts::github::recognition::PullRequestLink;
use subspace_contracts::github::{
    Changed, CheckStatus, MAX_RESOLVE, PullRequestDetails, PullRequestStatus, Resolution,
    ResolutionState, ResolvePullRequests, ReviewStatus,
};
use subspace_contracts::{MessageKey, MessageKind};

#[test]
fn suffixed_pull_request_urls_resolve_to_the_canonical_identity() {
    for url in [
        "https://github.com/example-org/subspace/pull/37",
        "https://github.com/example-org/subspace/pull/37/",
        "https://github.com/example-org/subspace/pull/37/files",
        "https://github.com/example-org/subspace/pull/37/files/",
        "https://github.com/example-org/subspace/pull/37/commits",
        "https://github.com/example-org/subspace/pull/37?w=1",
        "https://github.com/example-org/subspace/pull/37/files?diff=split#diff-abc",
        "https://github.com/example-org/subspace/pull/37#discussion_r1234",
        "https://github.com/example-org/subspace/pull/37#issuecomment-99",
    ] {
        let link = PullRequestLink::parse_lenient(url).unwrap_or_else(|| panic!("{url}"));
        assert_eq!(link.key(), "example-org/subspace#37", "{url}");
        assert_eq!(
            link.url(),
            "https://github.com/example-org/subspace/pull/37"
        );
    }
}

#[test]
fn lenient_parsing_still_refuses_other_hosts_paths_and_escapes() {
    for url in [
        "http://github.com/o/r/pull/1",
        "https://github.com.evil.test/o/r/pull/1",
        "https://github.com/o/r/issues/1",
        "https://github.com/o/r/pull/1/checks",
        "https://github.com/o/r/pull/1/files/extra",
        "https://github.com/o/r/pull/0",
        "https://github.com/o/r/pull/%31",
        "https://github.com/o/r/../r/pull/1",
        "https://github.com/o/r/pull/1//",
        "https://github.com/o/r/pull/?x=1",
        "https://user@github.com/o/r/pull/1",
    ] {
        assert!(PullRequestLink::parse_lenient(url).is_none(), "{url}");
    }
}

#[test]
fn the_strict_parser_keeps_refusing_suffixes() {
    for url in [
        "https://github.com/o/r/pull/1/files",
        "https://github.com/o/r/pull/1?x=1",
        "https://github.com/o/r/pull/1#discussion_r1",
        "https://github.com/o/r/pull/1/",
    ] {
        assert!(PullRequestLink::parse(url).is_none(), "{url}");
    }
}

#[test]
fn marker_keys_are_owner_repository_and_number() {
    let link = PullRequestLink::from_key("example-org/subspace#37").unwrap();
    assert_eq!(
        Some(link),
        PullRequestLink::parse("https://github.com/example-org/subspace/pull/37")
    );
    for key in [
        "subspace#37",
        "example-org/subspace",
        "example-org/subspace#",
        "example-org/subspace#0",
        "example-org/subspace#x",
        "example-org/sub/space#1",
        "-example/subspace#1",
        "example-org/subspace#1 ",
        "/subspace#1",
    ] {
        assert!(PullRequestLink::from_key(key).is_none(), "{key}");
    }
}

#[test]
fn resolution_messages_are_named_in_the_github_contract() {
    let changed = MessageKey::event::<Changed>();
    assert_eq!(changed.label(), "github@1/changed");
    assert_eq!(changed.kind, MessageKind::Event);
    let resolve = MessageKey::request::<ResolvePullRequests>();
    assert_eq!(resolve.label(), "github@1/resolve-pull-requests");
    assert_eq!(resolve.kind, MessageKind::Request);
    assert!(changed.is_valid() && resolve.is_valid());
}

#[test]
fn a_resolve_request_keeps_distinct_canonical_keys_up_to_the_limit() {
    let request = ResolvePullRequests::new(["o/r#2", "o/r#1", "o/r#2", "not a key", "repo-only#3"]);
    assert_eq!(request.keys(), ["o/r#2", "o/r#1"]);
    let many = (1..=80).map(|number| format!("o/r#{number}"));
    let capped = ResolvePullRequests::new(many);
    assert_eq!(capped.keys().len(), MAX_RESOLVE);
    assert_eq!(capped.keys()[0], "o/r#1");
}

#[test]
fn details_build_without_struct_literals() {
    let details = PullRequestDetails::new(
        "o/r#7",
        "Chips",
        "https://github.com/o/r/pull/7",
        PullRequestStatus::Draft,
    )
    .with_author(Some("octo"))
    .with_review(Some(ReviewStatus::ChangesRequested))
    .with_checks(Some(CheckStatus::Failure))
    .with_updated_at(Some("2026-11-17T10:00:00Z"))
    .with_branches(Some("feat"), Some("main"))
    .with_changes(Some(12), Some(3));
    assert_eq!(details.key, "o/r#7");
    assert_eq!(details.status, PullRequestStatus::Draft);
    assert_eq!(details.author.as_deref(), Some("octo"));
    assert_eq!(details.review, Some(ReviewStatus::ChangesRequested));
    assert_eq!(details.checks, Some(CheckStatus::Failure));
    assert_eq!(details.updated_at.as_deref(), Some("2026-11-17T10:00:00Z"));
    assert_eq!(details.head_ref.as_deref(), Some("feat"));
    assert_eq!(details.base_ref.as_deref(), Some("main"));
    assert_eq!((details.additions, details.deletions), (Some(12), Some(3)));
    let answer = Resolution::new("o/r#7", ResolutionState::Available(details));
    assert_eq!(answer.key, "o/r#7");
    assert!(matches!(answer.state, ResolutionState::Available(_)));
    let gone = Resolution::new("o/r#8", ResolutionState::Unavailable);
    assert!(gone.state == ResolutionState::Unavailable);
}
