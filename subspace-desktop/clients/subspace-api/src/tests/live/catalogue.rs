//! A stale subscription must recover after another client archives a project.
use super::{DETAIL, UPDATES, api, client, server::route};
use crate::{
    ListQuery,
    hub::{Hub, LiveStatus},
    tests::{page, wait},
};
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI64, Ordering},
};

const PROJECTS: &str = "/api/v1/projects";

fn projects(archived: bool) -> (u16, String) {
    let archived_at = archived.then_some("2026-11-18T12:00:00Z");
    (
        200,
        page(
            vec![
                json!({"id":"project-1","key":"SUB","name":"Active","defaultStateId":"state-1"}),
                json!({"id":"project-2","key":"OLD","name":"Old","defaultStateId":"state-2",
                    "archivedAt":archived_at}),
            ],
            None,
        ),
    )
}

#[test]
fn an_sse_404_refreshes_an_archived_project_and_recovers_the_remaining_stream() {
    let archived = Arc::new(AtomicBool::new(false));
    let serving = archived.clone();
    let rest = api(Arc::new(AtomicI64::new(1)));
    let server = super::Server::new(move |seen| {
        let archived = serving.load(Ordering::SeqCst);
        match route(&seen.path) {
            PROJECTS => projects(archived),
            UPDATES if archived && seen.path.contains("project=OLD") => (404, "{}".into()),
            _ => rest(seen),
        }
    });
    let hub = Hub::from_client(client(&server), "Subspace API");
    hub.activate(ListQuery::default()).unwrap();
    hub.open_issue("SUB-1", crate::PROFILE_ID).unwrap();
    let initial = wait(&hub, |snapshot| {
        snapshot.list().is_some()
            && snapshot
                .issue()
                .is_some_and(|issue| issue.detail().is_some())
    });
    let mut first = server.peer();
    assert_eq!(first.seen.path, "/api/v1/updates?project=OLD&project=SUB");
    first.ready();
    let live = wait(&hub, |snapshot| {
        snapshot.live() == LiveStatus::Live && snapshot.list_id() != initial.list_id()
    });

    archived.store(true, Ordering::SeqCst);
    first.close();
    let mut corrected = server.peer();
    assert_eq!(corrected.seen.path, "/api/v1/updates?project=SUB");
    let refreshed = hub.snapshot();
    let projects = refreshed.projects();
    assert_eq!(
        projects[0].states[0].id, "state-1",
        "cached states retained"
    );
    assert!(projects[1].archived_at.is_some());
    assert_eq!(refreshed.list_id(), live.list_id(), "no list replacement");
    assert!(refreshed.issue().unwrap().detail().is_some());
    assert_eq!(server.count("GET", "/api/v1/me"), 1, "no identity reload");
    assert_eq!(server.count("GET", PROJECTS), 2, "one catalogue refresh");
    assert!(server.count("GET", DETAIL) >= 1);
    corrected.ready();
    wait(&hub, |snapshot| snapshot.live() == LiveStatus::Live);
    hub.stop();
}
