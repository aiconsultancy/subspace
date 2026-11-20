//! A NETT build renews a token NETT refuses before it expires, once, and
//! carries on with the new one: the list and the open issue come back and
//! the stream opens again. Its own process, because the backend, the
//! account and the renewal are chosen once.

use serde_json::{Value, json};
use std::io::{BufRead as _, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use subspace_api_client::hub::{Hub, HubStatus, LiveStatus};
use subspace_api_client::{Client, ListQuery, ListTab, nett};
use subspace_framework::core::Secret;

struct Mock {
    child: Child,
    origin: String,
}

impl Mock {
    fn start() -> Self {
        let script =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/nett_api_test_server.py");
        let mut child = Command::new(
            subspace_framework::core::path::find_tool("python3")
                .expect("python3 runs the NETT mock"),
        )
        .arg("-I")
        .arg(&script)
        .args(["--port", "0", "--serve", "updates"])
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

    /// Update streams opened so far.
    fn streams(&self) -> usize {
        let mut response = ureq::get(&format!("{}/__fixture/requests", self.origin))
            .call()
            .unwrap();
        let log: Vec<Value> =
            serde_json::from_str(&response.body_mut().read_to_string().unwrap()).unwrap();
        log.iter()
            .filter(|r| r["method"] == "GET" && r["path"] == "/api/v1/updates")
            .count()
    }

    fn hook(&self, path: &str, body: &Value) {
        ureq::post(&format!("{}/__fixture/{path}", self.origin))
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .unwrap();
    }
}

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn wait(what: &str, done: &dyn Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_refused_token_is_renewed_once_and_the_stream_carries_on() {
    let mock = Mock::start();
    mock.hook(
        "tokens",
        &json!({"token": "nett-fixture-first", "user": "ada"}),
    );
    for renewed in ["nett-fixture-renewed", "nett-fixture-third"] {
        mock.hook("tokens", &json!({"token": renewed, "user": "ada"}));
    }
    let token = Arc::new(Mutex::new("nett-fixture-first".to_owned()));
    let renewals = Arc::new(AtomicUsize::new(0));
    subspace_api_client::configure_nett_origin(&format!("{}/api", mock.origin)).unwrap();
    {
        let token = token.clone();
        subspace_api_client::use_native_account(
            || 1,
            |_| None,
            move || Ok(Secret::new(token.lock().unwrap().clone())),
        );
    }
    {
        let (token, renewals) = (token.clone(), renewals.clone());
        // The provider's renewal: a fresh token from now on.
        subspace_api_client::use_token_renewal(move || {
            let n = renewals.fetch_add(1, Ordering::SeqCst);
            *token.lock().unwrap() =
                ["nett-fixture-renewed", "nett-fixture-third"][n.min(1)].to_owned();
            true
        });
    }
    Client::new().unwrap().me().unwrap();
    let works = nett::snapshot()
        .organisations
        .into_iter()
        .find(|org| org.name == "Fixture Works")
        .unwrap()
        .id;
    assert!(nett::select_organisation(&works));
    let hub = Hub::from_client(Client::new().unwrap(), "NETT");
    hub.activate(ListQuery::new(ListTab::All).project(Some("OPS")))
        .unwrap();
    hub.open_issue("OPS-1", subspace_api_client::PROFILE_ID)
        .unwrap();
    let shown = |hub: &Hub| {
        let snapshot = hub.snapshot();
        snapshot.live() == LiveStatus::Live
            && snapshot.status() == &HubStatus::Ready
            && snapshot.list().is_some()
            && snapshot
                .issue()
                .is_some_and(|issue| issue.detail().is_some())
    };
    wait("a live stream, the list and the issue", &|| shown(&hub));
    // NETT revokes the token before it expires: the stream closes, and the
    // reads that catch up are refused. The token is renewed once, the list
    // and the issue are read again with it, and the stream opens again.
    // Nothing asks the person to sign in.
    let cleared = Arc::new(AtomicUsize::new(0));
    let watching = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let watcher = {
        let (hub, cleared, watching) = (hub.clone(), cleared.clone(), watching.clone());
        std::thread::spawn(move || {
            while watching.load(Ordering::SeqCst) {
                let snapshot = hub.snapshot();
                let issue = snapshot
                    .issue()
                    .is_some_and(|issue| issue.detail().is_some());
                if snapshot.list().is_none()
                    || !issue
                    || matches!(snapshot.status(), HubStatus::Error(_))
                {
                    cleared.fetch_add(1, Ordering::SeqCst);
                }
                std::thread::sleep(Duration::from_millis(2));
            }
        })
    };
    mock.hook("tokens/revoke", &json!({"token": "nett-fixture-first"}));
    wait("the renewal", &|| renewals.load(Ordering::SeqCst) == 1);
    wait("the stream opened again", &|| mock.streams() >= 2);
    wait("everything back", &|| shown(&hub));
    std::thread::sleep(Duration::from_millis(500));
    watching.store(false, Ordering::SeqCst);
    watcher.join().unwrap();
    assert_eq!(renewals.load(Ordering::SeqCst), 1, "renewed once");
    assert!(shown(&hub), "the list, the issue and the stream stay");
    assert_eq!(
        cleared.load(Ordering::SeqCst),
        0,
        "nothing was cleared and no sign-in was asked for"
    );

    // The renewed token is refused too, soon after: the reads that catch
    // up count that as a failed renewal and clear what is shown. The
    // stream's own renewal then succeeds, and the list and the issue are
    // read again with the new token. Past the window in which one renewal
    // stands for the next, so the provider is asked again.
    std::thread::sleep(Duration::from_secs(6));
    mock.hook("tokens/revoke", &json!({"token": "nett-fixture-renewed"}));
    wait("the second renewal", &|| {
        renewals.load(Ordering::SeqCst) == 2
    });
    wait("the stream opened a third time", &|| mock.streams() >= 3);
    wait("everything back again", &|| shown(&hub));
    hub.stop();
}
