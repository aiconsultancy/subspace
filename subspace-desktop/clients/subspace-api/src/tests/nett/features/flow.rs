use super::super::{server::NettMock, wait_for};
use super::{org_path, own_client};
use crate::PageRequest;

#[test]
fn a_long_retry_after_is_reported_and_never_cut_short() {
    let mock = NettMock::start();
    let client = own_client(&mock, "nett-fixture-busy");
    let teams = org_path(&mock, "issue-teams");
    mock.fail_next("GET", &teams, 503, Some(45), 1);
    let start = std::time::Instant::now();
    assert_eq!(
        client.projects(&PageRequest::default()).err(),
        Some(crate::Error::UnexpectedStatus(503))
    );
    // The next request is refused here, without reaching NETT, until the
    // 45 seconds have passed.
    assert_eq!(
        client.projects(&PageRequest::default()).err(),
        Some(crate::Error::UnexpectedStatus(429))
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(mock.count("GET", "/issue-teams"), 1);
}

#[test]
fn a_day_long_wait_does_not_leave_issues_dead_and_reload_sends_again() {
    use crate::hub::{Hub, HubStatus};
    let mock = NettMock::start();
    let token = "nett-fixture-day-long-wait";
    let hub = Hub::from_client(own_client(&mock, token), "NETT");
    let query = crate::ListQuery::new(crate::ListTab::All).project(Some("OPS"));
    hub.activate(query.clone()).unwrap();
    wait_for("the list", &|| hub.snapshot().status() == &HubStatus::Ready);
    // NETT, or anything in front of it, answers 503 and asks for a day.
    mock.fail_next(
        "GET",
        &org_path(&mock, "issues/groups"),
        503,
        Some(86_400),
        1,
    );
    hub.reload(query.clone()).unwrap();
    wait_for("the refusal", &|| {
        matches!(hub.snapshot().status(), HubStatus::Error(_))
    });
    let account = own_client(&mock, token).flow_account();
    assert!(
        crate::nett::flow::paused_for(account)
            .is_some_and(|wait| wait <= crate::nett::flow::MAX_PAUSE),
        "held for five minutes at most"
    );
    // Reload is the person asking: it is sent at once.
    let reads = mock.count("GET", "/issues/groups");
    hub.reload(query).unwrap();
    wait_for("the list again", &|| {
        hub.snapshot().status() == &HubStatus::Ready && mock.count("GET", "/issues/groups") > reads
    });
    hub.stop();
}
