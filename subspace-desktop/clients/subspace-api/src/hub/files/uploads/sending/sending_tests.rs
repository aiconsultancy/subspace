use super::{Attempt, record, send};
use crate::hub::files::uploads::{AttachmentUpload, UploadIssue};
use crate::hub::{AttachmentState, Hub, IssueSnapshot, IssueStatus, lock};
use crate::uploads::UploadError;
use subspace_framework::client_test_support::Mock;

const ISSUE: &str = "00000000-0000-4000-8000-0000000000b7";

/// A NETT hub on `mock`, with OPS-1 open and read as [`ISSUE`].
fn hub(mock: &Mock, token: &str) -> Hub {
    let client = crate::Client::nett_loopback(
        &format!("{}/api", mock.base_url()),
        Some("00000000-0000-4000-8000-0000000000f4"),
        Some(subspace_framework::core::Secret::new(token)),
    )
    .unwrap()
    .without_live();
    let hub = Hub::from_client(client, "NETT");
    let mut issue = IssueSnapshot::new("OPS-1", crate::PROFILE_ID, IssueStatus::Loaded);
    issue.detail = Some(serde_json::from_value(crate::tests::detail(ISSUE)).unwrap());
    lock(&hub.0.state).snapshot.issue = Some(issue);
    hub
}

/// OPS-1, read as [`ISSUE`], once since moved.
fn issue() -> UploadIssue {
    UploadIssue {
        id: ISSUE.into(),
        identifier: "OPS-1".into(),
    }
}

/// An attempt on OPS-1, read as [`ISSUE`].
fn attempt() -> Attempt {
    Attempt::new(issue())
}

fn wait_for(what: &str, done: &dyn Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

fn upload(state: AttachmentState) -> AttachmentUpload {
    let mut upload = crate::hub::files::uploads::admission_tests::upload(8);
    upload.id = crate::hub::comments::next_attachment_id();
    upload.state = state;
    upload
}

#[test]
fn a_queued_upload_let_go_before_it_starts_sends_nothing() {
    let mock = Mock::new(|_| (500, "{}".into()));
    let hub = hub(&mock, "nett-sending-queued");
    let generation = lock(&hub.0.state).snapshot.generation;
    // Its row went, as when its closed issue was let go, while the job
    // waited for the pool.
    send(
        &hub.0,
        generation,
        &attempt(),
        &upload(AttachmentState::Uploading),
    );
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.contains("/attachments")),
        "nothing was sent"
    );
    hub.stop();
}

#[test]
fn a_queued_check_let_go_before_it_starts_cancels_the_reservation_it_holds() {
    let mock = Mock::new(|_| (500, "{}".into()));
    let hub = hub(&mock, "nett-sending-queued-check");
    let generation = lock(&hub.0.state).snapshot.generation;
    // Check was pressed for a transfer whose bytes may be stored; its row
    // went while the job waited for the pool.
    let mut upload = upload(AttachmentState::Checking);
    upload.transfer = Some("transfer-queued".into());
    upload.bytes_sent = true;
    send(&hub.0, generation, &attempt(), &upload);
    wait_for("the cancellation", &|| {
        mock.requests().iter().any(|request| {
            request.method == "DELETE"
                && request.path.ends_with(&format!(
                    "/issues/{ISSUE}/attachments/pending/transfer-queued"
                ))
        })
    });
    assert!(
        !mock
            .requests()
            .iter()
            .any(|request| request.path.ends_with("/attachments/from-upload")),
        "nothing was completed"
    );
    hub.stop();
}

#[test]
fn every_attempt_names_its_issue_by_its_id() {
    let mock = Mock::new(|request| {
        if request.path.ends_with("/desktop/capabilities") {
            (200, r#"{"features":["attachments"]}"#.into())
        } else {
            (500, "{}".into())
        }
    });
    let hub = hub(&mock, "nett-sending-stable-id");
    let upload = upload(AttachmentState::Uploading);
    let generation = {
        let mut state = lock(&hub.0.state);
        let opening = state.snapshot.issue.as_ref().unwrap().opening;
        state.uploads.add(upload.clone(), issue(), opening);
        state.snapshot.generation
    };
    // OPS-1 may have moved, and its key stopped naming it, while the job
    // waited for the pool.
    send(&hub.0, generation, &attempt(), &upload);
    let paths: Vec<String> = mock
        .requests()
        .into_iter()
        .map(|request| request.path)
        .filter(|path| path.contains("/attachments/"))
        .collect();
    assert!(!paths.is_empty());
    assert!(
        paths
            .iter()
            .all(|path| path.contains(&format!("/issues/{ISSUE}/attachments/"))),
        "{paths:?}"
    );
    hub.stop();
}

#[test]
fn an_attempt_ending_after_its_upload_was_let_go_cancels_under_the_issue_id() {
    let mock = Mock::new(|_| (500, "{}".into()));
    let hub = hub(&mock, "nett-sending-orphan");
    let mut state = lock(&hub.0.state);
    let outcome = (
        Err(UploadError::Network),
        Some("transfer-orphan".into()),
        true,
    );
    let cancels = record(
        &hub.0,
        &mut state,
        (&attempt(), &upload(AttachmentState::Uploading)),
        outcome,
    );
    // Under the issue's ID, which still names it after a move; its old key
    // may not.
    assert_eq!(cancels, [(ISSUE.to_owned(), "transfer-orphan".to_owned())]);
    drop(state);
    hub.stop();
}

#[test]
fn an_upload_let_go_but_stored_meanwhile_reads_the_open_issues_attachments_again() {
    let mock = Mock::new(|_| (500, "{}".into()));
    let hub = hub(&mock, "nett-sending-stored");
    let mut state = lock(&hub.0.state);
    let media = serde_json::from_value(serde_json::json!({
        "mediaId": "m", "version": 1, "kind": "file", "fileName": "a.txt",
        "contentType": "text/plain", "sizeBytes": 8, "expiresAt": null,
    }))
    .unwrap();
    let outcome = (Ok(Some(media)), Some("transfer-stored".into()), true);
    let cancels = record(
        &hub.0,
        &mut state,
        (&attempt(), &upload(AttachmentState::Checking)),
        outcome,
    );
    assert!(cancels.is_empty(), "a stored file keeps its reservation");
    assert!(
        state.snapshot.issue.as_ref().unwrap().files.is_loading(),
        "the open issue's attachments are read again"
    );
    drop(state);
    hub.stop();
}
