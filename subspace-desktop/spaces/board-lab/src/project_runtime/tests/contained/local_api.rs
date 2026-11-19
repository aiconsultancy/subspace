//! Real package/API integration through MXC and the persistent CONNECT proxy.
//! The isolated API fixture is plaintext inside CONNECT. This does not prove
//! TLS or certificate validation, which remains the runtime client's concern.

mod fixture;

use crate::contracts::RuntimeKind;
use crate::environments::EnvironmentManager;
use crate::project::{ProjectDefinition, ProjectStore, ResultValue};
use crate::project_runtime::proxy::DestinationPolicy;
use crate::project_runtime::{Completion, PreparedSession, Session, SessionStatus};
use fixture::{API_EVALUATED_AT_MS, API_SCOPE, ApiFixture};
use std::collections::BTreeMap;
use std::net::{SocketAddr, TcpListener};
use std::time::{Duration, Instant};

const SYNTHETIC_VALUE: &str = "dummy-test-only-selected-value";
const WAIT_TIMEOUT: Duration = Duration::from_secs(10);
const WAIT_POLL: Duration = Duration::from_millis(10);

#[test]
#[ignore = "requires isolated python-alt with packaging24.2; explicit SUBSPACE_RUNTIME_TEST_ROOT/id"]
fn contained_python_package_calls_local_api_over_connect_and_denies_other_destination() {
    let root = std::env::var_os("SUBSPACE_RUNTIME_TEST_ROOT")
        .expect("explicit isolated environments root");
    let id = std::env::var("SUBSPACE_RUNTIME_TEST_ENVIRONMENT")
        .expect("explicit fixture environment id");
    assert_eq!(
        id, "python-alt",
        "this fixture requires the prepared Python packaging environment"
    );
    let manager = EnvironmentManager::new(root.into());
    let info = manager
        .catalog()
        .unwrap()
        .into_iter()
        .find(|info| info.spec.id == id)
        .expect("prepared fixture environment");
    assert!(info.ready && matches!(info.spec.runtime, RuntimeKind::Python));
    let allowed = ApiFixture::start().expect("bounded synthetic API fixture");
    let denied = TcpListener::bind("127.0.0.1:0").unwrap();
    denied.set_nonblocking(true).unwrap();
    let (_source_root, mut prepared) = prepare(
        &manager,
        &id,
        allowed.address(),
        denied.local_addr().unwrap(),
    );
    assert!(prepared.snapshot().definition().destinations.is_empty());
    assert!(DestinationPolicy::new(&[format!("https://{}", allowed.address())]).is_err());
    prepared.policy = DestinationPolicy::local_fixture(allowed.address());
    let session =
        Session::start(prepared).expect("real MXC session, never a host process substitute");
    wait_running(&session);
    let fetch = session.invoke("fetch", 0).unwrap();
    let fetched = completion(&session, &fetch);
    assert!(
        fetched.error().is_none(),
        "authorized API failed: {:?}",
        fetched.error()
    );
    assert_authorized_result(&session, &allowed);
    let previous = session.live_state().unwrap().results().clone();
    let rejected = session.invoke("denied", 0).unwrap();
    let rejected_outcome = completion(&session, &rejected);
    assert!(
        rejected_outcome
            .error()
            .is_some_and(|error| error.contains("403")),
        "expected proxy denial: {:?}",
        rejected_outcome.error()
    );
    assert_eq!(
        denied.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(session.live_state().unwrap().results(), &previous);
    assert_eq!(session.status().unwrap(), SessionStatus::Running);
    session.stop().unwrap();
    assert_eq!(session.status().unwrap(), SessionStatus::Stopped);
}

fn prepare(
    manager: &EnvironmentManager,
    id: &str,
    allowed: SocketAddr,
    denied: SocketAddr,
) -> (tempfile::TempDir, PreparedSession) {
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let mut definition = ProjectDefinition::new("contained-api-fixture", id, "main.py");
    definition.actions = vec!["fetch".into(), "denied".into()];
    definition.environment_keys = vec![
        "SYNTHETIC_VALUE".into(),
        "SYNTHETIC_API_ALLOWED".into(),
        "SYNTHETIC_API_DENIED".into(),
    ];
    let loaded = store
        .create(
            &definition,
            BTreeMap::from([("main.py".into(), include_bytes!("local_api.py").to_vec())]),
        )
        .unwrap();
    let snapshot = store.snapshot(&definition.id, &loaded.revision).unwrap();
    let selected = BTreeMap::from([
        ("SYNTHETIC_VALUE".into(), SYNTHETIC_VALUE.into()),
        ("SYNTHETIC_API_ALLOWED".into(), allowed.to_string()),
        ("SYNTHETIC_API_DENIED".into(), denied.to_string()),
    ]);
    let prepared = PreparedSession::new(manager, snapshot, selected)
        .expect("normal validated source and managed environment preparation");
    (root, prepared)
}

fn wait_running(session: &Session) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    while session.status().unwrap() == SessionStatus::Starting {
        assert!(Instant::now() < deadline, "worker initialization timed out");
        std::thread::sleep(WAIT_POLL);
    }
    assert_eq!(
        session.status().unwrap(),
        SessionStatus::Running,
        "{}",
        session.snapshot().unwrap().diagnostics()
    );
}

fn completion(session: &Session, request: &str) -> Completion {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let mut completed = session.take_completions().unwrap();
        if let Some(completion) = completed.pop() {
            assert!(completed.is_empty(), "unexpected additional completion");
            assert_eq!(completion.request_id(), request);
            return completion;
        }
        assert!(
            Instant::now() < deadline,
            "API action timed out: {}",
            session.snapshot().unwrap().diagnostics()
        );
        std::thread::sleep(WAIT_POLL);
    }
}

fn assert_authorized_result(session: &Session, api: &ApiFixture) {
    let request = api
        .observation()
        .expect("host must observe the actual HTTP API request");
    assert_eq!(request.target, "GET /metrics HTTP/1.1");
    assert_eq!(request.authorization, format!("Bearer {SYNTHETIC_VALUE}"));
    let live = session.live_state().unwrap();
    let result = &live.results()["heading"];
    assert_eq!(result.scope, API_SCOPE);
    assert_eq!(result.evaluated_at_ms, API_EVALUATED_AT_MS);
    let ResultValue::Text { text } = &result.value else {
        panic!("expected API-derived text result");
    };
    let output: serde_json::Value = serde_json::from_str(text).unwrap();
    assert_eq!(output["count"], 17);
    assert_eq!(output["package_version"], "24.2");
    assert_eq!(output["parsed_major"], 1);
    assert!(!text.contains(SYNTHETIC_VALUE));
}
