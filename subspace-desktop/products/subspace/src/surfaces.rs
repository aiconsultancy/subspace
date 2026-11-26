//! What the app shows for its API backend. Subspace API and NETT builds
//! show everything. A build with no API backend (`api_backend: "none"`)
//! leaves out each surface that needs the API, so it shows no trace of one:
//!
//! - the Issues, Projects and Decisions Spaces, from the registrations and the
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
const API_SPACES: [&str; 3] = ["issues", "projects", "decisions"];

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

/// What a build with no API backend adds to its release asset names: the
/// plain archives' prefix and the macOS bundle zip's stem. The packaging
/// scripts add the same (`scripts/product.sh`).
pub(crate) const NO_API_ARCHIVE_SUFFIX: &str = "-no-api";
pub(crate) const NO_API_BUNDLE_SUFFIX: &str = "-NoAPI";

/// A build with no API backend lists no Space that needs the API, and
/// updates only from its own archives, so an update never installs the
/// standard build with its account and API in its place.
pub(crate) fn adjust_descriptor(descriptor: &mut ProductDescriptor) {
    descriptor
        .boards
        .retain(|board| keeps(ApiBackend::None, board.id.as_str(), &API_SPACES));
    if let Some(update) = &mut descriptor.update {
        update.archive_prefix.push_str(NO_API_ARCHIVE_SUFFIX);
        update.app_bundle.push_str(NO_API_BUNDLE_SUFFIX);
        update.exclusive = true;
    }
}

/// The built-in switcher order. Saved user order takes precedence.
pub(crate) fn order(board: BoardRegistration) -> BoardRegistration {
    const DEFAULT: [&str; 8] = [
        "irc",
        "issues",
        "github",
        "fleet-console",
        "projects",
        "decisions",
        "agentsview",
        "actions",
    ];
    let rank = DEFAULT.iter().position(|id| *id == board.id);
    board.with_default_order(rank.unwrap_or(usize::MAX))
}
