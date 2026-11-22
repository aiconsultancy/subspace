use super::{replace, store};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
#[ignore = "spawned only by concurrent_processes_cannot_both_commit_the_same_revision"]
fn concurrent_writer_child() {
    let root = std::env::var_os("PRIVATE_STORE_TEST_ROOT").unwrap();
    let name = std::env::var("PRIVATE_STORE_TEST_NAME").unwrap();
    let root = Path::new(&root);
    let settings = store(root, "project").unwrap();
    fs::write(root.join(format!("ready-{name}")), "ready").unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.join("go").exists() {
        assert!(Instant::now() < deadline, "start signal timeout");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(settings.update(0, replace(&name, "fake-value")).is_ok());
}

#[test]
fn concurrent_processes_cannot_both_commit_the_same_revision() {
    let root = tempfile::tempdir().unwrap();
    let settings = store(root.path(), "project").unwrap();
    let executable = std::env::current_exe().unwrap();
    let spawn = |name: &str| {
        Command::new(&executable)
            .args([
                "--exact",
                "private_settings::tests::concurrency::concurrent_writer_child",
                "--ignored",
            ])
            .env("PRIVATE_STORE_TEST_ROOT", root.path())
            .env("PRIVATE_STORE_TEST_NAME", name)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    };
    let mut first = spawn("FIRST");
    let mut second = spawn("SECOND");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !root.path().join("ready-FIRST").exists() || !root.path().join("ready-SECOND").exists() {
        assert!(Instant::now() < deadline, "children did not become ready");
        std::thread::sleep(Duration::from_millis(10));
    }
    fs::write(root.path().join("go"), "go").unwrap();
    let successes = [
        first.wait().unwrap().success(),
        second.wait().unwrap().success(),
    ];
    assert_eq!(successes.iter().filter(|success| **success).count(), 1);
    let saved = settings.metadata().unwrap();
    assert_eq!(saved.revision(), 1);
    assert_eq!(saved.names().len(), 1);
}
