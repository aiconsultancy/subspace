//! The Issues hub against the mock's update stream and to-do lists: gating
//! on the organisation's `updates`, Retry-After, a 404 that stops the
//! stream, REST catch-up after a server close, and versioned to-dos.

use super::server::NettMock;
use super::{TOKEN, organisation, works};
use crate::hub::{Hub, LiveStatus};
use crate::{Client, ListQuery, ListTab};
use serde_json::json;
use std::time::{Duration, Instant};
use subspace_framework::core::Secret;

const STREAM: &str = "/api/v1/updates";

fn wait(what: &str, limit: Duration, done: &dyn Fn() -> bool) {
    let deadline = Instant::now() + limit;
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn hub(mock: &NettMock, org: &str) -> Hub {
    let client = Client::nett_loopback(&mock.api(), Some(org), Some(Secret::new(TOKEN))).unwrap();
    let hub = Hub::from_client(client, "NETT");
    hub.activate(ListQuery::new(ListTab::All).project(Some("OPS")))
        .unwrap();
    hub
}

fn streams(mock: &NettMock) -> usize {
    mock.count("GET", STREAM)
}

#[test]
fn an_organisation_without_updates_neither_streams_nor_polls() {
    let mock = NettMock::start();
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait(
        "the list, the issue and no stream",
        Duration::from_secs(10),
        &|| {
            let snapshot = hub.snapshot();
            snapshot.list().is_some()
                && snapshot.live() == LiveStatus::Off
                && snapshot
                    .issue()
                    .is_some_and(|issue| issue.detail().is_some())
        },
    );
    mock.clear_requests();
    // The tests' poll interval is 300 ms: a NETT hub must not use it.
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(streams(&mock), 0);
    assert_eq!(mock.count("GET", "/issues/OPS-1"), 0, "no polling in NETT");
    hub.stop();
}

#[test]
fn a_busy_stream_waits_at_least_the_servers_retry_after() {
    let mock = NettMock::serving(&["updates"]);
    mock.config(&json!({"streamRefusal": {"status": 429, "retryAfter": 2, "times": 1}}));
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    wait("the second attempt", Duration::from_secs(10), &|| {
        streams(&mock) >= 2
    });
    let stream = arrivals(&mock, "GET", STREAM);
    assert!(
        stream[1] - stream[0] >= 1.9,
        "tried again after {:.2} s",
        stream[1] - stream[0]
    );
    wait("a live stream", Duration::from_secs(10), &|| {
        hub.snapshot().live() == LiveStatus::Live
    });
    hub.stop();
}

#[test]
fn a_server_close_catches_up_through_rest_and_reads_capabilities_again() {
    let mock = NettMock::serving(&["updates"]);
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    wait("a live stream", Duration::from_secs(10), &|| {
        hub.snapshot().list().is_some() && hub.snapshot().live() == LiveStatus::Live
    });
    let epoch = hub.snapshot().scope_epoch();
    mock.clear_requests();
    // As on queue overflow or lost authority.
    mock.close_streams();
    wait(
        "the catch-up and a new stream",
        Duration::from_secs(10),
        &|| {
            mock.count("GET", "/issues/groups") >= 1
                && mock.count("GET", "/desktop/capabilities") >= 1
                && streams(&mock) >= 1
        },
    );
    let requests = mock.requests();
    let first = |suffix: &str| {
        requests
            .iter()
            .position(|r| r["path"].as_str().is_some_and(|p| p.ends_with(suffix)))
            .unwrap()
    };
    assert!(
        first("/desktop/capabilities") < first(STREAM),
        "capabilities are read before reconnecting"
    );
    assert_ne!(hub.snapshot().scope_epoch(), epoch);
    wait("live again", Duration::from_secs(10), &|| {
        hub.snapshot().live() == LiveStatus::Live
    });
    hub.stop();
}

#[test]
fn a_404_stops_streaming_for_the_session_and_refreshes_the_feature_list() {
    let mock = NettMock::serving(&["updates"]);
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    wait("a live stream", Duration::from_secs(10), &|| {
        hub.snapshot().list().is_some() && hub.snapshot().live() == LiveStatus::Live
    });
    mock.clear_requests();
    // The followed team goes: the stream closes, and reopening it is 404.
    mock.archive_team(&org, "OPS");
    wait("the refused reconnection", Duration::from_secs(10), &|| {
        hub.snapshot().live() == LiveStatus::Off
    });
    wait("the feature list again", Duration::from_secs(5), &|| {
        mock.count("GET", "/desktop/capabilities") >= 2
    });
    let attempts = streams(&mock);
    // Following other teams does not reopen it: the stream stays off for
    // the rest of the session.
    hub.activate(ListQuery::new(ListTab::All).project(Some("WEB")))
        .unwrap();
    std::thread::sleep(Duration::from_millis(2500));
    assert_eq!(streams(&mock), attempts, "no stream after the 404");
    assert_eq!(hub.snapshot().live(), LiveStatus::Off);
    hub.stop();
}

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
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
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
fn todos_show_named_lists_and_save_checks_at_their_version() {
    let mock = NettMock::start();
    let org = organisation(&mock, "Fixture Works");
    let client = Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(TOKEN)))
        .unwrap()
        .without_live();
    let hub = Hub::from_client(client, "NETT");
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    let lists = |hub: &Hub| {
        hub.snapshot()
            .issue()
            .and_then(|issue| issue.todos().map(<[_]>::to_vec))
    };
    wait("the lists", Duration::from_secs(10), &|| {
        lists(&hub).is_some()
    });
    let shown = lists(&hub).unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].list.name, "Checklist");
    assert_eq!(shown[0].counts(), (1, 3));

    let second = shown[0].items[1].clone();
    hub.set_todo_done(&second.id, true).unwrap();
    wait("the saved check", Duration::from_secs(10), &|| {
        lists(&hub).is_some_and(|lists| lists[0].counts() == (2, 3))
    });

    // Someone else checks the third item; this hub still shows it unchecked
    // at the old version, so its write conflicts and the lists are read again.
    let third = shown[0].items[2].clone();
    works(&mock)
        .set_issue_list_item_done("OPS-1", &third, true)
        .unwrap();
    hub.set_todo_done(&third.id, true).unwrap();
    wait(
        "the conflict and a fresh read",
        Duration::from_secs(10),
        &|| {
            hub.snapshot().issue().is_some_and(|issue| {
                issue.todo_notice().is_some()
                    && issue
                        .todos()
                        .is_some_and(|lists| lists[0].counts() == (3, 3))
            })
        },
    );
    hub.stop();
}

#[test]
fn a_comment_draft_uploads_managed_media_and_its_comment_claims_it() {
    use crate::hub::AttachmentState;
    use crate::uploads::ImageDeclaration;
    use std::sync::Arc;
    let mock = NettMock::start();
    let org = organisation(&mock, "Fixture Works");
    let client = Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(TOKEN)))
        .unwrap()
        .without_live();
    let hub = Hub::from_client(client, "NETT");
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait("the issue", Duration::from_secs(10), &|| {
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.todos().is_some())
    });
    let attach = |name: &str| {
        let bytes: Arc<[u8]> = Arc::from(format!("{name} contents\n").into_bytes());
        let declaration = ImageDeclaration::new(name, "text/plain", &bytes).unwrap();
        let intake = hub
            .begin_comment_image_intake(hub.snapshot().generation(), "OPS-1", None)
            .unwrap();
        intake.attach(declaration, bytes).unwrap()
    };
    let kept = attach("kept.txt");
    let dropped = attach("dropped.txt");
    wait("both uploads", Duration::from_secs(10), &|| {
        hub.comment_draft("OPS-1", None)
            .is_some_and(|draft| draft.attachments_ready())
    });
    assert_eq!(mock.count("PUT", ""), 2, "one byte upload each");
    // Removing one cancels its reservation by its request key.
    hub.remove_comment_image("OPS-1", None, &dropped);
    wait("the removal", Duration::from_secs(10), &|| {
        hub.comment_draft("OPS-1", None)
            .is_some_and(|draft| draft.attachments().len() == 1)
    });
    assert_eq!(mock.count("PUT", "/upload"), 1);
    let draft = hub.comment_draft("OPS-1", None).unwrap();
    assert_eq!(draft.attachments()[0].id(), kept);
    assert_eq!(draft.attachments()[0].state(), AttachmentState::Ready);

    hub.edit_comment("OPS-1", None, "With the file.").unwrap();
    hub.submit_comment("OPS-1", None, false).unwrap();
    wait("the posted comment", Duration::from_secs(10), &|| {
        hub.comment_draft("OPS-1", None)
            .is_some_and(|draft| draft.notice() == Some("Posted."))
    });
    let comments = works(&mock)
        .comments("OPS-1", &crate::PageRequest::default())
        .unwrap();
    let posted = comments
        .items
        .iter()
        .find(|comment| comment.body.starts_with("With the file."))
        .unwrap();
    assert_eq!(posted.media.len(), 1);
    assert_eq!(posted.media[0].file_name, "kept.txt");
    hub.stop();
}

/// When each logged request to `suffix` arrived, in seconds.
fn arrivals(mock: &NettMock, method: &str, suffix: &str) -> Vec<f64> {
    mock.requests()
        .iter()
        .filter(|r| {
            r["method"] == method && r["path"].as_str().is_some_and(|p| p.ends_with(suffix))
        })
        .filter_map(|r| r["at"].as_f64())
        .collect()
}

#[test]
fn a_lasting_403_on_the_stream_stops_it_instead_of_polling() {
    let mock = NettMock::serving(&["updates"]);
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    wait("a live stream", Duration::from_secs(10), &|| {
        hub.snapshot().list().is_some() && hub.snapshot().live() == LiveStatus::Live
    });
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
fn a_broken_stream_catches_up_through_rest_before_it_reconnects() {
    let mock = NettMock::serving(&["updates"]);
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait(
        "a live stream and the issue",
        Duration::from_secs(10),
        &|| {
            let snapshot = hub.snapshot();
            snapshot.live() == LiveStatus::Live
                && snapshot
                    .issue()
                    .is_some_and(|issue| issue.detail().is_some())
        },
    );
    // The catch-up's issue read is slow: the stream must wait for it.
    mock.config(
        &json!({"slowBody": {"path": format!("/api/v1/orgs/{org}/issues/OPS-1"),
                                     "seconds": 1.5, "times": 1}}),
    );
    mock.clear_requests();
    mock.post_hook("updates/close", &json!({"abort": true}));
    wait("a new stream", Duration::from_secs(15), &|| {
        streams(&mock) >= 1
    });
    let issue = arrivals(&mock, "GET", "/issues/OPS-1");
    let stream = arrivals(&mock, "GET", STREAM);
    assert!(!issue.is_empty(), "the open issue was read again");
    assert!(
        stream[0] - issue[0] >= 1.4,
        "reconnected {:.2} s after the slow read began",
        stream[0] - issue[0]
    );
    hub.stop();
}

#[test]
fn a_servers_retry_after_holds_when_the_followed_teams_change() {
    let mock = NettMock::serving(&["updates"]);
    mock.config(&json!({"streamRefusal": {"status": 429, "retryAfter": 3, "times": 1}}));
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    wait("the refused stream", Duration::from_secs(10), &|| {
        streams(&mock) >= 1
    });
    // Following another team right away must not cut the wait short.
    std::thread::sleep(Duration::from_millis(300));
    hub.activate(ListQuery::new(ListTab::All).project(Some("WEB")))
        .unwrap();
    wait("the next attempt", Duration::from_secs(15), &|| {
        streams(&mock) >= 2
    });
    let stream = arrivals(&mock, "GET", STREAM);
    assert!(
        stream[1] - stream[0] >= 2.9,
        "tried again after {:.2} s",
        stream[1] - stream[0]
    );
    hub.stop();
}

#[test]
fn a_servers_longest_wait_holds_when_the_followed_teams_change() {
    let mock = NettMock::serving(&["updates"]);
    mock.config(&json!({"streamRefusal": {"status": 429, "retryAfter": u64::MAX, "times": 1}}));
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    wait("the refused stream", Duration::from_secs(10), &|| {
        streams(&mock) >= 1
    });
    std::thread::sleep(Duration::from_millis(300));
    hub.activate(ListQuery::new(ListTab::All).project(Some("WEB")))
        .unwrap();
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(streams(&mock), 1, "the wait still holds");
    hub.stop();
}

#[test]
fn a_count_already_out_when_access_went_does_not_come_back() {
    let mock = NettMock::with_args(&["updates"], &["--keepalive-seconds", "1"]);
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    wait("the list and its counts", Duration::from_secs(10), &|| {
        let snapshot = hub.snapshot();
        snapshot.live() == LiveStatus::Live
            && snapshot.list().is_some()
            && snapshot.my_issues_count().is_some()
    });
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
    let org = organisation(&mock, "Fixture Works");
    let hub = hub(&mock, &org);
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait(
        "a live stream and the issue",
        Duration::from_secs(10),
        &|| {
            let snapshot = hub.snapshot();
            snapshot.live() == LiveStatus::Live
                && snapshot
                    .issue()
                    .is_some_and(|issue| issue.detail().is_some())
        },
    );
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
