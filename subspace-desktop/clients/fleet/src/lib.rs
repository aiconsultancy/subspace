//! Stargate's Fleet API for the desktop: blocking reads of a fleet and its
//! ships ([`FleetClient`]), the fleet's event stream followed in the
//! background with reconnect, backoff and a polling fallback ([`Feed`]),
//! one saved token per fleet bound to the address it was saved for
//! ([`TokenStore`]), and the app-wide service that ties them to the fleets
//! the Subspace API assigns to the person's initiatives ([`FleetApi`]).
//!
//! Blocking calls never run in render: use `cx.background_spawn`, or the
//! service, which does. A token is only ever sent in the `Authorization`
//! header, to the address it was saved for; it never appears in a URL, a
//! log line or an error.

mod address;
mod client;
pub mod diff;
mod directory;
mod error;
mod events;
mod feed;
pub mod model;
pub mod navigation;
mod service;
pub mod settings;
mod stream;
mod tokens;

pub use address::FleetAddress;
pub use client::FleetClient;
pub use directory::{FleetRecord, FleetState, SharedFleet, fleets_of};
pub use error::Error;
pub use events::FleetEvent;
pub use feed::{Feed, FleetSnapshot, Link, MAX_ACTIVITY};
pub use service::{FleetAccess, FleetApi, FleetFollow, FleetInfo, TokenError};
pub use tokens::{SavedToken, StoreError, TOKEN_FILE, TokenMismatch, TokenStore};

#[cfg(test)]
mod tests;
