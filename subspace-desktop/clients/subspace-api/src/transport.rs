//! Bounded JSON transport. Never follows redirects or retains provider error text.
mod post;
pub(super) use post::post_json;
pub(crate) use post::post_json_bytes;
use serde::de::DeserializeOwned;
use std::time::Duration;
use subspace_framework::core::Secret;
use ureq::{
    Body,
    http::{Response, StatusCode},
};
use url::Url;

pub(crate) const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SEGMENT_BYTES: usize = 256;
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(45);
pub(crate) const USER_AGENT: &str = concat!("Subspace/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("Sign in under Settings › Account, then reload.")]
    SignInNeeded,
    #[error("You do not have permission to read this record.")]
    Forbidden,
    #[error("This record is unavailable.")]
    NotFound,
    #[error("The service could not be reached. Try again.")]
    Network,
    #[error("The service returned an unexpected response ({0}).")]
    UnexpectedStatus(u16),
    #[error("The service returned an invalid or oversized response.")]
    InvalidResponse,
    #[error("The requested selector, origin or page is invalid.")]
    InvalidRequest,
    #[error("Choose an organisation from the Organisation menu in Issues.")]
    OrganisationNeeded,
}

pub(crate) fn endpoint(
    base: &Url,
    path: &[&str],
    query: &[(String, String)],
) -> Result<Url, Error> {
    validate_segments(path.iter().copied())?;
    let mut url = base.clone();
    let mut parts = url.path_segments_mut().map_err(|_| Error::InvalidRequest)?;
    parts.pop_if_empty().push("v1").extend(path);
    drop(parts);
    if !query.is_empty() {
        url.query_pairs_mut().extend_pairs(query);
    }
    Ok(url)
}

pub(crate) fn validate_segments<'a>(segments: impl Iterator<Item = &'a str>) -> Result<(), Error> {
    let invalid = segments.into_iter().any(|s| {
        s.is_empty()
            || s.len() > MAX_SEGMENT_BYTES
            || !s
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    });
    if invalid {
        Err(Error::InvalidRequest)
    } else {
        Ok(())
    }
}

#[cfg(any(debug_assertions, feature = "test-support"))]
pub(crate) fn agent_for(base: &Url) -> ureq::Agent {
    agent_with_flow(base, None)
}

/// The loopback agent, and for a NETT client also NETT's flow control for
/// `account`.
#[cfg(any(debug_assertions, feature = "test-support"))]
pub(crate) fn agent_with_flow(base: &Url, flow: Option<u64>) -> ureq::Agent {
    #[cfg(debug_assertions)]
    if crate::fixture::loopback_url(base.as_str()).is_ok() {
        // A separate pool prevents a loopback assertion reaching an environment proxy.
        return ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .middleware(
                move |mut request: ureq::http::Request<ureq::SendBody>,
                      next: ureq::middleware::MiddlewareNext| {
                    crate::nett::add_organisation_header(&mut request);
                    match flow {
                        Some(account) => crate::nett::flow::through(account, request, next),
                        None => next.handle(request),
                    }
                },
            )
            .tls_config(
                subspace_framework::core::http::agent()
                    .config()
                    .tls_config()
                    .clone(),
            )
            .build()
            .into();
    }
    let _ = (base, flow);
    subspace_framework::core::http::agent().clone()
}

pub(crate) fn get<T: DeserializeOwned>(
    agent: &ureq::Agent,
    url: &Url,
    jwt: Option<&Secret>,
) -> Result<T, Error> {
    prepare(url);
    let mut request = agent
        .get(url.as_str())
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json");
    if let Some(jwt) = jwt {
        request = request.header("Authorization", format!("Bearer {}", jwt.expose()));
    }
    let response = request
        .config()
        .timeout_global(Some(READ_TIMEOUT))
        .timeout_recv_body(Some(READ_TIMEOUT))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .build()
        .call()
        .map_err(|_| Error::Network)?;
    read_json(check_response(response)?)
}

pub(crate) fn prepare(url: &Url) {
    #[cfg(test)]
    assert!(
        url.host_str().is_some_and(|host| host
            .trim_matches(['[', ']'])
            .parse::<std::net::IpAddr>()
            .is_ok_and(|ip| ip.is_loopback())),
        "offline client tests must use numeric loopback"
    );
    crate::pacing::wait(url);
}

pub(crate) fn get_bytes(
    agent: &ureq::Agent,
    url: &Url,
    jwt: Option<&Secret>,
    (mime, limit): (&str, usize),
) -> Result<Vec<u8>, Error> {
    get_bytes_with_timeout(agent, url, jwt, (mime, limit), READ_TIMEOUT)
}
pub(crate) fn get_bytes_with_timeout(
    agent: &ureq::Agent,
    url: &Url,
    jwt: Option<&Secret>,
    (mime, limit): (&str, usize),
    timeout: std::time::Duration,
) -> Result<Vec<u8>, Error> {
    prepare(url);
    let mut request = agent
        .get(url.as_str())
        .header("User-Agent", USER_AGENT)
        .header("Accept", mime);
    if let Some(jwt) = jwt {
        request = request.header("Authorization", format!("Bearer {}", jwt.expose()));
    }
    let response = request
        .config()
        .timeout_global(Some(timeout))
        .timeout_recv_body(Some(timeout))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .build()
        .call()
        .map_err(|_| Error::Network)?;
    let mut response = check_response(response)?;
    if header(&response, "content-type") != mime
        || !header(&response, "x-content-type-options").eq_ignore_ascii_case("nosniff")
    {
        return Err(Error::InvalidResponse);
    }
    response
        .body_mut()
        .with_config()
        .limit(limit as u64)
        .read_to_vec()
        .map_err(|_| Error::InvalidResponse)
}

pub(crate) fn check_response(response: Response<Body>) -> Result<Response<Body>, Error> {
    if is_access_challenge(&response) {
        return Err(Error::SignInNeeded);
    }
    match response.status() {
        StatusCode::OK | StatusCode::CREATED => Ok(response),
        StatusCode::UNAUTHORIZED => Err(Error::SignInNeeded),
        StatusCode::FORBIDDEN => Err(Error::Forbidden),
        StatusCode::NOT_FOUND => Err(Error::NotFound),
        status => Err(Error::UnexpectedStatus(status.as_u16())),
    }
}

pub(crate) fn is_access_challenge(response: &Response<Body>) -> bool {
    let location = header(response, "location");
    let location = if location.starts_with("//") {
        format!("https:{location}")
    } else {
        location.into()
    };
    // An authenticating proxy in front of the API redirects to its own login.
    let access_host = Url::parse(&location).ok().is_some_and(|url| {
        let path = url.path().to_ascii_lowercase();
        url.host_str().is_some()
            && ["login", "signin", "sign-in", "authorize", "/auth"]
                .iter()
                .any(|marker| path.contains(marker))
    });
    let problem = header(response, "content-type")
        .split(';')
        .next()
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/problem+json"));
    (response.status().is_redirection() && access_host)
        || (matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) && !problem)
}

pub(crate) fn header<'a>(response: &'a Response<Body>, name: &str) -> &'a str {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
}

pub(crate) fn read_json<T: DeserializeOwned>(mut response: Response<Body>) -> Result<T, Error> {
    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_JSON_BYTES)
        .read_to_string()
        .map_err(|_| Error::InvalidResponse)?;
    serde_json::from_str(&text).map_err(|_| Error::InvalidResponse)
}
