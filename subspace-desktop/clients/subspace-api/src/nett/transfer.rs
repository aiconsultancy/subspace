//! NETT's file host: one credential-free `PUT` of a prepared upload, and
//! credential-free reads of short-lived grants. Neither request carries the
//! bearer, a cookie or an organisation header, and neither follows a
//! redirect. Completion polling after an upload goes back to the API.

use serde_json::{Value, json};
use std::time::Duration;
use url::Url;

use crate::{Client, Error, transport};

/// The most one completion call sleeps in all while NETT checks the bytes,
/// in whole seconds. A wait that would take it past this leaves the upload
/// pending, to be completed later with the same transfer, so a slow check
/// never holds a worker for minutes.
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
/// bytes, and returns the stored record. A wait that would take the call
/// past a minute of waiting in all ends it with [`TransferError::Busy`] and
/// is kept for the transfer: a completion asked for before it has passed is
/// refused here, unsent.
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
    let mut waited: u64 = 0;
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
                // The reply holds one of the account's places until it is
                // dropped: let it go before sleeping, so other requests are
                // not held up behind a check that is only waiting.
                drop(response);
                if waited.saturating_add(wait) > MAX_COMPLETION_WAIT {
                    hold_until(upload_id, wait);
                    return Err(TransferError::Busy(wait));
                }
                waited += wait;
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
    let response = send_write(client, method, &url, body).map_err(|_| TransferError::Unclear)?;
    write_reply(client, response)
}

/// Sends one write with `method`. Routes without a body take no JSON:
/// nothing is sent, not even a type.
fn send_write(
    client: &Client,
    method: &str,
    url: &Url,
    body: Option<&Value>,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    let bearer = client
        .access_jwt
        .as_ref()
        .map(|jwt| format!("Bearer {}", jwt.expose()));
    if method == "DELETE" {
        return api_request(client.agent.delete(url.as_str()), bearer.as_deref()).call();
    }
    let request = match method {
        "PUT" => client.agent.put(url.as_str()),
        "PATCH" => client.agent.patch(url.as_str()),
        _ => client.agent.post(url.as_str()),
    };
    let request = api_request(request, bearer.as_deref());
    match body {
        Some(body) => request
            .header("Content-Type", "application/json")
            .send(body.to_string().as_bytes()),
        None => request.send_empty(),
    }
}

/// An API request's headers and limits: JSON accepted, the bearer when
/// there is one, and no redirect followed.
fn api_request<B>(
    request: ureq::RequestBuilder<B>,
    bearer: Option<&str>,
) -> ureq::RequestBuilder<B> {
    let request = request
        .header("User-Agent", transport::USER_AGENT)
        .header("Accept", "application/json");
    let request = match bearer {
        Some(bearer) => request.header("Authorization", bearer),
        None => request,
    };
    request
        .config()
        .timeout_global(Some(transport::READ_TIMEOUT))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .build()
}

/// A write's reply, read as JSON, with its status mapped onto
/// [`TransferError`].
fn write_reply(
    client: &Client,
    response: ureq::http::Response<ureq::Body>,
) -> Result<Option<Value>, TransferError> {
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
    if !acceptable(&response, expected) {
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

/// Whether a grant read's headers carry what [`Expected`] asks for: the
/// exact stored type, `nosniff` where it is needed, and the descriptor's
/// length wherever a length is sent or required.
fn acceptable(response: &ureq::http::Response<ureq::Body>, expected: &Expected<'_>) -> bool {
    let length = transport::header(response, "content-length")
        .parse::<u64>()
        .ok();
    let nosniff =
        transport::header(response, "x-content-type-options").eq_ignore_ascii_case("nosniff");
    let needs_nosniff = expected.strict || crate::media::readable_text(expected.content_type);
    transport::header(response, "content-type") == expected.content_type
        && (nosniff || !needs_nosniff)
        && (!expected.strict || length == Some(expected.size))
        && length.is_none_or(|length| length == expected.size)
}

#[cfg(test)]
mod tests {
    use super::{TransferError, file_url, hold_until, wait_left};
    use url::Url;

    #[test]
    fn a_completion_waiting_on_a_202_holds_no_place_while_it_sleeps() {
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let mock = subspace_framework::client_test_support::Mock::with_headers(move |_| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                (202, vec![("Retry-After".into(), "1".into())], "{}".into())
            } else {
                (201, Vec::new(), r#"{"id":"stored"}"#.into())
            }
        });
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-000000000001"),
            Some(subspace_framework::core::Secret::new(
                "nett-completion-places",
            )),
        )
        .unwrap()
        .without_live();
        let account = client.flow_account();
        let held = std::cell::Cell::new(usize::MAX);
        let record = super::complete(&client, &["chat", "uploads", "from-upload"], "u1", &|_| {
            held.set(super::super::flow::active(account));
        })
        .unwrap();
        assert_eq!(record["id"], "stored");
        assert_eq!(held.get(), 0, "no place is held while waiting");
    }

    #[test]
    fn a_completion_waits_a_minute_in_all_then_leaves_the_transfer_pending() {
        let mock = subspace_framework::client_test_support::Mock::with_headers(|_| {
            (202, vec![("Retry-After".into(), "25".into())], "{}".into())
        });
        let client = crate::Client::nett_loopback(
            &format!("{}/api", mock.base_url()),
            Some("00000000-0000-4000-8000-000000000001"),
            Some(subspace_framework::core::Secret::new(
                "nett-completion-bound",
            )),
        )
        .unwrap()
        .without_live();
        let slept = std::cell::Cell::new(std::time::Duration::ZERO);
        let result = super::complete(
            &client,
            &["chat", "uploads", "from-upload"],
            "transfer-bounded",
            &|wait| slept.set(slept.get() + wait),
        );
        // Two waits of 25 seconds fit in a minute; a third would not.
        assert_eq!(result, Err(TransferError::Busy(25)));
        assert_eq!(slept.get(), std::time::Duration::from_secs(50));
        assert_eq!(mock.requests().len(), 3, "three completions, not eight");
        assert!(
            wait_left("transfer-bounded").is_some(),
            "the server's wait is kept for the transfer"
        );
    }

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
