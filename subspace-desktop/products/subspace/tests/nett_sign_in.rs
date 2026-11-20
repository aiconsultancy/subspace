//! A NETT build's sign-in end to end against the loopback NETT mock: WorkOS
//! device authorisation through the Framework provider, then the NETT API
//! accepting the access token. No real WorkOS or NETT environment is used.

use std::io::{BufRead as _, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use subspace_framework::account::{Cancel, CredentialStore as _, MemoryStore, Provider as _};
use subspace_framework::workos_auth::{Config, SessionManager};

struct Mock(Child, String);

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_mock() -> Mock {
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/nett_api_test_server.py"
    );
    let mut child = Command::new(
        subspace_framework::core::path::find_tool("python3").expect("python3 runs the NETT mock"),
    )
    .args([
        "-I",
        script,
        "--port",
        "0",
        "--auto-approve",
        "--single-org",
    ])
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
        .map(str::to_owned);
    match origin {
        Some(origin) => Mock(child, origin),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the NETT mock did not start: {line:?}");
        }
    }
}

#[test]
fn device_authorisation_signs_in_and_nett_accepts_the_token() {
    let mock = start_mock();
    let config = Config::new("client_00000000000000000000000000")
        .unwrap()
        .with_api_url(&mock.1)
        .unwrap()
        .with_resource("nett-api")
        .unwrap()
        .with_device_authorization();
    let store = Arc::new(MemoryStore::default());
    let provider = SessionManager::new(config, store.clone());

    let started = provider.start().unwrap();
    let code = started.code.clone().expect("the user code to confirm");
    let url = started.url.clone().unwrap();
    assert!(url.starts_with(&format!("{}/device?user_code=", mock.1)));
    assert!(url.ends_with(&code));
    let generation = provider.generation();
    let account = provider
        .finish(started, &Cancel::default(), generation)
        .unwrap();
    assert_eq!(account.username, "ada@example.com");
    assert!(store.load().unwrap().is_some(), "the refresh token is kept");

    let token = provider
        .token_with_validity("nett-api", Duration::from_secs(60))
        .unwrap();
    let client = subspace_api_client::Client::nett_loopback(
        &format!("{}/api", mock.1),
        None,
        Some(token.token),
    )
    .unwrap();
    assert_eq!(client.me().unwrap().display_name, "Ada Fixture");

    // A restart restores the session from the stored refresh token alone.
    let config = Config::new("client_00000000000000000000000000")
        .unwrap()
        .with_api_url(&mock.1)
        .unwrap()
        .with_resource("nett-api")
        .unwrap()
        .with_device_authorization();
    let restarted = SessionManager::new(config, store);
    assert_eq!(restarted.restore().unwrap().unwrap().id, account.id);
}
