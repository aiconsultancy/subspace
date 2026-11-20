//! A NETT build's client bound to the native account, against the loopback
//! NETT mock: organisation discovery, the organisation menu, private caches
//! and prepared uploads belong to the account that read them. Its own
//! process, because the backend and the account are chosen once.

use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use subspace_api_client::chat_uploads::{UploadDraft, UploadError};
use subspace_api_client::{Client, ListQuery, ListTab, nett};
use subspace_framework::core::Secret;

struct Mock {
    child: Child,
    origin: String,
}

impl Mock {
    fn start(features: &[&str]) -> Self {
        let script =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/nett_api_test_server.py");
        let mut child = Command::new(
            subspace_framework::core::path::find_tool("python3")
                .expect("python3 runs the NETT mock"),
        )
        .arg("-I")
        .arg(&script)
        .args(["--port", "0"])
        .args(features.iter().flat_map(|feature| ["--serve", feature]))
        .env_clear()
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("python3 runs the NETT mock");
        let mut line = String::new();
        if let Some(stdout) = child.stdout.take() {
            let _ = BufReader::new(stdout).read_line(&mut line);
        }
        let origin = line
            .trim()
            .strip_prefix("NETT fixture listening on ")
            .map(str::to_owned)
            .expect("the NETT mock started");
        Self { child, origin }
    }

    fn hook(&self, path: &str, body: &Value) -> Value {
        let mut response = ureq::post(&format!("{}/__fixture/{path}", self.origin))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .unwrap();
        serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap_or(Value::Null)
    }

    fn requests(&self, method: &str, prefix: &str) -> usize {
        let mut response = ureq::get(&format!("{}/__fixture/requests", self.origin))
            .call()
            .unwrap();
        let log: Vec<Value> =
            serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap();
        log.iter()
            .filter(|r| {
                r["method"] == method && r["path"].as_str().is_some_and(|p| p.starts_with(prefix))
            })
            .count()
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
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
    let mock = Mock::start(&["chatUploads", "attachments", "issueMoves"]);
    let account = Arc::new(AtomicU64::new(1));
    let token = Arc::new(Mutex::new("nett-fixture-access".to_owned()));
    subspace_api_client::configure_nett_origin(&format!("{}/api", mock.origin)).unwrap();
    {
        let (account, token) = (account.clone(), token.clone());
        subspace_api_client::use_native_account(
            move || account.load(Ordering::SeqCst),
            |_| None,
            move || Ok(Secret::new(token.lock().unwrap().clone())),
        );
    }

    // A context body that arrives after the account changed belongs to no
    // one: it is dropped, and the menu shows nothing from it.
    mock.hook(
        "config",
        &json!({"slowBody": {"path": "/api/v1/auth/context", "seconds": 1.0, "times": 1}}),
    );
    let reading = std::thread::spawn(|| Client::new().unwrap().me());
    std::thread::sleep(Duration::from_millis(300));
    account.store(2, Ordering::SeqCst);
    assert!(reading.join().unwrap().is_err());
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
    assert!(nett::select_organisation(&works));
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
    nett::select_organisation(&works);
    let grace = Client::new().unwrap();
    assert!(
        identifiers(&grace, &platform).is_empty(),
        "Grace cannot see Launch, so Ada's list is not hers"
    );

    // An upload prepared for one account is not sent for the next: the
    // prepare reply arrives after the account changed.
    *token.lock().unwrap() = "nett-fixture-access".to_owned();
    account.store(5, Ordering::SeqCst);
    Client::new().unwrap().me().unwrap();
    nett::select_organisation(&works);
    let client = Client::new().unwrap();
    mock.hook(
        "config",
        &json!({"slowBody": {"path": format!("/api/v1/orgs/{works}/chat/uploads/prepare"),
                             "seconds": 1.0, "times": 1}}),
    );
    let draft = UploadDraft::new(
        "notes.txt".into(),
        "text/plain".into(),
        b"Prepared for the previous account.\n".to_vec(),
    )
    .unwrap();
    let uploading = std::thread::spawn(move || client.create_chat_upload(&draft));
    std::thread::sleep(Duration::from_millis(300));
    account.store(6, Ordering::SeqCst);
    assert_eq!(uploading.join().unwrap().err(), Some(UploadError::Access));
    assert_eq!(
        mock.requests("PUT", "/u/"),
        0,
        "the upload URL was never used"
    );

    // Comment media prepared for one account is not sent for the next.
    Client::new().unwrap().me().unwrap();
    nett::select_organisation(&works);
    let client = Client::new().unwrap();
    mock.hook(
        "config",
        &json!({"slowBody": {"path": format!("/api/v1/orgs/{works}/issues/media/prepare"),
                             "seconds": 1.0, "times": 1}}),
    );
    let bytes = b"Prepared for the previous account.\n".to_vec();
    let declaration =
        subspace_api_client::uploads::ImageDeclaration::new("comment.txt", "text/plain", &bytes)
            .unwrap();
    let draft = uuid_text(1);
    let key = uuid_text(2);
    let uploading = std::thread::spawn(move || {
        client.upload_comment_media("OPS-1", (&draft, &key), &declaration, &bytes)
    });
    std::thread::sleep(Duration::from_millis(300));
    account.store(9, Ordering::SeqCst);
    assert!(uploading.join().unwrap().is_err());
    assert_eq!(
        mock.requests("PUT", "/u/"),
        0,
        "the comment upload URL was never used"
    );

    // Any organisation read whose body arrives after the account changed
    // is dropped, not handed to its caller.
    Client::new().unwrap().me().unwrap();
    nett::select_organisation(&works);
    let client = Client::new().unwrap();
    mock.hook(
        "config",
        &json!({"slowBody": {"path": format!("/api/v1/orgs/{works}/issues/OPS-1"),
                             "seconds": 1.0, "times": 1}}),
    );
    let reading = std::thread::spawn(move || client.issue("OPS-1"));
    std::thread::sleep(Duration::from_millis(300));
    account.store(7, Ordering::SeqCst);
    assert!(
        reading.join().unwrap().is_err(),
        "the late issue is not returned"
    );

    // A file grant that arrives after the account changed is neither kept
    // nor redeemed.
    Client::new().unwrap().me().unwrap();
    nett::select_organisation(&works);
    let client = Client::new().unwrap();
    let log = client
        .attachments("OPS-1", &subspace_api_client::PageRequest::default())
        .unwrap()
        .items
        .into_iter()
        .find(|m| m.file_name == "restore-log.txt")
        .unwrap();
    mock.hook(
        "config",
        &json!({"slowBody": {"path": format!("/api/v1/orgs/{works}/files/grants"),
                             "seconds": 1.0, "times": 1}}),
    );
    let files_before = mock.requests("GET", "/files/");
    let fetching = std::thread::spawn(move || client.fetch_text(&log, 1024));
    std::thread::sleep(Duration::from_millis(300));
    account.store(8, Ordering::SeqCst);
    assert!(
        fetching.join().unwrap().is_err(),
        "the late grant gives no bytes"
    );
    assert_eq!(
        mock.requests("GET", "/files/"),
        files_before,
        "the grant was never redeemed"
    );

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
    // The organisation changes once NETT has answered the slow request,
    // while its body is still on the way.
    let late = |method: &str, path: String, call: Box<dyn FnOnce(Client) -> bool + Send>| {
        assert!(nett::select_organisation(&works));
        let client = Client::new().unwrap();
        mock.hook(
            "config",
            &json!({"slowBody": {"path": path, "seconds": 1.5, "times": 1}}),
        );
        let prefix = path.trim_end_matches('*').to_owned();
        let before = mock.requests(method, &prefix);
        let calling = std::thread::spawn(move || call(client));
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while mock.requests(method, &prefix) == before {
            assert!(std::time::Instant::now() < deadline, "no request to {path}");
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(nett::select_organisation(&second));
        assert!(
            calling.join().unwrap(),
            "the late reply was not used: {path}"
        );
    };
    let comment = |n: u8| {
        Box::new(move |client: Client| {
            let bytes = format!("Comment file {n}.\n").into_bytes();
            let declaration = subspace_api_client::uploads::ImageDeclaration::new(
                "comment.txt",
                "text/plain",
                &bytes,
            )
            .unwrap();
            client
                .upload_comment_media(
                    "OPS-1",
                    (&uuid_text(n), &uuid_text(n + 1)),
                    &declaration,
                    &bytes,
                )
                .is_err()
        }) as Box<dyn FnOnce(Client) -> bool + Send>
    };
    // NETT may answer a completion with the ready media at once.
    mock.hook("config", &json!({"mediaReadyAtOnce": true}));
    late(
        "POST",
        format!("/api/v1/orgs/{works}/issues/media/from-upload"),
        comment(3),
    );
    mock.hook("config", &json!({"mediaReadyAtOnce": false}));
    late(
        "GET",
        format!("/api/v1/orgs/{works}/issues/media/uploads/*"),
        comment(5),
    );
    assert!(nett::select_organisation(&works));
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
    late(
        "PUT",
        path,
        Box::new(move |client: Client| {
            client
                .set_issue_list_item_done("OPS-1", &item, true)
                .is_err()
        }),
    );
    assert!(nett::select_organisation(&works));
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
    late(
        "POST",
        format!("/api/v1/orgs/{works}/issues/{id}/move-preview"),
        Box::new(move |client: Client| client.preview_issue_move(&id, &web, version).is_err()),
    );

    // A day-long wait NETT asks for ends when the person chooses an
    // organisation.
    assert!(nett::select_organisation(&works));
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
    assert!(nett::select_organisation(&second));
    assert!(
        Client::new().unwrap().projects(&page).is_ok(),
        "sent at once"
    );

    // Another account reads its own capability list, even in the same
    // organisation.
    assert!(nett::select_organisation(&works));
    Client::new().unwrap().attachments("OPS-1", &page).unwrap();
    let capabilities = format!("/api/v1/orgs/{works}/desktop/capabilities");
    let read = mock.requests("GET", &capabilities);
    account.store(10, Ordering::SeqCst);
    Client::new().unwrap().me().unwrap();
    assert!(nett::select_organisation(&works));
    Client::new().unwrap().attachments("OPS-1", &page).unwrap();
    assert_eq!(
        mock.requests("GET", &capabilities),
        read + 1,
        "read again for the new account"
    );
}
