use std::path::{Path, PathBuf};

const SECRET_RELATIVE: &[&str] = &[
    ".ssh", ".aws", ".azure", ".config", ".docker", ".gnupg", ".kube", ".netrc", ".npmrc",
    ".pypirc",
];

const SYSTEM_PATH: &[&str] = &["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn host_home() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|path| std::fs::canonicalize(path).ok())
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) fn path_strings(paths: &[PathBuf]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

pub(crate) fn path_env(readonly_roots: &[PathBuf]) -> String {
    readonly_roots
        .iter()
        .flat_map(|root| {
            [
                root.join("bin"),
                root.join(".venv/bin"),
                root.join("node_modules/.bin"),
                root.clone(),
            ]
        })
        .chain(SYSTEM_PATH.iter().copied().map(PathBuf::from))
        .map(|path| path.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(":")
}

pub(crate) fn workspace_home(workspace: &Path) -> String {
    workspace.to_string_lossy().into_owned()
}

pub(crate) fn scoped_environment(
    readonly_roots: &[PathBuf],
    workspace: &Path,
) -> Vec<(String, String)> {
    home_tmp_path(workspace_home(workspace), path_env(readonly_roots))
}

fn home_tmp_path(home: String, path: String) -> Vec<(String, String)> {
    vec![
        ("HOME".into(), home.clone()),
        ("TMPDIR".into(), home),
        ("PATH".into(), path),
    ]
}

pub(crate) fn denied_secret_paths(
    host_home: Option<&Path>,
    roots: &[PathBuf],
    workspace: &Path,
) -> Vec<String> {
    let Some(home) = host_home else {
        return Vec::new();
    };
    let mut denied = Vec::new();
    for relative in SECRET_RELATIVE {
        let candidate = home.join(relative);
        let covered = roots
            .iter()
            .any(|root| candidate.starts_with(root) || root.starts_with(&candidate));
        let workspace_hit = candidate.starts_with(workspace) || workspace.starts_with(&candidate);
        if !covered && !workspace_hit {
            denied.push(candidate.to_string_lossy().into_owned());
        }
    }
    denied
}

#[cfg(test)]
mod tests {
    use super::denied_secret_paths;
    use super::path_env;
    use super::scoped_environment;
    use std::path::{Path, PathBuf};

    #[test]
    fn path_env_lists_root_bins_then_system_paths() {
        let root = PathBuf::from("/runtime");
        let env = path_env(std::slice::from_ref(&root));
        assert!(env.contains(&root.join("bin").to_string_lossy().into_owned()));
        assert!(env.contains(&root.join(".venv/bin").to_string_lossy().into_owned()));
        assert!(env.contains("/usr/bin"));
        assert!(env.contains("/sbin"));
    }

    #[test]
    fn scoped_environment_clears_home_to_workspace() {
        let workspace = Path::new("/workspace");
        let home = workspace.to_string_lossy().into_owned();
        let env = scoped_environment(&[PathBuf::from("/runtime")], workspace);
        assert_eq!(env[0], ("HOME".into(), home.clone()));
        assert_eq!(env[1], ("TMPDIR".into(), home));
        assert_eq!(env[2].0, "PATH");
        assert!(
            env[2].1.contains(
                &PathBuf::from("/runtime")
                    .join("bin")
                    .to_string_lossy()
                    .into_owned()
            )
        );
    }

    #[test]
    fn secret_paths_are_denied_when_outside_roots_and_workspace() {
        let denied = denied_secret_paths(
            Some(Path::new("/home/user")),
            &[PathBuf::from("/runtime")],
            Path::new("/workspace"),
        );
        assert!(denied.iter().any(|path| path.ends_with(".ssh")));
        assert!(denied.iter().any(|path| path.ends_with(".aws")));
        assert_eq!(denied.len(), super::SECRET_RELATIVE.len());
    }

    #[test]
    fn secret_paths_inside_workspace_are_not_denied() {
        let denied = denied_secret_paths(
            Some(Path::new("/workspace")),
            &[PathBuf::from("/runtime")],
            Path::new("/workspace"),
        );
        assert!(denied.is_empty());
    }

    #[test]
    fn missing_home_yields_no_denied_paths() {
        let denied =
            denied_secret_paths(None, &[PathBuf::from("/runtime")], Path::new("/workspace"));
        assert!(denied.is_empty());
    }
}
