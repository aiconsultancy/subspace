use super::super::{TOKEN, organisation, server::NettMock, wait, wait_for};
use super::{STREAM, arrivals, open_live_issue, streams, wait_for_live_list, works_hub};
use crate::hub::{Hub, LiveStatus};
use crate::{Client, ListQuery, ListTab};
use serde_json::json;
use std::time::Duration;
use subspace_framework::core::Secret;

#[test]
fn an_organisation_without_updates_neither_streams_nor_polls() {
    let mock = NettMock::start();
    let (_, hub) = works_hub(&mock);
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
fn a_capability_read_elsewhere_that_enables_updates_turns_the_stream_on() {
    let mock = NettMock::start();
    let (org, hub) = works_hub(&mock);
    wait("the list and no stream", Duration::from_secs(10), &|| {
        let snapshot = hub.snapshot();
        snapshot.list().is_some() && snapshot.live() == LiveStatus::Off
    });
    let epoch = hub.snapshot().scope_epoch();
    // NETT enables updates, and another read of the same account, as an
    // explicit reload's first team read may, finds that first: the list is
    // current again before the stream sees it due.
    mock.capabilities(&org, &["updates"]);
    let elsewhere =
        Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(TOKEN))).unwrap();
    assert_eq!(elsewhere.nett_refresh_served(), Ok(true));
    assert!(elsewhere.nett_served_current());
    wait("a live stream", Duration::from_secs(10), &|| {
        hub.snapshot().live() == LiveStatus::Live
    });
    assert_ne!(
        hub.snapshot().scope_epoch(),
        epoch,
        "the Spaces read their initiatives again"
    );
    hub.stop();
}

#[test]
fn a_server_close_catches_up_through_rest_and_reads_capabilities_again() {
    let mock = NettMock::serving(&["updates"]);
    let (_, hub) = works_hub(&mock);
    wait_for_live_list(&mock, &hub);
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
    let (org, hub) = works_hub(&mock);
    wait_for_live_list(&mock, &hub);
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
fn a_broken_stream_catches_up_through_rest_before_it_reconnects() {
    let mock = NettMock::serving(&["updates"]);
    let (org, hub) = works_hub(&mock);
    open_live_issue(&mock, &hub);
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
fn the_hub_follows_changes_made_elsewhere_over_the_update_stream() {
    let mock = NettMock::serving(&["updates"]);
    let org = organisation(&mock, "Fixture Works");
    let client = Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(TOKEN))).unwrap();
    // Give the initial list an activity timestamp from before the stream opens.
    mock.touch(&org, "OPS-1");
    let hub = Hub::from_client(client, "NETT");
    hub.activate(ListQuery::new(ListTab::All).project(Some("OPS")))
        .unwrap();
    wait_for("the first list, its counts and a live stream", &|| {
        let snapshot = hub.snapshot();
        snapshot.list().is_some()
            && snapshot.hidden_subissues_count().is_some()
            && snapshot.my_issues_count().is_some()
            && snapshot.live() == LiveStatus::Live
    });
    let streams: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|request| request["path"] == "/api/v1/updates")
        .collect();
    assert!(!streams.is_empty());
    assert!(
        streams
            .iter()
            .all(|request| request["authorization"] == true
                && request["orgHeader"] == serde_json::Value::String(org.clone())
                && request["query"]["project"] == serde_json::json!(["OPS"]))
    );
    let expected = touch_activity(&mock, &org, "OPS-1");
    wait_for("the touched issue's new activity in the list", &|| {
        hub.snapshot().list().is_some_and(|list| {
            list.groups
                .iter()
                .flat_map(|group| &group.items)
                .any(|row| {
                    row.fields.identifier == "OPS-1" && row.fields.last_activity_at == expected
                })
        })
    });
    hub.stop();
}

fn touch_activity(mock: &NettMock, org: &str, issue: &str) -> String {
    let agent: ureq::Agent = ureq::Agent::config_builder().proxy(None).build().into();
    let api = mock.api();
    let origin = api.strip_suffix("/api").unwrap();
    let mut response = agent
        .post(format!("{origin}/__fixture/touch"))
        .header("Content-Type", "application/json")
        .send(serde_json::json!({"org": org, "issue": issue}).to_string())
        .unwrap();
    let reply: serde_json::Value =
        serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap();
    reply["lastActivityAt"].as_str().unwrap().to_owned()
}
