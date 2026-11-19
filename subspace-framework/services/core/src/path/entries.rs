//! `PATH` values as lists of folders.

use std::ffi::OsString;
use std::path::PathBuf;

/// The non-empty entries of a `PATH` value, without duplicates, in order.
pub(super) fn distinct(value: &OsString) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = Vec::new();
    for entry in std::env::split_paths(value) {
        if !entry.as_os_str().is_empty() && !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    entries
}

/// `first` entries in order, then `second` entries that `first` lacks.
/// Empty entries and duplicates are dropped.
pub(super) fn merge(first: &OsString, second: &OsString) -> OsString {
    let mut entries = distinct(first);
    for entry in distinct(second) {
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    std::env::join_paths(&entries).unwrap_or_else(|_| second.clone())
}

/// The `variables` whose names `present` does not report as already set.
pub(super) fn missing_variables(
    variables: Vec<(String, String)>,
    present: impl Fn(&str) -> bool,
) -> Vec<(String, String)> {
    variables
        .into_iter()
        .filter(|(name, _)| !present(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::{distinct, merge, missing_variables};

    fn joined(entries: &[&str]) -> OsString {
        std::env::join_paths(entries).unwrap()
    }

    #[test]
    fn login_entries_come_first_and_duplicates_drop() {
        let shell = joined(&["/opt/homebrew/bin", "/usr/bin", "/bin"]);
        let current = joined(&["/usr/bin", "/bin", "/usr/sbin", "/sbin"]);
        let path = merge(&shell, &current);
        assert_eq!(
            path,
            joined(&[
                "/opt/homebrew/bin",
                "/usr/bin",
                "/bin",
                "/usr/sbin",
                "/sbin"
            ])
        );
        assert_eq!(distinct(&path).len() - distinct(&current).len(), 1);
    }

    #[test]
    fn reordering_alone_is_a_change() {
        // Same entries, different precedence: nothing is added, but the
        // login shell's order must still apply.
        let current = joined(&["/usr/bin", "/opt/homebrew/bin"]);
        let path = merge(&joined(&["/opt/homebrew/bin", "/usr/bin"]), &current);
        assert_eq!(distinct(&path).len(), distinct(&current).len());
        assert_ne!(path, current);
    }

    #[test]
    fn inherited_first_keeps_session_order_and_appends() {
        // The Windows order: a terminal's PATH leads, the registry appends.
        let current = joined(&["/session/tools", "/usr/bin"]);
        let path = merge(&current, &joined(&["/usr/bin", "/new/tool"]));
        assert_eq!(path, joined(&["/session/tools", "/usr/bin", "/new/tool"]));
    }

    #[test]
    fn the_login_shell_supplies_only_network_variables_this_process_lacks() {
        let found = vec![
            (
                "HTTPS_PROXY".to_string(),
                "http://proxy.fixture.invalid:8080".to_string(),
            ),
            ("no_proxy".to_string(), "localhost".to_string()),
        ];
        let missing = missing_variables(found, |name| name == "no_proxy");
        assert_eq!(
            missing
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
            ["HTTPS_PROXY"]
        );
    }
}
