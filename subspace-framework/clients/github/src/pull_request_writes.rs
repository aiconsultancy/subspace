//! Single-attempt REST writes. A successful mutation is separate from its refresh.

use serde_json::json;
use subspace_core::http::Error;

use crate::{Client, transport, validate};

const API_VERSION: &str = "2022-11-28";

pub(super) fn issue_comment(
    client: &Client,
    login: &str,
    key: &str,
    body: &str,
) -> Result<(), Error> {
    validate::check_login(login)?;
    let item = validate::check_key(key)?;
    let body = validate::check_body(body.trim(), true)?;
    let number = item.number.to_string();
    let url = client.rest_url(
        [
            "repos",
            &item.owner,
            &item.name,
            "issues",
            &number,
            "comments",
        ],
        &[],
    )?;
    write(
        client,
        login,
        subspace_core::http::agent().post(&url),
        json!({ "body": body }),
    )
}

pub(super) fn close(client: &Client, login: &str, key: &str) -> Result<(), Error> {
    validate::check_login(login)?;
    let item = validate::check_key(key)?;
    let number = item.number.to_string();
    let url = client.rest_url(["repos", &item.owner, &item.name, "pulls", &number], &[])?;
    write(
        client,
        login,
        subspace_core::http::agent().patch(&url),
        json!({ "state": "closed" }),
    )
}

fn write(
    client: &Client,
    login: &str,
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    body: serde_json::Value,
) -> Result<(), Error> {
    let credential = client.credential(Some(login))?;
    let account = transport::account_label(&credential, Some(login));
    transport::post_rest_json_request(
        request,
        &credential,
        &account,
        &body.to_string(),
        API_VERSION,
    )?;
    Ok(())
}
#[cfg(test)]
mod tests;
