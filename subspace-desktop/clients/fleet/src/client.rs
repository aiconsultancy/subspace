//! Blocking one-shot reads. Run them in `cx.background_spawn`, never in
//! render. Requests never follow a redirect, never use a proxy and carry the
//! token only in the `Authorization` header.

use std::sync::OnceLock;
use std::time::Duration;

use serde::de::DeserializeOwned;
use subspace_framework::core::Secret;
use ureq::Body;
use ureq::http::{Response, StatusCode};

use crate::model::{FleetOverview, ShipDetail};
use crate::{Error, FleetAddress};

/// Stargate ends its reads one second before its 20 s request deadline.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A fleet with many ships and crew stays far below this.
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
pub(crate) const USER_AGENT: &str = concat!("Subspace/", env!("CARGO_PKG_VERSION"));

/// A fleet's address and the token saved for it. Deliberately has no
/// `Debug` that could print the token.
#[derive(Clone)]
pub struct FleetClient {
    address: FleetAddress,
    token: Secret,
}

impl FleetClient {
    /// A client for `address` with `token`. The token is only ever sent
    /// to `address`.
    pub fn new(address: FleetAddress, token: Secret) -> Self {
        Self { address, token }
    }

    pub fn address(&self) -> &FleetAddress {
        &self.address
    }

    pub(crate) fn token(&self) -> &Secret {
        &self.token
    }

    /// `GET /api/v1/fleet`: the fleet, its rear admiral, ships and host.
    pub fn fleet(&self) -> Result<FleetOverview, Error> {
        self.get(&["fleet"])
    }

    /// `GET /api/v1/ships/{id}`: one ship with its crew and their work.
    pub fn ship(&self, id: &str) -> Result<ShipDetail, Error> {
        self.get(&["ships", id])
    }

    fn get<T: DeserializeOwned>(&self, segments: &[&str]) -> Result<T, Error> {
        let url = self.address.url(segments)?;
        let response = agent()
            .get(url.as_str())
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", self.token.expose()))
            .call()
            .map_err(|_| Error::Unreachable)?;
        read_json(check_status(response)?)
    }
}

/// The one agent for Fleet API reads: no proxy, no redirects, bounded time.
fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::Agent::config_builder()
            .proxy(None)
            .http_status_as_error(false)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(READ_TIMEOUT))
            .user_agent(USER_AGENT)
            .tls_config(
                subspace_framework::core::http::agent()
                    .config()
                    .tls_config()
                    .clone(),
            )
            .build()
            .into()
    })
}

/// The meaning of a Fleet API status, the same for reads and the stream.
pub(crate) fn status_error(status: u16, retry_after: Option<u64>) -> Option<Error> {
    match status {
        200 => None,
        401 => Some(Error::TokenRefused),
        403 => Some(Error::Forbidden),
        404 | 405 => Some(Error::NotFound),
        429 | 503 => Some(Error::Busy(retry_after)),
        other => Some(Error::UnexpectedStatus(other)),
    }
}

/// `Retry-After` in whole seconds, when given as a number.
pub(crate) fn retry_after(value: Option<&str>) -> Option<u64> {
    value.and_then(|value| value.trim().parse::<u64>().ok())
}

fn check_status(response: Response<Body>) -> Result<Response<Body>, Error> {
    let wait = retry_after(
        response
            .headers()
            .get("retry-after")
            .and_then(|value| value.to_str().ok()),
    );
    match status_error(response.status().as_u16(), wait) {
        None if response.status() == StatusCode::OK => Ok(response),
        None => Err(Error::UnexpectedStatus(response.status().as_u16())),
        Some(error) => Err(error),
    }
}

fn read_json<T: DeserializeOwned>(mut response: Response<Body>) -> Result<T, Error> {
    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_JSON_BYTES)
        .read_to_string()
        .map_err(|_| Error::InvalidResponse)?;
    serde_json::from_str(&text).map_err(|_| Error::InvalidResponse)
}

#[cfg(test)]
mod tests {
    use super::{retry_after, status_error};
    use crate::Error;

    #[test]
    fn statuses_keep_their_meaning() {
        assert_eq!(status_error(200, None), None);
        assert_eq!(status_error(401, None), Some(Error::TokenRefused));
        assert_eq!(status_error(403, None), Some(Error::Forbidden));
        assert_eq!(status_error(404, None), Some(Error::NotFound));
        assert_eq!(status_error(503, Some(4)), Some(Error::Busy(Some(4))));
        assert_eq!(status_error(302, None), Some(Error::UnexpectedStatus(302)));
        assert_eq!(retry_after(Some(" 7 ")), Some(7));
        assert_eq!(retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")), None);
    }
}
