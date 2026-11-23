//! The mapping of a failed GitHub response to a message.

use subspace_core::credentials::{Credential, Source};
use subspace_core::http::Error;
use subspace_core::http::status as http_status;

use super::Api;

/// A failed HTTP response, reduced to what the mapping reads.
#[derive(Debug, Clone, Copy)]
pub struct Failure<'a> {
    pub status: u16,
    /// `x-ratelimit-remaining`.
    pub remaining: Option<&'a str>,
    /// `x-ratelimit-reset`, Unix seconds.
    pub reset: Option<&'a str>,
    /// `Retry-After`, which GitHub sends with a secondary rate limit.
    pub retry_after: Option<&'a str>,
    /// GitHub's JSON `message`, or empty.
    pub message: &'a str,
}

pub(super) fn is_rate_limit(message: &str) -> bool {
    message.to_lowercase().contains("rate limit")
}

pub(super) fn is_sso(message: &str) -> bool {
    let lower = message.to_lowercase();
    message.contains("SAML")
        || lower.contains("single sign-on")
        || message
            .split(|c: char| !c.is_ascii_alphanumeric())
            .any(|word| word.eq_ignore_ascii_case("sso"))
}

pub fn status(status: u16, message: impl Into<String>) -> Error {
    Error::Status {
        status,
        message: message.into(),
    }
}

pub fn not_found() -> Error {
    status(http_status::NOT_FOUND, "Not found on GitHub.")
}

pub fn unexpected(what: &str) -> Error {
    status(
        http_status::BAD_GATEWAY,
        format!("GitHub returned an unexpected {what}."),
    )
}

/// "; it resets at HH:MM UTC", else "; try again in a few minutes".
pub fn reset_hint(reset: Option<&str>) -> String {
    reset
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|seconds| *seconds > 0)
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds, 0))
        .map_or_else(
            || "; try again in a few minutes".to_owned(),
            |time| format!("; it resets at {} UTC", time.format("%H:%M")),
        )
}

/// The account a message names: the signed-in or gh login, else the one
/// asked for.
pub fn account_label(credential: &Credential, login: Option<&str>) -> String {
    match &credential.source {
        Source::GhCli { login } | Source::SignedIn { login } => login.clone(),
        _ => login.unwrap_or("the GitHub token").to_owned(),
    }
}

/// The 401 message, naming where the rejected token came from. It keeps
/// status 401 so `send_with_retry` retries once.
pub fn rejected(credential: &Credential, account: &str) -> Error {
    let message = match &credential.source {
        Source::Environment(name) => format!("GitHub rejected the token in {name}."),
        Source::ConfigFile(key) => format!("GitHub rejected {key} from the config file."),
        Source::SignedIn { login } => format!(
            "GitHub no longer accepts the sign-in for {login}. Sign in with GitHub again in Settings."
        ),
        _ => format!("GitHub token rejected for {account}; run gh auth login."),
    };
    status(http_status::UNAUTHORIZED, message)
}

/// Maps a failed HTTP response (not 401) to a message.
pub fn http_failure(account: &str, api: Api, failure: Failure<'_>) -> Error {
    let Failure {
        status: code,
        remaining,
        reset,
        retry_after,
        message,
    } = failure;
    if matches!(
        code,
        http_status::FORBIDDEN | http_status::TOO_MANY_REQUESTS
    ) && (remaining == Some("0") || retry_after.is_some() || is_rate_limit(message))
    {
        return status(
            http_status::TOO_MANY_REQUESTS,
            format!(
                "GitHub rate limit reached for {account}{}.",
                reset_hint(reset)
            ),
        );
    }
    if code == http_status::FORBIDDEN && is_sso(message) {
        return status(http_status::FORBIDDEN, message);
    }
    match (api, code) {
        (Api::GraphQl, http_status::BAD_GATEWAY | http_status::GATEWAY_TIMEOUT) => status(
            http_status::GATEWAY_TIMEOUT,
            "GitHub took too long to answer (its 10-second query limit); try again.",
        ),
        (Api::Rest, http_status::NOT_FOUND) => not_found(),
        _ => status(
            http_status::BAD_GATEWAY,
            format!("GitHub request failed (HTTP {code})."),
        ),
    }
}
