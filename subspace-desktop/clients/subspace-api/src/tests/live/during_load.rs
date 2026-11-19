//! A normal load can capture older data before a stream requests a refresh.
use super::{DETAIL, GROUPS, api, client, hub, server::route};
use crate::{
    ListQuery,
    hub::{Hub, IssueStatus, LiveStatus},
    tests::wait,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicI64, Ordering},
        mpsc::{Receiver, Sender, channel},
    },
    time::Duration,
};

fn hold_first_response() -> (
    impl Fn((u16, String)) -> (u16, String) + Send + Sync,
    Receiver<()>,
    Sender<()>,
) {
    let (holding, captured) = channel();
    let (release, held) = channel();
    let held = Mutex::new(Some(held));
    let serve = move |response| {
        let next = held.lock().unwrap().take();
        if let Some(held) = next {
            holding.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(7)).unwrap();
        }
        response
    };
    (serve, captured, release)
}

fn issue_is(snapshot: &crate::hub::HubSnapshot, version: i64) -> bool {
    snapshot
        .issue()
        .and_then(|issue| issue.detail())
        .is_some_and(|detail| detail.fields.version == version)
}

fn list_is(snapshot: &crate::hub::HubSnapshot, version: i64) -> bool {
    snapshot
        .list()
        .and_then(|list| list.groups.first())
        .and_then(|group| group.items.first())
        .is_some_and(|row| row.fields.version == version)
}

#[test]
fn ready_owes_an_issue_read_when_the_initial_detail_is_still_loading() {
    let version = Arc::new(AtomicI64::new(1));
    let api = api(version.clone());
    let (hold, captured, release) = hold_first_response();
    let server = super::Server::new(move |seen| {
        let response = api(seen);
        if route(&seen.path) == DETAIL {
            hold(response)
        } else {
            response
        }
    });
    let hub = Hub::from_client(client(&server), "Subspace API");
    hub.activate(ListQuery::default().project(Some("SUB")))
        .unwrap();
    hub.open_issue("SUB-1", crate::PROFILE_ID).unwrap();
    wait(&hub, |snapshot| list_is(snapshot, 1));
    captured.recv_timeout(Duration::from_secs(5)).unwrap();
    let mut peer = server.peer();

    version.store(2, Ordering::SeqCst);
    peer.ready();
    // The list updating proves the ready catch-up tick already ran.
    let snapshot = wait(&hub, |snapshot| list_is(snapshot, 2));
    assert_eq!(snapshot.issue().unwrap().status(), &IssueStatus::Loading);
    release.send(()).unwrap();
    wait(&hub, |snapshot| issue_is(snapshot, 2));

    assert_eq!(server.count("GET", DETAIL), 2);
    hub.stop();
}

#[test]
fn a_hint_owes_a_list_read_while_a_replacement_query_is_loading() {
    let version = Arc::new(AtomicI64::new(1));
    let api = api(version.clone());
    let (hold, captured, release) = hold_first_response();
    let server = super::Server::new(move |seen| {
        let response = api(seen);
        if route(&seen.path) == GROUPS
            && seen.path.contains("q=held")
            && seen.path.contains("limitPerGroup=25")
        {
            hold(response)
        } else {
            response
        }
    });
    let hub = hub(&server);
    let mut peer = server.peer();
    version.store(2, Ordering::SeqCst);
    peer.ready();
    wait(&hub, |snapshot| {
        snapshot.live() == LiveStatus::Live && issue_is(snapshot, 2) && list_is(snapshot, 2)
    });

    hub.activate(ListQuery::default().project(Some("SUB")).search("held"))
        .unwrap();
    captured.recv_timeout(Duration::from_secs(5)).unwrap();
    version.store(3, Ordering::SeqCst);
    peer.changed("SUB-1", 3);
    // The detail updating proves the live hint's refresh tick already ran.
    let snapshot = wait(&hub, |snapshot| issue_is(snapshot, 3));
    assert!(snapshot.list_loading());
    release.send(()).unwrap();
    wait(&hub, |snapshot| list_is(snapshot, 3));
    assert_eq!(server.count("GET", super::UPDATES), 1);
    hub.stop();
}
