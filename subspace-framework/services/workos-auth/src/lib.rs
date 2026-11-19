//! A native WorkOS AuthKit public client. The system browser completes the
//! User Management authorisation-code flow with PKCE and a loopback redirect;
//! the app holds no client secret. The rotating refresh token is kept in the
//! OS keychain and the access token, a JWT the app's API verifies, only in
//! memory. The client ID and API origin come from the app's configuration.

mod config;
mod http;
mod session;

pub use config::{Config, DEFAULT_API_URL};
pub use session::SessionManager;
pub use subspace_account::{Account, Cancel, CredentialStore, Error, ServiceToken, Started};
