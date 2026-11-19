//! Credential-bound reads, independent of subsequent account/environment changes.

use crate::{Client, detail_from_data, model::GhItemDetail, queries, transport, validate};
use serde_json::{Value, json};
use subspace_core::{
    credentials::Credential,
    http::{Error, status},
};

const BOUND_VIEWER_QUERY: &str = "query ViewerBound { viewer { id login } }";
const MAX_ACCOUNT_ID_BYTES: usize = 256;

mod packages;
pub use packages::{
    Organization, PackageRelease, PackageRepository, ReleaseAsset, RepositoryRef,
    ResolvedRepository, newest_package_release,
};

/// A verified principal and one in-memory credential. Never serialized or Debug.
pub struct BoundRead {
    credential: Credential,
    endpoint: String,
    account: String,
    account_id: String,
}

impl BoundRead {
    /// Verify the requested login using exactly the credential retained for reads.
    pub(crate) fn bind(client: &Client, login: &str) -> Result<Self, Error> {
        validate::check_login(login)?;
        let mut bound = BoundRead {
            credential: client.credential(Some(login))?,
            endpoint: client.api_url.clone(),
            account: login.to_owned(),
            account_id: String::new(),
        };
        let data = bound.graphql(BOUND_VIEWER_QUERY, json!({}))?;
        let actual = data["viewer"]["login"]
            .as_str()
            .ok_or_else(|| transport::unexpected("viewer"))?;
        validate::check_login(actual)?;
        if !actual.eq_ignore_ascii_case(login) {
            return Err(transport::status(
                status::UNAUTHORIZED,
                "The GitHub credential belongs to another account.",
            ));
        }
        bound.account = actual.to_ascii_lowercase();
        bound.account_id = verified_account_id(&data)?.to_owned();
        Ok(bound)
    }
}

impl BoundRead {
    // qual:allow(dry, boilerplate) reason: "Verified binding metadata must not be mutable independently of its credential."
    pub fn account(&self) -> &str {
        &self.account
    }

    /// The immutable viewer node ID verified with this binding's credential.
    // qual:api
    // qual:allow(dry, boilerplate) reason: "Verified binding metadata must not be mutable independently of its credential."
    pub fn account_id(&self) -> &str {
        &self.account_id
    }

    // qual:allow(dry, boilerplate) reason: "Verified binding metadata must not be mutable independently of its credential."
    pub fn endpoint(&self) -> &str {
        &self.endpoint
    }

    /// Reuses the existing bounded detail query, including comments and reviews.
    pub fn item(&self, key: &str) -> Result<GhItemDetail, Error> {
        let item = validate::check_key(key)?;
        let variables = json!({"owner":item.owner,"name":item.name,"number":item.number});
        let data = self.graphql(&queries::detail_query(), variables)?;
        let data = serde_json::from_value(data).map_err(|_| transport::unexpected("response"))?;
        detail_from_data(data)
    }

    fn graphql(&self, query: &str, variables: Value) -> Result<Value, Error> {
        let body = json!({"query":query,"variables":variables}).to_string();
        let response = transport::post_bound_graphql(
            &format!("{}/graphql", self.endpoint),
            &self.credential,
            &self.account,
            &body,
        )?;
        transport::graphql_data(&self.account, response, false).map_err(safe_error)
    }
}

fn verified_account_id(data: &Value) -> Result<&str, Error> {
    data["viewer"]["id"]
        .as_str()
        .filter(|id| {
            !id.trim().is_empty()
                && id.len() <= MAX_ACCOUNT_ID_BYTES
                && !id.chars().any(char::is_control)
        })
        .ok_or_else(|| transport::unexpected("viewer identity"))
}

fn safe_error(error: Error) -> Error {
    match error {
        Error::Status { status, .. } => {
            transport::status(status, "GitHub rejected the bound read.")
        }
        _ => Error::Network("GitHub bound read failed.".into()),
    }
}
