//! A short-lived IRC sign-in for the caller: `POST /v1/me/irc-credential`,
//! attempted once. The API answers the account name the IRC server knows
//! the caller by and an opaque password it accepts for a few minutes, which
//! the server checks with the API. The password is a [`Secret`]: it is
//! never formatted, stored or logged, and an API without the feature
//! answers 404.
use serde::Deserialize;
use subspace_framework::core::Secret;

use crate::{Backend, Client, Error, transport};

/// The longest account name accepted, in bytes.
const ACCOUNT_LIMIT: usize = 64;
/// The longest password accepted, in bytes. The API's are at most 200
/// characters; the limit leaves room without carrying anything large into
/// an IRC line.
const PASSWORD_LIMIT: usize = 400;

/// An IRC sign-in the API issued for the signed-in person.
pub struct IrcCredential {
    /// The account name on the IRC server, such as `ada`. The IRC server
    /// forces the nickname to it.
    pub account: String,
    /// The SASL PLAIN password, valid until [`Self::expires_at`].
    pub password: Secret,
    /// When the password stops being accepted.
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

impl std::fmt::Debug for IrcCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IrcCredential")
            .field("account", &self.account)
            .field("password", &self.password)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Wire {
    account: String,
    password: Secret,
    expires_at: String,
}

// qual:allow(coupling, oi) reason: "The HTTP boundary composes the endpoint, single-attempt transport and safe response checks."
impl Client {
    /// Asks the API for an IRC sign-in, once. Blocking: call it on a
    /// background task. [`Error::NotFound`] means this API does not issue
    /// them, and NETT does not yet.
    pub fn irc_credential(&self) -> Result<IrcCredential, Error> {
        if self.backend != Backend::Subspace {
            return Err(Error::NotFound);
        }
        let url = self.endpoint(&["me", "irc-credential"], &[])?;
        let response =
            transport::post_json(self, &url, &serde_json::json!({}), transport::READ_TIMEOUT)
                .map_err(|_| Error::Network)?;
        let wire: Wire = transport::read_json(transport::check_response(response)?)?;
        checked(wire).ok_or(Error::InvalidResponse)
    }
}

/// The credential, when every field is one an IRC line can carry.
fn checked(wire: Wire) -> Option<IrcCredential> {
    let carryable = valid_account(&wire.account) && valid_password(wire.password.expose());
    let expires_at = expiry(&wire.expires_at).filter(|_| carryable)?;
    Some(IrcCredential {
        account: wire.account,
        password: wire.password,
        expires_at,
    })
}

/// An ISO 8601 time with its offset, in UTC.
fn expiry(text: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&chrono::Utc))
}

/// An IRC account name: ASCII letters, digits and the punctuation nicks
/// allow, starting with a letter. Never a device suffix (`@`), a space or a
/// separator SASL PLAIN uses.
fn valid_account(account: &str) -> bool {
    let mut bytes = account.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && account.len() <= ACCOUNT_LIMIT
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || b"-_[]\\`^{|}".contains(&byte))
}

/// Printable ASCII without spaces, of bounded length.
fn valid_password(password: &str) -> bool {
    !password.is_empty()
        && password.len() <= PASSWORD_LIMIT
        && password.bytes().all(|byte| byte.is_ascii_graphic())
}

#[cfg(test)]
mod tests {
    use super::valid_account;

    #[test]
    fn an_account_is_one_irc_name_without_a_device_or_separator() {
        for good in ["ada", "ada2", "aurora-lyra", "Wren_", &"a".repeat(64)] {
            assert!(valid_account(good), "{good}");
        }
        for bad in [
            "",
            "2ada",
            "-ada",
            "ada@laptop",
            "ada fix",
            "ada\0x",
            "ada:x",
            "jäc",
            &"a".repeat(65),
        ] {
            assert!(!valid_account(bad), "{bad:?}");
        }
    }
}
