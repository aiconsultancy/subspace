use crate::hub::{AttachmentState, Hub, IssueSnapshot, IssueStatus, lock};
use crate::uploads::UploadError;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subspace_framework::client_test_support::{Mock, Request};

const ISSUE: &str = "00000000-0000-4000-8000-0000000000f7";
const UPLOAD: &str = "upload-unknowable";

/// NETT serving attachments, whose completion of every transfer is refused
/// with 409 and whose list holds a same-named file that names no transfer
/// and gives neither size nor digest: no check can tell whether the upload
/// was stored.
fn nett(base: &Arc<Mutex<String>>, request: &Request) -> (u16, String) {
    let path = request.path.as_str();
    match request.method.as_str() {
        _ if path.ends_with("/desktop/capabilities") => {
            (200, r#"{"features":["attachments"]}"#.into())
        }
        _ if path.ends_with("/attachments/prepare") => {
            let url = format!("{}/files/{UPLOAD}", lock(base));
            let reply = serde_json::json!({"uploadId": UPLOAD, "uploadUrl": url, "method": "PUT"});
            (200, reply.to_string())
        }
        "PUT" => (200, String::new()),
        _ if path.ends_with("/attachments/from-upload") => (409, "{}".into()),
        "GET" if path.ends_with("/attachments") => (200, unknowable_list()),
        _ => (404, "{}".into()),
    }
}

/// A list whose one file has the upload's name, but names no transfer and
/// gives neither size nor digest.
fn unknowable_list() -> String {
    let record = serde_json::json!({"mediaId": "m", "version": 1, "kind": "file",
        "fileName": "again.txt", "contentType": "text/plain", "uploadId": null,
        "expiresAt": null});
    serde_json::json!({"items": [record], "nextCursor": null}).to_string()
}

/// A NETT hub on `mock` with OPS-1 open, read as [`ISSUE`].
fn hub(mock: &Mock) -> Hub {
    let client = crate::Client::nett_loopback(
        &format!("{}/api", mock.base_url()),
        Some("00000000-0000-4000-8000-0000000000f8"),
        Some(subspace_framework::core::Secret::new("nett-upload-again")),
    )
    .unwrap()
    .without_live();
    let hub = Hub::from_client(client, "NETT");
    let mut issue = IssueSnapshot::new("OPS-1", crate::PROFILE_ID, IssueStatus::Loaded);
    issue.detail = Some(serde_json::from_value(crate::tests::detail(ISSUE)).unwrap());
    lock(&hub.0.state).snapshot.issue = Some(issue);
    hub
}

/// The request keys of the reservations asked for so far.
fn keys(mock: &Mock) -> Vec<String> {
    mock.requests()
        .iter()
        .filter(|request| request.path.ends_with("/attachments/prepare"))
        .filter_map(|request| serde_json::from_str::<serde_json::Value>(&request.body).ok())
        .filter_map(|body| body["requestKey"].as_str().map(str::to_owned))
        .collect()
}

fn wait_for(what: &str, done: &dyn Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn count(mock: &Mock, method: &str, suffix: &str) -> usize {
    mock.requests()
        .iter()
        .filter(|request| request.method == method && request.path.ends_with(suffix))
        .count()
}

/// The open issue's one upload, as the panel shows it.
fn shown(hub: &Hub) -> crate::hub::AttachmentUpload {
    hub.snapshot().issue().unwrap().files().uploads()[0].clone()
}

#[test]
fn after_checks_that_cannot_tell_the_person_may_send_the_file_again() {
    let base = Arc::new(Mutex::new(String::new()));
    let served = Arc::clone(&base);
    let mock = Mock::new(move |request| nett(&served, request));
    *lock(&base) = mock.base_url().to_owned();
    let hub = hub(&mock);
    hub.add_attachment("again.txt", "text/plain", b"Stored, or not.\n".to_vec())
        .unwrap();
    let checked = |n: usize| {
        wait_for("the check", &|| {
            count(&mock, "POST", "/attachments/from-upload") >= n
                && matches!(
                    shown(&hub).state(),
                    AttachmentState::Failed(UploadError::Checking)
                )
        })
    };
    checked(1);
    // Each Check that cannot tell is counted; until a few in a row, Check
    // is all that is offered.
    for n in 2..=4 {
        assert!(!shown(&hub).may_send_again());
        let id = shown(&hub).id().to_owned();
        hub.retry_attachment_upload(&id);
        checked(n);
    }
    let upload = shown(&hub);
    assert!(upload.checks() && upload.may_send_again());
    // The person chooses to send it again, warned it may be attached twice:
    // a new reservation under a new key, the old one left to lapse.
    hub.send_attachment_upload_again(upload.id());
    wait_for("the second reservation", &|| keys(&mock).len() == 2);
    let keys = keys(&mock);
    assert_ne!(keys[0], keys[1], "sent again under a new key");
    assert_eq!(
        count(&mock, "DELETE", ""),
        0,
        "the first is never cancelled"
    );
    hub.stop();
}

#[test]
fn an_upload_whose_bytes_went_is_checked_but_never_sent_again() {
    // Its bytes may be stored: Check needs none.
    let waiting = crate::hub::AttachmentUpload::for_test(
        "kept.txt",
        AttachmentState::Failed(UploadError::Checking),
        true,
        0,
        true,
    )
    .unwrap();
    assert!(waiting.retryable() && waiting.checks());
    assert!(!waiting.may_send_again(), "there are no bytes to send");
    // Nothing was stored: sending again needs the file chosen again.
    let absent = crate::hub::AttachmentUpload::for_test(
        "kept.txt",
        AttachmentState::NotFound,
        false,
        0,
        true,
    )
    .unwrap();
    assert!(!absent.retryable());
}
