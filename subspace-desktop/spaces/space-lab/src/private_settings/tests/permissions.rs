use super::{replace, store};
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};

#[test]
fn private_modes_and_unsafe_existing_entries_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let settings = store(root.path(), "project").unwrap();
    settings
        .update(
            settings.metadata().unwrap().revision(),
            replace("API_KEY", "fake"),
        )
        .unwrap();
    let private = root.path().join("private");
    let project = private.join("project");
    let file = project.join("settings.json");
    assert_eq!(
        fs::metadata(&private).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&project).unwrap().permissions().mode() & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(settings.metadata().is_err());
    fs::remove_file(&file).unwrap();
    let target = root.path().join("target");
    fs::write(&target, "untouched").unwrap();
    symlink(&target, &file).unwrap();
    assert!(settings.update(0, replace("API_KEY", "fake")).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
}
