//! Installed packages and their verification. Packages live in the app's
//! config folder under `tsx/packages`, one `<app id>.nshpkg` each; nsh's
//! rollback records are under `tsx/trust`, unpacked assets under
//! `tsx/assets/<app id>/<package hash>`, each app's storage under
//! `tsx/data/<app id>/<release key>`, and the install of each app in
//! `tsx/installs.json`. Every function here reads or writes files or hashes
//! a package: run them off the UI thread. Changes to these files hold one
//! process-wide lock, so concurrent installs and accepts lose nothing.
//!
//! Packages run from their verified JS source only. QuickJS does not
//! validate bytecode, and a signature says who published a package, not
//! that its bytecode is safe to load, so package bytecode is never read.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use nsh_engine::{Bundle, BundleCode};
use nsh_package::{PackageReceipt, PublicKey, TrustStore, VerifiedPackage};

use crate::grant::Identity;
use installs::{Installs, new_install, read_installs, write_installs};

/// The extension of a signed package.
pub(crate) const EXTENSION: &str = "nshpkg";
/// The bytecode version asked of a package: no runtime compiles for it, so
/// verification never hands package bytecode back.
pub(super) const SOURCE_ONLY: &str = "subspace-source-only";

/// Serialises every change to the store's files.
static STORE: Mutex<()> = Mutex::new(());

pub(super) fn lock() -> MutexGuard<'static, ()> {
    STORE.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A package that passed every check, with its source-only bundle.
#[derive(Clone)]
pub(crate) struct Verified {
    pub(crate) name: String,
    pub(crate) version: String,
    pub(crate) sha256: String,
    pub(crate) identity: Identity,
    pub(crate) bundle: Bundle,
    pub(crate) assets: PathBuf,
    receipt: PackageReceipt,
}

/// Runs `write` under the store's lock, for files kept beside the store.
pub(crate) fn locked<T>(write: impl FnOnce() -> T) -> T {
    let _store = lock();
    write()
}

/// Verifies `source` and, if it passes, puts it in place of an earlier build
/// of the same app. The install record is written first and the package
/// staged and renamed last; if the package cannot be saved the earlier
/// record is put back, so a failure at either step leaves the earlier build
/// and its grants whole. A refused package leaves nothing behind.
pub(crate) fn install(paths: &Paths, roots: &[String], source: &Path) -> Result<Verified, String> {
    let bytes = read(source)?;
    let _store = lock();
    let mut installs = read_installs(paths);
    let verified = verify(paths, roots, &bytes, &installs)?;
    let app_id = verified.identity.app_id.clone();
    let earlier = installs.clone();
    // An update signed by the same key keeps its install, and so its grants.
    let install = match installs.get(&app_id) {
        Some((key, install)) if *key == verified.identity.key_id => install.clone(),
        _ => new_install(),
    };
    installs.insert(app_id.clone(), (verified.identity.key_id.clone(), install));
    write_installs(paths, &installs)?;
    if let Err(error) = replace(&paths.package_file(&app_id), &bytes) {
        let _ = write_installs(paths, &earlier);
        return Err(error);
    }
    verify(paths, roots, &bytes, &installs)
}

/// Records the build as accepted: older builds are refused from now on.
pub(crate) fn accept(paths: &Paths, roots: &[String], verified: &Verified) -> Result<(), String> {
    let _store = lock();
    let mut trust = trust(paths, roots)?;
    trust
        .accept_receipt(&verified.receipt)
        .map_err(|error| format!("The build was not recorded: {error}"))
}

pub(super) fn read(file: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(file).map_err(|error| format!("{}: {error}", file.display()))
}

pub(super) fn verify(
    paths: &Paths,
    roots: &[String],
    bytes: &[u8],
    installs: &Installs,
) -> Result<Verified, String> {
    if roots.is_empty() {
        return Err("No signing root is trusted. Add one in Settings › TSX Spaces.".into());
    }
    let package = trust(paths, roots)?
        .verify(bytes, SOURCE_ONLY)
        .map_err(|error| error.to_string())?;
    let bundle = source_bundle(&package)?;
    let assets = unpack(paths, &package)?;
    let install = installs
        .get(&package.app.id)
        .filter(|(key, _)| *key == package.key_id)
        .map_or_else(String::new, |(_, install)| install.clone());
    let declared = bundle
        .policy
        .declared
        .as_ref()
        .map(|capabilities| Identity::declared_tiers(&capabilities.host))
        .unwrap_or_default();
    Ok(Verified {
        name: package.app.name.clone(),
        version: package.app.version.clone(),
        sha256: package.sha256.clone(),
        identity: Identity {
            app_id: package.app.id.clone(),
            key_id: package.key_id.clone(),
            install,
            declared,
        },
        bundle,
        assets,
        receipt: package.receipt(),
    })
}

/// The package's bundle from its JS source, whatever bytecode it carries.
fn source_bundle(package: &VerifiedPackage) -> Result<Bundle, String> {
    let mut bundle = Bundle::from_package(package)?;
    bundle.code = BundleCode::Source(package.source.clone());
    Ok(bundle)
}

/// Writes the package's assets under its hash, once.
fn unpack(paths: &Paths, package: &VerifiedPackage) -> Result<PathBuf, String> {
    let folder = paths.assets(&package.app.id).join(&package.sha256);
    let assets = folder.join("assets");
    if folder.join(".complete").exists() {
        return Ok(assets);
    }
    for (path, data) in &package.assets {
        // nsh checked each path when it read the package: relative, no `..`.
        let target = assets.join(path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&target, data).map_err(|error| format!("{}: {error}", target.display()))?;
    }
    std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
    std::fs::write(folder.join(".complete"), b"").map_err(|error| error.to_string())?;
    Ok(assets)
}

/// Writes `bytes` to a staged file beside `target`, then renames it over.
pub(crate) fn replace(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let folder = target.parent().ok_or("no folder")?;
    std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    let staged = folder.join(format!(
        ".{}.staged",
        target
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));
    let written = std::fs::write(&staged, bytes).and_then(|()| std::fs::rename(&staged, target));
    written.map_err(|error| {
        let _ = std::fs::remove_file(&staged);
        format!("{} was not saved: {error}", target.display())
    })
}

pub(super) fn trust(paths: &Paths, roots: &[String]) -> Result<TrustStore, String> {
    let mut trust = TrustStore::open(paths.trust()).map_err(|error| error.to_string())?;
    for root in roots {
        let key = PublicKey::from_base64(root)
            .map_err(|error| format!("Trusted root {root} is not a public key: {error}"))?;
        trust.add_root(key);
    }
    Ok(trust)
}

#[path = "store/scan.rs"]
mod scan;
pub(crate) use scan::{Installed, Unverified, installed};
#[path = "store/paths.rs"]
mod paths;
pub(crate) use paths::Paths;
#[path = "store/installs.rs"]
mod installs;
#[path = "store/removal.rs"]
mod removal;
pub(crate) use removal::{removal, remove};

#[cfg(test)]
#[path = "store/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "store/remove_tests.rs"]
mod remove_tests;
