//! The desktop issue feed against loopback HTTP: authentication, event
//! framing, reconnection, project scope and the reads that hints trigger.
mod catalogue;
mod during_load;
mod logging;
mod refresh;
mod scope;
mod server;

use super::{detail, row, wait};
use crate::{
    Client, ListQuery,
    hub::{Hub, HubStatus, LiveStatus},
    live::{self, LiveError, Record},
};
use serde_json::json;
use server::{Peer, Seen, Server, route};
use std::{
    collections::BTreeSet,
    sync::{
        Arc,
        atomic::{AtomicI64, AtomicUsize, Ordering},
    },
    time::Duration,
};
use subspace_framework::core::Secret;

const JWT: &str = "fixture-access";
const UPDATES: &str = "/api/v1/updates";
const DETAIL: &str = "/api/v1/issues/SUB-1";
const GROUPS: &str = "/api/v1/issues/groups";
const PULL_REQUESTS: &str = "/api/v1/issues/SUB-1/pull-requests";

fn sse(_: &Seen) -> (u16, String) {
    (200, String::new())
}

fn client(server: &Server) -> Client {
    Client::loopback(&format!("{}/api", server.base), Some(Secret::new(JWT))).unwrap()
}

async fn connect(server: &Server) -> Result<crate::live::EventStream, LiveError> {
    let client = client(server);
    let http = live::http_client()?;
    live::connect(&http, &client, &BTreeSet::from(["SUB".into()])).await
}

/// REST answers like `common`, with a detail version the test can move.
fn api(version: Arc<AtomicI64>) -> impl Fn(&Seen) -> (u16, String) + Send + Sync {
    move |seen| {
        let path = seen.path.split('?').next().unwrap();
        match path {
            UPDATES => sse(seen),
            DETAIL => {
                let mut issue = detail("1");
                issue["version"] = version.load(Ordering::SeqCst).into();
                (200, issue.to_string())
            }
            GROUPS => {
                let mut first = row("1");
                first["version"] = version.load(Ordering::SeqCst).into();
                (
                    200,
                    json!({"groupBy":"state","groups":[{"stateId":"state-1","stateType":"started",
                        "count":1,"items":[first],"nextCursor":null}]})
                    .to_string(),
                )
            }
            _ => super::common_path(&seen.path),
        }
    }
}

fn hub(server: &Server) -> Hub {
    let hub = Hub::from_client(client(server), "Subspace API");
    wait(&hub, |snapshot| snapshot.status() == &HubStatus::Ready);
    hub.activate(ListQuery::default().project(Some("SUB")))
        .unwrap();
    hub.open_issue("SUB-1", crate::PROFILE_ID).unwrap();
    wait(&hub, |snapshot| {
        snapshot.list().is_some()
            && snapshot
                .issue()
                .is_some_and(|issue| issue.detail().is_some())
    });
    hub
}

/// Sends `ready` and waits until the hub has caught up after it. A few
/// tenths of a second after `ready` the hub reads the open issue and the
/// list head again, and once the issue read is back it reads the issue's
/// pull requests and starts its media lists again. A test that acts before
/// that has landed races it, and a busy machine delays it further, so wait
/// for those reads rather than a fixed time.
fn ready_and_caught_up(server: &Server, hub: &Hub, peer: &mut Peer) {
    let reads = [DETAIL, GROUPS, PULL_REQUESTS];
    let before = reads.map(|path| server.count("GET", path));
    peer.ready();
    wait(hub, |snapshot| snapshot.live() == LiveStatus::Live);
    wait(hub, |_| {
        reads
            .iter()
            .zip(before)
            .all(|(path, seen)| server.count("GET", path) > seen)
    });
}

fn version(hub: &Hub) -> i64 {
    hub.snapshot()
        .issue()
        .unwrap()
        .detail()
        .unwrap()
        .fields
        .version
}

#[tokio::test]
async fn stream_request_carries_project_scope_and_the_loopback_assertion() {
    let server = Server::new(sse);
    let _stream = connect(&server).await.unwrap();
    let peer = server.peer();
    assert_eq!(
        (peer.seen.method.as_str(), peer.seen.path.as_str()),
        ("GET", "/api/v1/updates?project=SUB")
    );
    assert_eq!(peer.seen.header("accept"), Some("text/event-stream"));
    assert_eq!(
        peer.seen.header("authorization"),
        Some(format!("Bearer {JWT}").as_str())
    );
    assert!(peer.seen.header("upgrade").is_none());
}

#[tokio::test]
async fn redirects_never_forward_the_assertion() {
    let destination = Server::new(sse);
    let redirect = format!("{}{}", destination.base, UPDATES);
    let origin = Server::new(move |_| (302, redirect.clone()));
    assert!(connect(&origin).await.is_err());
    assert_eq!(origin.count("GET", UPDATES), 1);
    assert_eq!(destination.count("GET", UPDATES), 0);
}

#[tokio::test]
async fn missing_refused_and_invalid_streams_are_classified() {
    let missing = Server::new(|_| (404, "{}".into()));
    assert_eq!(connect(&missing).await.err(), Some(LiveError::Unavailable));
    let denied = Server::new(|_| (403, "{}".into()));
    assert_eq!(connect(&denied).await.err(), Some(LiveError::Refused));
    let json = Server::new(|_| (200, "{}".into()));
    assert_eq!(connect(&json).await.err(), Some(LiveError::Refused));
}

#[tokio::test]
async fn reads_fragmented_events_and_multiple_events_in_one_chunk() {
    let server = Server::new(sse);
    let mut stream = connect(&server).await.unwrap();
    let mut peer = server.peer();
    peer.chunk(b"event: ready\r");
    peer.chunk(b"\ndata: null\r\n\r\nevent: heartbeat\ndata: null\n\nevent: issue");
    peer.chunk(
        format!(
            "Changed\ndata: {}\n\n",
            server::payload("SUB-1", 2, "issue.changed")
        )
        .as_bytes(),
    );
    peer.close();
    assert!(matches!(stream.next().await, Ok(Record::Ready)));
    assert!(matches!(stream.next().await, Ok(Record::Other)));
    assert!(matches!(stream.next().await, Ok(Record::Changed(hint)) if hint.version == 2));
    assert_eq!(stream.next().await.err(), Some(LiveError::Closed));
}

#[tokio::test]
async fn malformed_hint_ends_the_stream() {
    let server = Server::new(sse);
    let mut stream = connect(&server).await.unwrap();
    let mut peer = server.peer();
    peer.ready();
    peer.chunk(b"event: issueChanged\ndata: {broken}\n\n");
    assert!(matches!(stream.next().await, Ok(Record::Ready)));
    assert_eq!(stream.next().await.err(), Some(LiveError::Protocol));
}

#[tokio::test]
async fn an_oversized_event_is_rejected_before_blocking_on_more_bytes() {
    let server = Server::new(sse);
    let mut stream = connect(&server).await.unwrap();
    let mut peer = server.peer();
    const OVERSIZED_COMMENT: usize = 65 * 1024;
    let mut data = vec![b'x'; OVERSIZED_COMMENT];
    data[0] = b':';
    peer.chunk(&data);
    assert_eq!(stream.next().await.err(), Some(LiveError::Protocol));
}

#[test]
fn ready_stream_resyncs_and_reads_again_after_a_hint() {
    let current = Arc::new(AtomicI64::new(1));
    let server = Server::new(api(current.clone()));
    let hub = hub(&server);
    let mut peer = server.peer();
    ready_and_caught_up(&server, &hub, &mut peer);
    let details = server.count("GET", DETAIL);
    // Collaboration writes keep the version: a hint still reads once.
    peer.changed("SUB-1", 1);
    wait(&hub, |_| server.count("GET", DETAIL) == details + 1);
    std::thread::sleep(Duration::from_millis(700));
    let settled = server.count("GET", DETAIL);
    assert_eq!(settled, details + 1, "one read for one hint");
    current.store(5, Ordering::SeqCst);
    for _ in 0..4 {
        peer.changed("SUB-1", 5);
    }
    wait(&hub, |_| version(&hub) == 5);
    std::thread::sleep(Duration::from_millis(400));
    // The burst coalesced into one read of the detail.
    assert_eq!(server.count("GET", DETAIL), settled + 1);
    let snapshot = wait(&hub, |snapshot| {
        snapshot.list().unwrap().groups[0].items[0].fields.version == 5
    });
    assert!(snapshot.issue().unwrap().timeline().page().is_some());
    hub.stop();
}

#[test]
fn hub_reconnects_with_the_same_scope_after_a_drop() {
    let server = Server::new(api(Arc::new(AtomicI64::new(1))));
    let hub = hub(&server);
    let mut peer = server.peer();
    peer.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    let project = peer.seen.path.clone();
    peer.close();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Reconnecting);
    let mut again = server.peer();
    assert_eq!(again.seen.path, project);
    again.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    assert!(server.count("GET", UPDATES) >= 2);
    hub.stop();
}

#[test]
fn without_the_stream_the_open_issue_is_polled() {
    let reads = Arc::new(AtomicUsize::new(0));
    let counter = reads.clone();
    let server = Server::new(move |seen| {
        if route(&seen.path) == UPDATES {
            return (404, "{}".into());
        }
        if seen.path.starts_with(DETAIL) {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        api(Arc::new(AtomicI64::new(1)))(seen)
    });
    let hub = hub(&server);
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Polling);
    let before = reads.load(Ordering::SeqCst);
    // The hub keeps trying in the background.
    wait(&hub, |_| server.count("GET", UPDATES) >= 2);
    assert!(
        reads.load(Ordering::SeqCst) > before,
        "the open issue was not polled"
    );
    hub.stop();
}
