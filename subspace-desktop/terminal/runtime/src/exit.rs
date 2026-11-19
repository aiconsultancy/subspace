//! How a session's program ended (REQ-09).

/// How a session's program ended (REQ-09).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionExit {
    /// The program exited with this status code.
    Code(i32),
    /// A signal ended the program, named as the operating system names it.
    Signal(String),
}

impl SessionExit {
    /// The line REQ-09 adds to the screen when the program ends.
    pub fn marker(&self) -> String {
        match self {
            Self::Code(code) => format!("[process exited with code {code}]"),
            Self::Signal(signal) => format!("[process exited with signal {signal}]"),
        }
    }
}

/// How the program ended, by signal name or status code (REQ-09).
pub(crate) fn exit_from(status: Result<&portable_pty::ExitStatus, &std::io::Error>) -> SessionExit {
    let Ok(status) = status else {
        return SessionExit::Code(-1);
    };
    match status.signal() {
        Some(signal) => SessionExit::Signal(signal.to_owned()),
        None => SessionExit::Code(i32::try_from(status.exit_code()).unwrap_or(-1)),
    }
}

/// The REQ-09 marker, on its own line.
pub(crate) fn exit_marker(exit: &SessionExit) -> String {
    format!("\r\n{}\r\n", exit.marker())
}

#[cfg(test)]
mod tests {
    use portable_pty::ExitStatus;

    use crate::exit::{SessionExit, exit_from, exit_marker};

    #[test]
    fn the_exit_marker_names_a_code_or_a_signal() {
        assert_eq!(
            exit_marker(&SessionExit::Code(3)),
            "\r\n[process exited with code 3]\r\n"
        );
        assert_eq!(
            exit_marker(&SessionExit::Signal("Terminated".to_owned())),
            "\r\n[process exited with signal Terminated]\r\n"
        );
    }

    #[test]
    fn a_status_becomes_a_code_or_a_signal() {
        assert_eq!(
            exit_from(Ok(&ExitStatus::with_exit_code(2))),
            SessionExit::Code(2)
        );
        assert_eq!(
            exit_from(Ok(&ExitStatus::with_signal("Hangup"))),
            SessionExit::Signal("Hangup".to_owned())
        );
    }

    #[test]
    fn an_unreadable_status_still_ends_the_session() {
        let error = std::io::Error::other("no status");

        assert_eq!(exit_from(Err(&error)), SessionExit::Code(-1));
    }
}
