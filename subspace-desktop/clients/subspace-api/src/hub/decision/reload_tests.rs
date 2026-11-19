use super::{DecisionRequest, DecisionStatus, Hub, Job, Task, load, lock};
use crate::tests::{common, connected, decisions::detail, wait};
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::Duration;
use subspace_framework::client_test_support::Mock;

const ID: &str = "00000000-0000-0000-0000-000000000001";
const DEADLINE: Duration = Duration::from_secs(5);

#[test]
fn old_detail_cannot_replace_a_reopened_panel_for_the_same_id() {
    assert_stale_detail_is_dropped(|hub| {
        hub.close_decision();
        assert!(hub.snapshot().issue().unwrap().decision(ID).is_none());
        hub.open_decision(ID).unwrap();
    });
}

#[test]
fn pending_detail_restarts_after_reload_without_installing_the_old_reply() {
    assert_stale_detail_is_dropped(|hub| hub.reload_issue().unwrap());
}

fn assert_stale_detail_is_dropped(replace: impl FnOnce(&Hub)) {
    let (mock, started, release) = held_detail();
    let hub = connected(&mock);
    let job = pending_job(&hub);
    let Task::Decision(request) = &job.task else {
        unreachable!()
    };
    std::thread::scope(|scope| {
        let old = scope.spawn(|| load(&hub.0, &job, request));
        started.recv_timeout(DEADLINE).unwrap();
        replace(&hub);
        wait(&hub, |snap| {
            matches!(
                snap.issue().unwrap().decision(ID),
                Some(DecisionStatus::Gone)
            )
        });
        release.send(()).unwrap();
        old.join().unwrap();
        assert!(matches!(
            hub.snapshot().issue().unwrap().decision(ID),
            Some(DecisionStatus::Gone)
        ));
    });
    assert_eq!(
        mock.requests()
            .iter()
            .filter(|r| r.path.ends_with(ID))
            .count(),
        2
    );
    hub.stop();
}

fn pending_job(hub: &Hub) -> Job {
    let mut state = lock(&hub.0.state);
    let slot = &mut state.snapshot.issue.as_mut().unwrap().decision;
    slot.install(ID, DecisionStatus::Loading);
    let request = DecisionRequest {
        generation: slot.generation,
        id: ID.into(),
    };
    let job = Job::new(&state, Task::Decision(request));
    assert!(job.current(&state));
    job
}

fn held_detail() -> (Mock, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (started, seen) = mpsc::channel();
    let (release, held) = mpsc::channel();
    let held = Mutex::new(held);
    let initial = AtomicBool::new(true);
    let mock = Mock::new(move |request| {
        if !request.path.ends_with(ID) {
            return common(request);
        }
        if initial.swap(false, Ordering::Relaxed) {
            started.send(()).unwrap();
            held.lock().unwrap().recv_timeout(DEADLINE).unwrap();
            return (200, detail().to_string());
        }
        (404, "{}".into())
    });
    (mock, seen, release)
}
