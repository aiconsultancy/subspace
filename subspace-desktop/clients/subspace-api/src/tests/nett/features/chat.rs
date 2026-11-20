use super::super::{server::NettMock, works};
use super::{assert_credential_free, file_host, org_path, own_client};
use crate::chat_uploads::{UploadDraft, UploadError as ChatError};
use serde_json::json;

#[test]
fn chat_uploads_prepare_once_recover_by_reference_and_read_through_grants() {
    let mock = NettMock::serving(&["chatUploads"]);
    mock.config(&json!({"checkingPolls": 1, "retryAfterSeconds": 0}));
    let client = works(&mock);
    let draft = UploadDraft::new(
        "notes.txt".into(),
        "text/plain".into(),
        b"Paste from Channels.\n".to_vec(),
    )
    .unwrap();
    mock.clear_requests();
    let upload = client.create_chat_upload(&draft).unwrap();
    assert_eq!(upload.reference, draft.reference());
    assert_eq!(upload.id, draft.id());
    assert!(upload.url.is_empty() && upload.version >= 1);
    let puts: Vec<_> = file_host(&mock)
        .into_iter()
        .filter(|r| r["method"] == "PUT")
        .collect();
    assert_eq!(puts.len(), 1);
    assert_credential_free(&puts);
    assert_eq!(mock.count("POST", "/chat/uploads/from-upload"), 2);
    // The draft's key is spent; recovery finds the upload by reference.
    assert_eq!(
        client.create_chat_upload(&draft).err(),
        Some(ChatError::Conflict)
    );
    assert_eq!(client.check_chat_upload(&draft).unwrap().id, upload.id);

    let card = client.resolve_chat_upload(&draft.reference()).unwrap();
    assert_eq!(card.status, "available");
    mock.clear_requests();
    let bytes = client
        .chat_upload_bytes(card.upload.as_ref().unwrap())
        .unwrap();
    assert_eq!(bytes, b"Paste from Channels.\n");
    assert_eq!(mock.count("POST", "/grants"), 1);
    assert_credential_free(&file_host(&mock));

    // An unknown reference is a missing card, not an error.
    let missing = client
        .resolve_chat_upload("UPL-AQIDBAUGBwgJCgsMDQ4PEA")
        .unwrap();
    assert_eq!(missing.status, "missing");
    assert!(missing.upload.is_none());
}

fn draft(text: &str) -> UploadDraft {
    UploadDraft::new(
        "notes.txt".into(),
        "text/plain".into(),
        text.as_bytes().to_vec(),
    )
    .unwrap()
}

#[test]
fn a_chat_upload_whose_put_reply_was_lost_completes_without_resending() {
    let mock = NettMock::serving(&["chatUploads"]);
    let client = own_client(&mock, "nett-fixture-chat-lost");
    mock.config(&json!({"dropPutResponse": 1}));
    let draft = draft("The reply to this PUT never arrives.\n");
    let upload = client.create_chat_upload(&draft).unwrap();
    assert_eq!(upload.reference, draft.reference());
    assert_eq!(mock.count("PUT", ""), 1, "the bytes went once");
}

#[test]
fn a_chat_upload_refused_after_its_bytes_is_checked_and_completed_not_resent() {
    let mock = NettMock::serving(&["chatUploads"]);
    let client = own_client(&mock, "nett-fixture-chat-refused");
    let complete = org_path(&mock, "chat/uploads/from-upload");
    mock.fail_next("POST", &complete, 401, None, 1);
    let draft = draft("Completion is refused once.\n");
    assert_eq!(
        client.create_chat_upload(&draft).err(),
        Some(ChatError::Unconfirmed)
    );
    let upload = client.check_chat_upload(&draft).unwrap();
    assert_eq!(upload.reference, draft.reference());
    assert_eq!(
        mock.count("PUT", ""),
        1,
        "the check completed the same transfer"
    );
}

#[test]
fn a_chat_upload_past_its_checking_window_completes_when_checked() {
    let mock = NettMock::serving(&["chatUploads"]);
    let client = own_client(&mock, "nett-fixture-chat-slow");
    mock.config(&json!({"checkingPolls": 9, "retryAfterSeconds": 0}));
    let draft = draft("NETT checks this one for a while.\n");
    assert_eq!(
        client.create_chat_upload(&draft).err(),
        Some(ChatError::Unconfirmed)
    );
    assert_eq!(client.check_chat_upload(&draft).unwrap().id, draft.id());
    assert_eq!(mock.count("PUT", ""), 1);
}

#[test]
fn a_chat_upload_whose_completion_reply_was_lost_is_found_by_its_reference() {
    let mock = NettMock::serving(&["chatUploads"]);
    let client = own_client(&mock, "nett-fixture-chat-dropped");
    let complete = org_path(&mock, "chat/uploads/from-upload");
    mock.config(&json!({"dropNext": [{"method": "POST", "path": complete, "times": 1}]}));
    let draft = draft("Completed, but the reply was lost.\n");
    assert_eq!(
        client.create_chat_upload(&draft).err(),
        Some(ChatError::Unconfirmed)
    );
    assert_eq!(client.check_chat_upload(&draft).unwrap().id, draft.id());
    assert_eq!(mock.count("PUT", ""), 1);
}

#[test]
fn a_chat_check_before_the_servers_wait_sends_no_completion() {
    let mock = NettMock::serving(&["chatUploads"]);
    let client = own_client(&mock, "nett-fixture-chat-wait");
    mock.config(&json!({"checkingPolls": 1, "retryAfterSeconds": 90}));
    let draft = draft("Checked for a long time.\n");
    assert_eq!(
        client.create_chat_upload(&draft).err(),
        Some(ChatError::Unconfirmed)
    );
    // A check before NETT's wait ends sends nothing at all: no read of the
    // reference, no reservation and no completion.
    let before = mock.requests().len();
    assert!(client.check_chat_upload(&draft).is_err());
    assert_eq!(mock.requests().len(), before, "nothing was sent");
}
