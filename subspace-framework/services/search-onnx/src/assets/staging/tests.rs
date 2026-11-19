use super::{Installer, Stage};
use std::{fs, io::Write};

#[test]
fn abandoned_slots_are_recovered_without_unrelated_deletions() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    fs::write(root.join("keep-me"), "unrelated").unwrap();
    for _ in 0..3 {
        fs::write(root.join(".model.onnx.part"), "abandoned").unwrap();
        let _installer = Installer::acquire(root).unwrap();
        assert!(!root.join(".model.onnx.part").exists());
        let mut stage = Stage::create(root, "model.onnx").unwrap();
        stage.file.write_all(b"partial").unwrap();
        assert!(Stage::create(root, "model.onnx").is_err());
        drop(stage);
        assert!(!root.join(".model.onnx.part").exists());
    }
    assert_eq!(fs::read_dir(root).unwrap().count(), 2);
    assert_eq!(
        fs::read_to_string(root.join("keep-me")).unwrap(),
        "unrelated"
    );
}

#[test]
fn commit_publishes_bytes_and_removes_the_staging_slot() {
    let temp = tempfile::tempdir().unwrap();
    let _installer = Installer::acquire(temp.path()).unwrap();
    let mut stage = Stage::create(temp.path(), "model.onnx").unwrap();
    stage.file.write_all(b"verified fixture").unwrap();
    let target = temp.path().join("model.onnx");
    stage.commit(&target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"verified fixture");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&target).unwrap().permissions().mode() & 0o077,
            0
        );
        assert_eq!(
            fs::metadata(temp.path().join(".install.lock"))
                .unwrap()
                .permissions()
                .mode()
                & 0o077,
            0
        );
    }
    assert!(!temp.path().join(".model.onnx.part").exists());
}

#[test]
fn independent_installers_cannot_recover_each_others_staging() {
    let temp = tempfile::tempdir().unwrap();
    let first = Installer::acquire(temp.path()).unwrap();
    let stage = Stage::create(temp.path(), "model.onnx").unwrap();
    assert!(Installer::acquire(temp.path()).is_err());
    assert!(temp.path().join(".model.onnx.part").exists());
    drop(stage);
    drop(first);
    assert!(Installer::acquire(temp.path()).is_ok());
}

#[cfg(unix)]
#[test]
fn links_are_rejected_without_touching_their_targets() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("target");
    fs::write(&target, "untouched").unwrap();
    let lock = temp.path().join(".install.lock");
    symlink(&target, &lock).unwrap();
    assert!(Installer::acquire(temp.path()).is_err());
    fs::remove_file(lock).unwrap();
    let slot = temp.path().join(".model.onnx.part");
    symlink(&target, &slot).unwrap();
    assert!(Installer::acquire(temp.path()).is_err());
    assert!(Stage::create(temp.path(), "model.onnx").is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "untouched");
    assert!(fs::symlink_metadata(slot).unwrap().file_type().is_symlink());
}
