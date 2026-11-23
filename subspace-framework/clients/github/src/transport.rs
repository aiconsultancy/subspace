//! Requests to api.github.com and their error mapping.
//! Every function takes the credential for one attempt; the caller wraps it
//! in [`send_with_retry`](subspace_core::http::send_with_retry) so a 401
//! gets a fresh token once.

use serde::Deserialize;
use serde_json::Value;
use subspace_core::credentials::Credential;
use subspace_core::http::status as http_status;
use subspace_core::http::{Error, agent};
use ureq::Body;
use ureq::http::Response;

const USER_AGENT: &str = "subspace";
const API_VERSION: &str = "2022-11-28";
const ERROR_BODY_LIMIT_BYTES: u64 = subspace_core::http::ERROR_BODY_BYTES;
pub const JSON: &str = "application/vnd.github+json";
pub const RAW: &str = "application/vnd.github.raw+json";

mod graphql;
mod mapping;
pub(crate) use graphql::post_bound_graphql;
pub use graphql::post_graphql;

pub use mapping::{Failure, account_label, http_failure, not_found, rejected, status, unexpected};

use mapping::{is_rate_limit, is_sso};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Api {
    GraphQl,
    Rest,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct GraphQlError {
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub extensions: Option<Extensions>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Extensions {
    #[serde(default)]
    pub code: Option<String>,
}

/// Maps GraphQL `errors` to one error.
pub fn graphql_failure(account: &str, errors: &[GraphQlError]) -> Error {
    let message = |error: &GraphQlError| error.message.clone().unwrap_or_default();
    if let Some(sso) = errors.iter().find(|error| is_sso(&message(error))) {
        return status(http_status::FORBIDDEN, message(sso));
    }
    let limited = errors.iter().any(|error| {
        error.kind.as_deref() == Some("RATE_LIMITED") || is_rate_limit(&message(error))
    });
    if limited {
        return status(
            http_status::TOO_MANY_REQUESTS,
            format!("GitHub rate limit reached for {account}; try again in a few minutes."),
        );
    }
    let first = errors.first();
    let text = first
        .and_then(|error| error.message.clone())
        .unwrap_or_else(|| "GitHub returned an error.".to_owned());
    let kind = first.and_then(|error| error.kind.as_deref());
    if kind == Some("FORBIDDEN") {
        return status(http_status::FORBIDDEN, text);
    }
    let has_code = first
        .and_then(|error| error.extensions.as_ref())
        .and_then(|extensions| extensions.code.as_ref())
        .is_some();
    let invalid = matches!(kind, Some("NOT_FOUND" | "UNPROCESSABLE" | "INVALID")) || has_code;
    status(
        if invalid {
            http_status::BAD_REQUEST
        } else {
            http_status::BAD_GATEWAY
        },
        text,
    )
}

#[derive(Debug, Default, Deserialize)]
pub struct GraphQlResponse {
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub errors: Vec<GraphQlError>,
    #[serde(default)]
    pub message: Option<String>,
}

/// The `data` of a GraphQL answer. With `partial`, data that came with
/// errors (such as null nodes in SSO-protected organisations) is kept.
pub fn graphql_data(
    account: &str,
    response: GraphQlResponse,
    partial: bool,
) -> Result<Value, Error> {
    let data = response.data.filter(|data| !data.is_null());
    if !response.errors.is_empty() && !(partial && data.is_some()) {
        return Err(graphql_failure(account, &response.errors));
    }
    data.ok_or_else(|| status(http_status::BAD_GATEWAY, "GitHub returned no data."))
}

fn header<'a>(response: &'a Response<Body>, name: &str) -> Option<&'a str> {
    response.headers().get(name)?.to_str().ok()
}

fn network(_: ureq::Error) -> Error {
    Error::Network("GitHub could not be reached.".into())
}

/// Maps a response that is not a success; `Ok` passes it through.
fn check(
    credential: &Credential,
    account: &str,
    api: Api,
    mut response: Response<Body>,
) -> Result<Response<Body>, Error> {
    let code = response.status().as_u16();
    if response.status().is_success() {
        return Ok(response);
    }
    if code == http_status::UNAUTHORIZED {
        return Err(rejected(credential, account));
    }
    let text = response
        .body_mut()
        .with_config()
        .limit(ERROR_BODY_LIMIT_BYTES)
        .read_to_string()
        .unwrap_or_default();
    let message = serde_json::from_str::<GraphQlResponse>(&text)
        .ok()
        .and_then(|body| body.message)
        .unwrap_or_default();
    if code == http_status::BAD_REQUEST {
        return Err(status(http_status::BAD_REQUEST, message));
    }
    Err(http_failure(
        account,
        api,
        Failure {
            status: code,
            remaining: header(&response, "x-ratelimit-remaining"),
            reset: header(&response, "x-ratelimit-reset"),
            retry_after: header(&response, "retry-after"),
            message: &message,
        },
    ))
}

/// GETs a REST path; the caller reads the body.
pub fn get_rest(
    url: &str,
    credential: &Credential,
    account: &str,
    accept: &str,
) -> Result<Response<Body>, Error> {
    get_rest_version(url, credential, account, accept, API_VERSION)
}

pub fn get_rest_version(
    url: &str,
    credential: &Credential,
    account: &str,
    accept: &str,
    version: &str,
) -> Result<Response<Body>, Error> {
    let authorization = credential.authorization();
    let response = agent()
        .get(url)
        .header("Authorization", authorization.expose())
        .header("Accept", accept)
        .header("X-GitHub-Api-Version", version)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(network)?;
    check(credential, account, Api::Rest, response)
}

/// The Copilot agent API names the client that calls it.
pub const COPILOT_INTEGRATION_ID: &str = "copilot-4-cli";
/// The API version the Copilot agent endpoints answer.
pub const COPILOT_API_VERSION: &str = "2026-01-09";
pub const EVENT_STREAM: &str = "text/event-stream";

/// GETs a Copilot agent API path; the caller reads the body. The Copilot
/// endpoints need their own integration id and version, so they do not go
/// through [`get_rest`].
pub fn get_copilot(
    url: &str,
    credential: &Credential,
    account: &str,
    accept: &str,
) -> Result<Response<Body>, Error> {
    let authorization = credential.authorization();
    let response = agent()
        .get(url)
        .header("Authorization", authorization.expose())
        .header("Accept", accept)
        .header("Copilot-Integration-Id", COPILOT_INTEGRATION_ID)
        .header("X-GitHub-Api-Version", COPILOT_API_VERSION)
        .header("User-Agent", USER_AGENT)
        .call()
        .map_err(network)?;
    check(credential, account, Api::Rest, response)
}

pub fn is_api_version_rejection(error: &Error) -> bool {
    matches!(error, Error::Status { status: http_status::BAD_REQUEST, message } if {
        let message = message.to_ascii_lowercase();
        message.contains("api version") || message.contains("api-version") || message.contains("unsupported version")
    })
}

/// POSTs JSON to a REST endpoint. The caller may retry a rejected API version
/// without retrying the operation itself.
pub fn post_rest_json(
    url: &str,
    credential: &Credential,
    account: &str,
    body: &str,
    version: &str,
) -> Result<(u16, String), Error> {
    post_rest_json_request(agent().post(url), credential, account, body, version)
}

/// The same JSON transport with caller-owned request configuration, such as a
/// remaining submission budget. Authentication, limits and status mapping agree.
pub(crate) fn post_rest_json_request(
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    credential: &Credential,
    account: &str,
    body: &str,
    version: &str,
) -> Result<(u16, String), Error> {
    let response = request
        .header("Authorization", credential.authorization().expose())
        .header("Content-Type", "application/json")
        .header("Accept", JSON)
        .header("X-GitHub-Api-Version", version)
        .header("User-Agent", USER_AGENT)
        .send(body)
        .map_err(network)?;
    let code = response.status().as_u16();
    let mut response = response;
    let text = response
        .body_mut()
        .with_config()
        .limit(subspace_core::http::MAX_BODY_BYTES)
        .read_to_string()
        .map_err(network)?;
    if !http_status::is_success(code) {
        if code == http_status::UNAUTHORIZED {
            return Err(rejected(credential, account));
        }
        let message = serde_json::from_str::<GraphQlResponse>(&text)
            .ok()
            .and_then(|body| body.message)
            .unwrap_or_default();
        if code == http_status::BAD_REQUEST {
            return Err(status(http_status::BAD_REQUEST, message));
        }
        return Err(http_failure(
            account,
            Api::Rest,
            Failure {
                status: code,
                remaining: None,
                reset: None,
                retry_after: None,
                message: &message,
            },
        ));
    }
    Ok((code, text))
}

/// Reads a body up to `max` bytes; a larger one is a 413.
pub fn read_limited(mut response: Response<Body>, max: usize) -> Result<Vec<u8>, Error> {
    let too_large = || {
        status(
            http_status::PAYLOAD_TOO_LARGE,
            format!("The file is larger than {} MB.", max / 1024 / 1024),
        )
    };
    let declared = header(&response, "content-length").and_then(|value| value.parse::<u64>().ok());
    if declared.is_some_and(|length| length > max as u64) {
        return Err(too_large());
    }
    response
        .body_mut()
        .with_config()
        .limit(max as u64)
        .read_to_vec()
        .map_err(|error| match error {
            ureq::Error::BodyExceedsLimit(_) => too_large(),
            other => network(other),
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use subspace_core::Secret;
    use subspace_core::credentials::{Credential, Scheme, Source};
    use subspace_core::http::Error;

    use super::mapping::reset_hint;
    use super::{
        Api, Failure, GraphQlError, GraphQlResponse, graphql_data, graphql_failure, http_failure,
        rejected,
    };

    fn code(error: &Error) -> u16 {
        match error {
            Error::Status { status, .. } => *status,
            _ => 0,
        }
    }

    fn failure(
        status: u16,
        remaining: Option<&'static str>,
        message: &'static str,
    ) -> Failure<'static> {
        Failure {
            status,
            remaining,
            reset: Some("1790000000"),
            retry_after: None,
            message,
        }
    }

    #[test]
    fn rate_limits_name_the_reset_time() {
        let error = http_failure("ann", Api::Rest, failure(403, Some("0"), ""));
        assert_eq!(code(&error), 429);
        assert!(
            error
                .to_string()
                .starts_with("GitHub rate limit reached for ann; it resets at ")
        );
        assert!(error.to_string().ends_with(" UTC."));
        let by_message = http_failure(
            "ann",
            Api::GraphQl,
            failure(429, None, "API rate limit exceeded"),
        );
        assert_eq!(code(&by_message), 429);
        let secondary = Failure {
            retry_after: Some("60"),
            ..failure(403, None, "You have been blocked for a while.")
        };
        let secondary = http_failure("ann", Api::Rest, secondary);
        assert_eq!(
            code(&secondary),
            429,
            "a 403 with Retry-After is a rate limit"
        );
        assert_eq!(reset_hint(None), "; try again in a few minutes");
        assert_eq!(reset_hint(Some("0")), "; try again in a few minutes");
        assert_eq!(reset_hint(Some("3600")), "; it resets at 01:00 UTC");
    }

    #[test]
    fn sso_timeouts_and_not_found() {
        let sso = http_failure(
            "ann",
            Api::Rest,
            failure(
                403,
                None,
                "Resource protected by organization SAML enforcement.",
            ),
        );
        assert_eq!(code(&sso), 403);
        assert_eq!(
            code(&http_failure("ann", Api::GraphQl, failure(502, None, ""))),
            504
        );
        assert_eq!(
            code(&http_failure("ann", Api::Rest, failure(502, None, ""))),
            502
        );
        assert_eq!(
            code(&http_failure("ann", Api::Rest, failure(404, None, ""))),
            404
        );
        assert_eq!(
            code(&http_failure("ann", Api::GraphQl, failure(404, None, ""))),
            502
        );
        assert_eq!(
            code(&http_failure("ann", Api::Rest, failure(403, None, "nope"))),
            502
        );
    }

    fn errors(value: serde_json::Value) -> Vec<GraphQlError> {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn graphql_errors_map_by_type() {
        let cases = [
            (json!([{"type": "FORBIDDEN", "message": "x"}]), 403),
            (json!([{"message": "Resource protected by SSO"}]), 403),
            (json!([{"type": "RATE_LIMITED"}]), 429),
            (json!([{"type": "NOT_FOUND", "message": "gone"}]), 400),
            (
                json!([{"message": "bad", "extensions": {"code": "undefinedField"}}]),
                400,
            ),
            (json!([{"message": "boom"}]), 502),
        ];
        for (value, expected) in cases {
            assert_eq!(
                code(&graphql_failure("ann", &errors(value.clone()))),
                expected,
                "{value}"
            );
        }
    }

    #[test]
    fn partial_data_is_kept_only_when_asked() {
        let response = || GraphQlResponse {
            data: Some(json!({"search": {}})),
            errors: errors(json!([{"type": "FORBIDDEN", "message": "SAML"}])),
            message: None,
        };
        assert!(graphql_data("ann", response(), true).is_ok());
        assert_eq!(
            code(&graphql_data("ann", response(), false).unwrap_err()),
            403
        );
        let empty = GraphQlResponse::default();
        assert_eq!(code(&graphql_data("ann", empty, true).unwrap_err()), 502);
    }

    #[test]
    fn a_rejected_token_names_its_source_and_keeps_401() {
        let credential = |source| Credential {
            value: Secret::new("t"),
            scheme: Scheme::Bearer,
            source,
        };
        let env = rejected(&credential(Source::Environment("GH_TOKEN")), "ann");
        assert_eq!(code(&env), 401);
        assert_eq!(env.to_string(), "GitHub rejected the token in GH_TOKEN.");
        let cli = rejected(
            &credential(Source::GhCli {
                login: "ann".into(),
            }),
            "ann",
        );
        assert_eq!(
            cli.to_string(),
            "GitHub token rejected for ann; run gh auth login."
        );
    }
}
