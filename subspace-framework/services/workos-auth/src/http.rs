//! The token transport: one JSON or form POST, no redirects or request-log
//! middleware.

use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use subspace_account::Error;
use subspace_core::Secret;
use subspace_core::http::status::{BAD_REQUEST, FORBIDDEN, UNAUTHORIZED, is_success, is_transient};
use ureq::tls::{RootCerts, TlsConfig};

/// The longest a token request may take.
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const RESPONSE_LIMIT: u64 = 1024 * 1024;

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

/// WorkOS's device authorisation reply, before validation.
#[derive(Deserialize)]
pub(crate) struct DeviceStart {
    pub device_code: Secret,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default)]
    pub interval: Option<u64>,
}

/// One device-code poll that did not fail.
pub(crate) enum DevicePoll {
    Granted(Box<Reply>),
    Pending,
    SlowDown,
}

/// Swappable for offline tests.
pub(crate) trait Transport: Send + Sync {
    fn authenticate(&self, url: &str, body: serde_json::Value) -> Result<Reply, Error>;
    /// Ask for a device code. The body is form-encoded, as WorkOS documents.
    fn authorize_device(&self, url: &str, client_id: &str) -> Result<DeviceStart, Error> {
        let _ = (url, client_id);
        Err(Error::Unconfigured)
    }
    /// Exchange a device code once, form-encoded, within `timeout`.
    fn poll_device(
        &self,
        url: &str,
        form: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<DevicePoll, Error> {
        let _ = (url, form, timeout);
        Err(Error::Unconfigured)
    }
    /// Renew a device sign-in with the form-encoded refresh grant, as
    /// device clients send it.
    fn refresh_form(&self, url: &str, form: &[(&str, &str)]) -> Result<Reply, Error> {
        let _ = (url, form);
        Err(Error::Unconfigured)
    }
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
        parse(status, &bytes)
    }

    fn authorize_device(&self, url: &str, client_id: &str) -> Result<DeviceStart, Error> {
        let (status, bytes) = send_form(url, &[("client_id", client_id)], REQUEST_TIMEOUT)?;
        parse(status, &bytes)
    }

    fn poll_device(
        &self,
        url: &str,
        form: &[(&str, &str)],
        timeout: Duration,
    ) -> Result<DevicePoll, Error> {
        let (status, bytes) = send_form(url, form, timeout.min(REQUEST_TIMEOUT))?;
        device_poll(status, &bytes)
    }

    fn refresh_form(&self, url: &str, form: &[(&str, &str)]) -> Result<Reply, Error> {
        let (status, bytes) = send_form(url, form, REQUEST_TIMEOUT)?;
        parse(status, &bytes)
    }
}

/// A successful reply's JSON, or the fixed error for a failure.
fn parse<T: serde::de::DeserializeOwned>(status: u16, bytes: &[u8]) -> Result<T, Error> {
    if is_success(status) {
        return serde_json::from_slice(bytes).map_err(|_| Error::InvalidReply);
    }
    let failure = serde_json::from_slice::<Failure>(bytes).ok();
    Err(failure_error(
        status,
        failure.as_ref().map(|f| f.error.as_str()),
    ))
}

fn send_form(url: &str, form: &[(&str, &str)], timeout: Duration) -> Result<(u16, Vec<u8>), Error> {
    let mut response = agent()
        .post(url)
        .header("Accept", "application/json")
        .config()
        .timeout_global(Some(timeout))
        .build()
        .send_form(form.iter().copied())
        .map_err(|_| Error::Offline)?;
    let status = response.status().as_u16();
    let bytes = response
        .body_mut()
        .with_config()
        .limit(RESPONSE_LIMIT)
        .read_to_vec()
        .map_err(|_| Error::Offline)?;
    Ok((status, bytes))
}

/// A poll reply: tokens, keep waiting, slow down, or a fixed error.
pub(crate) fn device_poll(status: u16, bytes: &[u8]) -> Result<DevicePoll, Error> {
    if is_success(status) {
        let reply: Reply = serde_json::from_slice(bytes).map_err(|_| Error::InvalidReply)?;
        return Ok(DevicePoll::Granted(Box::new(reply)));
    }
    let code = serde_json::from_slice::<Failure>(bytes)
        .map(|failure| failure.error)
        .unwrap_or_default();
    match (status, code.as_str()) {
        (BAD_REQUEST, "authorization_pending") => Ok(DevicePoll::Pending),
        (BAD_REQUEST, "slow_down") => Ok(DevicePoll::SlowDown),
        (BAD_REQUEST, "expired_token") => Err(Error::TimedOut),
        (_, code) => Err(failure_error(status, Some(code).filter(|c| !c.is_empty()))),
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
        // A service briefly unavailable says nothing about the grant.
        (status, _) if is_transient(status) => Error::Service,
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
    use super::{DevicePoll, device_poll, failure_error};
    use subspace_account::Error;

    #[test]
    fn device_polls_wait_slow_down_expire_or_fail() {
        let pending = device_poll(400, br#"{"error":"authorization_pending"}"#);
        assert!(matches!(pending, Ok(DevicePoll::Pending)));
        let slow = device_poll(400, br#"{"error":"slow_down"}"#);
        assert!(matches!(slow, Ok(DevicePoll::SlowDown)));
        assert_eq!(
            device_poll(400, br#"{"error":"expired_token"}"#).err(),
            Some(Error::TimedOut)
        );
        assert_eq!(
            device_poll(400, br#"{"error":"access_denied"}"#).err(),
            Some(Error::NotPermitted)
        );
        assert_eq!(device_poll(503, b"").err(), Some(Error::Service));
        assert_eq!(device_poll(200, b"{}").err(), Some(Error::InvalidReply));
    }

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

    /// An outage, whatever its body says, never ends a saved session; a
    /// grant the service refuses still does.
    #[test]
    fn a_briefly_unavailable_service_is_not_a_refused_grant() {
        for status in [502, 503, 504, 429] {
            assert_eq!(
                failure_error(status, Some("invalid_grant")),
                Error::Service,
                "{status}"
            );
            assert_eq!(
                super::parse::<super::Reply>(status, b"<html>Bad gateway</html>").err(),
                Some(Error::Service),
                "{status}"
            );
        }
        assert_eq!(
            super::parse::<super::Reply>(400, br#"{"error":"invalid_grant"}"#).err(),
            Some(Error::SignInRequired)
        );
    }
}
