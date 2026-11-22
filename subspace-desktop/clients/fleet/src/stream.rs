//! A cancellable connection to `GET /api/v1/events`. No redirects, no
//! proxy, and the token only in a sensitive `Authorization` header.

use std::sync::Arc;
use std::time::Duration;

use reqwest::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue, RETRY_AFTER};
use reqwest::redirect::Policy;
use rustls_platform_verifier::BuilderVerifierExt as _;

use crate::client::{CONNECT_TIMEOUT, USER_AGENT, retry_after, status_error};
use crate::events::{FleetEvent, Frames};
use crate::{Error, FleetClient};

const MEDIA_TYPE: &str = "text/event-stream";
/// Stargate sends a heartbeat every fifteen seconds and closes a stream
/// idle for sixty.
const IDLE_TIMEOUT: Duration = Duration::from_secs(40);

/// Why a stream ended or could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamEnd {
    /// Stargate answered, but not with the stream.
    Refused(Error),
    /// The connection failed, timed out or dropped.
    Network,
    /// The stream broke its framing.
    Protocol,
    /// Stargate closed the stream, as it does when its roster changes.
    Closed,
}

pub(crate) fn http_client() -> Result<reqwest::Client, StreamEnd> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|builder| builder.with_platform_verifier())
        .map_err(|_| StreamEnd::Network)?
        .with_no_client_auth();
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(IDLE_TIMEOUT)
        .tls_backend_preconfigured(tls)
        .build()
        .map_err(|_| StreamEnd::Network)
}

pub(crate) async fn connect(
    http: &reqwest::Client,
    client: &FleetClient,
) -> Result<EventStream, StreamEnd> {
    let url = client
        .address()
        .url(&["events"])
        .map_err(StreamEnd::Refused)?;
    let mut bearer = HeaderValue::from_str(&format!("Bearer {}", client.token().expose()))
        .map_err(|_| StreamEnd::Refused(Error::TokenRefused))?;
    bearer.set_sensitive(true);
    let response = http
        .get(url)
        .header(ACCEPT, MEDIA_TYPE)
        .header(AUTHORIZATION, bearer)
        .send()
        .await
        .map_err(|_| StreamEnd::Network)?;
    check(&response)?;
    Ok(EventStream {
        response,
        frames: Frames::default(),
    })
}

fn check(response: &reqwest::Response) -> Result<(), StreamEnd> {
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value: &HeaderValue| value.to_str().ok())
    };
    if let Some(error) = status_error(response.status().as_u16(), retry_after(header(RETRY_AFTER)))
    {
        return Err(StreamEnd::Refused(error));
    }
    // A proxy's sign-in or error page can answer 200: that is not the stream.
    let mime = header(CONTENT_TYPE).and_then(|value| value.split(';').next());
    if mime.is_some_and(|value| value.trim().eq_ignore_ascii_case(MEDIA_TYPE)) {
        Ok(())
    } else {
        Err(StreamEnd::Refused(Error::InvalidResponse))
    }
}

pub(crate) struct EventStream {
    response: reqwest::Response,
    frames: Frames,
}

impl EventStream {
    pub(crate) async fn next(&mut self) -> Result<FleetEvent, StreamEnd> {
        loop {
            if let Some(event) = self.frames.ready.pop_front() {
                return Ok(event);
            }
            let chunk = self
                .response
                .chunk()
                .await
                .map_err(|_| StreamEnd::Network)?
                .ok_or(StreamEnd::Closed)?;
            self.frames.push(&chunk).map_err(|_| StreamEnd::Protocol)?;
        }
    }
}
