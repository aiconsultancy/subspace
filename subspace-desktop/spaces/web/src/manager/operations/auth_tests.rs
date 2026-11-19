use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::TestAppContext;
use subspace_framework::client_github::Client;
use subspace_framework::core::config::Config;
use subspace_framework::core::credentials::{Credentials, Environment, GITHUB_ACCOUNT_KEY, Runner};

// Synthetic CLI account and token. No process or live GitHub endpoint is used.
const LOGIN: &str = "Test-Mixed_emu";

struct TestEnvironment(PathBuf);
impl Environment for TestEnvironment {
    fn var(&self, name: &str) -> Option<String> {
        (name == "GH_CONFIG_DIR").then(|| self.0.to_string_lossy().into_owned())
    }
}

struct ExactCaseRunner;
impl Runner for ExactCaseRunner {
    fn run(&self, program: &str, args: &[&str], _: &[&str], _: Duration) -> Result<String, String> {
        assert_eq!(program, "gh");
        if args == ["auth", "token", "--hostname", "github.com", "--user", LOGIN] {
            Ok("dummy-test-only-nonworking-token".into())
        } else {
            Err("The test CLI accepts only the canonical mixed-case login".into())
        }
    }
}

#[gpui_kit::test]
fn selected_account_preserves_the_saved_cli_spelling(cx: &mut TestAppContext) {
    cx.update(|cx| {
        subspace_framework::dashboard::prefs::init_file(None, cx);
        subspace_framework::dashboard::prefs::set(cx, GITHUB_ACCOUNT_KEY, LOGIN);
        assert_eq!(super::super::selected_account(cx).as_deref(), Some(LOGIN));
    });
}

fn viewer(listener: TcpListener) {
    listener.set_nonblocking(true).unwrap();
    for _ in 0..200 {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // Windows sockets inherit the listener's non-blocking mode.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                assert!(request.starts_with("POST /graphql "));
                let mut length = 0;
                loop {
                    let mut header = String::new();
                    reader.read_line(&mut header).unwrap();
                    if header == "\r\n" {
                        break;
                    }
                    if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:")
                    {
                        length = value.trim().parse().unwrap();
                    }
                }
                reader.read_exact(&mut vec![0; length]).unwrap();
                let body =
                    serde_json::json!({"data":{"viewer":{"id":"test-viewer-id","login":LOGIN}}})
                        .to_string();
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
                return;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("test viewer: {error}"),
        }
    }
}

#[test]
fn binding_resolves_legacy_lowercase_selection_before_requesting_a_cli_token() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(
        root.path().join("hosts.yml"),
        format!("github.com:\n  user: {LOGIN}\n  users:\n    {LOGIN}: {{}}\n"),
    )
    .unwrap();
    let credentials = Credentials::new(
        &Config::default(),
        TestEnvironment(root.path().into()),
        ExactCaseRunner,
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || viewer(listener));
    let result = super::bind(
        Client::new(Arc::new(credentials)).with_api_url(endpoint),
        Some(LOGIN.to_ascii_lowercase()),
    );
    server.join().unwrap();
    let bound = result.unwrap_or_else(|error| panic!("mixed-case CLI account must bind: {error}"));
    assert_eq!(bound.account(), LOGIN.to_ascii_lowercase());
    assert_eq!(bound.account_id(), "test-viewer-id");
}

#[test]
fn account_epoch_ignores_spelling_case_but_still_invalidates_a_different_account() {
    let mut epoch = super::super::model::Epoch::new(Some(LOGIN.into()));
    let ticket = epoch.ticket();
    assert!(!epoch.sync(Some(LOGIN.to_ascii_lowercase())));
    assert!(epoch.accepts(ticket));
    assert!(epoch.sync(Some("test-other-account".into())));
    assert!(!epoch.accepts(ticket));
}
