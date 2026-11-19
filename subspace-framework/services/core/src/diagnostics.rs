//! Explicit diagnostics profiles. Local copy owns sanitized debugging context;
//! issue reports default to registered identities and can opt into sanitized paths.

mod error;
mod home;
mod identity;
mod issue;
mod issue_paths;
mod local;
mod logging;
mod markdown;
mod redact;
mod runtime;

pub use error::ErrorDetails;
pub use identity::BuildIdentity;
pub use issue::IssueDiagnostics;
pub use issue_paths::IssuePaths;
pub use local::{LocalCopy, LocalDiagnostics};
pub use redact::RedactionContext;
pub use runtime::{LaunchKind, RuntimeDetails};

#[cfg(test)]
mod issue_paths_tests;
#[cfg(test)]
mod logging_tests;
#[cfg(test)]
mod privacy_tests;
#[cfg(test)]
mod tests;
