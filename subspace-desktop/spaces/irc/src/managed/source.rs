//! The credential source of one account network: at each connect and
//! reconnect, the IRC runtime asks for that server's login on one of its
//! blocking threads. The login is the one issued for that server alone,
//! kept until shortly before it expires; nothing here shows or logs it.

use std::sync::Arc;
use std::time::Instant;

use subspace_api_client::{Client, Error};
use subspace_client_irc::{CredentialSource, IssuedLogin, IssuedPassword};

use super::device::sasl_username;
use super::env::Env;
use super::login::{ServerLogin, Unavailable};

/// The account a check verified, with the client it used and the account
/// generation it began in. A source refuses once that generation is over.
pub(crate) struct Binding {
    pub env: Arc<dyn Env>,
    pub client: Arc<Client>,
    pub generation: Option<u64>,
    /// The API's stable ID of the verified person.
    pub person_id: String,
}

impl Binding {
    /// Whether the account it was made for is still the one signed in.
    pub fn current(&self) -> bool {
        self.env.generation() == self.generation
    }

    /// Whether memberships read for `owner`, an account generation and
    /// verified user, belong to this account.
    pub fn owns(&self, owner: Option<(Option<u64>, &str)>) -> bool {
        owner == Some((self.generation, self.person_id.as_str()))
    }
}

/// The source of the network on the server `login` belongs to.
pub(crate) fn source(
    binding: &Arc<Binding>,
    login: &Arc<ServerLogin>,
    device: &str,
) -> CredentialSource {
    let key = format!(
        "{} {} {}",
        binding.client.api_origin(),
        binding.person_id,
        login.server()
    );
    let (binding, issuing, device) = (Arc::clone(binding), Arc::clone(login), device.to_owned());
    let refused = Arc::clone(login);
    CredentialSource::new(key, move || issue(&binding, &issuing, &device))
        .on_refused(move || refused.forget())
}

fn issue(binding: &Arc<Binding>, login: &ServerLogin, device: &str) -> Result<IssuedLogin, String> {
    if !binding.current() {
        return Err(reason(Unavailable::Api(Error::SignInNeeded)));
    }
    let login = login.login(&binding.client).map_err(reason)?;
    // Asked again the moment it would be sent.
    let (still, valid_until) = (Arc::clone(binding), login.valid_until);
    let password = IssuedPassword::new(login.password.expose())
        .while_valid(move || still.current() && Instant::now() < valid_until);
    Ok(IssuedLogin {
        username: sasl_username(&login.account, device),
        nickname: login.account,
        password,
    })
}

/// Why no login was issued, shown with the retry: never a server's words.
pub(crate) fn reason(unavailable: Unavailable) -> String {
    let service = subspace_api_client::service_name();
    match unavailable {
        Unavailable::Api(Error::Network) => {
            format!("{service} could not be reached for an IRC sign-in")
        }
        Unavailable::Api(Error::SignInNeeded | Error::Forbidden) => {
            "Sign in under Settings › Account to connect".into()
        }
        Unavailable::Api(Error::NotFound) => {
            format!("{service} does not issue sign-ins for this IRC server")
        }
        Unavailable::Api(_) => format!("{service} could not issue an IRC sign-in"),
        Unavailable::Expired => format!("{service} issued an IRC sign-in that had expired"),
        Unavailable::Waiting => format!("Waiting before asking {service} for an IRC sign-in"),
    }
}

#[cfg(test)]
mod tests {
    use subspace_api_client::Error;

    use super::{Unavailable, reason};

    #[test]
    fn a_refusal_reads_as_what_to_do_and_names_no_secret() {
        assert_eq!(
            reason(Unavailable::Api(Error::Network)),
            "Subspace API could not be reached for an IRC sign-in"
        );
        assert_eq!(
            reason(Unavailable::Api(Error::SignInNeeded)),
            "Sign in under Settings › Account to connect"
        );
        assert_eq!(
            reason(Unavailable::Api(Error::NotFound)),
            "Subspace API does not issue sign-ins for this IRC server"
        );
        assert_eq!(
            reason(Unavailable::Api(Error::UnexpectedStatus(500))),
            "Subspace API could not issue an IRC sign-in"
        );
        assert_eq!(
            reason(Unavailable::Expired),
            "Subspace API issued an IRC sign-in that had expired"
        );
        assert_eq!(
            reason(Unavailable::Waiting),
            "Waiting before asking Subspace API for an IRC sign-in"
        );
    }
}
