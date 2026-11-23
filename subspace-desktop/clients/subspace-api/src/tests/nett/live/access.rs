use super::super::{TOKEN, organisation, server::NettMock, wait};
use super::{open_live_issue, streams, wait_for_live_list, wait_for_ready_catch_up, works_hub};
use crate::hub::{Hub, LiveStatus};
use crate::{Client, ListQuery, ListTab};
use serde_json::json;
use std::time::Duration;
use subspace_framework::core::Secret;

#[test]
fn a_refused_token_is_renewed_once_and_then_the_stream_stops() {
    let mock = NettMock::serving(&["updates"]);
    let org = organisation(&mock, "Fixture Works");
    mock.token("nett-fixture-revocable", "ada");
    let client = Client::nett_loopback(
        &mock.api(),
        Some(&org),
        Some(Secret::new("nett-fixture-revocable")),
    )
    .unwrap();
    let hub = Hub::from_client(client, "NETT");
    hub.activate(ListQuery::new(ListTab::All).project(Some("OPS")))
        .unwrap();
    wait("a live stream", Duration::from_secs(10), &|| {
        hub.snapshot().live() == LiveStatus::Live
    });
    mock.clear_requests();
    // Revoked before it expires: the stream closes, one reconnection is
    // refused with 401, the token is renewed and one more attempt made.
    mock.revoke_token("nett-fixture-revocable");
    wait("the stream off", Duration::from_secs(15), &|| {
        hub.snapshot().live() == LiveStatus::Off
    });
    let attempts = streams(&mock);
    assert_eq!(attempts, 2, "one attempt before and one after renewing");
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(streams(&mock), attempts);
    hub.stop();
}

#[test]
fn lost_access_closes_the_stream_and_retires_the_open_issue() {
    let mock = NettMock::with_args(&["updates"], &["--keepalive-seconds", "1"]);
    let (org, hub) = works_hub(&mock);
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait(
        "a live stream, the list, its counts and the issue",
        Duration::from_secs(10),
        &|| {
            let snapshot = hub.snapshot();
            snapshot.live() == LiveStatus::Live
                && snapshot.list().is_some()
                && snapshot.my_issues_count().is_some()
                && snapshot
                    .issue()
                    .is_some_and(|issue| issue.detail().is_some())
        },
    );
    mock.revoke_member(&org, "ada");
    // The stream closes at its next flush; the reconnection is refused with
    // 403; the open issue and the list are read again and, refused too, are
    // cleared.
    wait(
        "the issue and the list cleared",
        Duration::from_secs(15),
        &|| {
            let snapshot = hub.snapshot();
            snapshot.list().is_none()
                && snapshot.issue().is_some_and(|issue| {
                    issue.detail().is_none()
                        && matches!(issue.status(), crate::hub::IssueStatus::Error(_))
                })
        },
    );
    wait("the feature list again", Duration::from_secs(5), &|| {
        mock.count("GET", "/desktop/capabilities") >= 2
    });
    hub.stop();
}

#[test]
fn a_lasting_403_on_the_stream_stops_it_instead_of_polling() {
    let mock = NettMock::serving(&["updates"]);
    let (_, hub) = works_hub(&mock);
    wait_for_live_list(&hub);
    mock.config(&json!({"streamRefusal": {"status": 403, "retryAfter": 0, "times": 50}}));
    mock.clear_requests();
    mock.close_streams();
    wait("the stream off", Duration::from_secs(20), &|| {
        hub.snapshot().live() == LiveStatus::Off
    });
    let attempts = streams(&mock);
    assert_eq!(
        attempts, 2,
        "one reconnection, refused, then one more, refused"
    );
    // Each refusal read again what the person may see.
    assert!(mock.count("GET", "/issues/groups") >= 2);
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(streams(&mock), attempts, "no further attempts");
    hub.stop();
}

#[test]
fn a_stream_stopped_by_refusals_comes_back_when_another_read_finds_access_changed() {
    let mock = NettMock::serving(&["updates"]);
    let (org, hub) = works_hub(&mock);
    wait_for_live_list(&hub);
    mock.config(&json!({"streamRefusal": {"status": 403, "retryAfter": 0, "times": 50}}));
    mock.close_streams();
    wait("the stream off", Duration::from_secs(20), &|| {
        hub.snapshot().live() == LiveStatus::Off
    });
    // Access comes back, and an explicit reload's read of the same account
    // finds the capability list changed, while the followed teams stay.
    mock.config(&json!({"streamRefusal": null}));
    mock.capabilities(&org, &["updates", "attachments"]);
    let elsewhere =
        Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(TOKEN))).unwrap();
    assert_eq!(elsewhere.nett_refresh_served(), Ok(true));
    wait("a live stream again", Duration::from_secs(10), &|| {
        hub.snapshot().live() == LiveStatus::Live
    });
    hub.stop();
}

#[test]
fn a_change_the_last_refusals_own_read_finds_brings_the_stream_back() {
    let mock = NettMock::serving(&["updates"]);
    let (org, hub) = works_hub(&mock);
    wait_for_live_list(&hub);
    mock.config(&json!({"streamRefusal": {"status": 403, "retryAfter": 0, "times": 2}}));
    mock.clear_requests();
    mock.close_streams();
    // The close and the first refusal each read the capability list again;
    // the second attempt follows a backoff of most of a second.
    wait("the first refusal's read", Duration::from_secs(10), &|| {
        streams(&mock) >= 1 && mock.count("GET", "/desktop/capabilities") >= 2
    });
    // Access comes back with a changed list, which the second refusal's
    // own read finds just as the stream turns off.
    mock.capabilities(&org, &["updates", "attachments"]);
    wait("a live stream again", Duration::from_secs(15), &|| {
        hub.snapshot().live() == LiveStatus::Live
    });
    hub.stop();
}

#[test]
fn a_count_already_out_when_access_went_does_not_come_back() {
    let mock = NettMock::with_args(&["updates"], &["--keepalive-seconds", "1"]);
    let (org, hub) = works_hub(&mock);
    wait("the list and its counts", Duration::from_secs(10), &|| {
        let snapshot = hub.snapshot();
        snapshot.live() == LiveStatus::Live
            && snapshot.list().is_some()
            && snapshot.my_issues_count().is_some()
    });
    wait_for_ready_catch_up(&hub);
    // The list is read again; it and the two counts after it are slow.
    // Access goes once NETT has answered the last count, while its body is
    // still on the way, and the stream's catch-up finds the list refused.
    let groups = format!("/api/v1/orgs/{org}/issues/groups");
    mock.config(&json!({"slowBody": {"path": groups, "seconds": 2.0, "times": 3}}));
    let before = mock.count("GET", "/issues/groups");
    hub.reload(ListQuery::new(ListTab::All).project(Some("OPS")))
        .unwrap();
    wait("the counts out", Duration::from_secs(15), &|| {
        mock.count("GET", "/issues/groups") >= before + 3
    });
    mock.revoke_member(&org, "ada");
    mock.close_streams();
    wait("the list cleared", Duration::from_secs(10), &|| {
        hub.snapshot().list().is_none()
    });
    // Every slow answer has landed by now.
    std::thread::sleep(Duration::from_secs(3));
    let snapshot = hub.snapshot();
    assert!(snapshot.list().is_none());
    assert_eq!(snapshot.my_issues_count(), None, "the old count stays gone");
    assert_eq!(snapshot.hidden_subissues_count(), None);
    hub.stop();
}

#[test]
fn an_attachment_list_already_out_when_access_went_does_not_come_back() {
    let mock = NettMock::with_args(&["updates", "attachments"], &["--keepalive-seconds", "1"]);
    let (org, hub) = works_hub(&mock);
    open_live_issue(&hub);
    let list = format!("/api/v1/orgs/{org}/issues/OPS-1/attachments");
    mock.config(&json!({"slowBody": {"path": list, "seconds": 3.0, "times": 1}}));
    hub.load_attachments().unwrap();
    wait("the attachment list out", Duration::from_secs(10), &|| {
        mock.count("GET", "/issues/OPS-1/attachments") >= 1
    });
    mock.revoke_member(&org, "ada");
    mock.close_streams();
    wait("the issue retired", Duration::from_secs(10), &|| {
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.detail().is_none())
    });
    std::thread::sleep(Duration::from_secs(4));
    assert!(
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.files().items().is_empty()),
        "the late list was not shown on the retired issue"
    );
    hub.stop();
}
