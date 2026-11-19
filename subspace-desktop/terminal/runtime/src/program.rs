//! The program a session runs (REQ-08) and the name of the program that holds
//! the foreground, which tasks 03 and 10 need for REQ-10 and REQ-34.

use std::ffi::OsStr;
use std::path::Path;

use crate::environment::Probe;

/// The shell a session runs when `SHELL` is unset or empty.
#[cfg(not(windows))]
pub const DEFAULT_SHELL: &str = "/bin/sh";

/// The shell a session runs when `SHELL` is unset or empty and neither
/// PowerShell nor `ComSpec` is found.
#[cfg(windows)]
pub const DEFAULT_SHELL: &str = "cmd.exe";

/// The shells a Windows session prefers, searched for on `PATH` in order.
#[cfg(windows)]
const WINDOWS_SHELLS: [&str; 2] = ["pwsh.exe", "powershell.exe"];

/// The displayed name of a program: a file name, never a path.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProgramName(String);

impl ProgramName {
    /// The file name of a program path.
    pub fn from_path(path: &str) -> Self {
        Self(file_name(path).to_owned())
    }

    /// The name as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

// qual:allow(dry, boilerplate) reason: "A program name is shown to the user and interpolated into REQ-28 messages, so it formats as its own text."
impl std::fmt::Display for ProgramName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// What a session runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    /// The Ghostty `command` value, split into an executable and arguments.
    Command {
        /// The program to run, as written in the configuration.
        executable: String,
        /// The remaining words of the `command` value.
        arguments: Vec<String>,
    },
    /// The user's shell. On Unix it runs as a login shell, with `-<name>` as
    /// its `argv[0]`; Windows has no login shells, so it runs as given.
    LoginShell {
        /// The shell path: `SHELL`, or the platform default when it is unset.
        path: String,
    },
}

impl Program {
    /// The executable this program runs.
    pub fn executable(&self) -> &str {
        match self {
            Self::Command { executable, .. } => executable,
            Self::LoginShell { path } => path,
        }
    }

    /// The displayed name, which a tab shows until the program sets a title.
    pub fn name(&self) -> ProgramName {
        ProgramName::from_path(self.executable())
    }
}

/// Resolve the program per REQ-08: the Ghostty `command` value when it is set,
/// otherwise `SHELL` as a login shell, and [`DEFAULT_SHELL`] when `SHELL` is
/// unset or empty.
///
/// A `SHELL` that names a missing or non-executable file is kept rather than
/// replaced, so starting it fails and REQ-28 names the path the user
/// configured. See "REQ-08 and REQ-28" in the crate documentation.
pub fn resolve_program(configured: Option<&str>, probe: &dyn Probe) -> Program {
    configured
        .and_then(command_program)
        .unwrap_or_else(|| login_shell(probe))
}

/// The Ghostty `command` value, split the way a shell splits a command line.
fn command_program(line: &str) -> Option<Program> {
    let words = shell_words::split(line).ok()?;
    let (executable, arguments) = words.split_first()?;
    Some(Program::Command {
        executable: executable.clone(),
        arguments: arguments.to_vec(),
    })
}

/// `SHELL` as a login shell, or the platform default when it is unset or
/// empty.
fn login_shell(probe: &dyn Probe) -> Program {
    let path = non_empty_var("SHELL", probe).unwrap_or_else(|| default_shell(probe));
    Program::LoginShell { path }
}

/// A variable's trimmed value, when it is set and not blank.
fn non_empty_var(name: &str, probe: &dyn Probe) -> Option<String> {
    probe
        .var(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// The shell a session runs when `SHELL` is unset or empty.
#[cfg(not(windows))]
fn default_shell(_probe: &dyn Probe) -> String {
    DEFAULT_SHELL.to_owned()
}

/// PowerShell 7 or Windows PowerShell from `PATH`, then `ComSpec`, then
/// [`DEFAULT_SHELL`].
#[cfg(windows)]
fn default_shell(probe: &dyn Probe) -> String {
    WINDOWS_SHELLS
        .iter()
        .find_map(|name| find_on_path(name, probe))
        .or_else(|| non_empty_var("ComSpec", probe))
        .unwrap_or_else(|| DEFAULT_SHELL.to_owned())
}

/// The full path of the first `PATH` entry that holds the file.
#[cfg(windows)]
fn find_on_path(name: &str, probe: &dyn Probe) -> Option<String> {
    let search = probe.var("PATH")?;
    std::env::split_paths(&search)
        .map(|directory| directory.join(name))
        .find(|candidate| probe.is_file(candidate))
        .map(|candidate| candidate.display().to_string())
}

/// The file name of a path, or the whole value when it has none.
fn file_name(path: &str) -> &str {
    Path::new(path)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or(path)
}

/// The name of a running process, read from the operating system rather than
/// from a subprocess.
#[cfg(target_os = "linux")]
pub(crate) fn process_name(pid: u32) -> Option<ProgramName> {
    let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = comm.trim();
    (!name.is_empty()).then(|| ProgramName(name.to_owned()))
}

/// The name of a running process, read from the operating system rather than
/// from a subprocess.
#[cfg(target_os = "macos")]
pub(crate) fn process_name(pid: u32) -> Option<ProgramName> {
    process_path(pid).map(|path| ProgramName::from_path(&path))
}

/// The executable path of a running process.
#[cfg(target_os = "macos")]
fn process_path(pid: u32) -> Option<String> {
    let pid = i32::try_from(pid).ok()?;
    let mut buffer = [0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
    // SAFETY: `proc_pidpath` writes at most `buffersize` bytes into `buffer`,
    // which is owned here and outlives the call. An unknown pid returns a
    // non-positive length and leaves the buffer alone.
    let written = unsafe {
        libc::proc_pidpath(
            pid,
            buffer.as_mut_ptr().cast(),
            u32::try_from(buffer.len()).unwrap_or(u32::MAX),
        )
    };
    let length = usize::try_from(written).ok().filter(|length| *length > 0)?;
    String::from_utf8(buffer[..length].to_vec()).ok()
}

/// Other systems report no process name; the shell's own name is shown.
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(crate) fn process_name(_pid: u32) -> Option<ProgramName> {
    None
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use crate::environment::Probe;
    use crate::program::{DEFAULT_SHELL, Program, ProgramName, resolve_program};

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

        #[cfg_attr(not(windows), allow(dead_code))]
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
    fn the_configured_command_wins() {
        let probe = Fake::default().with_var("SHELL", "/bin/zsh");

        let program = resolve_program(Some("nvim --clean"), &probe);

        assert_eq!(
            program,
            Program::Command {
                executable: "nvim".to_owned(),
                arguments: vec!["--clean".to_owned()],
            }
        );
    }

    #[test]
    fn quoted_command_words_stay_together() {
        let probe = Fake::default();

        let program = resolve_program(Some("/bin/sh -c 'echo one two'"), &probe);

        assert_eq!(
            program,
            Program::Command {
                executable: "/bin/sh".to_owned(),
                arguments: vec!["-c".to_owned(), "echo one two".to_owned()],
            }
        );
    }

    #[test]
    fn an_empty_command_falls_through_to_the_shell() {
        let probe = Fake::default().with_var("SHELL", "/bin/zsh");

        let program = resolve_program(Some("   "), &probe);

        assert_eq!(
            program,
            Program::LoginShell {
                path: "/bin/zsh".to_owned(),
            }
        );
    }

    #[test]
    fn the_shell_runs_as_a_login_shell() {
        let probe = Fake::default().with_var("SHELL", "/bin/bash");

        let program = resolve_program(None, &probe);

        assert_eq!(
            program,
            Program::LoginShell {
                path: "/bin/bash".to_owned(),
            }
        );
    }

    #[test]
    fn an_unset_shell_falls_back_to_bin_sh() {
        let probe = Fake::default();

        let program = resolve_program(None, &probe);

        assert_eq!(
            program,
            Program::LoginShell {
                path: DEFAULT_SHELL.to_owned(),
            }
        );
    }

    #[test]
    fn an_empty_shell_falls_back_to_bin_sh() {
        let probe = Fake::default().with_var("SHELL", "  ");

        let program = resolve_program(None, &probe);

        assert_eq!(program.executable(), DEFAULT_SHELL);
    }

    #[test]
    fn a_missing_shell_is_kept_so_the_failure_names_it() {
        let probe = Fake::default().with_var("SHELL", "/missing/shell");

        let program = resolve_program(None, &probe);

        assert_eq!(program.executable(), "/missing/shell");
    }

    #[test]
    fn the_name_is_the_file_name() {
        let program = Program::LoginShell {
            path: "/opt/homebrew/bin/fish".to_owned(),
        };

        assert_eq!(program.name(), ProgramName::from_path("fish"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_prefers_powershell_7_from_path() {
        let probe = Fake::default()
            .with_var("PATH", r"C:\Tools;C:\Program Files\PowerShell\7")
            .with_var("ComSpec", r"C:\Windows\system32\cmd.exe")
            .with_file(r"C:\Program Files\PowerShell\7\pwsh.exe");

        let program = resolve_program(None, &probe);

        assert_eq!(
            program.executable(),
            r"C:\Program Files\PowerShell\7\pwsh.exe"
        );
        assert_eq!(program.name(), ProgramName::from_path("pwsh.exe"));
    }

    #[cfg(windows)]
    #[test]
    fn windows_falls_back_to_windows_powershell_then_comspec() {
        let powershell = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe";
        let with_powershell = Fake::default()
            .with_var("PATH", r"C:\Windows\System32\WindowsPowerShell\v1.0")
            .with_var("ComSpec", r"C:\Windows\system32\cmd.exe")
            .with_file(powershell);
        let without_powershell = Fake::default()
            .with_var("PATH", r"C:\Tools")
            .with_var("ComSpec", r"C:\Windows\system32\cmd.exe");

        assert_eq!(
            resolve_program(None, &with_powershell).executable(),
            powershell
        );
        assert_eq!(
            resolve_program(None, &without_powershell).executable(),
            r"C:\Windows\system32\cmd.exe"
        );
    }

    #[cfg(unix)]
    #[test]
    fn this_process_has_a_name() {
        let pid = std::process::id();

        let name = crate::program::process_name(pid);

        assert!(
            name.is_some_and(|name| !name.as_str().is_empty()),
            "the running test process should have a name"
        );
    }
}
