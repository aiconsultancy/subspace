use std::collections::BTreeMap;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use anyhow::{Result, bail};

use super::definition_with_input;
use crate::project::{InputValue, LoadedProject};
use crate::project_runtime::SessionStatus;
use crate::project_service::{InputRevision, ProjectService, shared};

const WAIT: Duration = Duration::from_secs(2);
const ID: &str = "service-test";

fn fixture() -> (tempfile::TempDir, Arc<ProjectService>, LoadedProject) {
    let root = tempfile::tempdir().expect("service root");
    let service = shared(root.path()).expect("service");
    let loaded = service
        .create(
            &definition_with_input(),
            BTreeMap::from([("main.py".into(), b"pass".to_vec())]),
        )
        .expect("create");
    service.open(ID).expect("open");
    (root, service, loaded)
}

// Only the slow preparation boundary is controlled. Capture, locking,
// cancellation, revision validation and replacement use the production path.
fn during_prepare(
    service: &Arc<ProjectService>,
    loaded: LoadedProject,
    action: impl FnOnce(Arc<ProjectService>) -> Result<()> + Send + 'static,
) -> (bool, String) {
    let (entered, waiting) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let launcher = service.clone();
    let launch = std::thread::spawn(move || {
        launcher.launch_with(ID, &loaded.revision, true, |_, _| {
            entered.send(()).expect("preparation entered");
            released
                .recv_timeout(WAIT * 2)
                .expect("release preparation");
            bail!("test-only preparation failure")
        })
    });
    waiting
        .recv_timeout(WAIT)
        .expect("launch reached preparation");
    let (finished, completion) = mpsc::channel();
    let controller = service.clone();
    let control = std::thread::spawn(move || {
        finished
            .send(action(controller))
            .expect("control completion");
    });
    let timely = completion.recv_timeout(WAIT);
    release.send(()).expect("release launch");
    control.join().expect("control thread");
    let error = launch.join().expect("launch thread").expect_err("rejected");
    if let Ok(result) = &timely {
        assert!(result.is_ok(), "control failed: {result:?}");
    }
    (timely.is_ok(), error.to_string())
}

#[test]
fn stop_on_stopped_slot_cancels_preparation_without_waiting() {
    let (_root, service, loaded) = fixture();
    let (timely, error) = during_prepare(&service, loaded, |service| service.stop(ID));
    assert!(timely, "Stop waited for preparation");
    assert!(error.contains("Project launch conflict"), "{error}");
    assert_eq!(
        service.open(ID).expect("readback").status,
        SessionStatus::Stopped
    );
}

#[test]
fn close_and_reopen_same_stopped_session_does_not_revive_preparation() {
    let (_root, service, loaded) = fixture();
    let before = service.live_state(ID).expect("before");
    let (timely, error) = during_prepare(&service, loaded, |service| {
        service.close(ID)?;
        service.open(ID)?;
        Ok(())
    });
    assert!(timely, "Close waited for preparation");
    assert!(error.contains("Project launch conflict"), "{error}");
    assert_eq!(
        service.live_state(ID).expect("after").session_id(),
        before.session_id()
    );
}

#[test]
fn shutdown_cancels_preparation_and_disallows_later_launches() {
    let (_root, service, loaded) = fixture();
    let revision = loaded.revision.clone();
    let (timely, error) = during_prepare(&service, loaded, |service| service.shutdown());
    assert!(timely, "Shutdown waited for preparation");
    assert!(error.contains("Project launch conflict"), "{error}");
    assert!(service.start(ID, &revision).is_err());
    assert!(service.live_state(ID).is_err());
}

#[test]
fn accepted_input_edit_rejects_the_prepared_input_snapshot() {
    let (_root, service, loaded) = fixture();
    let live = service.live_state(ID).expect("input snapshot");
    let expected = InputRevision::new(live.session_id(), live.input_revision());
    let (timely, error) = during_prepare(&service, loaded, move |service| {
        service.set_value(ID, "amount", InputValue::Number(2.), &expected)?;
        Ok(())
    });
    assert!(timely, "Input edit waited for preparation");
    assert!(error.contains("revision conflict"), "{error}");
    assert_eq!(
        service.live_state(ID).expect("readback").input_revision(),
        1
    );
}

#[test]
fn source_edit_and_restore_still_rejects_the_pending_launch() {
    let (_root, service, loaded) = fixture();
    let revision = loaded.revision.clone();
    let (timely, error) = during_prepare(&service, loaded, move |service| {
        let edited = service.edit_file(ID, "main.py", b"# edited", &revision)?;
        service.edit_file(ID, "main.py", b"pass", &edited.revision)?;
        Ok(())
    });
    assert!(timely, "Source edit waited for preparation");
    assert!(error.contains("Project launch conflict"), "{error}");
    assert_eq!(
        service.load(ID).expect("restored source").files["main.py"],
        b"pass"
    );
}
