//! An OIDC-specific transport: no redirects or request-log middleware.

use openidconnect::{HttpRequest, HttpResponse};
use std::sync::Arc;
use std::time::Duration;
use ureq::tls::{RootCerts, TlsConfig};

#[derive(Debug, thiserror::Error)]
#[error("identity transport failed")]
pub(crate) struct TransportError;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);
const RESPONSE_LIMIT: u64 = 1024 * 1024;

pub(crate) fn send(request: HttpRequest) -> Result<HttpResponse, TransportError> {
    let tls = TlsConfig::builder()
        .root_certs(RootCerts::PlatformVerifier)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::aws_lc_rs::default_provider()))
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(REQUEST_TIMEOUT))
        .max_redirects(0)
        .http_status_as_error(false)
        .proxy(ureq::Proxy::try_from_env())
        .tls_config(tls)
        .build()
        .into();
    let (parts, body) = request.into_parts();
    let request = ureq::http::Request::from_parts(parts, body);
    let mut response = agent.run(request).map_err(|_| TransportError)?;
    let mut builder = openidconnect::http::Response::builder().status(response.status());
    for (key, value) in response.headers() {
        builder = builder.header(key, value);
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(RESPONSE_LIMIT)
        .read_to_vec()
        .map_err(|_| TransportError)?;
    builder.body(bytes).map_err(|_| TransportError)
}
