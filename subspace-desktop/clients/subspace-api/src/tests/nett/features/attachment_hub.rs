mod recovery;

use super::super::{server::NettMock, wait_for};
use super::{org_path, own_client};
use crate::hub::{FilesSnapshot, Hub};
use crate::model::IssueMedia;
use crate::uploads::UploadError;
use serde_json::json;

#[test]
fn the_hub_adds_saves_and_removes_standalone_attachments() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = load_issue_attachments(&mock, "nett-fixture-attach-hub");
    wait_for("the attachments", &|| upload_files(&hub).items().len() == 2);
    hub.add_attachment(
        "added.txt",
        "text/plain",
        b"Added from the panel.\n".to_vec(),
    )
    .unwrap();
    wait_for("the added attachment", &|| {
        let files = upload_files(&hub);
        files.uploads().is_empty() && files.items().iter().any(|m| m.file_name == "added.txt")
    });
    let added = upload_files(&hub)
        .items()
        .iter()
        .find(|m| m.file_name == "added.txt")
        .cloned()
        .unwrap();
    assert!(added.is_standalone());
    assert_attachment_conflict_refreshes_the_list(&mock, &hub, &added);
    hub.remove_attachment(&added.media_id);
    wait_for("the removal", &|| {
        let files = upload_files(&hub);
        !files.removing(&added.media_id) && files.items().iter().all(|m| m.file_name != "added.txt")
    });
    hub.stop();
}

fn assert_attachment_conflict_refreshes_the_list(mock: &NettMock, hub: &Hub, added: &IssueMedia) {
    // A version changed elsewhere: the list is read again with a notice.
    let delete = org_path(
        mock,
        &format!("issues/OPS-1/attachments/{}", added.media_id),
    );
    mock.fail_next("DELETE", &delete, 409, None, 1);
    let lists = mock.count("GET", "/issues/OPS-1/attachments");
    hub.remove_attachment(&added.media_id);
    wait_for("the notice", &|| upload_files(hub).notice().is_some());
    wait_for("a fresh read of the list", &|| {
        mock.count("GET", "/issues/OPS-1/attachments") > lists
            && !upload_files(hub).is_loading()
            && upload_files(hub)
                .items()
                .iter()
                .any(|m| m.file_name == "added.txt")
    });
}

/// The hub, open on OPS-1 with its attachments listed, for upload tests.
fn attachments_hub(mock: &NettMock, token: &str) -> Hub {
    let hub = load_issue_attachments(mock, token);
    wait_for("the attachments", &|| upload_files(&hub).items().len() >= 2);
    hub
}

fn load_issue_attachments(mock: &NettMock, token: &str) -> Hub {
    let hub = Hub::from_client(own_client(mock, token), "NETT");
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait_for("the issue", &|| {
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.detail().is_some())
    });
    hub.load_attachments().unwrap();
    hub
}

fn upload_files(hub: &Hub) -> FilesSnapshot {
    hub.snapshot()
        .issue()
        .map(|issue| issue.files().clone())
        .unwrap_or_default()
}

#[test]
fn dismissing_an_unfinished_upload_cancels_its_reservation() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-cancel");
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    mock.fail_next("POST", &complete, 400, None, 1);
    hub.add_attachment(
        "cancelled.txt",
        "text/plain",
        b"Never completed.\n".to_vec(),
    )
    .unwrap();
    wait_for("the failed upload", &|| {
        upload_files(&hub)
            .uploads()
            .iter()
            .any(|u| matches!(u.state(), crate::hub::AttachmentState::Failed(_)))
    });
    let id = upload_files(&hub).uploads()[0].id().to_owned();
    hub.dismiss_attachment_upload(&id);
    wait_for("the cancellation", &|| {
        mock.requests().iter().any(|r| {
            r["method"] == "DELETE"
                && r["path"]
                    .as_str()
                    .is_some_and(|p| p.contains("/attachments/pending/"))
        })
    });
    assert!(upload_files(&hub).uploads().is_empty());
    hub.stop();
}

#[test]
fn a_rejected_or_forbidden_completion_offers_no_check() {
    for (status, name) in [(400, "rejected.txt"), (403, "forbidden.txt")] {
        let mock = NettMock::serving(&["attachments"]);
        let hub = attachments_hub(&mock, &format!("nett-fixture-attach-{status}"));
        let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
        mock.fail_next("POST", &complete, status, None, 1);
        hub.add_attachment(name, "text/plain", format!("{name}\n").into_bytes())
            .unwrap();
        let id = failed_upload(&hub, &|_| true);
        let upload = upload_files(&hub).uploads()[0].clone();
        assert!(
            !matches!(
                upload.state(),
                crate::hub::AttachmentState::Failed(UploadError::Checking)
            ),
            "{status}: not offered as a check"
        );
        assert!(!upload.retryable(), "{status}: only Dismiss");
        let completions = mock.count("POST", "/attachments/from-upload");
        hub.retry_attachment_upload(&id);
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert_eq!(mock.count("POST", "/attachments/from-upload"), completions);
        hub.stop();
    }
}

#[test]
fn a_completion_wait_past_five_minutes_holds_the_upload_for_five_minutes() {
    let mock = NettMock::serving(&["attachments"]);
    mock.config(&json!({"checkingPolls": 1, "retryAfterSeconds": u64::MAX}));
    let hub = attachments_hub(&mock, "nett-fixture-attach-forever");
    hub.add_attachment(
        "forever.txt",
        "text/plain",
        b"Checked for longer than the clock.\n".to_vec(),
    )
    .unwrap();
    failed_upload(&hub, &|_| true);
    let upload = upload_files(&hub).uploads()[0].clone();
    assert!(
        upload
            .retry_delay()
            .is_some_and(|wait| wait.as_secs() > 60 && wait <= crate::nett::flow::MAX_PAUSE),
        "the wait holds, for five minutes at most"
    );
    assert!(
        matches!(upload.state(), crate::hub::AttachmentState::Failed(UploadError::Unclear { retry_after }) if *retry_after <= 300),
        "and says so"
    );
    hub.stop();
}

#[test]
fn a_ninth_upload_waiting_on_an_issue_is_refused_until_one_goes() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-budget");
    // Every reservation fails for now: the uploads wait, keeping their bytes.
    let prepare = org_path(&mock, "issues/OPS-1/attachments/prepare");
    mock.fail_next("POST", &prepare, 500, None, 9);
    for n in 0..8 {
        hub.add_attachment(
            &format!("held-{n}.txt"),
            "text/plain",
            format!("Held {n}\n").into_bytes(),
        )
        .unwrap();
    }
    assert_eq!(
        hub.add_attachment("ninth.txt", "text/plain", b"Ninth\n".to_vec())
            .err(),
        Some(UploadError::Budget)
    );
    wait_for("eight failed uploads", &|| {
        upload_files(&hub)
            .uploads()
            .iter()
            .filter(|u| matches!(u.state(), crate::hub::AttachmentState::Failed(_)))
            .count()
            == 8
    });
    let first = upload_files(&hub).uploads()[0].id().to_owned();
    hub.dismiss_attachment_upload(&first);
    assert!(
        hub.add_attachment("ninth.txt", "text/plain", b"Ninth\n".to_vec())
            .is_ok(),
        "dismissing one makes room"
    );
    hub.stop();
}

#[test]
fn an_upload_kept_for_a_closed_issue_shows_again_under_the_key_it_moved_to() {
    let mock = NettMock::serving(&["attachments", "issueMoves"]);
    mock.config(&json!({"checkingPolls": 1, "retryAfterSeconds": 600}));
    let hub = attachments_hub(&mock, "nett-fixture-attach-moved");
    hub.add_attachment(
        "moved.txt",
        "text/plain",
        b"Checked after the issue moved.\n".to_vec(),
    )
    .unwrap();
    // NETT is still checking the bytes when the person moves away.
    failed_upload(&hub, &crate::hub::AttachmentUpload::checks);
    hub.close_issue();
    // Someone moves the issue to another team meanwhile.
    let mover = own_client(&mock, "nett-fixture-attach-mover");
    let (web, issue, version) = super::moves::move_inputs_for(&mover, "OPS-1");
    let preview = mover.preview_issue_move(&issue, &web, version).unwrap();
    let moved = mover.move_issue(&preview).unwrap().issue.fields.identifier;
    assert!(moved.starts_with("WEB-"));
    // Opened under its new key, the issue finds its upload once its detail
    // says it is the same issue.
    hub.open_issue(&moved, crate::PROFILE_ID).unwrap();
    wait_for("the upload, shown again", &|| {
        upload_files(&hub).uploads().len() == 1
    });
    assert!(
        upload_files(&hub).uploads()[0].checks(),
        "Check completes it"
    );
    hub.stop();
}

/// The first upload attempt's row, once it has failed as arranged.
fn failed_upload(
    hub: &crate::hub::Hub,
    done: &dyn Fn(&crate::hub::AttachmentUpload) -> bool,
) -> String {
    wait_for("the failed upload", &|| {
        upload_files(hub)
            .uploads()
            .iter()
            .any(|u| matches!(u.state(), crate::hub::AttachmentState::Failed(_)) && done(u))
    });
    upload_files(hub).uploads()[0].id().to_owned()
}
