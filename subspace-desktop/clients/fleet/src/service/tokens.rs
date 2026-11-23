//! Saving, clearing and testing a fleet's token.

use gpui_kit::{AppContext as _, Context, Task};
use subspace_framework::core::Secret;

use super::FleetApi;
use super::queue::Op;
use crate::directory::{self, FleetState, TokenLoad};
use crate::model::FleetOverview;
use crate::tokens::{StoreError, clean_token};
use crate::{Error, FleetAddress, FleetClient};

/// Why a token could not be saved or tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TokenError {
    #[error("This fleet is not one of your initiatives' fleets.")]
    UnknownFleet,
    #[error("{0}")]
    Address(Error),
    #[error("{0}")]
    Store(StoreError),
    #[error("Paste the token to test the connection.")]
    NoToken,
    #[error(
        "The fleet's address changed since its token was saved. Paste the token for the new address."
    )]
    AddressChanged,
    #[error("{0}")]
    Connection(Error),
}

impl FleetApi {
    /// Saves `token` for `fleet`, bound to the address the API records now.
    /// A token or address that cannot be saved is refused before anything
    /// changes. Otherwise the save queues behind every store operation
    /// already asked for ([`super::queue`]), and a read queued before it is
    /// dropped when it lands: this one is newer.
    pub fn save_token(
        &mut self,
        fleet: &str,
        token: Secret,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), TokenError>> {
        let checked = self.address(fleet).and_then(|address| {
            let token = clean_token(&token).map_err(TokenError::Store)?;
            Ok((address, Secret::new(token)))
        });
        let (address, token) = match checked {
            Ok(checked) => checked,
            Err(error) => return Task::ready(Err(error)),
        };
        self.supersede(fleet);
        self.enqueue(fleet, Op::Save(address, token), cx)
    }

    /// Forgets `fleet`'s token on this computer. The token stops being
    /// used, followed and shared at once; the keychain delete queues behind
    /// every store operation already asked for, and nothing queued before
    /// it can bring the token back, in memory or in the keychain.
    pub fn clear_token(
        &mut self,
        fleet: &str,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), TokenError>> {
        match self.forget(fleet, cx) {
            Ok(()) => self.enqueue(fleet, Op::Clear, cx),
            Err(error) => Task::ready(Err(error)),
        }
    }

    /// Stops using `fleet`'s token at once, in a new generation.
    fn forget(&mut self, fleet: &str, cx: &mut Context<Self>) -> Result<(), TokenError> {
        self.record(fleet).ok_or(TokenError::UnknownFleet)?;
        let generation = self.supersede(fleet);
        self.reads.cleared.insert(fleet.to_owned(), generation);
        self.set_token(fleet, TokenLoad::Missing, cx);
        Ok(())
    }

    /// Reads the fleet once at its recorded address, with `token` when
    /// given (not saved) or else the saved one, only if it was saved for
    /// that address.
    pub fn test_connection(
        &self,
        fleet: &str,
        token: Option<Secret>,
        cx: &mut Context<Self>,
    ) -> Task<Result<FleetOverview, TokenError>> {
        let client = token.map_or_else(
            || self.saved_client(fleet),
            |token| self.typed_client(fleet, &token),
        );
        cx.background_spawn(async move { client?.fleet().map_err(TokenError::Connection) })
    }

    fn typed_client(&self, fleet: &str, token: &Secret) -> Result<FleetClient, TokenError> {
        let address = self.address(fleet)?;
        let token = clean_token(token).map_err(|_| TokenError::NoToken)?;
        Ok(FleetClient::new(address, Secret::new(token)))
    }

    fn saved_client(&self, fleet: &str) -> Result<FleetClient, TokenError> {
        self.client(fleet).map_err(|state| match state {
            FleetState::NotConfigured | FleetState::InvalidAddress => {
                TokenError::Address(Error::InvalidAddress)
            }
            FleetState::AddressChanged { .. } => TokenError::AddressChanged,
            FleetState::StoreUnavailable => TokenError::Store(StoreError::Unavailable),
            _ => TokenError::NoToken,
        })
    }

    fn address(&self, fleet: &str) -> Result<FleetAddress, TokenError> {
        let record = self.record(fleet).ok_or(TokenError::UnknownFleet)?;
        directory::address_of(record).map_err(|_| TokenError::Address(Error::InvalidAddress))
    }
}
