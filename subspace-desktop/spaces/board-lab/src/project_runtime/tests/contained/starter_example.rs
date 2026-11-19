//! The browser's actual starter sources and definition through the real adapter.

use crate::environments::EnvironmentManager;
use crate::project::{InputValue, ProjectStore, ResultValue};
use crate::project_browser::example;
use crate::project_runtime::{PreparedSession, Session, SessionStatus};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[test]
#[ignore = "requires an explicitly provisioned isolated Python, Node or Bun environment"]
fn contained_browser_starter_runs_and_updates() {
    let root = std::env::var_os("SUBSPACE_RUNTIME_TEST_ROOT").expect("isolated environments root");
    let id = std::env::var("SUBSPACE_RUNTIME_TEST_ENVIRONMENT").expect("ready environment id");
    let manager = EnvironmentManager::new(root.into());
    let info = manager
        .catalog()
        .unwrap()
        .into_iter()
        .find(|item| item.spec.id == id)
        .expect("prepared fixture environment");
    assert!(info.ready, "environment must be ready");
    let (definition, files) = example("starter-fixture", &info.spec).unwrap();
    let root = tempfile::tempdir().unwrap();
    let store = ProjectStore::new(root.path()).unwrap();
    let loaded = store.create(&definition, files).unwrap();
    let snapshot = store.snapshot(&definition.id, &loaded.revision).unwrap();
    let prepared = PreparedSession::new(&manager, snapshot, BTreeMap::new()).unwrap();
    let (sender, mut changes) = tokio::sync::watch::channel(());
    let session =
        Session::start_with_changes(prepared, sender).expect("real contained persistent session");
    wait_for(&session, || {
        session.status().unwrap() != SessionStatus::Starting
    });
    assert_eq!(
        session.status().unwrap(),
        SessionStatus::Running,
        "{}",
        session.snapshot().unwrap().diagnostics()
    );
    wait_for(&session, || summary(&session).contains("SIMULATED CPU"));
    assert_outputs(&session);
    exercise_controls(&session, &mut changes);
    session.stop().unwrap();
}

fn exercise_controls(session: &Session, changes: &mut tokio::sync::watch::Receiver<()>) {
    let revision = session
        .set_value("mode", InputValue::String("Paused".into()), 0)
        .unwrap();
    wait_for(session, || summary(session).contains("Paused"));
    let revision = session
        .set_value("rate", InputValue::Number(0.1), revision)
        .unwrap();
    wait_for(session, || {
        session.live_state().unwrap().values()["rate"] == InputValue::Number(0.1)
    });
    // Allow the accepted input to reach the worker before checking it stays paused.
    std::thread::sleep(Duration::from_millis(300));
    let paused = sample_count(session);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(sample_count(session), paused);
    let revision = session
        .set_value("mode", InputValue::String("Running".into()), revision)
        .unwrap();
    wait_for(session, || {
        summary(session).contains("Running") && sample_count(session) > paused
    });
    assert_live_cadence(session, changes);
    let revision = session
        .set_value("mode", InputValue::String("Paused".into()), revision)
        .unwrap();
    wait_for(session, || summary(session).contains("Paused"));
    let request = session.invoke("reset", revision).unwrap();
    wait_for(session, || sample_count(session) == 1);
    let completions = session.take_completions().unwrap();
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].request_id(), request);
    assert!(completions[0].error().is_none());
    assert_outputs(session);
}

fn assert_live_cadence(session: &Session, changes: &mut tokio::sync::watch::Receiver<()>) {
    let start = Instant::now();
    let mut updates = 0;
    let mut previous = sample_count(session);
    changes.borrow_and_update();
    // Real 100ms worker publications must reach fresh presentations, rather
    // than ten samples appearing together at the old one-second boundary.
    while start.elapsed() < Duration::from_secs(1) {
        std::thread::sleep(Duration::from_millis(33));
        if !changes.has_changed().unwrap() {
            continue;
        }
        changes.borrow_and_update();
        let snapshot = session.snapshot().unwrap();
        let ResultValue::Table { table } = &snapshot.live_state().results()["rows"].value else {
            panic!("expected live sample table");
        };
        if table.rows.len() > previous {
            updates += 1;
            previous = table.rows.len();
        }
    }
    assert!(
        (8..=11).contains(&updates),
        "expected about 10 updates, got {updates}"
    );
}

fn wait_for(session: &Session, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition() {
        assert!(
            Instant::now() < deadline && session.status().unwrap() != SessionStatus::Failed,
            "starter timed out or failed: {}",
            session.snapshot().unwrap().diagnostics()
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn summary(session: &Session) -> String {
    match session
        .live_state()
        .unwrap()
        .results()
        .get("summary")
        .map(|result| &result.value)
    {
        Some(ResultValue::Text { text }) => text.clone(),
        _ => String::new(),
    }
}

fn sample_count(session: &Session) -> usize {
    let live = session.live_state().unwrap();
    match live.results().get("rows").map(|result| &result.value) {
        Some(ResultValue::Table { table }) => table.rows.len(),
        _ => 0,
    }
}

fn assert_outputs(session: &Session) {
    let live = session.live_state().unwrap();
    assert!(matches!(
        &live.results()["history"].value,
        ResultValue::Chart { .. }
    ));
    let ResultValue::Table { table } = &live.results()["rows"].value else {
        panic!("expected table")
    };
    assert_eq!(
        table.columns,
        [
            "Sample",
            "SIMULATED CPU %",
            "SIMULATED RAM %",
            "SIMULATED disk %"
        ]
    );
    assert!((1..=60).contains(&table.rows.len()));
    assert_eq!(table.rows[0].len(), 4);
}
