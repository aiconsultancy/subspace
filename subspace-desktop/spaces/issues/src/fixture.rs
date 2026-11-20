//! Debug fixtures use the client's validated numeric-loopback seam.
use subspace_api_client::{Client, fixture_from_env, hub::Hub};
#[cfg(test)]
use subspace_framework::core::Secret;
/// Client::new reads the same debug override, including the private
/// SUBSPACE_API_ACCESS_JWT_FILE assertion, and validates the loopback origin.
pub(crate) fn from_env() -> Option<(Hub, String)> {
    if subspace_api_client::backend() != subspace_api_client::Backend::Subspace {
        return None;
    }
    let api = std::env::var("SUBSPACE_API_URL").ok();
    let origin = fixture_from_env(true, api.as_deref())?;
    let client = Client::new().ok()?;
    Some((Hub::from_client(client, "Subspace API"), origin.to_string()))
}
#[cfg(test)]
pub(crate) fn hub_for(api: Option<&str>, jwt: Option<Secret>) -> Option<(Hub, String)> {
    let origin = fixture_from_env(true, api)?;
    // Board tests count requests; the updates hub has its own client tests.
    let client = Client::loopback(origin.as_str(), jwt).ok()?.without_live();
    Some((Hub::from_client(client, "Subspace API"), origin.to_string()))
}
