//! NETT's file host: one credential-free `PUT` of a prepared upload, and
//! credential-free reads of short-lived grants. Neither request carries the
//! bearer, a cookie or an organisation header, and neither follows a
//! redirect. Completion polling after an upload goes back to the API.

use serde_json::{Value, json};
use std::time::Duration;
use url::Url;

use crate::{Client, Error, transport};

/// The longest completion wait slept through here; a longer one leaves the
/// upload pending, to be completed later with the same transfer.
const MAX_COMPLETION_WAIT: u64 = 60;
/// Completion attempts before an upload counts as unconfirmed.
const MAX_COMPLETIONS: usize = 8;
const UPLOAD_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// Why a prepared transfer did not finish. Callers map it onto their own
/// error type; nothing here carries response text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransferError {
    SignIn,
    Forbidden,
    Missing,
    Conflict,
    Invalid,
    /// 429 or 503, with the server's wait in whole seconds.
    Busy(u64),
    /// The outcome is unknown; check before sending again.
    Unclear,
    Network,
}

impl TransferError {
    pub(crate) fn from_status(status: u16, retry_after: u64) -> Self {
        match status {
            401 => Self::SignIn,
            403 => Self::Forbidden,
            404 => Self::Missing,
            409 => Self::Conflict,
            400 | 413 | 415 | 422 => Self::Invalid,
            429 | 503 => Self::Busy(retry_after),
            _ => Self::Unclear,
        }
    }
}

/// A file declaration as NETT's prepare routes take it.
pub(crate) struct Declaration<'a> {
    pub request_key: &'a str,
    pub file_name: &'a str,
    pub content_type: &'a str,
    pub bytes: &'a [u8],
}

impl Declaration<'_> {
    pub(crate) fn body(&self) -> Value {
        use sha2::Digest as _;
        let digest: String = sha2::Sha256::digest(self.bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        json!({
            "requestKey": self.request_key,
            "fileName": self.file_name,
            "contentType": self.content_type,
            "sizeBytes": self.bytes.len(),
            "sha256": digest,
        })
    }
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            subspace_framework::core::http::agent()
                .config()
                .tls_config()
                .clone(),
        )
        .proxy(None)
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into()
}

/// The server's wait in whole seconds, never shortened but at most five
/// minutes, as for every NETT wait; one second when it names none.
fn retry_after(response: &ureq::http::Response<ureq::Body>) -> u64 {
    transport::header(response, "retry-after")
        .trim()
        .parse::<u64>()
        .unwrap_or(1)
        .min(super::flow::MAX_PAUSE.as_secs())
}

/// HTTPS without credentials or a fragment. Plain HTTP only on a loopback
/// address, and only when the API itself is a loopback fixture.
pub(crate) fn file_url(value: &str, api: &Url) -> Result<Url, Error> {
    let url = Url::parse(value).map_err(|_| Error::InvalidResponse)?;
    let loopback = |url: &Url| {
        crate::fixture::loopback_url(&format!(
            "{}://{}/",
            url.scheme(),
            url.host_str().unwrap_or_default()
        ))
        .is_ok()
    };
    let secure = url.scheme() == "https"
        || (url.scheme() == "http" && api.scheme() == "http" && loopback(&url) && loopback(api));
    if !secure
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(Error::InvalidResponse);
    }
    Ok(url)
}

/// Sends the declared bytes once to the prepared upload URL.
pub(crate) fn put(
    upload: &Value,
    api: &Url,
    content_type: &str,
    bytes: &[u8],
) -> Result<(), TransferError> {
    let url = upload
        .get("uploadUrl")
        .and_then(Value::as_str)
        .ok_or(TransferError::Unclear)?;
    let url = file_url(url, api).map_err(|_| TransferError::Unclear)?;
    if upload
        .get("method")
        .and_then(Value::as_str)
        .is_some_and(|method| !method.eq_ignore_ascii_case("PUT"))
    {
        return Err(TransferError::Unclear);
    }
    transport::prepare(&url);
    let response = agent()
        .put(url.as_str())
        .header("User-Agent", transport::USER_AGENT)
        .header("Content-Type", content_type)
        .config()
        .timeout_global(Some(UPLOAD_TIMEOUT))
        .max_redirects_will_error(false)
        .build()
        .send(bytes)
        .map_err(|_| TransferError::Unclear)?;
    match response.status().as_u16() {
        200 | 201 | 204 => Ok(()),
        status => Err(TransferError::from_status(status, retry_after(&response))),
    }
}

/// When each transfer's completion may next be sent, after NETT asked for a
/// wait longer than one completion sleeps through. Kept per transfer, and
/// per chat draft, so a later check, whoever asks, waits too.
static NOT_BEFORE: std::sync::Mutex<Option<std::collections::HashMap<String, std::time::Instant>>> =
    std::sync::Mutex::new(None);

/// The whole seconds left of a wait kept for `key`, if any.
pub(crate) fn wait_left(key: &str) -> Option<u64> {
    let mut table = NOT_BEFORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let table = table.get_or_insert_with(Default::default);
    let now = std::time::Instant::now();
    table.retain(|_, at| *at > now);
    table
        .get(key)
        .and_then(|at| at.checked_duration_since(now))
        .map(|left| left.as_secs() + u64::from(left.subsec_nanos() > 0))
}

/// Keeps NETT's wait of `seconds` for `key`, for at most five minutes.
pub(crate) fn hold_until(key: &str, seconds: u64) {
    let at = super::flow::deadline(std::time::Instant::now(), Duration::from_secs(seconds));
    NOT_BEFORE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get_or_insert_with(Default::default)
        .insert(key.to_owned(), at);
}

/// Completes a transfer, waiting as the server asks while it checks the
/// bytes, and returns the stored record. A wait longer than a minute ends
/// the call with [`TransferError::Busy`] and is kept for the transfer: a
/// completion asked for before it has passed is refused here, unsent.
pub(crate) fn complete(
    client: &Client,
    path: &[&str],
    upload_id: &str,
    sleep: &dyn Fn(Duration),
) -> Result<Value, TransferError> {
    if let Some(left) = wait_left(upload_id) {
        return Err(TransferError::Busy(left));
    }
    let url = client
        .endpoint(path, &[])
        .map_err(|_| TransferError::Unclear)?;
    let body = json!({"uploadId": upload_id});
    for _ in 0..MAX_COMPLETIONS {
        let response = transport::post_json(client, &url, &body, transport::READ_TIMEOUT)
            .map_err(|_| TransferError::Unclear)?;
        if transport::is_access_challenge(&response) {
            return Err(TransferError::SignIn);
        }
        match response.status().as_u16() {
            200 | 201 => {
                let record = transport::read_json(response).map_err(|_| TransferError::Unclear)?;
                client.current().map_err(|_| TransferError::SignIn)?;
                return Ok(record);
            }
            202 => {
                let wait = retry_after(&response);
                if wait > MAX_COMPLETION_WAIT {
                    hold_until(upload_id, wait);
                    return Err(TransferError::Busy(wait));
                }
                sleep(Duration::from_secs(wait));
            }
            status => {
                let error = TransferError::from_status(status, retry_after(&response));
                if let TransferError::Busy(wait) = error {
                    hold_until(upload_id, wait);
                }
                return Err(error);
            }
        }
    }
    Err(TransferError::Unclear)
}

/// A JSON write to an organisation route, read back as JSON, with its
/// status mapped onto [`TransferError`].
pub(crate) fn write(
    client: &Client,
    method: &str,
    path: &[&str],
    query: &[(String, String)],
    body: Option<&Value>,
) -> Result<Option<Value>, TransferError> {
    let url = client
        .endpoint(path, query)
        .map_err(|_| TransferError::Unclear)?;
    transport::prepare(&url);
    let bearer = client
        .access_jwt
        .as_ref()
        .map(|jwt| format!("Bearer {}", jwt.expose()));
    // Routes without a body take no JSON: nothing is sent, not even a type.
    let response = if method == "DELETE" {
        let mut request = client
            .agent
            .delete(url.as_str())
            .header("User-Agent", transport::USER_AGENT)
            .header("Accept", "application/json");
        if let Some(bearer) = &bearer {
            request = request.header("Authorization", bearer);
        }
        request
            .config()
            .timeout_global(Some(transport::READ_TIMEOUT))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build()
            .call()
    } else {
        let request = match method {
            "PUT" => client.agent.put(url.as_str()),
            "PATCH" => client.agent.patch(url.as_str()),
            _ => client.agent.post(url.as_str()),
        };
        let mut request = request
            .header("User-Agent", transport::USER_AGENT)
            .header("Accept", "application/json");
        if let Some(bearer) = &bearer {
            request = request.header("Authorization", bearer);
        }
        if body.is_some() {
            request = request.header("Content-Type", "application/json");
        }
        let request = request
            .config()
            .timeout_global(Some(transport::READ_TIMEOUT))
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build();
        match body {
            Some(body) => request.send(body.to_string().as_bytes()),
            None => request.send_empty(),
        }
    }
    .map_err(|_| TransferError::Unclear)?;
    if transport::is_access_challenge(&response) {
        return Err(TransferError::SignIn);
    }
    match response.status().as_u16() {
        204 => Ok(None),
        200 | 201 => {
            let value = transport::read_json(response).map_err(|_| TransferError::Unclear)?;
            // A reply read after the account or organisation changed
            // belongs to no one.
            client.current().map_err(|_| TransferError::SignIn)?;
            #[cfg(test)]
            crate::native::test_lease::reply_accepted();
            Ok(Some(value))
        }
        status => Err(TransferError::from_status(status, retry_after(&response))),
    }
}

/// What a grant read must carry before its bytes are used.
pub(crate) struct Expected<'a> {
    pub content_type: &'a str,
    pub size: u64,
    pub limit: usize,
    /// Standalone attachments and chat files: `nosniff` and a
    /// `Content-Length` equal to the descriptor's size are required.
    /// Managed media need `nosniff` only for text.
    pub strict: bool,
}

/// Reads one grant URL without credentials: the exact stored type, then
/// exactly the descriptor's length, with `nosniff` and the length header
/// where [`Expected::strict`] asks for them.
pub(crate) fn fetch(url: &Url, expected: &Expected<'_>) -> Result<Vec<u8>, Error> {
    if expected.size == 0 || expected.size > expected.limit as u64 {
        return Err(Error::InvalidRequest);
    }
    transport::prepare(url);
    let response = agent()
        .get(url.as_str())
        .header("User-Agent", transport::USER_AGENT)
        .header("Accept", expected.content_type)
        .config()
        .timeout_global(Some(UPLOAD_TIMEOUT))
        .max_redirects_will_error(false)
        .build()
        .call()
        .map_err(|_| Error::Network)?;
    let mut response = transport::check_response(response)?;
    let length = transport::header(&response, "content-length")
        .parse::<u64>()
        .ok();
    let nosniff =
        transport::header(&response, "x-content-type-options").eq_ignore_ascii_case("nosniff");
    let needs_nosniff = expected.strict || crate::media::readable_text(expected.content_type);
    if transport::header(&response, "content-type") != expected.content_type
        || (needs_nosniff && !nosniff)
        || (expected.strict && length != Some(expected.size))
        || length.is_some_and(|length| length != expected.size)
    {
        return Err(Error::InvalidResponse);
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(expected.size)
        .read_to_vec()
        .map_err(|_| Error::InvalidResponse)?;
    if bytes.len() as u64 != expected.size {
        return Err(Error::InvalidResponse);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::{TransferError, file_url, hold_until, wait_left};
    use url::Url;

    #[test]
    fn a_long_wait_holds_the_transfer_for_five_minutes_at_most() {
        hold_until("transfer-held-long", u64::MAX);
        assert!(wait_left("transfer-held-long").is_some_and(|left| (295..=300).contains(&left)));
    }

    #[test]
    fn file_urls_are_https_or_loopback_fixtures_without_credentials() {
        let api = Url::parse("https://api.nett.example/api").unwrap();
        assert!(file_url("https://files.nett.example/f1?grant=g", &api).is_ok());
        for value in [
            "http://files.nett.example/f1?grant=g",
            "https://user@files.nett.example/f1",
            "https://files.nett.example/f1#x",
            "http://127.0.0.1:9000/files/f1",
        ] {
            assert!(file_url(value, &api).is_err(), "{value}");
        }
        let fixture = Url::parse("http://127.0.0.1:9000/api").unwrap();
        assert!(file_url("http://127.0.0.1:9000/files/f1?grant=g", &fixture).is_ok());
    }

    #[test]
    fn statuses_keep_the_servers_wait_for_busy_answers() {
        assert_eq!(TransferError::from_status(429, 7), TransferError::Busy(7));
        assert_eq!(TransferError::from_status(503, 2), TransferError::Busy(2));
        assert_eq!(TransferError::from_status(409, 0), TransferError::Conflict);
        assert_eq!(TransferError::from_status(500, 0), TransferError::Unclear);
    }
}
