//! Tests of the features the mock advertises and the requests they send.

mod attachment_hub;
mod attachments;
mod chat;
mod comment_media;
mod flow;
mod moves;
mod todos;

use super::{organisation, server::NettMock};
use crate::Client;
use serde_json::Value;
use subspace_framework::core::Secret;

/// Requests to NETT's file host: uploads and grant reads.
fn file_host(mock: &NettMock) -> Vec<Value> {
    mock.requests()
        .into_iter()
        .filter(|request| {
            request["path"]
                .as_str()
                .is_some_and(|path| path.starts_with("/u/") || path.starts_with("/files/"))
        })
        .collect()
}

fn assert_credential_free(requests: &[Value]) {
    assert!(!requests.is_empty());
    for request in requests {
        assert_eq!(request["authorization"], false, "{request}");
        assert_eq!(request["cookie"], false, "{request}");
        assert!(request["orgHeader"].is_null(), "{request}");
    }
}

/// A client with its own bearer, so its flow control and caches are its
/// own.
fn own_client(mock: &NettMock, token: &str) -> Client {
    let org = organisation(mock, "Fixture Works");
    mock.token(token, "ada");
    Client::nett_loopback(&mock.api(), Some(&org), Some(Secret::new(token)))
        .unwrap()
        .without_live()
}

fn org_path(mock: &NettMock, rest: &str) -> String {
    format!(
        "/api/v1/orgs/{}/{rest}",
        organisation(mock, "Fixture Works")
    )
}
