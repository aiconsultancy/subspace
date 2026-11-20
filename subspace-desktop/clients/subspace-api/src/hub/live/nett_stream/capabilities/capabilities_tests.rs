use super::recheck_capabilities;
use crate::hub::{Hub, lock};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use subspace_framework::client_test_support::Mock;

const TOKEN: &str = "nett-stream-recheck";

fn client(mock: &Mock) -> crate::Client {
    crate::Client::nett_loopback(
        &format!("{}/api", mock.base_url()),
        Some("00000000-0000-4000-8000-0000000000d7"),
        Some(subspace_framework::core::Secret::new(TOKEN)),
    )
    .unwrap()
    .without_live()
}

#[test]
fn a_change_another_read_found_first_is_still_announced_after_a_due_recheck() {
    let updates = Arc::new(AtomicBool::new(false));
    let serving = Arc::clone(&updates);
    let mock = Mock::new(move |request| {
        if request.path.ends_with("/desktop/capabilities") {
            let features = if serving.load(Ordering::SeqCst) {
                r#"{"features":["updates"]}"#
            } else {
                r#"{"features":[]}"#
            };
            (200, features.into())
        } else {
            (404, "{}".into())
        }
    });
    let stream = client(&mock);
    let hub = Hub::from_client(stream.clone(), "NETT");
    assert_eq!(stream.nett_refresh_served(), Ok(false));
    // The stream waits with this revision, and finds the list due.
    let seen = stream.nett_served_revision();
    assert!(seen.is_some());
    updates.store(true, Ordering::SeqCst);
    // A team read of the same account reads the changed list first...
    assert_eq!(client(&mock).nett_refresh_served(), Ok(true));
    let epoch = lock(&hub.0.state).snapshot.scope_epoch;
    // ...so the stream's own read finds no change, yet the Spaces are told.
    recheck_capabilities(&Arc::downgrade(&hub.0), &stream, seen);
    assert_ne!(lock(&hub.0.state).snapshot.scope_epoch, epoch);
    // A recheck that finds the list as seen tells no one.
    let seen = stream.nett_served_revision();
    let epoch = lock(&hub.0.state).snapshot.scope_epoch;
    recheck_capabilities(&Arc::downgrade(&hub.0), &stream, seen);
    assert_eq!(lock(&hub.0.state).snapshot.scope_epoch, epoch);
    hub.stop();
}
