use super::{replace, store};

#[test]
fn import_previews_names_and_requires_explicit_conflict_approval() {
    let root = tempfile::tempdir().unwrap();
    let settings = store(root.path(), "project").unwrap();
    let saved = settings
        .update(
            settings.metadata().unwrap().revision(),
            replace("API_KEY", "old-fake"),
        )
        .unwrap();
    let preview = settings
        .preview_import("API_KEY=new-fake\nOTHER=value-fake\n")
        .unwrap();
    assert_eq!(preview.names(), ["API_KEY", "OTHER"]);
    assert_eq!(preview.conflicts(), ["API_KEY"]);
    let debug = format!("{preview:?}");
    assert!(!debug.contains("new-fake") && !debug.contains("value-fake"));
    assert!(
        settings
            .apply_import(saved.revision(), preview, &[])
            .is_err()
    );
    assert_eq!(
        settings.selected(&["API_KEY"]).unwrap().into_values()["API_KEY"],
        "old-fake"
    );
    let preview = settings
        .preview_import("API_KEY=new-fake\nOTHER=value-fake\n")
        .unwrap();
    settings
        .apply_import(saved.revision(), preview, &["API_KEY"])
        .unwrap();
    assert_eq!(
        settings.selected(&["API_KEY"]).unwrap().into_values()["API_KEY"],
        "new-fake"
    );
    assert!(settings.preview_import("BAD=$(command)").is_err());
}

#[test]
fn import_preview_cannot_be_applied_to_another_project() {
    let root = tempfile::tempdir().unwrap();
    let first = store(root.path(), "first").unwrap();
    let second = store(root.path(), "second").unwrap();
    let preview = first.preview_import("TOKEN=fake-value").unwrap();
    assert!(second.apply_import(0, preview, &[]).is_err());
    assert!(second.metadata().unwrap().names().is_empty());
}
