//! Reading and verifying the installed packages. The scan also records why
//! a package did not verify; that only chooses the Remove prompt's words.

use std::path::{Path, PathBuf};

use super::installs::{Installs, new_install, read_installs, write_installs};
use super::{EXTENSION, Paths, SOURCE_ONLY, Verified, lock, read, trust, verify};
use crate::grant::Identity;

/// An installed package file, verified or refused.
#[derive(Clone)]
pub(crate) struct Installed {
    pub(crate) file: PathBuf,
    pub(crate) state: Result<Verified, String>,
    /// The app the file verified as, even when it was refused as a copy of
    /// that app under another name. Remove cleans up only by this ID.
    pub(crate) app_id: Option<String>,
    /// Whether the package's own signature checks out, whatever its root:
    /// tells a package of an untrusted root from a damaged file, for the
    /// Remove prompt only.
    pub(crate) unverified: Unverified,
}

/// The installed packages, each verified against `roots`, by file name.
pub(crate) fn installed(paths: &Paths, roots: &[String]) -> Vec<Installed> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(paths.packages())
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok().map(|entry| entry.path()))
                .filter(|path| path.extension().is_some_and(|ext| ext == EXTENSION))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let _store = lock();
    let mut installs = read_installs(paths);
    let packages: Vec<Installed> = files
        .into_iter()
        .map(|file| scan(paths, roots, file, &installs))
        .collect();
    // A package copied in by hand has no install record: give it one, so its
    // grants bind to an install like any other.
    let unrecorded: Vec<Identity> = packages
        .iter()
        .filter_map(|package| package.state.as_ref().ok())
        .filter(|verified| verified.identity.install.is_empty())
        .map(|verified| verified.identity.clone())
        .collect();
    if unrecorded.is_empty() {
        return packages;
    }
    for identity in &unrecorded {
        installs.insert(
            identity.app_id.clone(),
            (identity.key_id.clone(), new_install()),
        );
    }
    if write_installs(paths, &installs).is_err() {
        return packages;
    }
    packages
        .into_iter()
        .map(|package| with_install(package, &installs))
        .collect()
}

/// `package` with the install `installs` records for it.
fn with_install(mut package: Installed, installs: &Installs) -> Installed {
    if let Ok(verified) = &mut package.state
        && let Some((_, install)) = installs.get(&verified.identity.app_id)
    {
        verified.identity.install.clone_from(install);
    }
    package
}

/// Reads and verifies one package file.
fn scan(paths: &Paths, roots: &[String], file: PathBuf, installs: &Installs) -> Installed {
    let bytes = read(&file);
    let verified = bytes
        .clone()
        .and_then(|bytes| verify(paths, roots, &bytes, installs));
    let unverified = match (&bytes, &verified) {
        (Ok(bytes), Err(reason)) => unverified(paths, roots, bytes, reason),
        _ => Unverified::Damaged,
    };
    let app_id = match &verified {
        Ok(verified) => Some(verified.identity.app_id.clone()),
        Err(_) => bytes
            .ok()
            .and_then(|bytes| rolled_back(paths, roots, &bytes)),
    };
    Installed {
        app_id,
        unverified,
        state: verified.and_then(|verified| canonical(paths, &file, verified)),
        file,
    }
}

/// Why a package did not verify, for the Remove prompt only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Unverified {
    /// No trusted root covers its signing key, or no root is trusted.
    Untrusted,
    /// Correctly signed, but refused for this reason: revoked, outside its
    /// certificate, for a newer runtime and so on.
    Refused(String),
    /// Its own signature fails, or it cannot be read.
    Damaged,
}

/// Why the signed or unsigned `bytes` were refused with `reason`.
fn unverified(paths: &Paths, roots: &[String], bytes: &[u8], reason: &str) -> Unverified {
    if nsh_package::verify_signature(bytes).is_err() {
        return Unverified::Damaged;
    }
    if roots.is_empty() {
        return Unverified::Untrusted;
    }
    match trust(paths, roots).map(|trust| trust.verify(bytes, SOURCE_ONLY)) {
        Ok(Err(error)) => from_error(&error),
        _ => Unverified::Refused(reason.to_owned()),
    }
}

/// How the Remove prompt treats a verification error of a signed package.
pub(crate) fn from_error(error: &nsh_package::Error) -> Unverified {
    match error {
        nsh_package::Error::UntrustedKey(_) => Unverified::Untrusted,
        other => Unverified::Refused(other.to_string()),
    }
}

/// The app of a package refused only as an older build: its signature
/// checked out against a trusted root, so its app ID is authenticated.
fn rolled_back(paths: &Paths, roots: &[String], bytes: &[u8]) -> Option<String> {
    match trust(paths, roots).ok()?.verify(bytes, SOURCE_ONLY) {
        Err(nsh_package::Error::Rollback { app, .. }) => Some(app),
        _ => None,
    }
}

/// `verified` if `file` is its app's one file in the store. Any other file
/// holding the app, such as a copy under another name, is refused, so two
/// files cannot take turns as the app's install.
fn canonical(paths: &Paths, file: &Path, verified: Verified) -> Result<Verified, String> {
    let app_id = &verified.identity.app_id;
    if paths.package_file(app_id) == file {
        Ok(verified)
    } else {
        Err(format!(
            "This file holds {app_id}, which is kept as {app_id}.{EXTENSION}. Remove this copy."
        ))
    }
}
