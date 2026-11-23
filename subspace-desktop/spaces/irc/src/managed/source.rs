//! The credential source of the account's networks: at each connect and
//! reconnect, the IRC runtime asks the Subspace API for a short-lived
//! login on one of its blocking threads. The password goes straight into
//! the session's memory; nothing here keeps, shows or logs it.

use std::sync::Arc;

use subspace_api_client::{Client, Error};
use subspace_client_irc::{CredentialSource, IssuedLogin, IssuedPassword};

use super::device::sasl_username;

/// The source for `account` through `client`, which stays bound to the
/// account it was made for: after a sign-out or a switch it is refused.
pub(crate) fn source(client: Arc<Client>, account: &str, device: &str) -> CredentialSource {
    let key = format!("{} {account}", client.api_origin());
    let device = device.to_owned();
    CredentialSource::new(key, move || issue(&client, &device))
}

fn issue(client: &Client, device: &str) -> Result<IssuedLogin, String> {
    let credential = client.irc_credential().map_err(reason)?;
    Ok(IssuedLogin {
        username: sasl_username(&credential.account, device),
        nickname: credential.account,
        password: IssuedPassword::new(credential.password.expose()),
    })
}

/// Why no login was issued, shown with the retry: never a server's words.
pub(crate) fn reason(error: Error) -> String {
    let service = subspace_api_client::service_name();
    match error {
        Error::Network => format!("{service} could not be reached for an IRC sign-in"),
        Error::SignInNeeded | Error::Forbidden => {
            "Sign in under Settings › Account to connect".into()
        }
        Error::NotFound => format!("{service} does not issue IRC sign-ins"),
        _ => format!("{service} could not issue an IRC sign-in"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use subspace_api_client::{Client, Error};
    use subspace_framework::client_test_support::Mock;

    use super::{reason, source};

    #[test]
    fn each_connect_asks_the_api_and_signs_in_from_this_device() {
        let mock = Mock::new(|_| {
            (
                200,
                r#"{"account":"ada","password":"sirc1.xxxxxxxx","expiresAt":"2026-11-23T21:10:00Z"}"#
                    .into(),
            )
        });
        let client = Client::loopback(&format!("{}/api", mock.base_url()), None).unwrap();
        let login = source(Arc::new(client), "ada", "0123456789abcdef");
        let source = login.clone();
        let issued =
            std::thread::spawn(move || subspace_client_irc::test_support::fetch(&source).unwrap())
                .join()
                .unwrap();
        assert_eq!(issued.nickname, "ada");
        assert_eq!(issued.username, "ada@0123456789abcdef");
        assert_eq!(issued.password.expose(), "sirc1.xxxxxxxx");
        assert_eq!(mock.requests()[0].path, "/api/v1/me/irc-credential");
        assert!(login.key().ends_with(" ada"));
    }

    #[test]
    fn a_refusal_reads_as_what_to_do_and_names_no_secret() {
        assert_eq!(
            reason(Error::Network),
            "Subspace API could not be reached for an IRC sign-in"
        );
        assert_eq!(
            reason(Error::SignInNeeded),
            "Sign in under Settings › Account to connect"
        );
        assert_eq!(
            reason(Error::NotFound),
            "Subspace API does not issue IRC sign-ins"
        );
        assert_eq!(
            reason(Error::UnexpectedStatus(500)),
            "Subspace API could not issue an IRC sign-in"
        );
    }
}
