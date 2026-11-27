//! Blocking one-shot reads. Run them in `cx.background_spawn`, never in
//! render. Requests never follow a redirect, never use a proxy and carry the
//! token only in the `Authorization` header.

use std::io::{self, BufReader, Read};
use std::sync::OnceLock;
use std::time::Duration;

use serde::de::DeserializeOwned;
use subspace_framework::core::Secret;
use ureq::Body;
use ureq::http::{Response, StatusCode};

use crate::diff::{DiffFilePage, DiffTarget, V1Diff, V1DiffFilePage};
use crate::model::{FleetOverview, ShipDetail};
use crate::{Error, FleetAddress};

/// Stargate ends its reads one second before its 20 s request deadline.
pub(crate) const READ_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// A fleet with many ships and crew stays far below this.
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;
// Stargate v0.52.0 caps a list at 1,000 files and 4 MiB of UTF-8 patches,
// and a file page at 1 MiB of patch or raw bytes. JSON escaping takes at
// most six bytes per input byte, more than base64's expansion. Allow for
// two 4,096-code-point paths per entry, scalar fields, the cursor and envelope.
const DIFF_LIST_FILES: u64 = 1_000;
const DIFF_LIST_PATCH_BYTES: u64 = 4 * 1024 * 1024;
const DIFF_FILE_PAGE_BYTES: u64 = 1024 * 1024;
const DIFF_PATH_CODE_POINTS: u64 = 4_096;
const JSON_ESCAPE_BYTES: u64 = 6;
const DIFF_ENTRY_METADATA: u64 = 2 * DIFF_PATH_CODE_POINTS * JSON_ESCAPE_BYTES + 1024;
const DIFF_ENVELOPE_BYTES: u64 = 64 * 1024;
const MAX_DIFF_BYTES: u64 = DIFF_LIST_PATCH_BYTES * JSON_ESCAPE_BYTES
    + DIFF_LIST_FILES * DIFF_ENTRY_METADATA
    + DIFF_ENVELOPE_BYTES;
const MAX_FILE_BYTES: u64 =
    DIFF_FILE_PAGE_BYTES * JSON_ESCAPE_BYTES + DIFF_ENTRY_METADATA + DIFF_ENVELOPE_BYTES;
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

    /// Read one page of changed files. `None` means the route is unavailable.
    pub fn diff(
        &self,
        target: DiffTarget<'_>,
        cursor: Option<&str>,
    ) -> Result<Option<V1Diff>, Error> {
        let url = target.list_url(self.address(), cursor)?;
        match self.get_json(&url, MAX_DIFF_BYTES) {
            Ok(page) => Ok(Some(page)),
            Err(Error::NotFound) if cursor.is_none() => Ok(None),
            Err(Error::NotFound | Error::UnexpectedStatus(409)) => Err(Error::StaleDiff),
            Err(error) => Err(error),
        }
    }

    /// Read one page of a file. Unlike a list-route 404, a file-route 404
    /// can mean that the requested path does not exist.
    pub fn diff_file(
        &self,
        target: DiffTarget<'_>,
        request: DiffFilePage<'_>,
    ) -> Result<V1DiffFilePage, Error> {
        let url = target.file_url(self.address(), request)?;
        self.get_json(&url, MAX_FILE_BYTES)
            .map_err(|error| match error {
                Error::UnexpectedStatus(409) => Error::StaleDiff,
                other => other,
            })
    }

    fn get<T: DeserializeOwned>(&self, segments: &[&str]) -> Result<T, Error> {
        let url = self.address.url(segments)?;
        self.get_json(&url, MAX_JSON_BYTES)
    }

    fn get_json<T: DeserializeOwned>(&self, url: &url::Url, max_bytes: u64) -> Result<T, Error> {
        let mut response = check_status(
            agent()
                .get(url.as_str())
                .header("Accept", "application/json")
                .header("Cache-Control", "no-store")
                .header("Authorization", format!("Bearer {}", self.token.expose()))
                .call()
                .map_err(|_| Error::Unreachable)?,
        )?;
        let reader = response.body_mut().with_config().limit(max_bytes).reader();
        read_json(reader)
    }
}

fn read_json<T: DeserializeOwned>(reader: impl Read) -> Result<T, Error> {
    serde_json::from_reader(BufReader::new(reader)).map_err(|error| {
        // serde_json preserves the original I/O error; ureq wraps its own
        // timeout in ErrorKind::Other, while TCP can return TimedOut directly.
        match ureq::Error::from(io::Error::from(error)) {
            ureq::Error::Timeout(_) => Error::Unreachable,
            ureq::Error::Io(error) if error.kind() == io::ErrorKind::TimedOut => Error::Unreachable,
            _ => Error::InvalidResponse,
        }
    })
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

#[cfg(test)]
mod tests {
    use std::io;

    use super::{read_json, retry_after, status_error};
    use crate::Error;

    struct BrokenBody(Option<io::Error>);

    impl io::Read for BrokenBody {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            self.0.take().map_or(Ok(0), Err)
        }
    }

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

    #[test]
    fn body_timeouts_are_unreachable_but_oversized_or_malformed_bodies_are_invalid() {
        for (error, expected) in [
            (
                ureq::Error::Timeout(ureq::Timeout::RecvBody).into_io(),
                Error::Unreachable,
            ),
            (io::ErrorKind::TimedOut.into(), Error::Unreachable),
            (
                ureq::Error::BodyExceedsLimit(1).into_io(),
                Error::InvalidResponse,
            ),
        ] {
            assert_eq!(
                read_json::<serde_json::Value>(BrokenBody(Some(error))),
                Err(expected)
            );
        }
        assert_eq!(
            read_json::<serde_json::Value>(&b"{"[..]),
            Err(Error::InvalidResponse)
        );
    }
}
