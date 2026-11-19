//! A native Microsoft Entra public client: code + PKCE through the system
//! browser and a loopback callback, with no client secret, browser-cookie
//! copying, environment token lookup, plaintext vault or provider errors in
//! UI state. The tenant, client, authority and API scopes come from the app's
//! build or run-time configuration; nothing here names a real tenant.

mod browser;
mod config;
mod http;
mod oidc;
mod session;

pub use config::Config;
pub use session::SessionManager;
pub use subspace_account::{
    Account, Cancel, CredentialStore, Error, ServiceToken, Started, SystemStore,
};
