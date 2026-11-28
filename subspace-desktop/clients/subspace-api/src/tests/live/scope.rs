//! The stream confirms scope with a ready event. While it connects or
//! reconnects, polling keeps the open issue current.
use super::{
    DETAIL, GROUPS, UPDATES, api, hub,
    server::{Server, route},
};
use crate::{ListQuery, hub::LiveStatus, tests::wait};
use std::{
    sync::{
        Arc,
        atomic::{AtomicI64, Ordering},
    },
    time::{Duration, Instant},
};

fn served() -> Server {
    Server::new(api(Arc::new(AtomicI64::new(1))))
}

#[test]
fn a_live_list_refresh_invalidates_matches_found_for_the_previous_installation() {
    let rest = api(Arc::new(AtomicI64::new(1)));
    let server = Server::new(move |seen| {
        if route(&seen.path) == GROUPS && seen.path.contains("stateType=") {
            crate::tests::common_path(&seen.path)
        } else {
            rest(seen)
        }
    });
    let hub = hub(&server);
    let mut peer = server.peer();
    super::ready_and_caught_up(&hub, &mut peer);
    let wider = ListQuery::new(crate::ListTab::All)
        .project(Some("SUB"))
        .search("fixture");
    let scoped = ListQuery::new(crate::ListTab::Backlog)
        .project(Some("SUB"))
        .search("fixture");
    hub.activate(scoped.clone()).unwrap();
    let checked = wait(&hub, |snapshot| {
        snapshot.list().is_some_and(|list| list.groups.is_empty())
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let found = runtime
        .block_on(hub.find_wider_matches(&scoped, &wider))
        .unwrap();
    peer.hint("SUB-1", 1, "issue.changed");
    let refreshed = wait(&hub, |snapshot| snapshot.list_id() != checked.list_id());
    assert_eq!(refreshed.list_revision(), checked.list_revision());
    assert!(!hub.adopt_wider_matches(found));
    hub.stop();
}

#[test]
fn catch_up_waits_for_ready() {
    let server = served();
    let hub = hub(&server);
    let mut peer = server.peer();
    // The list shows before its count reads, which follow it, are back.
    wait(&hub, |snapshot| snapshot.my_issues_count().is_some());
    let lists = server.count("GET", GROUPS);
    std::thread::sleep(Duration::from_millis(800));
    assert_eq!(hub.snapshot().live(), LiveStatus::Connecting);
    // Polling reads only the open issue. The list head is read on catch-up.
    assert_eq!(
        server.count("GET", GROUPS),
        lists,
        "caught up before the stream was ready"
    );
    let details = server.count("GET", DETAIL);
    peer.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    wait(&hub, |_| {
        server.count("GET", DETAIL) > details && server.count("GET", GROUPS) > lists
    });
    hub.stop();
}

#[test]
fn a_project_scope_change_keeps_the_open_issues_project_and_waits_for_ready() {
    let server = served();
    let hub = hub(&server);
    let mut first = server.peer();
    assert_eq!(first.seen.path, "/api/v1/updates?project=SUB");
    first.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);

    hub.activate(ListQuery::default().project(Some("OPS")))
        .unwrap();
    let mut next = server.peer();
    assert_eq!(next.seen.path, "/api/v1/updates?project=OPS&project=SUB");
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Connecting);
    polled_while(&server, &hub);
    let details = server.count("GET", DETAIL);
    next.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    wait(&hub, |_| server.count("GET", DETAIL) > details);
    hub.stop();
}

#[test]
fn an_empty_initiative_releases_the_list_scope_and_keeps_the_open_issue_live() {
    let server = served();
    let hub = hub(&server);
    let mut first = server.peer();
    first.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);

    hub.activate(ListQuery::default().project(Some("OPS")))
        .unwrap();
    let scoped = server.peer();
    assert_eq!(scoped.seen.path, "/api/v1/updates?project=OPS&project=SUB");

    hub.clear_project_scope();
    let next = server.peer();
    assert_eq!(next.seen.path, "/api/v1/updates?project=SUB");
    assert!(hub.snapshot().list().is_none());
    assert!(hub.snapshot().list_project().is_none());
    hub.stop();
}

#[test]
fn a_verified_account_change_replaces_a_stream_with_the_same_project_scope() {
    let actor = Arc::new(AtomicI64::new(1));
    let current_actor = actor.clone();
    let rest = api(Arc::new(AtomicI64::new(1)));
    let server = Server::new(move |seen| {
        if route(&seen.path) == "/api/v1/me" {
            let id = current_actor.load(Ordering::SeqCst).to_string();
            return (
                200,
                serde_json::json!({"userId":id,"displayName":"Fixture","kind":"human"}).to_string(),
            );
        }
        rest(seen)
    });
    let hub = hub(&server);
    let mut first = server.peer();
    assert_eq!(first.seen.path, "/api/v1/updates?project=SUB");
    first.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);

    actor.store(2, Ordering::SeqCst);
    hub.reload_projects().unwrap();
    let mut next = server.peer();
    assert_eq!(next.seen.path, first.seen.path);
    assert_eq!(hub.snapshot().me().unwrap().user_id, "2");
    next.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    hub.stop();
}

fn polled_while(server: &Server, hub: &crate::hub::Hub) {
    let details = server.count("GET", DETAIL);
    wait(hub, |_| server.count("GET", DETAIL) >= details + 2);
    assert_ne!(hub.snapshot().live(), LiveStatus::Live);
}

#[test]
fn waiting_for_response_headers_still_polls_the_open_issue() {
    let rest = api(Arc::new(AtomicI64::new(1)));
    let server = Server::new(move |seen| {
        if route(&seen.path) == UPDATES {
            std::thread::sleep(Duration::from_secs(4));
        }
        rest(seen)
    });
    let hub = hub(&server);
    polled_while(&server, &hub);
    hub.stop();
}

#[test]
fn waiting_for_ready_still_polls_the_open_issue() {
    let server = served();
    let hub = hub(&server);
    let _peer = server.peer();
    assert_eq!(hub.snapshot().live(), LiveStatus::Connecting);
    polled_while(&server, &hub);
    hub.stop();
}

#[test]
fn a_refused_stream_still_polls_and_recovers() {
    let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = attempts.clone();
    let rest = api(Arc::new(AtomicI64::new(1)));
    let server = Server::new(move |seen| {
        if route(&seen.path) == UPDATES && counted.fetch_add(1, Ordering::SeqCst) == 0 {
            return (403, "{}".into());
        }
        rest(seen)
    });
    let hub = hub(&server);
    let mut next = server.peer();
    polled_while(&server, &hub);
    next.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    assert!(attempts.load(Ordering::SeqCst) >= 2);
    hub.stop();
}

#[test]
fn an_unreachable_stream_still_polls_the_open_issue() {
    let rest = api(Arc::new(AtomicI64::new(1)));
    let server = Server::new(move |seen| {
        if route(&seen.path) == UPDATES {
            return (0, String::new());
        }
        rest(seen)
    });
    let hub = hub(&server);
    polled_while(&server, &hub);
    hub.stop();
}

#[test]
fn a_flapping_stream_keeps_backing_off() {
    let server = served();
    let hub = hub(&server);
    let mut gaps = Vec::new();
    let mut peer = server.peer();
    for _ in 0..2 {
        peer.ready();
        wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
        peer.close();
        let dropped = Instant::now();
        peer = server.peer();
        gaps.push(dropped.elapsed());
    }
    // The first delay is at most 1.25 s. The second starts at 1.5 s.
    assert!(
        gaps[1] >= Duration::from_millis(1400),
        "backoff reset on a short connection: {gaps:?}"
    );
    hub.stop();
}

#[test]
fn archived_projects_stay_visible_without_blocking_active_project_streams() {
    use crate::tests::{page, row};
    use serde_json::json;

    let mut archived = row("2");
    archived["projectKey"] = "OLD".into();
    archived["identifier"] = "OLD-2".into();
    let groups = json!({"groupBy":"stateType","groups":[{
        "stateId":null, "stateType":"started", "count":2,
        "items":[row("1"), archived], "nextCursor":null,
    }]})
    .to_string();
    let server = Server::new(move |seen| match route(&seen.path) {
        UPDATES => super::sse(seen),
        GROUPS => (200, groups.clone()),
        "/api/v1/projects" => (
            200,
            page(
                vec![
                    json!({"id":"p1","key":"SUB","name":"Active","defaultStateId":"state-1"}),
                    json!({"id":"p2","key":"OLD","name":"Archived","defaultStateId":"state-2",
                        "archivedAt":"2026-11-18T12:00:00Z"}),
                ],
                None,
            ),
        ),
        _ => crate::tests::common_path(&seen.path),
    });
    let hub = crate::hub::Hub::from_client(super::client(&server), "Subspace API");
    hub.activate(ListQuery::default()).unwrap();
    let snapshot = wait(&hub, |snapshot| snapshot.list().is_some());
    assert_eq!(snapshot.list().unwrap().groups[0].items.len(), 2);
    let mut peer = server.peer();
    assert_eq!(peer.seen.path, "/api/v1/updates?project=SUB");
    peer.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    hub.stop();
}

#[test]
fn all_projects_receives_the_first_matching_issue() {
    let version = Arc::new(AtomicI64::new(0));
    let current = version.clone();
    let rest = api(version.clone());
    let server = Server::new(move |seen| {
        if route(&seen.path) == GROUPS && current.load(Ordering::SeqCst) == 0 {
            crate::tests::common_path(&seen.path)
        } else {
            rest(seen)
        }
    });
    let hub = crate::hub::Hub::from_client(super::client(&server), "Subspace API");
    hub.activate(ListQuery::default()).unwrap();
    let initial = wait(&hub, |snapshot| snapshot.list().is_some());
    assert!(initial.list().unwrap().groups.is_empty());
    let mut peer = server.peer();
    assert_eq!(peer.seen.path, "/api/v1/updates?project=SUB");
    peer.ready();
    wait(&hub, |snapshot| {
        snapshot.live() == LiveStatus::Live && snapshot.list_id() != initial.list_id()
    });

    version.store(1, Ordering::SeqCst);
    peer.hint("SUB-1", 1, "issue.created");
    let snapshot = wait(&hub, |snapshot| {
        snapshot.list().is_some_and(|list| !list.groups.is_empty())
    });
    assert_eq!(
        snapshot.list().unwrap().groups[0].items[0]
            .fields
            .identifier,
        "SUB-1"
    );
    hub.stop();
}
