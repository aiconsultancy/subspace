//! A cancellable HTTP stream for project-scoped issue hints.
mod protocol;

pub use protocol::IssueChanged;
pub(crate) use protocol::Record;

use crate::{Client, Error, NativeLease, fixture, native, pacing, transport};
use protocol::Records;
use reqwest::{
    StatusCode,
    header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderValue},
    redirect::Policy,
};
use rustls_platform_verifier::BuilderVerifierExt as _;
use std::{collections::BTreeSet, sync::Arc, time::Duration};

const MEDIA_TYPE: &str = "text/event-stream";
pub(crate) const MAX_PROJECTS: usize = 64;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const IDLE_TIMEOUT: Duration = Duration::from_secs(40);

/// Failures carry no URLs, credentials or response text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum LiveError {
    #[error(
        "{} has no issue update stream or a project is unavailable.",
        crate::backend::service_name()
    )]
    Unavailable,
    #[error("{} refused the issue update stream.", crate::backend::service_name())]
    Refused,
    #[error("The issue update stream could not be reached.")]
    Network,
    #[error("The issue update stream sent an invalid message.")]
    Protocol,
    #[error("The issue update stream closed.")]
    Closed,
    /// NETT: 429 or 503, with the server's wait in whole seconds when it
    /// gave one.
    #[error(
        "{} is busy; the issue update stream will try again.",
        crate::backend::service_name()
    )]
    Busy(Option<u64>),
    /// NETT: 401, the bearer was refused.
    #[error("The issue update stream needs a fresh sign-in.")]
    Unauthorized,
    /// NETT: 403, the person no longer has access to what was followed.
    #[error("The issue update stream was refused access.")]
    Forbidden,
}

pub(crate) fn http_client() -> Result<reqwest::Client, LiveError> {
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let tls = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .and_then(|builder| builder.with_platform_verifier())
        .map_err(|_| LiveError::Network)?
        .with_no_client_auth();
    reqwest::Client::builder()
        .user_agent(transport::USER_AGENT)
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(CONNECT_TIMEOUT)
        .read_timeout(IDLE_TIMEOUT)
        .tls_backend_preconfigured(tls)
        .build()
        .map_err(|_| LiveError::Network)
}

pub(crate) async fn connect(
    http: &reqwest::Client,
    client: &Client,
    projects: &BTreeSet<String>,
) -> Result<EventStream, LiveError> {
    if projects.is_empty() || projects.len() > MAX_PROJECTS {
        return Err(LiveError::Refused);
    }
    let query: Vec<_> = projects
        .iter()
        .map(|key| ("project".to_owned(), key.clone()))
        .collect();
    let url =
        transport::endpoint(&client.base, &["updates"], &query).map_err(|_| LiveError::Refused)?;
    pacing::wait_async(&url).await;
    let mut request = http.get(url.clone()).header(ACCEPT, MEDIA_TYPE);
    if client.backend == crate::Backend::Nett {
        // NETT serves the same stream; project keys are its team keys, which
        // are unique only inside the organisation the header names.
        let org = client.org.as_deref().ok_or(LiveError::Refused)?;
        request = request.header("x-org-id", org);
    }
    if let Some(NativeLease(generation)) = client.native_lease {
        request = request.header(AUTHORIZATION, private_bearer(url, generation).await?);
    } else if let Some(jwt) = &client.access_jwt {
        fixture::loopback_url(client.base.as_str()).map_err(|_| LiveError::Refused)?;
        let mut value = HeaderValue::from_str(&format!("Bearer {}", jwt.expose()))
            .map_err(|_| LiveError::Refused)?;
        value.set_sensitive(true);
        request = request.header(AUTHORIZATION, value);
    }
    let response = request.send().await.map_err(|_| LiveError::Network)?;
    if client.account_changed() {
        return Err(LiveError::Refused);
    }
    check_response(&response, client.backend == crate::Backend::Nett)?;
    Ok(EventStream {
        response,
        records: Records::default(),
    })
}

async fn private_bearer(url: url::Url, lease: Option<u64>) -> Result<HeaderValue, LiveError> {
    let uri = url.as_str().parse().map_err(|_| LiveError::Refused)?;
    tokio::task::spawn_blocking(move || native::bearer(&uri, lease))
        .await
        .map_err(|_| LiveError::Network)?
        .map_err(|err| match err {
            Error::Network => LiveError::Network,
            _ => LiveError::Refused,
        })?
        .ok_or(LiveError::Refused)
}

/// Subspace API's refusals keep their meaning; NETT's also say why, so the
/// hub can wait as asked, renew the bearer or check access again.
fn check_response(response: &reqwest::Response, nett: bool) -> Result<(), LiveError> {
    match response.status() {
        StatusCode::OK => {}
        StatusCode::NOT_FOUND | StatusCode::METHOD_NOT_ALLOWED => {
            return Err(LiveError::Unavailable);
        }
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE if nett => {
            let wait = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse::<u64>().ok());
            return Err(LiveError::Busy(wait));
        }
        StatusCode::TOO_MANY_REQUESTS | StatusCode::SERVICE_UNAVAILABLE => {
            return Err(LiveError::Network);
        }
        StatusCode::UNAUTHORIZED if nett => return Err(LiveError::Unauthorized),
        StatusCode::FORBIDDEN if nett => return Err(LiveError::Forbidden),
        _ => return Err(LiveError::Refused),
    }
    let mime = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next());
    if mime.is_some_and(|value| value.trim().eq_ignore_ascii_case(MEDIA_TYPE)) {
        Ok(())
    } else {
        Err(LiveError::Refused)
    }
}

pub(crate) struct EventStream {
    response: reqwest::Response,
    records: Records,
}

impl EventStream {
    pub(crate) async fn next(&mut self) -> Result<Record, LiveError> {
        while self.records.ready.is_empty() {
            let chunk = self
                .response
                .chunk()
                .await
                .map_err(|_| LiveError::Network)?
                .ok_or(LiveError::Closed)?;
            self.records.push(&chunk)?;
        }
        self.records.ready.pop_front().ok_or(LiveError::Protocol)
    }
}

#[cfg(test)]
mod wording_tests {
    use super::LiveError;

    #[test]
    fn the_default_build_names_subspace_api_as_before() {
        assert_eq!(
            LiveError::Unavailable.to_string(),
            "Subspace API has no issue update stream or a project is unavailable."
        );
        assert_eq!(
            LiveError::Refused.to_string(),
            "Subspace API refused the issue update stream."
        );
    }
}
