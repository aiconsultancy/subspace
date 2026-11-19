//! Why a session could not start, or could not take input (REQ-28).

use crate::program::ProgramName;

/// A session failure that names the failing program or operation with the
/// operating system message.
///
/// A failure touches no other session, so the board can offer **Retry** and
/// start again once the cause is corrected.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SessionError {
    /// The pseudo-terminal could not be created.
    #[error("the pseudo-terminal could not be created: {message}")]
    PseudoTerminal {
        /// The operating system message.
        message: String,
    },
    /// The program could not start.
    #[error("{program} could not start: {message}")]
    Start {
        /// The program that did not start.
        program: ProgramName,
        /// The operating system message.
        message: String,
    },
    /// The terminal emulator could not be created.
    #[error("the terminal emulator could not be created: {message}")]
    Emulator {
        /// The libghostty message.
        message: String,
    },
    /// The program could not receive input.
    #[error("{program} could not receive input: {message}")]
    Input {
        /// The program that did not receive the input.
        program: ProgramName,
        /// The operating system message.
        message: String,
    },
}

impl SessionError {
    /// The program the failure names, when it names one.
    pub fn program(&self) -> Option<&ProgramName> {
        match self {
            Self::Start { program, .. } | Self::Input { program, .. } => Some(program),
            Self::PseudoTerminal { .. } | Self::Emulator { .. } => None,
        }
    }
}

/// A pseudo-terminal failure, with the cause chain that carries the operating
/// system message.
pub(crate) fn pseudo_terminal_error(error: anyhow::Error) -> SessionError {
    SessionError::PseudoTerminal {
        message: format!("{error:#}"),
    }
}

/// A start failure that names the program and its cause chain.
pub(crate) fn start_error(program: &ProgramName, error: &anyhow::Error) -> SessionError {
    SessionError::Start {
        program: program.clone(),
        message: format!("{error:#}"),
    }
}

/// An emulator failure.
pub(crate) fn emulator_error(error: libghostty_vt::Error) -> SessionError {
    SessionError::Emulator {
        message: error.to_string(),
    }
}

/// An input failure that names the program.
pub(crate) fn input_error(program: &ProgramName, message: &str) -> SessionError {
    SessionError::Input {
        program: program.clone(),
        message: message.to_owned(),
    }
}

/// A thread that could not be started, reported as a pseudo-terminal failure
/// because the session's reader is part of opening the pseudo-terminal.
pub(crate) fn reader_error(error: &std::io::Error) -> SessionError {
    SessionError::PseudoTerminal {
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use crate::error::{SessionError, input_error, start_error};
    use crate::program::ProgramName;

    #[test]
    fn a_start_failure_names_the_program_and_the_cause_chain() {
        let cause = anyhow::anyhow!("No such file or directory").context("cannot spawn /bin/nope");

        let error = start_error(&ProgramName::from_path("/bin/nope"), &cause);

        assert_eq!(
            error.to_string(),
            "nope could not start: cannot spawn /bin/nope: No such file or directory"
        );
    }

    #[test]
    fn a_start_failure_reports_its_program() {
        let error = input_error(&ProgramName::from_path("/bin/zsh"), "broken pipe");

        assert_eq!(error.program(), Some(&ProgramName::from_path("zsh")));
    }

    #[test]
    fn an_emulator_failure_names_no_program() {
        let error = SessionError::Emulator {
            message: "out of memory".to_owned(),
        };

        assert_eq!(error.program(), None);
    }
}
