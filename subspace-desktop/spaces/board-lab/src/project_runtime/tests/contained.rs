//! Opt-in real-adapter evidence. Requires an explicitly provisioned isolated
//! environment. Never substitutes a host Command or a fake process for MXC.

use crate::contracts::RuntimeKind;
use crate::environments::EnvironmentManager;
use crate::project::{InputDeclaration, InputValue, ProjectDefinition, ProjectStore, ResultValue};
use crate::project_runtime::{PreparedSession, Session, SessionStatus};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

mod local_api;
mod starter_example;

fn wait(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "contained fixture condition timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn text(session: &Session) -> String {
    let live = session.live_state().unwrap();
    match live.results().get("heading").map(|result| &result.value) {
        Some(ResultValue::Text { text }) => text.clone(),
        _ => String::new(),
    }
}

fn create(manager: &EnvironmentManager, id: &str) -> (tempfile::TempDir, PreparedSession) {
    let info = manager
        .catalog()
        .unwrap()
        .into_iter()
        .find(|info| info.spec.id == id)
        .expect("explicit ready fixture environment");
    assert!(info.ready, "provision the isolated fixture first");
    let (entry, sibling, source, module) = match info.spec.runtime {
        RuntimeKind::Python => (
            "main.py",
            "sibling.py",
            include_str!("contained.py"),
            "VALUE = 7\n",
        ),
        RuntimeKind::Node | RuntimeKind::Bun => (
            "main.cjs",
            "sibling.cjs",
            include_str!("contained.cjs"),
            "exports.VALUE = 7;\n",
        ),
        RuntimeKind::Shell => panic!("Shell is not a persistent runtime fixture"),
    };
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let definition = definition(id, entry, sibling);
    let loaded = store
        .create(
            &definition,
            BTreeMap::from([
                (entry.into(), source.as_bytes().to_vec()),
                (sibling.into(), module.as_bytes().to_vec()),
            ]),
        )
        .unwrap();
    let snapshot = store.snapshot(&definition.id, &loaded.revision).unwrap();
    let prepared = PreparedSession::new(
        manager,
        snapshot,
        BTreeMap::from([(
            "SYNTHETIC_VALUE".into(),
            "dummy-test-only-selected-value".into(),
        )]),
    )
    .unwrap()
    .with_values(BTreeMap::from([("amount".into(), InputValue::Number(9.0))]))
    .unwrap();
    (root, prepared)
}

fn definition(id: &str, entry: &str, sibling: &str) -> ProjectDefinition {
    let mut definition = ProjectDefinition::new("contained-fixture", id, entry);
    definition.files.push(sibling.into());
    definition.actions = vec!["refresh".into(), "malformed".into()];
    definition.environment_keys.push("SYNTHETIC_VALUE".into());
    definition.inputs.insert(
        "amount".into(),
        InputDeclaration::Number {
            default: 1.0,
            min: 0.0,
            max: 10.0,
            step: 1.0,
        },
    );
    definition
}

#[test]
#[ignore = "requires isolated ready environment; Python packaging or JS semver; explicit root/id env"]
fn project_runtime_contained_retains_packages_values_timers_and_fails_closed_without_restart() {
    let root = std::env::var_os("SUBSPACE_RUNTIME_TEST_ROOT")
        .expect("explicit isolated environments root");
    let id =
        std::env::var("SUBSPACE_RUNTIME_TEST_ENVIRONMENT").expect("explicit ready environment id");
    let manager = EnvironmentManager::new(root.into());
    let (_source_root, prepared) = create(&manager, &id);
    let session = Session::start(prepared).expect("real contained persistent session");
    wait(|| session.status().unwrap() != SessionStatus::Starting);
    assert_eq!(
        session.status().unwrap(),
        SessionStatus::Running,
        "{}",
        session.snapshot().unwrap().diagnostics()
    );
    assert_eq!(
        session.live_state().unwrap().values()["amount"],
        InputValue::Number(9.0)
    );
    wait(|| text(&session) == "timer");
    wait(|| {
        session
            .snapshot()
            .unwrap()
            .diagnostics()
            .contains("[redacted]")
    });
    let temporary_view = session.clone();
    drop(temporary_view);
    for count in 1..=2 {
        let request = session.invoke("refresh", 0).unwrap();
        wait(|| text(&session).starts_with(&format!("count:{count}:")));
        let completions = session.take_completions().unwrap();
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].request_id(), request);
        assert!(completions[0].error().is_none());
    }
    let previous = session.live_state().unwrap().results().clone();
    session.invoke("malformed", 0).unwrap();
    wait(|| session.status().unwrap() == SessionStatus::Failed);
    assert_eq!(session.live_state().unwrap().results(), &previous);
    assert!(session.invoke("refresh", 0).is_err());
    let diagnostic = session.snapshot().unwrap().diagnostics().to_owned();
    assert!(!diagnostic.contains("dummy-test-only-selected-value"));
    session.stop().unwrap();
    assert_eq!(session.status().unwrap(), SessionStatus::Failed);
}
