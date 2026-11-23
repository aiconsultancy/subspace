//! What the app shows for its API backend. Subspace API and NETT builds
//! show everything. A build with no API backend (`api_backend: "none"`)
//! leaves out each surface that needs the API, so it shows no trace of one:
//!
//! - the Issues and Decisions Spaces, from the registrations and the
//!   descriptor (Settings, **Add a Space**, search and the command palette
//!   list only those);
//! - Settings › Fleets, whose fleets are the API's initiatives' fleets.
//!
//! Everything else is the same in every build: the updater checks GitHub
//! releases, never the API, and Fleet Console, which then has only the
//! fleets a person enters in its own settings or adds inside it.
//!
//! Sign-in needs a provider, which such a build refuses (`identity.rs`), and
//! the API client refuses every request once `launch::install` has recorded
//! the choice, which turns Channels' initiatives, fleet context, captain
//! notices, uploads and tracker commands off.

use subspace_framework::product::ProductDescriptor;
use subspace_framework::shell::{BoardRegistration, SettingsExtension};

use crate::settings::ApiBackend;

/// The Spaces that read the tracker API.
const API_SPACES: [&str; 2] = ["issues", "decisions"];

/// The Settings extensions that list what the tracker API records.
const API_SETTINGS: [&str; 1] = ["fleets"];

/// Whether `backend` keeps the Space or Settings page `id`, of which those
/// in `needs_api` need an API.
fn keeps(backend: ApiBackend, id: &str, needs_api: &[&str]) -> bool {
    backend != ApiBackend::None || !needs_api.contains(&id)
}

/// The Spaces of `boards` that `backend` registers, in their order.
pub(crate) fn spaces(
    backend: ApiBackend,
    boards: Vec<BoardRegistration>,
) -> Vec<BoardRegistration> {
    boards
        .into_iter()
        .filter(|board| keeps(backend, board.id, &API_SPACES))
        .collect()
}

/// The Settings extensions of `all` that `backend` offers, in their order.
pub(crate) fn settings_extensions(
    backend: ApiBackend,
    all: &[SettingsExtension],
) -> Vec<SettingsExtension> {
    all.iter()
        .filter(|extension| keeps(backend, extension.id, &API_SETTINGS))
        .copied()
        .collect()
}

/// A build with no API backend lists no Space that needs the API.
pub(crate) fn adjust_descriptor(descriptor: &mut ProductDescriptor) {
    descriptor
        .boards
        .retain(|board| keeps(ApiBackend::None, board.id.as_str(), &API_SPACES));
}
