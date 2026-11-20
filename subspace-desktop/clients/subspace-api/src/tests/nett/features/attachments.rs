use super::super::{organisation, server::NettMock, works};
use super::{assert_credential_free, file_host, org_path, own_client};
use crate::model::{IssueMedia, MediaBinding};
use crate::uploads::{ImageDeclaration, UploadError};
use crate::{Client, PageRequest};
use serde_json::json;
use subspace_framework::core::Secret;

#[test]
fn standalone_attachments_list_upload_once_wait_while_checked_and_delete_by_version() {
    let mock = NettMock::serving(&["attachments"]);
    let client = works(&mock);
    assert_lists_and_fetches_attachments(&mock, &client);
    let media = assert_uploads_attachment_once(&mock, &client);
    assert_deletes_current_version(&mock, &client, &media);
}

fn assert_lists_and_fetches_attachments(mock: &NettMock, client: &Client) {
    let page = client
        .attachments("OPS-1", &PageRequest::default())
        .unwrap();
    let names: Vec<_> = page.items.iter().map(|m| m.file_name.as_str()).collect();
    assert_eq!(names.len(), 2, "{names:?}");
    let log = page
        .items
        .iter()
        .find(|m| m.file_name == "restore-log.txt")
        .unwrap();
    let chart = page
        .items
        .iter()
        .find(|m| m.file_name == "restore-chart.png")
        .unwrap();
    assert!(matches!(log.binding, MediaBinding::Standalone { .. }));
    assert_eq!((log.kind.as_str(), chart.kind.as_str()), ("file", "image"));
    assert!(!log.public_file_id.is_empty() && log.expires_at.is_none());

    // Downloads: a file grant from the API, then the bytes without it.
    mock.clear_requests();
    assert_eq!(
        client.fetch_text(log, 1024).unwrap(),
        b"Restore finished in 4 minutes.\n"
    );
    assert!(
        client
            .fetch_image(chart, 1024 * 1024)
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    // Saving reads any standalone file through its grant, whatever its type.
    assert_eq!(
        client.fetch_file(log, 1024).unwrap(),
        b"Restore finished in 4 minutes.\n"
    );
    assert!(
        client
            .fetch_file(chart, 1024 * 1024)
            .unwrap()
            .starts_with(b"\x89PNG")
    );
    let grants: Vec<_> = mock
        .requests()
        .into_iter()
        .filter(|r| {
            r["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("/files/grants"))
        })
        .collect();
    assert!(!grants.is_empty());
    assert!(
        grants
            .iter()
            .all(|r| r["authorization"] == true && r["orgHeader"].is_string())
    );
    assert_credential_free(&file_host(mock));
}

fn assert_uploads_attachment_once(mock: &NettMock, client: &Client) -> IssueMedia {
    // Prepare, send exactly one PUT, then check completion twice.
    mock.config(&json!({"checkingPolls": 2, "retryAfterSeconds": 0}));
    mock.clear_requests();
    let bytes = b"Notes from the restore.\n";
    let declaration = ImageDeclaration::new("restore-notes.txt", "text/plain", bytes).unwrap();
    let media = client
        .upload_attachment("OPS-1", &declaration, bytes)
        .unwrap();
    assert_eq!(media.file_name, "restore-notes.txt");
    assert!(matches!(media.binding, MediaBinding::Standalone { .. }));
    let puts: Vec<_> = file_host(mock)
        .into_iter()
        .filter(|r| r["method"] == "PUT")
        .collect();
    assert_eq!(puts.len(), 1, "the bytes are sent exactly once");
    assert_eq!(puts[0]["contentType"], "text/plain");
    assert_eq!(puts[0]["contentLength"], json!(bytes.len()));
    assert_credential_free(&puts);
    assert_eq!(mock.count("POST", "/attachments/from-upload"), 3);

    media
}

fn assert_deletes_current_version(mock: &NettMock, client: &Client, media: &IssueMedia) {
    // A stale version is refused; the current one removes it.
    let mut stale = media.clone();
    stale.version += 7;
    assert_eq!(
        client.delete_media(&stale),
        Err(UploadError::Refused {
            status: 409,
            retry_after: 0
        }),
        "a changed attachment is reported so the list is read again"
    );
    client.delete_media(media).unwrap();
    let after = client
        .attachments("OPS-1", &PageRequest::default())
        .unwrap();
    assert!(after.items.iter().all(|m| m.media_id != media.media_id));
    assert_eq!(
        mock.count("DELETE", &format!("/attachments/{}", media.media_id)),
        2
    );
}

#[test]
fn a_second_account_never_reuses_the_first_accounts_cached_grant() {
    let mock = NettMock::serving(&["attachments"]);
    let org = organisation(&mock, "Fixture Works");
    mock.grantless_token("nett-fixture-second");
    let first = works(&mock);
    let second = Client::nett_loopback(
        &mock.api(),
        Some(&org),
        Some(Secret::new("nett-fixture-second")),
    )
    .unwrap()
    .without_live();
    let page = first.attachments("OPS-1", &PageRequest::default()).unwrap();
    let log = page
        .items
        .iter()
        .find(|m| m.file_name == "restore-log.txt")
        .unwrap();
    assert!(first.fetch_text(log, 1024).is_ok());
    mock.clear_requests();
    assert_eq!(
        second.fetch_text(log, 1024).err(),
        Some(crate::Error::Forbidden)
    );
    assert_eq!(
        mock.count("POST", "/files/grants"),
        1,
        "it asked for its own"
    );
    assert!(file_host(&mock).is_empty(), "nothing reached the file host");
    // The first account's grant still serves it.
    assert!(first.fetch_text(log, 1024).is_ok());
}

#[test]
fn an_unclear_standalone_upload_is_completed_under_its_key() {
    let mock = NettMock::serving(&["attachments"]);
    let client = own_client(&mock, "nett-fixture-attach-unclear");
    let complete = org_path(&mock, "issues/OPS-1/attachments/from-upload");
    mock.fail_next("POST", &complete, 503, Some(0), 1);
    let bytes = b"Kept while NETT was unavailable.\n";
    let declaration = ImageDeclaration::new("unclear.txt", "text/plain", bytes).unwrap();
    let key = uuid::Uuid::new_v4().to_string();
    let attempt = client.upload_issue_attachment("OPS-1", &key, &declaration, bytes);
    assert!(matches!(
        attempt.result,
        Err(UploadError::Unclear { .. } | UploadError::Checking)
    ));
    assert!(attempt.bytes_sent);
    let media = client
        .complete_issue_attachment("OPS-1", attempt.transfer.as_deref().unwrap(), &declaration)
        .unwrap()
        .expect("the transfer was completed");
    assert_eq!(media.file_name, "unclear.txt");
    assert_eq!(mock.count("PUT", ""), 1, "the bytes went once");
    assert_eq!(
        mock.count("POST", "/attachments/prepare"),
        1,
        "never prepared again"
    );
}
