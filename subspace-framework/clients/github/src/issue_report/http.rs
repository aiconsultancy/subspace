//! Shared HTTP agent and bounded response reader, without retries or redirects.

use std::time::{Duration, Instant};

use serde_json::Value;
use subspace_core::credentials::Credential;
use subspace_core::http::{ERROR_BODY_BYTES, agent, status};

use crate::{Client, transport};

pub(super) struct Response {
    pub status: u16,
    pub rate_limited: bool,
    pub body: Vec<u8>,
}

pub(super) fn send(
    client: &Client,
    credential: &Credential,
    path: &str,
    body: Option<&Value>,
    deadline: Instant,
) -> Result<Response, ()> {
    let remaining = deadline.checked_duration_since(Instant::now()).ok_or(())?;
    let url = format!("{}{path}", client.api_url);
    // Test builds cannot dispatch a report to GitHub, even through a bad fixture.
    #[cfg(test)]
    assert!(url::Url::parse(&url).is_ok_and(|url| url.host_str() == Some("127.0.0.1")));
    let response = match body {
        Some(body) => authenticated(agent().post(&url), credential, remaining)
            .header("Content-Type", "application/json")
            .send(body.to_string()),
        None => authenticated(agent().get(&url), credential, remaining).call(),
    }
    .map_err(|_| ())?;
    let status = response.status().as_u16();
    let rate_limited = status == status::TOO_MANY_REQUESTS
        || (status == status::FORBIDDEN
            && response
                .headers()
                .get("x-ratelimit-remaining")
                .is_some_and(|value| value == "0"));
    let body = transport::read_limited(response, ERROR_BODY_BYTES as usize).map_err(|_| ())?;
    Ok(Response {
        status,
        rate_limited,
        body,
    })
}

fn authenticated<Body>(
    request: ureq::RequestBuilder<Body>,
    credential: &Credential,
    timeout: Duration,
) -> ureq::RequestBuilder<Body> {
    request
        .header("Authorization", credential.authorization().expose())
        .header("Accept", transport::JSON)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "subspace")
        .config()
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .build()
}
