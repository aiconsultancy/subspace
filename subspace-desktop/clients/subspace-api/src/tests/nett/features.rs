//! Every feature the mock advertises, through the adapter's own operations:
//! the wire each sends, nullable fields, conflicts, pending completion and
//! a file host that never sees a credential.

use super::server::NettMock;
use super::{TOKEN, organisation, signed_in, works};
use crate::chat_uploads::{UploadDraft, UploadError as ChatError};
use crate::issue_writes::IssueSaveError;
use crate::model::MediaBinding;
use crate::uploads::{ImageDeclaration, UploadError};
use crate::{Client, CommentIntent, CommentPost, PageRequest};
use serde_json::{Value, json};
use subspace_framework::core::Secret;

/// Requests to NETT's file host: uploads and grant reads.
fn file_host(mock: &NettMock) -> Vec<Value> {
    mock.requests()
        .into_iter()
        .filter(|request| {
            request["path"]
                .as_str()
                .is_some_and(|path| path.starts_with("/u/") || path.starts_with("/files/"))
        })
        .collect()
}

fn assert_credential_free(requests: &[Value]) {
    assert!(!requests.is_empty());
    for request in requests {
        assert_eq!(request["authorization"], false, "{request}");
        assert_eq!(request["cookie"], false, "{request}");
        assert!(request["orgHeader"].is_null(), "{request}");
    }
}

#[test]
fn standalone_attachments_list_upload_once_wait_while_checked_and_delete_by_version() {
    let mock = NettMock::serving(&["attachments"]);
    let client = works(&mock);
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
    assert_credential_free(&file_host(&mock));

    // Uploads: prepare, exactly one PUT, then completion that checks twice.
    mock.config(&json!({"checkingPolls": 2, "retryAfterSeconds": 0}));
    mock.clear_requests();
    let bytes = b"Notes from the restore.\n";
    let declaration = ImageDeclaration::new("restore-notes.txt", "text/plain", bytes).unwrap();
    let media = client
        .upload_attachment("OPS-1", &declaration, bytes)
        .unwrap();
    assert_eq!(media.file_name, "restore-notes.txt");
    assert!(matches!(media.binding, MediaBinding::Standalone { .. }));
    let puts: Vec<_> = file_host(&mock)
        .into_iter()
        .filter(|r| r["method"] == "PUT")
        .collect();
    assert_eq!(puts.len(), 1, "the bytes are sent exactly once");
    assert_eq!(puts[0]["contentType"], "text/plain");
    assert_eq!(puts[0]["contentLength"], json!(bytes.len()));
    assert_credential_free(&puts);
    assert_eq!(mock.count("POST", "/attachments/from-upload"), 3);

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
    client.delete_media(&media).unwrap();
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
fn comment_files_are_managed_media_the_comment_claims_without_the_attachments_feature() {
    let mock = NettMock::start();
    let client = works(&mock);
    let draft = uuid::Uuid::new_v4().to_string();
    let key = uuid::Uuid::new_v4().to_string();
    let bytes = b"Restore output, attached to a comment.\n";
    let declaration = ImageDeclaration::new("restore.txt", "text/plain", bytes).unwrap();
    let media = client
        .upload_comment_media("OPS-1", (&draft, &key), &declaration, bytes)
        .unwrap();
    assert!(matches!(media.binding, MediaBinding::Managed));
    // The same request key finds the same upload again.
    let found = client
        .nett_check_comment_media("OPS-1", &draft, &key, &declaration)
        .unwrap()
        .unwrap();
    assert_eq!(found.media_id, media.media_id);
    assert_eq!(
        mock.count("PUT", ""),
        1,
        "one byte upload, none on the check"
    );

    let body = format!(
        "See the output.\n\n[Attached file](nett-media:{})",
        media.media_id
    );
    let intent = CommentIntent::new(&body, None)
        .unwrap()
        .with_media_draft(&draft);
    let Ok(CommentPost::Created(comment)) = client.post_comment("OPS-1", &intent) else {
        panic!("the comment was not created");
    };
    assert!(comment.media.iter().any(|m| m.media_id == media.media_id));

    // A second upload removed before posting is cancelled by its key.
    let other = uuid::Uuid::new_v4().to_string();
    client
        .upload_comment_media("OPS-1", (&draft, &other), &declaration, bytes)
        .unwrap();
    client
        .nett_cancel_comment_media("OPS-1", &draft, &other, &declaration)
        .unwrap();
    assert_eq!(mock.count("PUT", "/upload"), 1);
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

#[test]
fn moves_preview_without_numbers_and_confirm_the_subtree_with_the_token() {
    let mock = NettMock::serving(&["issueMoves"]);
    let client = works(&mock);
    let teams = client.all_projects().unwrap();
    let web = teams.iter().find(|t| t.key == "WEB").unwrap();
    let root = client.issue("OPS-2").unwrap();
    let version = u32::try_from(root.fields.version).unwrap();
    assert_eq!(
        client
            .preview_issue_move(&root.fields.id, &web.id, version + 1)
            .err(),
        Some(IssueSaveError::Conflict)
    );
    let preview = client
        .preview_issue_move(&root.fields.id, &web.id, version)
        .unwrap();
    assert_eq!(
        preview.issues.len(),
        3,
        "the tracker and its two sub-issues"
    );
    assert!(preview.issues.iter().all(|i| i.new_identifier.is_none()));
    assert_eq!(preview.destination.key, "WEB");
    assert!(!preview.moves_archived_sub_issue());
    let sent = mock
        .requests()
        .into_iter()
        .rev()
        .find(|r| {
            r["path"]
                .as_str()
                .is_some_and(|p| p.ends_with("/move-preview"))
        })
        .unwrap();
    assert_eq!(sent["method"], "POST");

    let result = client.move_issue(&preview).unwrap();
    assert_eq!(result.issue.fields.project_id, web.id);
    assert!(result.issue.fields.identifier.starts_with("WEB-"));
    assert!(result.moved.iter().all(|item| {
        item.new_identifier
            .as_deref()
            .is_some_and(|id| id.starts_with("WEB-"))
    }));
    assert_eq!(
        mock.count("PATCH", &format!("/issues/{}", root.fields.id)),
        1
    );
    // The token is spent: confirming again asks for a fresh preview.
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::Conflict)
    );
}

#[test]
fn a_move_with_an_archived_sub_issue_is_refused_before_it_is_sent() {
    let mock = NettMock::serving(&["issueMoves"]);
    let org = organisation(&mock, "Fixture Works");
    mock.archive_issue(&org, "OPS-4");
    let client = works(&mock);
    let web = client
        .all_projects()
        .unwrap()
        .into_iter()
        .find(|t| t.key == "WEB")
        .unwrap();
    let root = client.issue("OPS-2").unwrap();
    let preview = client
        .preview_issue_move(
            &root.fields.id,
            &web.id,
            u32::try_from(root.fields.version).unwrap(),
        )
        .unwrap();
    assert!(preview.moves_archived_sub_issue());
    mock.clear_requests();
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::ArchivedSubIssue)
    );
    assert_eq!(mock.count("PATCH", ""), 0);
}

#[test]
fn initiative_discovery_reads_nested_and_fleetless_rows_and_leaves_archived_out() {
    let mock = NettMock::serving(&["initiativeChannels"]);
    let initiatives = works(&mock).member_initiatives().unwrap();
    let names: Vec<_> = initiatives.iter().map(|i| i.name.as_str()).collect();
    assert!(names.contains(&"Platform › Desktop"), "{names:?}");
    assert!(
        !names
            .iter()
            .any(|n| n.contains("Retired") || n.contains("Legacy"))
    );
    let research = initiatives.iter().find(|i| i.name == "Research").unwrap();
    assert!(research.fleet_id.is_none() && research.fleet.is_none());
}

#[test]
fn names_nett_advertises_that_the_desktop_does_not_use_change_nothing() {
    let mock = NettMock::start();
    let org = organisation(&mock, "Fixture Works");
    mock.capabilities(
        &org,
        &[
            "issueSubscribers",
            "notificationUnread",
            "issueDocumentArchive",
            "laterThing",
        ],
    );
    let client = signed_in(&mock, Some(&org));
    assert!(client.all_projects().is_ok());
    for feature in [
        crate::backend::Feature::InitiativeChannels,
        crate::backend::Feature::ChatUploads,
        crate::backend::Feature::Attachments,
        crate::backend::Feature::IssueMoves,
        crate::backend::Feature::Updates,
    ] {
        assert!(!client.serves(feature));
    }
    let _ = TOKEN;
}

/// A client with its own bearer, so its flow control and caches are its
/// own.
fn own_client(mock: &NettMock, token: &str) -> Client {
    let org = organisation(mock, "Fixture Works");
    mock.token(token, "ada");
    Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(token)))
        .unwrap()
        .without_live()
}

fn org_path(mock: &NettMock, rest: &str) -> String {
    format!(
        "/api/v1/orgs/{}/{rest}",
        organisation(mock, "Fixture Works")
    )
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

#[test]
fn a_long_retry_after_is_reported_and_never_cut_short() {
    let mock = NettMock::start();
    let client = own_client(&mock, "nett-fixture-busy");
    let teams = org_path(&mock, "issue-teams");
    mock.fail_next("GET", &teams, 503, Some(45), 1);
    let start = std::time::Instant::now();
    assert_eq!(
        client.projects(&PageRequest::default()).err(),
        Some(crate::Error::UnexpectedStatus(503))
    );
    // The next request is refused here, without reaching NETT, until the
    // 45 seconds have passed.
    assert_eq!(
        client.projects(&PageRequest::default()).err(),
        Some(crate::Error::UnexpectedStatus(429))
    );
    assert!(start.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(mock.count("GET", "/issue-teams"), 1);
}

#[test]
fn a_move_whose_sub_issue_cannot_be_read_back_is_checked_without_moving_again() {
    let mock = NettMock::serving(&["issueMoves"]);
    let client = own_client(&mock, "nett-fixture-move-check");
    let web = client
        .all_projects()
        .unwrap()
        .into_iter()
        .find(|t| t.key == "WEB")
        .unwrap();
    let root = client.issue("OPS-2").unwrap();
    let preview = client
        .preview_issue_move(
            &root.fields.id,
            &web.id,
            u32::try_from(root.fields.version).unwrap(),
        )
        .unwrap();
    let child = preview.issues[1].id.clone();
    mock.fail_next(
        "GET",
        &org_path(&mock, &format!("issues/{child}")),
        503,
        Some(0),
        3,
    );
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
    let checked = client.verify_issue_move(&preview).unwrap();
    assert!(checked.moved.iter().all(|item| {
        item.new_identifier
            .as_deref()
            .is_some_and(|id| id.starts_with("WEB-"))
    }));
    assert_eq!(mock.count("PATCH", ""), 1, "the move was sent once");
    // An issue the plan named that is not in the destination fails the check.
    let mut other = preview.clone();
    other.issues[1].id = client.issue("OPS-1").unwrap().fields.id;
    assert_eq!(
        client.verify_issue_move(&other).err(),
        Some(IssueSaveError::Conflict)
    );
}

#[test]
fn a_busy_to_do_save_is_reported_as_busy() {
    let mock = NettMock::start();
    let client = own_client(&mock, "nett-fixture-todo-busy");
    let lists = client.issue_lists("OPS-1").unwrap();
    let items = client.issue_list_items("OPS-1", &lists[0].id).unwrap();
    let issue = client.issue("OPS-1").unwrap().fields.id;
    let path = org_path(
        &mock,
        &format!("issues/{issue}/lists/{}/items/{}", lists[0].id, items[1].id),
    );
    mock.fail_next("PUT", &path, 503, Some(0), 1);
    assert_eq!(
        client
            .set_issue_list_item_done("OPS-1", &items[1], true)
            .err(),
        Some(IssueSaveError::RateLimited)
    );
}

#[test]
fn the_hub_adds_saves_and_removes_standalone_attachments() {
    use crate::hub::Hub;
    let mock = NettMock::serving(&["attachments"]);
    let client = own_client(&mock, "nett-fixture-attach-hub");
    let hub = Hub::from_client(client, "NETT");
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    let wait = |what: &str, done: &dyn Fn() -> bool| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !done() {
            assert!(
                std::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    };
    let files = |hub: &Hub| {
        hub.snapshot()
            .issue()
            .map(|issue| issue.files().clone())
            .unwrap_or_default()
    };
    wait("the issue", &|| {
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.detail().is_some())
    });
    hub.load_attachments().unwrap();
    wait("the attachments", &|| files(&hub).items().len() == 2);
    hub.add_attachment(
        "added.txt",
        "text/plain",
        b"Added from the panel.\n".to_vec(),
    )
    .unwrap();
    wait("the added attachment", &|| {
        let files = files(&hub);
        files.uploads().is_empty() && files.items().iter().any(|m| m.file_name == "added.txt")
    });
    let added = files(&hub)
        .items()
        .iter()
        .find(|m| m.file_name == "added.txt")
        .cloned()
        .unwrap();
    assert!(added.is_standalone());
    // A version changed elsewhere: the list is read again with a notice.
    let delete = org_path(
        &mock,
        &format!("issues/OPS-1/attachments/{}", added.media_id),
    );
    mock.fail_next("DELETE", &delete, 409, None, 1);
    let lists = mock.count("GET", "/issues/OPS-1/attachments");
    hub.remove_attachment(&added.media_id);
    wait("the notice", &|| files(&hub).notice().is_some());
    wait("a fresh read of the list", &|| {
        mock.count("GET", "/issues/OPS-1/attachments") > lists
            && !files(&hub).is_loading()
            && files(&hub)
                .items()
                .iter()
                .any(|m| m.file_name == "added.txt")
    });
    hub.remove_attachment(&added.media_id);
    wait("the removal", &|| {
        let files = files(&hub);
        !files.removing(&added.media_id) && files.items().iter().all(|m| m.file_name != "added.txt")
    });
    hub.stop();
}

/// The hub, open on OPS-1 with its attachments listed, for upload tests.
fn attachments_hub(mock: &NettMock, token: &str) -> crate::hub::Hub {
    let hub = crate::hub::Hub::from_client(own_client(mock, token), "NETT");
    hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
    wait_for("the issue", &|| {
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.detail().is_some())
    });
    hub.load_attachments().unwrap();
    wait_for("the attachments", &|| upload_files(&hub).items().len() >= 2);
    hub
}

fn upload_files(hub: &crate::hub::Hub) -> crate::hub::FilesSnapshot {
    hub.snapshot()
        .issue()
        .map(|issue| issue.files().clone())
        .unwrap_or_default()
}

fn wait_for(what: &str, done: &dyn Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done() {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for {what}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

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

#[test]
fn comment_media_whose_completion_reply_was_lost_or_refused_is_checked_not_resent() {
    let mock = NettMock::start();
    let client = own_client(&mock, "nett-fixture-comment-retry");
    let complete = org_path(&mock, "issues/media/from-upload");
    let draft = uuid::Uuid::new_v4().to_string();
    for (n, arrange) in ["drop", "refuse"].into_iter().enumerate() {
        match arrange {
            "drop" => mock
                .config(&json!({"dropNext": [{"method": "POST", "path": complete, "times": 1}]})),
            _ => mock.fail_next("POST", &complete, 401, None, 1),
        }
        let key = uuid::Uuid::new_v4().to_string();
        let bytes = format!("Comment file {n}\n").into_bytes();
        let declaration = ImageDeclaration::new("comment.txt", "text/plain", &bytes).unwrap();
        let uploads = |mock: &NettMock| {
            file_host(mock)
                .iter()
                .filter(|r| r["method"] == "PUT")
                .count()
        };
        let puts = uploads(&mock);
        assert_eq!(
            client
                .upload_comment_media("OPS-1", (&draft, &key), &declaration, &bytes)
                .err(),
            Some(UploadError::Checking),
            "{arrange}: checked, not refused"
        );
        let found = client
            .nett_check_comment_media("OPS-1", &draft, &key, &declaration)
            .unwrap();
        assert!(
            found.is_some(),
            "{arrange}: the same transfer was completed"
        );
        assert_eq!(uploads(&mock), puts + 1, "{arrange}: one byte upload");
    }
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

#[test]
fn a_move_that_never_reached_nett_can_be_previewed_again() {
    let mock = NettMock::serving(&["issueMoves"]);
    let client = own_client(&mock, "nett-fixture-move-lost");
    let web = client
        .all_projects()
        .unwrap()
        .into_iter()
        .find(|t| t.key == "WEB")
        .unwrap();
    let root = client.issue("OPS-2").unwrap();
    let version = u32::try_from(root.fields.version).unwrap();
    let preview = client
        .preview_issue_move(&root.fields.id, &web.id, version)
        .unwrap();
    // The confirmation fails in transit: NETT never moved anything.
    mock.fail_next(
        "PATCH",
        &org_path(&mock, &format!("issues/{}", root.fields.id)),
        502,
        None,
        1,
    );
    assert_eq!(
        client.move_issue(&preview).err(),
        Some(IssueSaveError::Unconfirmed)
    );
    // The check finds the root where it was, so it says the plan did not
    // happen instead of asking for another check.
    assert_eq!(
        client.verify_issue_move(&preview).err(),
        Some(IssueSaveError::Conflict)
    );
    assert!(
        client
            .preview_issue_move(&root.fields.id, &web.id, version)
            .is_ok()
    );
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
    wait_for("the proven absence", &|| {
        upload_files(&hub)
            .uploads()
            .iter()
            .any(|u| matches!(u.state(), crate::hub::AttachmentState::NotFound) && !u.checks())
    });
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
fn comment_media_prepared_for_a_lease_that_ended_is_never_sent() {
    let mock = NettMock::start();
    crate::native::test_lease::set(Some(1));
    // Bound to the test thread's lease, as a native client is bound to its
    // account and organisation.
    let mut client = own_client(&mock, "nett-fixture-comment-lease");
    client.native_lease = Some(crate::NativeLease(crate::native::generation()));
    // The account or organisation changes right after the reservation's
    // reply was accepted, before the bytes would go.
    crate::native::test_lease::AFTER_REPLY.with(|next| next.set(Some(2)));
    let bytes = b"Prepared for an ended lease.\n".to_vec();
    let declaration = ImageDeclaration::new("comment.txt", "text/plain", &bytes).unwrap();
    let (draft, key) = (
        uuid::Uuid::new_v4().to_string(),
        uuid::Uuid::new_v4().to_string(),
    );
    let result = client.upload_comment_media("OPS-1", (&draft, &key), &declaration, &bytes);
    crate::native::test_lease::set(None);
    assert_eq!(result.err(), Some(UploadError::SignInNeeded));
    assert!(
        file_host(&mock).iter().all(|r| r["method"] != "PUT"),
        "the upload URL was never used"
    );
}

#[test]
fn a_late_to_do_answer_leaves_a_newer_save_pending_and_is_read_again() {
    use crate::hub::Hub;
    let mock = NettMock::start();
    let hub = Hub::from_client(own_client(&mock, "nett-fixture-todo-late"), "NETT");
    let open = |hub: &Hub| {
        hub.open_issue("OPS-1", crate::PROFILE_ID).unwrap();
        wait_for("the lists", &|| {
            hub.snapshot()
                .issue()
                .is_some_and(|issue| issue.todos().is_some())
        });
    };
    open(&hub);
    let snapshot = hub.snapshot();
    let issue = snapshot.issue().unwrap();
    let issue_id = issue.detail().unwrap().fields.id.clone();
    let item = issue.todos().unwrap()[0].items[1].clone();
    let put = org_path(
        &mock,
        &format!("issues/{issue_id}/lists/{}/items/{}", item.list_id, item.id),
    );
    mock.config(&json!({"slowBody": {"path": put, "seconds": 1.5, "times": 2}}));
    // Save A is stored, but its answer is slow; the issue is opened again
    // and save B starts on the same item before A's answer lands.
    let started = std::time::Instant::now();
    hub.set_todo_done(&item.id, true).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(300));
    hub.close_issue();
    open(&hub);
    wait_for("A stored", &|| {
        hub.snapshot().issue().is_some_and(|issue| {
            issue.todos().unwrap()[0]
                .items
                .iter()
                .any(|row| row.id == item.id && row.completed)
        })
    });
    let reads = mock.count("GET", "/items");
    hub.set_todo_done(&item.id, false).unwrap();
    let a_lands = std::time::Duration::from_millis(1800);
    std::thread::sleep(a_lands.saturating_sub(started.elapsed()));
    assert!(
        hub.snapshot()
            .issue()
            .is_some_and(|issue| issue.todo_saving(&item.id)),
        "B is still pending after A's answer"
    );
    wait_for("B saved and A reconciled", &|| {
        hub.snapshot().issue().is_some_and(|issue| {
            !issue.todo_saving(&item.id)
                && issue.todos().unwrap()[0]
                    .items
                    .iter()
                    .any(|row| row.id == item.id && !row.completed)
        }) && mock.count("GET", "/items") > reads
    });
    hub.stop();
}

#[test]
fn a_day_long_wait_does_not_leave_issues_dead_and_reload_sends_again() {
    use crate::hub::{Hub, HubStatus};
    let mock = NettMock::start();
    let token = "nett-fixture-day-long-wait";
    let hub = Hub::from_client(own_client(&mock, token), "NETT");
    let query = crate::ListQuery::new(crate::ListTab::All).project(Some("OPS"));
    hub.activate(query.clone()).unwrap();
    wait_for("the list", &|| hub.snapshot().status() == &HubStatus::Ready);
    // NETT, or anything in front of it, answers 503 and asks for a day.
    mock.fail_next(
        "GET",
        &org_path(&mock, "issues/groups"),
        503,
        Some(86_400),
        1,
    );
    hub.reload(query.clone()).unwrap();
    wait_for("the refusal", &|| {
        matches!(hub.snapshot().status(), HubStatus::Error(_))
    });
    let account = own_client(&mock, token).flow_account();
    assert!(
        crate::nett::flow::paused_for(account)
            .is_some_and(|wait| wait <= crate::nett::flow::MAX_PAUSE),
        "held for five minutes at most"
    );
    // Reload is the person asking: it is sent at once.
    let reads = mock.count("GET", "/issues/groups");
    hub.reload(query).unwrap();
    wait_for("the list again", &|| {
        hub.snapshot().status() == &HubStatus::Ready && mock.count("GET", "/issues/groups") > reads
    });
    hub.stop();
}
