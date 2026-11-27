//! What Remove does to a package, and what its prompt says.

use std::path::Path;

use super::installs::{read_installs, write_installs};
use super::{Installed, Paths, Unverified, lock};

/// What Remove does to a package, as its prompt says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Removal {
    /// The app's own file: its data and install go too.
    Owned,
    /// An authenticated app under another file name: the file alone.
    Copy,
    /// The file alone, for a package that did not verify.
    Unverified(Unverified),
}

/// What Remove does to `package`, decided by the check Remove itself makes.
pub(crate) fn removal(paths: &Paths, package: &Installed) -> Removal {
    let app_id = package.app_id.as_deref();
    if owner(paths, &package.file, app_id).is_some() {
        Removal::Owned
    } else if app_id.is_some() {
        Removal::Copy
    } else {
        Removal::Unverified(package.unverified.clone())
    }
}

impl Removal {
    /// What Remove deletes and what it leaves, for its prompt.
    pub(crate) fn detail(&self) -> String {
        let text = match self {
            Self::Owned => {
                "Its files and saved data are deleted. Installed again, it asks for its \
                 capabilities again."
            }
            Self::Copy => {
                "Only this file is removed; the app's saved data and grants are left alone."
            }
            Self::Unverified(Unverified::Untrusted) => {
                "Only the package file is removed: its root is not trusted, so its saved \
                 data, install record and grants stay, and apply again to a reinstall signed \
                 by the same key. To remove everything, trust its root again, then remove it."
            }
            Self::Unverified(Unverified::Refused(reason)) => {
                return format!(
                    "It does not verify: {reason}. Only this file is removed; its saved data \
                     and grants are left alone."
                );
            }
            Self::Unverified(Unverified::Damaged) => {
                "This file is damaged or cannot be read. Only this file is removed; no other \
                 data is touched."
            }
        };
        text.to_owned()
    }
}

/// Removes an installed package file. If it is its app's file in the store,
/// the app's unpacked assets, storage and install go too: installing it
/// again asks for its capabilities again. The app is the one the package
/// authenticated as, `app_id`: verified, or refused only as an older build.
/// Without it, such as for a package whose root is no longer trusted, or
/// for a copy under another app's name, only the file goes: an identity
/// anyone could forge never decides a recursive deletion.
pub(crate) fn remove(paths: &Paths, file: &Path, verified: Option<&str>) -> Result<(), String> {
    let _store = lock();
    std::fs::remove_file(file).map_err(|error| error.to_string())?;
    let mut installs = read_installs(paths);
    let Some(app_id) = owner(paths, file, verified) else {
        return Ok(());
    };
    for folder in [paths.assets(&app_id), paths.root.join("data").join(&app_id)] {
        if folder.exists() {
            std::fs::remove_dir_all(folder).map_err(|error| error.to_string())?;
        }
    }
    installs.remove(&app_id);
    write_installs(paths, &installs)
}

/// The app whose files go with `file`: its authenticated app ID, only if
/// `file` is that app's file in the store and the ID is one folder name.
fn owner(paths: &Paths, file: &Path, app_id: Option<&str>) -> Option<String> {
    let app_id = app_id?;
    (one_folder(app_id) && paths.package_file(app_id) == file).then(|| app_id.to_owned())
}

/// Whether `name` names one folder inside its parent: not empty, `.`, `..`
/// or anything with a separator.
pub(super) fn one_folder(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(
        (parts.next(), parts.next()),
        (Some(std::path::Component::Normal(part)), None) if part == name
    ) && !name.contains(['/', '\\', '\0'])
}

#[cfg(test)]
mod tests {
    use super::super::scan::from_error;
    use super::{Removal, Unverified};
    use nsh_package::Error;

    fn detail(error: Error) -> String {
        Removal::Unverified(from_error(&error)).detail()
    }

    #[test]
    fn only_an_untrusted_signing_key_gets_the_trust_advice() {
        let untrusted = detail(Error::UntrustedKey("cf13".into()));
        assert!(untrusted.contains("trust its root again"), "{untrusted}");
        for error in [
            Error::Revoked("key cf13".into()),
            Error::Certificate("app dev.example.app is outside dev.other.*".into()),
            Error::RuntimeTooOld {
                needs: "9.0.0".into(),
                runtime: "0.7.1".into(),
            },
            Error::Integrity("app.js".into()),
        ] {
            let reason = error.to_string();
            let text = detail(error);
            assert!(
                text.contains(&format!("It does not verify: {reason}.")),
                "{text}"
            );
            assert!(!text.contains("trust"), "{text}");
            assert!(text.contains("left alone"), "{text}");
        }
    }

    #[test]
    fn a_copy_and_a_damaged_file_say_only_this_file_goes() {
        assert!(Removal::Owned.detail().contains("saved data are deleted"));
        let copy = Removal::Copy.detail();
        assert!(copy.contains("Only this file is removed"), "{copy}");
        assert!(!copy.contains("stays installed"), "{copy}");
        let damaged = Removal::Unverified(Unverified::Damaged).detail();
        assert!(damaged.contains("no other data is touched"), "{damaged}");
    }
}
