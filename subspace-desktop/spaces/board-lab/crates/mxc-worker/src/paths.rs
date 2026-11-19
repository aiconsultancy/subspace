use std::path::{Path, PathBuf};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use anyhow::Context;
use anyhow::{Result, bail};
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::fs;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn canonicalize(path: &Path, label: &str) -> Result<PathBuf> {
    let path = fs::canonicalize(path).with_context(|| format!("canonicalize {label}"))?;
    if !path.is_absolute() {
        bail!(
            "{label} is not absolute after canonicalization: {}",
            path.display()
        );
    }
    Ok(path)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn require_file(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("executable is not a file: {}", path.display());
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn require_directory(path: &Path, label: &str) -> Result<()> {
    if path.is_dir() {
        Ok(())
    } else {
        bail!("{label} is not an existing directory: {}", path.display());
    }
}

pub(crate) fn require_nonempty(roots: &[PathBuf]) -> Result<()> {
    if roots.is_empty() {
        bail!("at least one read-only runtime or project root is required");
    }
    Ok(())
}

pub(crate) fn reject_overlap(root: &Path, workspace: &Path) -> Result<()> {
    if root.starts_with(workspace) || workspace.starts_with(root) {
        bail!("readonly runtime roots and workspace must not overlap");
    }
    Ok(())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn launcher_path(executable: &Path) -> Result<PathBuf> {
    let parent = executable.parent().context("executable has no parent")?;
    let parent = fs::canonicalize(parent).context("canonicalize executable directory")?;
    let name = executable.file_name().context("executable has no name")?;
    Ok(parent.join(name))
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn resolve_directory(path: &Path, label: &str) -> Result<PathBuf> {
    let path = canonicalize(path, label)?;
    require_directory(&path, label)?;
    Ok(path)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn resolve_workspace(workspace: &Path) -> Result<PathBuf> {
    resolve_directory(workspace, "workspace")
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn resolve_one_root(root: &Path, workspace: &Path) -> Result<PathBuf> {
    let root = canonicalize(root, "readonly runtime root")?;
    require_directory(&root, "readonly runtime root")?;
    reject_overlap(&root, workspace)?;
    Ok(root)
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn resolve_roots(roots: &[PathBuf], workspace: &Path) -> Result<Vec<PathBuf>> {
    require_nonempty(roots)?;
    roots
        .iter()
        .map(|root| resolve_one_root(root, workspace))
        .collect()
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn canary_parent(workspace: &Path) -> &Path {
    workspace.parent().unwrap_or_else(|| Path::new("."))
}

#[cfg(test)]
mod tests {
    use super::reject_overlap;
    use super::require_nonempty;
    use std::path::{Path, PathBuf};

    #[test]
    fn identical_root_and_workspace_are_rejected() {
        let error = reject_overlap(Path::new("/runtime"), Path::new("/runtime")).unwrap_err();
        assert!(error.to_string().contains("must not overlap"));
    }

    #[test]
    fn empty_roots_are_rejected() {
        let error = require_nonempty(&[]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("at least one read-only runtime or project root is required")
        );
    }

    #[test]
    fn nonempty_roots_are_accepted() {
        assert!(require_nonempty(&[PathBuf::from("/runtime")]).is_ok());
    }

    #[test]
    fn nested_workspace_inside_root_is_rejected() {
        let error = reject_overlap(Path::new("/runtime"), Path::new("/runtime/job")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("readonly runtime roots and workspace must not overlap")
        );
    }

    #[test]
    fn root_inside_workspace_is_rejected() {
        let error =
            reject_overlap(Path::new("/workspace/runtime"), Path::new("/workspace")).unwrap_err();
        assert!(error.to_string().contains("must not overlap"));
    }

    #[test]
    fn disjoint_and_similar_prefix_paths_are_accepted() {
        assert!(reject_overlap(Path::new("/runtime"), Path::new("/workspace")).is_ok());
        assert!(reject_overlap(Path::new("/runtime"), Path::new("/runtime-other")).is_ok());
    }
}
