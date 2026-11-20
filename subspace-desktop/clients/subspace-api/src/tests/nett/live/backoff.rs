use super::super::{server::NettMock, wait};
use super::{STREAM, arrivals, streams, works_hub};
use crate::hub::LiveStatus;
use crate::{ListQuery, ListTab};
use serde_json::json;
use std::time::Duration;

#[test]
fn a_busy_stream_waits_at_least_the_servers_retry_after() {
    let mock = NettMock::serving(&["updates"]);
    mock.config(&json!({"streamRefusal": {"status": 429, "retryAfter": 2, "times": 1}}));
    let (_, hub) = works_hub(&mock);
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
fn a_servers_retry_after_holds_when_the_followed_teams_change() {
    let mock = NettMock::serving(&["updates"]);
    mock.config(&json!({"streamRefusal": {"status": 429, "retryAfter": 3, "times": 1}}));
    let (_, hub) = works_hub(&mock);
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
    let (_, hub) = works_hub(&mock);
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
