//! One GraphQL exchange, with stricter transport rules for bound reads.

use super::{Api, GraphQlResponse, USER_AGENT, check, network, status, unexpected};
use subspace_core::{
    credentials::Credential,
    http::{Error, agent},
};

/// POSTs a GraphQL document and parses the answer.
pub fn post_graphql(
    url: &str,
    credential: &Credential,
    account: &str,
    body: &str,
) -> Result<GraphQlResponse, Error> {
    post_graphql_request(url, credential, account, body, false)
}

pub(crate) fn post_bound_graphql(
    url: &str,
    credential: &Credential,
    account: &str,
    body: &str,
) -> Result<GraphQlResponse, Error> {
    post_graphql_request(url, credential, account, body, true)
}

fn post_graphql_request(
    url: &str,
    credential: &Credential,
    account: &str,
    body: &str,
    bound: bool,
) -> Result<GraphQlResponse, Error> {
    let authorization = credential.authorization();
    let builder = agent().post(url);
    let builder = if bound {
        builder.config().max_redirects(0).build()
    } else {
        builder
    };
    let response = builder
        .header("Authorization", authorization.expose())
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT)
        .send(body)
        .map_err(network)?;
    if bound && !response.status().is_success() {
        return Err(status(
            response.status().as_u16(),
            "GitHub rejected the bound read.",
        ));
    }
    let mut response = check(credential, account, Api::GraphQl, response)?;
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|_| unexpected("response"))?;
    serde_json::from_str(&text).map_err(|_| unexpected("response"))
}
