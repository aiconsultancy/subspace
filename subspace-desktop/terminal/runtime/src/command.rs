//! The command a session starts: the program REQ-08 resolves, the environment
//! it runs with and the directory it starts in.

use std::path::{Path, PathBuf};

use portable_pty::CommandBuilder;

use crate::environment::{self, Probe, TermSelection};
use crate::program::{self, Program};

/// The Ghostty `scrollback-limit` default, in bytes (REQ-16).
pub const DEFAULT_SCROLLBACK_LIMIT_BYTES: usize = 10_000_000;

/// What a session should run, taken from the Ghostty configuration by the
/// caller. A plain input type: this crate does not read configuration files.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionOptions {
    /// The Ghostty `command` value.
    pub command: Option<String>,
    /// The Ghostty `working-directory` value: `home`, an absolute path or
    /// `~/…`.
    pub working_directory: Option<String>,
    /// The Ghostty `term` value.
    pub term: Option<String>,
    /// The Ghostty `scrollback-limit` value, in bytes.
    pub scrollback_limit_bytes: Option<usize>,
    /// Variables applied only to the child process. The terminal crate does
    /// not own or mutate the application's environment.
    pub environment: Vec<(String, String)>,
}

/// The resolved facts a session starts from.
pub(crate) struct Launch {
    pub(crate) program: Program,
    pub(crate) term: TermSelection,
    pub(crate) directory: Option<PathBuf>,
    pub(crate) scrollback_limit_bytes: Option<usize>,
    pub(crate) environment: Vec<(String, String)>,
}

/// Resolve the program, the environment and the start directory (REQ-08).
pub(crate) fn resolve(options: &SessionOptions, probe: &dyn Probe) -> Launch {
    Launch {
        program: program::resolve_program(options.command.as_deref(), probe),
        term: environment::resolve_term(options.term.as_deref(), probe),
        directory: environment::resolve_working_directory(
            options.working_directory.as_deref(),
            probe,
        ),
        scrollback_limit_bytes: options.scrollback_limit_bytes,
        environment: options.environment.clone(),
    }
}

/// Build the command for a resolved program, environment and directory.
pub(crate) fn build_command(launch: &Launch) -> CommandBuilder {
    let mut command = program_command(&launch.program);
    apply_environment(&mut command, &launch.term);
    apply_child_environment(&mut command, &launch.environment);
    apply_directory(&mut command, launch.directory.as_deref());
    command
}

/// The command for a resolved program.
fn program_command(program: &Program) -> CommandBuilder {
    match program {
        Program::Command {
            executable,
            arguments,
        } => explicit_command(executable, arguments),
        Program::LoginShell { path } => login_shell_command(path),
    }
}

/// A command the Ghostty `command` value named.
fn explicit_command(executable: &str, arguments: &[String]) -> CommandBuilder {
    let mut command = CommandBuilder::new(executable);
    command.args(arguments);
    command
}

/// A shell run as a login shell.
///
/// portable-pty runs its default program as a login shell, prefixing the
/// shell's file name with `-` as `argv[0]`, and takes the shell from the
/// builder's `SHELL`. It does that only for a shell it can execute: for
/// anything else it substitutes the password-database shell, which would hide
/// the broken configuration REQ-28 has to report. A shell that cannot be
/// executed is therefore started by path, and fails naming that path.
#[cfg(unix)]
fn login_shell_command(path: &str) -> CommandBuilder {
    if !is_executable(Path::new(path)) {
        return CommandBuilder::new(path);
    }
    let mut command = CommandBuilder::new_default_prog();
    command.env("SHELL", path);
    command
}

/// The shell, started by path: Windows has no login shells, and
/// portable-pty's default program there is `ComSpec`, not the resolved shell.
/// A missing shell fails naming that path (REQ-28).
#[cfg(windows)]
fn login_shell_command(path: &str) -> CommandBuilder {
    CommandBuilder::new(path)
}

/// Whether the current user can execute the file.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
}

/// Set `TERM`, `COLORTERM` and, when it is needed, `TERMINFO` (REQ-08).
fn apply_environment(command: &mut CommandBuilder, term: &TermSelection) {
    for (name, value) in environment::session_variables(term) {
        command.env(name, value);
    }
}

/// Apply caller-provided child variables after the terminal defaults. This is
/// deliberately a command-builder operation so each session gets its own
/// environment and prepared resources can be held by its owner.
pub(crate) fn apply_child_environment(
    command: &mut CommandBuilder,
    environment: &[(String, String)],
) {
    for (name, value) in environment {
        command.env(name, value);
    }
}

/// Start the program in the resolved directory, when there is one.
fn apply_directory(command: &mut CommandBuilder, directory: Option<&Path>) {
    if let Some(directory) = directory {
        command.cwd(directory);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use crate::command::{SessionOptions, build_command, resolve};
    use crate::environment::Probe;

    /// A table of environment variables, with no terminfo entries.
    #[derive(Default)]
    struct Fake {
        variables: HashMap<String, String>,
    }

    impl Fake {
        fn with_var(mut self, name: &str, value: &str) -> Self {
            self.variables.insert(name.to_owned(), value.to_owned());
            self
        }
    }

    impl Probe for Fake {
        fn var(&self, name: &str) -> Option<String> {
            self.variables.get(name).cloned()
        }

        fn is_file(&self, path: &Path) -> bool {
            path.starts_with("/opt/ghostty/terminfo")
        }
    }

    /// The command a set of options and a probe produce.
    fn command(options: &SessionOptions, probe: &Fake) -> portable_pty::CommandBuilder {
        build_command(&resolve(options, probe))
    }

    /// The value of one environment variable of a built command.
    fn variable(command: &portable_pty::CommandBuilder, name: &str) -> String {
        command
            .get_env(name)
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_owned()
    }

    #[cfg(unix)]
    #[test]
    fn an_executable_shell_runs_as_a_login_shell() {
        let probe = Fake::default().with_var("SHELL", "/bin/sh");

        let command = command(&SessionOptions::default(), &probe);

        assert!(
            command.is_default_prog(),
            "an executable shell uses the login-shell form, which sets `-sh` as argv0"
        );
        assert_eq!(variable(&command, "SHELL"), "/bin/sh");
    }

    #[test]
    fn a_missing_shell_is_started_by_path_so_the_failure_names_it() {
        let probe = Fake::default().with_var("SHELL", "/missing/shell");

        let command = command(&SessionOptions::default(), &probe);

        assert_eq!(
            command.get_argv(),
            &vec![std::ffi::OsString::from("/missing/shell")]
        );
    }

    #[test]
    fn a_configured_command_keeps_its_arguments() {
        let options = SessionOptions {
            command: Some("nvim --clean".to_owned()),
            ..SessionOptions::default()
        };

        let command = command(&options, &Fake::default());

        assert_eq!(
            command.get_argv(),
            &vec![
                std::ffi::OsString::from("nvim"),
                std::ffi::OsString::from("--clean"),
            ]
        );
    }

    #[test]
    fn the_environment_declares_term_and_truecolor() {
        let options = SessionOptions {
            term: Some("xterm-kitty".to_owned()),
            ..SessionOptions::default()
        };

        let command = command(&options, &Fake::default());

        assert_eq!(variable(&command, "TERM"), "xterm-kitty");
        assert_eq!(variable(&command, "COLORTERM"), "truecolor");
    }

    #[test]
    fn child_environment_overrides_are_kept_off_the_parent_process() {
        let options = SessionOptions {
            environment: vec![
                (
                    "HTTPS_PROXY".to_owned(),
                    "http://127.0.0.1:18080".to_owned(),
                ),
                ("KUBECONFIG".to_owned(), "/private/session.yaml".to_owned()),
            ],
            ..SessionOptions::default()
        };

        let command = command(&options, &Fake::default());

        assert_eq!(variable(&command, "HTTPS_PROXY"), "http://127.0.0.1:18080");
        assert_eq!(variable(&command, "KUBECONFIG"), "/private/session.yaml");
    }

    #[test]
    fn an_installed_ghostty_publishes_its_terminfo_directory() {
        let probe = Fake::default().with_var("GHOSTTY_RESOURCES_DIR", "/opt/ghostty");

        let command = command(&SessionOptions::default(), &probe);

        assert_eq!(variable(&command, "TERM"), "xterm-ghostty");
        assert_eq!(
            variable(&command, "TERMINFO"),
            Path::new("/opt/ghostty")
                .join("terminfo")
                .display()
                .to_string()
        );
    }

    #[test]
    fn the_command_starts_in_the_resolved_directory() {
        let options = SessionOptions {
            working_directory: Some("~/Code".to_owned()),
            ..SessionOptions::default()
        };
        let probe = Fake::default().with_var("HOME", "/home/fixture");

        let command = command(&options, &probe);

        assert_eq!(
            command.get_cwd(),
            Some(&Path::new("/home/fixture").join("Code").into_os_string())
        );
    }

    #[cfg(windows)]
    #[test]
    fn a_windows_shell_starts_by_path_without_a_login_form() {
        let probe = Fake::default().with_var("SHELL", r"C:\Program Files\PowerShell\7\pwsh.exe");

        let command = command(&SessionOptions::default(), &probe);

        assert!(!command.is_default_prog());
        assert_eq!(
            command.get_argv(),
            &vec![std::ffi::OsString::from(
                r"C:\Program Files\PowerShell\7\pwsh.exe"
            )]
        );
    }
}
