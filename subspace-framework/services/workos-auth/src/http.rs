//! The token transport: one JSON POST, no redirects or request-log middleware.

use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use subspace_account::Error;
use subspace_core::Secret;
use ureq::tls::{RootCerts, TlsConfig};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const RESPONSE_LIMIT: u64 = 1024 * 1024;
const BAD_REQUEST: u16 = 400;
const UNAUTHORIZED: u16 = 401;
const FORBIDDEN: u16 = 403;

/// The parts of WorkOS's authentication reply the session keeps.
#[derive(Deserialize)]
pub(crate) struct Reply {
    pub user: User,
    #[serde(default)]
    pub organization_id: Option<String>,
    pub access_token: Secret,
    pub refresh_token: Secret,
}

#[derive(Deserialize)]
pub(crate) struct User {
    pub id: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub first_name: Option<String>,
    #[serde(default)]
    pub last_name: Option<String>,
}

#[derive(Deserialize)]
struct Failure {
    #[serde(default)]
    error: String,
}

/// Swappable for offline tests.
pub(crate) trait Transport: Send + Sync {
    fn authenticate(&self, url: &str, body: serde_json::Value) -> Result<Reply, Error>;
}

pub(crate) struct Live;

impl Transport for Live {
    fn authenticate(&self, url: &str, body: serde_json::Value) -> Result<Reply, Error> {
        let mut response = agent()
            .post(url)
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .send(body.to_string())
            .map_err(|_| Error::Offline)?;
        let status = response.status().as_u16();
        let bytes = response
            .body_mut()
            .with_config()
            .limit(RESPONSE_LIMIT)
            .read_to_vec()
            .map_err(|_| Error::Offline)?;
        if response.status().is_success() {
            return serde_json::from_slice(&bytes).map_err(|_| Error::InvalidReply);
        }
        let failure = serde_json::from_slice::<Failure>(&bytes).ok();
        Err(failure_error(
            status,
            failure.as_ref().map(|f| f.error.as_str()),
        ))
    }
}

fn agent() -> ureq::Agent {
    let tls = TlsConfig::builder()
        .root_certs(RootCerts::PlatformVerifier)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .build();
    ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .proxy(ureq::Proxy::try_from_env())
        .tls_config(tls)
        .build()
        .into()
}

pub(crate) fn failure_error(status: u16, code: Option<&str>) -> Error {
    match (status, code) {
        (_, Some("invalid_grant" | "mfa_enrollment" | "sso_required")) => Error::SignInRequired,
        (_, Some("access_denied")) => Error::NotPermitted,
        (_, Some("invalid_client" | "unauthorized_client")) => Error::NotPermitted,
        (UNAUTHORIZED | FORBIDDEN, _) => Error::NotPermitted,
        (BAD_REQUEST, _) => Error::SignInRequired,
        _ => Error::Service,
    }
}

#[cfg(test)]
mod tests {
    use super::failure_error;
    use subspace_account::Error;

    #[test]
    fn provider_failures_map_to_fixed_errors() {
        assert_eq!(
            failure_error(400, Some("invalid_grant")),
            Error::SignInRequired
        );
        assert_eq!(
            failure_error(400, Some("invalid_client")),
            Error::NotPermitted
        );
        assert_eq!(failure_error(403, None), Error::NotPermitted);
        assert_eq!(failure_error(500, None), Error::Service);
    }
}
