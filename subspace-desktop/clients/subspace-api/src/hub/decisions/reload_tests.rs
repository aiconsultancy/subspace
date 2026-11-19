use super::{Hub, IssueStatus, Job, Read, Task, load, lock};
use crate::tests::{common, connected, decisions::row, detail, page, wait};
use serde_json::{Value, json};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;
use subspace_framework::client_test_support::Mock;

const DEADLINE: Duration = Duration::from_secs(5);
const FIRST: &str = "/api/v1/decisions?issue=SUB-1&limit=50";
const OLD_NEXT: &str = "/api/v1/decisions?issue=SUB-1&limit=50&cursor=old";
const FRESH_NEXT: &str = "/api/v1/decisions?issue=SUB-1&limit=50&cursor=fresh";
const OLD_TAIL_ID: &str = "00000000-0000-0000-0000-000000000003";
const FRESH_TAIL_ID: &str = "00000000-0000-0000-0000-000000000006";

#[test]
fn stale_list_cannot_land_after_issue_navigation() {
    let (mock, started, release) = held_list();
    let hub = connected(&mock);
    let job = pending_job(&hub, Read::First);
    let Task::Decisions(request) = &job.task else {
        unreachable!()
    };
    std::thread::scope(|scope| {
        let old = scope.spawn(|| load(&hub.0, &job, request));
        started.recv_timeout(DEADLINE).unwrap();
        hub.open_issue("SUB-2", crate::PROFILE_ID).unwrap();
        wait(&hub, |snap| {
            snap.issue()
                .is_some_and(|i| i.identifier() == "SUB-2" && *i.status() == IssueStatus::Loaded)
        });
        release.send(()).unwrap();
        old.join().unwrap();
    });
    let snapshot = hub.snapshot();
    let next = snapshot.issue().unwrap();
    assert!(!next.decisions_loading());
    assert!(next.decisions().is_none());
    hub.stop();
}

fn pending_job(hub: &Hub, read: Read) -> Job {
    let mut state = lock(&hub.0.state);
    let issue = state.snapshot.issue.as_mut().unwrap();
    assert!(issue.decisions.stage(Some(read)));
    let request = issue.decisions.request(&issue.identifier).unwrap();
    let job = Job::new(&state, Task::Decisions(request));
    assert!(job.current(&state));
    job
}

fn held_list() -> (Mock, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (started, seen) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let held = Mutex::new(held);
    let mock = Mock::new(move |request| match request.path.as_str() {
        FIRST => {
            started.send(()).unwrap();
            held.lock().unwrap().recv_timeout(DEADLINE).unwrap();
            (200, page(vec![row()], None))
        }
        "/api/v1/issues/SUB-2" => (200, detail("2").to_string()),
        _ => common(request),
    });
    (mock, seen, release)
}

#[test]
fn live_refresh_preserves_pending_more_when_more_responds_first() {
    assert_eq!(after_live_refresh(true), ["Fresh head", "Fresh tail"]);
}

#[test]
fn live_refresh_preserves_pending_more_when_refresh_responds_first() {
    assert_eq!(after_live_refresh(false), ["Fresh head", "Fresh tail"]);
}

fn after_live_refresh(more_first: bool) -> Vec<String> {
    let (old, old_request) = held_reply();
    let (fresh, fresh_request) = held_reply();
    let mock = refresh_fixture(old_request, fresh_request);
    let hub = connected(&mock);
    hub.activate_decisions().unwrap();
    wait(&hub, |s| s.issue().unwrap().decisions().is_some());
    let job = pending_job(&hub, Read::More);
    let Task::Decisions(request) = &job.task else {
        unreachable!()
    };
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| load(&hub.0, &job, request));
        old.started.recv_timeout(DEADLINE).unwrap();
        crate::hub::live::reread_issue(&hub.0, &mut lock(&hub.0.state));
        fresh.started.recv_timeout(DEADLINE).unwrap();
        if more_first {
            old.release.send(()).unwrap();
            worker.join().unwrap();
            // The old worker reached the apply path, but the refresh has not.
            assert_eq!(titles(&hub.snapshot()), ["Original head"]);
            fresh.release.send(()).unwrap();
            wait_for_refresh(&hub);
        } else {
            fresh.release.send(()).unwrap();
            wait_for_refresh(&hub);
            old.release.send(()).unwrap();
            worker.join().unwrap();
        }
    });
    let result = titles(&hub.snapshot());
    hub.stop();
    result
}

fn wait_for_refresh(hub: &Hub) {
    wait(hub, |s| {
        let issue = s.issue().unwrap();
        !issue.decisions_loading() && issue.decisions().unwrap().next_cursor.is_none()
    });
}

fn titles(snapshot: &crate::hub::HubSnapshot) -> Vec<String> {
    snapshot
        .issue()
        .unwrap()
        .decisions()
        .unwrap()
        .items
        .iter()
        .map(|item| item.title.clone())
        .collect()
}

struct HeldReply {
    started: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

fn held_reply() -> (HeldReply, impl Fn() + Send + Sync) {
    let (started, seen) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let held = Mutex::new(held);
    let wait = move || {
        started.send(()).unwrap();
        held.lock().unwrap().recv_timeout(DEADLINE).unwrap();
    };
    (
        HeldReply {
            started: seen,
            release,
        },
        wait,
    )
}

fn refresh_fixture(
    old: impl Fn() + Send + Sync + 'static,
    fresh: impl Fn() + Send + Sync + 'static,
) -> Mock {
    let first = AtomicBool::new(true);
    Mock::new(move |request| match request.path.as_str() {
        FIRST if first.swap(false, Ordering::Relaxed) => {
            (200, page(vec![named("Original head")], Some("old")))
        }
        FIRST => (200, page(vec![named("Fresh head")], Some("fresh"))),
        OLD_NEXT => {
            old();
            (200, page(vec![tail(OLD_TAIL_ID, "Obsolete tail")], None))
        }
        FRESH_NEXT => {
            fresh();
            (200, page(vec![tail(FRESH_TAIL_ID, "Fresh tail")], None))
        }
        _ => common(request),
    })
}

fn named(title: &str) -> Value {
    let mut item = row();
    item["title"] = json!(title);
    item
}

fn tail(id: &str, title: &str) -> Value {
    let mut item = named(title);
    item["id"] = json!(id);
    item
}

#[test]
fn explicit_refresh_during_show_more_rereads_from_the_first_page() {
    let (old, old_request) = held_reply();
    let mock = restart_fixture(old_request);
    let hub = connected(&mock);
    hub.activate_decisions().unwrap();
    wait(&hub, |s| s.issue().unwrap().decisions().is_some());
    let job = pending_job(&hub, Read::More);
    let Task::Decisions(request) = &job.task else {
        unreachable!()
    };
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| load(&hub.0, &job, request));
        old.started.recv_timeout(DEADLINE).unwrap();
        hub.reload_issue().unwrap();
        wait_for_refresh(&hub);
        assert_eq!(titles(&hub.snapshot()), ["Fresh head", "Fresh tail"]);
        old.release.send(()).unwrap();
        worker.join().unwrap();
    });
    // The superseded continuation cannot land on the fresh chain.
    assert_eq!(titles(&hub.snapshot()), ["Fresh head", "Fresh tail"]);
    let reads: Vec<String> = mock
        .requests()
        .into_iter()
        .map(|r| r.path)
        .filter(|path| path.starts_with("/api/v1/decisions"))
        .collect();
    assert_eq!(reads, [FIRST, OLD_NEXT, FIRST, FRESH_NEXT]);
    hub.stop();
}

#[test]
fn a_restarted_continuation_keeps_the_cached_cursor_until_the_refresh_lands() {
    let mut decisions = super::IssueDecisions::default();
    assert!(decisions.stage(Some(Read::First)));
    let page: crate::model::Page<crate::model::DecisionListItem> =
        serde_json::from_str(&page(vec![named("Original head")], Some("old"))).unwrap();
    decisions.install(Read::First, Ok((page, 1)));
    assert!(decisions.stage(Some(Read::More)));
    decisions.restart();
    assert!(decisions.pending == Some(Read::Refresh(2)));
    assert_eq!(
        decisions.page.as_ref().unwrap().next_cursor.as_deref(),
        Some("old")
    );
    // A refresh that fails keeps the rows and asks to retry the same depth.
    decisions.install(Read::Refresh(2), Err(crate::Error::Network));
    assert!(decisions.retry == Some(Read::Refresh(2)));
    assert_eq!(decisions.page.as_ref().unwrap().items.len(), 1);
}

fn restart_fixture(old: impl Fn() + Send + Sync + 'static) -> Mock {
    let first = AtomicBool::new(true);
    let held = AtomicBool::new(true);
    Mock::new(move |request| match request.path.as_str() {
        FIRST if first.swap(false, Ordering::Relaxed) => {
            (200, page(vec![named("Original head")], Some("old")))
        }
        FIRST => (200, page(vec![named("Fresh head")], Some("fresh"))),
        OLD_NEXT => {
            if held.swap(false, Ordering::Relaxed) {
                old();
            }
            (200, page(vec![tail(OLD_TAIL_ID, "Obsolete tail")], None))
        }
        FRESH_NEXT => (200, page(vec![tail(FRESH_TAIL_ID, "Fresh tail")], None)),
        _ => common(request),
    })
}
