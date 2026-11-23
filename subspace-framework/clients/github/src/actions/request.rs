use super::{DeploymentStatus, Page};
use crate::{
    Client, MAX_BODY_BYTES,
    transport::{self, JSON, unexpected},
    validate::{check_login, check_repo},
};
use serde::de::DeserializeOwned;
use std::io::Read;
use subspace_core::http::{Error, send_with_retry, status as http_status};

const JOB_LOG_BYTES: usize = 1024 * 1024;
const LOG_BUFFER_DIVISOR: usize = 16;
const LOG_TRUNCATION_MARKER: &str = "\n\n[Subspace truncated this log at 1 MB.]\n";

/// The run-wide jobs read keeps only each job's latest attempt.
const LATEST_JOBS: &[(&str, &str)] = &[("filter", "latest")];
/// One attempt's jobs read takes no filter: GitHub has none for it.
const ATTEMPT_JOBS: &[(&str, &str)] = &[];

/// The endpoint and query for run `run_id`'s jobs: attempt `attempt`'s, or
/// the latest jobs for attempt 0, an attempt the run did not name.
pub(super) fn jobs_endpoint(
    run_id: u64,
    attempt: u64,
) -> (String, &'static [(&'static str, &'static str)]) {
    match attempt {
        0 => (format!("actions/runs/{run_id}/jobs"), LATEST_JOBS),
        attempt => (
            format!("actions/runs/{run_id}/attempts/{attempt}/jobs"),
            ATTEMPT_JOBS,
        ),
    }
}

pub(super) fn job_log(
    client: &Client,
    login: &str,
    repo: &str,
    job_id: u64,
) -> Result<super::JobLog, Error> {
    check_login(login)?;
    let url = action_url(client, repo, &format!("actions/jobs/{job_id}/logs"), &[])?;
    let bytes = send_with_retry(
        &client.credentials,
        |credentials| credentials.github(Some(login)),
        |credential| {
            let account = transport::account_label(credential, Some(login));
            let mut response = transport::get_rest(&url, credential, &account, transport::RAW)?;
            let mut bytes = Vec::with_capacity(JOB_LOG_BYTES / LOG_BUFFER_DIVISOR);
            response
                .body_mut()
                .as_reader()
                .take((JOB_LOG_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| Error::Network("GitHub could not be reached.".into()))?;
            Ok(bytes)
        },
    )
    .map_err(|error| scope_error(error, "Actions: read"))?;
    let truncated = bytes.len() > JOB_LOG_BYTES;
    let bytes = if truncated {
        &bytes[..JOB_LOG_BYTES]
    } else {
        &bytes
    };
    let mut text = String::from_utf8_lossy(bytes).into_owned();
    if truncated {
        text.push_str(LOG_TRUNCATION_MARKER);
    }
    Ok(super::JobLog { text, truncated })
}

pub(super) struct PageRequest<'a> {
    pub repo: &'a str,
    pub endpoint: &'a str,
    pub key: &'a str,
    pub query: &'a [(&'a str, &'a str)],
    pub max_pages: usize,
}

pub(super) struct PostRequest<'a> {
    pub repo: &'a str,
    pub endpoint: &'a str,
    pub body: &'a str,
    pub scope: &'a str,
}

pub(super) fn deployment_status(
    client: &Client,
    login: &str,
    repo: &str,
    deployment_id: u64,
) -> Result<Option<DeploymentStatus>, Error> {
    let endpoint = format!("deployments/{deployment_id}/statuses");
    let page = page(
        client,
        login,
        &PageRequest {
            repo,
            endpoint: &endpoint,
            key: "",
            query: &[("per_page", "1")],
            max_pages: 1,
        },
    )?;
    Ok(page.items.into_iter().next())
}

pub(super) fn json<T: DeserializeOwned>(
    client: &Client,
    login: &str,
    repo: &str,
    endpoint: &str,
    query: &[(&str, &str)],
) -> Result<T, Error> {
    check_login(login)?;
    let url = action_url(client, repo, endpoint, query)?;
    let bytes = client
        .rest_bytes(login, &url, JSON, MAX_BODY_BYTES)
        .map_err(|error| scope_error(error, "Actions: read"))?;
    serde_json::from_slice(&bytes).map_err(|_| unexpected("Actions response"))
}

pub(super) fn page<T: DeserializeOwned>(
    client: &Client,
    login: &str,
    request: &PageRequest<'_>,
) -> Result<Page<T>, Error> {
    check_login(login)?;
    let mut query = request.query.to_vec();
    if !query.iter().any(|(key, _)| *key == "per_page") {
        query.push(("per_page", "100"));
    }
    let mut next = Some(action_url(client, request.repo, request.endpoint, &query)?);
    let mut items = Vec::new();
    for _ in 0..request.max_pages {
        let Some(url) = next.take() else { break };
        validate_page_url(client, &url)?;
        let (mut page, following) = page_response(client, login, &url, request.key)?;
        items.append(&mut page);
        next = following;
    }
    Ok(Page {
        items,
        truncated: next.is_some(),
    })
}

fn page_response<T: DeserializeOwned>(
    client: &Client,
    login: &str,
    url: &str,
    key: &str,
) -> Result<(Vec<T>, Option<String>), Error> {
    send_with_retry(
        &client.credentials,
        |credentials| credentials.github(Some(login)),
        |credential| {
            let account = transport::account_label(credential, Some(login));
            let response = transport::get_rest(url, credential, &account, JSON)?;
            let next = next_link(&response);
            let bytes = transport::read_limited(response, MAX_BODY_BYTES)?;
            let items = if key.is_empty() {
                serde_json::from_slice(&bytes)
            } else {
                let value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|_| unexpected("Actions response"))?;
                let selected = value.get(key).cloned().unwrap_or(serde_json::Value::Null);
                serde_json::from_value(selected)
            }
            .map_err(|_| unexpected("Actions response"))?;
            Ok((items, next))
        },
    )
    .map_err(|error| scope_error(error, "Actions: read"))
}

fn action_url(
    client: &Client,
    repo: &str,
    endpoint: &str,
    query: &[(&str, &str)],
) -> Result<String, Error> {
    let (owner, name) = check_repo(repo)?;
    let segments = ["repos", owner.as_str(), name.as_str()]
        .into_iter()
        .chain(endpoint.split('/').filter(|segment| !segment.is_empty()));
    client.rest_url(segments, query)
}

fn validate_page_url(client: &Client, url: &str) -> Result<(), Error> {
    let base = url::Url::parse(&client.api_url).map_err(|_| unexpected("GitHub API URL"))?;
    let target = url::Url::parse(url).map_err(|_| unexpected("pagination link"))?;
    if target.origin() != base.origin()
        || !target.username().is_empty()
        || target.password().is_some()
    {
        return Err(unexpected("pagination link"));
    }
    Ok(())
}

pub(super) fn post(client: &Client, login: &str, request: &PostRequest<'_>) -> Result<(), Error> {
    check_login(login)?;
    let url = action_url(client, request.repo, request.endpoint, &[])?;
    send_with_retry(
        &client.credentials,
        |credentials| credentials.github(Some(login)),
        |credential| {
            let account = transport::account_label(credential, Some(login));
            transport::post_rest_json(&url, credential, &account, request.body, "2022-11-28")?;
            Ok(())
        },
    )
    .map_err(|error| scope_error(error, request.scope))
}

fn next_link(response: &ureq::http::Response<ureq::Body>) -> Option<String> {
    response
        .headers()
        .get("link")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| {
            value.split(',').find_map(|part| {
                let (target, rel) = part.trim().split_once(';')?;
                if !rel.contains("rel=\"next\"") {
                    return None;
                }
                target
                    .trim()
                    .strip_prefix('<')?
                    .strip_suffix('>')
                    .map(str::to_owned)
            })
        })
}

pub(super) fn scope_error(error: Error, scope: &str) -> Error {
    if matches!(&error, Error::Status { status: http_status::FORBIDDEN, message } if message.contains("HTTP 403"))
    {
        return transport::status(
            http_status::FORBIDDEN,
            format!("GitHub denied this operation. Required permission: {scope}."),
        );
    }
    error
}
