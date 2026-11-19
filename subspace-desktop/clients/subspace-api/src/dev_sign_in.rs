//! Local development sign-in: the API's dev issuer, with no external identity
//! provider. It only talks to a loopback API, keeps tokens in memory and
//! signs in again silently at startup, so a local stack works out of the box.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use subspace_framework::account::{
    Account, Cancel, Error, Provider, ProviderInfo, ServiceToken, Started,
};
use subspace_framework::core::Secret;
use url::Url;

use crate::native;

const TOKEN_PATH: [&str; 3] = ["v1", "auth", "dev"];
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const RESPONSE_LIMIT: u64 = 64 * 1024;
const EXPIRY_SKEW: Duration = Duration::from_secs(60);
/// Assumed when the issuer names no lifetime.
const DEFAULT_LIFETIME_SECS: u64 = 3600;

/// Who the dev issuer signs in. Every field is optional on the API side.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct DevUser {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub email: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub organisation: String,
}

struct Session {
    account: Account,
    token: Secret,
    expires_at: Instant,
}

/// The `dev` provider, for a loopback API.
pub struct DevSignIn {
    base: Url,
    user: DevUser,
    session: Mutex<Option<Session>>,
    generation: AtomicU64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Issued {
    #[serde(alias = "access_token")]
    access_token: Secret,
    #[serde(default, alias = "expires_in")]
    expires_in: Option<u64>,
}

#[derive(Deserialize, Default)]
struct Claims {
    #[serde(default)]
    sub: String,
    #[serde(default)]
    org_id: String,
}

impl DevSignIn {
    /// `api` is the configured API URL; only a loopback API is accepted.
    pub fn new(api: &str, user: DevUser) -> Result<Self, Error> {
        let base = native::api_base(api).map_err(|_| Error::Unconfigured)?;
        token_url(&base)?;
        Ok(Self {
            base,
            user,
            session: Mutex::default(),
            generation: AtomicU64::default(),
        })
    }

    fn issue(&self) -> Result<Session, Error> {
        let url = token_url(&self.base)?;
        let body = serde_json::to_string(&self.user).map_err(|_| Error::Unconfigured)?;
        let mut response = agent()
            .post(url.as_str())
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|_| Error::Offline)?;
        match response.status().as_u16() {
            200 | 201 => {}
            404 => return Err(Error::Unconfigured),
            401 | 403 => return Err(Error::NotPermitted),
            _ => return Err(Error::Service),
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(RESPONSE_LIMIT)
            .read_to_vec()
            .map_err(|_| Error::InvalidReply)?;
        let issued: Issued = serde_json::from_slice(&bytes).map_err(|_| Error::InvalidReply)?;
        Ok(self.session(issued))
    }

    fn session(&self, issued: Issued) -> Session {
        let claims = claims(issued.access_token.expose());
        let lifetime = Duration::from_secs(issued.expires_in.unwrap_or(DEFAULT_LIFETIME_SECS));
        Session {
            account: Account {
                tenant: claims.org_id,
                id: if claims.sub.is_empty() {
                    self.user.email.clone()
                } else {
                    claims.sub
                },
                name: if self.user.name.is_empty() {
                    "Local developer".into()
                } else {
                    self.user.name.clone()
                },
                username: self.user.email.clone(),
            },
            token: issued.access_token,
            expires_at: Instant::now() + lifetime.saturating_sub(EXPIRY_SKEW),
        }
    }

    fn renew(&self, minimum: Duration, restore: bool) -> Result<(Account, Secret, Instant), Error> {
        let generation = self.generation();
        let mut session = self.session.lock().map_err(|_| Error::Service)?;
        if session.is_none() && !restore {
            return Err(Error::SignInRequired);
        }
        let fresh = session
            .as_ref()
            .is_some_and(|s| s.expires_at.saturating_duration_since(Instant::now()) > minimum);
        if !fresh {
            let issued = self.issue()?;
            if generation != self.generation() {
                return Err(Error::Cancelled);
            }
            // Signing in at startup changes the account: clients that read
            // before it, while signed out, start again with the new token.
            if session.replace(issued).is_none() {
                self.invalidate();
            }
        }
        let session = session.as_ref().ok_or(Error::SignInRequired)?;
        Ok((
            session.account.clone(),
            session.token.clone(),
            session.expires_at,
        ))
    }
}

impl Provider for DevSignIn {
    /// Tokens live in memory and the API is checked to be loopback.
    fn local_development(&self) -> bool {
        true
    }

    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: "local development".into(),
            organization: String::new(),
            detail: Some(("API".into(), self.base.as_str().into())),
            browser: false,
        }
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }

    fn invalidate(&self) {
        self.generation.fetch_add(1, Ordering::SeqCst);
    }

    fn start(&self) -> Result<Started, Error> {
        Ok(Started::headless())
    }

    fn finish(&self, _: Started, cancel: &Cancel, generation: u64) -> Result<Account, Error> {
        cancel.check()?;
        let issued = self.issue()?;
        cancel.check()?;
        let mut session = self.session.lock().map_err(|_| Error::Service)?;
        if generation != self.generation() {
            return Err(Error::Cancelled);
        }
        cancel.begin_commit()?;
        let account = issued.account.clone();
        *session = Some(issued);
        // Leases taken during the switch belong to the previous identity.
        self.invalidate();
        Ok(account)
    }

    /// Local stacks sign in on startup without a prompt.
    fn restore(&self) -> Result<Option<Account>, Error> {
        self.renew(Duration::ZERO, true)
            .map(|(account, ..)| Some(account))
    }

    fn reconnect(&self) -> Result<Option<Account>, Error> {
        match self.renew(Duration::ZERO, false) {
            Ok((account, ..)) => Ok(Some(account)),
            Err(Error::SignInRequired) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn sign_out(&self) -> Result<(), Error> {
        self.invalidate();
        *self.session.lock().map_err(|_| Error::Service)? = None;
        Ok(())
    }

    fn token_with_validity(&self, _: &str, minimum: Duration) -> Result<ServiceToken, Error> {
        let (account, token, expires_at) = self.renew(minimum, false)?;
        Ok(ServiceToken {
            account,
            token,
            expires_at,
        })
    }
}

/// `{base}/v1/auth/dev/token` on a loopback API only.
fn token_url(base: &Url) -> Result<Url, Error> {
    let loopback =
        crate::fixture::loopback_url(base.as_str()).is_ok() || base.host_str() == Some("localhost");
    if !loopback {
        return Err(Error::Unconfigured);
    }
    let mut url = base.clone();
    url.path_segments_mut()
        .map_err(|_| Error::Unconfigured)?
        .pop_if_empty()
        .extend(TOKEN_PATH)
        .push("token");
    Ok(url)
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .proxy(None)
        .max_redirects(0)
        .http_status_as_error(false)
        .timeout_global(Some(REQUEST_TIMEOUT))
        .build()
        .into()
}

/// The token's unverified claims, only to label the account.
fn claims(token: &str) -> Claims {
    token
        .split('.')
        .nth(1)
        .and_then(|payload| URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
