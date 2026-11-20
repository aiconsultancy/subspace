//! A NETT build's client bound to the native account, against the loopback
//! NETT mock: organisation discovery, the organisation menu, private caches
//! and prepared uploads belong to the account that read them. Its own
//! process, because the backend and the account are chosen once.

use serde_json::{Value, json};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subspace_api_client::chat_uploads::{UploadDraft, UploadError};
use subspace_api_client::issue_writes::IssueSaveError;
use subspace_api_client::model::IssueMedia;
use subspace_api_client::uploads::{ImageDeclaration, UploadError as CommentUploadError};
use subspace_api_client::{Client, Error, ListQuery, ListTab, nett};
use subspace_framework::core::Secret;

#[path = "../src/tests/nett/process.rs"]
mod process;
use process::NettProcess as Mock;

/// Fixture traffic stays on loopback: no proxy from the environment sees it.
fn direct() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .timeout_global(Some(Duration::from_secs(5)))
        .build()
        .into()
}

impl Mock {
    fn hook(&self, path: &str, body: &Value) {
        direct()
            .post(&format!("{}/__fixture/{path}", self.origin))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .unwrap();
    }

    fn request_log(&self) -> Vec<Value> {
        let mut response = direct()
            .get(&format!("{}/__fixture/requests", self.origin))
            .call()
            .unwrap();
        serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap()
    }

    fn requests(&self, method: &str, path: &str) -> usize {
        self.request_log()
            .iter()
            .filter(|entry| matches_request(entry, method, path))
            .count()
    }
}

fn matches_request(entry: &Value, method: &str, path: &str) -> bool {
    entry["method"] == method
        && entry["path"].as_str().is_some_and(|actual| {
            path.strip_suffix('*')
                .map_or_else(|| actual == path, |prefix| actual.starts_with(prefix))
        })
}

/// Wait for NETT to send successful headers, then retire the scope while
/// the response body is held. `retire` can verify any fixture state first.
fn late_reply<T: Send + 'static>(
    mock: &Mock,
    method: &str,
    path: &str,
    call: impl FnOnce() -> T + Send + 'static,
    retire: impl FnOnce(),
) -> T {
    let before = mock.requests(method, path);
    mock.hook(
        "config",
        &json!({"slowBody": {"path": path, "seconds": 10, "waitForRelease": true}}),
    );
    let worker = std::thread::spawn(call);
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        let status = mock
            .request_log()
            .iter()
            .filter(|entry| matches_request(entry, method, path))
            .skip(before)
            .find_map(|entry| entry["slowBodyHeadersStatus"].as_u64());
        if let Some(status) = status {
            break status;
        }
        assert!(
            Instant::now() < deadline,
            "no response headers for {method} {path}"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(
        status, 200,
        "{method} {path} succeeded before the scope changed"
    );
    assert!(
        !worker.is_finished(),
        "the response body should still be held"
    );
    retire();
    assert!(
        !worker.is_finished(),
        "the response body arrived before its release"
    );
    mock.hook("config", &json!({"slowBody": null}));
    worker.join().unwrap()
}

/// A fixed UUID's text, numbered `n`.
fn uuid_text(n: u8) -> String {
    format!("00000000-0000-4000-8000-0000000000{n:02x}")
}

fn identifiers(client: &Client, initiative: &str) -> Vec<String> {
    let query = ListQuery::new(ListTab::All).initiative(Some(initiative));
    let mut ids: Vec<String> = client
        .issue_groups(&query)
        .unwrap()
        .groups
        .iter()
        .flat_map(|group| &group.items)
        .map(|row| row.identifier.clone())
        .collect();
    ids.sort();
    ids
}

#[test]
fn organisations_caches_and_uploads_belong_to_the_account_that_read_them() {
    let mock = Mock::start(&["chatUploads", "attachments", "issueMoves"], &[]);
    let account = Arc::new(AtomicU64::new(1));
    let token = Arc::new(Mutex::new("nett-fixture-access".to_owned()));
    install_account(&mock, account.clone(), token.clone());

    let works = assert_context_follows_the_account(&mock, &account);
    assert_planning_projects_are_account_scoped(&mock, &account, &token, &works);
    assert_prepared_uploads_are_retired(&mock, &account, &token, &works);
    assert_late_reads_and_grants_are_discarded(&mock, &account, &works);
    let second = assert_organisation_switch_discards_old_replies(&mock, &works);
    assert_switching_organisations_clears_wait(&mock, &works, &second);
    assert_capabilities_are_read_for_each_account(&mock, &account, &works);
}

fn install_account(mock: &Mock, account: Arc<AtomicU64>, token: Arc<Mutex<String>>) {
    subspace_api_client::configure_nett_origin(&format!("{}/api", mock.origin)).unwrap();
    subspace_api_client::use_native_account(
        move || account.load(Ordering::SeqCst),
        |_| None,
        move || Ok(Secret::new(token.lock().unwrap().clone())),
    );
}

fn assert_context_follows_the_account(mock: &Mock, account: &AtomicU64) -> String {
    // A context body that arrives after the account changed belongs to no
    // one: it is dropped, and the menu shows nothing from it.
    let response = late_reply(
        mock,
        "GET",
        "/api/v1/auth/context",
        || Client::new().unwrap().me(),
        || account.store(2, Ordering::SeqCst),
    );
    assert_eq!(response.err(), Some(Error::SignInNeeded));
    let menu = nett::snapshot();
    assert!(menu.organisations.is_empty() && menu.selected.is_none());
    assert_eq!(menu.status, nett::Status::Unknown);

    // The current account's context is adopted.
    assert_eq!(
        Client::new().unwrap().me().unwrap().display_name,
        "Ada Fixture"
    );
    let menu = nett::snapshot();
    let works = menu
        .organisations
        .iter()
        .find(|org| org.name == "Fixture Works")
        .unwrap()
        .id
        .clone();
    assert_eq!(menu.status, nett::Status::Choose);

    // Signing out: nothing of the previous account shows or can be chosen.
    account.store(3, Ordering::SeqCst);
    assert!(nett::snapshot().organisations.is_empty());
    assert!(!nett::select_organisation(&works));
    works
}

fn assert_planning_projects_are_account_scoped(
    mock: &Mock,
    account: &AtomicU64,
    token: &Mutex<String>,
    works: &str,
) {
    // Two people in one organisation who see different planning projects
    // never share the initiative's project list.
    mock.hook(
        "tokens",
        &json!({"token": "nett-fixture-grace", "user": "grace"}),
    );
    mock.hook(
        "projects/hide",
        &json!({"org": works, "project": "Launch", "user": "grace"}),
    );
    Client::new().unwrap().me().unwrap();
    assert!(nett::select_organisation(works));
    let ada = Client::new().unwrap();
    let platform = ada
        .member_initiatives()
        .unwrap()
        .into_iter()
        .find(|i| i.name == "Platform")
        .unwrap()
        .id;
    assert!(identifiers(&ada, &platform).contains(&"OPS-1".to_owned()));
    *token.lock().unwrap() = "nett-fixture-grace".to_owned();
    account.store(4, Ordering::SeqCst);
    let me = Client::new().unwrap().me().unwrap();
    assert_eq!(me.display_name, "Grace Fixture");
    nett::select_organisation(works);
    let grace = Client::new().unwrap();
    assert!(
        identifiers(&grace, &platform).is_empty(),
        "Grace cannot see Launch, so Ada's list is not hers"
    );
}

fn assert_prepared_uploads_are_retired(
    mock: &Mock,
    account: &AtomicU64,
    token: &Mutex<String>,
    works: &str,
) {
    // An upload prepared for one account is not sent for the next: the
    // prepare reply arrives after the account changed.
    *token.lock().unwrap() = "nett-fixture-access".to_owned();
    account.store(5, Ordering::SeqCst);
    Client::new().unwrap().me().unwrap();
    nett::select_organisation(works);
    let client = Client::new().unwrap();
    let draft = UploadDraft::new(
        "notes.txt".into(),
        "text/plain".into(),
        b"Prepared for the previous account.\n".to_vec(),
    )
    .unwrap();
    let response = late_reply(
        mock,
        "POST",
        &format!("/api/v1/orgs/{works}/chat/uploads/prepare"),
        move || client.create_chat_upload(&draft),
        || account.store(6, Ordering::SeqCst),
    );
    assert_eq!(response.err(), Some(UploadError::Access));
    assert_eq!(
        mock.requests("PUT", "/u/*"),
        0,
        "the upload URL was never used"
    );

    // Comment media prepared for one account is not sent for the next.
    Client::new().unwrap().me().unwrap();
    nett::select_organisation(works);
    let client = Client::new().unwrap();
    let response = late_reply(
        mock,
        "POST",
        &format!("/api/v1/orgs/{works}/issues/media/prepare"),
        move || comment_upload(&client, 1),
        || account.store(7, Ordering::SeqCst),
    );
    assert_eq!(response.err(), Some(CommentUploadError::SignInNeeded));
    assert_eq!(
        mock.requests("PUT", "/u/*"),
        0,
        "the comment upload URL was never used"
    );
}

fn assert_late_reads_and_grants_are_discarded(mock: &Mock, account: &AtomicU64, works: &str) {
    // A successful issue read is also discarded if the account changes.
    assert!(Client::new().unwrap().me().is_ok());
    assert!(nett::select_organisation(works));
    let client = Client::new().unwrap();
    let response = late_reply(
        mock,
        "GET",
        &format!("/api/v1/orgs/{works}/issues/OPS-1"),
        move || client.issue("OPS-1"),
        || account.store(8, Ordering::SeqCst),
    );
    assert_eq!(response.err(), Some(Error::SignInNeeded));

    // A file grant that arrives after the account changed is neither kept
    // nor redeemed.
    Client::new().unwrap().me().unwrap();
    nett::select_organisation(works);
    let client = Client::new().unwrap();
    let log = client
        .attachments("OPS-1", &subspace_api_client::PageRequest::default())
        .unwrap()
        .items
        .into_iter()
        .find(|m| m.file_name == "restore-log.txt")
        .unwrap();
    let files_before = mock.requests("GET", "/files/*");
    let response = late_reply(
        mock,
        "POST",
        &format!("/api/v1/orgs/{works}/files/grants"),
        move || client.fetch_text(&log, 1024),
        || account.store(9, Ordering::SeqCst),
    );
    assert_eq!(response.err(), Some(Error::SignInNeeded));
    assert_eq!(
        mock.requests("GET", "/files/*"),
        files_before,
        "the grant was never redeemed"
    );
}

fn assert_organisation_switch_discards_old_replies(mock: &Mock, works: &str) -> String {
    // Replies whose bodies arrive after the organisation changed belong to
    // no one: a managed media completion, its checking state, a to-do save
    // and a move preview.
    Client::new().unwrap().me().unwrap();
    let second = nett::snapshot()
        .organisations
        .into_iter()
        .find(|org| org.name == "Second Fixture Org")
        .unwrap()
        .id;
    discard_old_organisation_replies(mock, works, &second);
    second
}

fn assert_switching_organisations_clears_wait(mock: &Mock, works: &str, second: &str) {
    // A day-long wait NETT asks for ends when the person chooses an
    // organisation.
    assert!(nett::select_organisation(works));
    let teams = format!("/api/v1/orgs/{works}/issue-teams");
    mock.hook(
        "config",
        &json!({"failNext": [{"method": "GET", "path": teams, "status": 503,
                              "retryAfter": 86_400, "times": 1}]}),
    );
    let client = Client::new().unwrap();
    let page = subspace_api_client::PageRequest::default();
    assert!(client.projects(&page).is_err());
    assert!(client.projects(&page).is_err(), "paused, not sent");
    assert!(nett::select_organisation(second));
    assert!(
        Client::new().unwrap().projects(&page).is_ok(),
        "sent at once"
    );
}

fn assert_capabilities_are_read_for_each_account(mock: &Mock, account: &AtomicU64, works: &str) {
    // Another account reads its own capability list, even in the same
    // organisation.
    let page = subspace_api_client::PageRequest::default();
    assert!(nett::select_organisation(works));
    Client::new().unwrap().attachments("OPS-1", &page).unwrap();
    let capabilities = format!("/api/v1/orgs/{works}/desktop/capabilities");
    let read = mock.requests("GET", &capabilities);
    account.store(10, Ordering::SeqCst);
    Client::new().unwrap().me().unwrap();
    assert!(nett::select_organisation(works));
    Client::new().unwrap().attachments("OPS-1", &page).unwrap();
    assert_eq!(
        mock.requests("GET", &capabilities),
        read + 1,
        "read again for the new account"
    );
}

fn comment_upload(client: &Client, n: u8) -> Result<IssueMedia, CommentUploadError> {
    let bytes = format!("Comment file {n}.\n").into_bytes();
    let declaration = ImageDeclaration::new("comment.txt", "text/plain", &bytes).unwrap();
    client.upload_comment_media(
        "OPS-1",
        (&uuid_text(n), &uuid_text(n + 1)),
        &declaration,
        &bytes,
    )
}

fn discard_old_organisation_replies(mock: &Mock, works: &str, second: &str) {
    for (ready_at_once, draft, method, route) in [
        (true, 3, "POST", "/from-upload"),
        (false, 5, "GET", "/uploads/*"),
    ] {
        mock.hook("config", &json!({"mediaReadyAtOnce": ready_at_once}));
        assert!(nett::select_organisation(works));
        let reader = Client::new().unwrap();
        let worker = reader.clone();
        let response = late_reply(
            mock,
            method,
            &format!("/api/v1/orgs/{works}/issues/media{route}"),
            move || comment_upload(&worker, draft),
            || {
                // A replay of the same reservation sees ready media. The
                // first request already stored it and no bytes go again.
                let sent = mock.requests("PUT", "/u/*");
                assert_eq!(
                    comment_upload(&reader, draft).unwrap().file_name,
                    "comment.txt"
                );
                assert_eq!(mock.requests("PUT", "/u/*"), sent);
                assert!(nett::select_organisation(second));
            },
        );
        // After sending bytes, the public error asks the caller to check
        // the transfer even if the cause was a retired scope.
        assert_eq!(response.err(), Some(CommentUploadError::Checking));
    }
    discard_saved_todo_reply(mock, works, second);
    discard_move_preview_reply(mock, works, second);
}

fn discard_saved_todo_reply(mock: &Mock, works: &str, second: &str) {
    assert!(nett::select_organisation(works));
    let client = Client::new().unwrap();
    let issue = client.issue("OPS-1").unwrap().fields.id;
    let list = client.issue_lists("OPS-1").unwrap().remove(0);
    let item = client
        .issue_list_items("OPS-1", &list.id)
        .unwrap()
        .into_iter()
        .find(|item| !item.completed)
        .unwrap();
    let path = format!(
        "/api/v1/orgs/{works}/issues/{issue}/lists/{}/items/{}",
        list.id, item.id
    );
    let (worker, edited) = (client.clone(), item.clone());
    let response = late_reply(
        mock,
        "PUT",
        &path,
        move || worker.set_issue_list_item_done("OPS-1", &edited, true),
        || {
            let saved = client
                .issue_list_items("OPS-1", &list.id)
                .unwrap()
                .into_iter()
                .find(|saved| saved.id == item.id)
                .unwrap();
            assert!(saved.completed);
            assert!(saved.version > item.version);
            assert!(nett::select_organisation(second));
        },
    );
    assert_eq!(response.err(), Some(IssueSaveError::SignIn));
}

fn discard_move_preview_reply(mock: &Mock, works: &str, second: &str) {
    assert!(nett::select_organisation(works));
    let client = Client::new().unwrap();
    let web = client
        .all_projects()
        .unwrap()
        .into_iter()
        .find(|team| team.key == "WEB")
        .unwrap()
        .id;
    let root = client.issue("OPS-2").unwrap();
    let version = u32::try_from(root.fields.version).unwrap();
    let id = root.fields.id;
    let response = late_reply(
        mock,
        "POST",
        &format!("/api/v1/orgs/{works}/issues/{id}/move-preview"),
        move || client.preview_issue_move(&id, &web, version),
        || assert!(nett::select_organisation(second)),
    );
    assert_eq!(response.err(), Some(IssueSaveError::SignIn));
}
