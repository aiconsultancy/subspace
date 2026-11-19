//! Immutable workflow text shares the client's bounded file cache.
use sha2::{Digest, Sha256};
use subspace_core::http::{Error, send_with_retry, status as http_status};

use crate::{
    Client,
    transport::{self, RAW},
    validate::{check_login, check_repo, is_sha},
};

use super::{
    Definition, WorkflowDefinition, definition::MAX_WORKFLOW_BYTES, parse_workflow_definition,
    request::scope_error,
};

pub(super) fn definition(
    client: &Client,
    login: &str,
    repo: &str,
    sha: &str,
    path: &str,
) -> Result<Definition<WorkflowDefinition>, Error> {
    check_login(login)?;
    let (owner, name) = check_repo(repo)?;
    let url = client.contents_url(&owner, &name, sha, path)?;
    match text(client, login, &url, is_sha(&sha.to_ascii_lowercase())) {
        Ok(text) => Ok(parse_workflow_definition(&text)),
        Err(Error::Status {
            status: http_status::PAYLOAD_TOO_LARGE,
            ..
        }) => Ok(Definition::Unreadable(
            "workflow file exceeds the 1 MB limit".into(),
        )),
        Err(error) => Err(scope_error(error, "Contents: read")),
    }
}

fn text(client: &Client, login: &str, url: &str, immutable: bool) -> Result<String, Error> {
    send_with_retry(
        &client.credentials,
        |credentials| credentials.github(Some(login)),
        |credential| {
            // The URL contains the endpoint, repository, exact ref and path.
            // Resolve the token once per attempt for both cache and download.
            let key = immutable.then(|| {
                let fingerprint = Sha256::digest(credential.value.expose().as_bytes());
                format!("workflow\n{login}\n{url}\n{fingerprint:02x?}")
            });
            if let Some(hit) = key.as_ref().and_then(|key| client.files.get(key)) {
                return Ok(hit);
            }
            let account = transport::account_label(credential, Some(login));
            let response = transport::get_rest(url, credential, &account, RAW)?;
            let bytes = transport::read_limited(response, MAX_WORKFLOW_BYTES)?;
            let text = String::from_utf8_lossy(&bytes).into_owned();
            // Empty values have zero weight in the shared byte-bounded cache.
            if let Some(key) = key.filter(|_| !text.is_empty()) {
                client.files.set(key, text.clone());
            }
            Ok(text)
        },
    )
}
