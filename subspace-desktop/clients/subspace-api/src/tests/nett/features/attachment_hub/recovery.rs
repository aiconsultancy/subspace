use super::{NettMock, attachments_hub, failed_upload, org_path, upload_files, wait_for};
use crate::tests::nett::organisation;
use crate::uploads::UploadError;
use serde_json::json;

/// Adds a file from the hub, lets the first attempt fail as arranged, then
/// checks it: the transfer is completed, never prepared or sent again.
fn check_completes_the_transfer(mock: &NettMock, hub: &crate::hub::Hub, name: &str) {
    hub.add_attachment(
        name,
        "text/plain",
        format!("{name} contents\n").into_bytes(),
    )
    .unwrap();
    wait_for("the first attempt", &|| {
        upload_files(hub).uploads().iter().any(|u| {
            u.checks()
                && matches!(
                    u.state(),
                    crate::hub::AttachmentState::Failed(UploadError::Checking)
                )
        })
    });
    let id = upload_files(hub).uploads()[0].id().to_owned();
    hub.retry_attachment_upload(&id);
    wait_for("the stored attachment", &|| {
        let files = upload_files(hub);
        files.uploads().is_empty() && files.items().iter().any(|m| m.file_name == name)
    });
    assert_eq!(mock.count("PUT", ""), 1, "the bytes went once");
    assert_eq!(
        mock.count("POST", "/attachments/prepare"),
        1,
        "prepared once"
    );
}

#[test]
fn a_standalone_completion_whose_reply_was_lost_is_completed_again_not_resent() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-dropped");
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    mock.config(&json!({"dropNext": [{"method": "POST", "path": complete, "times": 1}]}));
    check_completes_the_transfer(&mock, &hub, "dropped.txt");
    hub.stop();
}

#[test]
fn a_standalone_completion_refused_sign_in_after_the_bytes_is_checked_not_resent() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-401");
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    mock.fail_next("POST", &complete, 401, None, 1);
    check_completes_the_transfer(&mock, &hub, "refused.txt");
    hub.stop();
}

#[test]
fn a_long_completion_wait_is_kept_and_a_check_before_it_sends_nothing() {
    let mock = NettMock::serving(&["attachments"]);
    mock.config(&json!({"checkingPolls": 1, "retryAfterSeconds": 90}));
    let hub = attachments_hub(&mock, "nett-fixture-attach-wait");
    hub.add_attachment(
        "slow.txt",
        "text/plain",
        b"Checked for a long time.\n".to_vec(),
    )
    .unwrap();
    wait_for("the wait", &|| {
        upload_files(&hub)
            .uploads()
            .iter()
            .any(|u| u.retry_delay().is_some_and(|d| d.as_secs() > 60))
    });
    let id = upload_files(&hub).uploads()[0].id().to_owned();
    let completions = mock.count("POST", "/attachments/from-upload");
    hub.retry_attachment_upload(&id);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(mock.count("POST", "/attachments/from-upload"), completions);
    assert!(upload_files(&hub).notice().is_some());
    hub.stop();
}

fn wait_stored(hub: &crate::hub::Hub, name: &str) {
    wait_for("the stored attachment", &|| {
        let files = upload_files(hub);
        files.uploads().is_empty() && files.items().iter().any(|m| m.file_name == name)
    });
}

#[test]
fn a_completion_refused_after_the_upload_was_stored_finds_it_stored() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-archived");
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    // The completion is stored but its reply is lost; then the issue is
    // archived, so checking it again is refused with 409.
    mock.config(&json!({"dropNext": [{"method": "POST", "path": complete, "times": 1}]}));
    hub.add_attachment(
        "archived.txt",
        "text/plain",
        b"Stored before the issue was archived.\n".to_vec(),
    )
    .unwrap();
    let id = failed_upload(&hub, &|u| u.checks());
    mock.archive_issue(&organisation(&mock, "Fixture Works"), "OPS-1");
    hub.retry_attachment_upload(&id);
    wait_stored(&hub, "archived.txt");
    assert_eq!(mock.count("PUT", ""), 1, "the bytes went once");
    assert_eq!(mock.count("POST", "/attachments/prepare"), 1);
    hub.stop();
}

#[test]
fn a_completion_refused_when_nothing_was_stored_reserves_anew() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-lapsed");
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    // The completion fails before anything is stored, then the reservation
    // lapses: the check is refused and the list shows no such file.
    mock.fail_next("POST", &complete, 500, None, 1);
    hub.add_attachment("lapsed.txt", "text/plain", b"Never stored.\n".to_vec())
        .unwrap();
    let id = failed_upload(&hub, &|u| u.checks());
    mock.post_hook("transfers/expire", &json!({}));
    hub.retry_attachment_upload(&id);
    wait_for_absent_upload(&hub);
    hub.retry_attachment_upload(&id);
    wait_stored(&hub, "lapsed.txt");
    assert_eq!(
        mock.count("POST", "/attachments/prepare"),
        2,
        "a new reservation"
    );
    hub.stop();
}

#[test]
fn a_check_after_the_issue_was_refreshed_completes_the_same_transfer() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-refresh");
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    mock.config(&json!({"dropNext": [{"method": "POST", "path": complete, "times": 1}]}));
    hub.add_attachment(
        "refreshed.txt",
        "text/plain",
        b"Checked after a refresh.\n".to_vec(),
    )
    .unwrap();
    let id = failed_upload(&hub, &|u| u.checks());
    hub.reload_issue().unwrap();
    wait_for("the refreshed issue", &|| {
        hub.snapshot().issue().is_some_and(|issue| {
            issue.detail().is_some() && issue.status() == &crate::hub::IssueStatus::Loaded
        })
    });
    assert_eq!(upload_files(&hub).uploads().len(), 1, "the upload is kept");
    hub.retry_attachment_upload(&id);
    wait_stored(&hub, "refreshed.txt");
    assert_eq!(mock.count("PUT", ""), 1, "the bytes went once");
    assert_eq!(mock.count("POST", "/attachments/prepare"), 1);
    hub.stop();
}

#[test]
fn a_reservation_that_lapsed_before_its_bytes_is_made_again_under_a_new_key() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-expired");
    let prepare = org_path(&mock, "issues/OPS-1/attachments/prepare");
    // The reservation is made but its reply is lost, and then it lapses:
    // its key is refused from now on.
    mock.config(&json!({"dropNext": [{"method": "POST", "path": prepare, "times": 1}]}));
    hub.add_attachment(
        "expired.txt",
        "text/plain",
        b"Reserved, lapsed, reserved again.\n".to_vec(),
    )
    .unwrap();
    let id = failed_upload(&hub, &|u| !u.checks());
    mock.post_hook("transfers/expire", &json!({}));
    hub.retry_attachment_upload(&id);
    wait_for("the refused key", &|| {
        upload_files(&hub).uploads().iter().any(|u| {
            matches!(
                u.state(),
                crate::hub::AttachmentState::Failed(UploadError::Refused { status: 409, .. })
            )
        })
    });
    hub.retry_attachment_upload(&id);
    wait_stored(&hub, "expired.txt");
    assert_eq!(mock.count("PUT", ""), 1, "the bytes went once");
    hub.stop();
}

#[test]
fn an_identical_file_from_another_upload_is_not_taken_for_this_one() {
    let mock = NettMock::serving(&["attachments"]);
    let hub = attachments_hub(&mock, "nett-fixture-attach-twin");
    let bytes = b"The same file, twice.\n".to_vec();
    hub.add_attachment("twin.txt", "text/plain", bytes.clone())
        .unwrap();
    wait_stored(&hub, "twin.txt");
    // An identical second upload fails before anything is stored, and its
    // reservation lapses: the first file is someone else's upload.
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    mock.fail_next("POST", &complete, 500, None, 1);
    hub.add_attachment("twin.txt", "text/plain", bytes).unwrap();
    let id = failed_upload(&hub, &|u| u.checks());
    mock.post_hook("transfers/expire", &json!({}));
    hub.retry_attachment_upload(&id);
    wait_for_absent_upload(&hub);
    let twins = upload_files(&hub)
        .items()
        .iter()
        .filter(|m| m.file_name == "twin.txt")
        .count();
    assert_eq!(twins, 1, "only the first upload was stored");
    hub.stop();
}

fn wait_for_absent_upload(hub: &crate::hub::Hub) {
    wait_for("the proven absence", &|| {
        upload_files(hub)
            .uploads()
            .iter()
            .any(|u| matches!(u.state(), crate::hub::AttachmentState::NotFound) && !u.checks())
    });
}
