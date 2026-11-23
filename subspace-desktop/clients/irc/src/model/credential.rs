//! A SASL PLAIN login fetched at each connect, beside a password in the
//! settings and a password file: for a short-lived login that another
//! service issues for one server. Its password never enters the network's
//! configuration: the session holds it for one SASL exchange, then drops it.

use std::sync::Arc;

/// An issued password: never formatted, serialised or stored with the
/// configuration.
#[derive(Clone, PartialEq, Eq)]
pub struct IssuedPassword(String);

impl IssuedPassword {
    pub fn new(password: impl Into<String>) -> Self {
        Self(password.into())
    }

    /// The password, for the one SASL exchange that sends it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for IssuedPassword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("***")
    }
}

/// What a [`CredentialSource`] issues for one connect.
pub struct IssuedLogin {
    /// The nickname to register with, such as the server's account name.
    pub nickname: String,
    /// The SASL PLAIN username, sent as both the authorisation and the
    /// authentication identity, such as `account` or `account@device`.
    pub username: String,
    pub password: IssuedPassword,
}

impl std::fmt::Debug for IssuedLogin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedLogin")
            .field("nickname", &self.nickname)
            .field("username", &self.username)
            .field("password", &self.password)
            .finish()
    }
}

type Fetch = dyn Fn() -> Result<IssuedLogin, String> + Send + Sync;
type Refused = dyn Fn() + Send + Sync;

/// Where a network's SASL PLAIN login comes from at each connect and
/// reconnect, when the settings hold no password and no password file. The
/// fetch may block, so the runtime calls it on a blocking thread; its error
/// is shown as the reason for the retry and must hold no secret. A failed
/// fetch is retried with the connection's backoff, never treated as final.
///
/// A network signing in this way must complete SASL: a server that
/// registers the connection without accepting the login is disconnected,
/// joins nothing and is retried, and the source is told the login was
/// refused.
///
/// Two sources are equal when their keys are, so an owner can build the
/// same source again without the runtime restarting the connection. It is
/// never read from or written to a settings file.
#[derive(Clone)]
pub struct CredentialSource {
    key: String,
    fetch: Arc<Fetch>,
    refused: Option<Arc<Refused>>,
}

impl CredentialSource {
    pub fn new(
        key: impl Into<String>,
        fetch: impl Fn() -> Result<IssuedLogin, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            key: key.into(),
            fetch: Arc::new(fetch),
            refused: None,
        }
    }

    /// Calls `refused` when a server did not accept a login this source
    /// issued, so the source can stop offering it.
    // qual:api - owners pass it; this crate only calls it.
    pub fn on_refused(mut self, refused: impl Fn() + Send + Sync + 'static) -> Self {
        self.refused = Some(Arc::new(refused));
        self
    }

    /// What tells this source apart from others.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Fetches a login. Blocking.
    pub(crate) fn fetch(&self) -> Result<IssuedLogin, String> {
        (self.fetch)()
    }

    /// Reports that a server did not accept the login.
    pub(crate) fn refused(&self) {
        if let Some(refused) = &self.refused {
            refused();
        }
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
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{CredentialSource, IssuedLogin, IssuedPassword};

    fn login() -> Result<IssuedLogin, String> {
        Ok(IssuedLogin {
            nickname: "ada".into(),
            username: "ada@desk".into(),
            password: IssuedPassword::new("pw-fixture"),
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

    #[test]
    fn a_refusal_reaches_the_owner() {
        let told = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&told);
        let source = CredentialSource::new("api", login).on_refused(move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        source.refused();
        CredentialSource::new("plain", login).refused();
        assert_eq!(told.load(Ordering::SeqCst), 1);
    }
}
