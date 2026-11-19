use crate::{
    ListQuery, ListTab, PageRequest,
    hub::{
        Hub, IssueStatus, lock,
        timeline::TimelineRequest,
        worker::{Job, Task},
    },
    model::DecisionStatus,
    tests::{common, connected, detail, wait},
};
use serde_json::json;
use std::{
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};
use subspace_framework::client_test_support::Mock;

const DECISION: &str = "00000000-0000-0000-0000-000000000001";
const DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn same_issue_reload_fences_queued_and_in_flight_timeline_reads() {
    let (mock, started, release) = fixture();
    let hub = connected(&mock);
    hub.activate_timeline(0).unwrap();
    wait_for_title(&hub, "Initial");
    let revision = hub.snapshot().issue_revision();
    let (queued, held) = pre_reset_jobs(&hub);

    std::thread::scope(|scope| {
        // Run the same refresh worker as dispatch, but join it explicitly so
        // the final assertion cannot race the stale response's installation.
        let read = scope.spawn(|| super::refresh(&hub.0, &held, "SUB-1"));
        started.recv_timeout(DEADLINE).unwrap();
        hub.reload(ListQuery::new(ListTab::All)).unwrap();
        let reset = wait(&hub, |snapshot| {
            let issue = snapshot.issue().unwrap();
            *issue.status() == IssueStatus::Loaded
                && issue.detail().unwrap().fields.version == 2
                && issue.timeline().idle()
        });
        assert_eq!(reset.issue_revision(), revision);
        let queued_current_at_reset = queued.current(&lock(&hub.0.state));

        hub.activate_timeline(0).unwrap();
        wait_for_title(&hub, "Reloaded");
        release.send(()).unwrap();
        read.join().unwrap();
        let snapshot = hub.snapshot();
        let timeline = snapshot.issue().unwrap().timeline();
        let decision = timeline.decision(DECISION).unwrap();
        assert_eq!(decision.title, "Reloaded");
        assert_eq!(decision.status, DecisionStatus::Accepted);
        assert_eq!(timeline.page().unwrap().items.len(), 1);
        assert_eq!(timeline.page().unwrap().items[0].id, "reloaded-event");
        assert!(!queued_current_at_reset, "pre-reset job can still dispatch");
    });

    hub.refresh_discussion("SUB-1").unwrap();
    let refreshed = wait_for_title(&hub, "Refreshed");
    let decision = refreshed
        .issue()
        .unwrap()
        .timeline()
        .decision(DECISION)
        .unwrap();
    assert_eq!(decision.status, DecisionStatus::Superseded);
    hub.stop();
}

fn pre_reset_jobs(hub: &Hub) -> (Job, Job) {
    let state = lock(&hub.0.state);
    let queued = Job::new(
        &state,
        Task::Timeline(TimelineRequest {
            identifier: "SUB-1".into(),
            page: PageRequest {
                cursor: Some("old-cursor".into()),
                ..PageRequest::default()
            },
            comment_goal: 0,
            remaining: 0,
        }),
    );
    assert!(queued.current(&state));
    (queued, Job::new(&state, Task::Refresh("SUB-1".into())))
}

fn wait_for_title(hub: &Hub, title: &str) -> crate::hub::HubSnapshot {
    wait(hub, |snapshot| {
        snapshot
            .issue()
            .unwrap()
            .timeline()
            .decision(DECISION)
            .is_some_and(|decision| decision.title == title)
    })
}

fn fixture() -> (Mock, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (started, in_flight) = mpsc::channel();
    let (release, respond) = mpsc::channel();
    let respond = Mutex::new(respond);
    let issues = AtomicUsize::new(0);
    let reads = AtomicUsize::new(0);
    let mock = Mock::new(
        move |request| match request.path.split('?').next().unwrap() {
            "/api/v1/issues/SUB-1" => {
                let mut issue = detail("1");
                issue["version"] = json!(issues.fetch_add(1, Ordering::SeqCst) + 1);
                (200, issue.to_string())
            }
            "/api/v1/issues/SUB-1/activity" => {
                let (id, title, status) = match reads.fetch_add(1, Ordering::SeqCst) {
                    0 => ("initial-event", "Initial", "proposed"),
                    1 => {
                        started.send(()).unwrap();
                        respond.lock().unwrap().recv_timeout(DEADLINE).unwrap();
                        ("obsolete-event", "Obsolete", "rejected")
                    }
                    2 => ("reloaded-event", "Reloaded", "accepted"),
                    3 => ("reloaded-event", "Refreshed", "superseded"),
                    n => panic!("unexpected timeline read {n}"),
                };
                (200, decision_page(id, title, status))
            }
            _ => common(request),
        },
    );
    (mock, in_flight, release)
}

fn decision_page(event: &str, title: &str, status: &str) -> String {
    json!({"items":[{"id":event,"kind":"decision.logged",
        "occurredAt":"2026-11-16T12:00:00Z","displayAt":"2026-11-16T12:00:00Z",
        "actor":{"type":"system","label":"Fixture service"},"data":{"decisionId":DECISION}}],
        "references":{"decisions":[{"id":DECISION,"title":title,"status":status}]}})
    .to_string()
}
