//! What the app shows for its API backend. Subspace API and NETT builds
//! show everything. A build with no API backend (`api_backend: "none"`)
//! leaves out each surface that needs the API, so it shows no trace of one:
//!
//! - the Issues and Decisions Spaces, from the registrations and the
//!   descriptor (Settings, **Add a Space**, search and the command palette
//!   list only those);
//! - Settings › Fleets, whose fleets are the API's initiatives' fleets;
//! - the built-in Fleet Console until its own Stargate API address and token
//!   are saved, since only those give it a fleet.
//!
//! Updates are the same in every build: the updater checks GitHub releases,
//! never the API.
//!
//! Sign-in needs a provider, which such a build refuses (`identity.rs`), and
//! the API client refuses every request once `launch::install` has recorded
//! the choice, which turns Channels' initiatives, fleet context, captain
//! notices, uploads and tracker commands off.

use subspace_framework::core::config::web_boards::WebBoard;
use subspace_framework::product::ProductDescriptor;
use subspace_framework::shell::{BoardRegistration, SettingsExtension};
use subspace_web_board::builtin::FLEET_CONSOLE_ID;

use crate::settings::ApiBackend;

/// The Spaces that read the tracker API.
const API_SPACES: [&str; 2] = ["issues", "decisions"];

/// The Settings extensions that list what the tracker API records.
const API_SETTINGS: [&str; 1] = ["fleets"];

/// Fleet Console's own settings for its first fleet, from its manifest.
const FLEET_ENDPOINT: &str = "endpoint";
const FLEET_TOKEN: &str = "token";

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

/// Whether `backend` shows the configured web Space `board`: always, except
/// Fleet Console in a build with no API backend, which has no fleets from
/// the API and shows once its Stargate API address and token are saved, as
/// the installer saves them.
pub(crate) fn shows_web_space(backend: ApiBackend, board: &WebBoard) -> bool {
    backend != ApiBackend::None || board.id != FLEET_CONSOLE_ID || has_own_fleet(board)
}

/// Whether Fleet Console's settings hold a fleet: an address and a token.
fn has_own_fleet(board: &WebBoard) -> bool {
    let endpoint = board
        .settings
        .get(FLEET_ENDPOINT)
        .and_then(|value| value.as_str())
        .is_some_and(|value| !value.trim().is_empty());
    let token = board
        .secrets
        .get(FLEET_TOKEN)
        .is_some_and(|token| !token.expose().trim().is_empty());
    endpoint && token
}

#[cfg(test)]
mod tests {
    use super::{ApiBackend, FLEET_CONSOLE_ID, shows_web_space};
    use std::path::PathBuf;
    use subspace_framework::core::Secret;
    use subspace_framework::core::config::web_boards::{WebBoard, WebBoardSource};

    fn board(id: &str, endpoint: Option<&str>, token: Option<&str>) -> WebBoard {
        WebBoard {
            id: id.to_owned(),
            title: "Fixture".to_owned(),
            source: WebBoardSource::Folder(PathBuf::from("/apps/fixture")),
            settings: endpoint
                .map(|endpoint| ("endpoint".to_owned(), endpoint.into()))
                .into_iter()
                .collect(),
            secrets: token
                .map(|token| ("token".to_owned(), Secret::new(token)))
                .into_iter()
                .collect(),
            permissions: Default::default(),
        }
    }

    #[test]
    fn without_a_backend_fleet_console_shows_once_its_fleet_is_saved() {
        let address = Some("https://fleet.example.com");
        let saved = board(FLEET_CONSOLE_ID, address, Some("fixture-token"));
        assert!(shows_web_space(ApiBackend::None, &saved));
        for unsaved in [
            board(FLEET_CONSOLE_ID, None, None),
            board(FLEET_CONSOLE_ID, address, None),
            board(FLEET_CONSOLE_ID, None, Some("fixture-token")),
            board(FLEET_CONSOLE_ID, Some("  "), Some("fixture-token")),
            board(FLEET_CONSOLE_ID, address, Some(" ")),
        ] {
            assert!(!shows_web_space(ApiBackend::None, &unsaved));
            // The API's fleets give the other builds a fleet without them.
            assert!(shows_web_space(ApiBackend::Subspace, &unsaved));
            assert!(shows_web_space(ApiBackend::Nett, &unsaved));
        }
        // Every other web Space shows, whatever its settings.
        assert!(shows_web_space(
            ApiBackend::None,
            &board("notes", None, None)
        ));
    }
}
