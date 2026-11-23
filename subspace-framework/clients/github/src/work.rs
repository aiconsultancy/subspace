//! The work credential pinned for one operation.
//!
//! [`Client::pin_work`] resolves the work account's credential once and
//! verifies, with that very credential, whose it is. Every request the
//! operation then makes goes out with that credential and nothing else: no
//! request looks the credential up again, and a rejected one is not retried
//! with whichever credential the account holds by then. So an answer is
//! always the answer for the account the operation names, even when the
//! credential changes while the operation runs.

use std::sync::PoisonError;

use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use subspace_core::credentials::Credential;
use subspace_core::http::Error;
use subspace_core::http::status as http_status;
use ureq::Body;
use ureq::http::Response;

use crate::transport::{self, JSON, unexpected};
use crate::{Client, MAX_BODY_BYTES, pick_work_login, validate, verified_work_login};

/// The viewer read that verifies a work credential: its login and its
/// immutable node ID.
const WORK_VIEWER_QUERY: &str = "query WorkViewer { viewer { id login } }";

/// The account GitHub verified a credential belongs to.
#[derive(Clone)]
pub(crate) struct Owner {
    login: String,
    account_id: String,
}

/// The work account's credential and the account GitHub verified it belongs
/// to, for one operation. It borrows the client and is never stored,
/// serialized or printed.
pub struct PinnedWork<'a> {
    pub(crate) client: &'a Client,
    pub(crate) credential: Credential,
    owner: Owner,
}

/// Whose a credential is, remembered by its fingerprint. A token's owner
/// never changes, so a credential verified once this run need not be
/// verified again; a new token, for another account or the same one, is a
/// new fingerprint and is verified afresh.
pub(crate) struct Verified {
    fingerprint: Vec<u8>,
    owner: Owner,
}

fn fingerprint(credential: &Credential) -> Vec<u8> {
    Sha256::digest(credential.value.expose().as_bytes()).to_vec()
}

fn no_work_account(message: &str) -> Error {
    Error::Status {
        status: http_status::BAD_GATEWAY,
        message: message.into(),
    }
}

fn is_rejected<T>(result: &Result<T, Error>) -> bool {
    matches!(
        result,
        Err(Error::Status {
            status: http_status::UNAUTHORIZED,
            ..
        })
    )
}

/// The account `credential` belongs to, from the viewer read made with
/// exactly that credential, or from an earlier such read of the same
/// credential this run.
fn credential_owner(client: &Client, credential: &Credential, login: &str) -> Result<Owner, Error> {
    let fingerprint = fingerprint(credential);
    if let Some(known) = client
        .verified
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .filter(|known| known.fingerprint == fingerprint)
    {
        return Ok(known.owner.clone());
    }
    let account = transport::account_label(credential, Some(login));
    let body = json!({ "query": WORK_VIEWER_QUERY, "variables": {} }).to_string();
    let response = transport::post_graphql(
        &format!("{}/graphql", client.api_url),
        credential,
        &account,
        &body,
    )?;
    let data = transport::graphql_data(&account, response, false)?;
    let actual = data["viewer"]["login"]
        .as_str()
        .ok_or_else(|| unexpected("viewer"))?;
    validate::check_login(actual)?;
    let owner = Owner {
        login: actual.to_owned(),
        account_id: crate::bound::verified_account_id(&data)?.to_owned(),
    };
    *client
        .verified
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(Verified {
        fingerprint,
        owner: owner.clone(),
    });
    Ok(owner)
}

/// Forgets whose `credential` is, once GitHub rejected it.
fn forget_verified(client: &Client, credential: &Credential) {
    let fingerprint = fingerprint(credential);
    let mut verified = client
        .verified
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if verified
        .as_ref()
        .is_some_and(|known| known.fingerprint == fingerprint)
    {
        *verified = None;
    }
}

impl<'a> PinnedWork<'a> {
    /// Resolves the work account's credential once and verifies whose it is
    /// with that same credential ([`Client::pin_work`]).
    pub(crate) fn pin(client: &'a Client) -> Result<Self, Error> {
        let accounts = client.credentials.github_accounts().map_err(|_| {
            no_work_account("No GitHub accounts are configured for the work account.")
        })?;
        let login = pick_work_login(&accounts.logins, client.work_account.as_ref())
            .ok_or_else(|| {
                no_work_account("No work (EMU) or chosen GitHub account is configured.")
            })?
            .clone();
        let resolve = || {
            client
                .credentials
                .github(Some(&login))
                .map_err(|error| no_work_account(&error.to_string()))
        };
        // A credential GitHub rejects is dropped and resolved once more
        // before anything is pinned; once pinned, nothing is resolved again.
        let mut credential = resolve()?;
        let mut owner = credential_owner(client, &credential, &login);
        if is_rejected(&owner) {
            client.credentials.invalidate(&credential);
            credential = resolve()?;
            owner = credential_owner(client, &credential, &login);
            if is_rejected(&owner) {
                client.credentials.invalidate(&credential);
            }
        }
        let mut owner = owner?;
        owner.login = verified_work_login(&login, &owner.login)?;
        Ok(Self {
            client,
            credential,
            owner,
        })
    }

    /// The credential of `login`, pinned without a viewer read, for tests
    /// of what a pinned operation sends.
    #[cfg(test)]
    pub(crate) fn for_tests(
        client: &'a Client,
        login: &str,
        account_id: &str,
    ) -> Result<Self, Error> {
        Ok(Self {
            client,
            credential: client.credentials.github(Some(login))?,
            owner: Owner {
                login: login.to_owned(),
                account_id: account_id.to_owned(),
            },
        })
    }
}

impl PinnedWork<'_> {
    /// The work login, as gh spells it, that GitHub verified the credential
    /// belongs to.
    pub fn login(&self) -> &str {
        &self.owner.login
    }

    /// The immutable node ID of the account the credential belongs to. Key
    /// anything kept per account by it: a login can be renamed and reused.
    // qual:api - Consumed by app crates through the Framework facade.
    pub fn account_id(&self) -> &str {
        &self.owner.account_id
    }

    /// The account messages name.
    pub(crate) fn account(&self) -> String {
        transport::account_label(&self.credential, Some(&self.owner.login))
    }

    /// Passes `result` through. A 401 drops the pinned credential from the
    /// cache, so the next operation resolves and verifies one afresh; this
    /// one is not retried with another.
    pub(crate) fn checked<T>(&self, result: Result<T, Error>) -> Result<T, Error> {
        if is_rejected(&result) {
            self.client.credentials.invalidate(&self.credential);
            forget_verified(self.client, &self.credential);
        }
        result
    }

    /// One GraphQL read with the pinned credential.
    pub(crate) fn graphql<T: DeserializeOwned>(
        &self,
        query: &str,
        variables: Value,
    ) -> Result<T, Error> {
        let account = self.account();
        let body = json!({ "query": query, "variables": variables }).to_string();
        let response = self.checked(transport::post_graphql(
            &format!("{}/graphql", self.client.api_url),
            &self.credential,
            &account,
            &body,
        ))?;
        let data = transport::graphql_data(&account, response, false)?;
        serde_json::from_value(data).map_err(|_| unexpected("response"))
    }

    /// One REST GET with the pinned credential, at `version` of the API.
    pub(crate) fn get(&self, url: &str, version: Option<&str>) -> Result<Response<Body>, Error> {
        let account = self.account();
        self.checked(match version {
            Some(version) => {
                transport::get_rest_version(url, &self.credential, &account, JSON, version)
            }
            None => transport::get_rest(url, &self.credential, &account, JSON),
        })
    }

    /// One REST JSON read with the pinned credential.
    pub(crate) fn rest_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, Error> {
        let bytes = transport::read_limited(self.get(url, None)?, MAX_BODY_BYTES)?;
        serde_json::from_slice(&bytes).map_err(|_| unexpected("response"))
    }
}
