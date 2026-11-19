//! Replacing the running executable so a failure leaves the previous one in
//! place (REQ-05). The backup lives in a staging directory beside the
//! executable, which is the same volume, so the final step is a rename.

use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, ensure};

/// Unix owner-execute bit, added to the permissions the previous executable
/// had so the replacement stays runnable.
#[cfg(unix)]
const UNIX_OWNER_EXECUTE: u32 = 0o100;
const STAGE_PREFIX: &str = ".subspace-update-";
const STAGED_NAME: &str = "new";

/// Refuses an executable a package manager installed under another name.
pub(crate) fn require_own_executable_name(executable: &Path, binary_name: &str) -> Result<()> {
    let name = executable
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned();
    ensure!(
        name == binary_name || name == format!("{binary_name}.exe"),
        "{name} was installed by a package manager, not by a {binary_name} release; update it through that package manager"
    );
    Ok(())
}

/// A staging directory inside `directory`, removed when the returned handle
/// drops.
pub(crate) fn stage_in(directory: &Path) -> Result<tempfile::TempDir> {
    tempfile::Builder::new()
        .prefix(STAGE_PREFIX)
        .tempdir_in(directory)
        .with_context(|| {
            format!(
                "cannot write to {}; use a writable installation folder",
                directory.display()
            )
        })
}

/// Writes `bytes` over `target`. On Unix the previous file is kept as a hard
/// link in the staging directory, so the rename is atomic and the old file
/// goes away with the stage. On Windows it is moved to `<target>.old`, which
/// `cleanup_previous` removes at the next start.
pub(crate) fn replace_executable(target: &Path, bytes: &[u8], windows: bool) -> Result<()> {
    let directory = target.parent().context("the executable has no parent")?;
    let stage = stage_in(directory)?;
    let staged = write_staged(stage.path(), target, bytes)?;
    commit(stage, &staged, target, windows)
}

fn write_staged(stage: &Path, target: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let metadata =
        fs::symlink_metadata(target).with_context(|| format!("read {}", target.display()))?;
    ensure!(
        metadata.file_type().is_file(),
        "{} is not a regular file; update its installation separately",
        target.display()
    );
    let staged = stage.join(STAGED_NAME);
    let mut file = fs::File::create(&staged)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::set_permissions(&staged, executable_permissions(metadata.permissions()))?;
    Ok(staged)
}

#[cfg(unix)]
fn executable_permissions(permissions: fs::Permissions) -> fs::Permissions {
    use std::os::unix::fs::PermissionsExt;
    fs::Permissions::from_mode(permissions.mode() | UNIX_OWNER_EXECUTE)
}

#[cfg(not(unix))]
fn executable_permissions(permissions: fs::Permissions) -> fs::Permissions {
    permissions
}

fn commit(stage: tempfile::TempDir, staged: &Path, target: &Path, windows: bool) -> Result<()> {
    let backup = backup_path(stage.path(), target, windows);
    prepare_backup(target, &backup, windows)?;
    match fs::rename(staged, target) {
        Ok(()) => Ok(()),
        Err(error) => Err(rolled_back(stage, &backup, target, windows, error)),
    }
}

fn backup_path(stage: &Path, target: &Path, windows: bool) -> PathBuf {
    if windows {
        return target.with_extension("exe.old");
    }
    stage.join(format!(
        "{}.old",
        target.file_name().unwrap_or_default().to_string_lossy()
    ))
}

fn prepare_backup(target: &Path, backup: &Path, windows: bool) -> Result<()> {
    if !windows {
        return fs::hard_link(target, backup).context("keep the previous executable for rollback");
    }
    if backup.exists() {
        fs::remove_file(backup)
            .context("remove the previous executable; close old application processes and retry")?;
    }
    fs::rename(target, backup).context("move the running executable aside")
}

/// Puts the previous executable back. On Windows the target was moved aside,
/// so it is renamed back; on Unix the hard link means the target is still in
/// place and only a lost target needs the backup.
fn restore(backup: &Path, target: &Path, windows: bool) -> std::io::Result<()> {
    if windows && target.exists() {
        fs::remove_file(target)?;
    }
    fs::rename(backup, target)
}

/// The error for a failed install, after putting the previous executable
/// back. The staging directory is kept only when a restore by hand is left to
/// do, and then the message says where the backup is.
fn rolled_back(
    stage: tempfile::TempDir,
    backup: &Path,
    target: &Path,
    windows: bool,
    error: std::io::Error,
) -> anyhow::Error {
    let what = format!("install {}", target.display());
    if !windows && target.exists() {
        return anyhow::Error::new(error).context(what);
    }
    match restore(backup, target, windows) {
        Ok(()) => anyhow::Error::new(error).context(what),
        Err(failure) => {
            let _ = stage.keep();
            anyhow::anyhow!(
                "{what}: {error}; restore {} from {} by hand: {failure}",
                target.display(),
                backup.display()
            )
        }
    }
}

/// Drops the executable a previous install moved aside, if it is still
/// there. Best effort: Windows keeps a replaced executable locked until its
/// own process exits, so the file can only go at the next start.
fn remove_previous(directory: &Path, binary_name: &str) {
    let _ = fs::remove_file(directory.join(format!("{binary_name}.exe.old")));
}

/// Called once at launch. Only Windows leaves a previous executable behind.
pub fn cleanup_previous(binary_name: &str) {
    if !cfg!(windows) {
        return;
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(directory) = executable.parent()
    {
        remove_previous(directory, binary_name);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::{
        cleanup_previous, commit, remove_previous, replace_executable, require_own_executable_name,
        stage_in,
    };

    fn installed(directory: &Path, windows: bool) -> PathBuf {
        let target = directory.join(if windows { "app.exe" } else { "app" });
        fs::write(&target, b"old binary").expect("write the previous executable");
        target
    }

    fn entries(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .expect("read the installation folder")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_replace_leaves_only_the_new_executable_behind() {
        let directory = tempfile::tempdir().unwrap();
        let target = installed(directory.path(), false);
        replace_executable(&target, b"new binary", false).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new binary");
        assert_eq!(entries(directory.path()), vec!["app".to_owned()]);
    }

    #[test]
    fn a_windows_replace_keeps_the_previous_executable_for_the_next_start() {
        let directory = tempfile::tempdir().unwrap();
        let target = installed(directory.path(), true);
        replace_executable(&target, b"new binary", true).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new binary");
        assert_eq!(
            fs::read(directory.path().join("app.exe.old")).unwrap(),
            b"old binary"
        );
        assert_eq!(
            entries(directory.path()),
            vec!["app.exe".to_owned(), "app.exe.old".to_owned()]
        );
    }

    #[test]
    fn a_windows_replace_overwrites_a_previous_old_file() {
        let directory = tempfile::tempdir().unwrap();
        let target = installed(directory.path(), true);
        fs::write(directory.path().join("app.exe.old"), b"stale").unwrap();
        replace_executable(&target, b"new binary", true).unwrap();
        assert_eq!(
            fs::read(directory.path().join("app.exe.old")).unwrap(),
            b"old binary"
        );
    }

    #[test]
    fn a_failed_rename_restores_the_previous_executable() {
        for windows in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let target = installed(directory.path(), windows);
            let stage = stage_in(directory.path()).unwrap();
            let missing = stage.path().join("never-written");
            let error = commit(stage, &missing, &target, windows).unwrap_err();
            assert!(error.to_string().contains("install"), "{error}");
            assert_eq!(fs::read(&target).unwrap(), b"old binary");
            assert_eq!(
                entries(directory.path()),
                vec![target.file_name().unwrap().to_string_lossy().into_owned()],
                "no backup or stage may be left behind"
            );
        }
    }

    #[test]
    fn the_next_start_drops_a_previous_executable_and_tolerates_none() {
        let directory = tempfile::tempdir().unwrap();
        let previous = directory.path().join("app.exe.old");
        fs::write(&previous, b"replaced").unwrap();
        remove_previous(directory.path(), "app");
        assert!(!previous.exists());
        remove_previous(directory.path(), "app");
        assert!(!previous.exists(), "a second start has nothing to remove");
        // The public entry acts only on Windows and touches nothing here.
        cleanup_previous("app");
        assert_eq!(entries(directory.path()), Vec::<String>::new());
    }

    #[test]
    fn an_executable_under_another_name_is_refused() {
        let error =
            require_own_executable_name(Path::new("/usr/local/bin/subspace-old"), "subspace")
                .unwrap_err();
        assert!(error.to_string().contains("package manager"), "{error}");
        assert!(require_own_executable_name(Path::new("/opt/subspace"), "subspace").is_ok());
        assert!(require_own_executable_name(Path::new("/apps/subspace.exe"), "subspace").is_ok());
    }
}
