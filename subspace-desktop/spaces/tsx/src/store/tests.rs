use std::path::Path;

use nsh_engine::BundleCode;
use nsh_package::TrustStore;

use super::{Paths, accept, install, installed, remove};
use crate::fixtures::{package, signer};

fn store() -> (tempfile::TempDir, Paths) {
    let folder = tempfile::tempdir().unwrap();
    let paths = Paths::beside(&folder.path().join("config.json"));
    (folder, paths)
}

pub(super) fn file(folder: &Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
    let path = folder.join(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A file holding build `build` of `dev.example.app`, signed by `signer`.
pub(super) fn signed(
    folder: &Path,
    name: &str,
    signer: &crate::fixtures::Signer,
    build: u64,
) -> std::path::PathBuf {
    file(
        folder,
        name,
        &package(signer, "dev.example.app", build, false),
    )
}

/// Installs `first`, then `second`, both of which must pass.
fn install_both(
    paths: &Paths,
    roots: &[String],
    first: &Path,
    second: &Path,
) -> (super::Verified, super::Verified) {
    (
        install(paths, roots, first).unwrap(),
        install(paths, roots, second).unwrap(),
    )
}

#[test]
fn files_live_beside_the_config_file() {
    let paths = Paths::beside(Path::new("/config/subspace/config.json"));
    assert_eq!(
        paths.data("dev.app", "key"),
        Path::new("/config/subspace/tsx/data/dev.app/key")
    );
}

#[test]
fn with_no_trusted_root_an_install_is_refused_and_nothing_is_kept() {
    let (folder, paths) = store();
    let source = file(folder.path(), "app.nshpkg", b"not a package");
    let refused = install(&paths, &[], &source).err().unwrap();
    assert!(refused.contains("No signing root is trusted"));
    assert!(installed(&paths, &[]).is_empty());
}

#[test]
fn a_package_runs_from_its_source_and_never_its_bytecode() {
    let (folder, paths) = store();
    let signer = signer();
    let source = file(
        folder.path(),
        "app.nshpkg",
        &package(&signer, "dev.example.app", 1, true),
    );
    let verified = install(&paths, std::slice::from_ref(&signer.root), &source).unwrap();
    assert!(matches!(&verified.bundle.code, BundleCode::Source(code) if code.contains("fixture")));
}

#[test]
fn an_update_keeps_its_install_and_an_older_build_is_refused_once_accepted() {
    let (folder, paths) = store();
    let signer = signer();
    let roots = [signer.root.clone()];
    let first = signed(folder.path(), "1.nshpkg", &signer, 1);
    let (one, two) = install_both(
        &paths,
        &roots,
        &first,
        &signed(folder.path(), "2.nshpkg", &signer, 2),
    );
    assert_eq!(one.identity.install, two.identity.install);
    assert!(!two.identity.install.is_empty());
    accept(&paths, &roots, &two).unwrap();
    let refused = install(&paths, &roots, &first).err().unwrap();
    assert!(refused.contains("build"), "{refused}");
    assert_eq!(installed(&paths, &roots).len(), 1);
}

#[test]
fn another_publisher_key_asks_again_and_gets_its_own_storage() {
    let (folder, paths) = store();
    let (a, b) = (signer(), signer());
    let roots = [a.root.clone(), b.root.clone()];
    let (one, two) = install_both(
        &paths,
        &roots,
        &signed(folder.path(), "a.nshpkg", &a, 1),
        &signed(folder.path(), "b.nshpkg", &b, 2),
    );
    assert_ne!(one.identity.install, two.identity.install);
    assert_ne!(
        paths.data("dev.example.app", &one.identity.key_id),
        paths.data("dev.example.app", &two.identity.key_id)
    );
}

#[test]
fn removing_and_installing_again_asks_again() {
    let (folder, paths) = store();
    let signer = signer();
    let roots = [signer.root.clone()];
    let source = signed(folder.path(), "app.nshpkg", &signer, 1);
    let first = install(&paths, &roots, &source).unwrap();
    let installed_file = paths.package_file("dev.example.app");
    remove(&paths, &installed_file, Some("dev.example.app")).unwrap();
    assert!(!paths.root.join("assets").join("dev.example.app").exists());
    let again = install(&paths, &roots, &source).unwrap();
    assert_ne!(first.identity.install, again.identity.install);
}

#[test]
fn concurrent_accepts_keep_every_build() {
    let (folder, paths) = store();
    let signer = signer();
    let roots = vec![signer.root.clone()];
    let verified: Vec<_> = (0..6)
        .map(|index| {
            let id = format!("dev.example.app{index}");
            let source = file(
                folder.path(),
                &format!("{index}.nshpkg"),
                &package(&signer, &id, 3, false),
            );
            install(&paths, &roots, &source).unwrap()
        })
        .collect();
    std::thread::scope(|scope| {
        for package in &verified {
            let (paths, roots) = (&paths, &roots);
            scope.spawn(move || accept(paths, roots, package).unwrap());
        }
    });
    let trust = TrustStore::open(paths.root.join("trust")).unwrap();
    for index in 0..6 {
        assert_eq!(
            trust.accepted_build(&format!("dev.example.app{index}")),
            Some(3)
        );
    }
}

#[test]
fn a_failed_replacement_keeps_the_installed_build() {
    let (folder, paths) = store();
    let signer = signer();
    let roots = [signer.root.clone()];
    let first = signed(folder.path(), "1.nshpkg", &signer, 1);
    install(&paths, &roots, &first).unwrap();
    let before = std::fs::read(paths.package_file("dev.example.app")).unwrap();
    // A damaged update is refused before anything is written.
    let mut damaged = package(&signer, "dev.example.app", 2, false);
    let last = damaged.len() - 1;
    damaged[last] ^= 0xff;
    let update = file(folder.path(), "2.nshpkg", &damaged);
    assert!(install(&paths, &roots, &update).is_err());
    // A replacement that cannot be written leaves the old file whole.
    let good = signed(folder.path(), "3.nshpkg", &signer, 2);
    let staged = paths
        .root
        .join("packages")
        .join(".dev.example.app.nshpkg.staged");
    std::fs::create_dir_all(&staged).unwrap();
    assert!(install(&paths, &roots, &good).is_err());
    assert_eq!(
        std::fs::read(paths.package_file("dev.example.app")).unwrap(),
        before
    );
}

/// A store with build 1 of `dev.example.app` installed.
pub(super) fn first_build() -> (
    tempfile::TempDir,
    Paths,
    crate::fixtures::Signer,
    super::Verified,
) {
    let (folder, paths) = store();
    let signer = signer();
    let source = signed(folder.path(), "1.nshpkg", &signer, 1);
    let first = install(&paths, std::slice::from_ref(&signer.root), &source).unwrap();
    (folder, paths, signer, first)
}

#[test]
fn a_failed_install_record_leaves_the_earlier_build_and_install() {
    let (folder, paths, signer, first) = first_build();
    let kept = std::fs::read(paths.package_file("dev.example.app")).unwrap();
    // The record cannot be renamed over a folder.
    let record = paths.root.join("installs.json");
    std::fs::remove_file(&record).unwrap();
    std::fs::create_dir(&record).unwrap();
    let other = crate::fixtures::signer();
    let roots = [signer.root.clone(), other.root.clone()];
    assert!(
        install(
            &paths,
            &roots,
            &signed(folder.path(), "2.nshpkg", &other, 2)
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(paths.package_file("dev.example.app")).unwrap(),
        kept
    );
    std::fs::remove_dir(&record).unwrap();
    assert!(!first.identity.install.is_empty());
}

#[test]
fn a_package_copied_in_without_a_record_gets_an_install() {
    let (folder, paths) = store();
    let signer = signer();
    let roots = [signer.root.clone()];
    let bytes = std::fs::read(signed(folder.path(), "1.nshpkg", &signer, 1)).unwrap();
    std::fs::create_dir_all(paths.package_file("x").parent().unwrap()).unwrap();
    std::fs::write(paths.package_file("dev.example.app"), bytes).unwrap();
    let found = installed(&paths, &roots);
    let install = &found[0].state.as_ref().unwrap().identity.install;
    assert!(!install.is_empty());
    let again = installed(&paths, &roots);
    assert_eq!(&again[0].state.as_ref().unwrap().identity.install, install);
}
