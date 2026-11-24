//! The shared HTTP agent (REQ-31), the error type the clients return, and
//! the retry after HTTP 401 (REQ-22).

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig};

use crate::credentials::{Credential, CredentialError, Credentials};

mod service_url;

pub use service_url::{InvalidUrl, plain_https_url};

pub const TIMEOUT: Duration = Duration::from_secs(30);

/// The body limit for a response that may be large (64 MiB).
pub const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;

/// The body limit for an error message or another small body (64 KiB).
pub const ERROR_BODY_BYTES: u64 = 64 * 1024;

/// The HTTP statuses the clients map, named so the mapping code holds no
/// bare numbers.
pub mod status {
    use ureq::http::StatusCode;

    pub const OK: u16 = StatusCode::OK.as_u16();
    pub const BAD_REQUEST: u16 = StatusCode::BAD_REQUEST.as_u16();
    pub const UNAUTHORIZED: u16 = StatusCode::UNAUTHORIZED.as_u16();
    pub const FORBIDDEN: u16 = StatusCode::FORBIDDEN.as_u16();
    pub const NOT_FOUND: u16 = StatusCode::NOT_FOUND.as_u16();
    pub const PAYLOAD_TOO_LARGE: u16 = StatusCode::PAYLOAD_TOO_LARGE.as_u16();
    pub const UNSUPPORTED_MEDIA_TYPE: u16 = StatusCode::UNSUPPORTED_MEDIA_TYPE.as_u16();
    pub const TOO_MANY_REQUESTS: u16 = StatusCode::TOO_MANY_REQUESTS.as_u16();
    pub const BAD_GATEWAY: u16 = StatusCode::BAD_GATEWAY.as_u16();
    pub const GATEWAY_TIMEOUT: u16 = StatusCode::GATEWAY_TIMEOUT.as_u16();

    /// True for a 2xx status.
    pub fn is_success(status: u16) -> bool {
        StatusCode::from_u16(status).is_ok_and(|code| code.is_success())
    }

    /// True for a status that says the service is briefly unavailable, not
    /// that the request was wrong: a 5xx, 408 or 429. Its body, often a
    /// gateway's HTML page, says nothing about the request.
    pub fn is_transient(status: u16) -> bool {
        StatusCode::from_u16(status).is_ok_and(|code| {
            code.is_server_error()
                || code == StatusCode::REQUEST_TIMEOUT
                || code == StatusCode::TOO_MANY_REQUESTS
        })
    }
}

/// One agent for every client: the platform certificate store (so a proxy
/// certificate the system trusts is accepted), `HTTPS_PROXY` and `NO_PROXY`
/// from the environment, and a 30 s timeout. A failure status is returned
/// as a response, so a client can read the service's error message.
pub fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        let tls = TlsConfig::builder()
            .root_certs(RootCerts::PlatformVerifier)
            .unversioned_rustls_crypto_provider(Arc::new(
                rustls::crypto::aws_lc_rs::default_provider(),
            ))
            .build();
        ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .proxy(ureq::Proxy::try_from_env())
            .tls_config(tls)
            .middleware(request_log::RequestLog)
            .build()
            .into()
    })
}

mod request_log;

/// A failed request. No variant holds a credential.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// No credential; the message says where to get one (REQ-22).
    #[error(transparent)]
    Credential(#[from] CredentialError),
    /// The service answered with a failure status.
    #[error("{message}")]
    Status { status: u16, message: String },
    /// The service could not be reached.
    #[error("{0}")]
    Network(String),
}

impl From<ureq::Error> for Error {
    fn from(error: ureq::Error) -> Self {
        Self::Network(error.to_string())
    }
}

/// Sends a request with a credential from `credential`. After HTTP 401 it
/// drops the credential, gets one again and retries once (REQ-22). A second
/// 401 also drops the credential and is returned to the caller.
pub fn send_with_retry<T>(
    credentials: &Credentials,
    credential: impl Fn(&Credentials) -> Result<Credential, CredentialError>,
    mut send: impl FnMut(&Credential) -> Result<T, Error>,
) -> Result<T, Error> {
    let rejected =
        |result: &Result<T, Error>| matches!(result, Err(Error::Status { status: 401, .. }));
    let first = credential(credentials)?;
    let result = send(&first);
    if !rejected(&result) {
        return result;
    }
    credentials.invalidate(&first);
    let second = credential(credentials)?;
    let result = send(&second);
    if rejected(&result) {
        credentials.invalidate(&second);
    }
    result
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::Mutex;
    use std::time::Duration;

    use super::{Error, send_with_retry};
    use crate::config::Config;
    use crate::credentials::{Credential, CredentialError, Credentials, Environment, Runner};

    #[test]
    fn transient_statuses_are_server_errors_timeouts_and_rate_limits() {
        for status in [500, 502, 503, 504, 408, 429] {
            assert!(super::status::is_transient(status), "{status}");
        }
        for status in [200, 400, 401, 403, 404] {
            assert!(!super::status::is_transient(status), "{status}");
        }
    }

    #[test]
    fn is_success_covers_the_2xx_range_only() {
        assert!(super::status::is_success(
            ureq::http::StatusCode::NON_AUTHORITATIVE_INFORMATION.as_u16()
        ));
        assert!(super::status::is_success(
            ureq::http::StatusCode::OK.as_u16()
        ));
        assert!(!super::status::is_success(300));
        assert!(!super::status::is_success(super::status::BAD_REQUEST));
        assert!(!super::status::is_success(0));
    }

    #[test]
    fn the_shared_agent_builds_once() {
        assert!(std::ptr::eq(super::agent(), super::agent()));
    }

    struct NoEnv;

    impl Environment for NoEnv {
        fn var(&self, _name: &str) -> Option<String> {
            None
        }
    }

    /// `gh` answering `gh-1`, `gh-2`, ... on each call.
    #[derive(Default)]
    struct CountingGh(Mutex<u32>);

    impl Runner for CountingGh {
        fn run(
            &self,
            _program: &str,
            _args: &[&str],
            _remove: &[&str],
            _timeout: Duration,
        ) -> Result<String, String> {
            let mut count = self.0.lock().unwrap();
            *count += 1;
            Ok(format!("gh-{count}\n"))
        }
    }

    fn gh_cli(credentials: &Credentials) -> Result<Credential, CredentialError> {
        credentials.gh_cli("fixture-user")
    }

    fn rejecting(
        tokens: &[&str],
    ) -> impl FnMut(&crate::credentials::Credential) -> Result<String, Error> {
        let rejected: HashSet<String> = tokens.iter().map(ToString::to_string).collect();
        move |credential| {
            let token = credential.value.expose().to_owned();
            if rejected.contains(&token) {
                Err(Error::Status {
                    status: 401,
                    message: "rejected".into(),
                })
            } else {
                Ok(token)
            }
        }
    }

    #[test]
    fn a_401_gets_a_fresh_credential_and_retries_once() {
        let credentials = Credentials::new(&Config::default(), NoEnv, CountingGh::default());
        let result = send_with_retry(&credentials, gh_cli, rejecting(&["gh-1"]));
        assert_eq!(result.unwrap(), "gh-2");
    }

    #[test]
    fn a_second_401_is_returned() {
        let credentials = Credentials::new(&Config::default(), NoEnv, CountingGh::default());
        let result = send_with_retry(&credentials, gh_cli, rejecting(&["gh-1", "gh-2"]));
        assert!(matches!(result, Err(Error::Status { status: 401, .. })));
        // Two attempts, and the rejected second token was dropped too.
        let next = send_with_retry(&credentials, gh_cli, rejecting(&[]));
        assert_eq!(next.unwrap(), "gh-3");
    }
}
