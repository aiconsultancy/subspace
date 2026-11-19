//! Single-attempt JSON writes. Callers retain their own error and response contracts.
use super::{USER_AGENT, prepare};
use crate::Client;
pub(crate) fn post_json(
    client: &Client,
    url: &url::Url,
    body: &serde_json::Value,
    timeout: std::time::Duration,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    post_json_bytes(client, url, body.to_string().as_bytes(), timeout)
}
pub(crate) fn post_json_bytes(
    client: &Client,
    url: &url::Url,
    body: &[u8],
    timeout: std::time::Duration,
) -> Result<ureq::http::Response<ureq::Body>, ureq::Error> {
    prepare(url);
    let mut request = client
        .agent
        .post(url.as_str())
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json");
    if let Some(jwt) = &client.access_jwt {
        request = request.header("Authorization", format!("Bearer {}", jwt.expose()));
    }
    request
        .config()
        .timeout_global(Some(timeout))
        .max_redirects(0)
        .max_redirects_will_error(false)
        .build()
        .send(body)
}
