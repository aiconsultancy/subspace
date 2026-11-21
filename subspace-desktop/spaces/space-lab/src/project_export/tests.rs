use super::export_project;
use crate::project::{Component, ComponentKind, ProjectDefinition, limits};
use std::fs;
use std::io::Read as _;
use std::path::Path;

fn project(root: &Path) -> Vec<String> {
    let mut definition = ProjectDefinition::new("share-test", "python-default", "main.py");
    definition.components.push(Component::new(
        "summary",
        "Summary",
        ComponentKind::Text {
            text: "Ready".into(),
        },
    ));
    definition
        .destinations
        .push("https://api.example.com".into());
    definition.environment_keys.push("API_TOKEN".into());
    fs::write(
        root.join("dashboard.json"),
        serde_json::to_vec(&definition).expect("definition bytes"),
    )
    .expect("test definition");
    fs::write(root.join("main.py"), "print('test-only public source')\n").expect("test source");
    vec!["dashboard.json".into(), "main.py".into()]
}

#[test]
fn only_explicit_source_artifacts_are_archived() {
    let directory = tempfile::tempdir().expect("test directory");
    let files = project(directory.path());
    fs::create_dir(directory.path().join("private")).expect("private fixture");
    fs::write(
        directory.path().join("private/settings.json"),
        "test-only-local-token",
    )
    .expect("private fixture");
    fs::write(directory.path().join(".env"), "TOKEN=test-only-local-token")
        .expect("dotenv fixture");
    fs::write(directory.path().join("worker.log"), "test-only-local-token").expect("log fixture");
    let archive_directory = tempfile::tempdir().expect("archive destination");
    let output = archive_directory.path().join("project.zip");
    export_project(
        directory.path(),
        &files,
        &output,
        &["test-only-local-token".into()],
    )
    .expect("safe export");
    let mut archive = zip::ZipArchive::new(fs::File::open(output).expect("archive")).expect("zip");
    assert_eq!(archive.len(), 2);
    assert_eq!(
        archive
            .file_names()
            .collect::<Result<Vec<_>, _>>()
            .expect("entry names"),
        ["dashboard.json", "main.py"]
    );
    let mut source = String::new();
    archive
        .by_name("main.py")
        .expect("source entry")
        .read_to_string(&mut source)
        .expect("source bytes");
    assert!(source.contains("test-only public source"));
    assert!(!source.contains("test-only-local-token"));
    let mut definition = Vec::new();
    archive
        .by_name("dashboard.json")
        .expect("definition entry")
        .read_to_end(&mut definition)
        .expect("definition bytes");
    let shared = ProjectDefinition::parse(&definition).expect("valid shared definition");
    assert!(
        shared.destinations.is_empty(),
        "Sharing must not retain execution grants"
    );
    assert_eq!(shared.environment_keys, ["API_TOKEN"]);
}

#[test]
fn forbidden_paths_and_credentials_are_not_exported_or_overwritten() {
    let directory = tempfile::tempdir().expect("test directory");
    let files = project(directory.path());
    let output = directory.path().join("project.zip");
    for forbidden in [
        "../outside.py",
        "/outside.py",
        ".env",
        "private/settings.json",
        "node_modules/library.js",
    ] {
        let mut selected = files.clone();
        selected.push(forbidden.into());
        assert!(export_project(directory.path(), &selected, &output, &[]).is_err());
        assert!(!output.exists());
    }
    fs::write(
        directory.path().join("main.py"),
        "TOKEN='test-only-hardcoded-value'",
    )
    .expect("unsafe source fixture");
    let error = export_project(
        directory.path(),
        &files,
        &output,
        &["test-only-hardcoded-value".into()],
    )
    .expect_err("known credential in source");
    assert!(!error.to_string().contains("test-only-hardcoded-value"));
    assert!(!output.exists());
    fs::write(directory.path().join("main.py"), "print('safe source')").expect("safe source");
    fs::write(&output, "keep this existing destination").expect("existing output");
    assert!(export_project(directory.path(), &files, &output, &[]).is_err());
    assert_eq!(
        fs::read_to_string(output).expect("unchanged destination"),
        "keep this existing destination"
    );
}

#[test]
fn export_enforces_the_declared_file_set_and_resource_budgets() {
    let directory = tempfile::tempdir().expect("test directory");
    let files = project(directory.path());
    let output = directory.path().join("project.zip");
    assert!(export_project(directory.path(), &["dashboard.json".into()], &output, &[]).is_err());
    let mut duplicate = files.clone();
    duplicate.push("main.py".into());
    assert!(export_project(directory.path(), &duplicate, &output, &[]).is_err());
    fs::write(
        directory.path().join("main.py"),
        vec![b'x'; limits::FILE_BYTES + 1],
    )
    .expect("oversized source");
    assert!(export_project(directory.path(), &files, &output, &[]).is_err());
    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn linked_source_cannot_escape_the_project() {
    use std::os::unix::fs::symlink;
    let directory = tempfile::tempdir().expect("test directory");
    let mut files = project(directory.path());
    let outside = tempfile::tempdir().expect("outside fixture");
    fs::write(
        outside.path().join("outside.py"),
        "test-only private source",
    )
    .expect("outside file");
    symlink(
        outside.path().join("outside.py"),
        directory.path().join("linked.py"),
    )
    .expect("linked source");
    files.push("linked.py".into());
    assert!(
        export_project(
            directory.path(),
            &files,
            &directory.path().join("project.zip"),
            &[]
        )
        .is_err()
    );
    assert!(!directory.path().join("project.zip").exists());
}
