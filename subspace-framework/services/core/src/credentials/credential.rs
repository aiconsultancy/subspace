//! One credential: its value, how it is sent and where it came from.

use crate::Secret;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Bearer,
}

/// Where a credential came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// The environment variable.
    Environment(&'static str),
    /// The config file key, such as `github.token`.
    ConfigFile(&'static str),
    /// An account signed in through Subspace.
    SignedIn {
        login: String,
    },
    GhCli {
        login: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credential {
    pub value: Secret,
    pub scheme: Scheme,
    pub source: Source,
}

impl Credential {
    /// A credential with its scheme and where it came from.
    pub fn new(value: Secret, scheme: Scheme, source: Source) -> Self {
        Self {
            value,
            scheme,
            source,
        }
    }

    /// The `Authorization` header value.
    pub fn authorization(&self) -> Secret {
        match self.scheme {
            Scheme::Bearer => Secret::new(format!("Bearer {}", self.value.expose())),
        }
    }
}
