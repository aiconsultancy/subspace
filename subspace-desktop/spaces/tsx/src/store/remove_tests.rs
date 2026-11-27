//! Remove, and the files the store refuses as copies of an app.

use std::path::Path;

use super::removal::Removal;
use super::removal::one_folder;
use super::scan::Unverified;
use super::tests::{file, first_build, signed};
use super::{Installed, Paths, install, installed, remove};
use crate::fixtures::{Signer, package};

/// The scan's entry for `file`.
fn scanned(paths: &Paths, roots: &[String], file: &Path) -> Installed {
    installed(paths, roots)
        .into_iter()
        .find(|package| package.file == file)
        .unwrap()
}

/// Installs `dev.example.other` with storage, and returns the storage.
fn other_app_with_storage(folder: &Path, paths: &Paths, signer: &Signer) -> std::path::PathBuf {
    let source = file(
        folder,
        "other.nshpkg",
        &package(signer, "dev.example.other", 1, false),
    );
    let other = install(paths, std::slice::from_ref(&signer.root), &source).unwrap();
    let data = paths.data("dev.example.other", &other.identity.key_id);
    std::fs::create_dir_all(&data).unwrap();
    data
}

/// Puts build `build` of `dev.example.app`, signed by `signer`, at `target`.
fn copy_in(folder: &Path, signer: &Signer, build: u64, target: &Path) {
    let source = signed(folder, &format!("{build}.nshpkg"), signer, build);
    std::fs::copy(source, target).unwrap();
}

#[test]
fn removing_a_package_whose_root_is_no_longer_trusted_removes_only_the_file() {
    let (_folder, paths, _signer, first) = first_build();
    let data = paths.data("dev.example.app", &first.identity.key_id);
    std::fs::create_dir_all(&data).unwrap();
    // The root is no longer trusted: nothing authenticates the package.
    let refused = installed(&paths, &[crate::fixtures::signer().root]);
    assert!(refused[0].state.is_err());
    assert_eq!(refused[0].app_id, None);
    remove(&paths, &refused[0].file, refused[0].app_id.as_deref()).unwrap();
    assert!(!refused[0].file.exists());
    assert!(
        data.exists(),
        "storage stays until removed from a trusted state"
    );
}

#[test]
fn an_older_build_of_one_app_at_another_apps_name_is_removed_alone() {
    let (folder, paths, signer, first) = first_build();
    let roots = [signer.root.clone()];
    // dev.example.app build 2 is accepted, so build 1 is now a rollback.
    let second = install(
        &paths,
        &roots,
        &signed(folder.path(), "2.nshpkg", &signer, 2),
    )
    .unwrap();
    super::accept(&paths, &roots, &second).unwrap();
    let data = other_app_with_storage(folder.path(), &paths, &signer);
    // The old build of dev.example.app, at dev.example.other's name.
    let other_file = paths.package_file("dev.example.other");
    copy_in(folder.path(), &signer, 1, &other_file);
    let copy = scanned(&paths, &roots, &other_file);
    assert!(copy.state.is_err());
    assert_eq!(copy.app_id.as_deref(), Some("dev.example.app"));
    remove(&paths, &copy.file, copy.app_id.as_deref()).unwrap();
    assert!(!other_file.exists());
    assert!(data.exists(), "the other app's storage is kept");
    assert!(paths.root.join("assets").join("dev.example.other").exists());
    assert!(paths.package_file("dev.example.app").exists());
    assert!(!first.identity.install.is_empty());
}

#[test]
fn removing_an_unrecorded_or_odd_file_removes_only_that_file() {
    let (folder, paths, signer, first) = first_build();
    let data = paths.data("dev.example.app", &first.identity.key_id);
    std::fs::create_dir_all(&data).unwrap();
    let packages = paths.package_file("x");
    let packages = packages.parent().unwrap();
    for name in ["..nshpkg", "...nshpkg", "other.app.nshpkg", "copy.nshpkg"] {
        let stray = packages.join(name);
        std::fs::write(&stray, b"not a package").unwrap();
        remove(&paths, &stray, None).unwrap();
        assert!(!stray.exists());
    }
    // A copy of the app under another name verifies as the app, but is not
    // its file: only the copy goes.
    let copy = packages.join("copy.nshpkg");
    copy_in(folder.path(), &signer, 1, &copy);
    remove(&paths, &copy, Some("dev.example.app")).unwrap();
    // A file named after the app, removed as another verified app.
    let other = paths.package_file("dev.example.other");
    std::fs::write(&other, b"x").unwrap();
    remove(&paths, &other, Some("dev.example.app")).unwrap();
    assert!(data.exists(), "the app's storage is kept");
    assert!(paths.package_file("dev.example.app").exists());
    assert!(paths.root.join("assets").join("dev.example.app").exists());
}

#[test]
fn only_a_single_folder_name_is_cleaned_up() {
    for name in ["", ".", "..", "a/b", "a\\b", "/a"] {
        assert!(!one_folder(name), "{name:?}");
    }
    assert!(one_folder("dev.example.app"));
}

#[test]
fn a_second_file_for_an_installed_app_is_refused() {
    let (folder, paths, signer, first) = first_build();
    let other = crate::fixtures::signer();
    let roots = [signer.root.clone(), other.root.clone()];
    let copy = paths
        .package_file("dev.example.app")
        .with_file_name("alias.nshpkg");
    copy_in(folder.path(), &other, 2, &copy);
    let alias = scanned(&paths, &roots, &copy);
    assert!(
        alias
            .state
            .as_ref()
            .err()
            .unwrap()
            .contains("Remove this copy")
    );
    let again = installed(&paths, &roots);
    let kept = again
        .iter()
        .find(|package| package.file == paths.package_file("dev.example.app"))
        .unwrap();
    assert_eq!(
        kept.state.as_ref().unwrap().identity.install,
        first.identity.install
    );
}

#[test]
fn removing_a_copy_named_after_another_recorded_app_removes_only_the_copy() {
    let (folder, paths, signer, _) = first_build();
    let roots = [signer.root.clone()];
    let data = other_app_with_storage(folder.path(), &paths, &signer);
    // Its file is overwritten with dev.example.app: a copy under its name.
    let other_file = paths.package_file("dev.example.other");
    copy_in(folder.path(), &signer, 1, &other_file);
    let copy = scanned(&paths, &roots, &other_file);
    assert!(copy.state.is_err());
    remove(&paths, &copy.file, copy.app_id.as_deref()).unwrap();
    assert!(!other_file.exists());
    assert!(data.exists(), "the other app's storage is kept");
    assert!(paths.root.join("assets").join("dev.example.other").exists());
    assert!(paths.package_file("dev.example.app").exists());
}

#[test]
fn the_scan_tells_each_kind_of_removal_apart() {
    let (folder, paths, signer, _) = first_build();
    let roots = [signer.root.clone()];
    let own = paths.package_file("dev.example.app");
    assert_eq!(
        super::removal(&paths, &scanned(&paths, &roots, &own)),
        Removal::Owned
    );
    let copy = own.with_file_name("copy.nshpkg");
    copy_in(folder.path(), &signer, 1, &copy);
    assert_eq!(
        super::removal(&paths, &scanned(&paths, &roots, &copy)),
        Removal::Copy
    );
    let damaged = own.with_file_name("damaged.nshpkg");
    std::fs::write(&damaged, b"NSHP not a package").unwrap();
    assert_eq!(
        super::removal(&paths, &scanned(&paths, &roots, &damaged)),
        Removal::Unverified(Unverified::Damaged)
    );
    // Its root no longer trusted.
    let untrusted = [crate::fixtures::signer().root];
    assert_eq!(
        super::removal(&paths, &scanned(&paths, &untrusted, &own)),
        Removal::Unverified(Unverified::Untrusted)
    );
    // No root trusted at all.
    assert_eq!(
        super::removal(&paths, &scanned(&paths, &[], &own)),
        Removal::Unverified(Unverified::Untrusted)
    );
}
