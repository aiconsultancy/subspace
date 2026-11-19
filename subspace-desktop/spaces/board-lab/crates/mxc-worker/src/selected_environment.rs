use std::collections::BTreeSet;

use anyhow::{Result, bail};

const MAX_NAMES: usize = 64;
const MAX_VALUE_BYTES: usize = 16 * 1024;
const MAX_TOTAL_BYTES: usize = 128 * 1024;

/// Reject names that could replace managed paths, runtimes, loaders or protocol settings.
/// Errors never contain the supplied name or value.
pub fn validate_environment_name(name: &str) -> Result<()> {
    if !is_valid_name(name) {
        bail!("selected environment name must be an ASCII identifier");
    }
    if is_reserved_name(name) {
        bail!("selected environment name is reserved for managed execution");
    }
    Ok(())
}

fn is_valid_name(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'A'..=b'Z' | b'a'..=b'z' | b'_'))
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn is_reserved_name(name: &str) -> bool {
    let name = name.to_ascii_uppercase();
    matches!(
        name.as_str(),
        "HOME"
            | "PATH"
            | "TMPDIR"
            | "TMP"
            | "TEMP"
            | "PWD"
            | "OLDPWD"
            | "SHELL"
            | "SHLVL"
            | "_"
            | "ENV"
            | "BASH_ENV"
            | "CDPATH"
            | "IFS"
            | "ZDOTDIR"
            | "HTTP_PROXY"
            | "HTTPS_PROXY"
            | "ALL_PROXY"
            | "NO_PROXY"
            | "FTP_PROXY"
            | "VIRTUAL_ENV"
    ) || [
        "LD_",
        "DYLD_",
        "PYTHON",
        "NODE_",
        "BUN_",
        "MISE_",
        "UV_",
        "CONDA_",
        "NPM_",
        "PNPM_",
        "COREPACK_",
        "SUBSPACE_",
        "MXC_",
        "BOARD_LAB_",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}

pub(crate) fn validate_selected_environment(selected: &[(String, String)]) -> Result<()> {
    require_count(selected.len())?;
    let mut seen = BTreeSet::new();
    let mut total = 0usize;
    for (name, value) in selected {
        validate_environment_name(name)?;
        require_unique(seen.insert(name))?;
        validate_value(value)?;
        total = total.saturating_add(name.len()).saturating_add(value.len());
    }
    require_total(total)
}

fn require_count(count: usize) -> Result<()> {
    if count > MAX_NAMES {
        bail!("selected environment exceeds the 64-name limit");
    }
    Ok(())
}

fn require_unique(unique: bool) -> Result<()> {
    if !unique {
        bail!("selected environment contains a duplicate name");
    }
    Ok(())
}

fn validate_value(value: &str) -> Result<()> {
    if value.contains('\0') || value.len() > MAX_VALUE_BYTES {
        bail!("selected environment value contains NUL or exceeds 16 KiB");
    }
    Ok(())
}

fn require_total(bytes: usize) -> Result<()> {
    if bytes > MAX_TOTAL_BYTES {
        bail!("selected environment exceeds the 128 KiB limit");
    }
    Ok(())
}

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
pub(crate) fn persistent_environment(
    mut managed: Vec<(String, String)>,
    selected: Vec<(String, String)>,
) -> Vec<(String, String)> {
    managed.extend(selected);
    managed
}

#[cfg(test)]
mod tests {
    use super::{persistent_environment, validate_environment_name, validate_selected_environment};
    use std::path::{Path, PathBuf};

    fn selected_values(count: usize, bytes: usize) -> Vec<(String, String)> {
        (0..count)
            .map(|ix| (format!("VALUE_{ix}"), "x".repeat(bytes)))
            .collect()
    }

    #[test]
    fn selected_values_extend_managed_environment_without_host_inheritance() {
        let selected = vec![("TEST_API_TOKEN".into(), "dummy-test-only-value".into())];
        let environment = persistent_environment(
            crate::env::scoped_environment(&[PathBuf::from("/runtime")], Path::new("/workspace")),
            selected,
        );
        assert_eq!(environment.len(), 4);
        assert_eq!(environment[0], ("HOME".into(), "/workspace".into()));
        assert_eq!(environment[1], ("TMPDIR".into(), "/workspace".into()));
        assert_eq!(environment[2].0, "PATH");
        assert_eq!(
            environment[3],
            ("TEST_API_TOKEN".into(), "dummy-test-only-value".into())
        );
    }

    #[test]
    fn invalid_and_managed_names_are_rejected_case_insensitively() {
        for name in [
            "",
            "1TOKEN",
            "BAD-NAME",
            "A=B",
            "A\0B",
            "ÜBER",
            "HOME",
            "path",
            "TmpDir",
            "http_proxy",
            "NO_PROXY",
            "LD_PRELOAD",
            "DYLD_INSERT_LIBRARIES",
            "PYTHONPATH",
            "NODE_OPTIONS",
            "BUN_OPTIONS",
            "UV_INDEX_URL",
            "MISE_CONFIG_DIR",
            "VIRTUAL_ENV",
            "NPM_CONFIG_PREFIX",
            "PNPM_HOME",
            "COREPACK_HOME",
            "ENV",
            "BASH_ENV",
            "IFS",
            "SUBSPACE_PROTOCOL",
            "MXC_CONFIG",
            "BOARD_LAB_PROTOCOL",
        ] {
            assert!(
                validate_environment_name(name).is_err(),
                "accepted {name:?}"
            );
        }
        for name in ["API_TOKEN", "test_value", "_PRIVATE", "VALUE2"] {
            assert!(validate_environment_name(name).is_ok(), "rejected {name:?}");
        }
    }

    #[test]
    fn duplicate_names_and_nul_values_fail_without_disclosing_values() {
        let value = "dummy-test-only-secret";
        for selected in [
            vec![
                ("TOKEN".into(), value.into()),
                ("TOKEN".into(), "other".into()),
            ],
            vec![("TOKEN".into(), format!("{value}\0"))],
            vec![("HOME".into(), value.into())],
        ] {
            let error = validate_selected_environment(&selected).unwrap_err();
            assert!(!format!("{error:?}").contains(value));
        }
    }

    #[test]
    fn selected_environment_enforces_count_value_and_total_budgets() {
        let maximum = selected_values(64, 0);
        assert!(validate_selected_environment(&maximum).is_ok());
        let mut too_many = maximum;
        too_many.push(("EXTRA".into(), String::new()));
        assert!(validate_selected_environment(&too_many).is_err());
        assert!(validate_selected_environment(&[("TOKEN".into(), "x".repeat(16 * 1024))]).is_ok());
        assert!(
            validate_selected_environment(&[("TOKEN".into(), "x".repeat(16 * 1024 + 1))]).is_err()
        );
        let total = selected_values(8, 16 * 1024);
        assert!(validate_selected_environment(&total).is_err());
        let boundary = selected_values(8, 16 * 1024 - 7);
        assert!(validate_selected_environment(&boundary).is_ok());
        let mut above = boundary;
        above[0].1.push('x');
        assert!(validate_selected_environment(&above).is_err());
    }
}
