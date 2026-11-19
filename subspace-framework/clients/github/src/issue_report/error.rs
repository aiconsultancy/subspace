use std::fmt;

/// Safe categories, with no server body, credential, account or user text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErrorKind {
    DescriptionRequired,
    InvalidTitle,
    InvalidDiagnostics,
    BodyTooLong,
    Authentication,
    AccessDenied,
    RepositoryUnavailable,
    RateLimited,
    Rejected,
    UncertainCreation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Error {
    kind: ErrorKind,
}

impl Error {
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }
}

// qual:allow(dry, boilerplate) reason: "One native conversion keeps safe error categories without adding an error-derive dependency to the client."
impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self.kind {
            ErrorKind::DescriptionRequired => "Enter a description before reporting the issue.",
            ErrorKind::InvalidTitle => "Use a single-line title of at most 256 characters.",
            ErrorKind::InvalidDiagnostics => {
                "Automatic diagnostics are invalid. Reopen the report form."
            }
            ErrorKind::BodyTooLong => {
                "The report exceeds GitHub's 65,536-character limit. Shorten it before sending."
            }
            ErrorKind::Authentication => {
                "GitHub rejected the credential. Sign in again, then review and retry."
            }
            ErrorKind::AccessDenied => {
                "GitHub denied access. Check repository access or use the browser editor."
            }
            ErrorKind::RepositoryUnavailable => {
                "The issue repository is unavailable. Check access or use the browser editor."
            }
            ErrorKind::RateLimited => {
                "GitHub's request limit was reached. Wait before trying again."
            }
            ErrorKind::Rejected => {
                "GitHub rejected the report. Review the title and description before retrying."
            }
            ErrorKind::UncertainCreation => {
                "GitHub may have created the issue. Check the repository's issues before retrying to avoid a duplicate."
            }
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for Error {}
