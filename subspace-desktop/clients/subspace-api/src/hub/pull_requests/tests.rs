use super::load;
use crate::PageRequest;
use crate::hub::{
    lock,
    worker::{Job, Task},
};
use crate::tests::{common, connected, detail, page, wait};
use subspace_framework::client_test_support::Mock;

#[test]
fn a_stale_page_cannot_replace_an_issue_opened_after_close_or_reload() {
    let mock = fixture();
    let hub = connected(&mock);
    wait(&hub, |s| s.issue().unwrap().pull_requests().is_some());
    let request = PageRequest::default();
    let stale = Job::new(
        &lock(&hub.0.state),
        Task::PullRequests("SUB-1".into(), request.clone(), 0),
    );
    hub.close_issue();
    hub.open_issue("SUB-2", crate::PROFILE_ID).unwrap();
    wait(&hub, |s| s.issue().unwrap().pull_requests().is_some());
    load(&hub.0, &stale, ("SUB-1", &request, 0));
    assert_eq!(
        hub.snapshot()
            .issue()
            .unwrap()
            .pull_requests()
            .unwrap()
            .items[0]
            .id,
        "current"
    );
    let stale = Job::new(
        &lock(&hub.0.state),
        Task::PullRequests("SUB-2".into(), request.clone(), 0),
    );
    hub.reload_issue().unwrap();
    wait(&hub, |s| {
        s.issue().unwrap().status() == &crate::hub::IssueStatus::Loaded
            && !s.issue().unwrap().pull_requests_loading()
    });
    load(&hub.0, &stale, ("SUB-1", &request, 0));
    assert_eq!(
        hub.snapshot()
            .issue()
            .unwrap()
            .pull_requests()
            .unwrap()
            .items[0]
            .id,
        "current"
    );
    hub.stop();
    assert!(hub.snapshot().issue().is_none());
}

fn fixture() -> Mock {
    Mock::new(|r| {
        if r.path == "/api/v1/issues/SUB-2" {
            return (200, detail("2").to_string());
        }
        if r.path.contains("/pull-requests?") {
            return (
                200,
                page(
                    vec![crate::tests::pull_requests::pull_request(
                        if r.path.contains("SUB-1") {
                            "old"
                        } else {
                            "current"
                        },
                    )],
                    None,
                ),
            );
        }
        common(r)
    })
}

#[test]
fn not_found_discards_previously_loaded_rows_for_reload_and_continuation() {
    for more in [false, true] {
        let mut requests = super::PullRequests::default();
        let page = serde_json::from_str(&page(
            vec![crate::tests::pull_requests::pull_request("old")],
            Some("next"),
        ))
        .unwrap();
        requests.apply(Ok(page), &first(), 0);
        assert_eq!(requests.page.as_ref().unwrap().items.len(), 1);
        requests.loading = true;
        let request = if more { next("next") } else { first() };
        requests.apply(Err(crate::Error::NotFound), &request, requests.epoch);
        assert!(requests.page.is_none());
        assert!(requests.unavailable);
        assert!(!requests.loading);
        assert!(requests.error.is_none());
    }
}

fn first() -> crate::PageRequest {
    crate::PageRequest {
        limit: super::PAGE_SIZE,
        cursor: None,
    }
}

fn next(cursor: &str) -> crate::PageRequest {
    crate::PageRequest {
        limit: super::PAGE_SIZE,
        cursor: Some(cursor.into()),
    }
}

fn prs(ids: &[&str], cursor: Option<&str>) -> crate::model::Page<crate::model::PullRequest> {
    let rows = ids
        .iter()
        .map(|id| crate::tests::pull_requests::pull_request(id))
        .collect();
    serde_json::from_str(&page(rows, cursor)).unwrap()
}

fn ids(requests: &super::PullRequests) -> Vec<String> {
    let page = requests.page.as_ref().unwrap();
    page.items.iter().map(|pr| pr.id.clone()).collect()
}

#[test]
fn a_head_reread_replaces_the_page_and_drops_a_stale_continuation() {
    let mut requests = super::PullRequests::default();
    requests.apply(Ok(prs(&["old"], Some("c-1"))), &first(), 0);
    let before = requests.epoch;
    // An unlink elsewhere: the head read shows the new association only.
    requests.apply(Ok(prs(&["new"], Some("c-2"))), &first(), 0);
    assert_eq!(ids(&requests), ["new"]);
    // A continuation asked for before the re-read no longer applies.
    requests.apply(Ok(prs(&["stale"], None)), &next("c-1"), before);
    assert_eq!(ids(&requests), ["new"]);
    requests.apply(Ok(prs(&["more"], None)), &next("c-2"), requests.epoch);
    assert_eq!(ids(&requests), ["new", "more"]);
}

#[test]
fn a_continuation_with_the_same_cursor_across_a_reread_is_dropped_with_its_error() {
    let mut requests = super::PullRequests::default();
    requests.apply(Ok(prs(&["1"], Some("same"))), &first(), 0);
    let asked = requests.epoch;
    requests.loading = true;
    // The re-read's first page has the same cursor text but new rows.
    requests.apply(Ok(prs(&["2"], Some("same"))), &first(), 0);
    requests.loading = true;
    requests.apply(Ok(prs(&["stale"], None)), &next("same"), asked);
    assert_eq!(ids(&requests), ["2"]);
    assert_eq!(
        requests.page.as_ref().unwrap().next_cursor.as_deref(),
        Some("same"),
        "the new installation's cursor is untouched"
    );
    assert!(requests.loading, "the stale reply leaves loading alone");
    requests.apply(Err(crate::Error::Network), &next("same"), asked);
    assert!(requests.error.is_none(), "a stale error is dropped");
    assert!(requests.loading);
}
