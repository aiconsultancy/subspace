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
    // A service that is briefly unavailable is no answer at all, whatever
    // its body: the session it would renew is still good.
    if subspace_core::http::status::is_transient(response.status().as_u16()) {
        return Err(TransportError);
    }
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

#[cfg(test)]
mod tests {
    use std::io::{Read as _, Write as _};

    /// Answers one request on loopback with `status` and an HTML body, as a
    /// gateway does during an outage.
    fn answer_once(status: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer);
            let body = "<html>Service Unavailable</html>";
            let reply = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(reply.as_bytes());
        });
        format!("http://{address}/oauth2/v2.0/token")
    }

    fn post(url: &str) -> Result<super::HttpResponse, super::TransportError> {
        let request = openidconnect::http::Request::builder()
            .method("POST")
            .uri(url)
            .body(b"grant_type=refresh_token".to_vec())
            .unwrap();
        super::send(request)
    }

    /// An outage is no answer, so restoring a saved session takes the
    /// offline path instead of reading the gateway's page as a reply.
    #[test]
    fn a_briefly_unavailable_token_endpoint_is_a_transport_failure() {
        for status in [
            "503 Service Unavailable",
            "502 Bad Gateway",
            "429 Too Many Requests",
        ] {
            assert!(post(&answer_once(status)).is_err(), "{status}");
        }
        let refused = post(&answer_once("400 Bad Request")).unwrap();
        assert_eq!(refused.status().as_u16(), 400, "a refusal is still read");
    }
}
