//! A SASL PLAIN login fetched at each connect, beside a password in the
//! settings and a password file: for a short-lived login that another
//! service issues. It lives in the session's memory only.

use std::sync::Arc;

/// What a [`CredentialSource`] issues for one connect.
pub struct IssuedLogin {
    /// The nickname to register with, such as the server's account name.
    pub nickname: String,
    /// The SASL PLAIN username, sent as both the authorisation and the
    /// authentication identity, such as `account` or `account@device`.
    pub username: String,
    pub password: String,
}

impl std::fmt::Debug for IssuedLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedLogin")
            .field("nickname", &self.nickname)
            .field("username", &self.username)
            .field("password", &"***")
            .finish()
    }
}

type Fetch = dyn Fn() -> Result<IssuedLogin, String> + Send + Sync;

/// Where a network's SASL PLAIN login comes from at each connect and
/// reconnect, when the settings hold no password and no password file. The
/// fetch may block, so the runtime calls it on a blocking thread; its error
/// is shown as the reason for the retry and must hold no secret. A failed
/// fetch is retried with the connection's backoff, never treated as final.
///
/// Two sources are equal when their keys are, so an owner can build the
/// same source again without the runtime restarting the connection. It is
/// never read from or written to a settings file.
#[derive(Clone)]
pub struct CredentialSource {
    key: String,
    fetch: Arc<Fetch>,
}

impl CredentialSource {
    pub fn new(
        key: impl Into<String>,
        fetch: impl Fn() -> Result<IssuedLogin, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            fetch: Arc::new(fetch),
        }
    }

    /// What tells this source apart from others.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Fetches a login. Blocking.
    pub(crate) fn fetch(&self) -> Result<IssuedLogin, String> {
        (self.fetch)()
    }
}

impl PartialEq for CredentialSource {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Eq for CredentialSource {}

impl std::fmt::Debug for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CredentialSource").field(&self.key).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{CredentialSource, IssuedLogin};

    fn login() -> Result<IssuedLogin, String> {
        Ok(IssuedLogin {
            nickname: "ada".into(),
            username: "ada@desk".into(),
            password: "pw-fixture".into(),
        })
    }

    #[test]
    fn sources_compare_by_key_and_never_show_a_password() {
        let one = CredentialSource::new("api", login);
        let again = CredentialSource::new("api", || Err("other".into()));
        assert_eq!(one, again, "rebuilding a source restarts nothing");
        assert_ne!(one, CredentialSource::new("other", login));
        assert_eq!(format!("{one:?}"), "CredentialSource(\"api\")");
        let shown = format!("{:?}", one.fetch().unwrap());
        assert!(
            shown.contains("ada@desk") && !shown.contains("pw-fixture"),
            "{shown}"
        );
    }
}
