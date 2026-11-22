//! Saving, clearing and testing a fleet's token.

use std::sync::Arc;

use gpui_kit::{AppContext as _, Context, Task};
use subspace_framework::core::Secret;

use super::FleetApi;
use crate::directory::{self, FleetState, TokenLoad};
use crate::model::FleetOverview;
use crate::tokens::{SavedToken, StoreError, clean_token};
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
    pub fn save_token(
        &mut self,
        fleet: &str,
        token: Secret,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), TokenError>> {
        let address = self.address(fleet);
        let (store, id) = (Arc::clone(&self.store), fleet.to_owned());
        let save = cx.background_spawn(async move {
            let address = address?;
            let token = Secret::new(clean_token(&token).map_err(TokenError::Store)?);
            store
                .save(&id, &address, &token)
                .map_err(TokenError::Store)?;
            Ok(SavedToken::new(&address, token))
        });
        let id = fleet.to_owned();
        cx.spawn(async move |this, cx| {
            let saved = save.await?;
            _ = this.update(cx, |this, cx| {
                this.set_token(&id, TokenLoad::Saved(saved), cx)
            });
            Ok(())
        })
    }

    /// Forgets `fleet`'s token on this computer.
    pub fn clear_token(
        &mut self,
        fleet: &str,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), TokenError>> {
        let known = self.record(fleet).map(|record| record.id.clone());
        let store = Arc::clone(&self.store);
        let clear = cx.background_spawn(async move {
            let id = known.ok_or(TokenError::UnknownFleet)?;
            store.clear(&id).map_err(TokenError::Store)?;
            Ok(id)
        });
        cx.spawn(async move |this, cx| {
            let id = clear.await?;
            _ = this.update(cx, |this, cx| this.set_token(&id, TokenLoad::Missing, cx));
            Ok(())
        })
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
