//! Single-attempt REST writes. A successful mutation is separate from its refresh.

use serde::Deserialize;
use serde_json::json;
use subspace_core::credentials::Credential;
use subspace_core::http::{Error, status as http_status};

use crate::model::{GhItem, ItemKind, MergeMethod, MergeTarget, PrState};
use crate::raw::RawDetail;
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

/// Repository-wide write permission and the methods it permits.
#[derive(Deserialize)]
#[serde(default, rename_all = "camelCase")]
#[derive(Default)]
pub(super) struct MergeSettings {
    viewer_permission: Option<String>,
    merge_commit_allowed: bool,
    squash_merge_allowed: bool,
    rebase_merge_allowed: bool,
}

impl MergeSettings {
    pub(super) fn target(&self, node: &RawDetail, item: &GhItem) -> Option<MergeTarget> {
        if item.kind != ItemKind::Pr
            || item.state != PrState::Open
            || node.is_merge_queue_enabled
            || !matches!(
                self.viewer_permission.as_deref(),
                Some("ADMIN" | "MAINTAIN" | "WRITE")
            )
            || node.mergeable.as_deref() != Some("MERGEABLE")
            || !matches!(
                node.merge_state_status.as_deref(),
                Some("CLEAN" | "HAS_HOOKS" | "UNSTABLE")
            )
        {
            return None;
        }
        let method = [
            (self.squash_merge_allowed, MergeMethod::Squash),
            (self.rebase_merge_allowed, MergeMethod::Rebase),
            (self.merge_commit_allowed, MergeMethod::Merge),
        ]
        .into_iter()
        .find_map(|(allowed, method)| allowed.then_some(method))?;
        let head_sha = node.head_ref_oid.as_ref().filter(|sha| !sha.is_empty())?;
        Some(MergeTarget {
            head_sha: head_sha.clone(),
            base_ref: item.base_ref.clone()?,
            method,
        })
    }
}

pub(super) fn merge(
    client: &Client,
    login: &str,
    key: &str,
    target: &MergeTarget,
) -> Result<(), Error> {
    let item = validate::check_key(key)?;
    let bound = client.bind_read(login)?;
    let fresh = bound.item(key)?;
    if !fresh.item.key.eq_ignore_ascii_case(key) || fresh.merge_target.as_ref() != Some(target) {
        return Err(transport::status(
            http_status::CONFLICT,
            "The confirmed merge target changed.",
        ));
    }
    let number = item.number.to_string();
    let url = client.rest_url(
        ["repos", &item.owner, &item.name, "pulls", &number, "merge"],
        &[],
    )?;
    let method = match target.method {
        MergeMethod::Merge => "merge",
        MergeMethod::Squash => "squash",
        MergeMethod::Rebase => "rebase",
    };
    let body = json!({ "sha": target.head_sha, "merge_method": method });
    let response = send(
        &bound.credential,
        bound.account(),
        subspace_core::http::agent().put(&url),
        body,
    )?;
    let merged = serde_json::from_str::<serde_json::Value>(&response)
        .ok()
        .and_then(|value| value["merged"].as_bool());
    if merged == Some(true) {
        Ok(())
    } else {
        Err(Error::Status {
            status: http_status::BAD_GATEWAY,
            message: "GitHub did not confirm the merge.".into(),
        })
    }
}

fn write(
    client: &Client,
    login: &str,
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    body: serde_json::Value,
) -> Result<(), Error> {
    let credential = client.credential(Some(login))?;
    send(&credential, login, request, body).map(|_| ())
}

fn send(
    credential: &Credential,
    login: &str,
    request: ureq::RequestBuilder<ureq::typestate::WithBody>,
    body: serde_json::Value,
) -> Result<String, Error> {
    let account = transport::account_label(credential, Some(login));
    transport::post_rest_json_request(
        request,
        credential,
        &account,
        &body.to_string(),
        API_VERSION,
    )
    .map(|(_, response)| response)
}
#[cfg(test)]
mod tests;
