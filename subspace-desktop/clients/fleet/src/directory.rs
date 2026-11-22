//! What the service knows of each fleet, without GPUI: the fleets the
//! Subspace API assigns, the token saved for each and the state that
//! follows. Pure, so the address rule is tested on its own.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use subspace_api_client::model::MemberInitiative;
use subspace_framework::core::Secret;

use crate::feed::Link;
use crate::tokens::{SavedToken, TokenMismatch, TokenStore};
use crate::{FleetAddress, FleetClient};

/// A fleet as the Subspace API records it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct FleetRecord {
    pub id: String,
    pub name: String,
    /// The `fleetApiUrl` the API records, as it records it.
    pub address: Option<String>,
}

impl FleetRecord {
    pub fn new(id: impl Into<String>, name: impl Into<String>, address: Option<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            address,
        }
    }
}

/// The distinct fleets of `initiatives`, in their order.
pub fn fleets_of(initiatives: &[MemberInitiative]) -> Vec<FleetRecord> {
    let mut fleets: Vec<FleetRecord> = Vec::new();
    for fleet in initiatives.iter().filter_map(|item| item.fleet.as_ref()) {
        if fleets.iter().any(|known| known.id == fleet.id) {
            continue;
        }
        let name = [&fleet.name, &fleet.slug, &fleet.id]
            .into_iter()
            .find(|name| !name.trim().is_empty())
            .cloned()
            .unwrap_or_default();
        fleets.push(FleetRecord::new(
            fleet.id.clone(),
            name,
            fleet.fleet_api_url.clone(),
        ));
    }
    fleets
}

/// Where a fleet's saved token stands on this computer.
#[derive(Clone, Default, PartialEq)]
pub(crate) enum TokenLoad {
    /// Not read from the secret store yet.
    #[default]
    Loading,
    Missing,
    Saved(SavedToken),
    /// The secret store could not be read.
    Unavailable,
}

impl TokenLoad {
    /// Fleet `fleet`'s token as `store` has it. Blocks on the keychain.
    pub(crate) fn read(store: &TokenStore, fleet: &str) -> Self {
        store.load(fleet).map_or(Self::Unavailable, |saved| {
            saved.map_or(Self::Missing, Self::Saved)
        })
    }
}

/// A fleet's state, for the status a Space or Settings shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FleetState {
    /// The Subspace API records no Fleet API address for the fleet.
    NotConfigured,
    /// The recorded address is not one Subspace sends a token to: not
    /// HTTPS (or HTTP on this computer), or with credentials.
    InvalidAddress,
    /// Reading the saved token.
    Checking,
    /// No token saved on this computer.
    NoToken,
    /// The token was saved for another address, which it is never sent
    /// past: confirm the recorded address by entering the token again.
    /// `saved_for` is that address, when it can be read.
    AddressChanged { saved_for: Option<String> },
    /// The secret store could not be read.
    StoreUnavailable,
    /// A token is saved for the recorded address; nothing follows the
    /// fleet now.
    Ready,
    /// Something follows the fleet: how its stream is doing, from
    /// [`Link::Connecting`] and [`Link::Live`] to [`Link::TokenRefused`] and
    /// [`Link::Unreachable`].
    Followed(Link),
}

impl FleetState {
    /// A short status for a row.
    pub fn label(&self) -> &'static str {
        match self {
            Self::NotConfigured => "Not configured",
            Self::InvalidAddress => "Address not usable",
            Self::Checking => "Checking…",
            Self::NoToken => "No token",
            Self::AddressChanged { .. } => "Address changed",
            Self::StoreUnavailable => "Keychain locked",
            Self::Ready => "Token saved",
            Self::Followed(link) => link.label(),
        }
    }

    /// Whether the fleet's data arrives: live, or read on a timer.
    pub fn is_live(&self) -> bool {
        matches!(self, Self::Followed(Link::Live | Link::Polling))
    }

    /// What the person can do about it, when anything.
    pub fn help(&self) -> Option<&'static str> {
        match self {
            Self::NotConfigured => Some(
                "The Subspace API records no Fleet API address for this fleet. Ask its administrator to add one.",
            ),
            Self::InvalidAddress => Some(
                "The recorded address is not HTTPS, or HTTP on this computer, so no token is sent to it.",
            ),
            Self::NoToken => Some("Paste the fleet's Fleet API token and save it."),
            Self::AddressChanged { .. } => Some(
                "The fleet's address changed since its token was saved. A saved token is only sent to the address it was saved for: check the new address, then paste the token again.",
            ),
            Self::StoreUnavailable => {
                Some("Subspace could not read its secret store. Unlock your system keychain.")
            }
            Self::Followed(link) => link.help(),
            Self::Checking | Self::Ready => None,
        }
    }
}

/// The recorded address, checked, or the state that explains why not.
pub(crate) fn address_of(record: &FleetRecord) -> Result<FleetAddress, FleetState> {
    let raw = record.address.as_deref().map(str::trim).unwrap_or_default();
    if raw.is_empty() {
        return Err(FleetState::NotConfigured);
    }
    FleetAddress::parse(raw).map_err(|_| FleetState::InvalidAddress)
}

/// The client for `record` with its saved token, only when the token was
/// saved for the address the API records now.
pub(crate) fn ready_client(
    record: &FleetRecord,
    token: &TokenLoad,
) -> Result<FleetClient, FleetState> {
    let address = address_of(record)?;
    let saved = match token {
        TokenLoad::Loading => return Err(FleetState::Checking),
        TokenLoad::Missing => return Err(FleetState::NoToken),
        TokenLoad::Unavailable => return Err(FleetState::StoreUnavailable),
        TokenLoad::Saved(saved) => saved,
    };
    match saved.for_address(&address) {
        Ok(token) => Ok(FleetClient::new(address, token.clone())),
        Err(TokenMismatch::SavedFor(other)) => Err(FleetState::AddressChanged {
            saved_for: Some(other),
        }),
        Err(TokenMismatch::Unbound) => Err(FleetState::AddressChanged { saved_for: None }),
    }
}

/// The state of `record`, with `link` the state of its feed if one runs.
pub(crate) fn state_of(record: &FleetRecord, token: &TokenLoad, link: Option<Link>) -> FleetState {
    match ready_client(record, token) {
        Ok(_) => link.map_or(FleetState::Ready, FleetState::Followed),
        Err(state) => state,
    }
}

/// A fleet as the built-in Fleet Console receives it: its address and,
/// only for that address, its token.
#[derive(Clone)]
#[non_exhaustive]
pub struct SharedFleet {
    pub id: String,
    pub name: String,
    /// The normalised address, when the API records a usable one.
    pub endpoint: Option<String>,
    /// The token, only when saved for `endpoint`.
    pub token: Option<Secret>,
    /// Why there is no token or endpoint, in the Fleet Console's words:
    /// `no-endpoint`, `bad-endpoint`, `no-token`, `token-address`,
    /// `token-checking` (still being read) or `token-store` (the keychain
    /// could not be read).
    pub problem: Option<&'static str>,
}

pub(crate) fn shared_fleet(record: &FleetRecord, token: &TokenLoad) -> SharedFleet {
    let endpoint = address_of(record)
        .ok()
        .map(|address| address.as_str().to_owned());
    let (token, problem) = match ready_client(record, token) {
        Ok(client) => (Some(client.token().clone()), None),
        Err(state) => (None, Some(problem_code(&state))),
    };
    SharedFleet {
        id: record.id.clone(),
        name: record.name.clone(),
        endpoint,
        token,
        problem,
    }
}

fn problem_code(state: &FleetState) -> &'static str {
    match state {
        FleetState::NotConfigured => "no-endpoint",
        FleetState::InvalidAddress => "bad-endpoint",
        FleetState::AddressChanged { .. } => "token-address",
        FleetState::Checking => "token-checking",
        FleetState::StoreUnavailable => "token-store",
        _ => "no-token",
    }
}

/// The token loads to keep for `fleets`: known ones kept, new ones loading.
pub(crate) fn retain_tokens(
    tokens: &mut BTreeMap<String, TokenLoad>,
    fleets: &[FleetRecord],
) -> Vec<String> {
    tokens.retain(|id, _| fleets.iter().any(|fleet| &fleet.id == id));
    let new: Vec<String> = fleets
        .iter()
        .filter(|fleet| !tokens.contains_key(&fleet.id))
        .map(|fleet| fleet.id.clone())
        .collect();
    for id in &new {
        tokens.insert(id.clone(), TokenLoad::Loading);
    }
    new
}

/// The fleets in `old` that `new` no longer has.
pub(crate) fn removed(old: &[FleetRecord], new: &[FleetRecord]) -> Vec<String> {
    old.iter()
        .filter(|fleet| !new.iter().any(|kept| kept.id == fleet.id))
        .map(|fleet| fleet.id.clone())
        .collect()
}

/// When the soonest fleet whose token the store could not read, and that
/// was read again within `interval` of `now`, may be read again: a Retry
/// asked too soon waits for it. `None` when no such fleet waits.
pub(crate) fn next_reread(
    tokens: &BTreeMap<String, TokenLoad>,
    last: &BTreeMap<String, Instant>,
    now: Instant,
    interval: Duration,
) -> Option<Instant> {
    tokens
        .iter()
        .filter(|(_, token)| **token == TokenLoad::Unavailable)
        .filter_map(|(fleet, _)| last.get(fleet).map(|at| *at + interval))
        .filter(|due| *due > now)
        .min()
}

/// The fleets whose token the store could not read and that may be read
/// again at `now`: none read again within `interval`. Marks them read at
/// `now` and forgets fleets no longer known.
pub(crate) fn due_rereads(
    tokens: &BTreeMap<String, TokenLoad>,
    last: &mut BTreeMap<String, Instant>,
    now: Instant,
    interval: Duration,
) -> Vec<String> {
    last.retain(|fleet, _| tokens.contains_key(fleet));
    let due: Vec<String> = tokens
        .iter()
        .filter(|(fleet, token)| {
            **token == TokenLoad::Unavailable
                && last
                    .get(*fleet)
                    .is_none_or(|at| now.saturating_duration_since(*at) >= interval)
        })
        .map(|(fleet, _)| fleet.clone())
        .collect();
    for fleet in &due {
        last.insert(fleet.clone(), now);
    }
    due
}

#[cfg(test)]
mod tests;
