//! The GitHub identity Copilot runs as, and the sign-in that would fix a
//! missing or rejected one.

use std::sync::Arc;

use subspace_client_github::credential_login;
use subspace_core::credentials::{
    AccountSource, Credential, CredentialError, Credentials, Scheme, Source, saved_account,
};
use subspace_core::http::Error as HttpError;

use crate::SignInOffer;
use crate::runtime::{Event, EventQueue, Secret, config};

/// The GitHub identity Copilot runs as: a token and the login GitHub reports
/// for it.
pub(in crate::runtime) struct Identity {
    pub token: Secret,
    pub login: String,
}

/// Why the chat account could not be confirmed, and the sign-in that would
/// fix it.
pub(in crate::runtime) struct IdentityError {
    message: String,
    sign_in: Option<SignInOffer>,
}

impl IdentityError {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            sign_in: None,
        }
    }
}

/// Why GitHub did not confirm a token.
enum Unconfirmed {
    /// GitHub answered 401: the token is expired or revoked.
    Rejected,
    Other(String),
}

/// A private `copilot.json` token wins; otherwise the token of the account
/// the GitHub dashboard uses (or of the `copilot.json` login): its Subspace
/// sign-in, else its gh CLI token. The token's owner is checked with GitHub
/// before Copilot starts. When a sign-in would fix a failure, the panel is
/// told which.
pub(in crate::runtime) async fn resolve_identity(
    settings: &config::CopilotConfig,
    credentials: Arc<Credentials>,
    events: &EventQueue,
) -> Result<Identity, String> {
    let token = settings.token.clone();
    let login = settings.login.clone();
    let result = tokio::task::spawn_blocking(move || match token {
        Some(token) => config_identity(token, login.as_deref()),
        None => account_identity(&credentials, login),
    })
    .await
    .map_err(|_| "Cannot read the GitHub account.".to_string())?;
    let IdentityError { message, sign_in } = match result {
        Ok(identity) => return Ok(identity),
        Err(error) => error,
    };
    if let Some(offer) = sign_in {
        events.push(Event::SignInRequired(offer)).await;
    }
    Err(message)
}

fn config_identity(token: Secret, expected: Option<&str>) -> Result<Identity, IdentityError> {
    let credential = Credential {
        value: token,
        scheme: Scheme::Bearer,
        source: Source::ConfigFile("copilot.json token"),
    };
    verified(credential, expected).map_err(|unconfirmed| match unconfirmed {
        Unconfirmed::Rejected => IdentityError::plain(
            "GitHub rejected the token in copilot.json. Replace it, then click Reconnect.",
        ),
        Unconfirmed::Other(message) => IdentityError::plain(message),
    })
}

fn account_identity(
    credentials: &Credentials,
    explicit: Option<String>,
) -> Result<Identity, IdentityError> {
    let accounts = credentials.github_accounts().ok();
    let in_app = credentials.sign_in_offered();
    let login = match explicit {
        Some(login) => login,
        None => accounts
            .as_ref()
            .and_then(|accounts| accounts.pick(saved_account().as_deref()))
            .ok_or_else(|| no_account(in_app))?,
    };
    let credential = credentials
        .account(&login)
        .map_err(|error| unavailable(&login, accounts.as_ref(), error, in_app))?;
    match verified(credential.clone(), Some(&login)) {
        Ok(identity) => Ok(identity),
        Err(Unconfirmed::Other(message)) => Err(IdentityError::plain(message)),
        Err(Unconfirmed::Rejected) => {
            // Reconnect asks again instead of reusing the rejected token.
            credentials.invalidate(&credential);
            let active = accounts.and_then(|accounts| accounts.active);
            Err(rejected(
                &login,
                &credential.source,
                active.as_deref(),
                in_app,
            ))
        }
    }
}

/// `in_app` is whether the app offers Sign in with GitHub; without it, no
/// message names it and no in-app sign-in is offered.
fn no_account(in_app: bool) -> IdentityError {
    let message = if in_app {
        "No GitHub account found. Click Sign in to GitHub, or run `gh auth login` and choose the \
         account in Settings > GitHub."
    } else {
        "No GitHub account found. Run `gh auth login` and choose the account in Settings > GitHub."
    };
    IdentityError {
        message: message.into(),
        sign_in: in_app.then_some(SignInOffer::InApp { login: None }),
    }
}

/// Why `login` has no token; a sign-in GitHub has since rejected can be
/// renewed in the app when it offers Sign in with GitHub.
fn unavailable(
    login: &str,
    accounts: Option<&subspace_core::credentials::GitHubAccounts>,
    error: CredentialError,
    in_app: bool,
) -> IdentityError {
    let expired =
        accounts.is_some_and(|accounts| accounts.source(login) == AccountSource::SignInExpired);
    IdentityError {
        message: error.0,
        sign_in: (expired && in_app).then(|| SignInOffer::InApp {
            login: Some(login.to_owned()),
        }),
    }
}

/// What to do when GitHub rejects the token of `login` from `source`.
fn rejected(login: &str, source: &Source, active: Option<&str>, in_app: bool) -> IdentityError {
    match source {
        Source::GhCli { .. } => rejected_gh_token(login, active, in_app),
        _ if in_app => IdentityError {
            message: format!(
                "GitHub no longer accepts the sign-in for {login}. Click Sign in to GitHub to \
                 sign in again."
            ),
            sign_in: Some(SignInOffer::InApp {
                login: Some(login.to_owned()),
            }),
        },
        _ => IdentityError::plain(format!(
            "GitHub no longer accepts the sign-in for {login}. Run `gh auth login`, then click \
             Reconnect."
        )),
    }
}

/// What to do when GitHub rejects the gh token of `login`. gh refreshes only
/// its active account, so gh's sign-in is offered only for that one; any
/// other account can sign in to the app instead, when it offers that.
fn rejected_gh_token(login: &str, active: Option<&str>, in_app: bool) -> IdentityError {
    if active.is_some_and(|active| active.eq_ignore_ascii_case(login)) {
        return IdentityError {
            message: format!(
                "GitHub rejected the gh CLI token for {login}. Click Sign in to GitHub, or run \
                 `gh auth refresh -h github.com` in a terminal and click Reconnect."
            ),
            sign_in: Some(SignInOffer::GhRefresh {
                login: login.to_owned(),
            }),
        };
    }
    let switch = format!(
        "in a terminal run `gh auth switch -h github.com -u {login}`, then `gh auth refresh -h \
         github.com`, then click Reconnect."
    );
    if !in_app {
        return IdentityError::plain(format!(
            "GitHub rejected the gh CLI token for {login}. To use it, {switch}"
        ));
    }
    IdentityError {
        message: format!(
            "GitHub rejected the gh CLI token for {login}. Click Sign in to GitHub to sign in to \
             the app as {login}, or {switch}"
        ),
        sign_in: Some(SignInOffer::InApp {
            login: Some(login.to_owned()),
        }),
    }
}

fn verified(credential: Credential, expected: Option<&str>) -> Result<Identity, Unconfirmed> {
    let owner = credential_login(&credential).map_err(unconfirmed)?;
    if expected.is_some_and(|expected| !expected.eq_ignore_ascii_case(&owner)) {
        return Err(Unconfirmed::Other(
            "The chat token belongs to a different GitHub user. Chat is blocked.".into(),
        ));
    }
    Ok(Identity {
        token: credential.value,
        login: owner,
    })
}

fn unconfirmed(error: HttpError) -> Unconfirmed {
    match error {
        HttpError::Status { status: 401, .. } => Unconfirmed::Rejected,
        HttpError::Network(_) => Unconfirmed::Other(
            "Cannot reach GitHub to confirm the chat account's token. Check the network, then \
             click Reconnect."
                .into(),
        ),
        error => Unconfirmed::Other(format!(
            "GitHub did not confirm the chat account's token: {error}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{HttpError, Unconfirmed, no_account, rejected, rejected_gh_token, unconfirmed};
    use crate::SignInOffer;
    use crate::runtime::config::CopilotConfig;
    use subspace_core::credentials::Source;

    #[test]
    fn a_rejected_token_of_the_active_gh_account_offers_sign_in() {
        let error = rejected_gh_token("Octo_corp", Some("octo_corp"), true);
        assert_eq!(
            error.sign_in,
            Some(SignInOffer::GhRefresh {
                login: "Octo_corp".into()
            })
        );
        assert!(
            error.message.contains("Sign in to GitHub"),
            "{}",
            error.message
        );
        assert!(error.message.contains("gh auth refresh -h github.com"));
    }

    #[test]
    fn a_rejected_token_of_another_gh_account_offers_a_subspace_sign_in() {
        let error = rejected_gh_token("work", Some("personal"), true);
        let in_app = Some(SignInOffer::InApp {
            login: Some("work".into()),
        });
        assert_eq!(
            error.sign_in, in_app,
            "gh refreshes only its active account"
        );
        assert!(
            error
                .message
                .contains("gh auth switch -h github.com -u work")
        );
        assert_eq!(rejected_gh_token("work", None, true).sign_in, in_app);
    }

    #[test]
    fn a_rejected_signed_in_token_offers_to_sign_in_again() {
        let source = Source::SignedIn {
            login: "octo-dev".into(),
        };
        let error = rejected("octo-dev", &source, None, true);
        assert_eq!(
            error.sign_in,
            Some(SignInOffer::InApp {
                login: Some("octo-dev".into())
            })
        );
        assert!(
            error.message.contains("no longer accepts"),
            "{}",
            error.message
        );
        assert_eq!(
            no_account(true).sign_in,
            Some(SignInOffer::InApp { login: None })
        );
    }

    #[test]
    fn without_sign_in_with_github_nothing_offers_or_names_it() {
        let signed_in = Source::SignedIn {
            login: "octo-dev".into(),
        };
        for error in [
            no_account(false),
            rejected("octo-dev", &signed_in, None, false),
            rejected_gh_token("work", Some("personal"), false),
        ] {
            assert_eq!(error.sign_in, None);
            assert!(
                !error.message.contains("Sign in to GitHub"),
                "{}",
                error.message
            );
            assert!(error.message.contains("gh auth"), "{}", error.message);
        }
        let active = rejected_gh_token("octo_corp", Some("octo_corp"), false);
        assert_eq!(
            active.sign_in,
            Some(SignInOffer::GhRefresh {
                login: "octo_corp".into()
            })
        );
    }

    #[test]
    fn only_http_401_counts_as_a_rejected_token() {
        let rejected = unconfirmed(HttpError::Status {
            status: 401,
            message: "GitHub token rejected for octo-dev; run gh auth login.".into(),
        });
        assert!(matches!(rejected, Unconfirmed::Rejected));
        let Unconfirmed::Other(offline) = unconfirmed(HttpError::Network("dns".into())) else {
            panic!("a network failure is not a rejection");
        };
        assert!(offline.contains("Cannot reach GitHub"));
        let Unconfirmed::Other(limited) = unconfirmed(HttpError::Status {
            status: 429,
            message: "GitHub rate limit reached for octo-dev.".into(),
        }) else {
            panic!("a rate limit is not a rejection");
        };
        assert!(limited.ends_with("GitHub rate limit reached for octo-dev."));
    }

    #[tokio::test]
    async fn identity_resolution_rejects_the_selected_missing_account_without_another_identity() {
        let config = subspace_core::config::Config::default();
        let credentials = subspace_agent_tools::test_support::credentials(&config);
        let settings = CopilotConfig {
            login: Some("test-only-missing-account".into()),
            ..Default::default()
        };
        let events = crate::runtime::EventQueue::default();
        let result = super::resolve_identity(&settings, credentials, &events).await;
        let error = result
            .err()
            .expect("missing selected identity must reject startup");
        assert!(error.contains("test-only-missing-account"), "{error}");
        assert!(error.contains("gh auth login"), "{error}");
        assert!(
            crate::runtime::lock(&events.items).is_empty(),
            "an unavailable account must not offer refresh of an unrelated account"
        );
    }
}
