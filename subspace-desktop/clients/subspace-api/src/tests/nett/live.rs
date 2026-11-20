//! The Issues hub against the mock's update stream.

mod access;
mod backoff;
mod issue;
mod updates;

use super::{TOKEN, organisation, server::NettMock, wait_for};
use crate::hub::{Hub, LiveStatus};
use crate::{Client, ListQuery, ListTab};
use subspace_framework::core::Secret;

const STREAM: &str = "/api/v1/updates";

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

fn works_hub(mock: &NettMock) -> (String, Hub) {
    let org = organisation(mock, "Fixture Works");
    let hub = hub(mock, &org);
    (org, hub)
}

fn wait_for_live_list(hub: &Hub) {
    wait_for("a live stream", &|| {
        hub.snapshot().list().is_some() && hub.snapshot().live() == LiveStatus::Live
    });
}

fn open_live_issue(hub: &Hub) {
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait_for("a live stream and the issue", &|| {
        let snapshot = hub.snapshot();
        snapshot.live() == LiveStatus::Live
            && snapshot
                .issue()
                .is_some_and(|issue| issue.detail().is_some())
    });
}
