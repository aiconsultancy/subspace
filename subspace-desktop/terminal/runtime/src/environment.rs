//! The session environment and start directory of REQ-08.
//!
//! Every rule is a pure function over an injected [`Probe`], so the tests read
//! fixtures rather than the real environment, the real home directory or an
//! installed Ghostty.

use std::path::{Path, PathBuf};

/// `TERM` when an `xterm-ghostty` terminfo entry exists.
pub const GHOSTTY_TERM: &str = "xterm-ghostty";

/// `TERM` when no `xterm-ghostty` entry exists.
pub const FALLBACK_TERM: &str = "xterm-256color";

/// `COLORTERM`, which every session sets so programs offer 24-bit colour.
pub const COLORTERM: &str = "truecolor";

/// Terminfo databases ncurses reads without a `TERMINFO` variable.
const SYSTEM_TERMINFO_DIRECTORIES: [&str; 4] = [
    "/usr/share/terminfo",
    "/usr/share/misc/terminfo",
    "/lib/terminfo",
    "/etc/terminfo",
];

/// Terminfo databases an installed Ghostty ships, outside the system ones.
#[cfg(target_os = "macos")]
const GHOSTTY_TERMINFO_DIRECTORIES: [&str; 2] = [
    "/Applications/Ghostty.app/Contents/Resources/terminfo",
    "/opt/homebrew/Caskroom/ghostty/Ghostty.app/Contents/Resources/terminfo",
];

/// Terminfo databases an installed Ghostty ships, outside the system ones.
#[cfg(not(target_os = "macos"))]
const GHOSTTY_TERMINFO_DIRECTORIES: [&str; 3] = [
    "/usr/local/share/terminfo",
    "/opt/ghostty/share/terminfo",
    "/var/lib/flatpak/app/com.mitchellh.ghostty/current/active/files/share/terminfo",
];

/// The environment and filesystem facts REQ-08 resolution needs.
///
/// Both methods are read-only, so a test fake is a table of answers.
pub trait Probe {
    /// The raw value of an environment variable, or `None` when it is unset.
    fn var(&self, name: &str) -> Option<String>;

    /// Whether the path names an existing file.
    fn is_file(&self, path: &Path) -> bool;
}

/// The [`Probe`] that reads the real process environment and filesystem.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemProbe;

impl Probe for SystemProbe {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }

    fn is_file(&self, path: &Path) -> bool {
        path.is_file()
    }
}

/// The `TERM` value for a session, with the terminfo database it needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermSelection {
    /// The `TERM` value.
    pub term: String,
    /// The directory to publish as `TERMINFO`, when the system database does
    /// not hold the entry.
    pub terminfo_directory: Option<PathBuf>,
}

impl TermSelection {
    /// A `TERM` the system terminfo database already resolves.
    fn system(term: &str) -> Self {
        Self {
            term: term.to_owned(),
            terminfo_directory: None,
        }
    }

    /// `xterm-ghostty` from an installed Ghostty's own terminfo directory.
    fn bundled(directory: PathBuf) -> Self {
        Self {
            term: GHOSTTY_TERM.to_owned(),
            terminfo_directory: Some(directory),
        }
    }
}

/// Resolve `TERM` per REQ-08: the Ghostty `term` value, else `xterm-ghostty`
/// when a terminfo entry exists, else `xterm-256color`.
pub fn resolve_term(configured: Option<&str>, probe: &dyn Probe) -> TermSelection {
    configured
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(TermSelection::system)
        .or_else(|| ghostty_term(probe))
        .unwrap_or_else(|| TermSelection::system(FALLBACK_TERM))
}

/// `xterm-ghostty` when the system database or an installed Ghostty has it.
fn ghostty_term(probe: &dyn Probe) -> Option<TermSelection> {
    system_term(probe).or_else(|| bundled_term(probe))
}

/// `xterm-ghostty` from a database ncurses reads on its own.
fn system_term(probe: &dyn Probe) -> Option<TermSelection> {
    first_directory_with_entry(&system_terminfo_directories(probe), GHOSTTY_TERM, probe)
        .map(|_| TermSelection::system(GHOSTTY_TERM))
}

/// `xterm-ghostty` from an installed Ghostty, which needs `TERMINFO`.
fn bundled_term(probe: &dyn Probe) -> Option<TermSelection> {
    first_directory_with_entry(&ghostty_terminfo_directories(probe), GHOSTTY_TERM, probe)
        .map(TermSelection::bundled)
}

/// The variables a session sets, in the order the command builder applies them.
pub fn session_variables(selection: &TermSelection) -> Vec<(&'static str, String)> {
    let mut variables = vec![
        ("TERM", selection.term.clone()),
        ("COLORTERM", COLORTERM.to_owned()),
    ];
    if let Some(directory) = &selection.terminfo_directory {
        variables.push(("TERMINFO", directory.display().to_string()));
    }
    variables
}

/// Resolve the start directory per REQ-08: the Ghostty `working-directory`
/// value when it is an absolute path or `~/…`, and the home directory for
/// `home`, for an unsupported value and when nothing is configured.
pub fn resolve_working_directory(configured: Option<&str>, probe: &dyn Probe) -> Option<PathBuf> {
    configured
        .and_then(|value| configured_directory(value, probe))
        .or_else(|| home_directory(probe))
}

/// The configured directory when REQ-08 accepts the value.
fn configured_directory(value: &str, probe: &dyn Probe) -> Option<PathBuf> {
    let value = value.trim();
    match value.strip_prefix("~/") {
        Some(relative) => home_relative_directory(relative, probe),
        None => absolute_directory(value),
    }
}

/// A `~/…` value resolved against the home directory.
fn home_relative_directory(relative: &str, probe: &dyn Probe) -> Option<PathBuf> {
    home_directory(probe).map(|home| home.join(relative))
}

/// The value itself when it is an absolute path.
fn absolute_directory(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    path.is_absolute().then(|| path.to_path_buf())
}

/// The home directory, from `HOME`, or on Windows from `USERPROFILE` when
/// `HOME` is unset.
pub fn home_directory(probe: &dyn Probe) -> Option<PathBuf> {
    HOME_VARIABLES
        .iter()
        .find_map(|name| probe.var(name).filter(|home| !home.is_empty()))
        .map(PathBuf::from)
}

/// The variables that name the home directory, most specific first.
#[cfg(not(windows))]
const HOME_VARIABLES: [&str; 1] = ["HOME"];

/// The variables that name the home directory, most specific first. Windows
/// sets only `USERPROFILE`; `HOME` wins when a user sets it.
#[cfg(windows)]
const HOME_VARIABLES: [&str; 2] = ["HOME", "USERPROFILE"];

/// The terminfo databases ncurses resolves on its own, most specific first.
/// An inherited `TERMINFO` counts as one of them, because the session inherits
/// the variable with the rest of the environment.
fn system_terminfo_directories(probe: &dyn Probe) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(terminfo) = probe.var("TERMINFO").filter(|value| !value.is_empty()) {
        directories.push(PathBuf::from(terminfo));
    }
    if let Some(home) = home_directory(probe) {
        directories.push(home.join(".terminfo"));
    }
    directories.extend(terminfo_dirs(probe));
    directories.extend(SYSTEM_TERMINFO_DIRECTORIES.iter().map(PathBuf::from));
    directories
}

/// The entries of `TERMINFO_DIRS`, where an empty entry means the compiled-in
/// default database.
fn terminfo_dirs(probe: &dyn Probe) -> Vec<PathBuf> {
    let Some(value) = probe.var("TERMINFO_DIRS") else {
        return Vec::new();
    };
    value
        .split(':')
        .filter(|entry| !entry.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// The terminfo databases an installed Ghostty ships, most specific first.
fn ghostty_terminfo_directories(probe: &dyn Probe) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(resources) = probe.var("GHOSTTY_RESOURCES_DIR") {
        directories.push(Path::new(&resources).join("terminfo"));
    }
    if let Some(home) = home_directory(probe) {
        directories.push(home.join("Applications/Ghostty.app/Contents/Resources/terminfo"));
        directories.push(home.join(".local/share/terminfo"));
    }
    directories.extend(GHOSTTY_TERMINFO_DIRECTORIES.iter().map(PathBuf::from));
    directories
}

/// The first listed directory whose database holds the entry.
fn first_directory_with_entry(
    directories: &[PathBuf],
    name: &str,
    probe: &dyn Probe,
) -> Option<PathBuf> {
    directories
        .iter()
        .find(|directory| directory_has_entry(directory, name, probe))
        .cloned()
}

/// Whether a terminfo database holds an entry, in either layout ncurses
/// writes: a directory named after the first letter, or after its hexadecimal
/// character code.
fn directory_has_entry(directory: &Path, name: &str, probe: &dyn Probe) -> bool {
    let Some(first) = name.chars().next() else {
        return false;
    };
    let letter = directory.join(first.to_string()).join(name);
    let hashed = directory.join(format!("{:x}", u32::from(first))).join(name);
    probe.is_file(&letter) || probe.is_file(&hashed)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use crate::environment::{
        COLORTERM, FALLBACK_TERM, GHOSTTY_TERM, Probe, TermSelection, resolve_term,
        resolve_working_directory, session_variables,
    };

    /// A table of environment variables and existing files.
    #[derive(Default)]
    struct Fake {
        variables: HashMap<String, String>,
        files: Vec<PathBuf>,
    }

    impl Fake {
        fn with_var(mut self, name: &str, value: &str) -> Self {
            self.variables.insert(name.to_owned(), value.to_owned());
            self
        }

        fn with_file(mut self, path: &str) -> Self {
            self.files.push(PathBuf::from(path));
            self
        }
    }

    impl Probe for Fake {
        fn var(&self, name: &str) -> Option<String> {
            self.variables.get(name).cloned()
        }

        fn is_file(&self, path: &Path) -> bool {
            self.files.iter().any(|file| file == path)
        }
    }

    #[test]
    fn configured_term_wins() {
        let probe = Fake::default().with_file("/usr/share/terminfo/x/xterm-ghostty");

        let selection = resolve_term(Some("screen-256color"), &probe);

        assert_eq!(
            selection,
            TermSelection {
                term: "screen-256color".to_owned(),
                terminfo_directory: None,
            }
        );
    }

    #[test]
    fn blank_configured_term_is_ignored() {
        let probe = Fake::default();

        let selection = resolve_term(Some("  "), &probe);

        assert_eq!(selection.term, FALLBACK_TERM);
    }

    #[test]
    fn system_database_needs_no_terminfo_variable() {
        let probe = Fake::default().with_file("/usr/share/terminfo/78/xterm-ghostty");

        let selection = resolve_term(None, &probe);

        assert_eq!(
            selection,
            TermSelection {
                term: GHOSTTY_TERM.to_owned(),
                terminfo_directory: None,
            }
        );
    }

    #[test]
    fn home_database_counts_as_a_system_database() {
        let probe = Fake::default()
            .with_var("HOME", "/home/fixture")
            .with_file("/home/fixture/.terminfo/x/xterm-ghostty");

        let selection = resolve_term(None, &probe);

        assert_eq!(selection.terminfo_directory, None);
    }

    #[test]
    fn installed_ghostty_supplies_the_terminfo_directory() {
        let probe = Fake::default()
            .with_var("GHOSTTY_RESOURCES_DIR", "/opt/ghostty/share/ghostty")
            .with_file("/opt/ghostty/share/ghostty/terminfo/x/xterm-ghostty");

        let selection = resolve_term(None, &probe);

        assert_eq!(
            selection,
            TermSelection {
                term: GHOSTTY_TERM.to_owned(),
                terminfo_directory: Some(PathBuf::from("/opt/ghostty/share/ghostty/terminfo")),
            }
        );
    }

    #[test]
    fn terminfo_dirs_entries_are_searched() {
        let probe = Fake::default()
            .with_var("TERMINFO_DIRS", "/first:/second")
            .with_file("/second/x/xterm-ghostty");

        let selection = resolve_term(None, &probe);

        assert_eq!(selection.term, GHOSTTY_TERM);
    }

    #[test]
    fn no_entry_falls_back_to_xterm_256color() {
        let probe = Fake::default().with_var("HOME", "/home/fixture");

        let selection = resolve_term(None, &probe);

        assert_eq!(
            selection,
            TermSelection {
                term: FALLBACK_TERM.to_owned(),
                terminfo_directory: None,
            }
        );
    }

    #[test]
    fn variables_always_declare_truecolor() {
        let probe = Fake::default();

        let variables = session_variables(&resolve_term(Some("xterm"), &probe));

        assert_eq!(
            variables,
            vec![
                ("TERM", "xterm".to_owned()),
                ("COLORTERM", COLORTERM.to_owned()),
            ]
        );
    }

    #[test]
    fn variables_publish_a_bundled_terminfo_directory() {
        let probe = Fake::default()
            .with_var("GHOSTTY_RESOURCES_DIR", "/opt/ghostty")
            .with_file("/opt/ghostty/terminfo/x/xterm-ghostty");

        let variables = session_variables(&resolve_term(None, &probe));

        let terminfo = Path::new("/opt/ghostty").join("terminfo");
        assert!(variables.contains(&("TERMINFO", terminfo.display().to_string())));
    }

    #[test]
    fn home_is_the_default_directory() {
        let probe = Fake::default().with_var("HOME", "/home/fixture");

        let directory = resolve_working_directory(None, &probe);

        assert_eq!(directory, Some(PathBuf::from("/home/fixture")));
    }

    #[test]
    fn the_home_value_means_the_home_directory() {
        let probe = Fake::default().with_var("HOME", "/home/fixture");

        let directory = resolve_working_directory(Some("home"), &probe);

        assert_eq!(directory, Some(PathBuf::from("/home/fixture")));
    }

    #[test]
    fn an_absolute_value_is_used_as_given() {
        // A Windows path is absolute only with a drive.
        let absolute = if cfg!(windows) {
            r"C:\srv\work"
        } else {
            "/srv/work"
        };
        let probe = Fake::default().with_var("HOME", "/home/fixture");

        let directory = resolve_working_directory(Some(absolute), &probe);

        assert_eq!(directory, Some(PathBuf::from(absolute)));
    }

    #[test]
    fn a_tilde_value_resolves_against_home() {
        let probe = Fake::default().with_var("HOME", "/home/fixture");

        let directory = resolve_working_directory(Some("~/Code/project"), &probe);

        assert_eq!(directory, Some(PathBuf::from("/home/fixture/Code/project")));
    }

    #[test]
    fn a_relative_value_falls_back_to_home() {
        let probe = Fake::default().with_var("HOME", "/home/fixture");

        let directory = resolve_working_directory(Some("Code/project"), &probe);

        assert_eq!(directory, Some(PathBuf::from("/home/fixture")));
    }

    #[test]
    fn an_unknown_home_leaves_the_directory_unset() {
        let probe = Fake::default();

        let directory = resolve_working_directory(Some("home"), &probe);

        assert_eq!(directory, None);
    }

    #[cfg(windows)]
    #[test]
    fn windows_uses_the_user_profile_when_home_is_unset() {
        let profile = Fake::default().with_var("USERPROFILE", r"C:\Users\fixture");
        let both = Fake::default()
            .with_var("USERPROFILE", r"C:\Users\fixture")
            .with_var("HOME", r"D:\home");

        assert_eq!(
            resolve_working_directory(None, &profile),
            Some(PathBuf::from(r"C:\Users\fixture"))
        );
        assert_eq!(
            resolve_working_directory(None, &both),
            Some(PathBuf::from(r"D:\home"))
        );
    }
}
