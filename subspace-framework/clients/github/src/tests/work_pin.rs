//! The pinned work credential: one verified lookup, then every request of
//! the operation with that credential, whatever the account holds by then.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use subspace_core::Secret;
use subspace_core::config::Config;
use subspace_core::credentials::{Credentials, Environment, Runner, SignedInAccounts};
use subspace_core::http::Error;

use super::{Request, serve};
use crate::Client;

/// No environment token, and a gh config directory that holds nothing, so
/// the signed-in accounts below are the only ones.
struct NoGh;

impl Environment for NoGh {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| "/nonexistent/subspace-test-gh".into())
    }
}

struct NoCli;

impl Runner for NoCli {
    fn run(&self, _: &str, _: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        Err("no cli in tests".into())
    }
}

/// A client whose work account `work_emu` is signed in with `token`, and
/// the credential sources, so a test can sign it in again with another.
fn signed_in(url: String, token: &str) -> (Client, Arc<Credentials>) {
    let credentials = Arc::new(
        Credentials::new(&Config::default(), NoGh, NoCli)
            .with_signed_in(SignedInAccounts::memory()),
    );
    credentials
        .sign_in("work_emu", &Secret::new(token))
        .unwrap();
    (
        Client::new(credentials.clone()).with_api_url(url),
        credentials,
    )
}

/// `token-a` belongs to `work_emu`; any other token to another account.
fn reply(request: &Request) -> (u16, String) {
    let line = request.line.as_str();
    if line.starts_with("POST /graphql") {
        let (id, login) = match request.authorization.as_str() {
            "Bearer token-a" => ("U_a", "work_emu"),
            _ => ("U_b", "other_emu"),
        };
        let viewer = json!({"data": {"viewer": {"id": id, "login": login}}});
        return (200, viewer.to_string());
    }
    if line.starts_with("GET /agents/tasks/task-1 ") {
        let task = json!({
            "id": "task-1", "state": "completed",
            "sessions": [{"id": "session-1", "state": "completed"}]
        });
        return (200, task.to_string());
    }
    if line.starts_with("GET /agents/tasks?") {
        return (200, json!({"tasks": []}).to_string());
    }
    if line.starts_with("GET /agents/sessions/session-1/logs ") {
        return (200, String::new());
    }
    (404, json!({"message": "Not Found"}).to_string())
}

/// What the server saw: each request line and its Authorization header.
type Seen = Arc<Mutex<Vec<(String, String)>>>;

fn recording(answer: fn(&Request) -> (u16, String)) -> (String, Seen) {
    let seen: Seen = Arc::new(Mutex::new(Vec::new()));
    let record = seen.clone();
    let url = serve(Arc::new(move |request| {
        record
            .lock()
            .unwrap()
            .push((request.line.clone(), request.authorization.clone()));
        answer(request)
    }));
    (url, seen)
}

/// The account is looked up once, and the list, the detail and the
/// session log then all go out with the credential that lookup verified,
/// though another account's credential replaced it meanwhile. Before the
/// pin, each read resolved the credential again, so the lookups could see
/// one account while a read ran as the other.
#[test]
fn a_pinned_operation_sends_every_request_with_the_credential_it_verified() {
    let (url, seen) = recording(reply);
    let (client, credentials) = signed_in(url.clone(), "token-a");
    client.copilot_api.set(url).unwrap();
    let work = client.pin_work().unwrap();
    assert_eq!((work.login(), work.account_id()), ("work_emu", "U_a"));

    credentials
        .sign_in("work_emu", &Secret::new("token-b"))
        .unwrap();
    work.list_my_copilot_agent_tasks().unwrap();
    let detail = work.get_copilot_agent_task_detail("task-1", None).unwrap();
    assert_eq!(detail.sessions.len(), 1);
    work.copilot_agent_session_log("session-1", 1024).unwrap();

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 4, "one lookup and three reads: {seen:?}");
    assert!(
        seen.iter().all(|(_, auth)| auth == "Bearer token-a"),
        "every request used the pinned credential: {seen:?}"
    );
    assert_eq!(
        seen.iter()
            .filter(|(line, _)| line.starts_with("POST /graphql"))
            .count(),
        1,
        "the account is looked up once per operation"
    );
}

/// A pinned credential GitHub rejects is not swapped for the one the
/// account holds by then: the operation fails, and only the next one
/// resolves a credential again.
#[test]
fn a_rejected_pinned_credential_is_not_retried_with_another() {
    fn rejecting(request: &Request) -> (u16, String) {
        if request.line.starts_with("GET /agents/tasks") {
            return (401, json!({"message": "Bad credentials"}).to_string());
        }
        reply(request)
    }
    let (url, seen) = recording(rejecting);
    let (client, credentials) = signed_in(url, "token-a");
    let work = client.pin_work().unwrap();
    credentials
        .sign_in("work_emu", &Secret::new("token-b"))
        .unwrap();
    let error = work.list_my_copilot_agent_tasks().unwrap_err();
    assert!(
        matches!(error, Error::Status { status: 401, .. }),
        "{error}"
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "no retry: {seen:?}");
    assert!(seen.iter().all(|(_, auth)| auth == "Bearer token-a"));
}

/// A credential verified once this run is pinned again without a lookup,
/// so a cached answer can still be shown with GitHub unreachable. A new
/// token is verified afresh, and is not pinned while that fails.
#[test]
fn a_credential_verified_once_is_pinned_again_without_a_lookup_even_offline() {
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    let url = serve(Arc::new(move |request| {
        // GitHub answers the first request only, then is unreachable.
        if count.fetch_add(1, Ordering::SeqCst) == 0 {
            reply(request)
        } else {
            (503, json!({"message": "unavailable"}).to_string())
        }
    }));
    let (client, credentials) = signed_in(url, "token-a");
    assert_eq!(client.pin_work().unwrap().account_id(), "U_a");
    assert_eq!(client.pin_work().unwrap().account_id(), "U_a");
    assert_eq!(client.work_login().unwrap(), "work_emu");
    assert_eq!(requests.load(Ordering::SeqCst), 1, "one lookup in all");

    credentials
        .sign_in("work_emu", &Secret::new("token-b"))
        .unwrap();
    assert!(
        client.pin_work().is_err(),
        "a new token is never trusted unverified"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
}

/// A credential that belongs to another account than the work account
/// is never pinned.
#[test]
fn a_credential_of_another_account_is_not_pinned() {
    let (url, _) = recording(reply);
    let (client, _) = signed_in(url, "token-b");
    let error = client.pin_work().err().unwrap();
    assert_eq!(
        error.to_string(),
        "The selected work GitHub credential belongs to another account."
    );
}
