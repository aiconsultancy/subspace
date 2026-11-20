//! The upload store through a hub: snapshots, admission and uploads under
//! way as their issue closes.
use super::KEPT_FOR;
use super::store_tests::{ISSUE, MIB, OTHER, add, awaiting_check, open, upload};
use crate::hub::{AttachmentState, Hub, lock};
use crate::uploads::UploadError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subspace_framework::client_test_support::{Mock, Request};

const UPLOAD: &str = "upload-parked-on-close";

/// A NETT hub on `mock`, its client signed in with `token`.
fn nett_hub(mock: &Mock, token: &str) -> Hub {
    let client = crate::Client::nett_loopback(
        &format!("{}/api", mock.base_url()),
        Some("00000000-0000-4000-8000-0000000000f2"),
        Some(subspace_framework::core::Secret::new(token)),
    )
    .unwrap()
    .without_live();
    Hub::from_client(client, "NETT")
}

/// NETT serving attachments: its capability list, the reservation and the
/// file host. The test answers completion itself.
fn nett(base: &Arc<Mutex<String>>, request: &Request) -> (u16, String) {
    let path = request.path.as_str();
    if path.ends_with("/desktop/capabilities") {
        (200, r#"{"features":["attachments"]}"#.into())
    } else if path.ends_with("/attachments/prepare") {
        let url = format!("{}/files/{UPLOAD}", lock(base));
        let reply = serde_json::json!({"uploadId": UPLOAD, "uploadUrl": url, "method": "PUT"});
        (200, reply.to_string())
    } else if request.method == "PUT" && path.ends_with(UPLOAD) {
        (200, String::new())
    } else {
        (404, "{}".into())
    }
}

fn wait_for(what: &str, done: &dyn Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_snapshot_lets_go_of_uploads_kept_too_long_while_the_hub_was_idle() {
    let mock = Mock::new(|_| (204, String::new()));
    let hub = nett_hub(&mock, "nett-store-idle");
    {
        let mut state = lock(&hub.0.state);
        open(&mut state, "OPS-1", Some(ISSUE));
        add(&mut state, ISSUE, vec![awaiting_check(8, "transfer-idle")]);
        state.snapshot.issue = None;
        // It closed long ago, and nothing used the store since.
        let long_ago = Instant::now().checked_sub(KEPT_FOR + Duration::from_secs(1));
        state.follow_uploads(long_ago.unwrap());
    }
    let _ = hub.snapshot();
    assert!(
        !lock(&hub.0.state)
            .uploads
            .contains_transfer("transfer-idle")
    );
    wait_for("the cancellation", &|| {
        mock.requests().iter().any(|request| {
            request.method == "DELETE"
                && request.path.ends_with(&format!(
                    "/issues/{ISSUE}/attachments/pending/transfer-idle"
                ))
        })
    });
    hub.stop();
}

#[test]
fn a_file_refused_on_its_issue_lets_no_kept_upload_go() {
    let mock = Mock::new(|_| (404, "{}".into()));
    let hub = nett_hub(&mock, "nett-store-refused");
    // Bytes held elsewhere leave too little room for a new file unless the
    // failed upload kept for a closed issue goes.
    let elsewhere: Arc<[u8]> = vec![0; 60 * MIB].into();
    let target = {
        let mut state = lock(&hub.0.state);
        state.attachment_buffers.push(Arc::downgrade(&elsewhere));
        open(&mut state, "OPS-2", Some(OTHER));
        add(
            &mut state,
            OTHER,
            vec![awaiting_check(4 * MIB, "transfer-kept")],
        );
        open(&mut state, "OPS-1", Some(ISSUE));
        // Eight uploads already wait on the open issue.
        let eight = (0..8).map(|_| upload(8, AttachmentState::Uploading, None, false));
        add(&mut state, ISSUE, eight.collect());
        state.follow_uploads(Instant::now());
        drop(state);
        hub.attachment_target().unwrap()
    };
    let file = || b"One more.\n".to_vec();
    assert_eq!(
        hub.add_attachment_to(&target, "more.txt", "text/plain", file()),
        Err(UploadError::Budget)
    );
    assert!(
        lock(&hub.0.state)
            .uploads
            .contains_transfer("transfer-kept")
    );
    // A file chosen for an earlier opening is refused before anything goes.
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    assert_eq!(
        hub.add_attachment_to(&target, "more.txt", "text/plain", file()),
        Err(UploadError::Removed)
    );
    assert!(
        lock(&hub.0.state)
            .uploads
            .contains_transfer("transfer-kept")
    );
    drop(elsewhere);
    hub.stop();
}

/// Whether an upload kept aside for a closed issue has failed.
fn kept_failure(hub: &Hub) -> bool {
    let state = lock(&hub.0.state);
    state.snapshot.issue.is_none()
        && state
            .uploads
            .entries
            .iter()
            .any(|entry| matches!(entry.upload.state, AttachmentState::Failed(_)))
}

#[test]
fn an_upload_in_progress_when_its_issue_closes_is_shown_again_when_it_reopens() {
    let base = Arc::new(Mutex::new(String::new()));
    let served = Arc::clone(&base);
    let mock = Mock::with_headers(move |request| {
        if request.path.ends_with("/attachments/from-upload") {
            std::thread::sleep(Duration::from_millis(400));
            return (202, vec![("Retry-After".into(), "120".into())], "{}".into());
        }
        let (status, body) = nett(&served, request);
        (status, Vec::new(), body)
    });
    *lock(&base) = mock.base_url().to_owned();
    let hub = nett_hub(&mock, "nett-parked-upload");
    // The issue is open with its detail, as once it has loaded.
    open(&mut lock(&hub.0.state), "OPS-1", Some("1"));
    hub.add_attachment(
        "parked.txt",
        "text/plain",
        b"Checked after the issue closed.\n".to_vec(),
    )
    .unwrap();
    // The bytes reached the file host and completion is under way when the
    // person moves away from the issue.
    wait_for("the completion", &|| {
        mock.requests()
            .iter()
            .any(|request| request.path.ends_with("/attachments/from-upload"))
    });
    hub.close_issue();
    wait_for("the outcome, kept aside", &|| kept_failure(&hub));
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    let snapshot = hub.snapshot();
    let uploads = snapshot.issue().unwrap().files().uploads();
    assert_eq!(uploads.len(), 1, "the upload is shown again");
    assert!(uploads[0].checks(), "Check completes the same transfer");
    assert!(matches!(
        uploads[0].state(),
        AttachmentState::Failed(UploadError::Unclear { retry_after: 120 })
    ));
    hub.stop();
}
