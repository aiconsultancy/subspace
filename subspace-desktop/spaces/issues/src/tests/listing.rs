use std::time::{Duration, Instant};

use subspace_api_client::hub::ScopeGeneration;

use super::{Failure, IssueGroup, ListId, Listing, More, STALE_AFTER, Status, updated};
use crate::fixtures::issue;
use crate::model::StateKind;
use serde_json::json;

pub(crate) fn group(key: &str, name: &str, kind: StateKind, rows: usize) -> IssueGroup {
    IssueGroup {
        index: 0,
        key: key.into(),
        name: name.into(),
        kind,
        total: Some(rows as u32),
        rows: (0..rows)
            .map(|n| {
                issue(
                    &format!("SUB-{}", 100 + n),
                    "Sample issue",
                    (name, "started"),
                    json!({}),
                )
            })
            .collect(),
        more: More::None,
    }
}

#[test]
fn a_network_failure_keeps_the_content_this_session_read() {
    let now = Instant::now();
    let mut listing = Listing::default();
    listing.loaded(vec![group("s1", "In Progress", StateKind::Started, 2)], now);
    listing.start_read();
    listing.failed(Failure::Offline);
    assert!(listing.content.is_some());
    assert_eq!(listing.offline(), Some(&Failure::Offline));
    assert_eq!(listing.issue_count(), Some(2));
}

#[test]
fn an_empty_first_page_ends_the_group_for_its_own_list_only() {
    let unread = || {
        let mut done = group("done", "Done", StateKind::Completed, 0);
        done.total = Some(3);
        done.more = More::Unread;
        vec![done]
    };
    let settled = |listing: &mut Listing| {
        let mut groups = unread();
        listing.settle_unread(&mut groups);
        groups[0].more
    };
    let first = ListId {
        generation: ScopeGeneration::default(),
        installation: 1,
    };
    let replacement = ListId {
        installation: 2,
        ..first
    };
    let mut listing = Listing::default();
    listing.follow_list(Some(first));
    assert_eq!(
        settled(&mut listing),
        More::Unread,
        "a first page to ask for"
    );
    listing.asked_first_page("done");
    assert_eq!(
        settled(&mut listing),
        More::None,
        "completed empty, it ran out"
    );
    listing.follow_list(Some(first));
    listing.drop_content();
    assert_eq!(
        settled(&mut listing),
        More::None,
        "the same list, or hidden, keeps it"
    );
    listing.follow_list(Some(replacement));
    assert_eq!(settled(&mut listing), More::Unread, "a new list asks again");
    listing.asked_first_page("done");
    listing.clear();
    assert_eq!(settled(&mut listing), More::Unread, "so does another scope");
}

#[test]
fn another_status_after_content_is_the_network_error_not_offline() {
    let now = Instant::now();
    let mut listing = Listing::default();
    listing.loaded(vec![group("s1", "In Progress", StateKind::Started, 2)], now);
    for failure in [Failure::Network, Failure::Unexpected { status: None }] {
        listing.failed(failure.clone());
        assert!(listing.content.is_none(), "{failure:?}");
        assert_eq!(listing.offline(), None);
    }
}

#[test]
fn a_refused_read_clears_the_content() {
    for failure in [Failure::Edge, Failure::Forbidden] {
        let mut listing = Listing::default();
        listing.loaded(Vec::new(), Instant::now());
        listing.failed(failure.clone());
        assert!(listing.content.is_none(), "{failure:?}");
        assert_eq!(listing.offline(), None);
        assert_eq!(listing.status, Status::Failed(failure));
    }
}

#[test]
fn account_level_states_are_the_session_and_access_ones() {
    assert!(Failure::Edge.is_account_level());
    assert!(Failure::Forbidden.is_account_level());

    assert!(!Failure::Network.is_account_level());
    assert!(!Failure::Unexpected { status: Some(502) }.is_account_level());
}

#[test]
fn the_list_is_read_again_after_a_minute() {
    let start = Instant::now();
    let mut listing = Listing::default();
    assert!(listing.is_stale(start), "never read");
    listing.start_read();
    assert!(!listing.is_stale(start), "already reading");
    listing.loaded(Vec::new(), start);
    assert!(!listing.is_stale(start + STALE_AFTER));
    assert!(listing.is_stale(start + STALE_AFTER + Duration::from_secs(1)));
    listing.clear();
    assert!(listing.content.is_none());
    assert_eq!(listing.status, Status::Idle);
}

#[test]
fn freshness_reads_in_minutes() {
    let read = Instant::now();
    assert_eq!(
        updated(read, read + Duration::from_secs(59)),
        "updated just now"
    );
    assert_eq!(
        updated(read, read + Duration::from_secs(61)),
        "updated 1 min ago"
    );
    assert_eq!(
        updated(read, read + Duration::from_secs(59 * 60)),
        "updated 59 min ago"
    );
    assert_eq!(
        updated(read, read + Duration::from_secs(2 * 3600)),
        "updated 2 h ago"
    );
}

#[test]
fn a_group_without_a_total_counts_its_rows() {
    let mut listing = Listing::default();
    let mut todo = group("s2", "Todo", StateKind::Unstarted, 3);
    todo.total = None;
    listing.loaded(
        vec![group("s1", "In Progress", StateKind::Started, 4), todo],
        Instant::now(),
    );
    assert_eq!(listing.issue_count(), Some(7));
    assert!(listing.group_mut("s2").is_some());
    assert!(listing.group_mut("s9").is_none());
}
